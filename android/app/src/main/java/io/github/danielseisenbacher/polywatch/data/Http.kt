package io.github.danielseisenbacher.polywatch.data

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL

val json = Json {
    ignoreUnknownKeys = true
    coerceInputValues = true
    explicitNulls = false
    isLenient = true
}

class FetchError(message: String, val status: Int) : Exception(message)

class HttpResponse(val status: Int, val body: String, val headers: Map<String, String>)

/** Tiny blocking-IO HTTP client; plenty for a handful of JSON files. */
object Http {
    private const val UA = "polywatch-android"

    suspend fun request(
        url: String,
        method: String = "GET",
        headers: Map<String, String> = emptyMap(),
        body: String? = null,
    ): HttpResponse = withContext(Dispatchers.IO) {
        val conn = try {
            (URL(url).openConnection() as HttpURLConnection)
        } catch (e: IOException) {
            throw FetchError("network error: ${e.message ?: e}", 0)
        }
        try {
            conn.requestMethod = method
            conn.connectTimeout = 15_000
            conn.readTimeout = 30_000
            conn.setRequestProperty("User-Agent", UA)
            headers.forEach { (k, v) -> conn.setRequestProperty(k, v) }
            if (body != null) {
                conn.doOutput = true
                conn.setRequestProperty("Content-Type", "application/json")
                conn.outputStream.use { it.write(body.toByteArray()) }
            }
            val status = conn.responseCode
            val stream = if (status in 200..299) conn.inputStream else conn.errorStream
            val text = stream?.bufferedReader()?.use { it.readText() } ?: ""
            val hs = conn.headerFields.filterKeys { it != null }.mapKeys { it.key.lowercase() }
                .mapValues { it.value.firstOrNull() ?: "" }
            HttpResponse(status, text, hs)
        } catch (e: IOException) {
            throw FetchError("network error: ${e.message ?: e}", 0)
        } finally {
            conn.disconnect()
        }
    }

    suspend fun getText(url: String, headers: Map<String, String> = emptyMap()): String {
        val r = request(url, headers = headers)
        if (r.status !in 200..299) {
            val name = url.substringAfterLast('/').substringBefore('?')
            throw FetchError("$name: HTTP ${r.status}${if (r.status == 404) " (not published yet?)" else ""}", r.status)
        }
        return r.body
    }
}
