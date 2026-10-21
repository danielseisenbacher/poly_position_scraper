// Number/time formatting and label tables, ported from site/js/util.js.
package io.github.danielseisenbacher.polywatch.ui

import io.github.danielseisenbacher.polywatch.data.nowS
import java.time.Instant
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.util.Locale
import kotlin.math.abs
import kotlin.math.roundToLong

data class SigInfo(val abbr: String, val label: String, val c: Int)

val SIGNALS = linkedMapOf(
    "NewAccountLongshot" to SigInfo("NAL", "New account longshot", 1),
    "OldAccountWinRate" to SigInfo("OWR", "Old account win rate", 2),
    "LeaderboardEntrant" to SigInfo("LBE", "Leaderboard entrant", 3),
    "LongshotWinner" to SigInfo("LSW", "Longshot winner", 4),
    "Concentration" to SigInfo("CON", "Concentration", 5),
    "ResolutionTiming" to SigInfo("RTM", "Resolution timing", 6),
    "PreMove" to SigInfo("PRE", "Pre-move", 7),
    "NicheMarket" to SigInfo("NCH", "Niche market", 8),
    "LinkedAccounts" to SigInfo("LNK", "Linked accounts", 9),
)
fun sigInfo(k: String) = SIGNALS[k] ?: SigInfo(k.take(3).uppercase(), k, 9)

val LINKS = linkedMapOf(
    "SameOwnerEoa" to SigInfo("EOA", "Same owner EOA", 1),
    "SameXUsername" to SigInfo("X", "Same X username", 2),
    "SameProfileImageId" to SigInfo("IMG", "Same profile image", 3),
    "SameNamePseudonym" to SigInfo("NAME", "Same name/pseudonym", 4),
    "SequentialUserId" to SigInfo("SEQ", "Sequential user id", 5),
    "FactoryCoCreation" to SigInfo("FAC", "Factory co-creation", 6),
    "CoTrade" to SigInfo("CO", "Co-trading", 7),
)
fun linkInfo(k: String) = LINKS[k] ?: SigInfo(k.take(4), k, 9)

data class KindInfo(val sym: String, val short: String, val label: String)
val KINDS = mapOf(
    "buy" to KindInfo("▲", "B", "buy"),
    "sell" to KindInfo("▼", "S", "sell"),
    "redeem" to KindInfo("◆", "R", "redeem"),
    "split" to KindInfo("⧉", "SP", "split"),
    "merge" to KindInfo("⧓", "M", "merge"),
    "deposit" to KindInfo("↓", "D", "deposit"),
    "withdraw" to KindInfo("↑", "W", "withdraw"),
    "other" to KindInfo("·", "?", "other"),
)
fun kindInfo(k: String) = KINDS[k] ?: KINDS.getValue("other")

fun shortAddr(a: String) = if (a.length > 12) "${a.take(6)}…${a.takeLast(4)}" else a

private fun Double.ok() = !isNaN() && !isInfinite()
private fun fmt(d: Double, decimals: Int) = String.format(Locale.US, "%.${decimals}f", d)
private fun grouped(n: Long) = String.format(Locale.US, "%,d", n)

/** $1,234 / $12.3k / $4.56M. */
fun usd(n: Double?, sign: Boolean = false): String {
    if (n == null || !n.ok()) return "–"
    val s = if (n < 0) "−" else if (sign && n > 0) "+" else ""
    val a = abs(n)
    val t = when {
        a >= 1e6 -> fmt(a / 1e6, if (a >= 1e7) 1 else 2) + "M"
        a >= 1e4 -> fmt(a / 1e3, 1) + "k"
        a >= 1000 -> grouped(a.roundToLong())
        a >= 100 -> fmt(a, 0)
        else -> fmt(a, if (a >= 10) 1 else 2)
    }
    return "$s\$$t"
}

fun num(n: Number?, d: Int = 0): String {
    if (n == null) return "–"
    val v = n.toDouble()
    if (!v.ok()) return "–"
    return if (d == 0) grouped(v.roundToLong()) else String.format(Locale.US, "%,.${d}f", v)
}

fun price(p: Double?): String = if (p == null || !p.ok()) "–" else fmt(p, if (p < 0.1 && p > 0) 3 else 2)
fun pct(p: Double?, d: Int = 0): String = if (p == null || !p.ok()) "–" else fmt(p * 100, d) + "%"
fun fixed(n: Double?, d: Int = 1): String = if (n == null || !n.ok()) "–" else fmt(n, d)

/** Trim trailing ".0" for fractional counts (CLOSED positions count half). */
fun frac(n: Double?): String = when {
    n == null || !n.ok() -> "–"
    n == n.toLong().toDouble() -> n.toLong().toString()
    else -> fmt(n, 1)
}

/** -1 / 0 / 1 for colouring PnL. */
fun pnlSign(n: Double?): Int = if (n == null || !n.ok() || n == 0.0) 0 else if (n > 0) 1 else -1

fun rel(ts: Long?, now: Long = nowS()): String {
    if (ts == null || ts <= 0) return "–"
    val d = now - ts
    val a = abs(d).toDouble()
    val f = when {
        a < 60 -> "${a.roundToLong()} s"
        a < 3600 -> "${(a / 60).roundToLong()} min"
        a < 172800 -> "${(a / 3600).roundToLong()} h"
        else -> "${(a / 86400).roundToLong()} d"
    }
    return if (d >= 0) "$f ago" else "in $f"
}

fun relShort(ts: Long?, now: Long = nowS()): String =
    rel(ts, now).replace(" ago", "").replace(" min", "m").replace(" s", "s").replace(" h", "h").replace(" d", "d")

private val DATE = DateTimeFormatter.ofPattern("yyyy-MM-dd").withZone(ZoneOffset.UTC)
private val DATETIME = DateTimeFormatter.ofPattern("yyyy-MM-dd HH:mm'Z'").withZone(ZoneOffset.UTC)

fun date(ts: Long?, withTime: Boolean = false): String {
    if (ts == null) return "–"
    return (if (withTime) DATETIME else DATE).format(Instant.ofEpochSecond(ts))
}

fun duration(s: Long): String = when {
    s < 60 -> "${s}s"
    s < 3600 -> "${s / 60}m ${s % 60}s"
    else -> "${s / 3600}h ${(s % 3600) / 60}m"
}

fun tierOf(score: Int) = if (score >= 70) "high" else if (score >= 45) "medium" else "low"

val ADDR_RE = Regex("^0x[0-9a-f]{40}$")
