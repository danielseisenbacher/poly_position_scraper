package io.github.danielseisenbacher.polywatch.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.danielseisenbacher.polywatch.data.AppViewModel
import io.github.danielseisenbacher.polywatch.data.CoreFile
import io.github.danielseisenbacher.polywatch.ui.charts.Histogram
import io.github.danielseisenbacher.polywatch.ui.theme.NumStyle
import io.github.danielseisenbacher.polywatch.ui.theme.Pw

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun StatsScreen(vm: AppViewModel, onFilterSignal: () -> Unit) {
    val core by rememberCore(vm)
    val f by vm.overview.collectAsStateWithLifecycle()
    val go = LocalNav.current
    val now = LocalNow.current
    val pw = Pw.colors
    var sheet by remember { mutableStateOf<TradeRef?>(null) }
    val m = core.meta

    PwScaffold(
        title = "Stats",
        subtitle = m?.let { "generated ${rel(it.generatedAt, now)}" },
        refreshing = core.loading,
        onRefresh = { vm.refresh() },
    ) { pad ->
        LazyColumn(contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = pad.calculateBottomPadding() + 24.dp)) {
            if (m == null) {
                item {
                    val e = core.err[CoreFile.META.path]
                    if (e != null) ErrorCard("meta.json: $e", onRetry = { vm.refresh() }) else LoadingBox("status")
                }
            } else {
                // ---- pipeline health (the dashboard header) ----
                item {
                    val stale = now - m.lastTrack > 1800
                    val api = m.api
                    KpiGrid(
                        listOf(
                            kpi("discovery", rel(m.lastDiscover, now), date(m.lastDiscover, true)),
                            kpi("tracker", rel(m.lastTrack, now), if (stale) "⚠ stale — more than 30 min old" else date(m.lastTrack, true), if (stale) pw.warn else null),
                            kpi("API lag", "${num(m.feedLagS)} s", "Data API /v2/status", if (m.feedLagS > 600) pw.warn else null),
                            kpi(
                                "API health",
                                if (api.rateLimited > 0 || api.errors > 0) "⚠ ${num(api.rateLimited)}×429 · ${num(api.errors)} err" else "✓ ok",
                                "requests: " + api.requests.entries.joinToString(", ") { "${it.key} ${num(it.value)}" },
                                if (api.rateLimited > 0 || api.errors > 0) pw.warn else pw.pos,
                            ),
                            kpi("watchlist", num(m.counts.watchlist)),
                            kpi("candidates", num(m.counts.candidates), "${num(m.counts.high)} high · ${num(m.counts.medium)} medium"),
                            kpi("clusters", num(m.counts.clusters)),
                            kpi("last run", "${num(m.counts.walletsScannedLastRun)} scanned", "${num(m.counts.tradesTrackedLastRun)} trades tracked"),
                        ),
                    )
                }
                item {
                    Column(Modifier.padding(top = 8.dp)) {
                        if (m.feedTruncated) Text("⚠ Feed truncated — some trades were skipped this run", color = pw.warn, style = MaterialTheme.typography.bodyMedium)
                        core.err.forEach { (k, v) -> Text("⚠ $k: $v", color = pw.warn, style = MaterialTheme.typography.bodySmall) }
                        if (core.cached.isNotEmpty()) Text("Offline: showing the last downloaded copy of ${core.cached.joinToString()}", color = pw.warn, style = MaterialTheme.typography.bodySmall)
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            Text(
                                "Generated ${date(m.generatedAt, true)}" + (m.runId?.let { " · run $it" } ?: ""),
                                Modifier.weight(1f),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                            m.runId?.let { id -> TextButton(onClick = { go("run/$id") }) { Text("View run") } }
                        }
                    }
                }

                // ---- score distribution ----
                item { SectionTitle("Score distribution", sub = "all scored wallets, log scale") }
                item { SectionCard { Box(Modifier.padding(12.dp)) { Histogram(m.scoreHistogram, go) } } }

                // ---- signal frequency ----
                item { SectionTitle("Signal frequency", sub = "tap a signal to filter the candidate list") }
                item {
                    val rows = m.signalCounts.entries.sortedByDescending { it.value }
                    val max = (rows.maxOfOrNull { it.value } ?: 1).coerceAtLeast(1)
                    SectionCard {
                        Column(Modifier.padding(vertical = 6.dp)) {
                            if (rows.isEmpty()) EmptyText("No signals", Modifier.padding(horizontal = 12.dp))
                            rows.forEach { (k, v) ->
                                val i = sigInfo(k)
                                val on = k in f.kinds
                                Row(
                                    Modifier
                                        .fillMaxWidth()
                                        .clickable {
                                            vm.setOverview(f.copy(kinds = if (on) f.kinds - k else f.kinds + k))
                                            if (!on) onFilterSignal()
                                        }
                                        .padding(horizontal = 12.dp, vertical = 8.dp),
                                    verticalAlignment = Alignment.CenterVertically,
                                ) {
                                    SignalChip(k)
                                    Spacer(Modifier.width(8.dp))
                                    Text(i.label, Modifier.width(140.dp), style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                    Box(Modifier.weight(1f).height(8.dp).clip(RoundedCornerShape(4.dp)).background(MaterialTheme.colorScheme.surfaceVariant)) {
                                        Box(Modifier.fillMaxWidth(v.toFloat() / max).height(8.dp).clip(RoundedCornerShape(4.dp)).background(pw.sig(i.c)))
                                    }
                                    Text(num(v), Modifier.width(40.dp).padding(start = 8.dp), style = MaterialTheme.typography.bodyMedium.merge(NumStyle))
                                    if (on) Icon(Icons.Filled.Check, "filter active", tint = MaterialTheme.colorScheme.primary)
                                }
                            }
                        }
                    }
                }
            }

            // ---- converging markets ----
            item { SectionTitle("Markets with ≥ 2 flagged wallets") }
            item {
                val ms = core.markets.orEmpty().filter { it.wallets.size >= 2 }
                SectionCard {
                    Column {
                        when {
                            core.err[CoreFile.MARKETS.path] != null -> ErrorCard("markets.json: ${core.err[CoreFile.MARKETS.path]}", onRetry = { vm.refresh() })
                            ms.isEmpty() -> EmptyText("None yet", Modifier.padding(horizontal = 16.dp))
                        }
                        ms.take(12).forEachIndexed { idx, mk ->
                            if (idx > 0) HorizontalDivider(Modifier.padding(horizontal = 16.dp))
                            ListItem(
                                headlineContent = { Text(mk.title, maxLines = 2, overflow = TextOverflow.Ellipsis) },
                                supportingContent = {
                                    Text(
                                        "${mk.wallets.size} wallets · max score ${mk.wallets.maxOf { it.score }}" + (mk.resolvedOutcome?.let { " · ✓ $it" } ?: ""),
                                    )
                                },
                                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                                modifier = Modifier.clickable { go("market/${mk.conditionId}") },
                            )
                        }
                    }
                }
            }

            // ---- latest trades ----
            item {
                SectionTitle("Latest trades") { TextButton(onClick = { go("tape") }) { Text("Tape →") } }
            }
            val latest = core.tapeSorted.take(15)
            if (core.tape == null && core.err[CoreFile.TAPE.path] != null) item { ErrorCard("activity_recent.json: ${core.err[CoreFile.TAPE.path]}") }
            else if (latest.isEmpty()) item { EmptyText("No trades in the last 48 h") }
            items(latest, key = { "${it.tx}${it.ts}${it.addr}" }) { t ->
                TradeItem(t.kind, t.title, t.name.ifBlank { shortAddr(t.addr) }, t.usdc, t.ts, onClick = {
                    sheet = TradeRef(t.addr, t.name, t.title, t.conditionId, t.tx, "${t.kind} ${t.outcome} @ ${price(t.price)} · ${usd(t.usdc)} · ${date(t.ts, true)}")
                })
            }

            // ---- about ----
            item { SectionTitle("About") }
            item {
                Column {
                    Text(
                        "Heuristic leads, not proof of insider trading. Scores flag unusual patterns; most have innocent explanations.",
                        style = MaterialTheme.typography.bodyMedium,
                    )
                    Text(
                        "Data: ${vm.settings.dataBase.removePrefix("https://")} · auto-refresh every 5 min",
                        Modifier.padding(top = 6.dp),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                        LinkChip("${vm.settings.owner}/${vm.settings.repo}", Links.repo(vm.settings))
                        LinkChip("insider-candidate issues", Links.issues(vm.settings))
                        LinkChip("Web dashboard", "https://${vm.settings.owner}.github.io/${vm.settings.repo}/")
                    }
                }
            }
        }
    }
    sheet?.let { t -> TradeSheet(t, core.marketById.containsKey(t.conditionId)) { sheet = null } }
}
