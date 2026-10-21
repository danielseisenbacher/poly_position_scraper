// JSON shapes published on the `data` branch. Mirrors crates/types/src/site.rs and
// crates/types/src/domain.rs; every field has a default so a schema change never
// breaks parsing of the rest.
package io.github.danielseisenbacher.polywatch.data

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.doubleOrNull
import kotlinx.serialization.json.jsonPrimitive

@Serializable
data class Meta(
    @SerialName("generated_at") val generatedAt: Long = 0,
    @SerialName("run_id") val runId: String? = null,
    @SerialName("last_discover") val lastDiscover: Long = 0,
    @SerialName("last_track") val lastTrack: Long = 0,
    val counts: Counts = Counts(),
    val api: ApiHealth = ApiHealth(),
    @SerialName("feed_lag_s") val feedLagS: Long = 0,
    @SerialName("feed_truncated") val feedTruncated: Boolean = false,
    @SerialName("score_histogram") val scoreHistogram: List<Long> = emptyList(),
    @SerialName("signal_counts") val signalCounts: Map<String, Long> = emptyMap(),
)

@Serializable
data class Counts(
    val candidates: Long = 0,
    val watchlist: Long = 0,
    val high: Long = 0,
    val medium: Long = 0,
    val clusters: Long = 0,
    @SerialName("wallets_scanned_last_run") val walletsScannedLastRun: Long = 0,
    @SerialName("trades_tracked_last_run") val tradesTrackedLastRun: Long = 0,
)

@Serializable
data class ApiHealth(
    val requests: Map<String, Long> = emptyMap(),
    @SerialName("rate_limited") val rateLimited: Long = 0,
    val errors: Long = 0,
)

/** A (kind, strength) pair from `CandidateRow.signals`. */
data class SignalStrength(val kind: String, val strength: Double)

@Serializable
data class CandidateRow(
    val addr: String = "",
    val name: String = "",
    val score: Int = 0,
    val tier: String = "low",
    @SerialName("signals") val rawSignals: List<JsonArray> = emptyList(),
    @SerialName("age_d") val ageD: Double? = null,
    val pnl: Double = 0.0,
    @SerialName("win_rate") val winRate: Double? = null,
    @SerialName("n_resolved") val nResolved: Double = 0.0,
    val z: Double = 0.0,
    val value: Double = 0.0,
    @SerialName("n_markets") val nMarkets: Int = 0,
    @SerialName("last_trade_ts") val lastTradeTs: Long? = null,
    @SerialName("cluster_id") val clusterId: String? = null,
    val issue: Long? = null,
    val watched: Boolean = false,
    @SerialName("first_seen") val firstSeen: Long = 0,
) {
    val signals: List<SignalStrength> by lazy {
        rawSignals.mapNotNull { a ->
            val k = a.getOrNull(0)?.jsonPrimitive?.content ?: return@mapNotNull null
            SignalStrength(k, a.getOrNull(1)?.jsonPrimitive?.doubleOrNull ?: 0.0)
        }
    }
}

@Serializable
data class MarketView(
    @SerialName("condition_id") val conditionId: String = "",
    val title: String = "",
    val slug: String = "",
    @SerialName("end_date") val endDate: Long? = null,
    val volume: Double? = null,
    @SerialName("resolved_outcome") val resolvedOutcome: String? = null,
    val wallets: List<MarketWallet> = emptyList(),
)

@Serializable
data class MarketWallet(
    val addr: String = "",
    val score: Int = 0,
    val outcome: String = "",
    @SerialName("avg_price") val avgPrice: Double = 0.0,
    val size: Double = 0.0,
    @SerialName("current_price") val currentPrice: Double = 0.0,
)

@Serializable
data class TapeRow(
    val addr: String = "",
    val name: String = "",
    val score: Int = 0,
    val ts: Long = 0,
    val kind: String = "other",
    val title: String = "",
    val outcome: String = "",
    val price: Double = 0.0,
    val usdc: Double = 0.0,
    @SerialName("condition_id") val conditionId: String = "",
    val tx: String = "",
)

@Serializable
data class Link(
    val a: String = "",
    val b: String = "",
    val kind: String = "",
    val confidence: Double = 0.0,
    val evidence: String = "",
)

@Serializable
data class Cluster(
    val id: String = "",
    val members: List<String> = emptyList(),
    val links: List<Link> = emptyList(),
)

@Serializable
data class Profile(
    val name: String = "",
    val pseudonym: String = "",
    @SerialName("x_username") val xUsername: String = "",
    @SerialName("profile_image_id") val profileImageId: Long? = null,
    @SerialName("user_id") val userId: Long? = null,
    @SerialName("created_at") val createdAt: Long? = null,
    @SerialName("owner_eoa") val ownerEoa: String? = null,
    val verified: Boolean = false,
)

@Serializable
data class UserStats(
    @SerialName("join_date") val joinDate: Long? = null,
    @SerialName("markets_traded") val marketsTraded: Long = 0,
    @SerialName("trade_count") val tradeCount: Long = 0,
    @SerialName("volume_usdc") val volumeUsdc: Double = 0.0,
    @SerialName("realized_pnl") val realizedPnl: Double? = null,
    @SerialName("biggest_win") val biggestWin: Double = 0.0,
)

@Serializable
data class Position(
    @SerialName("condition_id") val conditionId: String = "",
    @SerialName("token_id") val tokenId: String = "",
    val title: String = "",
    val slug: String = "",
    val outcome: String = "",
    @SerialName("avg_price") val avgPrice: Double = 0.0,
    @SerialName("total_size") val totalSize: Double = 0.0,
    @SerialName("current_size") val currentSize: Double = 0.0,
    val cost: Double = 0.0,
    @SerialName("current_value") val currentValue: Double = 0.0,
    @SerialName("current_price") val currentPrice: Double = 0.0,
    @SerialName("realized_pnl") val realizedPnl: Double = 0.0,
    @SerialName("total_pnl") val totalPnl: Double = 0.0,
    @SerialName("percent_pnl") val percentPnl: Double = 0.0,
    val status: String = "OPEN",
    @SerialName("first_entry_at") val firstEntryAt: Long? = null,
    @SerialName("last_event_at") val lastEventAt: Long? = null,
    @SerialName("end_date") val endDate: Long? = null,
    @SerialName("market_volume") val marketVolume: Double? = null,
    @SerialName("market_duration_h") val marketDurationH: Double? = null,
    val category: String = "other",
) {
    /** Mirrors `Position::won` in crates/types/src/domain.rs. */
    val won: Boolean?
        get() = when (status) {
            "REDEEMABLE_LOST" -> false
            "REDEEMABLE" -> currentPrice >= 0.999
            "CLOSED" -> if (currentPrice >= 0.999) true else if (currentPrice <= 0.001) false else null
            else -> null
        }
}

@Serializable
data class ResolvedSummary(
    val n: Double = 0.0,
    val wins: Double = 0.0,
    @SerialName("expected_wins") val expectedWins: Double = 0.0,
    val z: Double = 0.0,
    @SerialName("longshot_wins") val longshotWins: Long = 0,
)

@Serializable
data class Signal(
    val kind: String = "",
    val strength: Double = 0.0,
    val evidence: String = "",
    val refs: List<String> = emptyList(),
)

@Serializable
data class Score(
    val total: Int? = null,
    val tier: String? = null,
    val materiality: Double = 1.0,
)

@Serializable
data class TrackedTrade(
    val key: String = "",
    val ts: Long = 0,
    val kind: String = "other",
    @SerialName("condition_id") val conditionId: String = "",
    @SerialName("token_id") val tokenId: String = "",
    val title: String = "",
    val outcome: String = "",
    val price: Double = 0.0,
    val size: Double = 0.0,
    val usdc: Double = 0.0,
    val tx: String = "",
)

@Serializable
data class Wallet(
    val addr: String = "",
    val profile: Profile? = null,
    val stats: UserStats? = null,
    val positions: List<Position> = emptyList(),
    val resolved: ResolvedSummary = ResolvedSummary(),
    val signals: List<Signal> = emptyList(),
    val score: Score = Score(),
    val sources: List<String> = emptyList(),
    @SerialName("cluster_id") val clusterId: String? = null,
    @SerialName("first_seen") val firstSeen: Long = 0,
    @SerialName("last_scored") val lastScored: Long = 0,
    val value: Double = 0.0,
    val trades: List<TrackedTrade> = emptyList(),
    val issue: Long? = null,
)
