//! HTTP clients for Data API v2, Gamma and Polygon JSON-RPC.
//!
//! [`Client`] implements [`types::api::PolyApi`]. It owns rate limiting (one budget per
//! [`types::api::RateClass`]), retries (429 honouring `Retry-After`, 5xx, transport
//! errors, jittered exponential backoff), the explicit User-Agent, CloudFront
//! cache-busting and request counters.
//!
//! STUB (WP0): signatures are final, bodies are filled in by WP1.

use async_trait::async_trait;
use types::api::*;
use types::config::ApiConfig;
use types::{Addr, MarketMeta, PosStatus, Profile};

pub struct Client {
    _cfg: ApiConfig,
}

impl Client {
    pub fn new(cfg: &ApiConfig) -> Result<Self, ApiError> {
        Ok(Client { _cfg: cfg.clone() })
    }
}

#[async_trait]
impl PolyApi for Client {
    async fn trades_page(&self, _q: &TradesQuery, _cursor: Option<&str>) -> ApiResult<Page<FeedTrade>> {
        todo!("WP1")
    }
    async fn user_trades(&self, _user: &Addr, _start: Option<i64>, _cursor: Option<&str>) -> ApiResult<Page<FeedTrade>> {
        todo!("WP1")
    }
    async fn activity(&self, _q: &ActivityQuery, _cursor: Option<&str>) -> ApiResult<Page<ActivityRow>> {
        todo!("WP1")
    }
    async fn positions(&self, _user: &Addr, _status: PosStatus, _cursor: Option<&str>) -> ApiResult<Page<PositionRow>> {
        todo!("WP1")
    }
    async fn market_positions(&self, _condition_id: &str, _start: Option<i64>, _cursor: Option<&str>) -> ApiResult<Page<PositionRow>> {
        todo!("WP1")
    }
    async fn holders(&self, _condition_id: &str, _include_pnl: bool, _limit: u32) -> ApiResult<Vec<HolderGroup>> {
        todo!("WP1")
    }
    async fn user_stats(&self, _user: &Addr) -> ApiResult<Option<UserStatsRow>> {
        todo!("WP1")
    }
    async fn value(&self, _user: &Addr) -> ApiResult<f64> {
        todo!("WP1")
    }
    async fn leaderboard(&self, _period: TimePeriod, _sort: LeaderboardSort, _cursor: Option<&str>) -> ApiResult<Page<LeaderboardRow>> {
        todo!("WP1")
    }
    async fn biggest_winners(&self, _period: TimePeriod, _cursor: Option<&str>) -> ApiResult<Page<BiggestWinnerRow>> {
        todo!("WP1")
    }
    async fn tokens(&self, _condition_id: &str) -> ApiResult<Vec<TokenRow>> {
        todo!("WP1")
    }
    async fn prices_history(&self, _token_id: &str, _start: i64, _end: i64) -> ApiResult<Vec<PricePoint>> {
        todo!("WP1")
    }
    async fn status(&self) -> ApiResult<ApiStatus> {
        todo!("WP1")
    }
    async fn recently_closed_markets(&self, _since: i64, _max: usize) -> ApiResult<Vec<MarketMeta>> {
        todo!("WP1")
    }
    async fn markets_by_condition(&self, _condition_ids: &[String]) -> ApiResult<Vec<MarketMeta>> {
        todo!("WP1")
    }
    async fn profile(&self, _addr: &Addr) -> ApiResult<Option<Profile>> {
        todo!("WP1")
    }
    async fn block_number(&self) -> ApiResult<u64> {
        todo!("WP1")
    }
    async fn factory_logs(&self, _from_block: u64, _to_block: u64) -> ApiResult<Vec<FactoryLog>> {
        todo!("WP1")
    }
    fn stats(&self) -> RequestStats {
        RequestStats::default()
    }
}
