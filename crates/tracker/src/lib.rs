//! One tracker run (every 8 min):
//!
//! 1. Load `watchlist.json`; evict entries (score < `watchlist_min_score` for
//!    `evict_low_score_s`, no activity for `evict_inactive_s`).
//! 2. Poll order: highest score first, then stalest `last_polled`. For each wallet call
//!    `/v2/activity?user=&start=last_activity_ts+1` (all pages), stop when
//!    `time_budget_s` is used up — leftovers are polled next run, nothing is lost.
//! 3. Convert rows to [`types::TrackedTrade`] ([`from_activity`]), dedup by
//!    [`trade_key`], prepend to `wallets/<addr>.json` (cap `max_trades_per_wallet`),
//!    advance `last_activity_ts`.
//! 4. Wallets that traded: refresh OPEN positions + value if `refresh_positions`.
//! 5. Emit [`types::TradeEvent`]s for trades ≥ `big_trade_usd` or ≥ `big_trade_share`
//!    of wallet value (the CLI hands them to `alerts::comment_trades`).
//!
//! Nothing is written with `dry_run`.
//!
//! STUB (WP0): signatures are final, bodies are filled in by WP5.

use serde::{Deserialize, Serialize};
use store::DataDir;
use types::api::{ActivityRow, PolyApi, RequestStats};
use types::{Addr, Config, TrackedTrade, TradeEvent};

#[derive(Debug, Clone, Default)]
pub struct TrackOpts {
    pub dry_run: bool,
    pub now: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackReport {
    pub watchlist_size: u32,
    pub polled: u32,
    /// Wallets not reached because the time budget ran out.
    pub deferred: u32,
    pub new_trades: u32,
    pub evicted: Vec<Addr>,
    pub events: Vec<TradeEvent>,
    pub duration_s: f64,
    pub requests: RequestStats,
    /// New trades for the monthly archive (`store::archive::append`).
    #[serde(skip)]
    pub archive: Vec<(Addr, TrackedTrade)>,
}

pub async fn run(_api: &dyn PolyApi, _data: &DataDir, _cfg: &Config, _opts: &TrackOpts) -> anyhow::Result<TrackReport> {
    todo!("WP5")
}

/// Hex sha256 of `tx|token_id|kind|size|ts` (size formatted with 6 decimals).
pub fn trade_key(_tx: &str, _token_id: &str, _kind: types::TradeKind, _size: f64, _ts: i64) -> String {
    todo!("WP5")
}

/// `TRADE` → Buy/Sell by `side`, `REDEEM`/`SPLIT`/`MERGE`/`DEPOSIT`/`WITHDRAWAL` mapped,
/// `REWARD`/`CONVERSION`/unknown → `None` (not tracked).
pub fn from_activity(_row: &ActivityRow) -> Option<TrackedTrade> {
    todo!("WP5")
}
