// GitHub Actions REST API: workflow runs, jobs, and (with a token) dispatch/cancel/re-run.
package io.github.danielseisenbacher.polywatch.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import java.net.URLEncoder
import java.time.Instant

@Serializable
data class Actor(val login: String = "", @SerialName("avatar_url") val avatarUrl: String = "")

@Serializable
data class WorkflowRun(
    val id: Long = 0,
    val name: String = "",
    @SerialName("display_title") val displayTitle: String = "",
    val status: String = "",
    val conclusion: String? = null,
    val event: String = "",
    @SerialName("head_branch") val headBranch: String? = null,
    @SerialName("head_sha") val headSha: String = "",
    @SerialName("run_number") val runNumber: Long = 0,
    @SerialName("run_attempt") val runAttempt: Long = 1,
    @SerialName("created_at") val createdAt: String = "",
    @SerialName("updated_at") val updatedAt: String = "",
    @SerialName("run_started_at") val runStartedAt: String? = null,
    @SerialName("html_url") val htmlUrl: String = "",
    @SerialName("workflow_id") val workflowId: Long = 0,
    val path: String = "",
    val actor: Actor? = null,
    @SerialName("triggering_actor") val triggeringActor: Actor? = null,
) {
    val createdTs: Long get() = isoToEpoch(createdAt)
    val startedTs: Long get() = isoToEpoch(runStartedAt ?: createdAt)
    val updatedTs: Long get() = isoToEpoch(updatedAt)
    val active: Boolean get() = status != "completed"
    /** Wall-clock duration so far (running) or in total (completed). */
    fun durationS(now: Long): Long = ((if (active) now else updatedTs) - startedTs).coerceAtLeast(0)
}

@Serializable
data class RunsPage(
    @SerialName("total_count") val totalCount: Long = 0,
    @SerialName("workflow_runs") val runs: List<WorkflowRun> = emptyList(),
)

@Serializable
data class Workflow(
    val id: Long = 0,
    val name: String = "",
    val path: String = "",
    val state: String = "",
    @SerialName("html_url") val htmlUrl: String = "",
) {
    val file: String get() = path.substringAfterLast('/')
}

@Serializable
data class WorkflowsPage(val workflows: List<Workflow> = emptyList())

@Serializable
data class Step(
    val name: String = "",
    val status: String = "",
    val conclusion: String? = null,
    val number: Int = 0,
    @SerialName("started_at") val startedAt: String? = null,
    @SerialName("completed_at") val completedAt: String? = null,
)

@Serializable
data class Job(
    val id: Long = 0,
    val name: String = "",
    val status: String = "",
    val conclusion: String? = null,
    @SerialName("started_at") val startedAt: String? = null,
    @SerialName("completed_at") val completedAt: String? = null,
    @SerialName("html_url") val htmlUrl: String = "",
    @SerialName("runner_name") val runnerName: String? = null,
    val steps: List<Step> = emptyList(),
)

@Serializable
data class JobsPage(val jobs: List<Job> = emptyList())

/** One `workflow_dispatch` input, parsed from the workflow YAML. */
data class DispatchInput(
    val name: String,
    val description: String = "",
    val type: String = "string",
    val default: String = "",
    val required: Boolean = false,
    val options: List<String> = emptyList(),
)

fun isoToEpoch(s: String?): Long = try {
    if (s.isNullOrBlank()) 0 else Instant.parse(s).epochSecond
} catch (_: Exception) {
    0
}

class GitHub(private val settings: Settings) {
    private val api get() = "https://api.github.com/repos/${settings.owner}/${settings.repo}"

    /** Last seen `x-ratelimit-remaining` / `-limit`, for the UI. */
    var rateRemaining: Int? = null
        private set
    var rateLimit: Int? = null
        private set

    private fun headers(): Map<String, String> = buildMap {
        put("Accept", "application/vnd.github+json")
        put("X-GitHub-Api-Version", "2022-11-28")
        if (settings.token.isNotBlank()) put("Authorization", "Bearer ${settings.token}")
    }

    private suspend fun call(path: String, method: String = "GET", body: String? = null): String {
        val r = Http.request("$api$path", method, headers(), body)
        r.headers["x-ratelimit-remaining"]?.toIntOrNull()?.let { rateRemaining = it }
        r.headers["x-ratelimit-limit"]?.toIntOrNull()?.let { rateLimit = it }
        if (r.status !in 200..299) {
            val msg = runCatching { json.decodeFromString<JsonObject>(r.body)["message"] }
                .getOrNull()?.let { (it as? JsonPrimitive)?.content }
            val hint = when {
                r.status == 403 && rateRemaining == 0 -> " — API rate limit reached; add a token in Settings"
                r.status == 401 -> " — check the token in Settings"
                method != "GET" && (r.status == 403 || r.status == 404) ->" — the token needs Actions: read & write on this repo"
                else -> ""
            }
            throw FetchError("GitHub API ${r.status}: ${msg ?: "request failed"}$hint", r.status)
        }
        return r.body
    }

    suspend fun runs(workflowId: Long? = null, perPage: Int = 40): RunsPage {
        val base = if (workflowId != null) "/actions/workflows/$workflowId/runs" else "/actions/runs"
        return json.decodeFromString(call("$base?per_page=$perPage"))
    }

    suspend fun run(id: Long): WorkflowRun = json.decodeFromString(call("/actions/runs/$id"))

    suspend fun jobs(runId: Long): List<Job> =
        json.decodeFromString<JobsPage>(call("/actions/runs/$runId/jobs?per_page=50")).jobs

    suspend fun workflows(): List<Workflow> =
        json.decodeFromString<WorkflowsPage>(call("/actions/workflows?per_page=100")).workflows

    suspend fun dispatch(workflow: Workflow, ref: String, inputs: Map<String, String>) {
        val body = buildJsonObject {
            put("ref", ref)
            put("inputs", buildJsonObject { inputs.forEach { (k, v) -> put(k, v) } })
        }
        call("/actions/workflows/${workflow.id}/dispatches", "POST", body.toString())
    }

    suspend fun cancel(runId: Long) {
        call("/actions/runs/$runId/cancel", "POST")
    }

    suspend fun rerun(runId: Long, failedOnly: Boolean) {
        call("/actions/runs/$runId/${if (failedOnly) "rerun-failed-jobs" else "rerun"}", "POST")
    }

    /**
     * Fetch the workflow file and return its `workflow_dispatch` inputs, or null when the
     * workflow cannot be dispatched manually.
     */
    suspend fun dispatchInputs(workflow: Workflow, ref: String): List<DispatchInput>? {
        val path = workflow.path.split('/').joinToString("/") { URLEncoder.encode(it, "UTF-8") }
        val yaml = Http.getText("https://raw.githubusercontent.com/${settings.owner}/${settings.repo}/$ref/$path")
        return parseDispatchInputs(yaml)
    }
}

/**
 * Minimal indentation-based reader for `on.workflow_dispatch.inputs` — enough for the
 * plain block style GitHub workflows use. Returns null if there is no workflow_dispatch.
 */
fun parseDispatchInputs(yaml: String): List<DispatchInput>? {
    val lines = yaml.lines()
        .map { it.replace("\t", "  ") }
        .filter { it.isNotBlank() && !it.trimStart().startsWith("#") }
    fun indent(s: String) = s.length - s.trimStart().length
    fun keyOf(s: String) = s.trim().substringBefore(':').trim().trim('"', '\'')
    fun valueOf(s: String) = s.trim().substringAfter(':', "").trim().let { v ->
        val unquoted = if (v.length >= 2 && (v.first() == '"' || v.first() == '\'') && v.last() == v.first()) v.substring(1, v.length - 1) else v.substringBefore(" #")
        unquoted.trim()
    }

    val onIdx = lines.indexOfFirst { indent(it) == 0 && (keyOf(it) == "on" || keyOf(it) == "true") }
    if (onIdx < 0) return null
    // `on: workflow_dispatch` or `on: [push, workflow_dispatch]`
    val inline = valueOf(lines[onIdx])
    if (inline.isNotEmpty()) return if ("workflow_dispatch" in inline) emptyList() else null

    var i = onIdx + 1
    var wdIdx = -1
    while (i < lines.size && indent(lines[i]) > 0) {
        if (keyOf(lines[i]) == "workflow_dispatch" && !lines[i].trimStart().startsWith("-")) { wdIdx = i; break }
        if (lines[i].trim() == "- workflow_dispatch") return emptyList()
        i++
    }
    if (wdIdx < 0) return null
    val wdIndent = indent(lines[wdIdx])
    i = wdIdx + 1
    var inputsIndent = -1
    while (i < lines.size && indent(lines[i]) > wdIndent) {
        if (keyOf(lines[i]) == "inputs") { inputsIndent = indent(lines[i]); i++; break }
        i++
    }
    if (inputsIndent < 0) return emptyList()

    val out = mutableListOf<DispatchInput>()
    var cur: DispatchInput? = null
    var nameIndent = -1
    var inOptions = false
    while (i < lines.size && indent(lines[i]) > inputsIndent) {
        val l = lines[i]
        val ind = indent(l)
        if (nameIndent < 0) nameIndent = ind
        if (ind == nameIndent) {
            cur?.let(out::add)
            cur = DispatchInput(name = keyOf(l))
            inOptions = false
        } else if (cur != null) {
            val t = l.trim()
            if (inOptions && t.startsWith("- ")) {
                cur = cur.copy(options = cur.options + t.removePrefix("- ").trim().trim('"', '\''))
            } else {
                inOptions = false
                when (keyOf(l)) {
                    "description" -> cur = cur.copy(description = valueOf(l))
                    "type" -> cur = cur.copy(type = valueOf(l))
                    "default" -> cur = cur.copy(default = valueOf(l))
                    "required" -> cur = cur.copy(required = valueOf(l) == "true")
                    "options" -> {
                        val v = valueOf(l)
                        if (v.startsWith("[")) cur = cur.copy(options = v.trim('[', ']').split(',').map { it.trim().trim('"', '\'') }.filter { it.isNotEmpty() })
                        else inOptions = true
                    }
                }
            }
        }
        i++
    }
    cur?.let(out::add)
    return out
}
