use std::collections::HashMap;

use types::api::{BiggestWinnerRow, PricePoint, TimePeriod};
use types::config::SignalConfig;
use types::state::MarketCache;
use types::{
    Addr, Cluster, Link, LinkKind, MarketCategory, MarketMeta, PosStatus, Position, Signal,
    SignalKind, Source, Tier, UserStats, Wallet,
};

use super::*;

const NOW: i64 = 1_790_000_000;
const DAY: i64 = 86_400;

fn addr(n: u8) -> Addr {
    Addr::parse(&format!("0x{n:040x}")).unwrap()
}

fn cfg() -> SignalConfig {
    SignalConfig::default()
}

/// A position; value and price follow from the status (Open: marked at entry,
/// Redeemable: won, RedeemableLost: lost, Closed: sold at entry).
fn pos(cid: &str, status: PosStatus, avg_price: f64, cost: f64) -> Position {
    let shares = cost / avg_price;
    let (current_price, current_value) = match status {
        PosStatus::Open => (avg_price, cost),
        PosStatus::Redeemable => (1.0, shares),
        PosStatus::RedeemableLost => (0.0, 0.0),
        PosStatus::Closed => (avg_price, 0.0),
    };
    Position {
        condition_id: cid.to_string(),
        token_id: format!("tok-{cid}"),
        title: format!("Market {cid}"),
        slug: cid.to_string(),
        outcome: "Yes".into(),
        avg_price,
        total_size: shares,
        current_size: if status == PosStatus::Closed {
            0.0
        } else {
            shares
        },
        cost,
        current_value,
        current_price,
        realized_pnl: 0.0,
        total_pnl: current_value - cost,
        percent_pnl: (current_value - cost) / cost * 100.0,
        status,
        first_entry_at: None,
        last_event_at: None,
        end_date: None,
        market_volume: None,
        market_duration_h: Some(24.0 * 30.0),
        category: MarketCategory::Politics,
    }
}

fn open(cid: &str, avg_price: f64, cost: f64) -> Position {
    pos(cid, PosStatus::Open, avg_price, cost)
}

fn won(cid: &str, avg_price: f64, cost: f64) -> Position {
    pos(cid, PosStatus::Redeemable, avg_price, cost)
}

fn lost(cid: &str, avg_price: f64, cost: f64) -> Position {
    pos(cid, PosStatus::RedeemableLost, avg_price, cost)
}

fn wallet(n: u8, age_days: i64, markets_traded: u64, positions: Vec<Position>) -> Wallet {
    let mut w = Wallet::new(addr(n), NOW);
    w.stats = Some(UserStats {
        join_date: Some(NOW - age_days * DAY),
        markets_traded,
        ..Default::default()
    });
    w.positions = positions;
    w.sources = vec![Source::Feed];
    w
}

/// 12 wins out of 15 resolved, all bought at 0.25.
fn sharp_record() -> Vec<Position> {
    (0..15)
        .map(|i| {
            let cid = format!("r{i:02}");
            if i < 12 {
                won(&cid, 0.25, 400.0)
            } else {
                lost(&cid, 0.25, 400.0)
            }
        })
        .collect()
}

struct Inputs {
    markets: MarketCache,
    leaderboard: Vec<LeaderboardHit>,
    biggest_wins: Vec<BiggestWinnerRow>,
    longshot_hits: Vec<LongshotHit>,
    price_paths: HashMap<String, Vec<PricePoint>>,
}

impl Inputs {
    fn new() -> Self {
        Inputs {
            markets: MarketCache::default(),
            leaderboard: Vec::new(),
            biggest_wins: Vec::new(),
            longshot_hits: Vec::new(),
            price_paths: HashMap::new(),
        }
    }

    fn eval(&self, w: &Wallet) -> Evaluation {
        self.eval_with(w, &cfg())
    }

    fn eval_with(&self, w: &Wallet, cfg: &SignalConfig) -> Evaluation {
        let input = ScoreInput {
            wallet: w,
            markets: &self.markets,
            leaderboard: &self.leaderboard,
            biggest_wins: &self.biggest_wins,
            longshot_hits: &self.longshot_hits,
            price_paths: &self.price_paths,
            now: NOW,
        };
        evaluate(&input, cfg)
    }
}

fn eval(w: &Wallet) -> Evaluation {
    Inputs::new().eval(w)
}

fn signal(e: &Evaluation, kind: SignalKind) -> Option<&Signal> {
    e.signals.iter().find(|s| s.kind == kind)
}

fn strength(e: &Evaluation, kind: SignalKind) -> f64 {
    signal(e, kind).map_or(0.0, |s| s.strength)
}

fn market(
    cid: &str,
    start: i64,
    end: i64,
    volume: Option<f64>,
    category: MarketCategory,
) -> MarketMeta {
    MarketMeta {
        condition_id: cid.to_string(),
        question: format!("Market {cid}?"),
        slug: cid.to_string(),
        event_slug: String::new(),
        start_date: Some(start),
        end_date: Some(end),
        closed_time: None,
        volume,
        liquidity: None,
        outcomes: vec!["Yes".into(), "No".into()],
        token_ids: vec![format!("tok-{cid}"), format!("tok-{cid}-no")],
        outcome_prices: vec![0.5, 0.5],
        winning_index: None,
        category,
        neg_risk: false,
    }
}

fn sig(kind: SignalKind, strength: f64) -> Signal {
    Signal {
        kind,
        strength,
        evidence: String::new(),
        refs: Vec::new(),
    }
}

#[test]
fn fresh_account_with_two_cheap_longshots() {
    let w = wallet(
        1,
        10,
        2,
        vec![open("a", 0.10, 300.0), open("b", 0.20, 300.0)],
    );
    let e = eval(&w);
    let s = signal(&e, SignalKind::NewAccountLongshot).unwrap();
    assert_eq!(s.strength, 0.75);
    assert_eq!(s.refs, ["a", "b"]);
    assert_eq!(e.value, 600.0);
    insta::assert_json_snapshot!(e);
}

#[test]
fn new_account_longshot_rejects_old_expensive_or_spread() {
    let base = vec![open("a", 0.10, 300.0), open("b", 0.20, 300.0)];
    assert!(signal(
        &eval(&wallet(1, 61, 2, base.clone())),
        SignalKind::NewAccountLongshot
    )
    .is_none());
    let pricey = vec![open("a", 0.40, 300.0), open("b", 0.30, 300.0)];
    assert!(signal(
        &eval(&wallet(1, 10, 2, pricey)),
        SignalKind::NewAccountLongshot
    )
    .is_none());
    let small = vec![open("a", 0.10, 50.0), open("b", 0.20, 50.0)];
    assert!(signal(
        &eval(&wallet(1, 10, 2, small)),
        SignalKind::NewAccountLongshot
    )
    .is_none());
    let spread: Vec<_> = (0..7).map(|i| open(&format!("m{i}"), 0.1, 100.0)).collect();
    assert!(signal(
        &eval(&wallet(1, 10, 7, spread)),
        SignalKind::NewAccountLongshot
    )
    .is_none());
    let mut unknown_age = wallet(1, 10, 2, base);
    unknown_age.stats = None;
    assert!(signal(&eval(&unknown_age), SignalKind::NewAccountLongshot).is_none());
}

#[test]
fn seasoned_sharp_wins_and_outcome_selection_penalty() {
    let w = wallet(2, 400, 120, sharp_record());
    let e = eval(&w);
    assert_eq!(e.resolved.n, 15.0);
    assert_eq!(e.resolved.wins, 12.0);
    assert_eq!(e.resolved.expected_wins, 3.75);
    assert_eq!(e.resolved.longshot_wins, 12);
    assert_eq!(e.resolved.z, round3(8.25 / 2.8125f64.sqrt()));
    let s = signal(&e, SignalKind::OldAccountWinRate).unwrap();
    assert_eq!(s.strength, 1.0);
    assert_eq!(
        s.evidence,
        "won 12/15 resolved, mean entry 0.25, edge +55 pts, z=4.9"
    );
    assert_eq!(s.refs.len(), 12);
    insta::assert_json_snapshot!(e);

    let mut selected = w.clone();
    selected.sources = vec![Source::Leaderboard];
    let penalised = strength(&eval(&selected), SignalKind::OldAccountWinRate);
    assert!(penalised > 0.5 && penalised < 1.0, "{penalised}");

    // Outcome-selected wallets need more resolved positions.
    selected.positions.truncate(14);
    assert!(signal(&eval(&selected), SignalKind::OldAccountWinRate).is_none());
}

#[test]
fn coin_flip_gambler_gets_no_win_rate_signal() {
    let ps = (0..20)
        .map(|i| {
            let cid = format!("g{i}");
            if i % 2 == 0 {
                won(&cid, 0.5, 100.0)
            } else {
                lost(&cid, 0.5, 100.0)
            }
        })
        .collect();
    let e = eval(&wallet(3, 300, 50, ps));
    assert_eq!(e.resolved.z, 0.0);
    assert_eq!(e.resolved.win_rate(), Some(0.5));
    assert!(signal(&e, SignalKind::OldAccountWinRate).is_none());
}

#[test]
fn win_rate_requires_longshot_wins() {
    // 15/15 at 0.6: huge z but no longshot wins.
    let ps = (0..15).map(|i| won(&format!("f{i}"), 0.6, 100.0)).collect();
    let e = eval(&wallet(3, 300, 50, ps));
    assert!(e.resolved.z > 2.0);
    assert!(signal(&e, SignalKind::OldAccountWinRate).is_none());
}

#[test]
fn sports_heavy_record_is_discounted() {
    let mut ps = sharp_record();
    for p in &mut ps {
        p.category = MarketCategory::Sports;
    }
    let e = eval(&wallet(4, 400, 120, ps.clone()));
    let s = signal(&e, SignalKind::OldAccountWinRate).unwrap();
    assert_eq!(s.strength, 0.5);
    assert!(s.evidence.ends_with("100% sports/crypto"));

    // Category from the market cache when the position doesn't carry one.
    let mut inputs = Inputs::new();
    for p in &mut ps {
        p.category = MarketCategory::Other;
        let m = market(&p.condition_id, 0, 30 * DAY, None, MarketCategory::Crypto);
        inputs.markets.markets.insert(p.condition_id.clone(), m);
    }
    for p in ps.iter_mut().take(5) {
        p.category = MarketCategory::Politics;
    }
    let e = inputs.eval(&wallet(4, 400, 120, ps));
    assert_eq!(
        strength(&e, SignalKind::OldAccountWinRate),
        round3(1.0 - 0.5 * 10.0 / 15.0)
    );
}

#[test]
fn closed_positions_count_at_half_weight() {
    let mut sold_win = pos("c1", PosStatus::Closed, 0.2, 100.0);
    sold_win.current_price = 1.0;
    let mut sold_loss = pos("c2", PosStatus::Closed, 0.2, 100.0);
    sold_loss.current_price = 0.0;
    let still_live = pos("c3", PosStatus::Closed, 0.2, 100.0);
    let w = wallet(
        5,
        100,
        10,
        vec![sold_win, sold_loss, still_live, won("r", 0.2, 100.0)],
    );
    let s = resolved_summary(&w, &MarketCache::default(), &cfg());
    assert_eq!(s.n, 2.0);
    assert_eq!(s.wins, 1.5);
    assert!((s.expected_wins - 0.4).abs() < 1e-12);
    assert_eq!(s.longshot_wins, 2);
}

#[test]
fn short_markets_are_ignored_entirely() {
    let mut fast = open("btc-5m", 0.1, 1000.0);
    fast.market_duration_h = Some(0.25);
    let mut fast_win = won("eth-15m", 0.1, 1000.0);
    fast_win.market_duration_h = None;
    let mut inputs = Inputs::new();
    inputs.markets.markets.insert(
        "eth-15m".into(),
        market("eth-15m", 0, 900, Some(1e6), MarketCategory::Crypto),
    );
    let e = inputs.eval(&wallet(6, 5, 2, vec![fast, fast_win]));
    assert_eq!(e.value, 0.0);
    assert_eq!(e.resolved.n, 0.0);
    assert!(e.signals.is_empty());
    assert_eq!(e.score.total, 0);
}

#[test]
fn dust_is_not_value_but_counts_as_loss() {
    let mut dust = open("d", 0.3, 500.0);
    dust.current_value = 0.5;
    dust.current_price = 0.0003;
    dust.percent_pnl = -99.9;
    let w = wallet(
        7,
        300,
        10,
        vec![dust, open("a", 0.4, 300.0), won("w", 0.5, 100.0)],
    );
    let e = eval(&w);
    assert_eq!(e.value, 300.0 + 200.0);
    assert_eq!(e.resolved.n, 2.0);
    assert_eq!(e.resolved.wins, 1.0);
    // The dust position doesn't dilute concentration either: one live open position.
    assert_eq!(signal(&e, SignalKind::Concentration).unwrap().refs, ["a"]);
}

#[test]
fn leaderboard_rank_needs_young_or_narrow_account() {
    let mut inputs = Inputs::new();
    inputs.leaderboard = vec![LeaderboardHit {
        period: TimePeriod::Week,
        rank: 10,
        pnl: 45_000.0,
    }];
    let young = wallet(8, 30, 100, vec![]);
    let s = inputs.eval(&young);
    let s = signal(&s, SignalKind::LeaderboardEntrant).unwrap();
    assert_eq!(s.strength, round3(1.0 - 0.5 * 10.0 / 501.0));
    assert_eq!(s.evidence, "rank 10 on week PnL leaderboard ($45.0k)");

    assert!(signal(
        &inputs.eval(&wallet(8, 400, 100, vec![])),
        SignalKind::LeaderboardEntrant
    )
    .is_none());
    assert!(signal(
        &inputs.eval(&wallet(8, 400, 15, vec![])),
        SignalKind::LeaderboardEntrant
    )
    .is_some());

    inputs.leaderboard[0].rank = 501;
    assert!(signal(&inputs.eval(&young), SignalKind::LeaderboardEntrant).is_none());
}

#[test]
fn biggest_win_multiple_rule() {
    let row = |cid: &str, initial: f64, pnl: f64| BiggestWinnerRow {
        win_rank: 1,
        user_id: addr(9),
        pnl,
        initial_value: initial,
        final_value: initial + pnl,
        resolved_at: NOW - DAY,
        condition_id: cid.to_string(),
        event_slug: String::new(),
        event_title: String::new(),
        user_name: String::new(),
    };
    let mut inputs = Inputs::new();
    inputs.biggest_wins = vec![
        row("big", 500.0, 5000.0),
        row("tiny", 100.0, 5000.0),
        row("meh", 1000.0, 3000.0),
    ];
    let e = inputs.eval(&wallet(9, 400, 300, vec![]));
    let s = signal(&e, SignalKind::LeaderboardEntrant).unwrap();
    assert_eq!(s.strength, round3(0.6 + 0.4 * 5.0 / 15.0));
    assert_eq!(s.refs, ["big"]);
    assert_eq!(s.evidence, "biggest win 10.0x on $500");

    inputs.biggest_wins = vec![row("huge", 200.0, 200.0 * 40.0)];
    assert_eq!(
        strength(
            &inputs.eval(&wallet(9, 400, 300, vec![])),
            SignalKind::LeaderboardEntrant
        ),
        1.0
    );
}

#[test]
fn longshot_winner_hits() {
    let hit = |cid: &str, entry: f64| LongshotHit {
        condition_id: cid.to_string(),
        title: format!("Market {cid}"),
        token_id: format!("tok-{cid}"),
        entry_price: entry,
        low_price_before_close: entry * 0.8,
        closed_at: NOW - DAY,
        amount: 1000.0,
    };
    let mut inputs = Inputs::new();
    inputs.longshot_hits = vec![hit("one", 0.175)];
    let w = wallet(10, 400, 50, vec![]);
    assert_eq!(strength(&inputs.eval(&w), SignalKind::LongshotWinner), 0.8);

    inputs.longshot_hits = vec![hit("a", 0.28), hit("b", 0.35), hit("c", 0.5)];
    let e = inputs.eval(&w);
    let s = signal(&e, SignalKind::LongshotWinner).unwrap();
    assert_eq!(s.strength, round3(0.6 + 0.4 * 0.2 + 0.2));
    assert_eq!(s.refs, ["a", "b"]);

    inputs.longshot_hits = vec![hit("a", 0.07), hit("b", 0.2)];
    assert_eq!(strength(&inputs.eval(&w), SignalKind::LongshotWinner), 1.0);

    inputs.longshot_hits = vec![hit("c", 0.5)];
    assert!(signal(&inputs.eval(&w), SignalKind::LongshotWinner).is_none());
}

#[test]
fn concentration_rules() {
    let e = eval(&wallet(
        11,
        400,
        50,
        vec![open("big", 0.5, 800.0), open("small", 0.5, 200.0)],
    ));
    let s = signal(&e, SignalKind::Concentration).unwrap();
    assert_eq!(s.strength, 0.5);
    assert_eq!(s.refs, ["big"]);
    assert_eq!(
        s.evidence,
        "80% of $1,000 open value in one position, 2 markets"
    );

    let barely = eval(&wallet(
        11,
        400,
        50,
        vec![open("big", 0.5, 620.0), open("small", 0.5, 380.0)],
    ));
    assert_eq!(strength(&barely, SignalKind::Concentration), 0.3);

    let spread = (0..4)
        .map(|i| open(&format!("m{i}"), 0.5, if i == 0 { 900.0 } else { 30.0 }))
        .collect();
    assert!(signal(
        &eval(&wallet(11, 400, 50, spread)),
        SignalKind::Concentration
    )
    .is_none());

    let small = eval(&wallet(11, 400, 50, vec![open("big", 0.5, 150.0)]));
    assert!(signal(&small, SignalKind::Concentration).is_none());

    let balanced = eval(&wallet(
        11,
        400,
        50,
        vec![open("a", 0.5, 500.0), open("b", 0.5, 500.0)],
    ));
    assert!(signal(&balanced, SignalKind::Concentration).is_none());
}

#[test]
fn resolution_timing() {
    let end = 1_789_948_800; // 00:00 UTC; trading closes about a day later.
    let late = |cid: &str, hours_before_close: i64, price: f64| Position {
        first_entry_at: Some(end + DAY - hours_before_close * 3600),
        end_date: Some(end),
        ..won(cid, price, 100.0)
    };
    let ps = vec![
        late("a", 12, 0.3),
        late("b", 47, 0.25),
        late("c", 30, 0.2),
        late("early", 24 * 10, 0.3),
        late("pricey", 6, 0.8),
        late("after-close", -2, 0.2),
    ];
    let e = eval(&wallet(12, 400, 50, ps.clone()));
    let s = signal(&e, SignalKind::ResolutionTiming).unwrap();
    assert_eq!(s.strength, round3(3.0 / 6.0));
    assert_eq!(s.refs, ["a", "b", "c"]);
    assert_eq!(
        s.evidence,
        "3/6 wins entered <= 48h before close at <= 0.35"
    );

    // Too few late cheap wins.
    let two = eval(&wallet(
        12,
        400,
        50,
        vec![
            late("a", 12, 0.3),
            late("b", 20, 0.3),
            late("early", 240, 0.3),
        ],
    ));
    assert!(signal(&two, SignalKind::ResolutionTiming).is_none());

    // Sports games are bet on game day by everyone: no timing evidence.
    let sports: Vec<Position> = ps
        .into_iter()
        .map(|p| Position {
            category: MarketCategory::Sports,
            ..p
        })
        .collect();
    assert!(signal(
        &eval(&wallet(12, 400, 50, sports)),
        SignalKind::ResolutionTiming
    )
    .is_none());
}

#[test]
fn pre_move_from_price_paths() {
    let t = NOW - 10 * DAY;
    let entered = |cid: &str, price: f64| Position {
        first_entry_at: Some(t),
        ..open(cid, price, 300.0)
    };
    let mut inputs = Inputs::new();
    let pt = |dt: i64, price: f64| PricePoint {
        timestamp: t + dt,
        price,
    };
    inputs.price_paths.insert(
        "tok-up".into(),
        vec![pt(0, 0.9), pt(3600, 0.3), pt(7200, 0.5), pt(DAY, 0.4)],
    );
    inputs
        .price_paths
        .insert("tok-up2".into(), vec![pt(600, 0.45), pt(DAY, 0.3)]);
    inputs.price_paths.insert(
        "tok-late".into(),
        vec![pt(-60, 0.9), pt(3600, 0.3), pt(DAY + 60, 0.95)],
    );
    // Moves, but excluded: a sports market, and a market that resolves within the window.
    inputs
        .price_paths
        .insert("tok-sports".into(), vec![pt(600, 0.9)]);
    inputs
        .price_paths
        .insert("tok-resolving".into(), vec![pt(600, 0.9)]);
    let w = wallet(
        13,
        400,
        50,
        vec![
            entered("up", 0.2),
            entered("up2", 0.2),
            entered("late", 0.2),
            entered("no-path", 0.1),
            Position {
                category: MarketCategory::Sports,
                ..entered("sports", 0.2)
            },
            Position {
                end_date: Some(t - t.rem_euclid(DAY)),
                ..entered("resolving", 0.2)
            },
        ],
    );
    let e = inputs.eval(&w);
    let s = signal(&e, SignalKind::PreMove).unwrap();
    assert_eq!(s.strength, round3(2.0 / 3.0));
    assert_eq!(s.refs, ["up", "up2"]);
    assert_eq!(
        s.evidence,
        "2/3 checked positions rose >= 0.25 within 24h of entry (max +0.30)"
    );

    assert!(signal(&eval(&w), SignalKind::PreMove).is_none());

    // One lucky move among several checked positions is noise.
    let mut one = Inputs::new();
    one.price_paths
        .insert("tok-up".into(), inputs.price_paths["tok-up"].clone());
    one.price_paths
        .insert("tok-late".into(), inputs.price_paths["tok-late"].clone());
    assert!(signal(&one.eval(&w), SignalKind::PreMove).is_none());
}

#[test]
fn niche_market_share_of_volume() {
    let mut inputs = Inputs::new();
    inputs.markets.markets.insert(
        "thin".into(),
        market("thin", 0, 30 * DAY, Some(25_000.0), MarketCategory::Other),
    );
    let deep = Position {
        market_volume: Some(5e6),
        ..open("deep", 0.3, 5000.0)
    };
    let crumbs = Position {
        market_volume: Some(1000.0),
        ..open("crumbs", 0.3, 40.0)
    };
    let w = wallet(14, 400, 50, vec![open("thin", 0.3, 2000.0), deep, crumbs]);
    let e = inputs.eval(&w);
    let s = signal(&e, SignalKind::NicheMarket).unwrap();
    assert_eq!(s.strength, 0.9);
    assert_eq!(s.refs, ["thin"]);
    assert_eq!(s.evidence, "$2,000 = 8% of a $25.0k-volume market");

    let whale = Position {
        market_volume: Some(10_000.0),
        ..open("x", 0.3, 5000.0)
    };
    assert_eq!(
        strength(
            &eval(&wallet(14, 400, 50, vec![whale])),
            SignalKind::NicheMarket
        ),
        1.0
    );
    assert!(signal(&eval(&w), SignalKind::NicheMarket).is_none());
}

#[test]
fn combine_caps_materiality_and_tiers() {
    let c = cfg();
    let all: Vec<Signal> = SignalKind::ALL.iter().map(|k| sig(*k, 1.0)).collect();
    let s = combine(&all, 10_000.0, &c);
    assert_eq!((s.total, s.tier, s.materiality), (100, Tier::High, 1.2));
    assert_eq!(combine(&all, 1e12, &c).total, 100);
    assert_eq!(combine(&all, 1e12, &c).materiality, 1.2);
    assert_eq!(combine(&all, 0.0, &c).materiality, 0.8);
    assert_eq!(combine(&all, 50.0, &c).total, 80);

    // One strong signal reaches the watchlist; a second one makes an issue.
    let one = [sig(SignalKind::OldAccountWinRate, 1.0)];
    assert_eq!(combine(&one, 1000.0, &c).total, 55);
    assert_eq!(combine(&one, 1000.0, &c).tier, Tier::Medium);
    assert_eq!(combine(&one, 1000.0, &c).materiality, 1.0);
    assert_eq!(combine(&one, 100.0, &c).total, 44);

    let two = [
        sig(SignalKind::OldAccountWinRate, 1.0),
        sig(SignalKind::Concentration, 0.5),
    ];
    assert_eq!(combine(&two, 1000.0, &c).total, 63);
    assert_eq!(combine(&two, 100.0, &c).total, 50);

    let high = [
        sig(SignalKind::OldAccountWinRate, 1.0),
        sig(SignalKind::ResolutionTiming, 1.0),
    ];
    assert_eq!(combine(&high, 1000.0, &c).total, 75);
    assert_eq!(combine(&high, 1000.0, &c).tier, Tier::High);

    // Supporting context alone scores nothing…
    let context = [
        sig(SignalKind::LeaderboardEntrant, 1.0),
        sig(SignalKind::Concentration, 1.0),
        sig(SignalKind::ResolutionTiming, 1.0),
        sig(SignalKind::NicheMarket, 1.0),
    ];
    assert_eq!(combine(&context, 10_000.0, &c).total, 0);
    // …counts fully once primary evidence reaches the gate (30)…
    let gated = [
        sig(SignalKind::NewAccountLongshot, 0.75),
        sig(SignalKind::Concentration, 1.0),
    ];
    assert_eq!(combine(&gated, 1000.0, &c).total, 45);
    // …and proportionally below it.
    let weak = [
        sig(SignalKind::NewAccountLongshot, 0.5),
        sig(SignalKind::Concentration, 1.0),
    ];
    assert_eq!(combine(&weak, 1000.0, &c).total, 30);

    // A losing track record halves the score.
    let losing = ResolvedSummary {
        n: 20.0,
        z: -3.0,
        ..Default::default()
    };
    assert_eq!(combine_with_record(&one, 1000.0, &losing, &c).total, 28);
    let few = ResolvedSummary { n: 5.0, ..losing };
    assert_eq!(combine_with_record(&one, 1000.0, &few, &c).total, 55);
    assert_eq!(
        combine(&[], 10_000.0, &c),
        Score {
            total: 0,
            tier: Tier::Low,
            materiality: 1.2
        }
    );
}

#[test]
fn combine_is_bounded_over_a_grid() {
    let c = cfg();
    for k in 0..=20 {
        let strength = k as f64 / 20.0;
        let signals: Vec<Signal> = SignalKind::ALL
            .iter()
            .map(|kind| sig(*kind, strength))
            .collect();
        let mut last = 0;
        for e in -2..=12 {
            let s = combine(&signals, 10f64.powi(e), &c);
            assert!((0.8..=1.2).contains(&s.materiality));
            assert!(s.total <= 100);
            assert!(s.total >= last, "score must not fall with value");
            last = s.total;
        }
    }
}

#[test]
fn idle_profitable_wallet_keeps_materiality() {
    let w = wallet(15, 400, 120, sharp_record());
    let e = eval(&w);
    assert_eq!(e.value, 12.0 * 1600.0);
    let mut redeemed = w.clone();
    for p in &mut redeemed.positions {
        p.current_value = 0.0;
    }
    let e = eval(&redeemed);
    assert_eq!(e.value, 0.0);
    // Basis = $4,800 of winning cost, not $0.
    assert_eq!(e.score.materiality, round3(1.0 + 0.2 * 4.8f64.log10()));
}

#[test]
fn evaluation_of_a_kitchen_sink_insider() {
    let end = 1_789_948_800;
    let entry = end + DAY - 20 * 3600;
    let mut inputs = Inputs::new();
    inputs.markets.markets.insert(
        "x1".into(),
        market("x1", 0, 30 * DAY, Some(40_000.0), MarketCategory::Politics),
    );
    inputs.price_paths.insert(
        "tok-x1".into(),
        vec![
            PricePoint {
                timestamp: entry + 3600,
                price: 0.2,
            },
            PricePoint {
                timestamp: entry + 7200,
                price: 0.6,
            },
        ],
    );
    inputs.longshot_hits = vec![LongshotHit {
        condition_id: "x2".into(),
        title: "Market x2".into(),
        token_id: "tok-x2".into(),
        entry_price: 0.12,
        low_price_before_close: 0.09,
        closed_at: end + DAY,
        amount: 5000.0,
    }];
    let mut ps = vec![
        Position {
            first_entry_at: Some(entry),
            end_date: Some(end),
            ..won("x1", 0.15, 3000.0)
        },
        Position {
            first_entry_at: Some(entry),
            end_date: Some(end),
            ..won("x2", 0.12, 600.0)
        },
        open("x3", 0.2, 900.0),
    ];
    ps[1].market_volume = Some(3e6);
    let mut w = wallet(16, 20, 3, ps);
    w.sources = vec![Source::LongshotHolder];
    let e = inputs.eval(&w);
    let kinds: Vec<SignalKind> = e.signals.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        [
            SignalKind::NewAccountLongshot,
            SignalKind::LongshotWinner,
            SignalKind::Concentration,
            SignalKind::NicheMarket
        ]
    );
    assert_eq!(e.score.tier, Tier::High);
    insta::assert_json_snapshot!(e);
}

#[test]
fn premove_requests_gating_and_order() {
    let t = NOW - 5 * DAY;
    let mut ps: Vec<Position> = (0..7)
        .map(|i| Position {
            first_entry_at: Some(t + i),
            ..open(&format!("p{i}"), 0.2, 100.0 * (i + 1) as f64)
        })
        .collect();
    ps.push(Position {
        first_entry_at: Some(t),
        ..pos("sold", PosStatus::Closed, 0.2, 9000.0)
    });
    ps.push(open("no-entry", 0.2, 8000.0));
    ps.push(Position {
        first_entry_at: Some(t),
        market_duration_h: Some(1.0),
        ..open("fast", 0.2, 7000.0)
    });
    ps.push(Position {
        first_entry_at: Some(t),
        ..won("resolved", 0.2, 650.0)
    });
    let w = wallet(17, 400, 50, ps);

    // Gated on the provisional signals' ungated evidence, not on the (gated) score.
    let mut provisional = eval(&w);
    provisional.signals = vec![sig(SignalKind::Concentration, 1.0)];
    assert!(premove_requests(&w, &provisional, &cfg()).is_empty());

    provisional.signals = vec![
        sig(SignalKind::Concentration, 1.0),
        sig(SignalKind::ResolutionTiming, 1.0),
        sig(SignalKind::NicheMarket, 1.0),
    ];
    let reqs = premove_requests(&w, &provisional, &cfg());
    let tokens: Vec<&str> = reqs.iter().map(|r| r.token_id.as_str()).collect();
    assert_eq!(
        tokens,
        ["tok-p6", "tok-resolved", "tok-p5", "tok-p4", "tok-p3"]
    );
    assert_eq!(
        reqs[0],
        PriceRequest {
            token_id: "tok-p6".into(),
            start: t + 6,
            end: t + 6 + DAY
        }
    );
}

fn scored(n: u8, signals: Vec<Signal>, value: f64) -> Wallet {
    let mut w = wallet(n, 400, 50, vec![]);
    w.score = combine(&signals, value, &cfg());
    w.signals = signals;
    w.value = value;
    w
}

fn cluster(members: &[u8], kind: LinkKind) -> Cluster {
    let members: Vec<Addr> = members.iter().map(|n| addr(*n)).collect();
    let links = members
        .windows(2)
        .map(|p| Link {
            a: p[0].clone(),
            b: p[1].clone(),
            kind,
            confidence: 0.9,
            evidence: String::new(),
        })
        .collect();
    Cluster {
        id: format!("c-{}", &members[0].as_str()[2..12]),
        members,
        links,
    }
}

#[test]
fn cluster_bonus_add_refresh_and_remove() {
    let strong = || vec![sig(SignalKind::OldAccountWinRate, 1.0)];
    let mut ws = vec![
        scored(1, strong(), 1000.0),
        scored(2, strong(), 1000.0),
        scored(3, vec![sig(SignalKind::Concentration, 0.5)], 1000.0),
        scored(4, strong(), 1000.0),
    ];
    let clusters = vec![
        cluster(&[1, 2, 3], LinkKind::SameXUsername),
        cluster(&[4], LinkKind::CoTrade),
    ];
    apply_cluster_bonus(&mut ws, &clusters, &cfg());

    let linked = |w: &Wallet| {
        w.signals
            .iter()
            .find(|s| s.kind == SignalKind::LinkedAccounts)
            .cloned()
    };
    let a = linked(&ws[0]).unwrap();
    assert_eq!(a.strength, 0.5);
    assert_eq!(a.evidence, "linked to 2 wallets (1 flagged): SameXUsername");
    assert_eq!(ws[0].score.total, 65);
    assert_eq!(
        ws[0].signals.last().unwrap().kind,
        SignalKind::LinkedAccounts
    );
    assert_eq!(
        linked(&ws[2]).unwrap().evidence,
        "linked to 2 wallets (2 flagged): SameXUsername"
    );
    assert_eq!(ws[2].score.total, 13);
    assert_eq!(ws[2].cluster_id.as_deref(), Some("c-0000000000"));
    assert!(linked(&ws[3]).is_none());
    assert_eq!(ws[3].cluster_id, None);

    // Re-applying replaces rather than stacks.
    let before = ws.clone();
    apply_cluster_bonus(&mut ws, &clusters, &cfg());
    assert_eq!(ws, before);

    apply_cluster_bonus(&mut ws, &[], &cfg());
    assert!(ws
        .iter()
        .all(|w| linked(w).is_none() && w.cluster_id.is_none()));
    assert_eq!(ws[0].score.total, 55);
    // Context alone (no primary evidence left) scores nothing.
    assert_eq!(ws[2].score.total, 0);
}

#[test]
fn cluster_without_second_flagged_member_sets_id_only() {
    let mut ws = vec![
        scored(1, vec![sig(SignalKind::OldAccountWinRate, 1.0)], 1000.0),
        scored(2, vec![], 0.0),
    ];
    apply_cluster_bonus(&mut ws, &[cluster(&[1, 2], LinkKind::SameOwnerEoa)], &cfg());
    assert!(ws.iter().all(|w| w.cluster_id.is_some()));
    assert!(ws.iter().all(|w| w
        .signals
        .iter()
        .all(|s| s.kind != SignalKind::LinkedAccounts)));
    assert_eq!(ws[0].score.total, 55);
}

#[test]
fn cluster_bonus_caps_at_full_weight() {
    let strong = || vec![sig(SignalKind::OldAccountWinRate, 1.0)];
    let mut ws: Vec<Wallet> = (1..=5).map(|n| scored(n, strong(), 1000.0)).collect();
    apply_cluster_bonus(
        &mut ws,
        &[cluster(&[1, 2, 3, 4, 5], LinkKind::SequentialUserId)],
        &cfg(),
    );
    assert!(ws.iter().all(|w| w.signals.last().unwrap().strength == 1.0));
    assert!(ws.iter().all(|w| w.score.total == 75));
}

#[test]
fn hyperactive_young_trader_is_not_a_new_account_longshot() {
    // 30 days old with one cheap open bet, but 900 markets traded: not a fresh account.
    let ps = vec![open("big", 0.05, 5000.0)];
    let busy = eval(&wallet(18, 30, 900, ps.clone()));
    assert!(signal(&busy, SignalKind::NewAccountLongshot).is_none());
    let fresh = eval(&wallet(18, 30, 1, ps));
    assert!(signal(&fresh, SignalKind::NewAccountLongshot).is_some());
}
