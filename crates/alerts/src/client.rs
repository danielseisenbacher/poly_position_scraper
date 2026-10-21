//! `GitHubApi` over the GitHub REST API (reqwest).

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde_json::{json, Map, Value};

use crate::{GitHubApi, GitHubClient, IssueSummary, IssueUpdate};

/// Retries after a rate-limited response.
const MAX_RETRIES: u32 = 3;
const PER_PAGE: usize = 100;
/// Safety stop for pagination (10 000 issues).
const MAX_PAGES: u32 = 100;
/// Wait when GitHub signals a secondary rate limit without saying for how long.
const DEFAULT_RATE_WAIT_S: u64 = 60;

pub(crate) fn http_client() -> reqwest::Client {
    let mut headers = HeaderMap::new();
    headers.insert(
        ACCEPT,
        HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "X-GitHub-Api-Version",
        HeaderValue::from_static("2022-11-28"),
    );
    headers.insert(USER_AGENT, HeaderValue::from_static("polywatch"));
    reqwest::Client::builder()
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .build()
        .expect("building the HTTP client")
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// How long to wait before retrying a 403/429, or `None` if it is not a rate limit
/// (e.g. a plain permission error).
pub(crate) fn rate_limit_wait(
    status: StatusCode,
    headers: &HeaderMap,
    body: &str,
    now: i64,
) -> Option<Duration> {
    if status != StatusCode::FORBIDDEN && status != StatusCode::TOO_MANY_REQUESTS {
        return None;
    }
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
    };
    if let Some(secs) = header("retry-after").and_then(|v| v.parse::<u64>().ok()) {
        return Some(Duration::from_secs(secs));
    }
    if header("x-ratelimit-remaining") == Some("0") {
        if let Some(reset) = header("x-ratelimit-reset").and_then(|v| v.parse::<i64>().ok()) {
            return Some(Duration::from_secs((reset - now).max(0) as u64 + 1));
        }
    }
    if status == StatusCode::TOO_MANY_REQUESTS || body.to_ascii_lowercase().contains("rate limit") {
        return Some(Duration::from_secs(DEFAULT_RATE_WAIT_S));
    }
    None
}

fn snippet(s: &str) -> &str {
    match s.char_indices().nth(300) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

#[derive(Deserialize)]
struct RawLabel {
    name: String,
}

#[derive(Deserialize)]
struct RawIssue {
    number: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    labels: Vec<RawLabel>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    pull_request: Option<Value>,
}

impl GitHubClient {
    fn url(&self, path: &str) -> String {
        format!("{}/repos/{}{}", self.base_url, self.repo, path)
    }

    /// Sends one request, sitting out rate limits. Returns the final status and body.
    async fn send(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> anyhow::Result<(StatusCode, String)> {
        let url = self.url(path);
        // Mutating calls are serialised and spaced by `write_delay`.
        let mut write_guard = if method == Method::GET {
            None
        } else {
            let guard = self.last_write.lock().await;
            if let Some(last) = *guard {
                let since = last.elapsed();
                if since < self.write_delay {
                    tokio::time::sleep(self.write_delay - since).await;
                }
            }
            Some(guard)
        };
        let mut attempt = 0;
        loop {
            let mut req = self
                .http
                .request(method.clone(), &url)
                .bearer_auth(&self.token);
            if !query.is_empty() {
                req = req.query(query);
            }
            if let Some(b) = body {
                req = req.json(b);
            }
            let resp = req
                .send()
                .await
                .with_context(|| format!("{method} {path}"))?;
            let status = resp.status();
            let headers = resp.headers().clone();
            let text = resp.text().await.unwrap_or_default();
            if let Some(g) = write_guard.as_mut() {
                **g = Some(Instant::now());
            }
            if attempt < MAX_RETRIES {
                if let Some(wait) = rate_limit_wait(status, &headers, &text, unix_now()) {
                    if wait > self.max_retry_wait {
                        bail!(
                            "{method} {path}: rate limited for {}s (HTTP {status})",
                            wait.as_secs()
                        );
                    }
                    attempt += 1;
                    tracing::warn!(%status, wait_s = wait.as_secs(), attempt, "GitHub rate limit on {method} {path}");
                    tokio::time::sleep(wait).await;
                    continue;
                }
            }
            return Ok((status, text));
        }
    }

    async fn send_ok(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> anyhow::Result<String> {
        let (status, text) = self.send(method.clone(), path, query, body).await?;
        if !status.is_success() {
            bail!("{method} {path} failed: HTTP {status}: {}", snippet(&text));
        }
        Ok(text)
    }
}

#[async_trait]
impl GitHubApi for GitHubClient {
    async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> anyhow::Result<u64> {
        let payload = json!({ "title": title, "body": body, "labels": labels });
        let text = self
            .send_ok(Method::POST, "/issues", &[], Some(&payload))
            .await?;
        let v: Value = serde_json::from_str(&text).context("parsing created issue")?;
        v["number"].as_u64().context("created issue has no number")
    }

    async fn update_issue(&self, number: u64, update: &IssueUpdate) -> anyhow::Result<()> {
        let mut m = Map::new();
        if let Some(t) = &update.title {
            m.insert("title".into(), json!(t));
        }
        if let Some(b) = &update.body {
            m.insert("body".into(), json!(b));
        }
        if let Some(l) = &update.labels {
            m.insert("labels".into(), json!(l));
        }
        if let Some(s) = &update.state {
            m.insert("state".into(), json!(s));
        }
        if m.is_empty() {
            return Ok(());
        }
        let path = format!("/issues/{number}");
        self.send_ok(Method::PATCH, &path, &[], Some(&Value::Object(m)))
            .await?;
        Ok(())
    }

    async fn comment(&self, number: u64, body: &str) -> anyhow::Result<()> {
        let path = format!("/issues/{number}/comments");
        self.send_ok(Method::POST, &path, &[], Some(&json!({ "body": body })))
            .await?;
        Ok(())
    }

    async fn list_issues(&self, label: &str, state: &str) -> anyhow::Result<Vec<IssueSummary>> {
        let mut out = Vec::new();
        for page in 1..=MAX_PAGES {
            let query = [
                ("labels", label.to_string()),
                ("state", state.to_string()),
                ("per_page", PER_PAGE.to_string()),
                ("page", page.to_string()),
            ];
            let text = self.send_ok(Method::GET, "/issues", &query, None).await?;
            let items: Vec<RawIssue> = serde_json::from_str(&text)
                .with_context(|| format!("parsing issues page {page}"))?;
            let n = items.len();
            out.extend(
                items
                    .into_iter()
                    .filter(|i| i.pull_request.is_none())
                    .map(|i| IssueSummary {
                        number: i.number,
                        title: i.title,
                        state: i.state,
                        labels: i.labels.into_iter().map(|l| l.name).collect(),
                        body: i.body.unwrap_or_default(),
                    }),
            );
            if n < PER_PAGE {
                return Ok(out);
            }
        }
        tracing::warn!(
            label,
            state,
            "stopped listing issues after {MAX_PAGES} pages"
        );
        Ok(out)
    }

    async fn ensure_label(&self, name: &str, color: &str, description: &str) -> anyhow::Result<()> {
        let payload = json!({ "name": name, "color": color, "description": description });
        let (status, text) = self
            .send(Method::POST, "/labels", &[], Some(&payload))
            .await?;
        if status.is_success() {
            return Ok(());
        }
        if status == StatusCode::UNPROCESSABLE_ENTITY && text.contains("already_exists") {
            return Ok(());
        }
        bail!(
            "creating label {name:?} failed: HTTP {status}: {}",
            snippet(&text)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(&'static str, &'static str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(*k, HeaderValue::from_static(v));
        }
        h
    }

    #[test]
    fn rate_limit_waits() {
        let s = |n| Some(Duration::from_secs(n));
        let f = StatusCode::FORBIDDEN;
        assert_eq!(
            rate_limit_wait(f, &headers(&[("retry-after", "7")]), "", 0),
            s(7)
        );
        let h = headers(&[
            ("x-ratelimit-remaining", "0"),
            ("x-ratelimit-reset", "1030"),
        ]);
        assert_eq!(rate_limit_wait(f, &h, "", 1000), s(31));
        let h = headers(&[
            ("x-ratelimit-remaining", "12"),
            ("x-ratelimit-reset", "1030"),
        ]);
        assert_eq!(rate_limit_wait(f, &h, "", 1000), None);
        assert_eq!(
            rate_limit_wait(f, &HeaderMap::new(), "secondary rate limit", 0),
            s(60)
        );
        assert_eq!(
            rate_limit_wait(StatusCode::TOO_MANY_REQUESTS, &HeaderMap::new(), "", 0),
            s(60)
        );
        assert_eq!(
            rate_limit_wait(
                StatusCode::NOT_FOUND,
                &headers(&[("retry-after", "7")]),
                "",
                0
            ),
            None
        );
    }
}
