//! Watchlist admission, markets-cache pruning and report rows.

use std::collections::{HashMap, HashSet};

use types::config::SignalConfig;
use types::site::CandidateRow;
use types::state::{AlertsState, MarketCache, Watchlist, WatchlistEntry};
use types::{Addr, Config, PosStatus, Position, SignalKind, Wallet};

/// Cache entries no saved wallet references are dropped after this long.
pub const MARKET_CACHE_MAX_AGE_S: i64 = 30 * 86_400;

fn weight(kind: SignalKind, cfg: &SignalConfig) -> f64 {
    match kind {
        SignalKind::NewAccountLongshot => cfg.w_new_account_longshot,
        SignalKind::OldAccountWinRate => cfg.w_old_account_win_rate,
        SignalKind::LeaderboardEntrant => cfg.w_leaderboard_entrant,
        SignalKind::LongshotWinner => cfg.w_longshot_winner,
        SignalKind::Concentration => cfg.w_concentration,
        SignalKind::ResolutionTiming => cfg.w_resolution_timing,
        SignalKind::PreMove => cfg.w_pre_move,
        SignalKind::NicheMarket => cfg.w_niche_market,
        SignalKind::LinkedAccounts => cfg.w_linked_accounts,
    }
}

/// The signal contributing most to the score (weight × strength).
pub fn strongest_signal(w: &Wallet, cfg: &SignalConfig) -> Option<SignalKind> {
    w.signals
        .iter()
        .max_by(|a, b| {
            (weight(a.kind, cfg) * a.strength)
                .total_cmp(&(weight(b.kind, cfg) * b.strength))
                .then_with(|| b.kind.cmp(&a.kind))
        })
        .map(|s| s.kind)
}

/// Update scores of existing entries from freshly scored wallets, admit new wallets
/// ≥ `watchlist_min_score`, cap at `watchlist_max` keeping the highest scores.
/// Returns the number of admitted wallets that survived the cap.
pub fn update_watchlist(wl: &mut Watchlist, scored: &[&Wallet], cfg: &Config, now: i64) -> u32 {
    let min = cfg.tracker.watchlist_min_score;
    let by_addr: HashMap<&Addr, &Wallet> = scored.iter().map(|w| (&w.addr, *w)).collect();
    for e in &mut wl.entries {
        if let Some(w) = by_addr.get(&e.addr) {
            e.score = w.score.total;
            if e.score >= min {
                e.last_qualified = now;
            }
            if e.issue.is_none() {
                e.issue = w.issue;
            }
        }
    }
    // Re-scored to 0 (every signal gone, or a bot): stop tracking now rather than after
    // `evict_low_score_s`; alerts then closes the wallet's issue.
    wl.entries
        .retain(|e| by_addr.get(&e.addr).is_none_or(|w| w.score.total > 0));
    let present: HashSet<Addr> = wl.entries.iter().map(|e| e.addr.clone()).collect();
    let mut added: HashSet<Addr> = HashSet::new();
    let mut fresh: Vec<&&Wallet> = scored
        .iter()
        .filter(|w| w.score.total >= min && !present.contains(&w.addr))
        .collect();
    fresh.sort_by(|a, b| a.addr.cmp(&b.addr));
    for w in fresh {
        let Some(reason) = strongest_signal(w, &cfg.signals) else {
            continue;
        };
        if !added.insert(w.addr.clone()) {
            continue;
        }
        wl.entries.push(WatchlistEntry {
            addr: w.addr.clone(),
            added_at: now,
            reason,
            score: w.score.total,
            last_activity_ts: 0,
            last_polled: 0,
            last_qualified: now,
            issue: w.issue,
        });
    }
    wl.entries
        .sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.addr.cmp(&b.addr)));
    wl.entries.truncate(cfg.tracker.watchlist_max);
    wl.entries
        .iter()
        .filter(|e| added.contains(&e.addr))
        .count() as u32
}

/// Drop cache entries not referenced by any saved wallet and fetched more than 30 days ago.
/// Open and unredeemed positions kept in a wallet file (largest cost first).
pub const STORED_HELD_POSITIONS: usize = 200;
/// Closed positions kept in a wallet file (largest |realized PnL| first).
pub const STORED_CLOSED_POSITIONS: usize = 50;

/// The wallet as written to the `data` branch: scoring already used every position,
/// the file only feeds the dashboard and alerts, so active traders' long tails of
/// closed positions are trimmed (they made files of ~1 MB, rewritten every run).
pub fn for_storage(w: &Wallet) -> Wallet {
    let (mut held, mut closed): (Vec<&Position>, Vec<&Position>) = w
        .positions
        .iter()
        .partition(|p| p.status != PosStatus::Closed);
    held.sort_by(|a, b| b.cost.total_cmp(&a.cost));
    held.truncate(STORED_HELD_POSITIONS);
    closed.sort_by(|a, b| b.realized_pnl.abs().total_cmp(&a.realized_pnl.abs()));
    closed.truncate(STORED_CLOSED_POSITIONS);
    let mut out = w.clone();
    out.positions = held.into_iter().chain(closed).cloned().collect();
    out
}

pub fn prune_markets(cache: &mut MarketCache, referenced: &HashSet<String>, now: i64) {
    let cutoff = now - MARKET_CACHE_MAX_AGE_S;
    let fetched = cache.fetched_at.clone();
    cache.markets.retain(|cid, _| {
        referenced.contains(cid) || fetched.get(cid).copied().unwrap_or(0) >= cutoff
    });
    let markets = &cache.markets;
    cache.fetched_at.retain(|cid, _| markets.contains_key(cid));
}

/// Profile name, else pseudonym, else the short address (same as `store::render_site`).
fn display_name(w: &Wallet) -> String {
    let profile = w.profile.as_ref();
    [profile.map(|p| &p.name), profile.map(|p| &p.pseudonym)]
        .into_iter()
        .flatten()
        .find(|s| !s.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| w.addr.short())
}

/// Same field meanings as `store::render_site`'s `index.json` rows.
pub fn candidate_row(
    w: &Wallet,
    now: i64,
    watched: &HashSet<Addr>,
    alerts: &AlertsState,
) -> CandidateRow {
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
}
