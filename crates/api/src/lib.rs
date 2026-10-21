//! HTTP clients for Data API v2, Gamma and Polygon JSON-RPC.
//!
//! [`Client`] implements [`types::api::PolyApi`]. It owns rate limiting (one budget per
//! [`types::api::RateClass`]), retries (429 honouring `Retry-After`, 5xx, transport
//! errors, jittered exponential backoff), the explicit User-Agent, CloudFront
//! cache-busting and request counters.

mod data;
mod gamma;
mod http;
mod limiter;
mod rpc;
#[cfg(test)]
mod tests;

use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Semaphore;
use types::api::*;
use types::config::ApiConfig;
use types::{Addr, MarketMeta, PosStatus, Profile};

use crate::limiter::Limiter;

/// Rate-limited client for every external API polywatch reads. Cheap to share by reference.
pub struct Client {
    cfg: ApiConfig,
    http: reqwest::Client,
    sem: Semaphore,
    limiter: Limiter,
    stats: Mutex<RequestStats>,
    backoff_base: Duration,
}

impl Client {
    pub fn new(cfg: &ApiConfig) -> Result<Self, ApiError> {
        let http = reqwest::Client::builder()
            .user_agent(cfg.user_agent.clone())
            .gzip(true)
            .timeout(Duration::from_secs(cfg.timeout_s.max(1)))
            .build()
            .map_err(|e| ApiError::Transport {
                url: String::new(),
                msg: format!("building HTTP client: {e}"),
            })?;
        Ok(Client {
            cfg: cfg.clone(),
            http,
            sem: Semaphore::new(cfg.max_concurrency.max(1)),
            limiter: Limiter::from_config(cfg),
            stats: Mutex::new(RequestStats::default()),
            backoff_base: Duration::from_millis(500),
        })
    }
}

/// Combo/synthetic condition ids, which per-market endpoints reject with 400.
///
/// Recorded combo ids are 64 characters (`0x` + 62 hex), which
/// [`types::api::is_synthetic_condition`] (expects 66) misses, so match the zero run alone.
pub(crate) fn is_synthetic(condition_id: &str) -> bool {
    is_synthetic_condition(condition_id) || condition_id.ends_with("00000000000000000000")
}

#[async_trait]
impl PolyApi for Client {
    async fn trades_page(
        &self,
        q: &TradesQuery,
        cursor: Option<&str>,
    ) -> ApiResult<Page<FeedTrade>> {
        self.trades_page_impl(q, cursor).await
    }
    async fn user_trades(
        &self,
        user: &Addr,
        start: Option<i64>,
        cursor: Option<&str>,
    ) -> ApiResult<Page<FeedTrade>> {
        self.user_trades_impl(user, start, cursor).await
    }
    async fn activity(
        &self,
        q: &ActivityQuery,
        cursor: Option<&str>,
    ) -> ApiResult<Page<ActivityRow>> {
        self.activity_impl(q, cursor).await
    }
    async fn positions(
        &self,
        user: &Addr,
        status: PosStatus,
        cursor: Option<&str>,
    ) -> ApiResult<Page<PositionRow>> {
        self.positions_impl(user, status, cursor).await
    }
    async fn market_positions(
        &self,
        condition_id: &str,
        status: Option<PosStatus>,
        start: Option<i64>,
        cursor: Option<&str>,
    ) -> ApiResult<Page<PositionRow>> {
        self.market_positions_impl(condition_id, status, start, cursor)
            .await
    }
    async fn holders(
        &self,
        condition_id: &str,
        include_pnl: bool,
        limit: u32,
    ) -> ApiResult<Vec<HolderGroup>> {
        self.holders_impl(condition_id, include_pnl, limit).await
    }
    async fn user_stats(&self, user: &Addr) -> ApiResult<Option<UserStatsRow>> {
        self.user_stats_impl(user).await
    }
    async fn value(&self, user: &Addr) -> ApiResult<f64> {
        self.value_impl(user).await
    }
    async fn leaderboard(
        &self,
        period: TimePeriod,
        sort: LeaderboardSort,
        cursor: Option<&str>,
    ) -> ApiResult<Page<LeaderboardRow>> {
        self.leaderboard_impl(period, sort, cursor).await
    }
    async fn biggest_winners(
        &self,
        period: TimePeriod,
        cursor: Option<&str>,
    ) -> ApiResult<Page<BiggestWinnerRow>> {
        self.biggest_winners_impl(period, cursor).await
    }
    async fn tokens(&self, condition_id: &str) -> ApiResult<Vec<TokenRow>> {
        self.tokens_impl(condition_id).await
    }
    async fn prices_history(
        &self,
        token_id: &str,
        start: i64,
        end: i64,
    ) -> ApiResult<Vec<PricePoint>> {
        self.prices_history_impl(token_id, start, end).await
    }
    async fn status(&self) -> ApiResult<ApiStatus> {
        self.status_impl().await
    }
    async fn recently_closed_markets(
        &self,
        since: i64,
        min_volume: f64,
        max: usize,
    ) -> ApiResult<Vec<MarketMeta>> {
        self.recently_closed_markets_impl(since, min_volume, max)
            .await
    }
    async fn markets_by_condition(&self, condition_ids: &[String]) -> ApiResult<Vec<MarketMeta>> {
        self.markets_by_condition_impl(condition_ids).await
    }
    async fn profile(&self, addr: &Addr) -> ApiResult<Option<Profile>> {
        self.profile_impl(addr).await
    }
    async fn block_number(&self) -> ApiResult<u64> {
        self.block_number_impl().await
    }
    async fn factory_logs(&self, from_block: u64, to_block: u64) -> ApiResult<Vec<FactoryLog>> {
        self.factory_logs_impl(from_block, to_block).await
    }
    /// Counters since creation. `requests` counts every HTTP attempt (retries included)
    /// under its primary class; `/trades`, `/positions` and `/activity` are not double-counted
    /// under `Data` even though they also consume that budget.
    fn stats(&self) -> RequestStats {
        self.stats.lock().expect("stats mutex poisoned").clone()
    }
}
