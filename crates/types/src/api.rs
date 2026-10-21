//! Rows returned by the public APIs, and the [`PolyApi`] trait.
//!
//! Row structs mirror Data API v2 JSON exactly (snake_case, see
//! `fixtures/live/*.json` for recorded examples). Every field the API might omit
//! has `#[serde(default)]`, so new or missing fields never break decoding.
//! Gamma and RPC responses are messier; the `api` crate normalises them into the
//! domain types [`crate::MarketMeta`], [`crate::Profile`] and [`FactoryLog`].

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::{Addr, MarketMeta, PosStatus, Profile};

/// One page of a cursor-paginated endpoint. `next_cursor == None` means last page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page<T> {
    pub data: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> Default for Page<T> {
    fn default() -> Self {
        Page { data: Vec::new(), next_cursor: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Side {
    Buy,
    Sell,
}

/// Row of `GET /v2/trades` (global feed or `?user=`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeedTrade {
    pub proxy_wallet: Addr,
    pub side: Side,
    pub token_id: String,
    pub condition_id: String,
    /// Shares.
    pub size: f64,
    pub price: f64,
    pub timestamp: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub event_slug: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub outcome_index: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub pseudonym: String,
    #[serde(default)]
    pub profile_image: String,
    #[serde(default)]
    pub transaction_hash: String,
}

impl FeedTrade {
    /// Protocol-V2 combo bets have an empty title, `outcome_index == 999` and a
    /// zero-padded synthetic condition id that per-market endpoints reject (400).
    pub fn is_combo(&self) -> bool {
        self.outcome_index == 999 || self.title.is_empty() || is_synthetic_condition(&self.condition_id)
    }

    pub fn usdc(&self) -> f64 {
        self.size * self.price
    }
}

/// Combo condition ids end in a long run of zeros (`0x038e…93c0000000000…`).
pub fn is_synthetic_condition(condition_id: &str) -> bool {
    condition_id.len() == 66 && condition_id.ends_with("00000000000000000000")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ActivityType {
    Trade,
    Split,
    Merge,
    Redeem,
    Conversion,
    Reward,
    Deposit,
    Withdrawal,
    #[serde(other)]
    Other,
}

/// Row of `GET /v2/activity?user=` (includes maker fills, unlike `/v2/trades`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityRow {
    pub proxy_wallet: Addr,
    pub timestamp: i64,
    #[serde(default)]
    pub condition_id: String,
    #[serde(rename = "type")]
    pub kind: ActivityType,
    #[serde(default)]
    pub size: f64,
    #[serde(default)]
    pub usdc_size: f64,
    #[serde(default)]
    pub transaction_hash: String,
    #[serde(default)]
    pub price: f64,
    #[serde(default)]
    pub token_id: String,
    /// `None` for non-trade rows (the API sends `""`).
    #[serde(default, deserialize_with = "empty_as_none")]
    pub side: Option<Side>,
    #[serde(default)]
    pub outcome_index: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub is_combo: bool,
}

fn empty_as_none<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Side>, D::Error> {
    let s: Option<String> = Option::deserialize(d)?;
    match s.as_deref() {
        None | Some("") => Ok(None),
        Some(v) => serde_json::from_value(serde_json::Value::String(v.to_string()))
            .map(Some)
            .map_err(serde::de::Error::custom),
    }
}

/// Row of `GET /v2/positions?user=&status=` or `?condition=`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionRow {
    pub proxy_wallet: Addr,
    pub token_id: String,
    pub condition_id: String,
    #[serde(default)]
    pub current_size: f64,
    #[serde(default)]
    pub avg_price: f64,
    #[serde(default)]
    pub entry_cost_usdc: f64,
    #[serde(default)]
    pub total_cost_usdc: f64,
    #[serde(default)]
    pub current_price: f64,
    #[serde(default)]
    pub current_value: f64,
    #[serde(default)]
    pub total_size: f64,
    #[serde(default)]
    pub realized_pnl: f64,
    #[serde(default)]
    pub unrealized_pnl: f64,
    #[serde(default)]
    pub total_pnl: f64,
    #[serde(default)]
    pub percent_pnl: f64,
    pub status: PosStatus,
    #[serde(default)]
    pub redeemable: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub event_slug: String,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub outcome_index: i64,
    /// `"YYYY-MM-DD"`; parse with [`crate::domain::parse_ymd`].
    #[serde(default)]
    pub end_date: Option<String>,
    #[serde(default)]
    pub first_entry_at: Option<i64>,
    #[serde(default)]
    pub last_event_at: Option<i64>,
}

/// `GET /v2/holders?condition=` returns one group per outcome token.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HolderGroup {
    pub token_id: String,
    pub holders: Vec<HolderRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HolderRow {
    pub proxy_wallet: Addr,
    pub token_id: String,
    /// Shares held.
    #[serde(default)]
    pub amount: f64,
    #[serde(default)]
    pub outcome_index: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub pseudonym: String,
    // Present only with `include_pnl=true`:
    #[serde(default)]
    pub avg_price: Option<f64>,
    #[serde(default)]
    pub entry_cost_usdc: Option<f64>,
    #[serde(default)]
    pub current_value: Option<f64>,
    #[serde(default)]
    pub total_pnl: Option<f64>,
}

/// `GET /v2/user-stats?user=` → `data` (null for unknown wallets).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserStatsRow {
    pub proxy_wallet: Addr,
    /// Number of distinct markets traded.
    #[serde(default)]
    pub trades: u64,
    #[serde(default)]
    pub biggest_win: f64,
    #[serde(default)]
    pub join_date: Option<i64>,
    #[serde(default)]
    pub all_time_pnl: Option<AllTimePnl>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AllTimePnl {
    #[serde(default)]
    pub realized_pnl: f64,
    #[serde(default)]
    pub unrealized_pnl: f64,
    #[serde(default)]
    pub volume_usdc: f64,
    #[serde(default)]
    pub trade_count: u64,
    #[serde(default)]
    pub fees: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimePeriod {
    Day,
    Week,
    Month,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum LeaderboardSort {
    Pnl,
    Volume,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardRow {
    #[serde(default)]
    pub rank: u32,
    pub user_id: Addr,
    #[serde(default)]
    pub pnl: f64,
    #[serde(default)]
    pub volume: f64,
    #[serde(default)]
    pub user_name: String,
    #[serde(default)]
    pub x_username: String,
    #[serde(default)]
    pub verified: bool,
}

/// One winning position (not one user) from `GET /v2/biggest-winners`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BiggestWinnerRow {
    #[serde(default)]
    pub win_rank: u32,
    pub user_id: Addr,
    #[serde(default)]
    pub pnl: f64,
    #[serde(default)]
    pub initial_value: f64,
    #[serde(default)]
    pub final_value: f64,
    #[serde(default)]
    pub resolved_at: i64,
    #[serde(default)]
    pub condition_id: String,
    #[serde(default)]
    pub event_slug: String,
    #[serde(default)]
    pub event_title: String,
    #[serde(default)]
    pub user_name: String,
}

/// Row of `GET /v2/tokens?condition=`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TokenRow {
    pub token_id: String,
    pub condition_id: String,
    #[serde(default)]
    pub module: String,
    #[serde(default)]
    pub outcome_index: i64,
    #[serde(default)]
    pub outcome: String,
    #[serde(default)]
    pub opposite_token_id: Option<String>,
    #[serde(default)]
    pub resolved: bool,
    /// 1.0 for the winning token, 0.0 for the loser, `None` until resolved.
    #[serde(default)]
    pub final_price: Option<f64>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub market_slug: String,
    #[serde(default)]
    pub closed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PricePoint {
    pub timestamp: i64,
    pub price: f64,
}

/// Normalised `GET /v2/status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ApiStatus {
    pub lag_seconds: i64,
    pub max_synced_block: Option<u64>,
}

/// Decoded Deposit Wallet Factory log: a new Polymarket proxy wallet was created.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactoryLog {
    pub proxy: Addr,
    pub owner: Addr,
    pub block: u64,
    pub tx: String,
}

/// Global trade-feed query (`GET /v2/trades` without `user`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TradesQuery {
    pub limit: u32,
    /// `false` also returns maker rows (≈3× volume).
    pub taker_only: bool,
    /// Minimum trade size in USDC (`filter_type=CASH&filter_amount=`).
    pub min_cash: Option<f64>,
    pub side: Option<Side>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivityQuery {
    pub user: Addr,
    /// Inclusive lower bound (unix s). Also serves as the CloudFront cache-buster.
    pub start: Option<i64>,
    pub end: Option<i64>,
    pub limit: u32,
}

/// Rate-limit classes, each with its own budget (requests per 10 s, per IP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateClass {
    /// Data API general bucket (800 / 10 s).
    Data,
    /// `/v2/trades` (300 / 10 s).
    Trades,
    /// `/v2/positions` and `/v2/activity` (200 / 10 s).
    Positions,
    /// Gamma general (4000 / 10 s; `/markets` 300 / 10 s).
    Gamma,
    /// Polygon JSON-RPC (publicnode; be polite).
    Rpc,
}

/// Request counters, reported in `meta.json` and the dry-run report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct RequestStats {
    pub requests: std::collections::BTreeMap<RateClass, u64>,
    pub retries: u64,
    pub rate_limited: u64,
    pub errors: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("HTTP {status} from {url}: {body}")]
    Http { status: u16, url: String, body: String },
    #[error("rate limited by {url} after retries")]
    RateLimited { url: String },
    #[error("could not decode response from {url}: {msg}")]
    Decode { url: String, msg: String },
    #[error("transport error for {url}: {msg}")]
    Transport { url: String, msg: String },
}

pub type ApiResult<T> = Result<T, ApiError>;

/// Everything polywatch reads from the outside world. Implemented by `api::Client`;
/// discovery and tracker take `&dyn PolyApi` so tests can pass a mock.
///
/// Implementations handle rate limiting, retries (429 `Retry-After`, 503, transport
/// errors), the explicit User-Agent and CloudFront cache-busting. Callers just call.
#[async_trait]
pub trait PolyApi: Send + Sync {
    // ---- Data API v2 ----
    async fn trades_page(&self, q: &TradesQuery, cursor: Option<&str>) -> ApiResult<Page<FeedTrade>>;
    async fn user_trades(&self, user: &Addr, start: Option<i64>, cursor: Option<&str>) -> ApiResult<Page<FeedTrade>>;
    async fn activity(&self, q: &ActivityQuery, cursor: Option<&str>) -> ApiResult<Page<ActivityRow>>;
    async fn positions(&self, user: &Addr, status: PosStatus, cursor: Option<&str>) -> ApiResult<Page<PositionRow>>;
    /// All holders of one market whose position changed at or after `start`.
    async fn market_positions(&self, condition_id: &str, start: Option<i64>, cursor: Option<&str>) -> ApiResult<Page<PositionRow>>;
    async fn holders(&self, condition_id: &str, include_pnl: bool, limit: u32) -> ApiResult<Vec<HolderGroup>>;
    async fn user_stats(&self, user: &Addr) -> ApiResult<Option<UserStatsRow>>;
    async fn value(&self, user: &Addr) -> ApiResult<f64>;
    async fn leaderboard(&self, period: TimePeriod, sort: LeaderboardSort, cursor: Option<&str>) -> ApiResult<Page<LeaderboardRow>>;
    async fn biggest_winners(&self, period: TimePeriod, cursor: Option<&str>) -> ApiResult<Page<BiggestWinnerRow>>;
    async fn tokens(&self, condition_id: &str) -> ApiResult<Vec<TokenRow>>;
    async fn prices_history(&self, token_id: &str, start: i64, end: i64) -> ApiResult<Vec<PricePoint>>;
    async fn status(&self) -> ApiResult<ApiStatus>;

    // ---- Gamma ----
    /// Markets closed at or after `since` (unix s), newest first, at most `max`.
    async fn recently_closed_markets(&self, since: i64, max: usize) -> ApiResult<Vec<MarketMeta>>;
    /// Market metadata for the given condition ids (implementation batches requests).
    async fn markets_by_condition(&self, condition_ids: &[String]) -> ApiResult<Vec<MarketMeta>>;
    /// `None` when Gamma has no profile (404).
    async fn profile(&self, addr: &Addr) -> ApiResult<Option<Profile>>;

    // ---- Polygon RPC ----
    async fn block_number(&self) -> ApiResult<u64>;
    /// Deposit Wallet Factory creations in `[from_block, to_block]` (implementation chunks ranges).
    async fn factory_logs(&self, from_block: u64, to_block: u64) -> ApiResult<Vec<FactoryLog>>;

    /// Counters since the client was created.
    fn stats(&self) -> RequestStats;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> serde_json::Value {
        let p = format!("{}/../../fixtures/live/{name}.json", env!("CARGO_MANIFEST_DIR"));
        serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
    }

    #[test]
    fn decodes_recorded_feed_and_flags_combos() {
        let rows: Vec<FeedTrade> = serde_json::from_value(fixture("trades_global")["data"].clone()).unwrap();
        assert!(!rows.is_empty());
        assert!(rows.iter().any(|r| r.is_combo()));
        assert!(rows.iter().any(|r| !r.is_combo()));
    }

    #[test]
    fn decodes_recorded_rows() {
        let _: Vec<ActivityRow> = serde_json::from_value(fixture("activity_user")["data"].clone()).unwrap();
        for s in ["OPEN", "CLOSED", "REDEEMABLE", "REDEEMABLE_LOST"] {
            let _: Vec<PositionRow> =
                serde_json::from_value(fixture(&format!("positions_user_{s}"))["data"].clone()).unwrap();
        }
        let _: Vec<PositionRow> = serde_json::from_value(fixture("positions_condition")["data"].clone()).unwrap();
        let _: Vec<HolderGroup> = serde_json::from_value(fixture("holders_pnl")["data"].clone()).unwrap();
        let _: UserStatsRow = serde_json::from_value(fixture("user_stats")["data"].clone()).unwrap();
        let _: Vec<LeaderboardRow> = serde_json::from_value(fixture("leaderboard")["data"].clone()).unwrap();
        let _: Vec<BiggestWinnerRow> = serde_json::from_value(fixture("biggest_winners")["data"].clone()).unwrap();
        let _: Vec<TokenRow> = serde_json::from_value(fixture("tokens_resolved")["data"].clone()).unwrap();
        let _: Vec<PricePoint> = serde_json::from_value(fixture("prices_history")["data"].clone()).unwrap();
    }
}
