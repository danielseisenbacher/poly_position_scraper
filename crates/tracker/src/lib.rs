//! One tracker run (every 8 min):
//!
//! 1. Load `watchlist.json`; evict entries (score < `watchlist_min_score` for
//!    `evict_low_score_s`, no activity for `evict_inactive_s`).
//! 2. Poll order: highest score first, then stalest `last_polled`. For each wallet call
//!    `/v2/activity?user=&start=last_activity_ts+1−POLL_OVERLAP_S` (all pages), stop when
//!    `time_budget_s` is used up — leftovers are polled next run, nothing is lost.
//! 3. Convert rows to [`types::TrackedTrade`] ([`from_activity`]), dedup by
//!    [`trade_key`], prepend to `wallets/<addr>.json` (cap `max_trades_per_wallet`),
//!    advance `last_activity_ts`.
//! 4. Wallets that traded: refresh OPEN positions + value if `refresh_positions`.
//! 5. Emit [`types::TradeEvent`]s for trades ≥ `big_trade_usd` or ≥ `big_trade_share`
//!    of wallet value (the CLI hands them to `alerts::comment_trades`); never for the
//!    backfill of a wallet's first poll.
//!
//! Nothing is written with `dry_run`.
//!
//! ## Failure and truncation semantics
//!
//! - An API error while paging one wallet's activity discards everything fetched for
//!   that wallet in this run and leaves its entry untouched (cursor and `last_polled`),
//!   so it is retried — and polled early, being the stalest — next run.
//! - A never-polled entry (`last_activity_ts == 0`) starts at `added_at − 7 days`.
//! - Activity is returned newest first. If a wallet has more than
//!   [`MAX_ACTIVITY_PAGES`] pages since its cursor, the newest pages are stored, the
//!   cursor advances to the newest timestamp fetched, and a warning is logged: the
//!   older rows in the gap are skipped. Following the cursor would otherwise re-fetch
//!   the same newest pages forever, and the contract has no resume marker. At 1000
//!   rows/page this needs > 10 000 rows within one 8-minute interval (or the 7-day
//!   first-poll backfill), i.e. only market-maker bots, which are not insider candidates.
//! - Position/value refresh errors are logged and keep the old snapshot.

use std::collections::HashSet;
use std::time::Instant;

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use store::DataDir;
use tracing::{debug, info, warn};
use types::api::{ActivityQuery, ActivityRow, ActivityType, PolyApi, RequestStats, Side};
use types::state::WatchlistEntry;
use types::{Addr, Config, PosStatus, Position, TrackedTrade, TradeEvent, TradeKind, Wallet};

/// Rows requested per `/v2/activity` page.
pub const ACTIVITY_PAGE_LIMIT: u32 = 1000;
/// Max `/v2/activity` pages per wallet per run (see module docs for what happens beyond).
pub const MAX_ACTIVITY_PAGES: usize = 10;
/// Max `/v2/positions?status=OPEN` pages per refresh.
pub const MAX_POSITION_PAGES: usize = 20;
/// First poll of a new entry reaches this far before `added_at`.
pub const FIRST_POLL_LOOKBACK_S: i64 = 7 * 86_400;
/// Re-read this far behind the cursor so rows the API indexes late are not missed;
/// dedup by [`trade_key`] drops the overlap.
pub const POLL_OVERLAP_S: i64 = 15 * 60;

#[derive(Debug, Clone, Default)]
pub struct TrackOpts {
    pub dry_run: bool,
    pub now: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrackReport {
    pub watchlist_size: u32,
    pub polled: u32,
    /// Wallets not reached because the time budget ran out.
    pub deferred: u32,
    pub new_trades: u32,
    pub evicted: Vec<Addr>,
    pub events: Vec<TradeEvent>,
    pub duration_s: f64,
    pub requests: RequestStats,
    /// New trades for the monthly archive (`store::archive::append`).
    #[serde(skip)]
    pub archive: Vec<(Addr, TrackedTrade)>,
}

/// Result of polling one watchlist entry.
enum Outcome {
    /// Time budget used up before the poll started.
    Deferred,
    /// API or store error; the entry is left untouched.
    Failed,
    Polled(Box<Polled>),
}

struct Polled {
    /// Index into the watchlist entries.
    idx: usize,
    /// Max activity timestamp fetched (0 if none).
    max_ts: i64,
    /// New trades, newest first (already prepended to `wallet.trades`).
    new: Vec<TrackedTrade>,
    wallet: Wallet,
    events: Vec<TradeEvent>,
}

pub async fn run(
    api: &dyn PolyApi,
    data: &DataDir,
    cfg: &Config,
    opts: &TrackOpts,
) -> anyhow::Result<TrackReport> {
    let started = Instant::now();
    let now = opts.now;
    let tc = &cfg.tracker;
    let mut watchlist = data.watchlist()?;
    let mut cursors = data.cursors()?;
    let mut report = TrackReport::default();

    // 1. Evict.
    let (keep, evicted): (Vec<_>, Vec<_>) = watchlist
        .entries
        .drain(..)
        .partition(|e| !should_evict(e, cfg, now));
    watchlist.entries = keep;
    report.evicted = evicted.into_iter().map(|e| e.addr).collect();
    report.watchlist_size = watchlist.entries.len() as u32;
    if !report.evicted.is_empty() {
        info!(count = report.evicted.len(), "evicted watchlist entries");
    }

    // 2. Order and poll concurrently within the time budget.
    let order = poll_order(&watchlist.entries);
    let budget = std::time::Duration::from_secs(tc.time_budget_s);
    let entries = &watchlist.entries;
    let outcomes: Vec<Outcome> = futures::stream::iter(order)
        .map(|idx| async move {
            if started.elapsed() >= budget {
                return Outcome::Deferred;
            }
            poll_wallet(api, data, cfg, now, idx, &entries[idx]).await
        })
        .buffer_unordered(cfg.api.max_concurrency.max(1))
        .collect()
        .await;

    // 3. Merge results.
    let mut changed: Vec<Wallet> = Vec::new();
    let mut failed = 0u32;
    for outcome in outcomes {
        match outcome {
            Outcome::Deferred => report.deferred += 1,
            Outcome::Failed => failed += 1,
            Outcome::Polled(p) => {
                let p = *p;
                report.polled += 1;
                let entry = &mut watchlist.entries[p.idx];
                entry.last_activity_ts = entry.last_activity_ts.max(p.max_ts);
                entry.last_polled = now;
                report.events.extend(p.events);
                if !p.new.is_empty() {
                    report.new_trades += p.new.len() as u32;
                    report
                        .archive
                        .extend(p.new.into_iter().map(|t| (entry.addr.clone(), t)));
                    changed.push(p.wallet);
                }
            }
        }
    }
    // Deterministic output regardless of completion order.
    report.archive.sort_by(|(a, x), (b, y)| {
        y.ts.cmp(&x.ts)
            .then_with(|| a.cmp(b))
            .then_with(|| x.key.cmp(&y.key))
    });
    report.events.sort_by(|x, y| {
        y.trade
            .ts
            .cmp(&x.trade.ts)
            .then_with(|| x.addr.cmp(&y.addr))
    });

    if report.deferred > 0 {
        warn!(
            deferred = report.deferred,
            budget_s = tc.time_budget_s,
            "time budget used up; rest polled next run"
        );
    }
    if failed > 0 {
        warn!(failed, "wallet polls failed; retried next run");
    }

    // 7. Persist.
    if opts.dry_run {
        info!(changed = changed.len(), "dry run: nothing written");
    } else {
        for w in &changed {
            data.save_wallet(w)?;
        }
        data.save_watchlist(&watchlist)?;
        cursors.last_track = now;
        data.save_cursors(&cursors)?;
    }

    report.requests = api.stats();
    report.duration_s = started.elapsed().as_secs_f64();
    info!(
        watchlist = report.watchlist_size,
        polled = report.polled,
        deferred = report.deferred,
        failed,
        new_trades = report.new_trades,
        events = report.events.len(),
        duration_s = report.duration_s,
        "tracker run done"
    );
    Ok(report)
}

/// Eviction rule: below the score floor too long (`last_qualified`), or no activity
/// too long (`last_activity_ts`); both clocks start at `added_at` at the earliest.
pub fn should_evict(e: &WatchlistEntry, cfg: &Config, now: i64) -> bool {
    let tc = &cfg.tracker;
    now - e.last_qualified.max(e.added_at) > tc.evict_low_score_s
        || now - e.last_activity_ts.max(e.added_at) > tc.evict_inactive_s
}

/// Indices of `entries` in poll order: score desc, `last_polled` asc, addr asc.
pub fn poll_order(entries: &[WatchlistEntry]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..entries.len()).collect();
    order.sort_by(|&a, &b| {
        let (x, y) = (&entries[a], &entries[b]);
        y.score
            .cmp(&x.score)
            .then(x.last_polled.cmp(&y.last_polled))
            .then_with(|| x.addr.cmp(&y.addr))
    });
    order
}

/// `start` for the next activity poll of `e`.
pub fn poll_start(e: &WatchlistEntry) -> i64 {
    if e.last_activity_ts == 0 {
        e.added_at - FIRST_POLL_LOOKBACK_S
    } else {
        e.last_activity_ts + 1 - POLL_OVERLAP_S
    }
}

async fn poll_wallet(
    api: &dyn PolyApi,
    data: &DataDir,
    cfg: &Config,
    now: i64,
    idx: usize,
    entry: &WatchlistEntry,
) -> Outcome {
    let addr = &entry.addr;
    let rows = match fetch_activity(api, addr, poll_start(entry)).await {
        Ok(rows) => rows,
        Err(e) => {
            warn!(%addr, error = %e, "activity poll failed");
            return Outcome::Failed;
        }
    };
    let max_ts = rows.iter().map(|r| r.timestamp).max().unwrap_or(0);

    let mut wallet = match data.load_wallet(addr) {
        Ok(Some(w)) => w,
        Ok(None) => Wallet::new(addr.clone(), now),
        Err(e) => {
            warn!(%addr, error = %e, "could not load wallet file; skipping");
            return Outcome::Failed;
        }
    };

    let new = merge_trades(&mut wallet, &rows, cfg.tracker.max_trades_per_wallet);
    if !new.is_empty() && cfg.tracker.refresh_positions {
        refresh_wallet(api, &mut wallet).await;
    }
    let issue = entry.issue.or(wallet.issue);
    // The first poll backfills a week of history: old trades are not news.
    let backfill = entry.last_activity_ts == 0;
    let events = new
        .iter()
        .filter(|_| !backfill)
        .filter_map(|t| {
            big_trade_reason(t, wallet.value, cfg).map(|reason| TradeEvent {
                addr: addr.clone(),
                trade: t.clone(),
                wallet_value: wallet.value,
                issue,
                reason,
            })
        })
        .collect();
    debug!(%addr, rows = rows.len(), new = new.len(), "polled");
    Outcome::Polled(Box::new(Polled {
        idx,
        max_ts,
        new,
        wallet,
        events,
    }))
}

/// All activity rows with `timestamp >= start`, newest first, at most
/// [`MAX_ACTIVITY_PAGES`] pages.
async fn fetch_activity(
    api: &dyn PolyApi,
    user: &Addr,
    start: i64,
) -> anyhow::Result<Vec<ActivityRow>> {
    let q = ActivityQuery {
        user: user.clone(),
        start: Some(start),
        end: None,
        limit: ACTIVITY_PAGE_LIMIT,
    };
    let mut rows = Vec::new();
    let mut cursor: Option<String> = None;
    for page_no in 0.. {
        if page_no == MAX_ACTIVITY_PAGES {
            let oldest = rows
                .iter()
                .map(|r: &ActivityRow| r.timestamp)
                .min()
                .unwrap_or(start);
            warn!(%user, pages = MAX_ACTIVITY_PAGES, start, oldest, "activity page cap hit; skipping older rows in the gap");
            break;
        }
        let page = api.activity(&q, cursor.as_deref()).await?;
        let empty = page.data.is_empty();
        rows.extend(page.data);
        match page.next_cursor {
            Some(c) if !empty && !c.is_empty() => cursor = Some(c),
            _ => break,
        }
    }
    Ok(rows)
}

/// Converts `rows`, drops ones already in `wallet.trades` (or duplicated within
/// `rows`), prepends the rest newest first and caps the list at `max`. Returns the
/// new trades, newest first.
fn merge_trades(wallet: &mut Wallet, rows: &[ActivityRow], max: usize) -> Vec<TrackedTrade> {
    let mut seen: HashSet<String> = wallet.trades.iter().map(|t| t.key.clone()).collect();
    let mut new: Vec<TrackedTrade> = rows
        .iter()
        .filter_map(from_activity)
        .filter(|t| seen.insert(t.key.clone()))
        .collect();
    new.sort_by_key(|t| std::cmp::Reverse(t.ts));
    if !new.is_empty() {
        let mut trades = new.clone();
        trades.append(&mut wallet.trades);
        trades.truncate(max);
        wallet.trades = trades;
    }
    new
}

/// Replaces the wallet's OPEN positions (other statuses kept) and its value.
/// Errors keep the old data.
async fn refresh_wallet(api: &dyn PolyApi, wallet: &mut Wallet) {
    let addr = wallet.addr.clone();
    match fetch_open_positions(api, &addr).await {
        Ok(open) => {
            wallet.positions.retain(|p| p.status != PosStatus::Open);
            wallet.positions.extend(open);
        }
        Err(e) => warn!(%addr, error = %e, "open positions refresh failed"),
    }
    match api.value(&addr).await {
        Ok(v) => wallet.value = v,
        Err(e) => warn!(%addr, error = %e, "value refresh failed"),
    }
}

async fn fetch_open_positions(api: &dyn PolyApi, user: &Addr) -> anyhow::Result<Vec<Position>> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_POSITION_PAGES {
        let page = api
            .positions(user, PosStatus::Open, cursor.as_deref())
            .await?;
        let empty = page.data.is_empty();
        out.extend(
            page.data
                .into_iter()
                .map(Position::from)
                .filter(|p| p.status == PosStatus::Open),
        );
        match page.next_cursor {
            Some(c) if !empty && !c.is_empty() => cursor = Some(c),
            _ => return Ok(out),
        }
    }
    warn!(%user, pages = MAX_POSITION_PAGES, "open positions page cap hit");
    Ok(out)
}

/// Why `t` deserves a comment, if it does: Buy/Sell/Redeem ≥ `big_trade_usd`, or
/// Buy/Sell ≥ `big_trade_share` of a positive wallet value.
pub fn big_trade_reason(t: &TrackedTrade, wallet_value: f64, cfg: &Config) -> Option<String> {
    let tc = &cfg.tracker;
    let absolute = matches!(t.kind, TradeKind::Buy | TradeKind::Sell | TradeKind::Redeem);
    if absolute && t.usdc >= tc.big_trade_usd {
        return Some(format!("≥ ${:.0}", tc.big_trade_usd));
    }
    let share_kind = matches!(t.kind, TradeKind::Buy | TradeKind::Sell);
    if share_kind && wallet_value > 0.0 && t.usdc >= tc.big_trade_share * wallet_value {
        return Some(format!(
            "{:.0} % of wallet value",
            100.0 * t.usdc / wallet_value
        ));
    }
    None
}

/// Hex sha256 of `tx|token_id|kind|size|ts` (size formatted with 6 decimals).
pub fn trade_key(tx: &str, token_id: &str, kind: TradeKind, size: f64, ts: i64) -> String {
    let s = format!("{tx}|{token_id}|{kind:?}|{size:.6}|{ts}");
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// `TRADE` → Buy/Sell by `side`, `REDEEM`/`SPLIT`/`MERGE`/`DEPOSIT`/`WITHDRAWAL` mapped,
/// `REWARD`/`CONVERSION`/unknown → `None` (not tracked).
pub fn from_activity(row: &ActivityRow) -> Option<TrackedTrade> {
    let kind = match row.kind {
        ActivityType::Trade => match row.side? {
            Side::Buy => TradeKind::Buy,
            Side::Sell => TradeKind::Sell,
        },
        ActivityType::Redeem => TradeKind::Redeem,
        ActivityType::Split => TradeKind::Split,
        ActivityType::Merge => TradeKind::Merge,
        ActivityType::Deposit => TradeKind::Deposit,
        ActivityType::Withdrawal => TradeKind::Withdraw,
        ActivityType::Reward | ActivityType::Conversion | ActivityType::Other => return None,
    };
    let usdc = if row.usdc_size > 0.0 {
        row.usdc_size
    } else {
        row.size * row.price
    };
    let title = if row.title.is_empty() && row.is_combo {
        "(combo)".to_string()
    } else {
        row.title.clone()
    };
    Some(TrackedTrade {
        key: trade_key(
            &row.transaction_hash,
            &row.token_id,
            kind,
            row.size,
            row.timestamp,
        ),
        ts: row.timestamp,
        kind,
        condition_id: row.condition_id.clone(),
        token_id: row.token_id.clone(),
        title,
        outcome: row.outcome.clone(),
        price: row.price,
        size: row.size,
        usdc,
        tx: row.transaction_hash.clone(),
    })
}

#[cfg(test)]
mod tests;
