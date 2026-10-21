use std::collections::BTreeMap;

use types::site::{CandidateRow, MarketView, Meta, TapeRow};
use types::state::{AlertRecord, WatchlistEntry};
use types::*;

use super::*;

const NOW: i64 = 1_792_540_800; // 2026-10-21T00:00:00Z
const DAY: i64 = 86_400;

fn addr(n: u8) -> Addr {
    Addr::parse(&format!("0x{n:040x}")).unwrap()
}

fn tmp() -> (tempfile::TempDir, DataDir) {
    let t = tempfile::tempdir().unwrap();
    let d = DataDir::open(t.path().join("data")).unwrap();
    (t, d)
}

fn pos(cond: &str, outcome: &str, status: PosStatus, size: f64, avg: f64, cur: f64) -> Position {
    Position {
        condition_id: cond.into(),
        token_id: format!("{cond}-{outcome}"),
        title: format!("Market {cond}"),
        slug: format!("market-{cond}"),
        outcome: outcome.into(),
        avg_price: avg,
        total_size: size,
        current_size: size,
        cost: size * avg,
        current_value: size * cur,
        current_price: cur,
        realized_pnl: 0.0,
        total_pnl: size * (cur - avg),
        percent_pnl: 0.0,
        status,
        first_entry_at: Some(NOW - 10 * DAY),
        last_event_at: Some(NOW - 5 * DAY),
        end_date: Some(NOW + 30 * DAY),
        market_volume: None,
        market_duration_h: None,
        category: MarketCategory::Other,
    }
}

fn trade(key: &str, ts: i64, usdc: f64) -> TrackedTrade {
    TrackedTrade {
        key: key.into(),
        ts,
        kind: TradeKind::Buy,
        condition_id: "m1".into(),
        token_id: "m1-Yes".into(),
        title: "Market m1".into(),
        outcome: "Yes".into(),
        price: 0.2,
        size: usdc / 0.2,
        usdc,
        tx: format!("0xtx{key}"),
    }
}

fn signal(kind: SignalKind, strength: f64) -> Signal {
    Signal {
        kind,
        strength,
        evidence: format!("{kind} evidence"),
        refs: vec![],
    }
}

fn wallet(n: u8, score: u8, tier: Tier) -> Wallet {
    let mut w = Wallet::new(addr(n), NOW - 20 * DAY);
    w.score = Score {
        total: score,
        tier,
        materiality: 1.0,
    };
    w.last_scored = NOW - DAY;
    w
}

/// Five wallets covering the render rules; returns the data dir.
fn fixture() -> (tempfile::TempDir, DataDir) {
    let (t, d) = tmp();

    let mut a = wallet(1, 82, Tier::High);
    a.profile = Some(Profile {
        name: "alpha".into(),
        pseudonym: "Brave-Fox".into(),
        ..Default::default()
    });
    a.stats = Some(UserStats {
        join_date: Some(NOW - 40 * DAY),
        realized_pnl: 1234.5,
        ..Default::default()
    });
    a.signals = vec![
        signal(SignalKind::OldAccountWinRate, 0.6),
        signal(SignalKind::LongshotWinner, 1.0),
    ];
    a.resolved = ResolvedSummary {
        n: 12.0,
        wins: 9.0,
        expected_wins: 3.0,
        z: 3.5,
        longshot_wins: 4,
    };
    a.positions = vec![
        pos("m1", "Yes", PosStatus::Open, 1000.0, 0.2, 0.5),
        pos("m2", "Yes", PosStatus::Redeemable, 500.0, 0.1, 1.0),
    ];
    a.value = 500.0;
    a.cluster_id = Some("c-0000000000".into());
    a.issue = Some(12);
    a.trades = vec![
        trade("a1", NOW - 3600, 600.0),
        trade("a2", NOW - 49 * 3600, 50.0),
    ];

    let mut b = wallet(2, 55, Tier::Medium);
    b.profile = Some(Profile {
        pseudonym: "Quiet-Owl".into(),
        created_at: Some(NOW - 5 * DAY),
        ..Default::default()
    });
    b.signals = vec![signal(SignalKind::NewAccountLongshot, 0.8)];
    b.positions = vec![
        pos("m1", "No", PosStatus::Open, 300.0, 0.7, 0.5),
        pos("m2", "No", PosStatus::RedeemableLost, 200.0, 0.8, 0.0),
        pos("m3", "Yes", PosStatus::Closed, 50.0, 0.3, 1.0),
    ];
    b.trades = vec![trade("b1", NOW - 47 * 3600, 20.0)];

    let mut c = wallet(3, 40, Tier::Low);
    c.signals = vec![
        signal(SignalKind::Concentration, 0.5),
        signal(SignalKind::NicheMarket, 0.5),
    ];
    c.positions = vec![pos("m3", "Yes", PosStatus::Open, 80.0, 0.3, 0.4)];
    c.positions[0].last_event_at = Some(NOW - 2 * DAY);

    let mut low = wallet(4, 10, Tier::Low);
    low.positions = vec![pos("m1", "Yes", PosStatus::Open, 10.0, 0.2, 0.5)];
    low.trades = vec![trade("d1", NOW - 60, 5.0)];

    let mut e = wallet(5, 50, Tier::Medium);
    e.positions = vec![
        pos("m4", "Yes", PosStatus::Open, 70.0, 0.1, 0.2),
        pos("m2", "Yes", PosStatus::Closed, 1.0, 0.1, 1.0),
    ];

    for w in [&a, &b, &c, &low, &e] {
        d.save_wallet(w).unwrap();
    }

    let entry = |w: &Wallet| WatchlistEntry {
        addr: w.addr.clone(),
        added_at: NOW - 10 * DAY,
        reason: SignalKind::LongshotWinner,
        score: w.score.total,
        last_activity_ts: NOW - 3600,
        last_polled: NOW - 600,
        last_qualified: NOW,
        issue: w.issue,
    };
    d.save_watchlist(&Watchlist {
        entries: vec![entry(&a), entry(&b)],
    })
    .unwrap();
    let mut alerts = AlertsState::default();
    alerts.wallets.insert(
        b.addr.clone(),
        AlertRecord {
            issue: 34,
            opened_at: NOW - DAY,
            last_score: 55,
            signal_kinds: vec![SignalKind::NewAccountLongshot],
            last_comment_ts: 0,
            closed: false,
        },
    );
    d.save_alerts(&alerts).unwrap();
    d.save_cursors(&Cursors {
        last_discover: NOW - 7200,
        last_track: NOW - 300,
        ..Default::default()
    })
    .unwrap();
    d.save_clusters(&[Cluster {
        id: "c-0000000000".into(),
        members: vec![a.addr.clone()],
        links: vec![],
    }])
    .unwrap();

    let mut cache = MarketCache::default();
    let meta = |cond: &str, winner: Option<usize>| MarketMeta {
        condition_id: cond.into(),
        question: format!("Cached question {cond}?"),
        slug: format!("cached-{cond}"),
        event_slug: String::new(),
        start_date: Some(NOW - 60 * DAY),
        end_date: Some(NOW - DAY),
        closed_time: None,
        volume: Some(42_000.0),
        liquidity: None,
        outcomes: vec!["Yes".into(), "No".into()],
        token_ids: vec![format!("{cond}-Yes"), format!("{cond}-No")],
        outcome_prices: vec![1.0, 0.0],
        winning_index: winner,
        category: MarketCategory::Politics,
        neg_risk: false,
    };
    cache.markets.insert("m2".into(), meta("m2", Some(0)));
    d.save_markets_cache(&cache).unwrap();
    (t, d)
}

fn meta_input() -> MetaInput {
    let mut api = types::api::RequestStats::default();
    api.requests.insert(types::api::RateClass::Positions, 321);
    api.rate_limited = 2;
    MetaInput {
        now: NOW,
        run_id: Some("123456".into()),
        api,
        feed_lag_s: 4,
        feed_truncated: false,
        wallets_scanned: 77,
        trades_tracked: 3,
    }
}

fn site<T: serde::de::DeserializeOwned>(d: &DataDir, rel: &str) -> T {
    d.read(rel).unwrap().unwrap()
}

#[test]
fn read_write_roundtrip_and_atomicity() {
    let (_t, d) = tmp();
    assert!(d.root().join("wallets").is_dir());
    assert_eq!(d.read::<Cursors>(paths::CURSORS).unwrap(), None);
    assert_eq!(d.cursors().unwrap(), Cursors::default());
    assert_eq!(d.watchlist().unwrap(), Watchlist::default());
    assert!(d.clusters().unwrap().is_empty());

    let c = Cursors {
        feed_high_water_ts: 5,
        factory_last_block: 6,
        ..Default::default()
    };
    d.save_cursors(&c).unwrap();
    assert_eq!(d.cursors().unwrap(), c);
    let text = std::fs::read_to_string(d.root().join(paths::CURSORS)).unwrap();
    assert!(
        text.starts_with(r#"{"feed_high_water_ts":5,"#),
        "compact JSON: {text}"
    );

    d.write("nested/deeper/x.json", &vec![1, 2, 3]).unwrap();
    assert_eq!(
        d.read::<Vec<u8>>("nested/deeper/x.json").unwrap(),
        Some(vec![1, 2, 3])
    );
    let leftovers: Vec<_> = std::fs::read_dir(d.root().join("nested/deeper"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .filter(|n| n.to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty());

    d.remove("nested/deeper/x.json").unwrap();
    d.remove("nested/deeper/x.json").unwrap();
    assert_eq!(d.read::<Vec<u8>>("nested/deeper/x.json").unwrap(), None);

    std::fs::write(d.root().join(paths::ALERTS), b"{broken").unwrap();
    assert!(matches!(d.alerts(), Err(StoreError::Json { .. })));
}

#[test]
fn wallet_files() {
    let (_t, d) = tmp();
    let w2 = wallet(2, 50, Tier::Medium);
    let w1 = wallet(1, 70, Tier::High);
    d.save_wallet(&w2).unwrap();
    d.save_wallet(&w1).unwrap();
    let wdir = d.root().join(paths::WALLETS_DIR);
    for junk in [
        "README.json",
        "notes.txt",
        ".0x0000000000000000000000000000000000000003.json.tmp",
    ] {
        std::fs::write(wdir.join(junk), b"{}").unwrap();
    }
    std::fs::write(
        wdir.join("0x00000000000000000000000000000000000000AB.json"),
        b"{}",
    )
    .unwrap();

    assert_eq!(d.wallet_addrs().unwrap(), vec![addr(1), addr(2)]);
    assert_eq!(d.load_wallet(&addr(1)).unwrap(), Some(w1.clone()));
    assert_eq!(d.load_wallet(&addr(9)).unwrap(), None);
    assert_eq!(d.load_all_wallets().unwrap(), vec![w1, w2]);
    d.remove_wallet(&addr(1)).unwrap();
    assert_eq!(d.wallet_addrs().unwrap(), vec![addr(2)]);
}

#[test]
fn render_site_snapshots() {
    let (_t, d) = fixture();
    let cfg = Config::default();
    let summary = render_site(&d, &cfg, &meta_input()).unwrap();
    let index: Vec<CandidateRow> = site(&d, paths::INDEX);
    let markets: Vec<MarketView> = site(&d, paths::MARKETS);
    let tape: Vec<TapeRow> = site(&d, paths::ACTIVITY_RECENT);
    let meta: Meta = site(&d, paths::META);

    let on_disk: u64 = [
        paths::INDEX,
        paths::MARKETS,
        paths::ACTIVITY_RECENT,
        paths::META,
    ]
    .iter()
    .map(|p| std::fs::metadata(d.root().join(p)).unwrap().len())
    .sum();
    assert_eq!(
        summary,
        SiteSummary {
            candidates: 4,
            markets: 2,
            tape_rows: 3,
            bytes_written: on_disk
        }
    );

    insta::assert_json_snapshot!("index", index);
    insta::assert_json_snapshot!("markets", markets);
    insta::assert_json_snapshot!("tape", tape);
    insta::assert_json_snapshot!("meta", meta);

    let bytes: Vec<_> = [paths::INDEX, paths::MARKETS, paths::META]
        .iter()
        .map(|p| std::fs::read(d.root().join(p)).unwrap())
        .collect();
    assert_eq!(render_site(&d, &cfg, &meta_input()).unwrap(), summary);
    for (p, b) in [paths::INDEX, paths::MARKETS, paths::META]
        .iter()
        .zip(bytes)
    {
        assert_eq!(
            std::fs::read(d.root().join(p)).unwrap(),
            b,
            "{p} not idempotent"
        );
    }
}

#[test]
fn render_site_rules() {
    let (_t, d) = fixture();
    render_site(&d, &Config::default(), &meta_input()).unwrap();
    let index: Vec<CandidateRow> = site(&d, paths::INDEX);
    let rows: BTreeMap<_, _> = index.iter().map(|r| (r.addr.clone(), r)).collect();

    let order: Vec<u8> = index.iter().map(|r| r.score).collect();
    assert_eq!(
        order,
        vec![82, 55, 50, 40],
        "score ≥ min_score, sorted desc"
    );
    let a = rows[&addr(1)];
    assert_eq!(a.name, "alpha");
    assert_eq!(
        a.signals,
        vec![
            (SignalKind::LongshotWinner, 1.0),
            (SignalKind::OldAccountWinRate, 0.6)
        ]
    );
    assert_eq!(a.pnl, 1234.5);
    assert_eq!(a.age_d, Some(40.0));
    assert_eq!(a.win_rate, Some(0.75));
    assert_eq!(a.last_trade_ts, Some(NOW - 3600));
    assert!(a.watched);
    let b = rows[&addr(2)];
    assert_eq!(b.name, "Quiet-Owl");
    assert_eq!(b.issue, Some(34), "issue from alerts.json");
    assert_eq!(b.n_markets, 3);
    assert_eq!(b.age_d, Some(5.0));
    assert!(
        (b.pnl - (-60.0 - 160.0 + 35.0)).abs() < 1e-9,
        "Σ total_pnl without stats"
    );
    let c = rows[&addr(3)];
    assert_eq!(c.name, addr(3).short());
    assert_eq!(
        c.last_trade_ts,
        Some(NOW - 2 * DAY),
        "falls back to last_event_at"
    );
    assert!(!c.watched);

    let markets: Vec<MarketView> = site(&d, paths::MARKETS);
    let ids: Vec<&str> = markets.iter().map(|m| m.condition_id.as_str()).collect();
    // m3: only one medium+ wallet holds it Open (b's is Closed); m4: single wallet.
    assert_eq!(
        ids,
        vec!["m1", "m2"],
        "≥2 distinct wallets, tie broken by Σ size"
    );
    assert_eq!(markets[1].resolved_outcome.as_deref(), Some("Yes"));
    assert_eq!(markets[1].title, "Cached question m2?");
    assert_eq!(markets[0].title, "Market m1");
    assert!(
        markets[0].wallets.iter().all(|w| w.addr != addr(4)),
        "low-score wallets excluded"
    );

    let tape: Vec<TapeRow> = site(&d, paths::ACTIVITY_RECENT);
    let keys: Vec<_> = tape.iter().map(|t| t.tx.as_str()).collect();
    assert_eq!(
        keys,
        vec!["0xtxd1", "0xtxa1", "0xtxb1"],
        "newest first, 48 h window"
    );

    let meta: Meta = site(&d, paths::META);
    assert_eq!(meta.score_histogram, [0, 1, 0, 0, 1, 2, 0, 0, 1, 0]);
    assert_eq!(
        (meta.counts.high, meta.counts.medium, meta.counts.watchlist),
        (1, 2, 2)
    );
    assert_eq!(meta.signal_counts.get(&SignalKind::NicheMarket), Some(&1));
}

#[test]
fn render_site_caps() {
    let (_t, d) = fixture();
    let mut cfg = Config::default();
    cfg.site.max_candidates = 2;
    cfg.site.max_markets = 1;
    cfg.tracker.tape_max_rows = 2;
    let s = render_site(&d, &cfg, &meta_input()).unwrap();
    assert_eq!((s.candidates, s.markets, s.tape_rows), (2, 1, 2));
    let index: Vec<CandidateRow> = site(&d, paths::INDEX);
    assert_eq!(
        index.iter().map(|r| r.addr.clone()).collect::<Vec<_>>(),
        vec![addr(1), addr(2)]
    );
    let markets: Vec<MarketView> = site(&d, paths::MARKETS);
    assert_eq!(markets[0].condition_id, "m1");
    let tape: Vec<TapeRow> = site(&d, paths::ACTIVITY_RECENT);
    assert_eq!(
        tape.iter().map(|t| t.ts).collect::<Vec<_>>(),
        vec![NOW - 60, NOW - 3600]
    );
    let meta: Meta = site(&d, paths::META);
    assert_eq!(meta.counts.candidates, 2);
    assert_eq!(
        meta.score_histogram.iter().sum::<u32>(),
        5,
        "histogram covers all wallets"
    );
}

#[test]
fn render_site_empty_dir() {
    let (_t, d) = tmp();
    let s = render_site(
        &d,
        &Config::default(),
        &MetaInput {
            now: NOW,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!((s.candidates, s.markets, s.tape_rows), (0, 0, 0));
    assert_eq!(site::<Vec<CandidateRow>>(&d, paths::INDEX), vec![]);
    assert_eq!(site::<Meta>(&d, paths::META).generated_at, NOW);
}

#[test]
fn compact_rules() {
    let (_t, d) = tmp();
    let cfg = Config::default();
    let old = NOW - 31 * DAY;
    let mk = |n: u8, score: u8, last_scored: i64| {
        let mut w = wallet(n, score, Tier::Low);
        w.last_scored = last_scored;
        d.save_wallet(&w).unwrap();
    };
    mk(1, 10, old); // watched → kept
    mk(2, 80, old); // open alert → kept
    mk(3, 80, old); // closed alert, stale → removed
    mk(4, 80, NOW - DAY); // fresh, above min → kept
    mk(5, 10, NOW - DAY); // fresh but below min → removed
    mk(6, 80, old); // stale → removed

    let entry = WatchlistEntry {
        addr: addr(1),
        added_at: old,
        reason: SignalKind::PreMove,
        score: 10,
        last_activity_ts: old,
        last_polled: old,
        last_qualified: old,
        issue: None,
    };
    d.save_watchlist(&Watchlist {
        entries: vec![entry],
    })
    .unwrap();
    let record = |closed| AlertRecord {
        issue: 1,
        opened_at: old,
        last_score: 80,
        signal_kinds: vec![],
        last_comment_ts: 0,
        closed,
    };
    let mut alerts = AlertsState::default();
    alerts.wallets.insert(addr(2), record(false));
    alerts.wallets.insert(addr(3), record(true));
    d.save_alerts(&alerts).unwrap();

    let rep = compact(&d, &cfg, NOW).unwrap();
    assert_eq!(
        rep,
        CompactReport {
            wallets_removed: 3,
            wallets_kept: 3
        }
    );
    assert_eq!(d.wallet_addrs().unwrap(), vec![addr(1), addr(2), addr(4)]);
    assert_eq!(
        compact(&d, &cfg, NOW).unwrap(),
        CompactReport {
            wallets_removed: 0,
            wallets_kept: 3
        }
    );
}

#[test]
fn markets_cache_lives_gzipped_in_the_state_dir() {
    let t = tempfile::tempdir().unwrap();
    let d = DataDir::open(t.path().join("data"))
        .unwrap()
        .with_state_dir(t.path().join("state"));
    std::fs::create_dir_all(t.path().join("state")).unwrap();
    assert_eq!(d.markets_cache().unwrap(), MarketCache::default());
    let mut cache = MarketCache::default();
    cache.fetched_at.insert("m1".into(), 7);
    d.save_markets_cache(&cache).unwrap();
    assert_eq!(d.markets_cache().unwrap(), cache);
    assert!(t
        .path()
        .join("state")
        .join(paths::MARKETS_CACHE_GZ)
        .exists());
    assert!(!t.path().join("data").join(paths::MARKETS_CACHE).exists());
}
