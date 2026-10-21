//! Persistence for polywatch.
//!
//! - [`DataDir`]: the checked-out orphan `data` branch (JSON files, atomic writes).
//! - [`bin`]: compact encodings of the large release-asset state (`evaluated.bin`,
//!   `proxy_owner.bin`), gzip-compressed on disk.
//! - [`archive`]: append-only monthly trade archive (`trades_archive/YYYY-MM.jsonl.gz`).
//! - [`render_site`]: derive the dashboard files from wallets + state.
//! Git commits and release-asset transfers happen in the workflow YAML (`git`, `gh`).
//!
//! STUB (WP0): signatures are final, bodies are filled in by WP2.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use types::api::RequestStats;
use types::state::{AlertsState, Cursors, MarketCache, Watchlist};
use types::{Addr, Cluster, Config, Wallet};

/// Relative paths inside the data directory. The dashboard depends on these names.
pub mod paths {
    pub const META: &str = "meta.json";
    pub const INDEX: &str = "index.json";
    pub const MARKETS: &str = "markets.json";
    pub const CLUSTERS: &str = "clusters.json";
    pub const ACTIVITY_RECENT: &str = "activity_recent.json";
    pub const WATCHLIST: &str = "watchlist.json";
    pub const ALERTS: &str = "alerts.json";
    pub const CURSORS: &str = "cursors.json";
    pub const MARKETS_CACHE: &str = "markets_cache.json";
    pub const WALLETS_DIR: &str = "wallets";

    pub fn wallet(addr: &types::Addr) -> String {
        format!("{WALLETS_DIR}/{}.json", addr.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error on {path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("json error in {path}: {source}")]
    Json { path: String, source: serde_json::Error },
    #[error("corrupt state file {path}: {msg}")]
    Corrupt { path: String, msg: String },
    #[error("command `{cmd}` failed: {msg}")]
    Command { cmd: String, msg: String },
}

pub type StoreResult<T> = Result<T, StoreError>;

/// A checked-out `data` branch (or any directory with the same layout).
#[derive(Debug, Clone)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    /// Opens (and creates, including `wallets/`) the directory.
    pub fn open(root: impl Into<PathBuf>) -> StoreResult<Self> {
        let _ = root.into();
        todo!("WP2")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `Ok(None)` if the file does not exist.
    pub fn read<T: DeserializeOwned>(&self, _rel: &str) -> StoreResult<Option<T>> {
        todo!("WP2")
    }

    /// Atomic (write temp file + rename). Compact JSON; deterministic key order
    /// comes from the types (BTreeMap / struct order) so diffs stay small.
    pub fn write<T: Serialize>(&self, _rel: &str, _value: &T) -> StoreResult<()> {
        todo!("WP2")
    }

    pub fn remove(&self, _rel: &str) -> StoreResult<()> {
        todo!("WP2")
    }

    pub fn load_wallet(&self, _addr: &Addr) -> StoreResult<Option<Wallet>> {
        todo!("WP2")
    }

    pub fn save_wallet(&self, _wallet: &Wallet) -> StoreResult<()> {
        todo!("WP2")
    }

    pub fn remove_wallet(&self, _addr: &Addr) -> StoreResult<()> {
        todo!("WP2")
    }

    /// Addresses of every `wallets/*.json`, sorted.
    pub fn wallet_addrs(&self) -> StoreResult<Vec<Addr>> {
        todo!("WP2")
    }

    pub fn load_all_wallets(&self) -> StoreResult<Vec<Wallet>> {
        todo!("WP2")
    }

    // Typed shortcuts; missing files yield `Default`.
    pub fn watchlist(&self) -> StoreResult<Watchlist> {
        todo!("WP2")
    }
    pub fn save_watchlist(&self, _w: &Watchlist) -> StoreResult<()> {
        todo!("WP2")
    }
    pub fn alerts(&self) -> StoreResult<AlertsState> {
        todo!("WP2")
    }
    pub fn save_alerts(&self, _a: &AlertsState) -> StoreResult<()> {
        todo!("WP2")
    }
    pub fn cursors(&self) -> StoreResult<Cursors> {
        todo!("WP2")
    }
    pub fn save_cursors(&self, _c: &Cursors) -> StoreResult<()> {
        todo!("WP2")
    }
    pub fn markets_cache(&self) -> StoreResult<MarketCache> {
        todo!("WP2")
    }
    pub fn save_markets_cache(&self, _m: &MarketCache) -> StoreResult<()> {
        todo!("WP2")
    }
    pub fn clusters(&self) -> StoreResult<Vec<Cluster>> {
        todo!("WP2")
    }
    pub fn save_clusters(&self, _c: &[Cluster]) -> StoreResult<()> {
        todo!("WP2")
    }
}

/// Inputs for `meta.json` that only the caller knows.
#[derive(Debug, Clone, Default)]
pub struct MetaInput {
    pub now: i64,
    pub run_id: Option<String>,
    pub api: RequestStats,
    pub feed_lag_s: i64,
    pub feed_truncated: bool,
    pub wallets_scanned: u32,
    pub trades_tracked: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SiteSummary {
    pub candidates: usize,
    pub markets: usize,
    pub tape_rows: usize,
    pub bytes_written: u64,
}

/// Rebuild `index.json`, `markets.json`, `activity_recent.json` and `meta.json` from
/// `wallets/*`, `watchlist.json`, `clusters.json`, `cursors.json` and `markets_cache.json`.
/// Idempotent. Enforces the caps in `cfg.site` / `cfg.tracker`.
pub fn render_site(_dir: &DataDir, _cfg: &Config, _meta: &MetaInput) -> StoreResult<SiteSummary> {
    todo!("WP2")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CompactReport {
    pub wallets_removed: usize,
    pub wallets_kept: usize,
}

/// Delete wallet files that are not on the watchlist, have no open alert and were
/// last scored more than 30 days ago (or score below `cfg.site.min_score`).
pub fn compact(_dir: &DataDir, _cfg: &Config, _now: i64) -> StoreResult<CompactReport> {
    todo!("WP2")
}

/// Binary encodings for release-asset state. Files are gzip-compressed; a missing
/// file loads as empty. Formats are versioned with a 4-byte magic + u32 version.
pub mod bin {
    use std::path::Path;

    use types::state::{EvaluatedSet, ProxyOwnerMap};

    use super::StoreResult;

    pub fn load_evaluated(_path: &Path) -> StoreResult<EvaluatedSet> {
        todo!("WP2")
    }
    pub fn save_evaluated(_path: &Path, _set: &EvaluatedSet) -> StoreResult<()> {
        todo!("WP2")
    }
    pub fn load_proxy_owner(_path: &Path) -> StoreResult<ProxyOwnerMap> {
        todo!("WP2")
    }
    pub fn save_proxy_owner(_path: &Path, _map: &ProxyOwnerMap) -> StoreResult<()> {
        todo!("WP2")
    }
}

/// Append-only monthly archive of tracked trades: `<dir>/YYYY-MM.jsonl.gz`, one
/// `{"addr":…, …TrackedTrade}` object per line (multi-member gzip so appends are cheap).
pub mod archive {
    use std::path::Path;

    use types::{Addr, TrackedTrade};

    use super::StoreResult;

    pub fn append(_dir: &Path, _trades: &[(Addr, TrackedTrade)]) -> StoreResult<usize> {
        todo!("WP2")
    }
}
