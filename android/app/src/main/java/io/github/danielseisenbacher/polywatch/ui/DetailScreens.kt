// Market (`market/<condition_id>`) and cluster (`cluster/<id>`) screens.
package io.github.danielseisenbacher.polywatch.ui

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
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.danielseisenbacher.polywatch.data.AppViewModel
import io.github.danielseisenbacher.polywatch.data.CoreFile
import io.github.danielseisenbacher.polywatch.ui.charts.ClusterGraph
import io.github.danielseisenbacher.polywatch.ui.charts.EntryPlot
import io.github.danielseisenbacher.polywatch.ui.theme.Mono
import io.github.danielseisenbacher.polywatch.ui.theme.NumStyle
import io.github.danielseisenbacher.polywatch.ui.theme.Pw

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun MarketScreen(vm: AppViewModel, id: String, onBack: () -> Unit) {
    val core by rememberCore(vm)
    val go = LocalNav.current
    val ctx = LocalContext.current
    val now = LocalNow.current
    val m = core.marketById[id]

    PwScaffold(
        title = m?.title ?: "Market",
        subtitle = "market · ${shortAddr(id)}",
        onBack = onBack,
        refreshing = core.loading,
        onRefresh = { vm.refresh(listOf(CoreFile.MARKETS, CoreFile.INDEX)) },
        actions = { IconButton(onClick = { copyText(ctx, "condition id", id) }) { Icon(Icons.Filled.ContentCopy, "Copy condition id") } },
    ) { pad ->
        LazyColumn(contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = pad.calculateBottomPadding() + 24.dp)) {
            if (m == null) {
                item {
                    when {
                        core.markets == null && core.err[CoreFile.MARKETS.path] != null -> ErrorCard("markets.json: ${core.err[CoreFile.MARKETS.path]}", onRetry = { vm.refresh() })
                        core.markets == null -> LoadingBox("markets")
                        else -> ErrorCard("Market $id is not in markets.json — only markets with ≥ 2 flagged wallets are published.")
                    }
                }
                return@LazyColumn
            }
            val ws = m.wallets.sortedByDescending { it.score }
            val outs = ws.map { it.outcome }.distinct()
            item {
                Column {
                    Text(m.title, style = MaterialTheme.typography.headlineSmall)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(vertical = 8.dp)) {
                        if (m.slug.isNotEmpty()) LinkChip("Polymarket", Links.market(m.slug))
                    }
                }
            }
            item {
                KpiGrid(
                    listOf(
                        kpi("end date", date(m.endDate), m.endDate?.let { rel(it, now) } ?: ""),
                        kpi("volume", usd(m.volume)),
                        kpi("resolved", m.resolvedOutcome?.let { "✓ $it" } ?: "open", color = if (m.resolvedOutcome != null) Pw.colors.pos else MaterialTheme.colorScheme.onSurfaceVariant),
                        kpi("flagged wallets", num(ws.size)),
                    ) + outs.map { o ->
                        val g = ws.filter { it.outcome == o }
                        val cost = g.sumOf { it.avgPrice * it.size }
                        val sh = g.sumOf { it.size }
                        kpi("on $o", "${g.size} · ${usd(cost)}", if (sh > 0) "avg entry ${price(cost / sh)}" else "")
                    },
                )
            }
            item { SectionTitle("Entries", sub = "x = avg entry price · area ∝ cost basis · colour = tier") }
            item { SectionCard { Box(Modifier.padding(12.dp)) { EntryPlot(m, { core.nameOf(it) }, go) } } }
            item { SectionTitle("Flagged wallets in this market") }
            items(ws, key = { "mw-${it.addr}-${it.outcome}" }) { w ->
                val r = core.byAddr[w.addr]
                val cost = w.avgPrice * w.size
                val mtm = (w.currentPrice - w.avgPrice) * w.size
                Card(
                    onClick = { go("wallet/${w.addr}") },
                    modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                ) {
                    Row(Modifier.padding(12.dp), verticalAlignment = Alignment.Top) {
                        ScoreTile(w.score, r?.tier ?: tierOf(w.score), size = 40)
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            WalletLabel(w.addr, r?.name ?: "")
                            Row(verticalAlignment = Alignment.CenterVertically) {
                                Text(w.outcome, style = MaterialTheme.typography.bodySmall, fontWeight = FontWeight.SemiBold)
                                m.resolvedOutcome?.let { ro ->
                                    Text(if (w.outcome == ro) " ✓" else " ✗", color = if (w.outcome == ro) Pw.colors.pos else Pw.colors.neg, style = MaterialTheme.typography.bodySmall)
                                }
                                Text(" · ${price(w.avgPrice)} → ${price(w.currentPrice)} · ${num(w.size)} sh", style = MaterialTheme.typography.bodySmall.merge(NumStyle), color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                            if (r != null) {
                                Spacer(Modifier.height(4.dp))
                                SignalChips(r.signals)
                            }
                        }
                        Column(horizontalAlignment = Alignment.End) {
                            Text(usd(mtm, sign = true), color = pnlColor(mtm), style = MaterialTheme.typography.titleSmall.merge(NumStyle))
                            Text("cost ${usd(cost)}", style = MaterialTheme.typography.bodySmall.merge(NumStyle), color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
            item { Text("Right column: mark-to-market (current − avg price) × size.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun ClusterScreen(vm: AppViewModel, id: String, onBack: () -> Unit) {
    val core by rememberCore(vm)
    val go = LocalNav.current
    val c = core.clusterById[id]

    PwScaffold(
        title = "Cluster ${id.removePrefix("c-").take(6)}",
        subtitle = c?.let { "${it.members.size} members · ${it.links.size} links" } ?: id,
        onBack = onBack,
        refreshing = core.loading,
        onRefresh = { vm.refresh(listOf(CoreFile.CLUSTERS, CoreFile.INDEX)) },
    ) { pad ->
        LazyColumn(contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = pad.calculateBottomPadding() + 24.dp)) {
            if (c == null) {
                item {
                    when {
                        core.clusters == null && core.err[CoreFile.CLUSTERS.path] != null -> ErrorCard("clusters.json: ${core.err[CoreFile.CLUSTERS.path]}", onRetry = { vm.refresh() })
                        core.clusters == null -> LoadingBox("clusters")
                        else -> ErrorCard("Cluster $id not found in clusters.json.")
                    }
                }
                return@LazyColumn
            }
            val rows = c.members.map { it to core.byAddr[it] }.sortedByDescending { it.second?.score ?: -1 }
            item {
                KpiGrid(
                    listOf(
                        kpi("members", num(c.members.size)),
                        kpi("flagged", num(rows.count { it.second != null })),
                        kpi("max score", num(rows.maxOfOrNull { it.second?.score ?: 0 } ?: 0)),
                        kpi("combined value", usd(rows.sumOf { it.second?.value ?: 0.0 })),
                    ),
                )
            }
            item { SectionTitle("Graph", sub = "edge width = confidence") }
            item { SectionCard { Box(Modifier.padding(12.dp)) { ClusterGraph(c, { core.byAddr[it] }, go) } } }
            item {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalArrangement = Arrangement.spacedBy(6.dp), modifier = Modifier.padding(top = 8.dp)) {
                    c.links.map { it.kind }.distinct().forEach { k ->
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            LinkKindChip(k)
                            Text(" ${linkInfo(k).label}", style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
            item { SectionTitle("Members") }
            items(rows, key = { "m-${it.first}" }) { (a, r) ->
                Card(
                    onClick = { go("wallet/$a") },
                    modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                ) {
                    Row(Modifier.padding(12.dp), verticalAlignment = Alignment.Top) {
                        ScoreTile(r?.score, r?.tier, size = 40)
                        Spacer(Modifier.width(12.dp))
                        Column(Modifier.weight(1f)) {
                            WalletLabel(a, r?.name ?: "")
                            if (r == null) Text("not flagged", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            else {
                                Text(
                                    "${r.ageD?.let { "${num(it)} d old · " } ?: ""}value ${usd(r.value)}",
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                                Spacer(Modifier.height(4.dp))
                                SignalChips(r.signals)
                            }
                        }
                        if (r != null) Text(usd(r.pnl, sign = true), color = pnlColor(r.pnl), style = MaterialTheme.typography.titleSmall.merge(NumStyle))
                    }
                }
            }
            item { SectionTitle("Links") }
            items(c.links.withIndex().toList(), key = { "l-${it.index}" }) { (_, l) ->
                Card(
                    Modifier.fillMaxWidth().padding(bottom = 8.dp),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                ) {
                    Column(Modifier.padding(12.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            LinkKindChip(l.kind)
                            Text(" ${LINKS[l.kind]?.label ?: l.kind}", Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
                            Text("conf. ${fixed(l.confidence, 2)}", style = MaterialTheme.typography.bodySmall, fontFamily = Mono)
                        }
                        Row(Modifier.padding(vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                            WalletLink(l.a, core.nameOf(l.a))
                            Text("  ↔  ", color = MaterialTheme.colorScheme.onSurfaceVariant)
                            WalletLink(l.b, core.nameOf(l.b))
                        }
                        Text(l.evidence, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
        }
    }
}

@Composable
private fun WalletLink(addr: String, name: String) {
    val go = LocalNav.current
    TextButton(onClick = { go("wallet/$addr") }, contentPadding = PaddingValues(horizontal = 4.dp)) {
        Text(name.ifBlank { shortAddr(addr) }, fontFamily = if (name.isBlank()) Mono else null, maxLines = 1)
    }
}
