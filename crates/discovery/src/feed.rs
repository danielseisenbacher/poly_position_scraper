//! Global trade feed: page newest-first back to the high-water mark, skip noise
//! (combos, short crypto up/down series), aggregate per wallet, pre-filter.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use tracing::{info, warn};
use types::api::{FeedTrade, PolyApi, Side, TradesQuery};
use types::config::DiscoveryConfig;
use types::Addr;

/// Rows requested per `/v2/trades` page (the API maximum).
pub const FEED_PAGE_LIMIT: u32 = 1000;

/// What one wallet did in the feed window.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WalletAgg {
    /// Σ USDC over BUY rows.
    pub buy_usdc: f64,
    /// Σ usdc·price over BUY rows (for the USDC-weighted mean buy price).
    buy_price_usdc: f64,
    /// Distinct condition ids bought.
    pub markets: BTreeSet<String>,
    /// Rows of any side.
    pub trades: u32,
}

impl WalletAgg {
    /// USDC-weighted mean buy price, `None` without buys.
    pub fn mean_buy_price(&self) -> Option<f64> {
        (self.buy_usdc > 0.0).then(|| self.buy_price_usdc / self.buy_usdc)
    }
}

/// Per-wallet aggregation of feed rows plus a compact copy of the kept rows (for co-trade linking).
#[derive(Debug, Clone, Default)]
pub struct FeedAgg {
    pub wallets: BTreeMap<Addr, WalletAgg>,
    /// Kept rows, stripped to the fields linking needs (wallet, condition, side, size, price, ts).
    pub rows: Vec<FeedTrade>,
    pub skipped: u64,
}

impl FeedAgg {
    /// Add one row; returns `false` (and counts it as skipped) for combos and short crypto series.
    pub fn push(&mut self, t: &FeedTrade) -> bool {
        if is_noise(t) {
            self.skipped += 1;
            return false;
        }
        let a = self.wallets.entry(t.proxy_wallet.clone()).or_default();
        a.trades += 1;
        if t.side == Side::Buy {
            let usdc = t.usdc();
            a.buy_usdc += usdc;
            a.buy_price_usdc += usdc * t.price;
            if !a.markets.contains(&t.condition_id) {
                a.markets.insert(t.condition_id.clone());
            }
        }
        self.rows.push(compact(t));
        true
    }
}

/// Combo bets and crypto up/down series are never insider candidates.
pub fn is_noise(t: &FeedTrade) -> bool {
    t.is_combo() || is_short_crypto(&t.slug) || is_short_crypto(&t.event_slug)
}

/// Crypto up/down series (5–15 minute markets), recognised from the slug.
pub fn is_short_crypto(slug: &str) -> bool {
    let s = slug.to_ascii_lowercase();
    s.contains("updown") || s.contains("up-or-down")
}

fn compact(t: &FeedTrade) -> FeedTrade {
    FeedTrade {
        proxy_wallet: t.proxy_wallet.clone(),
        side: t.side,
        token_id: String::new(),
        condition_id: t.condition_id.clone(),
        size: t.size,
        price: t.price,
        timestamp: t.timestamp,
        title: String::new(),
        slug: String::new(),
        event_slug: String::new(),
        outcome: String::new(),
        outcome_index: t.outcome_index,
        name: String::new(),
        pseudonym: String::new(),
        profile_image: String::new(),
        transaction_hash: String::new(),
    }
}

/// Feed pre-filter: enough buy volume, and cheap entries or breadth.
pub fn passes_prefilter(a: &WalletAgg, cfg: &DiscoveryConfig) -> bool {
    a.buy_usdc >= cfg.prefilter_min_buy_usdc
        && (a
            .mean_buy_price()
            .is_some_and(|p| p <= cfg.prefilter_max_mean_buy_price)
            || a.markets.len() >= cfg.prefilter_min_markets as usize)
}

#[derive(Debug, Default)]
pub struct FeedResult {
    pub agg: FeedAgg,
    pub pages: u32,
    /// Rows received (before skipping, including the ones older than the stop mark).
    pub rows: u64,
    /// Stopped at `feed_max_pages` (or on an error after the first page) before reaching `stop_ts`.
    pub truncated: bool,
    /// Newest timestamp seen (0 if the feed was empty).
    pub high_water: i64,
}

/// Page the global taker feed (newest first) until a row older than `stop_ts`
/// appears or `feed_max_pages` pages were read. Only a failing first page is an error.
pub async fn ingest(
    api: &dyn PolyApi,
    cfg: &DiscoveryConfig,
    stop_ts: i64,
) -> anyhow::Result<FeedResult> {
    let q = TradesQuery {
        limit: FEED_PAGE_LIMIT,
        taker_only: true,
        min_cash: None,
        side: None,
    };
    let mut out = FeedResult::default();
    let mut cursor: Option<String> = None;
    loop {
        if out.pages >= cfg.feed_max_pages {
            out.truncated = true;
            warn!(
                pages = out.pages,
                "feed truncated at the page cap before reaching the high-water mark"
            );
            break;
        }
        let page = match api.trades_page(&q, cursor.as_deref()).await {
            Ok(p) => p,
            Err(e) if out.pages == 0 => {
                return Err(e).context("reading the first page of the global trade feed")
            }
            Err(e) => {
                warn!(pages = out.pages, "feed page failed, stopping early: {e}");
                out.truncated = true;
                break;
            }
        };
        out.pages += 1;
        out.rows += page.data.len() as u64;
        let mut reached = false;
        for t in &page.data {
            out.high_water = out.high_water.max(t.timestamp);
            if t.timestamp < stop_ts {
                reached = true;
            } else {
                out.agg.push(t);
            }
        }
        if reached || page.data.is_empty() {
            break;
        }
        match page.next_cursor {
            Some(c) => cursor = Some(c),
            None => break,
        }
    }
    info!(
        pages = out.pages,
        rows = out.rows,
        kept = out.agg.rows.len(),
        skipped = out.agg.skipped,
        wallets = out.agg.wallets.len(),
        truncated = out.truncated,
        "feed ingested"
    );
    Ok(out)
}
