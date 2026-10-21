//! Gamma: markets and public profiles, normalised into [`MarketMeta`] and [`Profile`].

use std::collections::{BTreeSet, HashSet};

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use serde::Deserialize;
use serde_json::Value;
use types::api::{ApiResult, RateClass};
use types::{Addr, MarketCategory, MarketMeta, Profile};

use crate::http::On404;
use crate::Client;

/// Gamma's page cap for `/markets`.
const PAGE: usize = 100;
/// Offsets beyond this return 422.
const MAX_OFFSET: usize = 2000;
/// `condition_ids` per request (verified live: 50 repeated params work with `limit=100`).
const CONDITION_BATCH: usize = 50;

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct GammaMarket {
    condition_id: Option<String>,
    question: Option<String>,
    slug: Option<String>,
    start_date: Option<String>,
    end_date: Option<String>,
    closed_time: Option<String>,
    volume_num: Option<f64>,
    volume: Option<Value>,
    liquidity_num: Option<f64>,
    liquidity: Option<Value>,
    outcomes: Option<Value>,
    outcome_prices: Option<Value>,
    clob_token_ids: Option<Value>,
    closed: Option<bool>,
    uma_resolution_status: Option<String>,
    sports_market_type: Option<String>,
    game_start_time: Option<String>,
    neg_risk: Option<bool>,
    category: Option<String>,
    tags: Option<Vec<GammaTag>>,
    events: Option<Vec<GammaEvent>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct GammaEvent {
    slug: Option<String>,
    category: Option<String>,
    series_slug: Option<String>,
    tags: Option<Vec<GammaTag>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GammaTag {
    slug: Option<String>,
    label: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct GammaProfile {
    created_at: Option<String>,
    name: Option<String>,
    pseudonym: Option<String>,
    x_username: Option<String>,
    verified_badge: Option<bool>,
    profile_image: Option<String>,
    users: Option<Vec<GammaUser>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GammaUser {
    id: Option<Value>,
}

/// Parses Gamma's timestamp flavours: RFC 3339, `2026-10-21 11:36:02.146326+00`, or a bare date.
pub(crate) fn parse_ts(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Some(t.timestamp());
    }
    for fmt in ["%Y-%m-%d %H:%M:%S%.f%#z", "%Y-%m-%dT%H:%M:%S%.f%#z"] {
        if let Ok(t) = DateTime::parse_from_str(s, fmt) {
            return Some(t.timestamp());
        }
    }
    for fmt in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f"] {
        if let Ok(t) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(t.and_utc().timestamp());
        }
    }
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d")
        .ok()
        .map(|d| d.and_time(Default::default()).and_utc().timestamp())
}

/// Gamma sends arrays as JSON strings (`"[\"Yes\", \"No\"]"`); accept real arrays too.
fn string_list(v: &Option<Value>) -> Vec<String> {
    let arr = match v {
        Some(Value::String(s)) => serde_json::from_str::<Vec<Value>>(s).unwrap_or_default(),
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    };
    arr.into_iter()
        .map(|x| match x {
            Value::String(s) => s,
            other => other.to_string(),
        })
        .collect()
}

/// A number that may arrive as a JSON number or a numeric string.
fn number(v: &Option<Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    }
}

const SPORTS_WORDS: &[&str] = &[
    "sports",
    "nba",
    "nfl",
    "mlb",
    "nhl",
    "ncaa",
    "cfb",
    "cbb",
    "epl",
    "ucl",
    "uel",
    "mls",
    "atp",
    "wta",
    "itf",
    "ufc",
    "mma",
    "f1",
    "soccer",
    "football",
    "basketball",
    "baseball",
    "hockey",
    "tennis",
    "golf",
    "cricket",
    "boxing",
    "esports",
    "table-tennis",
    "setka",
    "fifwc",
];
const CRYPTO_WORDS: &[&str] = &[
    "updown",
    "bitcoin",
    "btc",
    "ethereum",
    "eth",
    "solana",
    "sol",
    "xrp",
    "crypto-prices",
];

fn tokens(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty())
}

fn categorize(m: &GammaMarket, slug: &str, question: &str) -> MarketCategory {
    let events = m.events.as_deref().unwrap_or_default();
    let tag_slugs: Vec<String> = m
        .tags
        .iter()
        .flatten()
        .chain(events.iter().flat_map(|e| e.tags.iter().flatten()))
        .flat_map(|t| [t.slug.clone(), t.label.clone()])
        .flatten()
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let categories: Vec<String> = m
        .category
        .iter()
        .chain(events.iter().filter_map(|e| e.category.as_ref()))
        .map(|s| s.to_ascii_lowercase())
        .collect();
    let slug = slug.to_ascii_lowercase();
    let question = question.to_ascii_lowercase();
    let series: Vec<String> = events
        .iter()
        .filter_map(|e| e.series_slug.as_ref())
        .map(|s| s.to_ascii_lowercase())
        .collect();

    let set = |s: &Option<String>| s.as_deref().is_some_and(|v| !v.trim().is_empty());
    let first_token_sports = tokens(&slug)
        .next()
        .is_some_and(|t| SPORTS_WORDS.contains(&t));
    if set(&m.sports_market_type)
        || set(&m.game_start_time)
        || first_token_sports
        || tag_slugs.iter().any(|t| SPORTS_WORDS.contains(&t.as_str()))
        || categories.iter().any(|c| c.contains("sport"))
    {
        return MarketCategory::Sports;
    }

    let crypto_hit = |s: &str| tokens(s).any(|t| CRYPTO_WORDS.contains(&t));
    if slug.contains("up-or-down")
        || question.contains("up or down")
        || crypto_hit(&slug)
        || crypto_hit(&question)
        || series
            .iter()
            .any(|s| crypto_hit(s) || s.contains("up-or-down"))
    {
        return MarketCategory::Crypto;
    }

    if tag_slugs
        .iter()
        .chain(&categories)
        .any(|t| t.contains("politic") || t.contains("election"))
        || tokens(&slug).any(|t| t == "election" || t == "elections")
    {
        return MarketCategory::Politics;
    }
    MarketCategory::Other
}

/// Normalises one Gamma market; `None` without a condition id.
pub(crate) fn normalize_market(m: &GammaMarket) -> Option<MarketMeta> {
    let condition_id = m.condition_id.as_deref()?.trim().to_ascii_lowercase();
    if condition_id.is_empty() {
        return None;
    }
    let question = m.question.clone().unwrap_or_default();
    let slug = m.slug.clone().unwrap_or_default();
    let outcomes = string_list(&m.outcomes);
    let outcome_prices: Vec<f64> = string_list(&m.outcome_prices)
        .iter()
        .map(|p| p.trim().parse().unwrap_or(f64::NAN))
        .collect();
    let outcome_prices = if outcome_prices.iter().any(|p| p.is_nan()) {
        Vec::new()
    } else {
        outcome_prices
    };

    let binary_prices =
        !outcome_prices.is_empty() && outcome_prices.iter().all(|p| *p == 0.0 || *p == 1.0);
    let resolved = m
        .uma_resolution_status
        .as_deref()
        .is_some_and(|s| s.eq_ignore_ascii_case("resolved"));
    let winning_index = if m.closed == Some(true) && (resolved || binary_prices) {
        let winners: Vec<usize> = outcome_prices
            .iter()
            .enumerate()
            .filter(|(_, p)| (**p - 1.0).abs() < 1e-9)
            .map(|(i, _)| i)
            .collect();
        (winners.len() == 1).then(|| winners[0])
    } else {
        None
    };

    let category = categorize(m, &slug, &question);
    Some(MarketMeta {
        condition_id,
        event_slug: m
            .events
            .as_deref()
            .and_then(|e| e.first())
            .and_then(|e| e.slug.clone())
            .unwrap_or_default(),
        start_date: m.start_date.as_deref().and_then(parse_ts),
        end_date: m.end_date.as_deref().and_then(parse_ts),
        closed_time: m.closed_time.as_deref().and_then(parse_ts),
        volume: m.volume_num.or_else(|| number(&m.volume)),
        liquidity: m.liquidity_num.or_else(|| number(&m.liquidity)),
        outcomes,
        token_ids: string_list(&m.clob_token_ids),
        outcome_prices,
        winning_index,
        category,
        neg_risk: m.neg_risk.unwrap_or(false),
        question,
        slug,
    })
}

/// Digits after `profile-image-` in an image URL.
fn profile_image_id(url: &str) -> Option<u64> {
    let rest = &url[url.find("profile-image-")? + "profile-image-".len()..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

pub(crate) fn normalize_profile(p: GammaProfile) -> Profile {
    let user_id = p
        .users
        .as_deref()
        .and_then(|u| u.first())
        .and_then(|u| match &u.id {
            Some(Value::String(s)) => s.trim().parse().ok(),
            Some(Value::Number(n)) => n.as_u64(),
            _ => None,
        });
    Profile {
        name: p.name.unwrap_or_default(),
        pseudonym: p.pseudonym.unwrap_or_default(),
        x_username: p.x_username.unwrap_or_default(),
        profile_image_id: p.profile_image.as_deref().and_then(profile_image_id),
        user_id,
        created_at: p.created_at.as_deref().and_then(parse_ts),
        owner_eoa: None,
        verified: p.verified_badge.unwrap_or(false),
    }
}

impl Client {
    async fn gamma_markets(&self, query: &[(&str, String)]) -> ApiResult<Vec<MarketMeta>> {
        let url = format!("{}/markets", self.cfg.gamma_base.trim_end_matches('/'));
        let req = self.build_get(&url, query)?;
        let raw: Vec<GammaMarket> = self.get_json(RateClass::Gamma, req).await?;
        Ok(raw.iter().filter_map(normalize_market).collect())
    }

    pub(crate) async fn recently_closed_markets_impl(
        &self,
        since: i64,
        min_volume: f64,
        max: usize,
    ) -> ApiResult<Vec<MarketMeta>> {
        let mut out = Vec::new();
        let mut offset = 0;
        while out.len() < max && offset <= MAX_OFFSET {
            let page = self
                .gamma_markets(&[
                    ("closed", "true".into()),
                    ("order", "closedTime".into()),
                    ("ascending", "false".into()),
                    // Honoured server-side and keeps the closedTime order (verified live
                    // 2026-10-21: with 10k the 2000-offset window reaches back ~42 h).
                    ("volume_num_min", min_volume.to_string()),
                    ("include_tag", "true".into()),
                    ("limit", PAGE.to_string()),
                    ("offset", offset.to_string()),
                ])
                .await?;
            let n = page.len();
            for m in page {
                match m.closed_time {
                    Some(t) if t < since => return Ok(out),
                    Some(_) if out.len() < max => out.push(m),
                    _ => {}
                }
            }
            if n < PAGE {
                break;
            }
            offset += PAGE;
        }
        Ok(out)
    }

    /// Gamma hides closed markets unless `closed=true`, so each batch is asked for open
    /// markets first and the ids still missing are asked again with `closed=true`.
    pub(crate) async fn markets_by_condition_impl(
        &self,
        condition_ids: &[String],
    ) -> ApiResult<Vec<MarketMeta>> {
        let ids: BTreeSet<String> = condition_ids
            .iter()
            .map(|c| c.trim().to_ascii_lowercase())
            .filter(|c| !c.is_empty() && !crate::is_synthetic(c))
            .collect();
        let ids: Vec<String> = ids.into_iter().collect();
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for batch in ids.chunks(CONDITION_BATCH) {
            for closed in [false, true] {
                let missing: Vec<&String> = batch.iter().filter(|c| !seen.contains(*c)).collect();
                if missing.is_empty() {
                    break;
                }
                let mut query: Vec<(&str, String)> = missing
                    .iter()
                    .map(|c| ("condition_ids", (*c).clone()))
                    .collect();
                if closed {
                    query.push(("closed", "true".into()));
                }
                query.push(("include_tag", "true".into()));
                query.push(("limit", PAGE.to_string()));
                for m in self.gamma_markets(&query).await? {
                    if seen.insert(m.condition_id.clone()) {
                        out.push(m);
                    }
                }
            }
        }
        Ok(out)
    }

    pub(crate) async fn profile_impl(&self, addr: &Addr) -> ApiResult<Option<Profile>> {
        let url = format!(
            "{}/public-profile",
            self.cfg.gamma_base.trim_end_matches('/')
        );
        let req = self.build_get(&url, &[("address", addr.to_string())])?;
        let full_url = req.url().to_string();
        match self.send(RateClass::Gamma, req, On404::None).await? {
            None => Ok(None),
            Some(body) => Ok(Some(normalize_profile(self.decode(&full_url, &body)?))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_gamma_timestamps() {
        assert_eq!(parse_ts("2026-10-21T11:35:00Z"), Some(1_792_582_500));
        assert_eq!(
            parse_ts("2026-10-21 11:35:00.146326+00"),
            Some(1_792_582_500)
        );
        assert_eq!(parse_ts("2026-10-21 11:35:00+00"), Some(1_792_582_500));
        assert_eq!(
            parse_ts("2026-10-20T11:38:58.39987Z"),
            Some(1_792_582_500 - 86_400 + 238)
        );
        assert_eq!(parse_ts("2026-10-21"), Some(1_792_540_800));
        assert_eq!(parse_ts(""), None);
        assert_eq!(parse_ts("garbage"), None);
    }

    #[test]
    fn image_id_from_url() {
        assert_eq!(
            profile_image_id("https://polymarket-upload.s3.us-east-2.amazonaws.com/profile-image-7434297-fefd6ca3.png"),
            Some(7_434_297)
        );
        assert_eq!(profile_image_id("https://example.com/avatar.png"), None);
    }

    #[test]
    fn categories() {
        let m = |slug: &str, q: &str| GammaMarket {
            slug: Some(slug.into()),
            question: Some(q.into()),
            ..Default::default()
        };
        let cat = |g: GammaMarket| {
            let (s, q) = (g.slug.clone().unwrap(), g.question.clone().unwrap());
            categorize(&g, &s, &q)
        };
        assert_eq!(
            cat(m(
                "btc-updown-5m-1792582200",
                "Bitcoin Up or Down - October 21"
            )),
            MarketCategory::Crypto
        );
        assert_eq!(
            cat(m(
                "will-bitcoin-reach-92pt5k-in-october-2026",
                "Will Bitcoin reach $92,500?"
            )),
            MarketCategory::Crypto
        );
        assert_eq!(
            cat(m("nba-phi-nyk-2026-10-20", "76ers vs. Knicks")),
            MarketCategory::Sports
        );
        assert_eq!(
            cat(m("will-the-method-work", "Will the method work?")),
            MarketCategory::Other
        );
        let mut p = m("will-trump-win", "Will Trump win?");
        p.tags = Some(vec![GammaTag {
            slug: Some("politics".into()),
            label: Some("Politics".into()),
        }]);
        assert_eq!(cat(p), MarketCategory::Politics);
        let mut s = m("some-market", "Who wins?");
        s.sports_market_type = Some("moneyline".into());
        assert_eq!(cat(s), MarketCategory::Sports);
    }
}
