package io.github.danielseisenbacher.polywatch.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Hub
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.compositeOver
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.danielseisenbacher.polywatch.data.CandidateRow
import io.github.danielseisenbacher.polywatch.data.Position
import io.github.danielseisenbacher.polywatch.ui.theme.Mono
import io.github.danielseisenbacher.polywatch.ui.theme.NumStyle
import io.github.danielseisenbacher.polywatch.ui.theme.Pw

@Composable
fun CandidateCard(r: CandidateRow, isNew: Boolean, onIssue: (Long) -> Unit) {
    val go = LocalNav.current
    val now = LocalNow.current
    val wins = r.winRate?.let { Math.round(it * r.nResolved * 2) / 2.0 }
    Card(
        onClick = { go("wallet/${r.addr}") },
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = if (isNew) Pw.colors.newBg.compositeOver(MaterialTheme.colorScheme.surfaceContainerLow) else MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.Top) {
            ScoreTile(r.score, r.tier)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    WalletLabel(r.addr, r.name, Modifier.weight(1f, fill = false))
                    if (isNew) NewBadge()
                }
                Spacer(Modifier.height(2.dp))
                Text(
                    listOfNotNull(
                        r.ageD?.let { "${num(it)} d old" },
                        "${num(r.nMarkets)} mkts",
                        wins?.let { "won ${frac(it)}/${frac(r.nResolved)}" },
                        if (r.z.isFinite() && r.nResolved > 0) "z ${fixed(r.z)}" else null,
                    ).joinToString(" · "),
                    style = MaterialTheme.typography.bodySmall.merge(NumStyle),
                    color = if (r.z >= 2) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant,
                )
                Spacer(Modifier.height(6.dp))
                SignalChips(r.signals)
            }
            Spacer(Modifier.width(8.dp))
            Column(horizontalAlignment = Alignment.End) {
                Text(usd(r.pnl, sign = true), color = pnlColor(r.pnl), style = MaterialTheme.typography.titleSmall.merge(NumStyle), fontWeight = FontWeight.SemiBold)
                Text("${usd(r.value)} value", style = MaterialTheme.typography.bodySmall.merge(NumStyle), color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(relShort(r.lastTradeTs, now).let { if (it == "–") "no trades" else "last $it" }, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Spacer(Modifier.height(4.dp))
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    if (r.watched) Icon(Icons.Filled.Visibility, "Watched by tracker", Modifier.size(16.dp), tint = MaterialTheme.colorScheme.primary)
                    r.clusterId?.let { cid ->
                        Row(
                            Modifier.clip(RoundedCornerShape(6.dp)).clickable { go("cluster/$cid") }.padding(2.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(Icons.Filled.Hub, "Cluster", Modifier.size(14.dp), tint = MaterialTheme.colorScheme.tertiary)
                            Text(cid.removePrefix("c-").take(4), style = MaterialTheme.typography.labelSmall, fontFamily = Mono, color = MaterialTheme.colorScheme.tertiary)
                        }
                    }
                    r.issue?.let { n ->
                        Text(
                            "#$n",
                            Modifier.clip(RoundedCornerShape(6.dp)).clickable { onIssue(n) }.padding(horizontal = 3.dp, vertical = 2.dp),
                            style = MaterialTheme.typography.labelMedium,
                            color = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
            }
        }
    }
}

/** One trade: kind, market, who/what, USDC and age. Used by the tape and wallet views. */
@Composable
fun TradeItem(
    kind: String,
    title: String,
    detail: String,
    usdc: Double,
    ts: Long,
    onClick: () -> Unit,
    absoluteTime: Boolean = false,
) {
    val now = LocalNow.current
    val big = usdc >= 500
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(12.dp))
            .background(if (big) Pw.colors.bigBg else Color.Transparent)
            .clickable(onClick = onClick)
            .padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(36.dp).clip(CircleShape).background(MaterialTheme.colorScheme.surfaceContainerHighest), contentAlignment = Alignment.Center) {
            KindBadge(kind)
        }
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(title.ifBlank { "–" }, style = MaterialTheme.typography.bodyMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
            Text(detail, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        Spacer(Modifier.width(8.dp))
        Column(horizontalAlignment = Alignment.End) {
            Text(usd(usdc), style = MaterialTheme.typography.bodyMedium.merge(NumStyle), fontWeight = if (big) FontWeight.Bold else FontWeight.Normal)
            Text(
                if (absoluteTime) rel(ts, now) else relShort(ts, now),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

private val STATUS = mapOf("OPEN" to "open", "CLOSED" to "closed", "REDEEMABLE" to "redeemable", "REDEEMABLE_LOST" to "lost")

@Composable
fun PositionCard(p: Position, hasMarket: Boolean) {
    val go = LocalNav.current
    val won = p.won
    val st = when (won) { true -> "✓ won"; false -> "✗ lost"; else -> STATUS[p.status] ?: p.status }
    val stColor = when (won) { true -> Pw.colors.pos; false -> Pw.colors.neg; else -> MaterialTheme.colorScheme.onSurfaceVariant }
    val d = p.currentPrice - p.avgPrice
    val (arrow, arrowColor) = when {
        d > 0.005 -> "▲" to Pw.colors.pos
        d < -0.005 -> "▼" to Pw.colors.neg
        else -> "▶" to MaterialTheme.colorScheme.onSurfaceVariant
    }
    val target = when {
        hasMarket -> "market/${p.conditionId}"
        p.slug.isNotEmpty() -> Links.market(p.slug)
        else -> null
    }
    Card(
        onClick = { target?.let(go) },
        enabled = target != null,
        modifier = Modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.surfaceContainerLow,
            disabledContainerColor = MaterialTheme.colorScheme.surfaceContainerLow,
            disabledContentColor = MaterialTheme.colorScheme.onSurface,
        ),
    ) {
        Column(Modifier.padding(12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(st, color = stColor, style = MaterialTheme.typography.labelMedium, fontWeight = FontWeight.Bold)
                Text(" · ${p.outcome}", style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f), maxLines = 1)
                Text(usd(p.totalPnl, sign = true), color = pnlColor(p.totalPnl), style = MaterialTheme.typography.titleSmall.merge(NumStyle), fontWeight = FontWeight.SemiBold)
            }
            Text(p.title.ifBlank { p.conditionId }, style = MaterialTheme.typography.bodyMedium, maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.padding(vertical = 4.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp), verticalAlignment = Alignment.CenterVertically) {
                Row {
                    Text("${price(p.avgPrice)} ", style = MaterialTheme.typography.bodySmall.merge(NumStyle))
                    Text(arrow, style = MaterialTheme.typography.bodySmall, color = arrowColor)
                    Text(" ${price(p.currentPrice)}", style = MaterialTheme.typography.bodySmall.merge(NumStyle))
                }
                Text("${num(p.totalSize)} sh", style = MaterialTheme.typography.bodySmall.merge(NumStyle), color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text("cost ${usd(p.cost)}", style = MaterialTheme.typography.bodySmall.merge(NumStyle), color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text("val ${usd(p.currentValue)}", style = MaterialTheme.typography.bodySmall.merge(NumStyle), color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Text(
                "entry ${date(p.firstEntryAt)} · end ${date(p.endDate)}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
