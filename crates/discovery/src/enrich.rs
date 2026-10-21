//! Enrichment of selected wallets: user-stats gate → positions → market metadata →
//! provisional score → PreMove price paths → final score → profiles.

use std::collections::{BTreeSet, HashMap, HashSet};

use futures::StreamExt;
use signals::{LeaderboardHit, LongshotHit, PriceRequest, ScoreInput};
use store::DataDir;
use tracing::{debug, warn};
use types::api::{ApiResult, BiggestWinnerRow, PolyApi, PositionRow, PricePoint};
use types::state::{MarketCache, ProxyOwnerMap};
use types::{Addr, Config, PosStatus, Position, Profile, Source, UserStats, Wallet};

/// PreMove price paths are fetched for at most this many wallets per run.
pub const MAX_PREMOVE_WALLETS: usize = 60;
/// `condition_ids` per `markets_by_condition` call issued concurrently (the client batches further).
const MARKET_CHUNK: usize = 50;
/// Unresolved markets in the cache are refreshed after this long.
const MARKET_REFRESH_S: i64 = 86_400;

const STATUSES: [PosStatus; 4] = [
    PosStatus::Open,
    PosStatus::Closed,
    PosStatus::Redeemable,
    PosStatus::RedeemableLost,
];

/// Shared per-run context.
pub struct Ctx<'a> {
    pub api: &'a dyn PolyApi,
    pub data: &'a DataDir,
    pub cfg: &'a Config,
    pub now: i64,
}

impl Ctx<'_> {
    fn conc(&self) -> usize {
        self.cfg.api.max_concurrency.max(1)
    }
}

/// Side inputs for scoring, keyed by wallet.
#[derive(Debug, Default)]
pub struct SideInputs {
    pub leaderboard: HashMap<Addr, Vec<LeaderboardHit>>,
    pub biggest_wins: HashMap<Addr, Vec<BiggestWinnerRow>>,
    pub longshot_hits: HashMap<Addr, Vec<LongshotHit>>,
}

#[derive(Debug)]
pub struct Enriched {
    pub wallet: Wallet,
    /// `wallets/<addr>.json` existed before this run.
    pub had_file: bool,
}

#[derive(Debug)]
pub enum Outcome {
    Enriched(Box<Enriched>),
    /// Dropped by the user-stats gate (unknown wallet or too few trades).
    Gated(Addr),
    /// An API call failed; the wallet is retried next run.
    Failed(Addr),
}

#[derive(Debug, Default)]
pub struct EnrichBatch {
    pub wallets: Vec<Enriched>,
    pub gated: Vec<Addr>,
    pub failed: Vec<Addr>,
}

/// Enrich `list` concurrently. Order of the result follows `list`.
pub async fn enrich_all(ctx: &Ctx<'_>, list: Vec<(Addr, BTreeSet<Source>)>) -> EnrichBatch {
    let mut outcomes: Vec<(usize, Outcome)> = futures::stream::iter(list.into_iter().enumerate())
        .map(|(i, (addr, sources))| async move { (i, enrich_one(ctx, addr, sources).await) })
        .buffer_unordered(ctx.conc())
        .collect()
        .await;
    outcomes.sort_by_key(|(i, _)| *i);
    let mut batch = EnrichBatch::default();
    for (_, o) in outcomes {
        match o {
            Outcome::Enriched(e) => batch.wallets.push(*e),
            Outcome::Gated(a) => batch.gated.push(a),
            Outcome::Failed(a) => batch.failed.push(a),
        }
    }
    batch
}

async fn enrich_one(ctx: &Ctx<'_>, addr: Addr, sources: BTreeSet<Source>) -> Outcome {
    let seed = sources.contains(&Source::Seed);
    let stats: Option<UserStats> = match ctx.api.user_stats(&addr).await {
        Ok(s) => s.map(UserStats::from),
        Err(e) if seed => {
            warn!(%addr, "user stats failed for seed, continuing without: {e}");
            None
        }
        Err(e) => {
            warn!(%addr, "user stats failed: {e}");
            return Outcome::Failed(addr);
        }
    };
    let gated = if seed {
        // Seeds skip the gate, but never get a pass for being a bot.
        stats.as_ref().is_some_and(|s| is_bot(s, ctx.cfg))
    } else {
        !passes_gate(stats.as_ref(), &sources, ctx.now, ctx.cfg)
    };
    if gated {
        return Outcome::Gated(addr);
    }

    let pages = ctx.cfg.discovery.max_position_pages.max(1);
    let fetched =
        futures::future::join_all(STATUSES.map(|s| positions(ctx.api, &addr, s, pages))).await;
    let mut rows = Vec::new();
    for (status, r) in STATUSES.iter().zip(fetched) {
        match r {
            Ok(v) => rows.extend(v),
            Err(e) => {
                // A missing status (e.g. REDEEMABLE_LOST) would distort the win rate: skip the wallet.
                warn!(%addr, ?status, "positions failed: {e}");
                return Outcome::Failed(addr);
            }
        }
    }

    let existing = match ctx.data.load_wallet(&addr) {
        Ok(w) => w,
        Err(e) => {
            warn!(%addr, "existing wallet file unreadable, starting fresh: {e}");
            None
        }
    };
    let had_file = existing.is_some();
    let mut wallet = match existing {
        Some(old) => {
            // Keep tracker-owned and identity fields; everything else is recomputed.
            let mut w = Wallet::new(addr, old.first_seen);
            w.trades = old.trades;
            w.issue = old.issue;
            w.cluster_id = old.cluster_id;
            w.profile = old.profile;
            w
        }
        None => Wallet::new(addr, ctx.now),
    };
    wallet.stats = stats;
    wallet.positions = rows.into_iter().map(Position::from).collect();
    wallet.sources = sources.into_iter().collect();
    wallet.last_scored = ctx.now;
    debug!(addr = %wallet.addr, positions = wallet.positions.len(), "enriched");
    Outcome::Enriched(Box::new(Enriched { wallet, had_file }))
}

/// Market makers and bots: more markets or trades than the gate allows.
pub fn is_bot(s: &UserStats, cfg: &Config) -> bool {
    s.markets_traded > cfg.discovery.gate_max_markets
        || s.trade_count > cfg.discovery.gate_max_trade_count
}

/// Stage-A gate on user-stats, before the (much more expensive) positions calls.
/// Seeds only face the bot check (see `enrich_one`); watchlist members skip it so they
/// are re-scored (a bot scores 0 there and leaves the watchlist).
pub fn passes_gate(
    stats: Option<&UserStats>,
    sources: &BTreeSet<Source>,
    now: i64,
    cfg: &Config,
) -> bool {
    let d = &cfg.discovery;
    let Some(s) = stats else {
        return false;
    };
    if s.trade_count < d.min_trade_count {
        return false;
    }
    let watched = sources.contains(&Source::Watchlist);
    if !watched && is_bot(s, cfg) {
        return false;
    }
    if sources
        .iter()
        .any(|src| !matches!(src, Source::Feed | Source::NewWallet))
    {
        return true;
    }
    let young = s
        .join_date
        .is_some_and(|j| ((now - j) as f64) / 86_400.0 <= cfg.signals.new_account_days);
    let track_record = s.markets_traded as f64 >= cfg.signals.min_resolved
        && s.realized_pnl >= d.gate_min_realized_pnl;
    young || track_record
}

async fn positions(
    api: &dyn PolyApi,
    addr: &Addr,
    status: PosStatus,
    max_pages: usize,
) -> ApiResult<Vec<PositionRow>> {
    let mut out = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..max_pages {
        let page = api.positions(addr, status, cursor.as_deref()).await?;
        let empty = page.data.is_empty();
        out.extend(page.data);
        match page.next_cursor {
            Some(c) if !empty => cursor = Some(c),
            _ => break,
        }
    }
    Ok(out)
}

/// Fetch metadata for condition ids of `wallets` that are missing from the cache or
/// unresolved and older than a day. `attempted` prevents asking twice per run.
pub async fn refresh_markets<'w>(
    ctx: &Ctx<'_>,
    cache: &mut MarketCache,
    wallets: impl Iterator<Item = &'w Wallet>,
    attempted: &mut HashSet<String>,
) {
    let mut want: BTreeSet<String> = BTreeSet::new();
    let per_wallet = ctx.cfg.discovery.max_markets_per_wallet;
    let largest = wallets.flat_map(|w| {
        let mut ps: Vec<&Position> = w.positions.iter().collect();
        ps.sort_by(|a, b| b.cost.total_cmp(&a.cost));
        ps.dedup_by(|a, b| a.condition_id == b.condition_id);
        ps.truncate(per_wallet);
        ps
    });
    for p in largest {
        if attempted.contains(&p.condition_id)
            || types::api::is_synthetic_condition(&p.condition_id)
        {
            continue;
        }
        let stale = match cache.get(&p.condition_id) {
            None => true,
            Some(m) => {
                m.winning_index.is_none()
                    && cache.fetched_at.get(&p.condition_id).copied().unwrap_or(0)
                        < ctx.now - MARKET_REFRESH_S
            }
        };
        if stale {
            want.insert(p.condition_id.clone());
        }
    }
    if want.is_empty() {
        return;
    }
    attempted.extend(want.iter().cloned());
    let ids: Vec<String> = want.into_iter().collect();
    let results: Vec<_> = futures::stream::iter(ids.chunks(MARKET_CHUNK))
        .map(|chunk| async move { (chunk.len(), ctx.api.markets_by_condition(chunk).await) })
        .buffer_unordered(ctx.conc())
        .collect()
        .await;
    let (mut got, mut failed) = (0usize, 0usize);
    for (n, r) in results {
        match r {
            Ok(metas) => {
                for m in metas {
                    got += 1;
                    cache.fetched_at.insert(m.condition_id.clone(), ctx.now);
                    cache.markets.insert(m.condition_id.clone(), m);
                }
            }
            Err(e) => {
                failed += n;
                warn!("market metadata failed for {n} markets: {e}");
            }
        }
    }
    debug!(
        requested = ids.len(),
        got, failed, "market metadata refreshed"
    );
}

/// Copy volume / duration / category from the cache onto the wallet's positions.
/// Without metadata, crypto up/down series are still recognised by slug and marked
/// as 15-minute markets so the signals exclude them.
pub fn fill_market_fields(wallet: &mut Wallet, cache: &MarketCache) {
    for p in &mut wallet.positions {
        if let Some(m) = cache.get(&p.condition_id) {
            p.market_volume = m.volume.or(p.market_volume);
            p.market_duration_h = m.duration_h().or(p.market_duration_h);
            p.category = m.category;
        } else if crate::feed::is_short_crypto(&p.slug) {
            p.market_duration_h = Some(0.25);
            p.category = types::MarketCategory::Crypto;
        }
    }
}

/// Provisional score → PreMove price paths for the best ≤ 60 → final score.
pub async fn score_all(
    ctx: &Ctx<'_>,
    wallets: &mut [Wallet],
    side: &SideInputs,
    cache: &MarketCache,
) {
    let cfg = &ctx.cfg.signals;
    let empty_paths = HashMap::new();
    let evaluate = |w: &Wallet, paths: &HashMap<String, Vec<PricePoint>>| {
        let input = ScoreInput {
            wallet: w,
            markets: cache,
            leaderboard: side.leaderboard.get(&w.addr).map_or(&[][..], Vec::as_slice),
            biggest_wins: side
                .biggest_wins
                .get(&w.addr)
                .map_or(&[][..], Vec::as_slice),
            longshot_hits: side
                .longshot_hits
                .get(&w.addr)
                .map_or(&[][..], Vec::as_slice),
            price_paths: paths,
            now: ctx.now,
        };
        signals::evaluate(&input, cfg)
    };
    let mut evals: Vec<_> = wallets.iter().map(|w| evaluate(w, &empty_paths)).collect();

    let mut premove: Vec<(usize, Vec<PriceRequest>)> = wallets
        .iter()
        .zip(&evals)
        .enumerate()
        .map(|(i, (w, e))| (i, signals::premove_requests(w, e, cfg)))
        .filter(|(_, r)| !r.is_empty())
        .collect();
    premove.sort_by(|a, b| {
        evals[b.0]
            .score
            .total
            .cmp(&evals[a.0].score.total)
            .then_with(|| wallets[a.0].addr.cmp(&wallets[b.0].addr))
    });
    premove.truncate(MAX_PREMOVE_WALLETS);

    let requests: Vec<(usize, PriceRequest)> = premove
        .into_iter()
        .flat_map(|(i, rs)| rs.into_iter().map(move |r| (i, r)))
        .collect();
    let fetched: Vec<(usize, String, Option<Vec<PricePoint>>)> = futures::stream::iter(requests)
        .map(|(i, r)| async move {
            match ctx.api.prices_history(&r.token_id, r.start, r.end).await {
                Ok(p) => (i, r.token_id, Some(p)),
                Err(e) => {
                    warn!(token = %r.token_id, "prices history failed: {e}");
                    (i, r.token_id, None)
                }
            }
        })
        .buffer_unordered(ctx.conc())
        .collect()
        .await;
    let mut paths: HashMap<usize, HashMap<String, Vec<PricePoint>>> = HashMap::new();
    for (i, token, p) in fetched {
        if let Some(p) = p {
            paths.entry(i).or_default().insert(token, p);
        }
    }
    for (i, p) in &paths {
        evals[*i] = evaluate(&wallets[*i], p);
    }

    for (w, e) in wallets.iter_mut().zip(evals) {
        w.signals = e.signals;
        w.score = e.score;
        w.resolved = e.resolved;
        w.value = e.value;
        w.last_scored = ctx.now;
    }
}

/// Owner EOA from factory logs, else from the profile.
pub fn owner_of<'a>(w: &'a Wallet, proxy_owner: &'a ProxyOwnerMap) -> Option<&'a Addr> {
    proxy_owner
        .entries
        .get(&w.addr)
        .map(|(o, _)| o)
        .or_else(|| w.profile.as_ref().and_then(|p| p.owner_eoa.as_ref()))
}

/// Fetch Gamma profiles for wallets scoring ≥ `site.min_score`; set `owner_eoa` from factory logs.
pub async fn fetch_profiles(ctx: &Ctx<'_>, wallets: &mut [Wallet], proxy_owner: &ProxyOwnerMap) {
    let min = ctx.cfg.site.min_score;
    let wanted: Vec<(usize, Addr)> = wallets
        .iter()
        .enumerate()
        .filter(|(_, w)| w.score.total >= min)
        .map(|(i, w)| (i, w.addr.clone()))
        .collect();
    let fetched: Vec<(usize, Option<Profile>)> = futures::stream::iter(wanted)
        .map(|(i, addr)| async move {
            match ctx.api.profile(&addr).await {
                Ok(p) => (i, p),
                Err(e) => {
                    warn!(%addr, "profile failed: {e}");
                    (i, None)
                }
            }
        })
        .buffer_unordered(ctx.conc())
        .collect()
        .await;
    for (i, p) in fetched {
        if let Some(mut p) = p {
            // Gamma may know the owner even when our factory window does not.
            if p.owner_eoa.is_none() {
                p.owner_eoa = wallets[i]
                    .profile
                    .as_ref()
                    .and_then(|old| old.owner_eoa.clone());
            }
            wallets[i].profile = Some(p);
        }
    }
    for w in wallets.iter_mut() {
        if let Some((owner, _)) = proxy_owner.entries.get(&w.addr) {
            w.profile.get_or_insert_with(Profile::default).owner_eoa = Some(owner.clone());
        }
    }
}
