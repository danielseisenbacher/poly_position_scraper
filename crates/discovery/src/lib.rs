//! One discovery run (every 4 h):
//!
//! 1. Sources → candidate set with [`types::Source`] tags:
//!    - global trade feed newest-first back to `cursors.feed_high_water_ts`
//!      (skip combos and markets shorter than `min_market_duration_h`), aggregated per
//!      wallet and pre-filtered (buy volume, mean buy price / #markets);
//!    - PnL leaderboard (day, week) and biggest winners (day, week);
//!    - recently closed markets (≥ `longshot_min_volume`, no crypto / sports) whose
//!      winning token was a longshot → cheap entries on the winner from
//!      `/v2/positions?condition=` incl. redeemed ones ([`signals::LongshotHit`]);
//!    - Deposit Wallet Factory logs since `cursors.factory_last_block` (→ proxy_owner map);
//!    - `--seed` addresses and current watchlist members (re-score).
//! 2. Drop wallets evaluated within `reeval_ttl_s` (except seeds/watchlist), cap `max_enrich`.
//! 3. Enrich: user-stats (gate) → positions ×4 statuses → market metadata (cache) →
//!    provisional score → price paths for PreMove → final score → profile for scorers.
//! 4. Link + cluster, apply cluster bonus.
//! 5. Persist: wallets scoring ≥ `site.min_score`, watchlist admission
//!    (≥ `tracker.watchlist_min_score`, cap `watchlist_max`, keep highest), clusters,
//!    markets cache, cursors, evaluated set. Nothing is written with `dry_run`.
//!
//! Every external call degrades gracefully (logged, the run continues) except the
//! first page of the global feed.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use store::DataDir;
use tracing::{info, warn};
use types::api::{PolyApi, RequestStats};
use types::config::DiscoveryConfig;
use types::site::CandidateRow;
use types::state::{AlertsState, Evaluated, EvaluatedSet, MarketCache, ProxyOwnerMap};
use types::{Addr, Config, Source, Tier, Wallet};

mod enrich;
mod feed;
mod persist;
mod sources;
#[cfg(test)]
mod tests;

pub use feed::{is_noise, passes_prefilter, FeedAgg, WalletAgg};
pub use persist::strongest_signal;
pub use sources::{factory_range, pick_resolved_markets};

use enrich::{Ctx, SideInputs};

/// Sibling proxies (same owner EOA as a flagged wallet) enriched per run.
pub const MAX_SIBLINGS: usize = 50;
/// Rows in [`DiscoveryReport::top`].
pub const TOP_ROWS: usize = 25;
/// The longshot source never looks further back than this.
const LONGSHOT_MAX_LOOKBACK_S: i64 = 2 * 86_400;

#[derive(Debug, Clone, Default)]
pub struct DiscoverOpts {
    pub dry_run: bool,
    /// Extra addresses to evaluate regardless of TTL (e.g. legacy seed list).
    pub seed: Vec<Addr>,
    /// `(i, n)`: only enrich wallets whose address hash mod n == i.
    pub shard: Option<(u32, u32)>,
    pub now: i64,
}

/// Large state kept as release assets (loaded/saved by the caller via `store::bin`).
#[derive(Debug, Clone, Default)]
pub struct StateAssets {
    pub evaluated: EvaluatedSet,
    pub proxy_owner: ProxyOwnerMap,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscoveryReport {
    /// Global-feed pages read.
    pub feed_pages: u32,
    /// Global-feed rows received (before skipping combos / short crypto series).
    pub feed_rows: u64,
    /// The feed stopped (page cap or error) before reaching the previous high-water mark.
    pub feed_truncated: bool,
    /// Data API lag from `/v2/status` (0 if unavailable).
    pub feed_lag_s: i64,
    /// Candidates per source (snake_case `Source` name); a wallet may count under several.
    pub sources: std::collections::BTreeMap<String, u32>,
    /// Feed wallets that passed the pre-filter.
    pub prefiltered: u32,
    /// Wallets with stats + positions fetched and scored this run.
    pub enriched: u32,
    /// Enriched wallets scoring at least `site.min_score` (saved for the dashboard).
    pub scored: u32,
    pub flagged_medium: u32,
    pub flagged_high: u32,
    /// New watchlist entries (after the `watchlist_max` cap).
    pub watchlist_added: u32,
    pub watchlist_size: u32,
    /// Clusters (≥ 2 members) among flagged + watched wallets.
    pub clusters: u32,
    /// Deposit Wallet Factory creations scanned this run.
    pub new_factory_wallets: u32,
    pub duration_s: f64,
    pub requests: RequestStats,
    /// Top 25 by score, for the dry-run report and the job summary.
    pub top: Vec<CandidateRow>,
}

/// One wallet in the candidate funnel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Candidate {
    pub sources: BTreeSet<Source>,
    /// Buy volume in the feed window (feed candidates only), for ordering within the feed tier.
    pub buy_usdc: f64,
}

/// 64-bit FNV-1a, used for `--shard`.
pub fn fnv1a(s: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// `true` when `addr` belongs to shard `(i, n)` (always `true` without sharding).
pub fn in_shard(addr: &Addr, shard: Option<(u32, u32)>) -> bool {
    match shard {
        Some((i, n)) if n > 0 => fnv1a(addr.as_str()) % u64::from(n) == u64::from(i),
        _ => true,
    }
}

/// Lower is enriched first.
fn source_rank(s: Source) -> u8 {
    match s {
        Source::Seed => 0,
        Source::Watchlist => 1,
        Source::LongshotHolder => 2,
        Source::BiggestWinner => 3,
        Source::Leaderboard => 4,
        Source::NewWallet => 5,
        Source::Feed => 6,
    }
}

/// snake_case name, as serialized.
pub fn source_name(s: Source) -> &'static str {
    match s {
        Source::Feed => "feed",
        Source::Leaderboard => "leaderboard",
        Source::BiggestWinner => "biggest_winner",
        Source::LongshotHolder => "longshot_holder",
        Source::NewWallet => "new_wallet",
        Source::Seed => "seed",
        Source::Watchlist => "watchlist",
    }
}

/// Funnel: drop wallets evaluated within `reeval_ttl_s` (unless seed / watchlist /
/// longshot holder), keep the shard, cap at `max_enrich` by source priority
/// (Seed > Watchlist > LongshotHolder > BiggestWinner > Leaderboard > NewWallet > Feed;
/// within a tier by feed buy volume, then address).
pub fn select(
    cands: &BTreeMap<Addr, Candidate>,
    evaluated: &EvaluatedSet,
    cfg: &DiscoveryConfig,
    shard: Option<(u32, u32)>,
    now: i64,
) -> Vec<(Addr, BTreeSet<Source>)> {
    let mut picked: Vec<(u8, &Addr, &Candidate)> = cands
        .iter()
        .filter(|(addr, c)| {
            let sticky = c
                .sources
                .iter()
                .any(|s| matches!(s, Source::Seed | Source::Watchlist | Source::LongshotHolder));
            let fresh = evaluated
                .entries
                .get(*addr)
                .is_some_and(|e| now - e.last_eval_ts < cfg.reeval_ttl_s);
            (sticky || !fresh) && in_shard(addr, shard)
        })
        .map(|(addr, c)| {
            (
                c.sources
                    .iter()
                    .map(|s| source_rank(*s))
                    .min()
                    .unwrap_or(u8::MAX),
                addr,
                c,
            )
        })
        .collect();
    picked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.2.buy_usdc.total_cmp(&a.2.buy_usdc))
            .then_with(|| a.1.cmp(b.1))
    });
    picked.truncate(cfg.max_enrich);
    picked
        .into_iter()
        .map(|(_, a, c)| (a.clone(), c.sources.clone()))
        .collect()
}

/// Market metadata → position fields → score → profiles, for one enriched batch.
async fn score_batch(
    ctx: &Ctx<'_>,
    wallets: &mut [Wallet],
    cache: &mut MarketCache,
    attempted: &mut HashSet<String>,
    side: &SideInputs,
    proxy_owner: &ProxyOwnerMap,
) {
    enrich::refresh_markets(ctx, cache, wallets.iter(), attempted).await;
    for w in wallets.iter_mut() {
        enrich::fill_market_fields(w, cache);
    }
    enrich::score_all(ctx, wallets, side, cache).await;
    enrich::fetch_profiles(ctx, wallets, proxy_owner).await;
}

/// Unflagged-but-related proxies: same owner EOA as a wallet scoring ≥ `tier_medium`.
fn sibling_proxies(
    wallets: &[Wallet],
    proxy_owner: &ProxyOwnerMap,
    cfg: &Config,
    skip: &HashSet<Addr>,
) -> Vec<Addr> {
    let owners: HashSet<&Addr> = wallets
        .iter()
        .filter(|w| w.score.total >= cfg.signals.tier_medium)
        .filter_map(|w| enrich::owner_of(w, proxy_owner))
        .collect();
    if owners.is_empty() {
        return Vec::new();
    }
    // One pass over the map (equivalent to `linking::proxies_of` per owner, which scans it each time).
    let mut out: Vec<Addr> = proxy_owner
        .entries
        .iter()
        .filter(|(p, (o, _))| owners.contains(o) && !skip.contains(*p))
        .map(|(p, _)| p.clone())
        .collect();
    out.sort();
    out.truncate(MAX_SIBLINGS);
    out
}

pub async fn run(
    api: &dyn PolyApi,
    data: &DataDir,
    assets: &mut StateAssets,
    cfg: &Config,
    opts: &DiscoverOpts,
) -> anyhow::Result<DiscoveryReport> {
    let started = Instant::now();
    let now = opts.now;
    let d = &cfg.discovery;
    let mut cursors = data.cursors()?;
    let mut watchlist = data.watchlist()?;
    let mut cache = data.markets_cache()?;
    let alerts = data.alerts().unwrap_or_else(|e| {
        warn!("alerts.json unreadable: {e}");
        AlertsState::default()
    });
    let mut report = DiscoveryReport::default();

    // ---- 1. Sources (concurrently) ----
    let stop_ts = if cursors.feed_high_water_ts > 0 {
        cursors.feed_high_water_ts
    } else {
        now - d.feed_initial_lookback_s
    };
    let longshot_since = cursors
        .resolved_markets_since
        .max(now - LONGSHOT_MAX_LOOKBACK_S);
    let factory_from = cursors.factory_last_block;
    let (status, feed, boards, longshots, factory) = futures::join!(
        api.status(),
        feed::ingest(api, d, stop_ts),
        sources::boards(api, cfg),
        sources::longshots(api, cfg, longshot_since, now),
        async {
            if d.scan_factory {
                sources::factory(api, factory_from).await
            } else {
                None
            }
        },
    );
    match status {
        Ok(s) => report.feed_lag_s = s.lag_seconds,
        Err(e) => warn!("status failed: {e}"),
    }
    let feed = feed?;
    report.feed_pages = feed.pages;
    report.feed_rows = feed.rows;
    report.feed_truncated = feed.truncated;
    cursors.feed_high_water_ts = cursors.feed_high_water_ts.max(feed.high_water);

    let mut cands: BTreeMap<Addr, Candidate> = BTreeMap::new();
    let mut tag = |addr: &Addr, s: Source| {
        cands.entry(addr.clone()).or_default().sources.insert(s);
    };
    for (addr, agg) in &feed.agg.wallets {
        if passes_prefilter(agg, d) {
            tag(addr, Source::Feed);
            report.prefiltered += 1;
        }
    }
    for addr in boards.hits.keys() {
        tag(addr, Source::Leaderboard);
    }
    for addr in boards.wins.keys() {
        tag(addr, Source::BiggestWinner);
    }
    for addr in longshots.hits.keys() {
        tag(addr, Source::LongshotHolder);
    }
    for addr in &opts.seed {
        tag(addr, Source::Seed);
    }
    for e in &watchlist.entries {
        tag(&e.addr, Source::Watchlist);
    }
    for (addr, c) in cands.iter_mut() {
        if let Some(agg) = feed.agg.wallets.get(addr) {
            c.buy_usdc = agg.buy_usdc;
        }
    }

    if longshots.listed {
        cursors.resolved_markets_since = now;
    }
    for m in &longshots.markets {
        cache.fetched_at.insert(m.condition_id.clone(), now);
        cache.markets.insert(m.condition_id.clone(), m.clone());
    }

    let mut factory_logs = Vec::new();
    if let Some(scan) = factory {
        let retention_blocks = (d.proxy_owner_retention_s / 2).max(0) as u64;
        linking::update_proxy_owner(
            &mut assets.proxy_owner,
            &scan.logs,
            scan.head,
            retention_blocks,
        );
        for log in &scan.logs {
            if let Some(c) = cands.get_mut(&log.proxy) {
                c.sources.insert(Source::NewWallet);
            }
        }
        cursors.factory_last_block = scan.last_block;
        report.new_factory_wallets = scan.logs.len() as u32;
        factory_logs = scan.logs;
    }

    // Keep the compact feed rows only for candidates (co-trade linking needs nothing else).
    let mut feed_rows = feed.agg.rows;
    feed_rows.retain(|t| cands.contains_key(&t.proxy_wallet));
    drop(feed.agg.wallets);

    let mut stored_hits: BTreeMap<Addr, Vec<signals::LongshotHit>> =
        match data.read(persist::LONGSHOT_HITS) {
            Ok(h) => h.unwrap_or_default(),
            Err(e) => {
                warn!("longshot hits unreadable, starting empty: {e}");
                BTreeMap::new()
            }
        };
    persist::merge_longshot_hits(&mut stored_hits, &longshots.hits, now);
    let side = SideInputs {
        leaderboard: boards.hits,
        biggest_wins: boards.wins,
        longshot_hits: stored_hits
            .iter()
            .map(|(a, h)| (a.clone(), h.clone()))
            .collect(),
    };

    // ---- 2. Funnel ----
    let selected = select(&cands, &assets.evaluated, d, opts.shard, now);
    info!(
        candidates = cands.len(),
        selected = selected.len(),
        "funnel"
    );

    // ---- 3. Enrich + score ----
    let ctx = Ctx {
        api,
        data,
        cfg,
        now,
    };
    let mut attempted = HashSet::new();
    let batch = enrich::enrich_all(&ctx, selected).await;
    let mut had_file: HashSet<Addr> = HashSet::new();
    let mut tried: HashSet<Addr> = batch.gated.iter().chain(&batch.failed).cloned().collect();
    let mut gated = batch.gated;
    let mut wallets: Vec<Wallet> = Vec::with_capacity(batch.wallets.len());
    for e in batch.wallets {
        if e.had_file {
            had_file.insert(e.wallet.addr.clone());
        }
        wallets.push(e.wallet);
    }
    score_batch(
        &ctx,
        &mut wallets,
        &mut cache,
        &mut attempted,
        &side,
        &assets.proxy_owner,
    )
    .await;

    // Sibling proxies of flagged owners, so multi-account owners surface.
    tried.extend(wallets.iter().map(|w| w.addr.clone()));
    let siblings = sibling_proxies(&wallets, &assets.proxy_owner, cfg, &tried);
    if !siblings.is_empty() {
        let list: Vec<(Addr, BTreeSet<Source>)> = siblings
            .into_iter()
            .map(|a| {
                let c = cands.entry(a.clone()).or_default();
                c.sources.insert(Source::NewWallet);
                (a, c.sources.clone())
            })
            .collect();
        info!(siblings = list.len(), "enriching sibling proxies");
        let batch = enrich::enrich_all(&ctx, list).await;
        gated.extend(batch.gated);
        let mut more: Vec<Wallet> = Vec::with_capacity(batch.wallets.len());
        for e in batch.wallets {
            if e.had_file {
                had_file.insert(e.wallet.addr.clone());
            }
            more.push(e.wallet);
        }
        score_batch(
            &ctx,
            &mut more,
            &mut cache,
            &mut attempted,
            &side,
            &assets.proxy_owner,
        )
        .await;
        wallets.extend(more);
    }
    for c in cands.values() {
        for s in &c.sources {
            *report
                .sources
                .entry(source_name(*s).to_string())
                .or_default() += 1;
        }
    }

    // ---- 4. Link + cluster ----
    let min_score = cfg.site.min_score;
    let watched: HashSet<Addr> = watchlist.entries.iter().map(|e| e.addr.clone()).collect();
    let enriched_addrs: HashSet<Addr> = wallets.iter().map(|w| w.addr.clone()).collect();
    let (mut set, mut rest): (Vec<Wallet>, Vec<Wallet>) = wallets
        .into_iter()
        .partition(|w| w.score.total >= min_score || watched.contains(&w.addr));
    // Watched wallets not re-enriched this run (shard, cap, failures): (cluster_id, score) as loaded.
    let mut loaded: HashMap<Addr, (Option<String>, u8)> = HashMap::new();
    for e in &watchlist.entries {
        if enriched_addrs.contains(&e.addr) {
            continue;
        }
        match data.load_wallet(&e.addr) {
            Ok(Some(w)) => {
                loaded.insert(w.addr.clone(), (w.cluster_id.clone(), w.score.total));
                set.push(w);
            }
            Ok(None) => {}
            Err(err) => warn!(addr = %e.addr, "watched wallet file unreadable: {err}"),
        }
    }
    let set_addrs: HashSet<&Addr> = set.iter().map(|w| &w.addr).collect();
    feed_rows.retain(|t| set_addrs.contains(&t.proxy_wallet));
    let links = linking::find_links(
        &linking::LinkInput {
            wallets: &set,
            proxy_owner: &assets.proxy_owner,
            factory_logs: &factory_logs,
            feed: &feed_rows,
        },
        &cfg.linking,
    );
    let clusters = linking::cluster(&links, &cfg.linking);
    signals::apply_cluster_bonus(&mut set, &clusters, &cfg.signals);
    signals::apply_cluster_bonus(&mut rest, &[], &cfg.signals);
    report.clusters = clusters.len() as u32;

    for w in set
        .iter()
        .chain(&rest)
        .filter(|w| !loaded.contains_key(&w.addr))
    {
        assets.evaluated.entries.insert(
            w.addr.clone(),
            Evaluated {
                last_eval_ts: now,
                last_score: w.score.total,
            },
        );
    }
    for a in &gated {
        assets.evaluated.entries.insert(
            a.clone(),
            Evaluated {
                last_eval_ts: now,
                last_score: 0,
            },
        );
    }

    // ---- 5. Persist ----
    let all: Vec<&Wallet> = set.iter().chain(&rest).collect();
    report.watchlist_added = persist::update_watchlist(&mut watchlist, &all, cfg, now);
    report.watchlist_size = watchlist.entries.len() as u32;
    cursors.last_discover = now;
    if !opts.dry_run {
        for w in &all {
            let save = match loaded.get(&w.addr) {
                Some((cluster_id, score)) => *cluster_id != w.cluster_id || *score != w.score.total,
                None => w.score.total >= min_score || had_file.contains(&w.addr),
            };
            if save {
                data.save_wallet(&persist::for_storage(w))?;
            }
        }
        data.save_watchlist(&watchlist)?;
        match data.load_all_wallets() {
            Ok(saved) => {
                let referenced: HashSet<String> = saved
                    .iter()
                    .flat_map(|w| &w.positions)
                    .map(|p| p.condition_id.clone())
                    .collect();
                persist::prune_markets(&mut cache, &referenced, now);
            }
            Err(e) => warn!("not pruning the markets cache, wallet files unreadable: {e}"),
        }
        data.save_markets_cache(&cache)?;
        data.save_clusters(&clusters)?;
        data.write(persist::LONGSHOT_HITS, &stored_hits)?;
        data.save_cursors(&cursors)?;
    }

    // ---- 6. Report ----
    let fresh = || all.iter().filter(|w| !loaded.contains_key(&w.addr));
    report.enriched = fresh().count() as u32;
    report.scored = fresh().filter(|w| w.score.total >= min_score).count() as u32;
    report.flagged_medium = fresh().filter(|w| w.score.tier == Tier::Medium).count() as u32;
    report.flagged_high = fresh().filter(|w| w.score.tier == Tier::High).count() as u32;
    let final_watched: HashSet<Addr> = watchlist.entries.iter().map(|e| e.addr.clone()).collect();
    let mut top: Vec<&Wallet> = all.iter().copied().filter(|w| w.score.total > 0).collect();
    top.sort_by(|a, b| {
        b.score
            .total
            .cmp(&a.score.total)
            .then_with(|| a.addr.cmp(&b.addr))
    });
    report.top = top
        .into_iter()
        .take(TOP_ROWS)
        .map(|w| persist::candidate_row(w, now, &final_watched, &alerts))
        .collect();
    report.requests = api.stats();
    report.duration_s = started.elapsed().as_secs_f64();
    info!(
        enriched = report.enriched,
        scored = report.scored,
        medium = report.flagged_medium,
        high = report.flagged_high,
        watchlist_added = report.watchlist_added,
        clusters = report.clusters,
        dry_run = opts.dry_run,
        "discovery done"
    );
    Ok(report)
}
