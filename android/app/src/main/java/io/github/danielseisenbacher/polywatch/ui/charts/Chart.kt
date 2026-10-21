// Shared chart plumbing: a Canvas whose marks register tap targets, and a details
// card under it that replaces the dashboard's hover tooltips.
package io.github.danielseisenbacher.polywatch.ui.charts

import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** A tappable mark: tapping it shows [title]/[body]; [route] adds an "Open" button. */
class Hit(
    val area: Rect,
    val title: String,
    val body: String = "",
    val route: String? = null,
    val routeLabel: String = "Open",
) {
    val center get() = area.center
}

private class HitList {
    var list: List<Hit> = emptyList()
    fun at(p: Offset, slop: Float): Hit? = list
        .filter { it.area.inflate(slop).contains(p) }
        .minByOrNull { (it.center - p).getDistanceSquared() }
}

/** Everything a chart's draw lambda needs besides the DrawScope. */
class ChartCtx(
    val tm: TextMeasurer,
    val ink: Color,
    val muted: Color,
    val hits: MutableList<Hit>,
    val selected: Hit?,
) {
    fun DrawScope.text(
        s: String,
        x: Float,
        y: Float,
        color: Color = muted,
        size: TextUnit = 10.sp,
        align: Float = 0f, // 0 = left, 0.5 = centre, 1 = right
        bold: Boolean = false,
        mono: Boolean = false,
        maxWidth: Float? = null,
        baseline: Boolean = true,
    ): TextLayoutResult {
        val style = TextStyle(
            color = color, fontSize = size,
            fontWeight = if (bold) FontWeight.SemiBold else FontWeight.Normal,
            fontFamily = if (mono) FontFamily.Monospace else FontFamily.Default,
        )
        val r = if (maxWidth != null) {
            tm.measure(s, style, overflow = TextOverflow.Ellipsis, maxLines = 1, constraints = androidx.compose.ui.unit.Constraints(maxWidth = maxWidth.toInt().coerceAtLeast(1)))
        } else tm.measure(s, style, maxLines = 1)
        val top = if (baseline) y - r.firstBaseline else y
        drawText(r, topLeft = Offset(x - r.size.width * align, top))
        return r
    }
}

@Composable
fun InteractiveChart(
    height: Dp,
    description: String,
    hint: String,
    onOpen: (String) -> Unit,
    modifier: Modifier = Modifier,
    draw: DrawScope.(ChartCtx) -> Unit,
) {
    val tm = rememberTextMeasurer()
    val hits = remember { HitList() }
    var selected by remember { mutableStateOf<Hit?>(null) }
    val ink = MaterialTheme.colorScheme.onSurface
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    val slop = with(LocalDensity.current) { 6.dp.toPx() }
    Column(modifier.fillMaxWidth()) {
        Canvas(
            Modifier
                .fillMaxWidth()
                .height(height)
                .semantics { contentDescription = description }
                .pointerInput(Unit) {
                    detectTapGestures { p ->
                        val h = hits.at(p, slop)
                        selected = if (h != null && h.title == selected?.title && h.body == selected?.body) null else h
                    }
                },
        ) {
            val out = mutableListOf<Hit>()
            // Re-resolve the selection against freshly computed marks (sizes change on rotation).
            val sel = selected?.let { s -> hits.list.firstOrNull { it.title == s.title && it.body == s.body } ?: s }
            draw(ChartCtx(tm, ink, muted, out, sel))
            hits.list = out
        }
        AnimatedContent(selected, label = "tip") { s ->
            if (s == null) {
                Text(hint, Modifier.padding(top = 6.dp), style = MaterialTheme.typography.bodySmall, color = muted)
            } else {
                Card(
                    Modifier.fillMaxWidth().padding(top = 6.dp),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.secondaryContainer),
                ) {
                    Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {
                            Text(s.title, style = MaterialTheme.typography.titleSmall)
                            if (s.body.isNotEmpty()) Text(s.body, style = MaterialTheme.typography.bodySmall)
                        }
                        if (s.route != null) FilledTonalButton(onClick = { onOpen(s.route) }) { Text(s.routeLabel) }
                    }
                }
            }
        }
    }
}

fun isSel(c: ChartCtx, h: Hit) = c.selected != null && c.selected.title == h.title && c.selected.body == h.body
