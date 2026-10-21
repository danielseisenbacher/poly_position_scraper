package io.github.danielseisenbacher.polywatch.data

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Deferred
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.Dispatchers
import java.io.File

/** The dashboard's core files, plus lookups built from them. */
data class CoreState(
    val meta: Meta? = null,
    val index: List<CandidateRow>? = null,
    val markets: List<MarketView>? = null,
    val clusters: List<Cluster>? = null,
    val tape: List<TapeRow>? = null,
    /** file key → error message for the last failed load. */
    val err: Map<String, String> = emptyMap(),
    /** file keys currently shown from the on-device cache because the network failed. */
    val cached: Set<String> = emptySet(),
    val loadedAt: Long = 0,
    val loading: Boolean = false,
) {
    val byAddr: Map<String, CandidateRow> by lazy { index.orEmpty().associateBy { it.addr } }
    val marketById: Map<String, MarketView> by lazy { markets.orEmpty().associateBy { it.conditionId } }
    val clusterById: Map<String, Cluster> by lazy { clusters.orEmpty().associateBy { it.id } }
    val tapeSorted: List<TapeRow> by lazy { tape.orEmpty().sortedByDescending { it.ts } }
    fun nameOf(addr: String, fallback: String = "") = byAddr[addr]?.name?.takeIf { it.isNotBlank() } ?: fallback
}

data class RunsState(
    val runs: List<WorkflowRun> = emptyList(),
    val workflows: List<Workflow> = emptyList(),
    val filter: Long? = null,
    val loading: Boolean = false,
    val err: String? = null,
    val loadedAt: Long = 0,
)

/** Candidate list filters and sort, shared by the Candidates and Stats tabs. */
data class OverviewFilter(
    val q: String = "",
    val tier: String = "all",
    val kinds: Set<String> = emptySet(),
    val watched: Boolean = false,
    val sortKey: String = "score",
    val sortDesc: Boolean = true,
)

enum class CoreFile(val path: String) {
    META("meta.json"), INDEX("index.json"), MARKETS("markets.json"), CLUSTERS("clusters.json"), TAPE("activity_recent.json"),
}

fun nowS(): Long = System.currentTimeMillis() / 1000

class AppViewModel(app: Application) : AndroidViewModel(app) {
    val settings = Settings(app)
    val github = GitHub(settings)

    /** Previous visit, pinned for this process so "new" markers survive navigation. */
    val prevVisit: Long? = settings.lastVisit.also { settings.lastVisit = nowS() }
    fun isNew(r: CandidateRow) = prevVisit != null && r.firstSeen > prevVisit

    private val _core = MutableStateFlow(CoreState())
    val core: StateFlow<CoreState> = _core.asStateFlow()

    private val _runs = MutableStateFlow(RunsState())
    val runs: StateFlow<RunsState> = _runs.asStateFlow()

    private val _overview = MutableStateFlow(OverviewFilter(sortKey = settings.sortKey, sortDesc = settings.sortDesc))
    val overview: StateFlow<OverviewFilter> = _overview.asStateFlow()

    fun setOverview(f: OverviewFilter) {
        _overview.value = f
        settings.sortKey = f.sortKey
        settings.sortDesc = f.sortDesc
    }

    private val _dynamicColor = MutableStateFlow(settings.dynamicColor)
    val dynamicColor: StateFlow<Boolean> = _dynamicColor.asStateFlow()

    private val loadLock = Mutex()
    private val wallets = mutableMapOf<String, Deferred<Wallet>>()
    private val cacheDir get() = File(getApplication<Application>().filesDir, "data-cache")

    init {
        refresh()
    }

    // ---- core data ---------------------------------------------------------------

    fun refresh(files: Collection<CoreFile> = CoreFile.entries) {
        viewModelScope.launch { load(files) }
    }

    /** Refresh the files the dashboard re-polls every 5 minutes. */
    fun autoRefresh() = refresh(listOf(CoreFile.META, CoreFile.INDEX, CoreFile.TAPE))

    private fun url(path: String): String {
        val t = System.currentTimeMillis() / 300_000
        return "${settings.dataBase}$path${if ('?' in path) '&' else '?'}t=$t"
    }

    private suspend fun fetchCached(path: String): Pair<String, Boolean> {
        val f = File(cacheDir, path)
        return try {
            val text = Http.getText(url(path))
            withContext(Dispatchers.IO) {
                f.parentFile?.mkdirs()
                f.writeText(text)
            }
            text to false
        } catch (e: FetchError) {
            // Network trouble: fall back to the last good copy. A 404 means "really gone".
            if (e.status == 0 && f.exists()) withContext(Dispatchers.IO) { f.readText() } to true else throw e
        }
    }

    private suspend fun load(files: Collection<CoreFile>) = loadLock.withLock {
        _core.update { it.copy(loading = true) }
        val results = coroutineScope {
            files.map { f -> async { f to runCatching { fetchCached(f.path) } } }.awaitAll()
        }
        _core.update { s ->
            var n = s.copy(loading = false, loadedAt = System.currentTimeMillis())
            val err = s.err.toMutableMap()
            val cached = s.cached.toMutableSet()
            for ((f, r) in results) {
                r.fold(
                    onSuccess = { (text, fromCache) ->
                        try {
                            n = when (f) {
                                CoreFile.META -> n.copy(meta = json.decodeFromString(text))
                                CoreFile.INDEX -> n.copy(index = json.decodeFromString(text))
                                CoreFile.MARKETS -> n.copy(markets = json.decodeFromString(text))
                                CoreFile.CLUSTERS -> n.copy(clusters = json.decodeFromString(text))
                                CoreFile.TAPE -> n.copy(tape = json.decodeFromString(text))
                            }
                            err.remove(f.path)
                            if (fromCache) cached += f.path else cached -= f.path
                        } catch (e: Exception) {
                            err[f.path] = "invalid JSON (${e.message?.take(120)})"
                        }
                    },
                    onFailure = { e -> err[f.path] = e.message ?: e.toString() },
                )
            }
            n.copy(err = err, cached = cached)
        }
    }

    // ---- wallets -----------------------------------------------------------------

    /** Lazy, cached wallet file. A failed load is evicted so a retry refetches. */
    suspend fun wallet(addr: String, force: Boolean = false): Wallet {
        val a = addr.lowercase()
        val d = synchronized(wallets) {
            if (force) wallets.remove(a)
            wallets.getOrPut(a) {
                viewModelScope.async { json.decodeFromString<Wallet>(fetchCached("wallets/$a.json").first) }
            }
        }
        return try {
            d.await()
        } catch (e: Exception) {
            synchronized(wallets) { if (wallets[a] === d) wallets.remove(a) }
            throw e
        }
    }

    // ---- GitHub Actions ------------------------------------------------------------

    fun refreshRuns(filter: Long? = _runs.value.filter) {
        viewModelScope.launch {
            _runs.update { it.copy(loading = true, filter = filter) }
            try {
                val wf = if (_runs.value.workflows.isEmpty()) runCatching { github.workflows() }.getOrDefault(emptyList()) else _runs.value.workflows
                val page = github.runs(filter)
                _runs.update { it.copy(runs = page.runs, workflows = wf, loading = false, err = null, loadedAt = System.currentTimeMillis()) }
            } catch (e: Exception) {
                _runs.update { it.copy(loading = false, err = e.message ?: e.toString()) }
            }
        }
    }

    fun setDynamicColor(on: Boolean) {
        settings.dynamicColor = on
        _dynamicColor.value = on
    }

    /** After the repo or data source changes: drop everything and reload. */
    fun resetSource() {
        synchronized(wallets) { wallets.clear() }
        cacheDir.deleteRecursively()
        _core.value = CoreState()
        _runs.value = RunsState()
        refresh()
    }
}
