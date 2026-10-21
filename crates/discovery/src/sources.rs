//! Non-feed candidate sources: PnL leaderboards, biggest winners, longshot winners of
//! recently resolved markets, and Deposit Wallet Factory logs.

use std::collections::HashMap;

use futures::StreamExt;
use signals::{LeaderboardHit, LongshotHit};
use tracing::{debug, info, warn};
use types::api::{
    BiggestWinnerRow, FactoryLog, LeaderboardRow, LeaderboardSort, PolyApi, PositionRow, TimePeriod,
};
use types::{Addr, Config, MarketCategory, MarketMeta, PosStatus};

/// Guard against a misbehaving cursor on the leaderboard endpoints.
const MAX_BOARD_PAGES: usize = 10;
/// Factory logs: blocks scanned back from head on the first run (~1 h of Polygon blocks).
pub const FIRST_RUN_BLOCKS: u64 = 1800;
/// Factory logs: max blocks scanned per run.
pub const MAX_FACTORY_SPAN: u64 = 10_000;
/// Closed markets listed per run: Gamma's whole offset window (21 pages of 100). Even
/// with the volume floor ~90 % are sports/crypto (filtered client-side) and ~50 close
/// per hour, so a smaller cap would cover only the last few hours and the cursor, which
/// advances to `now`, would skip the rest.
const MAX_LISTED_MARKETS: usize = 2100;
/// `/v2/positions?condition=` pages (1000 rows each) per status per resolved market.
const MARKET_POSITION_PAGES: usize = 3;

#[derive(Debug, Default)]
pub struct Boards {
    pub hits: HashMap<Addr, Vec<LeaderboardHit>>,
    /// Qualifying biggest-winner rows per wallet.
    pub wins: HashMap<Addr, Vec<BiggestWinnerRow>>,
}

/// Day + week PnL leaderboards (first `leaderboard_depth` rows) and day + week biggest winners.
pub async fn boards(api: &dyn PolyApi, cfg: &Config) -> Boards {
    let depth = cfg.discovery.leaderboard_depth as usize;
    let (lb_day, lb_week, bw_day, bw_week) = futures::join!(
        leaderboard(api, TimePeriod::Day, depth),
        leaderboard(api, TimePeriod::Week, depth),
        biggest_winners(api, TimePeriod::Day, depth),
        biggest_winners(api, TimePeriod::Week, depth),
    );
    let mut out = Boards::default();
    for (period, rows) in [(TimePeriod::Day, lb_day), (TimePeriod::Week, lb_week)] {
        for (i, r) in rows.into_iter().enumerate() {
            let rank = if r.rank > 0 { r.rank } else { i as u32 + 1 };
            out.hits.entry(r.user_id).or_default().push(LeaderboardHit {
                period,
                rank,
                pnl: r.pnl,
            });
        }
    }
    let s = &cfg.signals;
    for r in bw_day.into_iter().chain(bw_week) {
        let qualifies = r.initial_value >= s.biggest_win_min_initial
            && r.initial_value > 0.0
            && r.pnl / r.initial_value >= s.biggest_win_min_multiple;
        if qualifies {
            let rows = out.wins.entry(r.user_id.clone()).or_default();
            if !rows.iter().any(|x| x.condition_id == r.condition_id) {
                rows.push(r);
            }
        }
    }
    info!(
        leaderboard = out.hits.len(),
        biggest_winners = out.wins.len(),
        "leaderboards read"
    );
    out
}

async fn leaderboard(api: &dyn PolyApi, period: TimePeriod, depth: usize) -> Vec<LeaderboardRow> {
    let mut rows = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_BOARD_PAGES {
        if rows.len() >= depth {
            break;
        }
        match api
            .leaderboard(period, LeaderboardSort::Pnl, cursor.as_deref())
            .await
        {
            Ok(p) => {
                let empty = p.data.is_empty();
                rows.extend(p.data);
                match p.next_cursor {
                    Some(c) if !empty => cursor = Some(c),
                    _ => break,
                }
            }
            Err(e) => {
                warn!(?period, "leaderboard failed: {e}");
                break;
            }
        }
    }
    rows.truncate(depth);
    rows
}

async fn biggest_winners(
    api: &dyn PolyApi,
    period: TimePeriod,
    depth: usize,
) -> Vec<BiggestWinnerRow> {
    let mut rows = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_BOARD_PAGES {
        if rows.len() >= depth {
            break;
        }
        match api.biggest_winners(period, cursor.as_deref()).await {
            Ok(p) => {
                let empty = p.data.is_empty();
                rows.extend(p.data);
                match p.next_cursor {
                    Some(c) if !empty => cursor = Some(c),
                    _ => break,
                }
            }
            Err(e) => {
                warn!(?period, "biggest winners failed: {e}");
                break;
            }
        }
    }
    rows.truncate(depth);
    rows
}

#[derive(Debug, Default)]
pub struct Longshots {
    pub hits: HashMap<Addr, Vec<LongshotHit>>,
    /// The resolved markets inspected (inserted into the markets cache).
    pub markets: Vec<MarketMeta>,
    /// The closed-markets listing succeeded (the cursor may advance).
    pub listed: bool,
}

/// Resolved, long enough, at least `longshot_min_volume`, not crypto (price markets:
/// a longshot win is public price action) and not sports when `longshot_skip_sports`
/// (upsets are public-information events); top `max_resolved_markets` by volume.
pub fn pick_resolved_markets(mut markets: Vec<MarketMeta>, cfg: &Config) -> Vec<MarketMeta> {
    let d = &cfg.discovery;
    markets.retain(|m| {
        let Some(hours) = m.duration_h() else {
            return false;
        };
        m.winning_token().is_some()
            && hours >= d.min_market_duration_h
            && m.volume.is_none_or(|v| v >= d.longshot_min_volume)
            && m.category != MarketCategory::Crypto
            && !(d.longshot_skip_sports && m.category == MarketCategory::Sports)
    });
    markets.sort_by(|a, b| {
        b.volume
            .unwrap_or(0.0)
            .total_cmp(&a.volume.unwrap_or(0.0))
            .then_with(|| a.condition_id.cmp(&b.condition_id))
    });
    markets.dedup_by(|a, b| a.condition_id == b.condition_id);
    markets.truncate(cfg.discovery.max_resolved_markets);
    markets
}

/// Holders of winning tokens that traded as longshots shortly before the market closed.
pub async fn longshots(api: &dyn PolyApi, cfg: &Config, since: i64, now: i64) -> Longshots {
    let d = &cfg.discovery;
    let listed = match api
        .recently_closed_markets(since, d.longshot_min_volume, MAX_LISTED_MARKETS)
        .await
    {
        Ok(m) => m,
        Err(e) => {
            warn!("recently closed markets failed, skipping longshot source: {e}");
            return Longshots::default();
        }
    };
    let markets = pick_resolved_markets(listed, cfg);
    let found: Vec<Vec<(Addr, LongshotHit)>> = futures::stream::iter(markets.iter())
        .map(|m| longshot_holders(api, cfg, m, now))
        .buffer_unordered(cfg.api.max_concurrency.max(1))
        .collect()
        .await;
    let mut hits: HashMap<Addr, Vec<LongshotHit>> = HashMap::new();
    for (addr, hit) in found.into_iter().flatten() {
        hits.entry(addr).or_default().push(hit);
    }
    for v in hits.values_mut() {
        v.sort_by(|a, b| a.condition_id.cmp(&b.condition_id));
    }
    info!(
        markets = markets.len(),
        holders = hits.len(),
        "longshot winners read"
    );
    Longshots {
        hits,
        markets,
        listed: true,
    }
}

async fn longshot_holders(
    api: &dyn PolyApi,
    cfg: &Config,
    m: &MarketMeta,
    now: i64,
) -> Vec<(Addr, LongshotHit)> {
    let Some(token) = m.winning_token() else {
        return Vec::new();
    };
    let closed = m.closed_time.or(m.end_date).unwrap_or(now);
    let path = match api
        .prices_history(token, closed - cfg.discovery.longshot_window_s, closed)
        .await
    {
        Ok(p) => p,
        Err(e) => {
            warn!(market = %m.condition_id, "prices history failed: {e}");
            return Vec::new();
        }
    };
    let Some(low) = path.iter().map(|p| p.price).reduce(f64::min) else {
        return Vec::new();
    };
    if low > cfg.discovery.longshot_price {
        return Vec::new();
    }
    // `/v2/holders` misses winners once they are auto-redeemed: current holders come
    // without `status`, redeemed (and sold) positions only with `status=CLOSED`.
    let (current, redeemed) = futures::join!(
        market_positions(api, &m.condition_id, None),
        market_positions(api, &m.condition_id, Some(PosStatus::Closed)),
    );
    let (n_current, n_redeemed) = (current.len(), redeemed.len());
    let entries = winning_entries(current.into_iter().chain(redeemed), token, cfg);
    debug!(
        market = %m.condition_id,
        low,
        n_current,
        n_redeemed,
        hits = entries.len(),
        "longshot market"
    );
    entries
        .into_iter()
        .map(|p| {
            (
                p.proxy_wallet,
                LongshotHit {
                    condition_id: m.condition_id.clone(),
                    title: m.question.clone(),
                    token_id: token.to_string(),
                    entry_price: p.avg_price,
                    low_price_before_close: low,
                    closed_at: closed,
                    amount: p.total_size,
                },
            )
        })
        .collect()
}

/// Positions on the winning `token` entered at ≤ `longshot_holder_max_entry` with a cost
/// basis of at least `min_value_usd / 4`, one per wallet (lowest entry), sorted by wallet.
pub(crate) fn winning_entries(
    rows: impl IntoIterator<Item = PositionRow>,
    token: &str,
    cfg: &Config,
) -> Vec<PositionRow> {
    let s = &cfg.signals;
    let min_cost = s.min_value_usd / 4.0;
    let mut best: HashMap<Addr, PositionRow> = HashMap::new();
    for p in rows {
        if p.token_id != token
            || p.avg_price <= 0.0
            || p.avg_price > s.longshot_holder_max_entry
            || p.avg_price * p.total_size < min_cost
        {
            continue;
        }
        match best.get(&p.proxy_wallet) {
            Some(b) if b.avg_price <= p.avg_price => {}
            _ => {
                best.insert(p.proxy_wallet.clone(), p);
            }
        }
    }
    let mut out: Vec<PositionRow> = best.into_values().collect();
    out.sort_by(|a, b| a.proxy_wallet.cmp(&b.proxy_wallet));
    out
}

/// Up to [`MARKET_POSITION_PAGES`] pages of one market's positions; a failed page keeps
/// what was read so far.
async fn market_positions(
    api: &dyn PolyApi,
    condition_id: &str,
    status: Option<PosStatus>,
) -> Vec<PositionRow> {
    let mut rows = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MARKET_POSITION_PAGES {
        match api
            .market_positions(condition_id, status, None, cursor.as_deref())
            .await
        {
            Ok(p) => {
                let empty = p.data.is_empty();
                rows.extend(p.data);
                match p.next_cursor {
                    Some(c) if !empty => cursor = Some(c),
                    _ => break,
                }
            }
            Err(e) => {
                warn!(market = %condition_id, ?status, "market positions failed: {e}");
                break;
            }
        }
    }
    rows
}

#[derive(Debug, Default)]
pub struct FactoryScan {
    /// Oldest first.
    pub logs: Vec<FactoryLog>,
    pub head: u64,
    /// New value for `cursors.factory_last_block`.
    pub last_block: u64,
}

/// Block range to scan: from the cursor (or `head − FIRST_RUN_BLOCKS` on the first run)
/// to head, at most `MAX_FACTORY_SPAN` blocks. When the cursor is further behind, the
/// newest blocks win (fresh accounts matter now; the gap is skipped).
pub fn factory_range(last_block: u64, head: u64) -> Option<(u64, u64)> {
    let from = if last_block > 0 {
        last_block + 1
    } else {
        head.saturating_sub(FIRST_RUN_BLOCKS)
    };
    let from = from.max(head.saturating_sub(MAX_FACTORY_SPAN - 1));
    (from <= head).then_some((from, head))
}

pub async fn factory(api: &dyn PolyApi, last_block: u64) -> Option<FactoryScan> {
    let head = match api.block_number().await {
        Ok(h) => h,
        Err(e) => {
            warn!("block number failed, skipping factory logs: {e}");
            return None;
        }
    };
    let Some((from, to)) = factory_range(last_block, head) else {
        return Some(FactoryScan {
            logs: Vec::new(),
            head,
            last_block: last_block.max(head),
        });
    };
    match api.factory_logs(from, to).await {
        Ok(mut logs) => {
            logs.sort_by(|a, b| (a.block, &a.proxy).cmp(&(b.block, &b.proxy)));
            info!(from, to, logs = logs.len(), "factory logs read");
            Some(FactoryScan {
                logs,
                head,
                last_block: to,
            })
        }
        Err(e) => {
            warn!(from, to, "factory logs failed: {e}");
            None
        }
    }
}
