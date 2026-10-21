//! One discovery run (every 4 h):
//!
//! 1. Sources → candidate set with [`types::Source`] tags:
//!    - global trade feed newest-first back to `cursors.feed_high_water_ts`
//!      (skip combos and markets shorter than `min_market_duration_h`), aggregated per
//!      wallet and pre-filtered (buy volume, mean buy price / #markets);
//!    - PnL leaderboard (day, week) and biggest winners (day, week);
//!    - recently closed markets whose winning token was a longshot → holders
//!      ([`signals::LongshotHit`]);
//!    - Deposit Wallet Factory logs since `cursors.factory_last_block` (→ proxy_owner map);
//!    - `--seed` addresses and current watchlist members (re-score).
//! 2. Drop wallets evaluated within `reeval_ttl_s` (except seeds/watchlist), cap `max_enrich`.
//! 3. Enrich: user-stats (gate) → positions ×4 statuses → market metadata (cache) →
//!    provisional score → price paths for PreMove → final score → profile for scorers.
//! 4. Link + cluster, apply cluster bonus.
//! 5. Persist: wallets scoring ≥ `site.min_score`, watchlist admission
//!    (≥ `tracker.watchlist_min_score`, cap `watchlist_max`, keep highest), clusters,
//!    markets cache, cursors, evaluated set. Nothing is written with `dry_run`.
//!
//! STUB (WP0): signatures are final, bodies are filled in by WP4.

use serde::{Deserialize, Serialize};
use store::DataDir;
use types::api::{PolyApi, RequestStats};
use types::site::CandidateRow;
use types::state::{EvaluatedSet, ProxyOwnerMap};
use types::{Addr, Config};

#[derive(Debug, Clone, Default)]
pub struct DiscoverOpts {
    pub dry_run: bool,
    /// Extra addresses to evaluate regardless of TTL (e.g. legacy seed list).
    pub seed: Vec<Addr>,
    /// `(i, n)`: only enrich wallets whose address hash mod n == i.
    pub shard: Option<(u32, u32)>,
    pub now: i64,
}

/// Large state kept as release assets (loaded/saved by the caller via `store::bin`).
#[derive(Debug, Clone, Default)]
pub struct StateAssets {
    pub evaluated: EvaluatedSet,
    pub proxy_owner: ProxyOwnerMap,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryReport {
    pub feed_pages: u32,
    pub feed_rows: u64,
    pub feed_truncated: bool,
    pub feed_lag_s: i64,
    pub sources: std::collections::BTreeMap<String, u32>,
    pub prefiltered: u32,
    pub enriched: u32,
    pub scored: u32,
    pub flagged_medium: u32,
    pub flagged_high: u32,
    pub watchlist_added: u32,
    pub watchlist_size: u32,
    pub clusters: u32,
    pub new_factory_wallets: u32,
    pub duration_s: f64,
    pub requests: RequestStats,
    /// Top 25 by score, for the dry-run report and the job summary.
    pub top: Vec<CandidateRow>,
}

pub async fn run(
    _api: &dyn PolyApi,
    _data: &DataDir,
    _assets: &mut StateAssets,
    _cfg: &Config,
    _opts: &DiscoverOpts,
) -> anyhow::Result<DiscoveryReport> {
    todo!("WP4")
}
