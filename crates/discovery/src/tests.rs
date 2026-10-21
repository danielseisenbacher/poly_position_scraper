use std::collections::{BTreeMap, BTreeSet, HashSet};

use types::api::{FeedTrade, Side};
use types::config::DiscoveryConfig;
use types::state::{Evaluated, EvaluatedSet, MarketCache, Watchlist, WatchlistEntry};
use types::{Addr, Config, MarketCategory, MarketMeta, Score, Signal, SignalKind, Source, Wallet};

use super::*;

fn addr(n: u8) -> Addr {
    Addr::parse(&format!("0x{:040x}", n)).unwrap()
}

fn row(wallet: u8, side: Side, cid: &str, size: f64, price: f64, ts: i64) -> FeedTrade {
    FeedTrade {
        proxy_wallet: addr(wallet),
        side,
        token_id: format!("tok-{cid}"),
        condition_id: cid.to_string(),
        size,
        price,
        timestamp: ts,
        title: format!("Market {cid}"),
        slug: format!("market-{cid}"),
        event_slug: String::new(),
        outcome: "Yes".into(),
        outcome_index: 0,
        name: String::new(),
        pseudonym: String::new(),
        profile_image: String::new(),
        transaction_hash: "0xabc".into(),
    }
}

#[test]
fn feed_aggregation_weights_and_skips() {
    let mut agg = FeedAgg::default();
    // $100 at 0.10 and $300 at 0.50 → weighted mean (10 + 150) / 400 = 0.40.
    assert!(agg.push(&row(1, Side::Buy, "m1", 1000.0, 0.10, 10)));
    assert!(agg.push(&row(1, Side::Buy, "m2", 600.0, 0.50, 11)));
    assert!(agg.push(&row(1, Side::Buy, "m1", 10.0, 0.10, 12)));
    // Sells count as trades but not as buys or markets bought.
    assert!(agg.push(&row(1, Side::Sell, "m3", 100.0, 0.9, 13)));

    // Noise: combo (empty title + synthetic id), up/down series in slug or event slug.
    let mut combo = row(
        2,
        Side::Buy,
        "0x038e536f830e26cc80c6b155230ea0a93c0000000000000000000000000000",
        1e4,
        0.05,
        14,
    );
    combo.title.clear();
    let mut updown = row(3, Side::Buy, "m4", 1e4, 0.05, 15);
    updown.slug = "btc-updown-15m-1792582200".into();
    let mut up_or_down = row(3, Side::Buy, "m5", 1e4, 0.05, 16);
    up_or_down.event_slug = "ethereum-up-or-down-october-21".into();
    for t in [&combo, &updown, &up_or_down] {
        assert!(is_noise(t));
        assert!(!agg.push(t));
    }

    assert_eq!(agg.skipped, 3);
    assert_eq!(agg.rows.len(), 4);
    assert_eq!(agg.wallets.len(), 1);
    let a = &agg.wallets[&addr(1)];
    assert_eq!(a.trades, 4);
    assert!((a.buy_usdc - 401.0).abs() < 1e-9);
    assert!((a.mean_buy_price().unwrap() - 160.1 / 401.0).abs() < 1e-9);
    assert_eq!(a.markets.len(), 2);
    // Compact rows keep what linking needs and drop the rest.
    assert!(agg.rows.iter().all(|r| r.title.is_empty()
        && r.transaction_hash.is_empty()
        && !r.condition_id.is_empty()));
}

#[test]
fn prefilter_branches() {
    let cfg = DiscoveryConfig::default(); // ≥ $200 and (mean ≤ 0.35 or ≥ 3 markets)
    let agg_of = |rows: &[FeedTrade]| {
        let mut a = FeedAgg::default();
        for r in rows {
            a.push(r);
        }
        a.wallets.into_values().next().unwrap()
    };
    // Cheap, big enough.
    assert!(passes_prefilter(
        &agg_of(&[row(1, Side::Buy, "a", 1000.0, 0.3, 1)]),
        &cfg
    ));
    // Cheap but too small.
    assert!(!passes_prefilter(
        &agg_of(&[row(1, Side::Buy, "a", 100.0, 0.3, 1)]),
        &cfg
    ));
    // Expensive, one market.
    assert!(!passes_prefilter(
        &agg_of(&[row(1, Side::Buy, "a", 1000.0, 0.8, 1)]),
        &cfg
    ));
    // Expensive but broad.
    let broad: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|m| row(1, Side::Buy, m, 100.0, 0.8, 1))
        .collect();
    assert!(passes_prefilter(&agg_of(&broad), &cfg));
    // Only sells: no buy volume.
    assert!(!passes_prefilter(
        &agg_of(&[row(1, Side::Sell, "a", 1e5, 0.1, 1)]),
        &cfg
    ));
}

#[test]
fn fnv_and_shards() {
    assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
    assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    let addrs: Vec<Addr> = (1..=50).map(addr).collect();
    for a in &addrs {
        let hits = (0..3).filter(|i| in_shard(a, Some((*i, 3)))).count();
        assert_eq!(hits, 1, "every address is in exactly one shard");
        assert!(in_shard(a, None));
    }
}

fn cand(sources: &[Source], buy: f64) -> Candidate {
    Candidate {
        sources: sources.iter().copied().collect(),
        buy_usdc: buy,
    }
}

#[test]
fn selection_ttl_priority_and_cap() {
    let now = 1_000_000;
    let mut cfg = DiscoveryConfig {
        max_enrich: 4,
        ..Default::default()
    };
    let mut cands = BTreeMap::new();
    cands.insert(addr(1), cand(&[Source::Feed], 500.0));
    cands.insert(addr(2), cand(&[Source::Feed], 900.0));
    cands.insert(addr(3), cand(&[Source::Feed, Source::Leaderboard], 10.0));
    cands.insert(addr(4), cand(&[Source::Watchlist], 0.0));
    cands.insert(addr(5), cand(&[Source::Seed], 0.0));
    cands.insert(addr(6), cand(&[Source::LongshotHolder], 0.0));
    cands.insert(addr(7), cand(&[Source::Feed], 10_000.0)); // evaluated recently → dropped
    let mut ev = EvaluatedSet::default();
    let recent = Evaluated {
        last_eval_ts: now - 3600,
        last_score: 10,
    };
    ev.entries.insert(addr(7), recent);
    ev.entries.insert(addr(4), recent); // watchlist ignores TTL
    ev.entries.insert(addr(6), recent); // longshot holder ignores TTL
    ev.entries.insert(
        addr(1),
        Evaluated {
            last_eval_ts: now - 30 * 86_400,
            last_score: 0,
        },
    ); // stale → eligible

    let order: Vec<Addr> = select(&cands, &ev, &cfg, None, now)
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    assert_eq!(order, vec![addr(5), addr(4), addr(6), addr(3)]);

    cfg.max_enrich = 100;
    let all: Vec<Addr> = select(&cands, &ev, &cfg, None, now)
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    assert_eq!(
        all,
        vec![addr(5), addr(4), addr(6), addr(3), addr(2), addr(1)]
    );

    let shard0: HashSet<Addr> = select(&cands, &ev, &cfg, Some((0, 2)), now)
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    let shard1: HashSet<Addr> = select(&cands, &ev, &cfg, Some((1, 2)), now)
        .into_iter()
        .map(|(a, _)| a)
        .collect();
    assert!(shard0.is_disjoint(&shard1));
    assert_eq!(shard0.len() + shard1.len(), 6);
    assert!(shard0.iter().all(|a| fnv1a(a.as_str()).is_multiple_of(2)));
}

#[test]
fn factory_ranges() {
    // First run: last 1800 blocks.
    assert_eq!(factory_range(0, 10_000), Some((8_200, 10_000)));
    // Normal: continue after the cursor.
    assert_eq!(factory_range(9_000, 10_000), Some((9_001, 10_000)));
    // Far behind: newest 10 000 blocks only.
    assert_eq!(factory_range(100, 50_000), Some((40_001, 50_000)));
    // Nothing new.
    assert_eq!(factory_range(10_000, 10_000), None);
    // Tiny chain.
    assert_eq!(factory_range(0, 100), Some((0, 100)));
}

fn market(
    cid: &str,
    hours: i64,
    volume: f64,
    category: MarketCategory,
    winner: Option<usize>,
) -> MarketMeta {
    MarketMeta {
        condition_id: cid.into(),
        question: format!("Q {cid}"),
        slug: cid.into(),
        event_slug: String::new(),
        start_date: Some(0),
        end_date: Some(hours * 3600),
        closed_time: Some(hours * 3600),
        volume: Some(volume),
        liquidity: None,
        outcomes: vec!["Yes".into(), "No".into()],
        token_ids: vec![format!("{cid}-yes"), format!("{cid}-no")],
        outcome_prices: vec![1.0, 0.0],
        winning_index: winner,
        category,
        neg_risk: false,
    }
}

#[test]
fn resolved_market_pick() {
    let mut cfg = Config {
        discovery: DiscoveryConfig {
            max_resolved_markets: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    let listed = vec![
        market("short", 2, 9e9, MarketCategory::Politics, Some(0)),
        market("crypto3d", 72, 9e8, MarketCategory::Crypto, Some(0)),
        market("unresolved", 100, 8e8, MarketCategory::Politics, None),
        market("small", 100, 9e3, MarketCategory::Politics, Some(1)),
        market("sports", 100, 5e6, MarketCategory::Sports, Some(0)),
        market("big", 100, 1e6, MarketCategory::Other, Some(0)),
        market("mid", 100, 2e4, MarketCategory::Politics, Some(1)),
    ];
    let ids =
        |v: Vec<MarketMeta>| -> Vec<String> { v.into_iter().map(|m| m.condition_id).collect() };
    assert_eq!(
        ids(pick_resolved_markets(listed.clone(), &cfg)),
        vec!["big", "mid"]
    );
    cfg.discovery.longshot_skip_sports = false;
    assert_eq!(
        ids(pick_resolved_markets(listed, &cfg)),
        vec!["sports", "big"]
    );
}

#[test]
fn winning_entries_filter_and_dedupe() {
    use types::api::PositionRow;
    use types::PosStatus;
    let row = |w: u8, token: &str, avg: f64, size: f64, status: PosStatus| PositionRow {
        proxy_wallet: addr(w),
        token_id: token.into(),
        condition_id: "c".into(),
        current_size: 0.0,
        avg_price: avg,
        entry_cost_usdc: 0.0,
        total_cost_usdc: 0.0,
        current_price: if token == "win" { 1.0 } else { 0.0 },
        current_value: 0.0,
        total_size: size,
        realized_pnl: 0.0,
        unrealized_pnl: 0.0,
        total_pnl: 0.0,
        percent_pnl: 0.0,
        status,
        redeemable: false,
        title: String::new(),
        slug: String::new(),
        event_slug: String::new(),
        outcome: String::new(),
        outcome_index: 0,
        end_date: None,
        first_entry_at: None,
        last_event_at: None,
    };
    let cfg = Config::default(); // max entry 0.35, min cost 200 / 4 = 50
    let rows = vec![
        row(3, "win", 0.2, 1_000.0, PosStatus::Redeemable),
        row(3, "win", 0.1, 600.0, PosStatus::Closed), // lower entry wins
        row(3, "win", 0.15, 900.0, PosStatus::Closed),
        row(1, "win", 0.3, 500.0, PosStatus::Closed), // redeemed winner
        row(2, "lose", 0.05, 9e4, PosStatus::Closed), // losing token
        row(4, "win", 0.6, 9e4, PosStatus::Redeemable), // favourite entry
        row(5, "win", 0.05, 900.0, PosStatus::Closed), // $45 < $50
        row(6, "win", 0.0, 9e4, PosStatus::Closed),   // no cost basis (transfer)
    ];
    let got: Vec<(Addr, f64, f64)> = crate::sources::winning_entries(rows, "win", &cfg)
        .into_iter()
        .map(|p| (p.proxy_wallet, p.avg_price, p.total_size))
        .collect();
    assert_eq!(got, vec![(addr(1), 0.3, 500.0), (addr(3), 0.1, 600.0)]);
}

fn scored(n: u8, score: u8, kind: SignalKind) -> Wallet {
    let mut w = Wallet::new(addr(n), 0);
    w.score = Score {
        total: score,
        ..Default::default()
    };
    w.signals = vec![Signal {
        kind,
        strength: 0.9,
        evidence: String::new(),
        refs: vec![],
    }];
    w
}

#[test]
fn watchlist_update_admits_updates_and_caps() {
    let now = 5_000;
    let mut cfg = Config::default();
    cfg.tracker.watchlist_max = 3;
    let entry = |n: u8, score: u8| WatchlistEntry {
        addr: addr(n),
        added_at: 1,
        reason: SignalKind::Concentration,
        score,
        last_activity_ts: 7,
        last_polled: 8,
        last_qualified: 1,
        issue: None,
    };
    let mut wl = Watchlist {
        entries: vec![entry(1, 50), entry(2, 60)],
    };
    let w1 = scored(1, 30, SignalKind::Concentration); // dropped below the bar: score updated, clock not reset
    let w2 = scored(2, 80, SignalKind::OldAccountWinRate);
    let w3 = scored(3, 70, SignalKind::LongshotWinner);
    let w4 = scored(4, 46, SignalKind::NewAccountLongshot);
    let w5 = scored(5, 20, SignalKind::NicheMarket);
    let added = persist::update_watchlist(&mut wl, &[&w1, &w2, &w3, &w4, &w5], &cfg, now);
    let got: Vec<(Addr, u8)> = wl
        .entries
        .iter()
        .map(|e| (e.addr.clone(), e.score))
        .collect();
    assert_eq!(got, vec![(addr(2), 80), (addr(3), 70), (addr(4), 46)]);
    assert_eq!(added, 2);
    assert_eq!(wl.entries[0].last_qualified, now);
    assert_eq!(wl.entries[0].last_activity_ts, 7, "tracker fields are kept");
    let new = &wl.entries[1];
    assert_eq!(
        (
            new.reason,
            new.added_at,
            new.last_activity_ts,
            new.last_qualified
        ),
        (SignalKind::LongshotWinner, now, 0, now)
    );
}

#[test]
fn strongest_signal_uses_weights() {
    let cfg = Config::default().signals;
    let mut w = Wallet::new(addr(1), 0);
    w.signals = vec![
        Signal {
            kind: SignalKind::Concentration,
            strength: 1.0,
            evidence: String::new(),
            refs: vec![],
        }, // 15
        Signal {
            kind: SignalKind::OldAccountWinRate,
            strength: 0.5,
            evidence: String::new(),
            refs: vec![],
        }, // 27.5
    ];
    assert_eq!(
        strongest_signal(&w, &cfg),
        Some(SignalKind::OldAccountWinRate)
    );
    assert_eq!(strongest_signal(&Wallet::new(addr(2), 0), &cfg), None);
}

#[test]
fn market_cache_pruning() {
    let now = 100 * 86_400;
    let mut cache = MarketCache::default();
    for (cid, fetched) in [
        ("kept-ref", 0),
        ("kept-new", now - 86_400),
        ("dropped", now - 31 * 86_400),
        ("no-ts", -1),
    ] {
        cache.markets.insert(
            cid.into(),
            market(cid, 100, 1.0, MarketCategory::Other, None),
        );
        if fetched >= 0 {
            cache.fetched_at.insert(cid.into(), fetched);
        }
    }
    let referenced: HashSet<String> = ["kept-ref".to_string()].into();
    persist::prune_markets(&mut cache, &referenced, now);
    let keys: BTreeSet<&str> = cache.markets.keys().map(String::as_str).collect();
    assert_eq!(keys, ["kept-new", "kept-ref"].into());
    assert!(!cache.fetched_at.contains_key("dropped"));
}

#[test]
fn user_stats_gate() {
    use crate::enrich::passes_gate;
    use types::UserStats;
    let cfg = Config::default();
    let now = 2_000_000_000;
    let st = |age_d: i64, markets: u64, trades: u64, pnl: f64| UserStats {
        join_date: Some(now - age_d * 86_400),
        markets_traded: markets,
        trade_count: trades,
        realized_pnl: pnl,
        ..Default::default()
    };
    let feed: BTreeSet<Source> = [Source::Feed].into();
    let leader: BTreeSet<Source> = [Source::Feed, Source::Leaderboard].into();
    let watched: BTreeSet<Source> = [Source::Watchlist].into();

    assert!(!passes_gate(None, &feed, now, &cfg));
    // New account: always worth a look.
    assert!(passes_gate(Some(&st(10, 2, 5, 0.0)), &feed, now, &cfg));
    // Old account needs a track record (enough markets and realized profit).
    assert!(passes_gate(
        Some(&st(400, 40, 300, 5000.0)),
        &feed,
        now,
        &cfg
    ));
    assert!(!passes_gate(
        Some(&st(400, 40, 300, -2000.0)),
        &feed,
        now,
        &cfg
    ));
    assert!(!passes_gate(
        Some(&st(400, 5, 30, 5000.0)),
        &feed,
        now,
        &cfg
    ));
    // Other sources pass without a track record.
    assert!(passes_gate(Some(&st(400, 5, 30, 0.0)), &leader, now, &cfg));
    // Bots never pass, except when already watched.
    let bot = st(400, 5000, 100_000, 1e6);
    assert!(!passes_gate(Some(&bot), &leader, now, &cfg));
    assert!(passes_gate(Some(&bot), &watched, now, &cfg));
    // No trades at all.
    assert!(!passes_gate(Some(&st(1, 0, 0, 0.0)), &feed, now, &cfg));
}

#[test]
fn stored_wallets_keep_held_positions_and_the_largest_closed_ones() {
    use crate::persist::{for_storage, STORED_CLOSED_POSITIONS};
    use types::{PosStatus, Position};
    let row = |i: usize, status: PosStatus, pnl: f64| -> Position {
        types::api::PositionRow {
            proxy_wallet: addr(1),
            token_id: format!("t{i}"),
            condition_id: format!("c{i}"),
            current_size: 0.0,
            avg_price: 0.5,
            entry_cost_usdc: 0.0,
            total_cost_usdc: 10.0 + i as f64,
            current_price: 0.5,
            current_value: 0.0,
            total_size: 20.0,
            realized_pnl: pnl,
            unrealized_pnl: 0.0,
            total_pnl: pnl,
            percent_pnl: 0.0,
            status,
            redeemable: false,
            title: String::new(),
            slug: String::new(),
            event_slug: String::new(),
            outcome: "Yes".into(),
            outcome_index: 0,
            end_date: None,
            first_entry_at: None,
            last_event_at: None,
        }
        .into()
    };
    let mut w = Wallet::new(addr(1), 0);
    w.positions = (0..5).map(|i| row(i, PosStatus::Open, 0.0)).collect();
    w.positions
        .extend((0..300).map(|i| row(100 + i, PosStatus::Closed, i as f64 - 150.0)));
    let stored = for_storage(&w);
    let open = stored
        .positions
        .iter()
        .filter(|p| p.status == PosStatus::Open)
        .count();
    let closed: Vec<f64> = stored
        .positions
        .iter()
        .filter(|p| p.status == PosStatus::Closed)
        .map(|p| p.realized_pnl)
        .collect();
    assert_eq!(open, 5);
    assert_eq!(closed.len(), STORED_CLOSED_POSITIONS);
    assert!(
        closed.iter().all(|p| p.abs() >= 125.0),
        "largest |pnl| kept: {closed:?}"
    );
    assert_eq!(
        w.positions.len(),
        305,
        "the scored wallet itself is untouched"
    );
}
