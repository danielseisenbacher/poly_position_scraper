//! GitHub issue alerts — one issue per wallet, updated over time.
//!
//! - [`sync`]: after discovery. For each watched wallet with score ≥ `issue_min_score`:
//!   no record in `alerts.json` → open an issue (cap `max_new_issues_per_run`);
//!   record exists and (score rose ≥ `rescore_delta` or a new signal kind appeared) →
//!   comment + update labels/title. Wallets that left the watchlist and whose issue is
//!   open → close with label `resolved`.
//! - [`comment_trades`]: after the tracker, one comment per [`TradeEvent`] on the
//!   wallet's issue (≤ 1 per wallet per `min_comment_interval_s`, ≤ `max_comments_per_run`).
//! - [`legacy_close`]: one-shot, closes every open issue carrying `old_label` and adds
//!   `cfg.alerts.label_legacy`.
//!
//! Issue body ends with the marker `<!-- polywatch:addr=0x… -->`; if `alerts.json` is
//! missing, existing issues are found by searching for it.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use store::DataDir;
use types::{Addr, Config, TradeEvent, Wallet};

mod client;
mod render;
mod run;
#[cfg(test)]
mod tests;

pub use render::{marker, parse_marker, MARKER_PREFIX, SIG_LABEL_MIN};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IssueSummary {
    pub number: u64,
    pub title: String,
    pub state: String,
    pub labels: Vec<String>,
    pub body: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct IssueUpdate {
    pub title: Option<String>,
    pub body: Option<String>,
    pub labels: Option<Vec<String>>,
    /// `"open"` or `"closed"`.
    pub state: Option<String>,
}

/// The subset of the GitHub REST API alerts need. Mock it in tests.
#[async_trait]
pub trait GitHubApi: Send + Sync {
    async fn create_issue(&self, title: &str, body: &str, labels: &[String])
        -> anyhow::Result<u64>;
    async fn update_issue(&self, number: u64, update: &IssueUpdate) -> anyhow::Result<()>;
    async fn comment(&self, number: u64, body: &str) -> anyhow::Result<()>;
    /// All issues with `label` in `state` (`open`/`closed`/`all`), following pagination.
    async fn list_issues(&self, label: &str, state: &str) -> anyhow::Result<Vec<IssueSummary>>;
    /// Create the label if missing (ignore "already exists").
    async fn ensure_label(&self, name: &str, color: &str, description: &str) -> anyhow::Result<()>;
}

/// REST client for `api.github.com` (or `base_url` in tests) using `GITHUB_TOKEN`.
///
/// Rate-limited responses (403/429 with `Retry-After`, an exhausted
/// `x-ratelimit-remaining` or a "secondary rate limit" message) are retried up to 3
/// times. Mutating requests are spaced by `write_delay` (default 1 s), per GitHub's
/// guidance for content-creating calls.
pub struct GitHubClient {
    token: String,
    repo: String,
    base_url: String,
    http: reqwest::Client,
    write_delay: Duration,
    max_retry_wait: Duration,
    last_write: tokio::sync::Mutex<Option<Instant>>,
}

impl GitHubClient {
    pub fn new(token: String, repo: String) -> Self {
        Self::with_base_url(token, repo, "https://api.github.com".into())
    }
    pub fn with_base_url(token: String, repo: String, base_url: String) -> Self {
        GitHubClient {
            token,
            repo,
            base_url: base_url.trim_end_matches('/').to_string(),
            http: client::http_client(),
            write_delay: Duration::from_secs(1),
            max_retry_wait: Duration::from_secs(120),
            last_write: tokio::sync::Mutex::new(None),
        }
    }

    /// Minimum spacing between mutating requests (default 1 s; tests use zero).
    pub fn with_write_delay(mut self, delay: Duration) -> Self {
        self.write_delay = delay;
        self
    }

    /// Longest rate-limit wait we sit out before giving up (default 120 s).
    pub fn with_max_retry_wait(mut self, wait: Duration) -> Self {
        self.max_retry_wait = wait;
        self
    }
}

#[derive(Debug, Clone, Default)]
pub struct AlertOpts {
    pub dry_run: bool,
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Open {
        addr: Addr,
        title: String,
    },
    Comment {
        addr: Addr,
        issue: u64,
        reason: String,
    },
    Close {
        addr: Addr,
        issue: u64,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AlertReport {
    /// Everything done (or, with `dry_run`, planned).
    pub actions: Vec<Action>,
    /// Actions dropped because of per-run caps.
    pub capped: u32,
}

/// Title, markdown body and labels for a wallet's issue.
pub fn render_issue(wallet: &Wallet, cfg: &Config) -> (String, String, Vec<String>) {
    render::issue(wallet, cfg)
}

/// Markdown for a big-trade comment.
pub fn render_trade_comment(event: &TradeEvent, cfg: &Config) -> String {
    render::trade_line(event, cfg)
}

/// Open/update/reopen/close issues to match the scored wallets and the watchlist.
///
/// With `dry_run` nothing is called on GitHub and nothing is written; the report lists
/// the actions a real run would take (a missing `alerts.json` is then not rebuilt from
/// the issue list, since that would need an API call).
pub async fn sync(
    gh: &dyn GitHubApi,
    data: &DataDir,
    cfg: &Config,
    opts: &AlertOpts,
) -> anyhow::Result<AlertReport> {
    let mut st = run::SyncState::new(data.load_all_wallets()?, data.alerts()?, data.watchlist()?);
    let mut report = AlertReport::default();
    let result = run::sync_state(gh, &mut st, cfg, opts, &mut report).await;
    if !opts.dry_run {
        // Persist whatever happened, even after a failure, so the next run does not
        // open duplicates for issues created before the error.
        for w in st
            .wallets
            .iter()
            .filter(|w| st.dirty_wallets.contains(&w.addr))
        {
            data.save_wallet(w)?;
        }
        if st.watchlist_dirty {
            data.save_watchlist(&st.watchlist)?;
        }
        data.save_alerts(&st.alerts)?;
    }
    result?;
    Ok(report)
}

/// Comment big trades on the wallets' issues, one combined comment per wallet.
pub async fn comment_trades(
    gh: &dyn GitHubApi,
    data: &DataDir,
    cfg: &Config,
    events: &[TradeEvent],
    opts: &AlertOpts,
) -> anyhow::Result<AlertReport> {
    if events.is_empty() {
        return Ok(AlertReport::default());
    }
    let mut alerts = data.alerts()?;
    let mut report = AlertReport::default();
    let result = run::comment_state(gh, &mut alerts, cfg, events, opts, &mut report).await;
    if !opts.dry_run
        && report
            .actions
            .iter()
            .any(|a| matches!(a, Action::Comment { .. }))
    {
        data.save_alerts(&alerts)?;
    }
    result?;
    Ok(report)
}

/// Returns the number of issues closed.
pub async fn legacy_close(
    gh: &dyn GitHubApi,
    cfg: &Config,
    old_label: &str,
    dry_run: bool,
) -> anyhow::Result<usize> {
    run::legacy_close(gh, cfg, old_label, dry_run).await
}
