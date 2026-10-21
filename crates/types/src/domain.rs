//! The scored-wallet model. One [`Wallet`] is persisted per flagged/watched address
//! at `data/wallets/<addr>.json`.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::api::PositionRow;
use crate::Addr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PosStatus {
    Open,
    Closed,
    Redeemable,
    RedeemableLost,
}

impl PosStatus {
    /// Query-string value for `/v2/positions?status=`.
    pub fn as_param(self) -> &'static str {
        match self {
            PosStatus::Open => "OPEN",
            PosStatus::Closed => "CLOSED",
            PosStatus::Redeemable => "REDEEMABLE",
            PosStatus::RedeemableLost => "REDEEMABLE_LOST",
        }
    }
}

/// Coarse market category used to discount public-information-driven markets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MarketCategory {
    Sports,
    Crypto,
    Politics,
    #[default]
    Other,
}

/// Normalised market metadata (from Gamma), cached in `data/markets_cache.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketMeta {
    pub condition_id: String,
    pub question: String,
    pub slug: String,
    #[serde(default)]
    pub event_slug: String,
    pub start_date: Option<i64>,
    pub end_date: Option<i64>,
    pub closed_time: Option<i64>,
    pub volume: Option<f64>,
    pub liquidity: Option<f64>,
    pub outcomes: Vec<String>,
    pub token_ids: Vec<String>,
    /// Final/current price per outcome, same order as `outcomes`.
    pub outcome_prices: Vec<f64>,
    /// Index into `outcomes` of the winner once resolved.
    pub winning_index: Option<usize>,
    pub category: MarketCategory,
    #[serde(default)]
    pub neg_risk: bool,
}

impl MarketMeta {
    pub fn duration_h(&self) -> Option<f64> {
        match (self.start_date, self.end_date.or(self.closed_time)) {
            (Some(s), Some(e)) if e > s => Some((e - s) as f64 / 3600.0),
            _ => None,
        }
    }

    pub fn winning_token(&self) -> Option<&str> {
        self.winning_index
            .and_then(|i| self.token_ids.get(i))
            .map(String::as_str)
    }
}

/// One position of a wallet, normalised from [`PositionRow`] and optionally
/// enriched with market metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub condition_id: String,
    pub token_id: String,
    pub title: String,
    pub slug: String,
    pub outcome: String,
    pub avg_price: f64,
    pub total_size: f64,
    pub current_size: f64,
    /// Cost basis in USDC.
    pub cost: f64,
    pub current_value: f64,
    /// 1.0 = this outcome won, 0.0 = lost, otherwise the live price.
    pub current_price: f64,
    pub realized_pnl: f64,
    pub total_pnl: f64,
    pub percent_pnl: f64,
    pub status: PosStatus,
    pub first_entry_at: Option<i64>,
    pub last_event_at: Option<i64>,
    pub end_date: Option<i64>,
    #[serde(default)]
    pub market_volume: Option<f64>,
    #[serde(default)]
    pub market_duration_h: Option<f64>,
    #[serde(default)]
    pub category: MarketCategory,
}

impl From<PositionRow> for Position {
    fn from(r: PositionRow) -> Self {
        let cost = if r.total_cost_usdc > 0.0 {
            r.total_cost_usdc
        } else {
            r.avg_price * r.total_size
        };
        Position {
            condition_id: r.condition_id,
            token_id: r.token_id,
            title: r.title,
            slug: r.slug,
            outcome: r.outcome,
            avg_price: r.avg_price,
            total_size: r.total_size,
            current_size: r.current_size,
            cost,
            current_value: r.current_value,
            current_price: r.current_price,
            realized_pnl: r.realized_pnl,
            total_pnl: r.total_pnl,
            percent_pnl: r.percent_pnl,
            status: r.status,
            first_entry_at: r.first_entry_at,
            last_event_at: r.last_event_at,
            end_date: r.end_date.as_deref().and_then(parse_ymd),
            market_volume: None,
            market_duration_h: None,
            category: MarketCategory::Other,
        }
    }
}

impl Position {
    pub fn is_resolved(&self) -> bool {
        matches!(
            self.status,
            PosStatus::Redeemable | PosStatus::RedeemableLost
        ) || (self.status == PosStatus::Closed
            && (self.current_price >= 0.999 || self.current_price <= 0.001))
    }

    /// `Some(true)` won, `Some(false)` lost, `None` unresolved/unknown.
    pub fn won(&self) -> Option<bool> {
        match self.status {
            PosStatus::RedeemableLost => Some(false),
            PosStatus::Redeemable => Some(self.current_price >= 0.999),
            PosStatus::Closed if self.current_price >= 0.999 => Some(true),
            PosStatus::Closed if self.current_price <= 0.001 => Some(false),
            _ => None,
        }
    }
}

/// Parse `"YYYY-MM-DD"` (optionally followed by `T…`) to unix seconds at 00:00 UTC.
pub fn parse_ymd(s: &str) -> Option<i64> {
    let d = s.get(..10)?;
    let mut it = d.split('-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let day: i64 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    // Howard Hinnant's days_from_civil.
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146_097 + doe - 719_468) * 86_400)
}

/// Gamma public profile, normalised.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Profile {
    pub name: String,
    pub pseudonym: String,
    pub x_username: String,
    /// Numeric id embedded in the profile image file name (`profile-image-<id>-…`).
    pub profile_image_id: Option<u64>,
    /// Sequential Polymarket user id (`users[0].id`).
    pub user_id: Option<u64>,
    pub created_at: Option<i64>,
    pub owner_eoa: Option<Addr>,
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct UserStats {
    pub join_date: Option<i64>,
    pub markets_traded: u64,
    pub trade_count: u64,
    pub volume_usdc: f64,
    pub realized_pnl: f64,
    pub biggest_win: f64,
}

impl From<crate::api::UserStatsRow> for UserStats {
    fn from(r: crate::api::UserStatsRow) -> Self {
        let p = r.all_time_pnl.unwrap_or_default();
        UserStats {
            join_date: r.join_date,
            markets_traded: r.trades,
            trade_count: p.trade_count,
            volume_usdc: p.volume_usdc,
            realized_pnl: p.realized_pnl,
            biggest_win: r.biggest_win,
        }
    }
}

/// How a wallet entered the candidate funnel. Leaderboard and biggest-winner
/// sources are selected on outcome, so the win-rate signal is penalised for them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Feed,
    Leaderboard,
    BiggestWinner,
    LongshotHolder,
    NewWallet,
    Seed,
    Watchlist,
}

impl Source {
    pub fn outcome_selected(self) -> bool {
        matches!(self, Source::Leaderboard | Source::BiggestWinner)
    }
}

/// Win-rate statistics over resolved positions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct ResolvedSummary {
    /// Resolved positions counted (CLOSED ones at half weight, so may be fractional).
    pub n: f64,
    pub wins: f64,
    /// Σ avg_price: wins expected if the wallet had no edge.
    pub expected_wins: f64,
    /// (wins − expected) / sqrt(Σ p(1−p)).
    pub z: f64,
    /// Wins bought at avg_price ≤ `longshot_win_price`.
    pub longshot_wins: u32,
}

impl ResolvedSummary {
    pub fn win_rate(&self) -> Option<f64> {
        (self.n > 0.0).then(|| self.wins / self.n)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum SignalKind {
    NewAccountLongshot,
    OldAccountWinRate,
    LeaderboardEntrant,
    LongshotWinner,
    Concentration,
    ResolutionTiming,
    PreMove,
    NicheMarket,
    LinkedAccounts,
}

impl SignalKind {
    pub const ALL: [SignalKind; 9] = [
        SignalKind::NewAccountLongshot,
        SignalKind::OldAccountWinRate,
        SignalKind::LeaderboardEntrant,
        SignalKind::LongshotWinner,
        SignalKind::Concentration,
        SignalKind::ResolutionTiming,
        SignalKind::PreMove,
        SignalKind::NicheMarket,
        SignalKind::LinkedAccounts,
    ];

    /// kebab-case slug for labels (`sig:old-account-win-rate`) and CSS classes.
    pub fn slug(self) -> &'static str {
        match self {
            SignalKind::NewAccountLongshot => "new-account-longshot",
            SignalKind::OldAccountWinRate => "old-account-win-rate",
            SignalKind::LeaderboardEntrant => "leaderboard-entrant",
            SignalKind::LongshotWinner => "longshot-winner",
            SignalKind::Concentration => "concentration",
            SignalKind::ResolutionTiming => "resolution-timing",
            SignalKind::PreMove => "pre-move",
            SignalKind::NicheMarket => "niche-market",
            SignalKind::LinkedAccounts => "linked-accounts",
        }
    }
}

impl fmt::Display for SignalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Signal {
    pub kind: SignalKind,
    /// 0..=1.
    pub strength: f64,
    /// Human-readable one-liner, e.g. "won 7/9 resolved at mean entry 0.21 (z=3.4)".
    pub evidence: String,
    /// condition_ids the evidence refers to.
    #[serde(default)]
    pub refs: Vec<String>,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    #[default]
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Score {
    /// 0..=100.
    pub total: u8,
    pub tier: Tier,
    /// Multiplier applied for position size (0.6..=1.3).
    pub materiality: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TradeKind {
    Buy,
    Sell,
    Redeem,
    Split,
    Merge,
    Deposit,
    Withdraw,
    Other,
}

/// A trade/activity row recorded by the tracker for a watched wallet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackedTrade {
    /// Stable dedup key: hex sha256 of `tx|token_id|kind|size|ts` (see `tracker`).
    pub key: String,
    pub ts: i64,
    pub kind: TradeKind,
    pub condition_id: String,
    pub token_id: String,
    pub title: String,
    pub outcome: String,
    pub price: f64,
    /// Shares.
    pub size: f64,
    pub usdc: f64,
    pub tx: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LinkKind {
    SameOwnerEoa,
    SameXUsername,
    SameProfileImageId,
    SameNamePseudonym,
    SequentialUserId,
    FactoryCoCreation,
    CoTrade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub a: Addr,
    pub b: Addr,
    pub kind: LinkKind,
    /// 0..=1.
    pub confidence: f64,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cluster {
    /// Deterministic: `c-` + first 10 hex chars of the smallest member address.
    pub id: String,
    pub members: Vec<Addr>,
    pub links: Vec<Link>,
}

/// A scored wallet. Persisted at `data/wallets/<addr>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wallet {
    pub addr: Addr,
    pub profile: Option<Profile>,
    pub stats: Option<UserStats>,
    /// Snapshot of OPEN + REDEEMABLE + REDEEMABLE_LOST + CLOSED positions.
    pub positions: Vec<Position>,
    pub resolved: ResolvedSummary,
    pub signals: Vec<Signal>,
    pub score: Score,
    #[serde(default)]
    pub sources: Vec<Source>,
    pub cluster_id: Option<String>,
    pub first_seen: i64,
    pub last_scored: i64,
    /// Total current value of open positions (USDC).
    #[serde(default)]
    pub value: f64,
    /// Rolling, newest first, capped at `tracker.max_trades_per_wallet`.
    #[serde(default)]
    pub trades: Vec<TrackedTrade>,
    pub issue: Option<u64>,
}

impl Wallet {
    pub fn new(addr: Addr, now: i64) -> Self {
        Wallet {
            addr,
            profile: None,
            stats: None,
            positions: Vec::new(),
            resolved: ResolvedSummary::default(),
            signals: Vec::new(),
            score: Score::default(),
            sources: Vec::new(),
            cluster_id: None,
            first_seen: now,
            last_scored: now,
            value: 0.0,
            trades: Vec::new(),
            issue: None,
        }
    }

    /// Account age in days, preferring user-stats `join_date`, then profile `created_at`.
    pub fn age_days(&self, now: i64) -> Option<f64> {
        let joined = self
            .stats
            .as_ref()
            .and_then(|s| s.join_date)
            .or_else(|| self.profile.as_ref().and_then(|p| p.created_at))?;
        Some(((now - joined).max(0)) as f64 / 86_400.0)
    }

    pub fn open_positions(&self) -> impl Iterator<Item = &Position> {
        self.positions
            .iter()
            .filter(|p| p.status == PosStatus::Open)
    }
}

/// Emitted by the tracker for trades worth a comment on the wallet's issue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TradeEvent {
    pub addr: Addr,
    pub trade: TrackedTrade,
    pub wallet_value: f64,
    pub issue: Option<u64>,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_dates() {
        assert_eq!(parse_ymd("1970-01-01"), Some(0));
        assert_eq!(parse_ymd("2026-08-01"), Some(1_785_542_400));
        assert_eq!(parse_ymd("2026-10-28T10:55:00Z"), Some(1_793_145_600));
        assert_eq!(parse_ymd("garbage"), None);
    }

    #[test]
    fn position_from_recorded_row() {
        let p = format!(
            "{}/../../fixtures/live/positions_user_REDEEMABLE.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
        let rows: Vec<PositionRow> = serde_json::from_value(v["data"].clone()).unwrap();
        let pos: Position = rows[0].clone().into();
        assert_eq!(pos.status, PosStatus::Redeemable);
        assert!(pos.end_date.is_some());
        assert!(pos.cost > 0.0);
    }

    #[test]
    fn signal_slugs_unique() {
        let mut s: Vec<_> = SignalKind::ALL.iter().map(|k| k.slug()).collect();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), SignalKind::ALL.len());
    }
}
