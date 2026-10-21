//! Persistence for polywatch.
//!
//! - [`DataDir`]: the checked-out orphan `data` branch (JSON files, atomic writes).
//! - [`bin`]: compact encodings of the large release-asset state (`evaluated.bin`,
//!   `proxy_owner.bin`), gzip-compressed on disk.
//! - [`archive`]: append-only monthly trade archive (`trades_archive/YYYY-MM.jsonl.gz`).
//! - [`render_site`]: derive the dashboard files from wallets + state.
//!
//! Git commits and release-asset transfers happen in the workflow YAML (`git`, `gh`).

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use types::state::{AlertsState, Cursors, MarketCache, Watchlist};
use types::{Addr, Cluster, Config, Wallet};

pub mod archive;
pub mod bin;
mod site;

pub use site::{render_site, MetaInput, SiteSummary};

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
    /// Name of the gzipped markets cache inside the state directory.
    pub const MARKETS_CACHE_GZ: &str = "markets_cache.json.gz";
    pub const WALLETS_DIR: &str = "wallets";

    pub fn wallet(addr: &types::Addr) -> String {
        format!("{WALLETS_DIR}/{}.json", addr.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error on {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("json error in {path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
    #[error("corrupt state file {path}: {msg}")]
    Corrupt { path: String, msg: String },
    #[error("command `{cmd}` failed: {msg}")]
    Command { cmd: String, msg: String },
}

pub type StoreResult<T> = Result<T, StoreError>;

pub(crate) fn io_err(path: &Path) -> impl FnOnce(io::Error) -> StoreError + '_ {
    move |source| StoreError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Writes `bytes` to `path` atomically: `<dir>/.<name>.tmp`, then rename over the target.
/// Creates missing parent directories.
pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> StoreResult<()> {
    let dir = path
        .parent()
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(io_err(dir))?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&tmp, bytes).map_err(io_err(&tmp))?;
    std::fs::rename(&tmp, path).map_err(io_err(path))
}

/// Reads `path`, `Ok(None)` if it does not exist.
pub(crate) fn read_optional(path: &Path) -> StoreResult<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(path)(e)),
    }
}

/// A checked-out `data` branch (or any directory with the same layout).
///
/// The markets cache is large and not needed by the dashboard: with
/// [`DataDir::with_state_dir`] it lives gzipped in the state directory (a release
/// asset) instead of on the `data` branch.
#[derive(Debug, Clone)]
pub struct DataDir {
    root: PathBuf,
    state: Option<PathBuf>,
}

impl DataDir {
    /// Opens (and creates, including `wallets/`) the directory.
    pub fn open(root: impl Into<PathBuf>) -> StoreResult<Self> {
        let root = root.into();
        let wallets = root.join(paths::WALLETS_DIR);
        std::fs::create_dir_all(&wallets).map_err(io_err(&wallets))?;
        Ok(DataDir { root, state: None })
    }

    /// Keep the markets cache at `<state>/markets_cache.json.gz`.
    pub fn with_state_dir(mut self, state: impl Into<PathBuf>) -> Self {
        self.state = Some(state.into());
        self
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// `Ok(None)` if the file does not exist.
    pub fn read<T: DeserializeOwned>(&self, rel: &str) -> StoreResult<Option<T>> {
        let path = self.root.join(rel);
        let Some(bytes) = read_optional(&path)? else {
            return Ok(None);
        };
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|source| StoreError::Json {
                path: path.display().to_string(),
                source,
            })
    }

    /// Atomic (write temp file + rename). Compact JSON; deterministic key order
    /// comes from the types (BTreeMap / struct order) so diffs stay small.
    pub fn write<T: Serialize>(&self, rel: &str, value: &T) -> StoreResult<()> {
        self.write_counted(rel, value).map(|_| ())
    }

    /// [`DataDir::write`], returning the number of bytes written.
    pub fn write_counted<T: Serialize>(&self, rel: &str, value: &T) -> StoreResult<u64> {
        let path = self.root.join(rel);
        let bytes = serde_json::to_vec(value).map_err(|source| StoreError::Json {
            path: path.display().to_string(),
            source,
        })?;
        atomic_write(&path, &bytes)?;
        Ok(bytes.len() as u64)
    }

    /// Removes a file; a missing file is not an error.
    pub fn remove(&self, rel: &str) -> StoreResult<()> {
        let path = self.root.join(rel);
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(io_err(&path)(e)),
            _ => Ok(()),
        }
    }

    pub fn load_wallet(&self, addr: &Addr) -> StoreResult<Option<Wallet>> {
        self.read(&paths::wallet(addr))
    }

    pub fn save_wallet(&self, wallet: &Wallet) -> StoreResult<()> {
        self.write(&paths::wallet(&wallet.addr), wallet)
    }

    pub fn remove_wallet(&self, addr: &Addr) -> StoreResult<()> {
        self.remove(&paths::wallet(addr))
    }

    /// Addresses of every `wallets/*.json`, sorted.
    pub fn wallet_addrs(&self) -> StoreResult<Vec<Addr>> {
        let dir = self.root.join(paths::WALLETS_DIR);
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_err(&dir)(e)),
        };
        let mut out = Vec::new();
        for entry in entries {
            let name = entry.map_err(io_err(&dir))?.file_name();
            let Some(stem) = name.to_str().and_then(|n| n.strip_suffix(".json")) else {
                continue;
            };
            // Only canonical (lowercase) names: anything else would not round-trip via `paths::wallet`.
            if let Some(addr) = Addr::parse(stem).ok().filter(|a| a.as_str() == stem) {
                out.push(addr);
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn load_all_wallets(&self) -> StoreResult<Vec<Wallet>> {
        let mut out = Vec::new();
        for addr in self.wallet_addrs()? {
            out.extend(self.load_wallet(&addr)?);
        }
        Ok(out)
    }

    fn read_or_default<T: DeserializeOwned + Default>(&self, rel: &str) -> StoreResult<T> {
        self.read(rel).map(Option::unwrap_or_default)
    }

    // Typed shortcuts; missing files yield `Default`.
    pub fn watchlist(&self) -> StoreResult<Watchlist> {
        self.read_or_default(paths::WATCHLIST)
    }
    pub fn save_watchlist(&self, w: &Watchlist) -> StoreResult<()> {
        self.write(paths::WATCHLIST, w)
    }
    pub fn alerts(&self) -> StoreResult<AlertsState> {
        self.read_or_default(paths::ALERTS)
    }
    pub fn save_alerts(&self, a: &AlertsState) -> StoreResult<()> {
        self.write(paths::ALERTS, a)
    }
    pub fn cursors(&self) -> StoreResult<Cursors> {
        self.read_or_default(paths::CURSORS)
    }
    pub fn save_cursors(&self, c: &Cursors) -> StoreResult<()> {
        self.write(paths::CURSORS, c)
    }
    pub fn markets_cache(&self) -> StoreResult<MarketCache> {
        let Some(state) = &self.state else {
            return self.read_or_default(paths::MARKETS_CACHE);
        };
        let path = state.join(paths::MARKETS_CACHE_GZ);
        let Some(bytes) = read_optional(&path)? else {
            return Ok(MarketCache::default());
        };
        let mut json = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&bytes[..]), &mut json)
            .map_err(|e| StoreError::Corrupt {
                path: path.display().to_string(),
                msg: e.to_string(),
            })?;
        serde_json::from_slice(&json).map_err(|source| StoreError::Json {
            path: path.display().to_string(),
            source,
        })
    }
    pub fn save_markets_cache(&self, m: &MarketCache) -> StoreResult<()> {
        let Some(state) = &self.state else {
            return self.write(paths::MARKETS_CACHE, m);
        };
        let path = state.join(paths::MARKETS_CACHE_GZ);
        let json = serde_json::to_vec(m).map_err(|source| StoreError::Json {
            path: path.display().to_string(),
            source,
        })?;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, &json).map_err(io_err(&path))?;
        let bytes = gz.finish().map_err(io_err(&path))?;
        atomic_write(&path, &bytes)
    }
    pub fn clusters(&self) -> StoreResult<Vec<Cluster>> {
        self.read_or_default(paths::CLUSTERS)
    }
    pub fn save_clusters(&self, c: &[Cluster]) -> StoreResult<()> {
        self.write(paths::CLUSTERS, &c)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CompactReport {
    pub wallets_removed: usize,
    pub wallets_kept: usize,
}

/// Wallet files older than this (by `last_scored`) are eligible for compaction.
const COMPACT_MAX_AGE_S: i64 = 30 * 86_400;

/// Delete wallet files that are not on the watchlist, have no open alert and were
/// last scored more than 30 days ago (or score below `cfg.site.min_score`).
pub fn compact(dir: &DataDir, cfg: &Config, now: i64) -> StoreResult<CompactReport> {
    let watched: HashSet<Addr> = dir
        .watchlist()?
        .entries
        .into_iter()
        .map(|e| e.addr)
        .collect();
    let alerts = dir.alerts()?;
    let mut rep = CompactReport::default();
    for wallet in dir.load_all_wallets()? {
        let protected = watched.contains(&wallet.addr)
            || alerts.wallets.get(&wallet.addr).is_some_and(|r| !r.closed);
        let stale =
            wallet.last_scored < now - COMPACT_MAX_AGE_S || wallet.score.total < cfg.site.min_score;
        if !protected && stale {
            dir.remove_wallet(&wallet.addr)?;
            rep.wallets_removed += 1;
        } else {
            rep.wallets_kept += 1;
        }
    }
    Ok(rep)
}

#[cfg(test)]
mod tests;
