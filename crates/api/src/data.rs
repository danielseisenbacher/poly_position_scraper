//! Data API v2: `{data, pagination}` envelopes and per-endpoint query strings.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use types::api::*;
use types::{Addr, PosStatus};

use crate::http::now_s;
use crate::Client;

pub(crate) const MAX_LIMIT: u32 = 1000;
const BIGGEST_WINNERS_LIMIT: u32 = 500;
const HOLDERS_PNL_LIMIT: u32 = 100;
const PRICES_LIMIT: u32 = 10_000;
/// `/v2/prices-history` rejects explicit windows longer than this.
const PRICES_MAX_SPAN_S: i64 = 15 * 86_400;

#[derive(Deserialize)]
struct Envelope<T> {
    data: T,
    #[serde(default)]
    pagination: Option<Pagination>,
}

#[derive(Deserialize)]
struct Pagination {
    #[serde(default)]
    next_cursor: Option<String>,
    #[serde(default)]
    has_more: Option<bool>,
}

impl Pagination {
    fn cursor(self) -> Option<String> {
        if self.has_more == Some(false) {
            return None;
        }
        self.next_cursor.filter(|c| !c.is_empty())
    }
}

#[derive(Deserialize)]
struct ValueData {
    #[serde(default)]
    value: Option<f64>,
}

#[derive(Deserialize)]
struct StatusData {
    #[serde(default)]
    serving: Option<Serving>,
    #[serde(default)]
    ingestion: Option<Ingestion>,
}

#[derive(Deserialize)]
struct Serving {
    #[serde(default)]
    lag_seconds: i64,
}

#[derive(Deserialize)]
struct Ingestion {
    #[serde(default)]
    max_synced_block: Option<u64>,
}

/// Which rate bucket a Data API path belongs to.
fn class_of(path: &str) -> RateClass {
    match path {
        "/trades" => RateClass::Trades,
        "/positions" | "/activity" => RateClass::Positions,
        _ => RateClass::Data,
    }
}

/// `0` means "as many as allowed".
fn clamp_limit(limit: u32, max: u32) -> u32 {
    if limit == 0 {
        max
    } else {
        limit.min(max)
    }
}

/// Serde name of a unit enum variant (`TimePeriod::Week` → `week`).
fn enum_param<T: Serialize>(v: T) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        _ => String::new(),
    }
}

/// A `filter_amount` below every real trade that changes every second, so repeated
/// polls of the first feed page never hit CloudFront's 300 s cache.
pub(crate) fn cache_buster_amount(unix_s: i64) -> String {
    format!("{:.9}", 0.000_001 + (unix_s.rem_euclid(1000)) as f64 * 1e-9)
}

type Query = Vec<(&'static str, String)>;

fn push_cursor(q: &mut Query, cursor: Option<&str>) {
    if let Some(c) = cursor {
        q.push(("cursor", c.to_string()));
    }
}

impl Client {
    async fn data_get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> ApiResult<(T, Option<String>)> {
        let url = format!("{}{path}", self.cfg.data_base.trim_end_matches('/'));
        let req = self.build_get(&url, query)?;
        let env: Envelope<T> = self.get_json(class_of(path), req).await?;
        Ok((env.data, env.pagination.and_then(Pagination::cursor)))
    }

    async fn data_page<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> ApiResult<Page<T>> {
        let (data, next_cursor): (Option<Vec<T>>, _) = self.data_get(path, query).await?;
        Ok(Page {
            data: data.unwrap_or_default(),
            next_cursor,
        })
    }

    pub(crate) async fn trades_page_impl(
        &self,
        q: &TradesQuery,
        cursor: Option<&str>,
    ) -> ApiResult<Page<FeedTrade>> {
        let mut query: Query = vec![
            ("limit", clamp_limit(q.limit, MAX_LIMIT).to_string()),
            ("taker_only", q.taker_only.to_string()),
            ("filter_type", "CASH".into()),
            (
                "filter_amount",
                q.min_cash
                    .map_or_else(|| cache_buster_amount(now_s()), |m| m.to_string()),
            ),
        ];
        if let Some(side) = q.side {
            query.push(("side", enum_param(side)));
        }
        push_cursor(&mut query, cursor);
        self.data_page("/trades", &query).await
    }

    pub(crate) async fn user_trades_impl(
        &self,
        user: &Addr,
        start: Option<i64>,
        cursor: Option<&str>,
    ) -> ApiResult<Page<FeedTrade>> {
        let mut query: Query = vec![
            ("user", user.to_string()),
            ("taker_only", "false".into()),
            ("limit", MAX_LIMIT.to_string()),
        ];
        if let Some(s) = start {
            query.push(("start", s.to_string()));
        }
        push_cursor(&mut query, cursor);
        self.data_page("/trades", &query).await
    }

    pub(crate) async fn activity_impl(
        &self,
        q: &ActivityQuery,
        cursor: Option<&str>,
    ) -> ApiResult<Page<ActivityRow>> {
        let mut query: Query = vec![
            ("user", q.user.to_string()),
            ("limit", clamp_limit(q.limit, MAX_LIMIT).to_string()),
        ];
        if let Some(s) = q.start {
            query.push(("start", s.to_string()));
        }
        // Always bounded: a fresh `end` makes every poll a distinct URL for CloudFront.
        query.push(("end", q.end.unwrap_or_else(|| now_s() + 60).to_string()));
        push_cursor(&mut query, cursor);
        query.push(("sort_direction", "DESC".into()));
        query.push(("exclude_deposits_withdrawals", "false".into()));
        self.data_page("/activity", &query).await
    }

    pub(crate) async fn positions_impl(
        &self,
        user: &Addr,
        status: PosStatus,
        cursor: Option<&str>,
    ) -> ApiResult<Page<PositionRow>> {
        let mut query: Query = vec![
            ("user", user.to_string()),
            ("status", status.as_param().into()),
            ("limit", MAX_LIMIT.to_string()),
        ];
        push_cursor(&mut query, cursor);
        // `end` bounds last_event_at; a day in the future filters nothing but busts the cache.
        query.push(("end", (now_s() + 86_400).to_string()));
        self.data_page("/positions", &query).await
    }

    pub(crate) async fn market_positions_impl(
        &self,
        condition_id: &str,
        status: Option<PosStatus>,
        start: Option<i64>,
        cursor: Option<&str>,
    ) -> ApiResult<Page<PositionRow>> {
        if crate::is_synthetic(condition_id) {
            return Ok(Page::default());
        }
        let mut query: Query = vec![("condition", condition_id.into())];
        if let Some(s) = status {
            query.push(("status", s.as_param().into()));
        }
        if let Some(s) = start {
            query.push(("start", s.to_string()));
            // Already bounded by `start`, so a future `end` drops nothing and busts the cache.
            query.push(("end", (now_s() + 86_400).to_string()));
        }
        query.push(("limit", MAX_LIMIT.to_string()));
        push_cursor(&mut query, cursor);
        query.push(("sort_by", "TIMESTAMP".into()));
        self.data_page("/positions", &query).await
    }

    pub(crate) async fn holders_impl(
        &self,
        condition_id: &str,
        include_pnl: bool,
        limit: u32,
    ) -> ApiResult<Vec<HolderGroup>> {
        if crate::is_synthetic(condition_id) {
            return Ok(Vec::new());
        }
        let max = if include_pnl {
            HOLDERS_PNL_LIMIT
        } else {
            MAX_LIMIT
        };
        let query: Query = vec![
            ("condition", condition_id.into()),
            ("limit", clamp_limit(limit, max).to_string()),
            ("include_pnl", include_pnl.to_string()),
        ];
        Ok(self.data_page("/holders", &query).await?.data)
    }

    pub(crate) async fn user_stats_impl(&self, user: &Addr) -> ApiResult<Option<UserStatsRow>> {
        Ok(self
            .data_get("/user-stats", &[("user", user.to_string())])
            .await?
            .0)
    }

    pub(crate) async fn value_impl(&self, user: &Addr) -> ApiResult<f64> {
        let (data, _): (Option<ValueData>, _) = self
            .data_get("/value", &[("user", user.to_string())])
            .await?;
        Ok(data.and_then(|d| d.value).unwrap_or(0.0))
    }

    pub(crate) async fn leaderboard_impl(
        &self,
        period: TimePeriod,
        sort: LeaderboardSort,
        cursor: Option<&str>,
    ) -> ApiResult<Page<LeaderboardRow>> {
        let mut query: Query = vec![
            ("time_period", enum_param(period)),
            ("sort_by", enum_param(sort)),
            ("limit", MAX_LIMIT.to_string()),
        ];
        push_cursor(&mut query, cursor);
        self.data_page("/leaderboard", &query).await
    }

    pub(crate) async fn biggest_winners_impl(
        &self,
        period: TimePeriod,
        cursor: Option<&str>,
    ) -> ApiResult<Page<BiggestWinnerRow>> {
        let mut query: Query = vec![
            ("time_period", enum_param(period)),
            ("limit", BIGGEST_WINNERS_LIMIT.to_string()),
        ];
        push_cursor(&mut query, cursor);
        self.data_page("/biggest-winners", &query).await
    }

    pub(crate) async fn tokens_impl(&self, condition_id: &str) -> ApiResult<Vec<TokenRow>> {
        if crate::is_synthetic(condition_id) {
            return Ok(Vec::new());
        }
        Ok(self
            .data_page("/tokens", &[("condition", condition_id.to_string())])
            .await?
            .data)
    }

    /// Walks `[start, end)` in windows of at most 15 days, following cursors.
    ///
    /// `bucket_seconds` is left to the server: it picks the finest grain that still
    /// covers the window, while an explicit fine grain over an old window (5-minute data
    /// expires after ~60 days) returns an empty series.
    pub(crate) async fn prices_history_impl(
        &self,
        token_id: &str,
        start: i64,
        end: i64,
    ) -> ApiResult<Vec<PricePoint>> {
        let mut out: Vec<PricePoint> = Vec::new();
        let mut from = start;
        while from < end {
            let to = end.min(from + PRICES_MAX_SPAN_S);
            let mut cursor: Option<String> = None;
            loop {
                let mut query: Query = vec![
                    ("token_id", token_id.into()),
                    ("start", from.to_string()),
                    ("end", to.to_string()),
                    ("limit", PRICES_LIMIT.to_string()),
                ];
                push_cursor(&mut query, cursor.as_deref());
                let page: Page<PricePoint> = self.data_page("/prices-history", &query).await?;
                out.extend(page.data);
                match page.next_cursor {
                    Some(c) if cursor.as_deref() != Some(c.as_str()) => cursor = Some(c),
                    _ => break,
                }
            }
            from = to;
        }
        out.sort_by_key(|p| p.timestamp);
        out.dedup_by_key(|p| p.timestamp);
        Ok(out)
    }

    pub(crate) async fn status_impl(&self) -> ApiResult<ApiStatus> {
        let (data, _): (Option<StatusData>, _) = self.data_get("/status", &[]).await?;
        let data = data.unwrap_or(StatusData {
            serving: None,
            ingestion: None,
        });
        Ok(ApiStatus {
            lag_seconds: data.serving.map_or(0, |s| s.lag_seconds),
            max_synced_block: data.ingestion.and_then(|i| i.max_synced_block),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_buster_is_tiny_and_varies() {
        let a = cache_buster_amount(1_792_582_000);
        let b = cache_buster_amount(1_792_582_001);
        assert_ne!(a, b);
        assert_eq!(a, "0.000001000");
        assert_eq!(cache_buster_amount(999), "0.000001999");
        assert!(b.parse::<f64>().unwrap() < 0.01);
    }

    #[test]
    fn enum_params_match_api_spelling() {
        assert_eq!(enum_param(TimePeriod::Week), "week");
        assert_eq!(enum_param(LeaderboardSort::Pnl), "PNL");
        assert_eq!(enum_param(Side::Sell), "SELL");
    }

    #[test]
    fn classes_by_path() {
        assert_eq!(class_of("/trades"), RateClass::Trades);
        assert_eq!(class_of("/activity"), RateClass::Positions);
        assert_eq!(class_of("/positions"), RateClass::Positions);
        assert_eq!(class_of("/holders"), RateClass::Data);
    }
}
