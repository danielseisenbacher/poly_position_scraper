package io.github.danielseisenbacher.polywatch.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Sort
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.Info
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.WarningAmber
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.danielseisenbacher.polywatch.data.AppViewModel
import io.github.danielseisenbacher.polywatch.data.CandidateRow
import io.github.danielseisenbacher.polywatch.data.CoreFile
import io.github.danielseisenbacher.polywatch.data.CoreState
import io.github.danielseisenbacher.polywatch.data.OverviewFilter
import io.github.danielseisenbacher.polywatch.ui.theme.Pw

/** Sort keys, mirroring the dashboard's candidate table columns. */
private class SortCol(val key: String, val label: String, val numeric: Boolean, val v: (CandidateRow, Boolean) -> Comparable<*>?)

private val SORTS = listOf(
    SortCol("score", "Score", true) { r, _ -> r.score },
    SortCol("signals", "Signal strength", true) { r, _ -> r.signals.sumOf { it.strength } },
    SortCol("new", "New since last visit", true) { _, n -> if (n) 1 else 0 },
    SortCol("pnl", "PnL", true) { r, _ -> r.pnl },
    SortCol("value", "Value", true) { r, _ -> r.value },
    SortCol("win", "Win rate", true) { r, _ -> r.winRate },
    SortCol("z", "Win-rate z-score", true) { r, _ -> r.z },
    SortCol("age", "Account age", true) { r, _ -> r.ageD },
    SortCol("mkts", "Markets", true) { r, _ -> r.nMarkets },
    SortCol("last", "Last trade", true) { r, _ -> r.lastTradeTs },
    SortCol("issue", "Issue number", true) { r, _ -> r.issue },
    SortCol("watched", "Watched", true) { r, _ -> if (r.watched) 1 else 0 },
    SortCol("cluster", "Cluster", false) { r, _ -> r.clusterId },
    SortCol("wallet", "Name", false) { r, _ -> (r.name.ifBlank { r.addr }).lowercase() },
)

/** Nulls last regardless of direction, like the dashboard's `cmp`. */
@Suppress("UNCHECKED_CAST")
private fun cmp(a: Comparable<*>?, b: Comparable<*>?, desc: Boolean): Int {
    val na = a == null || (a is Double && a.isNaN())
    val nb = b == null || (b is Double && b.isNaN())
    if (na || nb) return if (na == nb) 0 else if (na) 1 else -1
    val c = (a as Comparable<Any>).compareTo(b as Any)
    return if (desc) -c else c
}

fun filteredCandidates(core: CoreState, f: OverviewFilter, isNew: (CandidateRow) -> Boolean): List<CandidateRow> {
    val q = f.q.trim().lowercase()
    val col = SORTS.firstOrNull { it.key == f.sortKey } ?: SORTS[0]
    return core.index.orEmpty()
        .filter { r ->
            (q.isEmpty() || q in r.addr || q in r.name.lowercase()) &&
                (f.tier == "all" || r.tier == f.tier) &&
                (!f.watched || r.watched) &&
                (f.kinds.isEmpty() || r.signals.any { it.kind in f.kinds })
        }
        .sortedWith { a, b -> cmp(col.v(a, isNew(a)), col.v(b, isNew(b)), f.sortDesc).takeIf { it != 0 } ?: (b.score - a.score) }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun OverviewScreen(vm: AppViewModel) {
    val core by rememberCore(vm)
    val f by vm.overview.collectAsStateWithLifecycle()
    val go = LocalNav.current
    val now = LocalNow.current
    var info by remember { mutableStateOf(false) }
    var sortMenu by remember { mutableStateOf(false) }
    var q by rememberSaveable { mutableStateOf(f.q) }

    val rows = remember(core, f) { filteredCandidates(core, f, vm::isNew) }
    val counts = remember(core.index) {
        val m = mutableMapOf("all" to (core.index?.size ?: 0))
        core.index.orEmpty().forEach { m[it.tier] = (m[it.tier] ?: 0) + 1 }
        m
    }

    PwScaffold(
        title = "polywatch",
        subtitle = core.meta?.let { "tracker ${rel(it.lastTrack, now)} · ${num(it.counts.candidates)} candidates" } ?: if (core.loading) "loading…" else null,
        refreshing = core.loading,
        onRefresh = { vm.refresh() },
        actions = {
            IconButton(onClick = { info = true }) { Icon(Icons.Filled.Info, "About these scores") }
            IconButton(onClick = { go("settings") }) { Icon(Icons.Filled.Settings, "Settings") }
        },
    ) { pad ->
        LazyColumn(
            contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = pad.calculateBottomPadding() + 16.dp),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            item(key = "status") { StatusStrip(core, now) { go("stats") } }
            core.err[CoreFile.INDEX.path]?.takeIf { core.index == null }?.let { e ->
                item { ErrorCard("index.json: $e", onRetry = { vm.refresh() }) }
            }
            if (core.index == null && core.loading) item { LoadingBox("candidates") }
            if (core.index != null) {
                item(key = "search") {
                    OutlinedTextField(
                        value = q,
                        onValueChange = { q = it; vm.setOverview(f.copy(q = it)) },
                        modifier = Modifier.fillMaxWidth(),
                        placeholder = { Text("Filter address or name") },
                        leadingIcon = { Icon(Icons.Filled.Search, null) },
                        trailingIcon = {
                            if (q.isNotEmpty()) IconButton(onClick = { q = ""; vm.setOverview(f.copy(q = "")) }) { Icon(Icons.Filled.Clear, "Clear filter") }
                        },
                        singleLine = true,
                        shape = RoundedCornerShape(28.dp),
                        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                    )
                }
                item(key = "tier") {
                    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                        val tiers = listOf("all", "high", "medium", "low")
                        tiers.forEachIndexed { i, t ->
                            SegmentedButton(
                                selected = f.tier == t,
                                onClick = { vm.setOverview(f.copy(tier = t)) },
                                shape = SegmentedButtonDefaults.itemShape(i, tiers.size),
                                icon = {},
                                label = { Text("${t.replaceFirstChar { it.uppercase() }} ${counts[t] ?: 0}", fontSize = 13.sp, maxLines = 1) },
                            )
                        }
                    }
                }
                item(key = "filters") {
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                        FilterChip(
                            selected = f.watched,
                            onClick = { vm.setOverview(f.copy(watched = !f.watched)) },
                            label = { Text("Watched only") },
                            leadingIcon = { Icon(if (f.watched) Icons.Filled.Check else Icons.Filled.Visibility, null, Modifier.size(FilterChipDefaults.IconSize)) },
                        )
                        Column {
                            FilterChip(
                                selected = true,
                                onClick = { sortMenu = true },
                                label = { Text(SORTS.firstOrNull { it.key == f.sortKey }?.label ?: "Score") },
                                leadingIcon = { Icon(Icons.AutoMirrored.Filled.Sort, null, Modifier.size(FilterChipDefaults.IconSize)) },
                                trailingIcon = { Icon(if (f.sortDesc) Icons.Filled.ArrowDownward else Icons.Filled.ArrowUpward, if (f.sortDesc) "descending" else "ascending", Modifier.size(FilterChipDefaults.IconSize)) },
                            )
                            DropdownMenu(sortMenu, onDismissRequest = { sortMenu = false }) {
                                SORTS.forEach { s ->
                                    DropdownMenuItem(
                                        text = { Text(s.label) },
                                        leadingIcon = {
                                            if (s.key == f.sortKey) Icon(if (f.sortDesc) Icons.Filled.ArrowDownward else Icons.Filled.ArrowUpward, null)
                                        },
                                        onClick = {
                                            sortMenu = false
                                            vm.setOverview(f.copy(sortKey = s.key, sortDesc = if (s.key == f.sortKey) !f.sortDesc else s.numeric))
                                        },
                                    )
                                }
                            }
                        }
                    }
                }
                item(key = "signals") {
                    Row(Modifier.horizontalScroll(rememberScrollState()), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                        SIGNALS.forEach { (k, i) ->
                            val on = k in f.kinds
                            FilterChip(
                                selected = on,
                                onClick = { vm.setOverview(f.copy(kinds = if (on) f.kinds - k else f.kinds + k)) },
                                label = { Text(i.abbr) },
                                leadingIcon = {
                                    Box(Modifier.size(10.dp).clip(CircleShape).background(Pw.colors.sig(i.c)))
                                },
                            )
                        }
                        if (f.kinds.isNotEmpty()) TextButton(onClick = { vm.setOverview(f.copy(kinds = emptySet())) }) { Text("Clear") }
                    }
                }
                item(key = "count") {
                    Text(
                        "${rows.size} of ${core.index?.size ?: 0}" + if (f.kinds.isNotEmpty()) " · signals: " + f.kinds.joinToString { sigInfo(it).label } else "",
                        style = MaterialTheme.typography.labelMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (rows.isEmpty()) item { EmptyText("No candidates match the filters") }
                items(rows, key = { it.addr }) { r ->
                    CandidateCard(r, vm.isNew(r)) { n -> go(Links.issue(vm.settings, n)) }
                }
                item(key = "legend") {
                    Text(
                        "Tap a wallet for details. Chips: " + SIGNALS.values.joinToString(" · ") { "${it.abbr} = ${it.label}" } + ". Chip opacity = signal strength.",
                        Modifier.padding(top = 8.dp),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }

    if (info) AlertDialog(
        onDismissRequest = { info = false },
        confirmButton = { TextButton(onClick = { info = false }) { Text("OK") } },
        dismissButton = { TextButton(onClick = { info = false; go(Links.issues(vm.settings)) }) { Text("Open issues") } },
        icon = { Icon(Icons.Filled.Info, null) },
        title = { Text("Heuristic leads, not proof") },
        text = {
            Text(
                "polywatch flags Polymarket wallets whose trading looks unusual: new accounts betting big on longshots, " +
                    "improbable win rates, leaderboard newcomers, linked accounts and well-timed bets. " +
                    "Scores flag unusual patterns; most have innocent explanations — the signals also catch lucky, skilled or contrarian traders.\n\n" +
                    "Each flagged wallet has a GitHub issue labelled insider-candidate; new findings and big trades are added as comments.",
            )
        },
    )
}

/** Compact pipeline health line; the full version lives on the Stats tab. */
@Composable
fun StatusStrip(core: CoreState, now: Long, onClick: () -> Unit) {
    val m = core.meta
    val stale = m != null && now - m.lastTrack > 1800
    val problems = buildList {
        if (stale) add("tracker stale")
        if (m != null && (m.api.rateLimited > 0 || m.api.errors > 0)) add("${m.api.rateLimited}×429 · ${m.api.errors} err")
        if (m?.feedTruncated == true) add("feed truncated")
        if (core.err.isNotEmpty()) add("${core.err.size} file(s) failed")
        if (core.cached.isNotEmpty()) add("offline copy")
    }
    Card(
        onClick = onClick,
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer),
    ) {
        Row(Modifier.padding(horizontal = 14.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            if (problems.isNotEmpty()) {
                Icon(Icons.Filled.WarningAmber, null, tint = MaterialTheme.colorScheme.error, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(10.dp))
            }
            Column(Modifier.weight(1f)) {
                if (m == null) {
                    Text(core.err["meta.json"]?.let { "meta.json: $it" } ?: "Loading status…", style = MaterialTheme.typography.bodyMedium)
                } else {
                    Text(
                        "discovery ${rel(m.lastDiscover, now)} · tracker ${relShort(m.lastTrack, now)}",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Text(
                        "${num(m.counts.watchlist)} watched · ${num(m.counts.high)} high · ${num(m.counts.medium)} medium · ${num(m.counts.clusters)} clusters",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSecondaryContainer.copy(alpha = 0.8f),
                    )
                    if (problems.isNotEmpty()) Text(problems.joinToString(" · "), style = MaterialTheme.typography.bodySmall, fontWeight = androidx.compose.ui.text.font.FontWeight.SemiBold, color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }
}
