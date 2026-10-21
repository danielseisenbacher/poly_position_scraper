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
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.Sort
import androidx.compose.material.icons.filled.ArrowDownward
import androidx.compose.material.icons.filled.ArrowUpward
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilterChipDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SuggestionChip
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.danielseisenbacher.polywatch.data.AppViewModel
import io.github.danielseisenbacher.polywatch.data.FetchError
import io.github.danielseisenbacher.polywatch.data.Position
import io.github.danielseisenbacher.polywatch.data.Wallet
import io.github.danielseisenbacher.polywatch.ui.charts.Timeline
import io.github.danielseisenbacher.polywatch.ui.theme.Mono
import io.github.danielseisenbacher.polywatch.ui.theme.Pw
import kotlinx.coroutines.CancellationException

private class PosSort(val key: String, val label: String, val numeric: Boolean, val v: (Position) -> Comparable<*>?)

private val POS_SORTS = listOf(
    PosSort("pnl", "PnL", true) { it.totalPnl },
    PosSort("cost", "Cost", true) { it.cost },
    PosSort("value", "Value", true) { it.currentValue },
    PosSort("size", "Size", true) { it.totalSize },
    PosSort("avg", "Price move (avg → now)", true) { it.currentPrice - it.avgPrice },
    PosSort("entry", "First entry", true) { it.firstEntryAt },
    PosSort("end", "End date", true) { it.endDate },
    PosSort("status", "Status", false) { it.status },
    PosSort("title", "Market", false) { it.title.lowercase() },
    PosSort("outcome", "Outcome", false) { it.outcome },
)

@Suppress("UNCHECKED_CAST")
private fun cmpN(a: Comparable<*>?, b: Comparable<*>?, desc: Boolean): Int {
    if (a == null || b == null) return if (a == b) 0 else if (a == null) 1 else -1
    val c = (a as Comparable<Any>).compareTo(b as Any)
    return if (desc) -c else c
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun WalletScreen(vm: AppViewModel, addrIn: String, onBack: () -> Unit) {
    val core by rememberCore(vm)
    val a = addrIn.lowercase()
    val row = core.byAddr[a]
    val ctx = LocalContext.current
    val go = LocalNav.current
    val now = LocalNow.current

    var reload by remember { mutableIntStateOf(0) }
    var result by remember { mutableStateOf<Result<Wallet>?>(null) }
    var loading by remember { mutableStateOf(true) }
    LaunchedEffect(a, reload) {
        if (!ADDR_RE.matches(a)) { result = Result.failure(FetchError("\"$addrIn\" is not a wallet address", -1)); loading = false; return@LaunchedEffect }
        loading = true
        result = try {
            Result.success(vm.wallet(a, force = reload > 0))
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            Result.failure(e)
        }
        loading = false
    }
    val w = result?.getOrNull()
    val name = w?.profile?.let { it.name.ifBlank { it.pseudonym } }?.ifBlank { null } ?: row?.name?.ifBlank { null } ?: ""

    var posSort by rememberSaveable { mutableStateOf("pnl") }
    var posDesc by rememberSaveable { mutableStateOf(true) }
    var posMenu by remember { mutableStateOf(false) }
    var posLimit by rememberSaveable { mutableIntStateOf(30) }
    var tradeLimit by rememberSaveable { mutableIntStateOf(50) }
    var sheet by remember { mutableStateOf<TradeRef?>(null) }

    PwScaffold(
        title = name.ifBlank { shortAddr(a) },
        subtitle = if (name.isNotBlank()) shortAddr(a) else "wallet",
        onBack = onBack,
        refreshing = loading && w != null,
        onRefresh = { reload++ },
        actions = {
            IconButton(onClick = { copyText(ctx, "address", a) }) { Icon(Icons.Filled.ContentCopy, "Copy address") }
            IconButton(onClick = {
                val i = android.content.Intent(android.content.Intent.ACTION_SEND).apply {
                    type = "text/plain"
                    putExtra(android.content.Intent.EXTRA_TEXT, Links.profile(a))
                }
                ctx.startActivity(android.content.Intent.createChooser(i, "Share wallet"))
            }) { Icon(Icons.Filled.Share, "Share") }
        },
    ) { pad ->
        LazyColumn(contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = pad.calculateBottomPadding() + 24.dp)) {
            val err = result?.exceptionOrNull()
            if (w == null) {
                if (err == null) item { LoadingBox("wallet ${shortAddr(a)}") }
                else {
                    item {
                        val msg = if ((err as? FetchError)?.status == 404) "No wallet file published for $a (wallets/$a.json → 404)." else err.message ?: err.toString()
                        ErrorCard(msg, onRetry = if ((err as? FetchError)?.status != -1) ({ reload++ }) else null)
                    }
                    if (ADDR_RE.matches(a)) item {
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 12.dp)) {
                            LinkChip("Polymarket profile", Links.profile(a))
                            LinkChip("Polygonscan", Links.scanAddr(a))
                            row?.issue?.let { LinkChip("issue #$it", Links.issue(vm.settings, it)) }
                        }
                    }
                    if (row != null) item {
                        Column(Modifier.padding(top = 12.dp)) {
                            Text("From index", style = MaterialTheme.typography.titleSmall)
                            ScoreInline(row.score, row.tier)
                            Spacer(Modifier.height(6.dp))
                            SignalChips(row.signals)
                        }
                    }
                }
                return@LazyColumn
            }

            val p = w.profile
            val st = w.stats
            val joined = st?.joinDate ?: p?.createdAt
            val r = w.resolved
            val realized = st?.realizedPnl ?: w.positions.sumOf { it.realizedPnl }
            val issue = w.issue ?: row?.issue
            val trades = w.trades.sortedByDescending { it.ts }

            // ---- identity ----
            item {
                Column {
                    SelectionContainer { Text(a, style = MaterialTheme.typography.bodySmall, fontFamily = Mono, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                    if (p != null && p.pseudonym.isNotBlank() && p.name.isNotBlank()) Text("aka ${p.pseudonym}", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                        LinkChip("Polymarket", Links.profile(a))
                        LinkChip("Polygonscan", Links.scanAddr(a))
                        issue?.let { LinkChip("issue #$it", Links.issue(vm.settings, it)) }
                        p?.xUsername?.takeIf { it.isNotBlank() }?.let { LinkChip("@$it", Links.x(it)) }
                        w.clusterId?.let { cid ->
                            AssistChip(onClick = { go("cluster/$cid") }, label = { Text("cluster ${cid.removePrefix("c-").take(4)}") }, leadingIcon = { Icon(Icons.Filled.Hub, null, Modifier.size(18.dp)) })
                        }
                        w.sources.forEach { s -> SuggestionChip(onClick = {}, label = { Text(s) }) }
                    }
                }
            }

            // ---- KPIs ----
            item {
                Spacer(Modifier.height(12.dp))
                KpiGrid(
                    listOfNotNull(
                        Kpi("score", { ScoreInline(w.score.total ?: row?.score, w.score.tier ?: row?.tier) }, "materiality ×${fixed(w.score.materiality, 2)}"),
                        kpi("value", usd(w.value)),
                        kpi("realized PnL", usd(realized, sign = true), if ((st?.biggestWin ?: 0.0) > 0) "best ${usd(st?.biggestWin)}" else "", pnlColor(realized)),
                        kpi("win rate", if (r.n > 0) pct(r.wins / r.n) else "–", if (r.n > 0) "${frac(r.wins)}/${frac(r.n)} · z ${fixed(r.z)}" else ""),
                        kpi("resolved", frac(r.n), "exp. wins ${fixed(r.expectedWins)}"),
                        kpi("longshot wins", num(r.longshotWins)),
                        kpi("account age", joined?.let { "${num((now - it) / 86400.0)} d" } ?: "–", joined?.let { date(it) } ?: ""),
                        kpi("first seen", rel(w.firstSeen, now), "scored ${rel(w.lastScored, now)}"),
                        if ((st?.volumeUsdc ?: 0.0) > 0) kpi("volume", usd(st?.volumeUsdc), "${num(st?.marketsTraded)} mkts · ${num(st?.tradeCount)} trades") else null,
                    ),
                )
            }

            // ---- signals ----
            item { SectionTitle("Signals") }
            if (w.signals.isEmpty()) item { EmptyText("No signals") }
            items(w.signals, key = { "sig-${it.kind}" }) { s ->
                val i = sigInfo(s.kind)
                Card(
                    Modifier.fillMaxWidth().padding(bottom = 8.dp),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                ) {
                    Column(Modifier.padding(12.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            SignalChip(s.kind)
                            Spacer(Modifier.width(8.dp))
                            Text(i.label, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
                            Text(fixed(s.strength, 2), style = MaterialTheme.typography.titleSmall.merge(io.github.danielseisenbacher.polywatch.ui.theme.NumStyle))
                        }
                        Box(Modifier.padding(vertical = 8.dp).fillMaxWidth().height(6.dp).clip(RoundedCornerShape(3.dp)).background(MaterialTheme.colorScheme.surfaceVariant)) {
                            Box(Modifier.fillMaxWidth(s.strength.toFloat().coerceIn(0f, 1f)).height(6.dp).clip(RoundedCornerShape(3.dp)).background(Pw.colors.sig(i.c)))
                        }
                        Text(s.evidence, style = MaterialTheme.typography.bodyMedium)
                        if (s.refs.isNotEmpty()) {
                            Column(Modifier.padding(top = 6.dp)) {
                                s.refs.forEach { cid ->
                                    val pos = w.positions.firstOrNull { it.conditionId == cid }
                                    val t = pos?.title ?: (cid.take(10) + "…")
                                    val has = core.marketById.containsKey(cid)
                                    Text(
                                        "• $t",
                                        Modifier.then(if (has) Modifier.clickable { go("market/$cid") } else Modifier).padding(vertical = 2.dp),
                                        style = MaterialTheme.typography.bodySmall,
                                        color = if (has) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                                    )
                                }
                            }
                        }
                    }
                }
            }

            // ---- timeline ----
            item { SectionTitle("Trade timeline", sub = "lane per market · tint = won/lost") }
            item { SectionCard { Box(Modifier.padding(12.dp)) { Timeline(w, now, { core.marketById.containsKey(it) }, go) } } }

            // ---- positions ----
            item {
                SectionTitle("Positions", sub = "${w.positions.size} total") {
                    Box {
                        FilterChip(
                            selected = true,
                            onClick = { posMenu = true },
                            label = { Text(POS_SORTS.first { it.key == posSort }.label) },
                            leadingIcon = { Icon(Icons.AutoMirrored.Filled.Sort, null, Modifier.size(FilterChipDefaults.IconSize)) },
                            trailingIcon = { Icon(if (posDesc) Icons.Filled.ArrowDownward else Icons.Filled.ArrowUpward, null, Modifier.size(FilterChipDefaults.IconSize)) },
                        )
                        DropdownMenu(posMenu, { posMenu = false }) {
                            POS_SORTS.forEach { s ->
                                DropdownMenuItem(
                                    text = { Text(s.label) },
                                    leadingIcon = { if (s.key == posSort) Icon(if (posDesc) Icons.Filled.ArrowDownward else Icons.Filled.ArrowUpward, null) },
                                    onClick = {
                                        posMenu = false
                                        posDesc = if (s.key == posSort) !posDesc else s.numeric
                                        posSort = s.key
                                    },
                                )
                            }
                        }
                    }
                }
            }
            val col = POS_SORTS.first { it.key == posSort }
            val sorted = w.positions.sortedWith { x, y -> cmpN(col.v(x), col.v(y), posDesc) }
            items(sorted.take(posLimit), key = { "pos-${it.conditionId}-${it.tokenId}-${it.outcome}" }) { pos ->
                Box(Modifier.padding(bottom = 8.dp)) { PositionCard(pos, core.marketById.containsKey(pos.conditionId)) }
            }
            if (sorted.size > posLimit) item {
                FilledTonalButton(onClick = { posLimit = Int.MAX_VALUE }, Modifier.fillMaxWidth()) { Text("Show all ${sorted.size} positions") }
            }

            // ---- trades ----
            item { SectionTitle("Recent trades", sub = "${trades.size} tracked, newest first") }
            if (trades.isEmpty()) item { EmptyText("No tracked trades yet") }
            items(trades.take(tradeLimit), key = { "tr-${it.key}-${it.ts}" }) { t ->
                TradeItem(
                    t.kind, t.title,
                    listOfNotNull(t.outcome.ifBlank { null }, if (t.price > 0) "@ ${price(t.price)}" else null, if (t.size > 0) "${num(t.size)} sh" else null).joinToString(" "),
                    t.usdc, t.ts,
                    onClick = { sheet = TradeRef(null, "", t.title, t.conditionId, t.tx, "${t.kind} ${t.outcome} @ ${price(t.price)} · ${usd(t.usdc)} · ${date(t.ts, true)}") },
                    absoluteTime = true,
                )
            }
            if (trades.size > tradeLimit) item {
                FilledTonalButton(onClick = { tradeLimit = Int.MAX_VALUE }, Modifier.fillMaxWidth()) { Text("Show all ${trades.size}") }
            }

            // ---- cluster ----
            w.clusterId?.let { cid ->
                val c = core.clusterById[cid]
                item {
                    SectionTitle("Cluster", sub = if (c != null) "${c.members.size} members" else "cluster $cid not found in clusters.json") {
                        if (c != null) androidx.compose.material3.TextButton(onClick = { go("cluster/$cid") }) { Text("Open →") }
                    }
                }
                if (c != null) {
                    val links = c.links.filter { it.a == a || it.b == a }
                    items(c.members.filter { it != a }, key = { "cm-$it" }) { m ->
                        val mr = core.byAddr[m]
                        val ls = links.filter { it.a == m || it.b == m }
                        Card(
                            onClick = { go("wallet/$m") },
                            modifier = Modifier.fillMaxWidth().padding(bottom = 8.dp),
                            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
                        ) {
                            Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                                ScoreTile(mr?.score, mr?.tier, size = 40)
                                Spacer(Modifier.width(12.dp))
                                Column(Modifier.weight(1f)) {
                                    WalletLabel(m, mr?.name ?: "")
                                    if (ls.isEmpty()) Text("indirect", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                    ls.forEach { l ->
                                        Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(top = 4.dp)) {
                                            LinkKindChip(l.kind)
                                            Text(" ${linkInfo(l.kind).label} · ${fixed(l.confidence, 2)}", style = MaterialTheme.typography.bodySmall)
                                        }
                                        Text(l.evidence, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                                    }
                                }
                            }
                        }
                    }
                }
            }
            item {
                Text(
                    "Heuristic leads, not proof. Most flagged patterns have innocent explanations.",
                    Modifier.padding(top = 16.dp),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    fontWeight = FontWeight.Normal,
                )
            }
        }
    }
    sheet?.let { t -> TradeSheet(t, core.marketById.containsKey(t.conditionId)) { sheet = null } }
}
