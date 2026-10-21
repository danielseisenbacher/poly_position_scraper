//! JSON files read by the dashboard (`site/app.js`). Written by `store::render_site`.
//! Each file must stay under ~2 MB; caps live in `Config::site`.
//!
//! | file                   | type                      |
//! |------------------------|---------------------------|
//! | `meta.json`            | [`Meta`]                  |
//! | `index.json`           | `Vec<CandidateRow>`       |
//! | `wallets/<addr>.json`  | [`crate::Wallet`]         |
//! | `markets.json`         | `Vec<MarketView>`         |
//! | `clusters.json`        | `Vec<crate::Cluster>`     |
//! | `activity_recent.json` | `Vec<TapeRow>`            |

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::api::{RateClass, RequestStats};
use crate::{Addr, SignalKind, Tier, TradeKind};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateRow {
    pub addr: Addr,
    /// Display name or pseudonym.
    pub name: String,
    pub score: u8,
    pub tier: Tier,
    /// (kind, strength) pairs, strongest first.
    pub signals: Vec<(SignalKind, f64)>,
    pub age_d: Option<f64>,
    pub pnl: f64,
    pub win_rate: Option<f64>,
    pub n_resolved: f64,
    pub z: f64,
    pub value: f64,
    pub n_markets: u32,
    pub last_trade_ts: Option<i64>,
    pub cluster_id: Option<String>,
    pub issue: Option<u64>,
    pub watched: bool,
    pub first_seen: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketView {
    pub condition_id: String,
    pub title: String,
    pub slug: String,
    pub end_date: Option<i64>,
    pub volume: Option<f64>,
    pub resolved_outcome: Option<String>,
    pub wallets: Vec<MarketWallet>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketWallet {
    pub addr: Addr,
    pub score: u8,
    pub outcome: String,
    pub avg_price: f64,
    pub size: f64,
    pub current_price: f64,
}

/// One trade on the live tape (last 48 h of watchlist activity).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TapeRow {
    pub addr: Addr,
    pub name: String,
    pub score: u8,
    pub ts: i64,
    pub kind: TradeKind,
    pub title: String,
    pub outcome: String,
    pub price: f64,
    pub usdc: f64,
    pub condition_id: String,
    pub tx: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Meta {
    pub generated_at: i64,
    /// GitHub Actions run id, if any.
    pub run_id: Option<String>,
    pub last_discover: i64,
    pub last_track: i64,
    pub counts: Counts,
    pub api: ApiHealth,
    /// Data API lag reported by `/v2/status`.
    pub feed_lag_s: i64,
    pub feed_truncated: bool,
    /// Score histogram in buckets of 10 (index 0 = 0–9, …, 9 = 90–100), for calibration.
    pub score_histogram: [u32; 10],
    pub signal_counts: BTreeMap<SignalKind, u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Counts {
    pub candidates: u32,
    pub watchlist: u32,
    pub high: u32,
    pub medium: u32,
    pub clusters: u32,
    pub wallets_scanned_last_run: u32,
    pub trades_tracked_last_run: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ApiHealth {
    pub requests: BTreeMap<RateClass, u64>,
    pub rate_limited: u64,
    pub errors: u64,
}

impl From<&RequestStats> for ApiHealth {
    fn from(s: &RequestStats) -> Self {
        ApiHealth { requests: s.requests.clone(), rate_limited: s.rate_limited, errors: s.errors }
    }
}
