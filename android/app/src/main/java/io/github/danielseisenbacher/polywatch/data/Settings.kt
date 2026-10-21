package io.github.danielseisenbacher.polywatch.data

import android.content.Context
import androidx.core.content.edit

/** App preferences. Private to the app and excluded from backups (see data_extraction_rules.xml). */
class Settings(context: Context) {
    private val p = context.getSharedPreferences("polywatch", Context.MODE_PRIVATE)

    var owner: String
        get() = p.getString("owner", null)?.takeIf { it.isNotBlank() } ?: DEFAULT_OWNER
        set(v) = p.edit { putString("owner", v.trim()) }

    var repo: String
        get() = p.getString("repo", null)?.takeIf { it.isNotBlank() } ?: DEFAULT_REPO
        set(v) = p.edit { putString("repo", v.trim()) }

    /** Override for the data base URL (the dashboard's `?data=` parameter). Blank = the repo's `data` branch. */
    var dataOverride: String
        get() = p.getString("data", "") ?: ""
        set(v) = p.edit { putString("data", v.trim()) }

    val dataBase: String
        get() {
            val b = dataOverride.ifBlank { "https://raw.githubusercontent.com/$owner/$repo/data/" }
            return if (b.endsWith("/")) b else "$b/"
        }

    /** Optional GitHub token: lifts the API rate limit and enables run/cancel/re-run. */
    var token: String
        get() = p.getString("token", "") ?: ""
        set(v) = p.edit { putString("token", v.trim()) }

    var dynamicColor: Boolean
        get() = p.getBoolean("dynamic_color", true)
        set(v) = p.edit { putBoolean("dynamic_color", v) }

    var lastVisit: Long?
        get() = p.getLong("last_visit", -1).takeIf { it >= 0 }
        set(v) = p.edit { putLong("last_visit", v ?: -1) }

    var sortKey: String
        get() = p.getString("sort_key", "score") ?: "score"
        set(v) = p.edit { putString("sort_key", v) }

    var sortDesc: Boolean
        get() = p.getBoolean("sort_desc", true)
        set(v) = p.edit { putBoolean("sort_desc", v) }

    companion object {
        const val DEFAULT_OWNER = "danielseisenbacher"
        const val DEFAULT_REPO = "poly_position_scraper"
    }
}
