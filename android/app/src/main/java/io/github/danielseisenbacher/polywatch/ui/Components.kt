package io.github.danielseisenbacher.polywatch.ui

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.Toast
import androidx.browser.customtabs.CustomTabsIntent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.ErrorOutline
import androidx.compose.material3.AssistChip
import androidx.compose.material3.AssistChipDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielseisenbacher.polywatch.data.Settings
import io.github.danielseisenbacher.polywatch.data.SignalStrength
import io.github.danielseisenbacher.polywatch.ui.theme.Mono
import io.github.danielseisenbacher.polywatch.ui.theme.NumStyle
import io.github.danielseisenbacher.polywatch.ui.theme.Pw

// ---- links ---------------------------------------------------------------------

object Links {
    fun issue(s: Settings, n: Long) = "https://github.com/${Uri.encode(s.owner)}/${Uri.encode(s.repo)}/issues/$n"
    fun issues(s: Settings) = "https://github.com/${Uri.encode(s.owner)}/${Uri.encode(s.repo)}/issues?q=label%3Ainsider-candidate"
    fun repo(s: Settings) = "https://github.com/${Uri.encode(s.owner)}/${Uri.encode(s.repo)}"
    fun run(s: Settings, id: String) = "${repo(s)}/actions/runs/${Uri.encode(id)}"
    fun profile(a: String) = "https://polymarket.com/profile/${Uri.encode(a)}"
    fun scanAddr(a: String) = "https://polygonscan.com/address/${Uri.encode(a)}"
    fun scanTx(tx: String) = "https://polygonscan.com/tx/${Uri.encode(tx)}"
    fun market(slug: String) = "https://polymarket.com/market/${Uri.encode(slug)}"
    fun x(user: String) = "https://x.com/${Uri.encode(user)}"
}

fun openUrl(ctx: Context, url: String) {
    try {
        CustomTabsIntent.Builder().setShowTitle(true).build().launchUrl(ctx, Uri.parse(url))
    } catch (_: ActivityNotFoundException) {
        try {
            ctx.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
        } catch (_: ActivityNotFoundException) {
            Toast.makeText(ctx, "No app can open $url", Toast.LENGTH_SHORT).show()
        }
    }
}

fun copyText(ctx: Context, label: String, text: String) {
    val cm = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
    cm.setPrimaryClip(ClipData.newPlainText(label, text))
    if (android.os.Build.VERSION.SDK_INT < 33) Toast.makeText(ctx, "Copied", Toast.LENGTH_SHORT).show()
}

/** Text colour that stays legible on [bg]. */
fun onColor(bg: Color): Color = if (bg.luminance() > 0.28f) Color(0xFF111111) else Color.White

// ---- badges & chips --------------------------------------------------------------

/** Leading score tile: number on the tier colour, with the tier letter beneath. */
@Composable
fun ScoreTile(score: Int?, tier: String?, modifier: Modifier = Modifier, size: Int = 44) {
    val c = Pw.colors.tier(tier)
    Box(
        modifier
            .size(size.dp)
            .clip(RoundedCornerShape(12.dp))
            .background(c.copy(alpha = 0.18f))
            .border(1.5.dp, c, RoundedCornerShape(12.dp)),
        contentAlignment = Alignment.Center,
    ) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Text(
                score?.toString() ?: "–",
                style = MaterialTheme.typography.titleMedium.merge(NumStyle),
                fontWeight = FontWeight.Bold,
                lineHeight = 18.sp,
            )
            Text(
                (tier ?: "low").take(1).uppercase(),
                style = MaterialTheme.typography.labelSmall,
                fontSize = 9.sp,
                lineHeight = 10.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

/** Inline "72 ▮▮▮ H" score, like the dashboard's score cell. */
@Composable
fun ScoreInline(score: Int?, tier: String?) {
    val c = Pw.colors.tier(tier)
    val s = (score ?: 0).coerceIn(0, 100)
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(score?.toString() ?: "–", style = MaterialTheme.typography.titleMedium.merge(NumStyle), fontWeight = FontWeight.Bold)
        Box(Modifier.width(48.dp).height(6.dp).clip(RoundedCornerShape(3.dp)).background(MaterialTheme.colorScheme.surfaceVariant)) {
            Box(Modifier.fillMaxWidth(s / 100f).height(6.dp).clip(RoundedCornerShape(3.dp)).background(c))
        }
        TierBadge(tier)
    }
}

@Composable
fun TierBadge(tier: String?) {
    val c = Pw.colors.tier(tier)
    Box(Modifier.clip(RoundedCornerShape(4.dp)).background(c).padding(horizontal = 5.dp, vertical = 1.dp)) {
        Text((tier ?: "low").take(1).uppercase(), color = onColor(c), fontSize = 10.sp, fontWeight = FontWeight.Bold, fontFamily = Mono)
    }
}

/** Compact signal chip coloured like the dashboard (`.chip.sN`), opacity ∝ strength. */
@Composable
fun SignalChip(kind: String, strength: Double = 1.0, label: String? = null) {
    val i = sigInfo(kind)
    val base = Pw.colors.sig(i.c)
    val alpha = (0.35 + 0.65 * strength.coerceIn(0.0, 1.0)).toFloat()
    val bg = base.copy(alpha = alpha)
    Box(Modifier.clip(RoundedCornerShape(6.dp)).background(bg).padding(horizontal = 6.dp, vertical = 2.dp)) {
        Text(label ?: i.abbr, color = onColor(base), fontSize = 11.sp, fontWeight = FontWeight.SemiBold, fontFamily = Mono, maxLines = 1)
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SignalChips(sigs: List<SignalStrength>, modifier: Modifier = Modifier) {
    FlowRow(modifier, horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        sigs.forEach { SignalChip(it.kind, it.strength) }
    }
}

@Composable
fun LinkKindChip(kind: String) {
    val i = linkInfo(kind)
    val c = Pw.colors.sig(i.c)
    Box(Modifier.clip(RoundedCornerShape(6.dp)).border(1.dp, c, RoundedCornerShape(6.dp)).padding(horizontal = 6.dp, vertical = 1.dp)) {
        Text(i.abbr, color = c, fontSize = 11.sp, fontWeight = FontWeight.SemiBold, fontFamily = Mono)
    }
}

@Composable
fun KindBadge(kind: String) {
    val i = kindInfo(kind)
    val c = when (kind) {
        "buy" -> Pw.colors.buy
        "sell" -> Pw.colors.sell
        "redeem" -> Pw.colors.redeem
        else -> MaterialTheme.colorScheme.onSurfaceVariant
    }
    Text("${i.sym} ${i.short}", color = c, fontFamily = Mono, fontWeight = FontWeight.Bold, fontSize = 12.sp, maxLines = 1)
}

@Composable
fun NewBadge() {
    Box(Modifier.clip(RoundedCornerShape(4.dp)).background(MaterialTheme.colorScheme.primary).padding(horizontal = 5.dp, vertical = 1.dp)) {
        Text("new", color = MaterialTheme.colorScheme.onPrimary, fontSize = 10.sp, fontWeight = FontWeight.Bold)
    }
}

@Composable
fun pnlColor(n: Double?): Color = when (pnlSign(n)) {
    1 -> Pw.colors.pos
    -1 -> Pw.colors.neg
    else -> MaterialTheme.colorScheme.onSurface
}

@Composable
fun LinkChip(text: String, url: String) {
    val ctx = LocalContext.current
    AssistChip(
        onClick = { openUrl(ctx, url) },
        label = { Text(text, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        trailingIcon = { Icon(Icons.AutoMirrored.Filled.OpenInNew, null, Modifier.size(AssistChipDefaults.IconSize)) },
    )
}

// ---- layout pieces ---------------------------------------------------------------

data class Kpi(val label: String, val value: @Composable () -> Unit, val sub: String = "")

fun kpi(label: String, value: String, sub: String = "", color: Color? = null) =
    Kpi(label, { Text(value, style = MaterialTheme.typography.titleLarge.merge(NumStyle), color = color ?: Color.Unspecified, maxLines = 1, overflow = TextOverflow.Ellipsis) }, sub)

@Composable
fun KpiGrid(items: List<Kpi>, modifier: Modifier = Modifier) {
    BoxWithConstraints(modifier.fillMaxWidth()) {
        val cols = if (maxWidth < 380.dp) 2 else if (maxWidth < 600.dp) 3 else 4
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items.chunked(cols).forEach { row ->
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    row.forEach { k ->
                        Card(
                            Modifier.weight(1f),
                            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerHigh),
                        ) {
                            Column(Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
                                Text(k.label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
                                Spacer(Modifier.height(2.dp))
                                k.value()
                                if (k.sub.isNotEmpty()) Text(k.sub, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2)
                            }
                        }
                    }
                    repeat(cols - row.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
}

@Composable
fun SectionTitle(title: String, modifier: Modifier = Modifier, sub: String? = null, trailing: @Composable (() -> Unit)? = null) {
    Row(modifier.fillMaxWidth().padding(top = 20.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            if (sub != null) Text(sub, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        trailing?.invoke()
    }
}

@Composable
fun ErrorCard(msg: String, modifier: Modifier = Modifier, onRetry: (() -> Unit)? = null) {
    Card(
        modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(
            containerColor = MaterialTheme.colorScheme.errorContainer,
            contentColor = MaterialTheme.colorScheme.onErrorContainer,
        ),
    ) {
        Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            Icon(Icons.Filled.ErrorOutline, null)
            Spacer(Modifier.width(10.dp))
            Text(msg, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
            if (onRetry != null) TextButton(onClick = onRetry) { Text("Retry") }
        }
    }
}

@Composable
fun LoadingBox(what: String, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize().padding(32.dp), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator()
            Spacer(Modifier.height(12.dp))
            Text("Loading $what…", color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

@Composable
fun EmptyText(text: String, modifier: Modifier = Modifier) {
    Text(text, modifier.padding(vertical = 12.dp), color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodyMedium)
}

/** "name 0x1234…abcd" with the address in monospace. */
@Composable
fun WalletLabel(addr: String, name: String, modifier: Modifier = Modifier) {
    Column(modifier) {
        if (name.isNotBlank()) Text(name, style = MaterialTheme.typography.titleSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
        Text(
            shortAddr(addr),
            style = if (name.isBlank()) MaterialTheme.typography.titleSmall else MaterialTheme.typography.bodySmall,
            fontFamily = Mono,
            color = if (name.isBlank()) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
        )
    }
}

/** A plain rounded container for list sections. */
@Composable
fun SectionCard(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Card(
        modifier.fillMaxWidth(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) { content() }
}
