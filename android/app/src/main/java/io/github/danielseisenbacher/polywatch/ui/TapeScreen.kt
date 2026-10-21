package io.github.danielseisenbacher.polywatch.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.AccountBalanceWallet
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.ContentCopy
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.automirrored.filled.ShowChart
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.danielseisenbacher.polywatch.data.AppViewModel
import io.github.danielseisenbacher.polywatch.data.CoreFile
import io.github.danielseisenbacher.polywatch.ui.theme.Mono

/** What the trade sheet needs to offer its actions. */
data class TradeRef(
    val addr: String?,
    val name: String,
    val title: String,
    val conditionId: String,
    val tx: String,
    val summary: String,
)

/** Bottom sheet with the links a trade row has on the dashboard (wallet, market, tx). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TradeSheet(t: TradeRef, hasMarket: Boolean, onDismiss: () -> Unit) {
    val go = LocalNav.current
    val ctx = LocalContext.current
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.padding(bottom = 16.dp).navigationBarsPadding()) {
            Text(t.title.ifBlank { "Trade" }, Modifier.padding(horizontal = 24.dp), style = MaterialTheme.typography.titleMedium)
            Text(t.summary, Modifier.padding(horizontal = 24.dp, vertical = 4.dp), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            val colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)
            if (t.addr != null) ListItem(
                headlineContent = { Text("Open wallet") },
                supportingContent = { Text(t.name.ifBlank { shortAddr(t.addr) }) },
                leadingContent = { Icon(Icons.Filled.AccountBalanceWallet, null) },
                colors = colors,
                modifier = Modifier.clickable { onDismiss(); go("wallet/${t.addr}") },
            )
            if (hasMarket) ListItem(
                headlineContent = { Text("Open market") },
                supportingContent = { Text("flagged wallets in this market") },
                leadingContent = { Icon(Icons.AutoMirrored.Filled.ShowChart, null) },
                colors = colors,
                modifier = Modifier.clickable { onDismiss(); go("market/${t.conditionId}") },
            )
            if (t.tx.isNotEmpty()) {
                ListItem(
                    headlineContent = { Text("Transaction on Polygonscan") },
                    supportingContent = { Text(shortAddr(t.tx), fontFamily = Mono) },
                    leadingContent = { Icon(Icons.AutoMirrored.Filled.OpenInNew, null) },
                    trailingContent = {
                        IconButton(onClick = { copyText(ctx, "transaction", t.tx) }) { Icon(Icons.Filled.ContentCopy, "Copy transaction hash") }
                    },
                    colors = colors,
                    modifier = Modifier.clickable { go(Links.scanTx(t.tx)) },
                )
            }
        }
    }
}

@Composable
fun TapeScreen(vm: AppViewModel) {
    val core by rememberCore(vm)
    var q by rememberSaveable { mutableStateOf("") }
    var kind by rememberSaveable { mutableStateOf("") }
    var big by rememberSaveable { mutableStateOf(false) }
    var limit by rememberSaveable { mutableIntStateOf(500) }
    var sheet by remember { mutableStateOf<TradeRef?>(null) }

    val kinds = remember(core.tape) { core.tape.orEmpty().map { it.kind }.distinct().sorted() }
    val rows = remember(core.tapeSorted, q, kind, big) {
        val ql = q.trim().lowercase()
        core.tapeSorted.filter {
            (kind.isEmpty() || it.kind == kind) && (!big || it.usdc >= 500) &&
                (ql.isEmpty() || ql in it.addr || ql in it.name.lowercase() || ql in it.title.lowercase())
        }
    }

    PwScaffold(
        title = "Live tape",
        subtitle = "watchlist trades, last 48 h · ≥ \$500 highlighted",
        refreshing = core.loading,
        onRefresh = { vm.refresh(listOf(CoreFile.TAPE, CoreFile.META, CoreFile.MARKETS)) },
    ) { pad ->
        LazyColumn(
            contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = 4.dp, bottom = pad.calculateBottomPadding() + 16.dp),
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            val tapeErr = core.err[CoreFile.TAPE.path]
            if (core.tape == null) {
                item { if (tapeErr != null) ErrorCard("activity_recent.json: $tapeErr", onRetry = { vm.refresh() }) else LoadingBox("tape") }
                return@LazyColumn
            }
            item {
                OutlinedTextField(
                    q, { q = it },
                    Modifier.fillMaxWidth().padding(horizontal = 4.dp),
                    placeholder = { Text("Filter wallet, name or market") },
                    leadingIcon = { Icon(Icons.Filled.Search, null) },
                    trailingIcon = { if (q.isNotEmpty()) IconButton(onClick = { q = "" }) { Icon(Icons.Filled.Clear, "Clear filter") } },
                    singleLine = true,
                    shape = RoundedCornerShape(28.dp),
                )
            }
            item {
                Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(big, { big = !big }, label = { Text("≥ \$500 only") })
                    FilterChip(kind.isEmpty(), { kind = "" }, label = { Text("All kinds") })
                    kinds.forEach { k ->
                        FilterChip(kind == k, { kind = if (kind == k) "" else k }, label = { Text("${kindInfo(k).sym} $k") })
                    }
                }
            }
            item {
                Text(
                    "${num(rows.size)} trades · ${usd(rows.sumOf { it.usdc })}",
                    Modifier.padding(horizontal = 4.dp, vertical = 4.dp),
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            if (rows.isEmpty()) item { EmptyText("No trades match", Modifier.padding(horizontal = 4.dp)) }
            itemsIndexed(rows.take(limit), key = { i, t -> "$i${t.tx}${t.ts}" }) { _, t ->
                TradeItem(
                    t.kind, t.title,
                    listOfNotNull(t.name.ifBlank { shortAddr(t.addr) }, "score ${t.score}", if (t.price > 0) "${t.outcome} @ ${price(t.price)}".trim() else null).joinToString(" · "),
                    t.usdc, t.ts,
                    onClick = {
                        sheet = TradeRef(t.addr, t.name, t.title, t.conditionId, t.tx, "${t.kind} ${t.outcome} @ ${price(t.price)} · ${usd(t.usdc)} · ${date(t.ts, true)}")
                    },
                )
            }
            if (rows.size > limit) item {
                FilledTonalButton(onClick = { limit = Int.MAX_VALUE }, Modifier.fillMaxWidth().padding(8.dp)) {
                    Text("Show all ${num(rows.size)}", fontWeight = FontWeight.SemiBold)
                }
            }
        }
    }
    sheet?.let { t -> TradeSheet(t, core.marketById.containsKey(t.conditionId)) { sheet = null } }
}
