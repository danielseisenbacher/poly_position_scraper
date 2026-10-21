// The dashboard's charts (site/js/charts.js), redrawn for touch screens.
package io.github.danielseisenbacher.polywatch.ui.charts

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.danielseisenbacher.polywatch.data.Cluster
import io.github.danielseisenbacher.polywatch.data.CandidateRow
import io.github.danielseisenbacher.polywatch.data.MarketView
import io.github.danielseisenbacher.polywatch.data.Position
import io.github.danielseisenbacher.polywatch.data.TrackedTrade
import io.github.danielseisenbacher.polywatch.data.Wallet
import io.github.danielseisenbacher.polywatch.ui.date
import io.github.danielseisenbacher.polywatch.ui.fixed
import io.github.danielseisenbacher.polywatch.ui.kindInfo
import io.github.danielseisenbacher.polywatch.ui.linkInfo
import io.github.danielseisenbacher.polywatch.ui.num
import io.github.danielseisenbacher.polywatch.ui.price
import io.github.danielseisenbacher.polywatch.ui.shortAddr
import io.github.danielseisenbacher.polywatch.ui.theme.Pw
import io.github.danielseisenbacher.polywatch.ui.theme.PwColors
import io.github.danielseisenbacher.polywatch.ui.tierOf
import io.github.danielseisenbacher.polywatch.ui.usd
import java.time.Instant
import java.time.ZoneOffset
import kotlin.math.PI
import kotlin.math.ceil
import kotlin.math.cos
import kotlin.math.hypot
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sin
import kotlin.math.sqrt

private val dash = PathEffect.dashPathEffect(floatArrayOf(6f, 4f))
private val dot = PathEffect.dashPathEffect(floatArrayOf(2f, 3f))

// ---- score histogram --------------------------------------------------------------

/** Score histogram (10 buckets of 10) on a log scale, with tier thresholds. */
@Composable
fun Histogram(h: List<Long>, onOpen: (String) -> Unit) {
    val pw = Pw.colors
    InteractiveChart(
        150.dp,
        "Score histogram, log scale: " + h.mapIndexed { i, v -> "${i * 10}s $v" }.joinToString(", "),
        "Tap a bar for its count",
        onOpen,
    ) { c ->
        val L = 4.dp.toPx(); val B = 18.dp.toPx(); val T = 18.dp.toPx()
        val W = size.width; val H = size.height
        val bw = (W - L * 2) / h.size
        val mx = log10(max(h.maxOrNull() ?: 1, 1).toDouble() + 1)
        fun y(v: Long) = (H - B - (log10(v + 1.0) / mx) * (H - B - T)).toFloat()
        drawLine(pw.axis, Offset(L, H - B), Offset(W - L, H - B), 1.dp.toPx())
        h.forEachIndexed { i, v ->
            val x = L + i * bw
            val yy = y(v)
            val col = when {
                i >= 7 -> pw.tierHigh
                i >= 5 -> pw.tierMedium
                i == 4 -> pw.tierMedium.copy(alpha = 0.55f)
                else -> pw.tierLow
            }
            val hit = Hit(Rect(x, T, x + bw, H - B), "Score ${i * 10}–${if (i == 9) 100 else i * 10 + 9}", "${num(v)} wallets")
            c.hits += hit
            if (isSel(c, hit)) drawRect(c.ink.copy(alpha = 0.08f), Offset(x, T), Size(bw, H - B - T))
            drawRoundRect(col, Offset(x + 2, yy), Size(bw - 4, H - B - yy), CornerRadius(3.dp.toPx()))
            with(c) {
                if (v > 0) text(num(v), x + bw / 2, yy - 3.dp.toPx(), c.ink, 9.sp, 0.5f, mono = true)
                text("${i * 10}", x + bw / 2, H - 4.dp.toPx(), align = 0.5f, size = 9.sp)
            }
        }
        for ((s, lbl) in listOf(4.5f to "M≥45", 7f to "H≥70")) {
            val x = L + s * bw
            drawLine(c.muted, Offset(x, T - 6.dp.toPx()), Offset(x, H - B), 1.dp.toPx(), pathEffect = dash)
            with(c) { text(lbl, x + 3.dp.toPx(), T - 6.dp.toPx(), size = 9.sp) }
        }
    }
}

// ---- time axis ----------------------------------------------------------------------

private const val H1 = 3600L
private const val D1 = 86400L
private val STEPS = longArrayOf(H1, 3 * H1, 6 * H1, 12 * H1, D1, 2 * D1, 7 * D1, 14 * D1, 30 * D1, 91 * D1, 182 * D1, 365 * D1)
private val MON = arrayOf("Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec")

private fun timeTicks(t0: Double, t1: Double, maxTicks: Int): List<Pair<Double, String>> {
    val span = t1 - t0
    val step = STEPS.firstOrNull { span / it <= maxTicks } ?: (365 * D1)
    val out = mutableListOf<Pair<Double, String>>()
    var t = ceil(t0 / step) * step
    while (t <= t1) {
        val d = Instant.ofEpochSecond(t.toLong()).atZone(ZoneOffset.UTC)
        val lbl = when {
            step < D1 -> "%02d:00".format(d.hour)
            step >= 91 * D1 -> "${MON[d.monthValue - 1]} ${d.year % 100}"
            else -> "${MON[d.monthValue - 1]} ${d.dayOfMonth}"
        }
        out += t to lbl
        t += step
    }
    return out
}

private fun Path.tri(x: Float, y: Float, r: Float, up: Boolean) {
    val a = r * 0.95f; val b = r * 0.8f
    if (up) { moveTo(x, y - r); lineTo(x + a, y + b); lineTo(x - a, y + b) } else { moveTo(x, y + r); lineTo(x + a, y - b); lineTo(x - a, y - b) }
    close()
}
private fun Path.diamond(x: Float, y: Float, r: Float) {
    moveTo(x, y - r); lineTo(x + r, y); lineTo(x, y + r); lineTo(x - r, y); close()
}

private fun kindColor(pw: PwColors, kind: String, muted: Color) = when (kind) {
    "buy" -> pw.buy
    "sell" -> pw.sell
    "redeem" -> pw.redeem
    else -> muted
}

// ---- wallet trade timeline ------------------------------------------------------------

private class Lane(val cid: String, val title: String, val pos: Position?)

private class TimelineModel(
    val trades: List<TrackedTrade>, // ascending by ts
    val lanes: List<Lane>,
    val laneOf: (String) -> Int,
    val posBy: Map<String, Position>,
    val t0: Double,
    val t1: Double,
    val refU: Double,
    val nMarkets: Int,
    val cin: Double,
    val cout: Double,
)

private fun timelineModel(w: Wallet, now: Long): TimelineModel? {
    val tk = setOf("buy", "sell", "redeem")
    val trades = w.trades.filter { it.conditionId.isNotEmpty() && it.kind in tk }.sortedBy { it.ts }
    val posBy = mutableMapOf<String, Position>()
    for (p in w.positions) {
        val cur = posBy[p.conditionId]
        if (cur == null || p.cost > cur.cost) posBy[p.conditionId] = p
    }
    val vol = linkedMapOf<String, Double>()
    val titles = mutableMapOf<String, String>()
    for (t in trades) {
        vol[t.conditionId] = (vol[t.conditionId] ?: 0.0) + t.usdc
        titles[t.conditionId] = t.title
    }
    for (p in w.positions) {
        if (p.conditionId !in vol && p.firstEntryAt != null) vol[p.conditionId] = p.cost
        titles.putIfAbsent(p.conditionId, p.title)
    }
    if (vol.isEmpty()) return null
    val ranked = vol.entries.sortedByDescending { it.value }.map { it.key }
    val top = ranked.take(8)
    val rest = ranked.drop(8).toSet()
    val lanes = top.map { Lane(it, titles[it] ?: it, posBy[it]) }.toMutableList()
    if (rest.isNotEmpty()) lanes += Lane("__other", "other (${rest.size} markets)", null)
    val laneOf = { cid: String -> if (cid in rest) lanes.lastIndex else top.indexOf(cid) }

    val ts = trades.map { it.ts.toDouble() }.toMutableList()
    for (cid in ranked) posBy[cid]?.firstEntryAt?.let { ts += it.toDouble() }
    var t0 = ts.min()
    var t1 = ts.max()
    for (l in lanes) l.pos?.endDate?.let { e -> if (e <= now && e > t0) t1 = max(t1, e.toDouble()) }
    if (t1 - t0 < H1) { t0 -= H1; t1 += H1 }
    val pad = (t1 - t0) * 0.02
    t0 -= pad; t1 += pad
    val us = trades.map { it.usdc }.sorted()
    val refU = max(us.getOrNull((us.size * 0.9).toInt()) ?: 0.0, 1.0)
    return TimelineModel(
        trades, lanes, laneOf, posBy, t0, t1, refU, vol.size,
        trades.filter { it.kind == "buy" }.sumOf { it.usdc },
        trades.filter { it.kind != "buy" }.sumOf { it.usdc },
    )
}

/**
 * Trade timeline for one wallet: cumulative USDC in/out over one lane per market
 * (top 8 by volume + "other"). Lane y = price 0–1.
 */
@Composable
fun Timeline(w: Wallet, now: Long, hasMarket: (String) -> Boolean, onOpen: (String) -> Unit) {
    val m = remember(w, now / 3600) { timelineModel(w, now) }
    val pw = Pw.colors
    if (m == null) {
        Text("No trades or position entries to plot", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    Column {
        Row(horizontalArrangement = Arrangement.spacedBy(16.dp), modifier = Modifier.padding(bottom = 4.dp)) {
            Text("━ in ${usd(m.cin)}", color = pw.buy, style = MaterialTheme.typography.labelMedium, fontFamily = FontFamily.Monospace)
            Text("╍ out ${usd(m.cout)}", color = pw.sell, style = MaterialTheme.typography.labelMedium, fontFamily = FontFamily.Monospace)
            Text("cumulative USDC", color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.labelMedium)
        }
        val spkDp = 48.dp; val gapDp = 8.dp; val lhDp = 46.dp; val axDp = 22.dp
        InteractiveChart(
            spkDp + gapDp + lhDp * m.lanes.size + axDp,
            "Trade timeline: ${m.trades.size} trades across ${m.nMarkets} markets from ${date(m.t0.toLong())} to ${date(m.t1.toLong())}.",
            "▲ buy · ▼ sell · ◆ redeem · size ∝ √USDC · y = price · ┊ market end · tap a mark or lane title",
            onOpen,
        ) { c -> drawTimeline(c, m, now, pw, hasMarket) }
    }
}

private fun DrawScope.drawTimeline(c: ChartCtx, m: TimelineModel, now: Long, pw: PwColors, hasMarket: (String) -> Boolean) {
    val L = 4.dp.toPx(); val R = 28.dp.toPx(); val SPK = 48.dp.toPx(); val GAP = 8.dp.toPx()
    val LH = 46.dp.toPx(); val LBL = 15.dp.toPx()
    val top0 = SPK + GAP
    val W = size.width
    val pwid = max(80f, W - L - R)
    fun x(t: Double) = (L + (t - m.t0) / (m.t1 - m.t0) * pwid).toFloat()
    fun yIn(i: Int, p: Double) = top0 + i * LH + LBL + (1 - p.coerceIn(0.0, 1.0)).toFloat() * (LH - LBL - 5.dp.toPx())
    val rMax = 9.dp.toPx()
    fun rad(u: Double) = min(rMax, 2.5.dp.toPx() + (sqrt(max(0.0, u) / m.refU)).toFloat() * (rMax - 4.dp.toPx()))

    // lanes
    m.lanes.forEachIndexed { i, l ->
        val y0 = top0 + i * LH
        val won = l.pos?.won
        val bg = when (won) { true -> pw.wonBg; false -> pw.lostBg; else -> pw.laneBg }
        drawRect(bg, Offset(L, y0 + 1), Size(pwid, LH - 2))
        val ymid = yIn(i, 0.5)
        drawLine(pw.axis.copy(alpha = 0.5f), Offset(L, ymid), Offset(L + pwid, ymid), 0.5.dp.toPx())
        val res = when (won) { true -> "✓ won"; false -> "✗ lost"; else -> l.pos?.let { if (it.status == "OPEN") "open" else it.status.lowercase() } ?: "" }
        val resW = with(c) { if (res.isNotEmpty()) text("${l.pos?.outcome ?: ""} · $res", L + pwid - 4.dp.toPx(), y0 + 12.dp.toPx(), when (won) { true -> pw.pos; false -> pw.neg; else -> c.muted }, 10.sp, 1f, maxWidth = pwid * 0.4f).size.width.toFloat() else 0f }
        with(c) { text(l.title, L + 4.dp.toPx(), y0 + 12.dp.toPx(), c.ink, 10.sp, 0f, bold = true, maxWidth = pwid - resW - 12.dp.toPx()) }
        val body = l.pos?.let { p -> "${p.outcome} · $res · avg ${price(p.avgPrice)} → ${price(p.currentPrice)}\ncost ${usd(p.cost)} · PnL ${usd(p.totalPnl, sign = true)}" } ?: ""
        val route = if (l.cid != "__other" && hasMarket(l.cid)) "market/${l.cid}" else l.pos?.slug?.takeIf { it.isNotEmpty() }?.let { "https://polymarket.com/market/$it" }
        c.hits += Hit(Rect(L, y0, L + pwid, y0 + LBL), l.title, body, route, if (route?.startsWith("http") == true) "Polymarket" else "Market")
        l.pos?.endDate?.let { e ->
            if (e >= m.t0 && e <= m.t1) {
                val xe = x(e.toDouble())
                drawLine(c.muted, Offset(xe, y0 + LBL - 2), Offset(xe, y0 + LH - 1), 1.dp.toPx(), pathEffect = dot)
                c.hits += Hit(Rect(xe - 4.dp.toPx(), y0 + LBL, xe + 4.dp.toPx(), y0 + LH), "Market end ${date(e)}", l.title)
            }
        }
    }
    with(c) {
        text("p=1", L + pwid + 3.dp.toPx(), yIn(0, 1.0) + 3.dp.toPx(), size = 8.sp)
        text("p=0", L + pwid + 3.dp.toPx(), yIn(0, 0.0) + 3.dp.toPx(), size = 8.sp)
    }

    // entries without recorded trades: hollow ring at avg price
    for ((cid, p) in m.posBy) {
        val fe = p.firstEntryAt ?: continue
        if (m.trades.any { it.conditionId == cid }) continue
        val i = m.laneOf(cid)
        if (i < 0) continue
        val o = Offset(x(fe.toDouble()), yIn(i, p.avgPrice))
        drawCircle(c.muted, 4.dp.toPx(), o, style = Stroke(1.5.dp.toPx()))
        c.hits += Hit(Rect(o, 8.dp.toPx()), "First entry ${date(fe, true)}", "${p.title}\n${p.outcome} avg ${price(p.avgPrice)} · cost ${usd(p.cost)} · no tracked trades")
    }
    // trades, oldest first so newer ones draw on top
    for (t in m.trades) {
        val i = m.laneOf(t.conditionId)
        if (i < 0) continue
        val cx = x(t.ts.toDouble()); val cy = yIn(i, t.price); val r = rad(t.usdc)
        val path = Path().apply { if (t.kind == "redeem") diamond(cx, cy, r) else tri(cx, cy, r, t.kind == "buy") }
        val hit = Hit(
            Rect(Offset(cx, cy), r + 4.dp.toPx()),
            "${kindInfo(t.kind).sym} ${t.kind} · ${usd(t.usdc)}",
            "${date(t.ts, true)}\n${t.title}\n${t.outcome} @ ${price(t.price)} · ${num(t.size)} sh",
            if (hasMarket(t.conditionId)) "market/${t.conditionId}" else null,
            "Market",
        )
        c.hits += hit
        drawPath(path, kindColor(pw, t.kind, c.muted))
        if (isSel(c, hit)) drawCircle(c.ink, r + 3.dp.toPx(), Offset(cx, cy), style = Stroke(1.5.dp.toPx()))
    }

    // cumulative USDC in/out sparkline (one y scale: both are USDC)
    val ym = max(max(m.cin, m.cout), 1.0)
    fun ys(v: Double) = (SPK - 3.dp.toPx() - (v / ym) * (SPK - 8.dp.toPx())).toFloat()
    fun step(buy: Boolean): Path {
        val p = Path()
        var acc = 0.0
        p.moveTo(x(m.t0), ys(0.0))
        for (t in m.trades) {
            if ((t.kind == "buy") != buy) continue
            acc += t.usdc
            p.lineTo(x(t.ts.toDouble()), ys(acc - t.usdc))
            p.lineTo(x(t.ts.toDouble()), ys(acc))
        }
        p.lineTo(x(m.t1), ys(acc))
        return p
    }
    drawLine(pw.axis, Offset(L, SPK - 3.dp.toPx()), Offset(L + pwid, SPK - 3.dp.toPx()), 1.dp.toPx())
    drawPath(step(true), pw.buy, style = Stroke(1.5.dp.toPx()))
    drawPath(step(false), pw.sell, style = Stroke(1.5.dp.toPx(), pathEffect = dash))

    // time axis
    val yA = top0 + m.lanes.size * LH
    drawLine(pw.axis, Offset(L, yA), Offset(L + pwid, yA), 1.dp.toPx())
    for ((t, lbl) in timeTicks(m.t0, m.t1, max(3, (pwid / 64.dp.toPx()).toInt()))) {
        drawLine(pw.axis.copy(alpha = 0.35f), Offset(x(t), top0), Offset(x(t), yA), 0.5.dp.toPx())
        with(c) { text(lbl, x(t), yA + 14.dp.toPx(), size = 9.sp, align = 0.5f) }
    }
    if (now >= m.t0 && now <= m.t1) {
        drawLine(c.ink.copy(alpha = 0.5f), Offset(x(now.toDouble()), 0f), Offset(x(now.toDouble()), yA), 1.dp.toPx(), pathEffect = dash)
    }
}

// ---- market entry dot plot ------------------------------------------------------------

/** x = avg entry price, one lane per outcome, area ∝ cost basis. */
@Composable
fun EntryPlot(m: MarketView, nameOf: (String) -> String, onOpen: (String) -> Unit) {
    val pw = Pw.colors
    val outs = remember(m) { m.wallets.map { it.outcome }.distinct() }
    if (m.wallets.isEmpty()) {
        Text("Nothing to plot", color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    InteractiveChart(
        (8 + outs.size * 64 + 22).dp,
        "Entry prices of ${m.wallets.size} flagged wallets in ${m.title}; x is average entry price, circle area is cost basis.",
        "Tap a circle for the wallet · ┊ current price",
        onOpen,
    ) { c ->
        val L = 58.dp.toPx(); val R = 14.dp.toPx(); val T = 8.dp.toPx(); val LH = 64.dp.toPx()
        val pwid = max(120f, size.width - L - R)
        fun x(p: Double) = L + p.coerceIn(0.0, 1.0).toFloat() * pwid
        val maxC = max(m.wallets.maxOf { it.avgPrice * it.size }, 1.0)
        outs.forEachIndexed { i, o ->
            val y0 = T + i * LH
            val win = m.resolvedOutcome?.let { if (o == it) true else false }
            drawRect(when (win) { true -> pw.wonBg; false -> pw.lostBg; else -> pw.laneBg }, Offset(L, y0 + 1), Size(pwid, LH - 2))
            with(c) {
                text(o, L - 6.dp.toPx(), y0 + LH / 2, c.ink, 11.sp, 1f, bold = true, maxWidth = L - 8.dp.toPx())
                if (win != null) text(if (win) "✓ won" else "✗ lost", L - 6.dp.toPx(), y0 + LH / 2 + 13.dp.toPx(), if (win) pw.pos else pw.neg, 9.sp, 1f)
            }
            m.wallets.firstOrNull { it.outcome == o }?.currentPrice?.let { cur ->
                val xc = x(cur)
                drawLine(c.muted, Offset(xc, y0 + 1), Offset(xc, y0 + LH - 1), 1.dp.toPx(), pathEffect = dot)
                c.hits += Hit(Rect(xc - 4.dp.toPx(), y0, xc + 4.dp.toPx(), y0 + LH), "Current price of $o: ${price(cur)}")
            }
        }
        m.wallets.forEachIndexed { j, w ->
            val i = outs.indexOf(w.outcome)
            val cost = w.avgPrice * w.size
            val r = (3 + sqrt(cost / maxC) * 11).toFloat().dp.toPx()
            val jit = ((j * 0.618) % 1.0 - 0.5).toFloat()
            val cy = T + i * LH + LH / 2 + jit * (LH - 2 * min(r, 14.dp.toPx()) - 6.dp.toPx())
            val o = Offset(x(w.avgPrice), cy)
            val tc = pw.tier(tierOf(w.score))
            drawCircle(tc.copy(alpha = 0.6f), r, o)
            drawCircle(c.ink.copy(alpha = 0.25f), r, o, style = Stroke(1f))
            val hit = Hit(
                Rect(o, r + 4.dp.toPx()),
                "${nameOf(w.addr).ifBlank { shortAddr(w.addr) }} · score ${w.score}",
                "${w.outcome} avg ${price(w.avgPrice)} → ${price(w.currentPrice)}\n${num(w.size)} sh · cost ${usd(cost)}",
                "wallet/${w.addr}",
                "Wallet",
            )
            c.hits += hit
            if (isSel(c, hit)) drawCircle(c.ink, r + 2.dp.toPx(), o, style = Stroke(2.dp.toPx()))
        }
        val yA = T + outs.size * LH
        drawLine(pw.axis, Offset(L, yA), Offset(L + pwid, yA), 1.dp.toPx())
        for (k in 0..10) {
            val p = k / 10.0
            if (k % 2 == 1 && pwid < 300.dp.toPx()) continue
            drawLine(pw.axis.copy(alpha = 0.3f), Offset(x(p), T), Offset(x(p), yA), 0.5.dp.toPx())
            with(c) { text(fixed(p, 1), x(p), yA + 14.dp.toPx(), size = 9.sp, align = 0.5f) }
        }
    }
}

// ---- cluster node-link ------------------------------------------------------------------

/** Circle-layout node-link diagram; edges coloured by link kind, width by confidence. */
@Composable
fun ClusterGraph(cl: Cluster, rowOf: (String) -> CandidateRow?, onOpen: (String) -> Unit) {
    val pw = Pw.colors
    if (cl.members.isEmpty()) return
    InteractiveChart(
        320.dp,
        "Cluster ${cl.id}: ${cl.members.size} members, ${cl.links.size} links",
        "Tap a wallet or an edge label · edge width = confidence",
        onOpen,
    ) { c ->
        val W = size.width; val H = size.height
        val n = cl.members.size
        val cx = W / 2; val cy = H / 2
        val rad = if (n <= 1) 0f else min(W, H) / 2 - 44.dp.toPx()
        val pos = cl.members.mapIndexed { i, a ->
            val ang = -PI / 2 + i.toDouble() / n * 2 * PI
            a to Offset(cx + rad * cos(ang).toFloat(), cy + rad * sin(ang).toFloat())
        }.toMap()
        val pairCount = mutableMapOf<String, Int>()
        for (l in cl.links) {
            val pa = pos[l.a] ?: continue
            val pb = pos[l.b] ?: continue
            val key = listOf(l.a, l.b).sorted().joinToString("|")
            val k = pairCount[key] ?: 0
            pairCount[key] = k + 1
            val off = (if (k % 2 == 1) -1 else 1) * ceil(k / 2.0).toFloat() * 26.dp.toPx()
            val mx = (pa.x + pb.x) / 2; val my = (pa.y + pb.y) / 2
            val dx = pb.x - pa.x; val dy = pb.y - pa.y
            val len = hypot(dx, dy).takeIf { it > 0 } ?: 1f
            val q = Offset(mx - dy / len * off * 2, my + dx / len * off * 2)
            val lp = Offset(mx - dy / len * off, my + dx / len * off)
            val li = linkInfo(l.kind)
            val col = pw.sig(li.c)
            val path = Path().apply { moveTo(pa.x, pa.y); quadraticTo(q.x, q.y, pb.x, pb.y) }
            drawPath(path, col.copy(alpha = 0.8f), style = Stroke((1 + 4 * l.confidence).toFloat().dp.toPx(), cap = StrokeCap.Round))
            val hit = Hit(
                Rect(lp, 14.dp.toPx()),
                "${li.label} · confidence ${fixed(l.confidence, 2)}",
                "${shortAddr(l.a)} ↔ ${shortAddr(l.b)}\n${l.evidence}",
            )
            c.hits += hit
            val bw = 32.dp.toPx(); val bh = 15.dp.toPx()
            drawRoundRect(if (isSel(c, hit)) col else Color.Black.copy(alpha = 0.55f), Offset(lp.x - bw / 2, lp.y - bh / 2), Size(bw, bh), CornerRadius(4.dp.toPx()))
            with(c) { text(li.abbr, lp.x, lp.y + 3.5.dp.toPx(), Color.White, 9.sp, 0.5f, bold = true, mono = true) }
        }
        for (a in cl.members) {
            val p = pos.getValue(a)
            val r = rowOf(a)
            val sc = r?.score ?: 0
            val nr = (7 + sc / 12f).dp.toPx()
            val nm = r?.name?.takeIf { it.isNotBlank() } ?: shortAddr(a)
            val below = p.y >= cy
            val hit = Hit(Rect(p, nr + 6.dp.toPx()), nm, "$a\nscore ${if (r != null) sc.toString() else "– (not flagged)"}", "wallet/$a", "Wallet")
            c.hits += hit
            drawCircle(pw.tier(r?.tier), nr, p)
            drawCircle(if (isSel(c, hit)) c.ink else c.ink.copy(alpha = 0.3f), nr, p, style = Stroke(if (isSel(c, hit)) 2.5.dp.toPx() else 1.dp.toPx()))
            with(c) {
                text(
                    nm.take(18) + (if (r != null) " · $sc" else ""),
                    p.x, p.y + if (below) nr + 13.dp.toPx() else -nr - 5.dp.toPx(),
                    c.ink, 10.sp, 0.5f, maxWidth = W * 0.45f,
                )
            }
        }
    }
}
