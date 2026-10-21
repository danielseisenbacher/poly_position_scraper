//! GitHub issue alerts — one issue per wallet, updated over time.
//!
//! - [`sync`]: after discovery. For each wallet with score ≥ `issue_min_score`:
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
//!
//! STUB (WP0): signatures are final, bodies are filled in by WP6.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use store::DataDir;
use types::{Addr, Config, TradeEvent, Wallet};

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
    async fn create_issue(&self, title: &str, body: &str, labels: &[String]) -> anyhow::Result<u64>;
    async fn update_issue(&self, number: u64, update: &IssueUpdate) -> anyhow::Result<()>;
    async fn comment(&self, number: u64, body: &str) -> anyhow::Result<()>;
    /// All issues with `label` in `state` (`open`/`closed`/`all`), following pagination.
    async fn list_issues(&self, label: &str, state: &str) -> anyhow::Result<Vec<IssueSummary>>;
    /// Create the label if missing (ignore "already exists").
    async fn ensure_label(&self, name: &str, color: &str, description: &str) -> anyhow::Result<()>;
}

/// REST client for `api.github.com` (or `base_url` in tests) using `GITHUB_TOKEN`.
pub struct GitHubClient {
    _token: String,
    _repo: String,
    _base_url: String,
}

impl GitHubClient {
    pub fn new(token: String, repo: String) -> Self {
        Self::with_base_url(token, repo, "https://api.github.com".into())
    }
    pub fn with_base_url(token: String, repo: String, base_url: String) -> Self {
        GitHubClient { _token: token, _repo: repo, _base_url: base_url }
    }
}

#[async_trait]
impl GitHubApi for GitHubClient {
    async fn create_issue(&self, _title: &str, _body: &str, _labels: &[String]) -> anyhow::Result<u64> {
        todo!("WP6")
    }
    async fn update_issue(&self, _number: u64, _update: &IssueUpdate) -> anyhow::Result<()> {
        todo!("WP6")
    }
    async fn comment(&self, _number: u64, _body: &str) -> anyhow::Result<()> {
        todo!("WP6")
    }
    async fn list_issues(&self, _label: &str, _state: &str) -> anyhow::Result<Vec<IssueSummary>> {
        todo!("WP6")
    }
    async fn ensure_label(&self, _name: &str, _color: &str, _description: &str) -> anyhow::Result<()> {
        todo!("WP6")
    }
}

#[derive(Debug, Clone, Default)]
pub struct AlertOpts {
    pub dry_run: bool,
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    Open { addr: Addr, title: String },
    Comment { addr: Addr, issue: u64, reason: String },
    Close { addr: Addr, issue: u64 },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AlertReport {
    /// Everything done (or, with `dry_run`, planned).
    pub actions: Vec<Action>,
    /// Actions dropped because of per-run caps.
    pub capped: u32,
}

/// Title, markdown body and labels for a wallet's issue.
pub fn render_issue(_wallet: &Wallet, _cfg: &Config) -> (String, String, Vec<String>) {
    todo!("WP6")
}

/// Markdown for a big-trade comment.
pub fn render_trade_comment(_event: &TradeEvent, _cfg: &Config) -> String {
    todo!("WP6")
}

pub async fn sync(_gh: &dyn GitHubApi, _data: &DataDir, _cfg: &Config, _opts: &AlertOpts) -> anyhow::Result<AlertReport> {
    todo!("WP6")
}

pub async fn comment_trades(
    _gh: &dyn GitHubApi,
    _data: &DataDir,
    _cfg: &Config,
    _events: &[TradeEvent],
    _opts: &AlertOpts,
) -> anyhow::Result<AlertReport> {
    todo!("WP6")
}

/// Returns the number of issues closed.
pub async fn legacy_close(_gh: &dyn GitHubApi, _cfg: &Config, _old_label: &str, _dry_run: bool) -> anyhow::Result<usize> {
    todo!("WP6")
}
