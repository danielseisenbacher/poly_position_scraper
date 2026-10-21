//! Every tunable. Defaults are the plan's values; `polywatch.toml` at the repo root
//! overrides any subset (all structs are `#[serde(default)]`).

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub repo: RepoConfig,
    pub api: ApiConfig,
    pub discovery: DiscoveryConfig,
    pub signals: SignalConfig,
    pub linking: LinkingConfig,
    pub tracker: TrackerConfig,
    pub alerts: AlertConfig,
    pub site: SiteConfig,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("reading {0}: {1}")]
    Io(String, std::io::Error),
    #[error("parsing {0}: {1}")]
    Parse(String, toml::de::Error),
}

impl Config {
    /// Defaults overlaid with `path` if it exists. A missing file is not an error.
    pub fn load(path: &Path) -> Result<Config, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).map_err(|e| ConfigError::Parse(path.display().to_string(), e)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(ConfigError::Io(path.display().to_string(), e)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RepoConfig {
    /// `owner/name`, used for links and the GitHub API.
    pub slug: String,
    pub data_branch: String,
    pub dashboard_url: String,
}

impl Default for RepoConfig {
    fn default() -> Self {
        RepoConfig {
            slug: "danielseisenbacher/poly_position_scraper".into(),
            data_branch: "data".into(),
            dashboard_url: "https://danielseisenbacher.github.io/poly_position_scraper/".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiConfig {
    pub data_base: String,
    pub gamma_base: String,
    pub rpc_url: String,
    pub user_agent: String,
    /// Requests per 10 s we allow ourselves per class: a safety margin below the published limits.
    pub data_per_10s: u32,
    pub trades_per_10s: u32,
    pub positions_per_10s: u32,
    pub gamma_per_10s: u32,
    pub rpc_per_10s: u32,
    pub max_concurrency: usize,
    pub max_retries: u32,
    pub timeout_s: u64,
    /// Max block span per `eth_getLogs` call.
    pub rpc_log_chunk: u64,
    pub factory_address: String,
    pub factory_topic0: String,
}

impl Default for ApiConfig {
    fn default() -> Self {
        ApiConfig {
            data_base: "https://data-api.polymarket.com/v2".into(),
            gamma_base: "https://gamma-api.polymarket.com".into(),
            rpc_url: "https://polygon-bor-rpc.publicnode.com".into(),
            user_agent: "polywatch/0.1 (+https://github.com/danielseisenbacher/poly_position_scraper)".into(),
            data_per_10s: 600,
            trades_per_10s: 200,
            positions_per_10s: 150,
            gamma_per_10s: 250,
            rpc_per_10s: 20,
            max_concurrency: 16,
            max_retries: 5,
            timeout_s: 30,
            rpc_log_chunk: 2000,
            factory_address: "0x00000000000Fb5C9ADea0298D729A0CB3823Cc07".into(),
            factory_topic0: "0x7441de0ad639fe5d2bf1c22447715a0528b682385736bb40ae8dd92555eb8276".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiscoveryConfig {
    /// Stop paging the global feed after this many pages (sets `feed_truncated`).
    pub feed_max_pages: u32,
    /// First run (no cursor): how far back to read the feed.
    pub feed_initial_lookback_s: i64,
    /// Feed pre-filter: minimum buy volume in the window per wallet.
    pub prefilter_min_buy_usdc: f64,
    /// Feed pre-filter: mean buy price at or below this, or…
    pub prefilter_max_mean_buy_price: f64,
    /// …at least this many distinct markets bought.
    pub prefilter_min_markets: u32,
    /// Don't re-enrich a wallet evaluated within this window.
    pub reeval_ttl_s: i64,
    /// Hard cap on wallets enriched per run (API budget).
    pub max_enrich: usize,
    /// Stage-A gate after user-stats: drop wallets with fewer trades than this…
    pub min_trade_count: u64,
    /// Leaderboard depth per (period, sort) pull.
    pub leaderboard_depth: u32,
    /// Longshot-winner source: max recently-closed markets inspected per run.
    pub max_resolved_markets: usize,
    /// Longshot-winner source: winning token must have traded ≤ this in the window before close.
    pub longshot_price: f64,
    pub longshot_window_s: i64,
    /// Ignore markets shorter than this (crypto up/down 5–15 min series are noise).
    pub min_market_duration_h: f64,
    /// Scan factory logs for new wallets (for linking + NewWallet source).
    pub scan_factory: bool,
    /// Rolling window kept in `proxy_owner.bin`.
    pub proxy_owner_retention_s: i64,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        DiscoveryConfig {
            feed_max_pages: 600,
            feed_initial_lookback_s: 4 * 3600,
            prefilter_min_buy_usdc: 200.0,
            prefilter_max_mean_buy_price: 0.35,
            prefilter_min_markets: 3,
            reeval_ttl_s: 14 * 86_400,
            max_enrich: 2000,
            min_trade_count: 1,
            leaderboard_depth: 500,
            max_resolved_markets: 50,
            longshot_price: 0.30,
            longshot_window_s: 7 * 86_400,
            min_market_duration_h: 24.0,
            scan_factory: true,
            proxy_owner_retention_s: 180 * 86_400,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SignalConfig {
    // Weights (sum of weight × strength, capped at 100).
    pub w_new_account_longshot: f64,
    pub w_old_account_win_rate: f64,
    pub w_leaderboard_entrant: f64,
    pub w_longshot_winner: f64,
    pub w_concentration: f64,
    pub w_resolution_timing: f64,
    pub w_pre_move: f64,
    pub w_niche_market: f64,
    pub w_linked_accounts: f64,

    pub tier_high: u8,
    pub tier_medium: u8,

    /// Positions below this value are dust and ignored; also the global value floor.
    pub min_value_usd: f64,
    /// Open positions with percent_pnl at or below this are wiped out and ignored
    /// for value/concentration (they still count as losses in the win rate).
    pub dust_pnl_pct: f64,
    /// Positions in markets shorter than this (crypto up/down series) are ignored entirely.
    pub min_market_duration_h: f64,

    // NewAccountLongshot
    pub new_account_days: f64,
    pub longshot_price: f64,
    pub max_markets_new: u32,

    // OldAccountWinRate
    pub min_resolved: f64,
    pub min_resolved_selected: f64,
    pub min_longshot_wins: u32,
    pub longshot_win_price: f64,
    pub z_floor: f64,
    pub z_span: f64,
    pub outcome_selected_z_penalty: f64,
    /// Multiplier for sports/crypto wins (public-information markets).
    pub public_info_discount: f64,
    /// Weight of CLOSED (sold before resolution) positions in the win-rate count.
    pub closed_weight: f64,

    // LeaderboardEntrant
    pub leaderboard_max_rank: u32,
    pub leaderboard_max_age_days: f64,
    pub leaderboard_max_markets: u64,
    pub biggest_win_min_multiple: f64,
    pub biggest_win_min_initial: f64,

    // LongshotWinner
    pub longshot_holder_max_entry: f64,

    // Concentration
    pub concentration_share: f64,
    pub concentration_max_markets: u32,

    // ResolutionTiming
    pub timing_window_h: f64,
    pub timing_max_price: f64,
    pub timing_min_wins: u32,

    // PreMove
    pub premove_window_h: f64,
    pub premove_delta: f64,
    /// Only fetch price history for wallets whose provisional score reaches this.
    pub premove_min_provisional: u8,
    pub premove_max_positions: usize,

    // NicheMarket
    pub niche_max_volume: f64,
    pub niche_min_share: f64,

    // LinkedAccounts
    pub linked_bonus_per_member: f64,
}

impl Default for SignalConfig {
    fn default() -> Self {
        SignalConfig {
            w_new_account_longshot: 25.0,
            w_old_account_win_rate: 30.0,
            w_leaderboard_entrant: 15.0,
            w_longshot_winner: 25.0,
            w_concentration: 10.0,
            w_resolution_timing: 15.0,
            w_pre_move: 15.0,
            w_niche_market: 10.0,
            w_linked_accounts: 20.0,
            tier_high: 70,
            tier_medium: 45,
            min_value_usd: 200.0,
            dust_pnl_pct: -99.0,
            min_market_duration_h: 24.0,
            new_account_days: 60.0,
            longshot_price: 0.30,
            max_markets_new: 6,
            min_resolved: 10.0,
            min_resolved_selected: 15.0,
            min_longshot_wins: 2,
            longshot_win_price: 0.35,
            z_floor: 2.0,
            z_span: 2.5,
            outcome_selected_z_penalty: 0.75,
            public_info_discount: 0.5,
            closed_weight: 0.5,
            leaderboard_max_rank: 500,
            leaderboard_max_age_days: 90.0,
            leaderboard_max_markets: 20,
            biggest_win_min_multiple: 5.0,
            biggest_win_min_initial: 200.0,
            longshot_holder_max_entry: 0.35,
            concentration_share: 0.6,
            concentration_max_markets: 3,
            timing_window_h: 48.0,
            timing_max_price: 0.5,
            timing_min_wins: 2,
            premove_window_h: 24.0,
            premove_delta: 0.25,
            premove_min_provisional: 40,
            premove_max_positions: 5,
            niche_max_volume: 50_000.0,
            niche_min_share: 0.03,
            linked_bonus_per_member: 10.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LinkingConfig {
    pub min_confidence: f64,
    pub conf_same_owner: f64,
    pub conf_same_x: f64,
    pub conf_same_image: f64,
    pub conf_same_name: f64,
    pub conf_sequential_id: f64,
    pub conf_factory_cocreation: f64,
    pub conf_cotrade: f64,
    pub sequential_id_max_gap: u64,
    pub sequential_max_created_gap_s: i64,
    pub factory_cocreation_s: i64,
    pub cotrade_window_s: i64,
    pub cotrade_min_occasions: u32,
}

impl Default for LinkingConfig {
    fn default() -> Self {
        LinkingConfig {
            min_confidence: 0.5,
            conf_same_owner: 0.95,
            conf_same_x: 0.9,
            conf_same_image: 0.8,
            conf_same_name: 0.5,
            conf_sequential_id: 0.6,
            conf_factory_cocreation: 0.4,
            conf_cotrade: 0.7,
            sequential_id_max_gap: 50,
            sequential_max_created_gap_s: 600,
            factory_cocreation_s: 60,
            cotrade_window_s: 60,
            cotrade_min_occasions: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackerConfig {
    pub watchlist_min_score: u8,
    pub watchlist_max: usize,
    /// Evict after this long below `watchlist_min_score`.
    pub evict_low_score_s: i64,
    /// Evict after this long without any activity.
    pub evict_inactive_s: i64,
    /// Stop polling when the job has used this much wall time (leave room for commit/push).
    pub time_budget_s: u64,
    pub max_trades_per_wallet: usize,
    pub tape_window_s: i64,
    pub tape_max_rows: usize,
    /// Comment on the wallet's issue for trades at or above this…
    pub big_trade_usd: f64,
    /// …or at or above this share of the wallet's value.
    pub big_trade_share: f64,
    /// Refresh the positions snapshot of wallets that traded this run.
    pub refresh_positions: bool,
    /// Cadence, only used for "stale" badges on the dashboard.
    pub interval_min: u32,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        TrackerConfig {
            watchlist_min_score: 45,
            watchlist_max: 1500,
            evict_low_score_s: 14 * 86_400,
            evict_inactive_s: 60 * 86_400,
            time_budget_s: 150,
            max_trades_per_wallet: 500,
            tape_window_s: 48 * 3600,
            tape_max_rows: 5000,
            big_trade_usd: 500.0,
            big_trade_share: 0.25,
            refresh_positions: true,
            interval_min: 8,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertConfig {
    pub issue_min_score: u8,
    pub max_new_issues_per_run: usize,
    pub max_comments_per_run: usize,
    pub min_comment_interval_s: i64,
    /// Comment (instead of a new issue) when the score rises by at least this.
    pub rescore_delta: u8,
    pub label_candidate: String,
    pub label_legacy: String,
    pub label_resolved: String,
}

impl Default for AlertConfig {
    fn default() -> Self {
        AlertConfig {
            issue_min_score: 60,
            max_new_issues_per_run: 30,
            max_comments_per_run: 20,
            min_comment_interval_s: 3600,
            rescore_delta: 15,
            label_candidate: "insider-candidate".into(),
            label_legacy: "legacy".into(),
            label_resolved: "resolved".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SiteConfig {
    pub max_candidates: usize,
    pub max_markets: usize,
    /// Include wallets scoring at least this in `index.json`.
    pub min_score: u8,
}

impl Default for SiteConfig {
    fn default() -> Self {
        SiteConfig { max_candidates: 1500, max_markets: 300, min_score: 30 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_toml_overrides_defaults() {
        let c: Config = toml::from_str("[tracker]\nwatchlist_max = 10\n").unwrap();
        assert_eq!(c.tracker.watchlist_max, 10);
        assert_eq!(c.tracker.watchlist_min_score, 45);
        assert_eq!(c.signals, SignalConfig::default());
    }

    #[test]
    fn missing_file_is_default() {
        let c = Config::load(Path::new("/nonexistent/polywatch.toml")).unwrap();
        assert_eq!(c, Config::default());
    }
}
