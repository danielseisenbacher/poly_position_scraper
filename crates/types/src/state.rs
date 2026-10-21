//! Non-dashboard files on the `data` branch. Paths are constants in `store::paths`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Addr, MarketMeta, SignalKind};

/// `data/watchlist.json`: wallets the tracker polls.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Watchlist {
    pub entries: Vec<WatchlistEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WatchlistEntry {
    pub addr: Addr,
    pub added_at: i64,
    /// Strongest signal when added.
    pub reason: SignalKind,
    pub score: u8,
    /// Newest trade/activity timestamp seen; the tracker polls `start = last_activity_ts + 1`.
    pub last_activity_ts: i64,
    pub last_polled: i64,
    /// Last time the score was ≥ `watchlist_min_score` (eviction clock).
    #[serde(default)]
    pub last_qualified: i64,
    pub issue: Option<u64>,
}

/// `data/alerts.json`: dedup state for GitHub issues, keyed by wallet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct AlertsState {
    pub wallets: BTreeMap<Addr, AlertRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertRecord {
    pub issue: u64,
    pub opened_at: i64,
    pub last_score: u8,
    pub signal_kinds: Vec<SignalKind>,
    #[serde(default)]
    pub last_comment_ts: i64,
    #[serde(default)]
    pub closed: bool,
}

/// `data/cursors.json`: where each incremental source stopped last time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Cursors {
    /// Newest global-feed trade timestamp ingested. The feed is newest-first and
    /// ignores `start`, so discovery pages backwards until it reaches this.
    pub feed_high_water_ts: i64,
    /// Last Polygon block scanned for Deposit Wallet Factory logs.
    pub factory_last_block: u64,
    /// Markets closed after this were checked for longshot winners.
    pub resolved_markets_since: i64,
    pub last_discover: i64,
    pub last_track: i64,
}

/// `data/markets_cache.json`: Gamma metadata for every market a scored wallet touches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MarketCache {
    pub markets: BTreeMap<String, MarketMeta>,
    /// condition_id → unix s fetched (refresh unresolved markets after a day).
    #[serde(default)]
    pub fetched_at: BTreeMap<String, i64>,
}

impl MarketCache {
    pub fn get(&self, condition_id: &str) -> Option<&MarketMeta> {
        self.markets.get(condition_id)
    }
}

/// Release asset `evaluated.bin`: every wallet ever enriched, for the re-eval TTL.
/// The on-disk encoding is owned by `store` (fixed 28-byte records).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EvaluatedSet {
    pub entries: std::collections::HashMap<Addr, Evaluated>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Evaluated {
    pub last_eval_ts: i64,
    pub last_score: u8,
}

/// Release asset `proxy_owner.bin`: proxy wallet → owner EOA from factory logs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProxyOwnerMap {
    /// proxy → (owner, block created).
    pub entries: std::collections::HashMap<Addr, (Addr, u64)>,
}
