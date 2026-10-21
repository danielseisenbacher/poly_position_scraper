//! Dashboard JSON rendering (`index.json`, `markets.json`, `activity_recent.json`, `meta.json`).

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;
use types::api::RequestStats;
use types::site::{ApiHealth, CandidateRow, Counts, MarketView, MarketWallet, Meta, TapeRow};
use types::state::{AlertsState, MarketCache};
use types::{Addr, Config, PosStatus, Tier, Wallet};

use crate::{paths, DataDir, StoreResult};

/// Inputs for `meta.json` that only the caller knows.
#[derive(Debug, Clone, Default)]
pub struct MetaInput {
    pub now: i64,
    pub run_id: Option<String>,
    pub api: RequestStats,
    pub feed_lag_s: i64,
    pub feed_truncated: bool,
    pub wallets_scanned: u32,
    pub trades_tracked: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct SiteSummary {
    pub candidates: usize,
    pub markets: usize,
    pub tape_rows: usize,
    pub bytes_written: u64,
}

/// Rebuild `index.json`, `markets.json`, `activity_recent.json` and `meta.json` from
/// `wallets/*`, `watchlist.json`, `clusters.json`, `cursors.json` and the markets cache.
/// Idempotent. Enforces the caps in `cfg.site` / `cfg.tracker`.
pub fn render_site(dir: &DataDir, cfg: &Config, meta: &MetaInput) -> StoreResult<SiteSummary> {
    let wallets = dir.load_all_wallets()?;
    let watched: HashSet<Addr> = dir
        .watchlist()?
        .entries
        .into_iter()
        .map(|e| e.addr)
        .collect();
    let alerts = dir.alerts()?;
    let n_clusters = dir.clusters()?.len();
    let cursors = dir.cursors()?;
    let cache = dir.markets_cache()?;

    let index = candidate_rows(&wallets, cfg, meta.now, &watched, &alerts);
    let markets = market_views(&wallets, cfg, &cache);
    let tape = tape_rows(&wallets, cfg, meta.now);

    let mut score_histogram = [0u32; 10];
    for w in &wallets {
        score_histogram[usize::from(w.score.total / 10).min(9)] += 1;
    }
    let mut signal_counts = BTreeMap::new();
    for (kind, _) in index.iter().flat_map(|r| &r.signals) {
        *signal_counts.entry(*kind).or_insert(0u32) += 1;
    }
    let tier_count = |t: Tier| index.iter().filter(|r| r.tier == t).count() as u32;
    let meta_out = Meta {
        generated_at: meta.now,
        run_id: meta.run_id.clone(),
        last_discover: cursors.last_discover,
        last_track: cursors.last_track,
        counts: Counts {
            candidates: index.len() as u32,
            watchlist: watched.len() as u32,
            high: tier_count(Tier::High),
            medium: tier_count(Tier::Medium),
            clusters: n_clusters as u32,
            wallets_scanned_last_run: meta.wallets_scanned,
            trades_tracked_last_run: meta.trades_tracked,
        },
        api: ApiHealth::from(&meta.api),
        feed_lag_s: meta.feed_lag_s,
        feed_truncated: meta.feed_truncated,
        score_histogram,
        signal_counts,
    };

    let bytes_written = dir.write_counted(paths::INDEX, &index)?
        + dir.write_counted(paths::MARKETS, &markets)?
        + dir.write_counted(paths::ACTIVITY_RECENT, &tape)?
        + dir.write_counted(paths::META, &meta_out)?;
    Ok(SiteSummary {
        candidates: index.len(),
        markets: markets.len(),
        tape_rows: tape.len(),
        bytes_written,
    })
}

/// Profile name, else pseudonym, else the short address.
fn display_name(w: &Wallet) -> String {
    let profile = w.profile.as_ref();
    [profile.map(|p| &p.name), profile.map(|p| &p.pseudonym)]
        .into_iter()
        .flatten()
        .find(|s| !s.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| w.addr.short())
}

fn candidate_rows(
    wallets: &[Wallet],
    cfg: &Config,
    now: i64,
    watched: &HashSet<Addr>,
    alerts: &AlertsState,
) -> Vec<CandidateRow> {
    let mut picked: Vec<&Wallet> = wallets
        .iter()
        .filter(|w| w.score.total >= cfg.site.min_score)
        .collect();
    picked.sort_by(|a, b| {
        b.score
            .total
            .cmp(&a.score.total)
            .then_with(|| a.addr.cmp(&b.addr))
    });
    picked.truncate(cfg.site.max_candidates);
    picked
        .into_iter()
        .map(|w| {
            let mut signals: Vec<_> = w.signals.iter().map(|s| (s.kind, s.strength)).collect();
            signals.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
            let pnl = match &w.stats {
                Some(s) => s.realized_pnl,
                None => w.positions.iter().map(|p| p.total_pnl).sum(),
            };
            let n_markets = w
                .positions
                .iter()
                .map(|p| p.condition_id.as_str())
                .collect::<HashSet<_>>()
                .len();
            let last_trade_ts = w
                .trades
                .iter()
                .map(|t| t.ts)
                .max()
                .or_else(|| w.positions.iter().filter_map(|p| p.last_event_at).max());
            CandidateRow {
                addr: w.addr.clone(),
                name: display_name(w),
                score: w.score.total,
                tier: w.score.tier,
                signals,
                age_d: w.age_days(now),
                pnl,
                win_rate: w.resolved.win_rate(),
                n_resolved: w.resolved.n,
                z: w.resolved.z,
                value: w.value,
                n_markets: n_markets as u32,
                last_trade_ts,
                cluster_id: w.cluster_id.clone(),
                issue: w
                    .issue
                    .or_else(|| alerts.wallets.get(&w.addr).map(|r| r.issue)),
                watched: watched.contains(&w.addr),
                first_seen: w.first_seen,
            }
        })
        .collect()
}

fn market_views(wallets: &[Wallet], cfg: &Config, cache: &MarketCache) -> Vec<MarketView> {
    // condition_id → (view, Σ size); BTreeMap keeps the pre-sort order deterministic.
    let mut by_market: BTreeMap<&str, (MarketView, f64)> = BTreeMap::new();
    for w in wallets
        .iter()
        .filter(|w| w.score.total >= cfg.signals.tier_medium)
    {
        let held = w.positions.iter().filter(|p| {
            matches!(
                p.status,
                PosStatus::Open | PosStatus::Redeemable | PosStatus::RedeemableLost
            )
        });
        for p in held {
            let (view, size) = by_market.entry(&p.condition_id).or_insert_with(|| {
                let meta = cache.get(&p.condition_id);
                let title = meta.map(|m| m.question.clone()).filter(|t| !t.is_empty());
                let slug = meta.map(|m| m.slug.clone()).filter(|s| !s.is_empty());
                let view = MarketView {
                    condition_id: p.condition_id.clone(),
                    title: title.unwrap_or_else(|| p.title.clone()),
                    slug: slug.unwrap_or_else(|| p.slug.clone()),
                    end_date: meta.and_then(|m| m.end_date).or(p.end_date),
                    volume: meta.and_then(|m| m.volume).or(p.market_volume),
                    resolved_outcome: meta
                        .and_then(|m| m.winning_index.and_then(|i| m.outcomes.get(i)))
                        .cloned(),
                    wallets: Vec::new(),
                };
                (view, 0.0)
            });
            *size += p.total_size;
            // Without cached metadata (tracker runs), a won position still names the winner.
            if view.resolved_outcome.is_none() && p.won() == Some(true) {
                view.resolved_outcome = Some(p.outcome.clone());
            }
            view.wallets.push(MarketWallet {
                addr: w.addr.clone(),
                score: w.score.total,
                outcome: p.outcome.clone(),
                avg_price: p.avg_price,
                size: p.total_size,
                current_price: p.current_price,
            });
        }
    }
    let distinct = |v: &MarketView| {
        v.wallets
            .iter()
            .map(|w| &w.addr)
            .collect::<HashSet<_>>()
            .len()
    };
    let mut markets: Vec<(usize, f64, MarketView)> = by_market
        .into_values()
        .map(|(v, size)| (distinct(&v), size, v))
        .filter(|(n, _, _)| *n >= 2)
        .collect();
    markets.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.total_cmp(&a.1)));
    markets.truncate(cfg.site.max_markets);
    markets
        .into_iter()
        .map(|(_, _, mut v)| {
            v.wallets.sort_by(|a, b| {
                b.score
                    .cmp(&a.score)
                    .then_with(|| a.addr.cmp(&b.addr))
                    .then_with(|| a.outcome.cmp(&b.outcome))
            });
            v
        })
        .collect()
}

fn tape_rows(wallets: &[Wallet], cfg: &Config, now: i64) -> Vec<TapeRow> {
    let since = now - cfg.tracker.tape_window_s;
    let mut rows: Vec<(&Wallet, &types::TrackedTrade)> = wallets
        .iter()
        .flat_map(|w| {
            w.trades
                .iter()
                .filter(|t| t.ts >= since)
                .map(move |t| (w, t))
        })
        .collect();
    rows.sort_by(|a, b| {
        b.1.ts
            .cmp(&a.1.ts)
            .then_with(|| a.0.addr.cmp(&b.0.addr))
            .then_with(|| a.1.key.cmp(&b.1.key))
    });
    rows.truncate(cfg.tracker.tape_max_rows);
    rows.into_iter()
        .map(|(w, t)| TapeRow {
            addr: w.addr.clone(),
            name: display_name(w),
            score: w.score.total,
            ts: t.ts,
            kind: t.kind,
            title: t.title.clone(),
            outcome: t.outcome.clone(),
            price: t.price,
            usdc: t.usdc,
            condition_id: t.condition_id.clone(),
            tx: t.tx.clone(),
        })
        .collect()
}
