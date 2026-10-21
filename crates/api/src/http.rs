//! One request with rate limiting, the global concurrency cap, retries and counters.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{Request, StatusCode};
use serde::de::DeserializeOwned;
use types::api::{ApiError, ApiResult, RateClass};

use crate::Client;

const BACKOFF_CAP: Duration = Duration::from_secs(30);
/// Never sleep longer than this on a single `Retry-After`.
const RETRY_AFTER_CAP: Duration = Duration::from_secs(120);
const EXCERPT_CHARS: usize = 300;

/// What a non-retryable 404 should become.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum On404 {
    Error,
    None,
}

impl Client {
    /// Sends `req` (cloned per attempt) and returns the body of the first 2xx response.
    pub(crate) async fn send(
        &self,
        class: RateClass,
        req: Request,
        on404: On404,
    ) -> ApiResult<Option<String>> {
        let url = req.url().to_string();
        let res = self.send_inner(class, req, &url, on404).await;
        if res.is_err() {
            self.count_error();
        }
        res
    }

    pub(crate) fn count_error(&self) {
        self.stats.lock().expect("stats mutex poisoned").errors += 1;
    }

    async fn send_inner(
        &self,
        class: RateClass,
        req: Request,
        url: &str,
        on404: On404,
    ) -> ApiResult<Option<String>> {
        let mut attempt = 0u32;
        loop {
            let last = attempt >= self.cfg.max_retries;
            let attempt_req = req.try_clone().expect("request bodies are always buffered");
            self.limiter.acquire(class).await;
            *self
                .stats
                .lock()
                .expect("stats mutex poisoned")
                .requests
                .entry(class)
                .or_default() += 1;
            let outcome = {
                let _permit = self.sem.acquire().await.expect("semaphore never closed");
                async {
                    let resp = self.http.execute(attempt_req).await?;
                    let (status, retry_after) = (resp.status(), retry_after(&resp));
                    Ok::<_, reqwest::Error>((status, retry_after, resp.text().await?))
                }
                .await
            };
            let wait = match outcome {
                Ok((status, _, body)) if status.is_success() => return Ok(Some(body)),
                Ok((StatusCode::NOT_FOUND, _, _)) if on404 == On404::None => return Ok(None),
                Ok((StatusCode::TOO_MANY_REQUESTS, retry_after, _)) => {
                    self.stats
                        .lock()
                        .expect("stats mutex poisoned")
                        .rate_limited += 1;
                    if last {
                        return Err(ApiError::RateLimited {
                            url: url.to_string(),
                        });
                    }
                    retry_after.unwrap_or_else(|| self.backoff(attempt))
                }
                Ok((status, _, _)) if status.is_server_error() && !last => {
                    tracing::debug!(%url, %status, "retrying server error");
                    self.backoff(attempt)
                }
                Ok((status, _, body)) => {
                    return Err(ApiError::Http {
                        status: status.as_u16(),
                        url: url.to_string(),
                        body: excerpt(&body),
                    })
                }
                Err(e) if last => {
                    return Err(ApiError::Transport {
                        url: url.to_string(),
                        msg: e.to_string(),
                    })
                }
                Err(e) => {
                    tracing::debug!(%url, error = %e, "retrying transport error");
                    self.backoff(attempt)
                }
            };
            self.stats.lock().expect("stats mutex poisoned").retries += 1;
            tokio::time::sleep(wait).await;
            attempt += 1;
        }
    }

    /// `min(cap, base · 2^attempt)`, jittered into its upper half.
    fn backoff(&self, attempt: u32) -> Duration {
        let full = self
            .backoff_base
            .saturating_mul(1u32 << attempt.min(16))
            .min(BACKOFF_CAP);
        full.mul_f64(0.5 + 0.5 * jitter())
    }

    /// GET `req`, decode the JSON body.
    pub(crate) async fn get_json<T: DeserializeOwned>(
        &self,
        class: RateClass,
        req: Request,
    ) -> ApiResult<T> {
        let url = req.url().to_string();
        let body = self
            .send(class, req, On404::Error)
            .await?
            .unwrap_or_default();
        self.decode(&url, &body)
    }

    pub(crate) fn decode<T: DeserializeOwned>(&self, url: &str, body: &str) -> ApiResult<T> {
        serde_json::from_str(body).map_err(|e| {
            self.count_error();
            ApiError::Decode {
                url: url.to_string(),
                msg: format!("{e}; body: {}", excerpt(body)),
            }
        })
    }

    /// Builds a GET request; a build failure (malformed base URL) is reported as a transport error.
    pub(crate) fn build_get(&self, url: &str, query: &[(&str, String)]) -> ApiResult<Request> {
        self.http
            .get(url)
            .query(query)
            .build()
            .map_err(|e| ApiError::Transport {
                url: url.to_string(),
                msg: e.to_string(),
            })
    }
}

/// `Retry-After` in seconds (integer or fractional); HTTP dates are ignored.
fn retry_after(resp: &reqwest::Response) -> Option<Duration> {
    let v = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?;
    let secs: f64 = v.trim().parse().ok()?;
    (secs.is_finite() && secs >= 0.0).then(|| Duration::from_secs_f64(secs).min(RETRY_AFTER_CAP))
}

/// Uniform-ish in `[0, 1)` from the clock's nanoseconds (no `rand` dependency).
fn jitter() -> f64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    (nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 11) as f64 / (1u64 << 53) as f64
}

pub(crate) fn excerpt(body: &str) -> String {
    let t = body.trim();
    match t.char_indices().nth(EXCERPT_CHARS) {
        Some((i, _)) => format!("{}…", &t[..i]),
        None => t.to_string(),
    }
}

/// Current unix time in seconds.
pub(crate) fn now_s() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
