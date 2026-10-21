//! Insider-signal scoring. Pure functions, no I/O: everything a signal needs is in
//! [`ScoreInput`], gathered by `discovery`.
//!
//! `score = min(100, Σ weight_k · strength_k) × materiality`, where
//! `materiality = clamp(0.6 + 0.2·log10(max(value, 100) / 100), 0.6, 1.3)`.
//! Signal rules and thresholds: see the plan and [`types::config::SignalConfig`].
//!
//! STUB (WP0): signatures are final, bodies are filled in by WP3.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use types::api::{BiggestWinnerRow, PricePoint, TimePeriod};
use types::config::SignalConfig;
use types::state::MarketCache;
use types::{Cluster, ResolvedSummary, Score, Signal, Wallet};

/// The wallet appeared on a PnL leaderboard this run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardHit {
    pub period: TimePeriod,
    pub rank: u32,
    pub pnl: f64,
}

/// The wallet held the winning token of a recently resolved market that traded as a
/// longshot shortly before close.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LongshotHit {
    pub condition_id: String,
    pub title: String,
    pub token_id: String,
    /// The wallet's average entry price on the winning token.
    pub entry_price: f64,
    /// Lowest price the winning token traded at within the window before close.
    pub low_price_before_close: f64,
    pub closed_at: i64,
    /// Shares held.
    pub amount: f64,
}

pub struct ScoreInput<'a> {
    /// Must have `positions` (all four statuses), `stats`, and `sources` filled.
    /// `profile` is optional at this stage.
    pub wallet: &'a Wallet,
    pub markets: &'a MarketCache,
    pub leaderboard: &'a [LeaderboardHit],
    /// Rows of `/v2/biggest-winners` for this wallet.
    pub biggest_wins: &'a [BiggestWinnerRow],
    pub longshot_hits: &'a [LongshotHit],
    /// token_id → price path, only for tokens requested via [`premove_requests`].
    pub price_paths: &'a HashMap<String, Vec<PricePoint>>,
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evaluation {
    pub signals: Vec<Signal>,
    pub score: Score,
    pub resolved: ResolvedSummary,
    /// Total current value of open positions (USDC), excluding dust.
    pub value: f64,
}

/// Evaluate every signal except `LinkedAccounts` (cluster-level, see
/// [`apply_cluster_bonus`]) and combine them into a score.
/// `PreMove` is only produced when `price_paths` contains the needed tokens.
pub fn evaluate(_input: &ScoreInput<'_>, _cfg: &SignalConfig) -> Evaluation {
    todo!("WP3")
}

/// A price-history fetch needed for `PreMove`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PriceRequest {
    pub token_id: String,
    pub start: i64,
    pub end: i64,
}

/// Which price paths to fetch for this wallet, given its provisional evaluation
/// (empty if `provisional.score.total < cfg.premove_min_provisional`).
/// At most `cfg.premove_max_positions` requests, largest positions first.
pub fn premove_requests(_wallet: &Wallet, _provisional: &Evaluation, _cfg: &SignalConfig) -> Vec<PriceRequest> {
    todo!("WP3")
}

/// Win-rate statistics over resolved positions (see `OldAccountWinRate`).
pub fn resolved_summary(_wallet: &Wallet, _markets: &MarketCache, _cfg: &SignalConfig) -> ResolvedSummary {
    todo!("WP3")
}

/// Combine signals into a [`Score`]: weights from `cfg`, cap 100, materiality multiplier, tier.
pub fn combine(_signals: &[Signal], _value: f64, _cfg: &SignalConfig) -> Score {
    todo!("WP3")
}

/// Add/refresh the `LinkedAccounts` signal on every wallet that belongs to a cluster
/// with ≥ 2 members, set `wallet.cluster_id`, and recompute `wallet.score`.
/// Strength: `min(1, linked_bonus_per_member · (flagged_members − 1) / w_linked_accounts)`,
/// where flagged members are cluster members scoring ≥ `tier_medium` on their own.
pub fn apply_cluster_bonus(_wallets: &mut [Wallet], _clusters: &[Cluster], _cfg: &SignalConfig) {
    todo!("WP3")
}
