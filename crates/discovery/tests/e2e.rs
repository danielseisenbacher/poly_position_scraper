//! End-to-end discovery runs against a canned in-memory `PolyApi`.
//!
//! Planted wallets:
//! - WHALE: 10-day-old account, $5k on one market at 0.12 (feed), created by OWNER in the
//!   factory logs; SIB is OWNER's second proxy (not in the feed, found via the owner).
//! - SHARP: 400-day-old account, 12/15 resolved wins at 0.25 (feed).
//! - GAMBLER: old account, 9/20 at 0.50, spread over 5 markets (feed) → must not be flagged.
//! - COMBO / UPDOWN: only combo rows / crypto up-or-down rows → never enriched.
//! - OLD: big cheap buyer, but only before the feed stop mark → never enriched.
//! - HOLDER: bought the winner of resolved market r1 at 0.07 (traded at 0.08 before close)
//!   and already redeemed, so only `/v2/positions?condition=&status=CLOSED` shows it.
//! - PRO (winner at 0.60), LOSER (losing token) and DUST (winner at 0.05, $10) are r1
//!   positions that must not count.
//! - LEADER: day-leaderboard entry, scores low.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use async_trait::async_trait;
use discovery::{in_shard, run, DiscoverOpts, StateAssets};
use store::DataDir;
use types::api::*;
use types::{Addr, Config, MarketCategory, MarketMeta, PosStatus, Profile, SignalKind, Source};

const NOW: i64 = 1_792_600_000;
const DAY: i64 = 86_400;
const HEAD: u64 = 80_000_000;
const PAGE: usize = 4;
/// Page size of the mock's `market_positions`.
const MP_PAGE: usize = 2;

fn a(c: char) -> Addr {
    Addr::parse(&format!("0x{}", c.to_string().repeat(40))).unwrap()
}
fn whale() -> Addr {
    a('a')
}
fn sharp() -> Addr {
    a('b')
}
fn gambler() -> Addr {
    a('c')
}
fn combo() -> Addr {
    a('d')
}
fn updown() -> Addr {
    a('e')
}
fn holder() -> Addr {
    a('f')
}
fn sib() -> Addr {
    a('1')
}
fn owner() -> Addr {
    a('2')
}
fn pro() -> Addr {
    a('3')
}
fn leader() -> Addr {
    a('4')
}
fn dust() -> Addr {
    a('7')
}
fn loser() -> Addr {
    a('5')
}
fn old() -> Addr {
    a('6')
}

#[derive(Default)]
struct Mock {
    feed: Vec<FeedTrade>,
    fail_feed: bool,
    leaderboard: Vec<LeaderboardRow>,
    fail_leaderboard: bool,
    biggest: Vec<BiggestWinnerRow>,
    closed: Vec<MarketMeta>,
    prices: HashMap<String, Vec<PricePoint>>,
    /// `(condition, status)` → rows of `/v2/positions?condition=`, served `MP_PAGE` per page.
    market_positions: HashMap<(String, Option<PosStatus>), Vec<PositionRow>>,
    stats: HashMap<Addr, UserStatsRow>,
    fail_stats: HashSet<Addr>,
    positions: HashMap<(Addr, PosStatus), Vec<PositionRow>>,
    fail_positions: HashSet<(Addr, PosStatus)>,
    markets: HashMap<String, MarketMeta>,
    profiles: HashMap<Addr, Profile>,
    fail_profiles: bool,
    fail_rpc: bool,
    fail_status: bool,
    logs: Vec<FactoryLog>,
    calls: Mutex<Vec<(&'static str, String)>>,
}

fn err(what: &str) -> ApiError {
    ApiError::Http {
        status: 500,
        url: what.into(),
        body: "planted failure".into(),
    }
}

impl Mock {
    fn log(&self, ep: &'static str, arg: impl ToString) {
        self.calls.lock().unwrap().push((ep, arg.to_string()));
    }
    fn count(&self, ep: &str, arg: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(e, x)| *e == ep && x == arg)
            .count()
    }
    fn args(&self, ep: &str) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|(e, _)| *e == ep)
            .map(|(_, x)| x.clone())
            .collect()
    }
}

#[async_trait]
impl PolyApi for Mock {
    async fn trades_page(
        &self,
        q: &TradesQuery,
        cursor: Option<&str>,
    ) -> ApiResult<Page<FeedTrade>> {
        assert!(q.taker_only && q.limit == 1000);
        let i: usize = cursor.map_or(0, |c| c.parse().unwrap());
        self.log("trades", i);
        if self.fail_feed {
            return Err(err("trades"));
        }
        let mut rows = self.feed.clone();
        rows.sort_by_key(|x| std::cmp::Reverse(x.timestamp));
        let chunk: Vec<FeedTrade> = rows.iter().skip(i * PAGE).take(PAGE).cloned().collect();
        let next = ((i + 1) * PAGE < rows.len()).then(|| (i + 1).to_string());
        Ok(Page {
            data: chunk,
            next_cursor: next,
        })
    }
    async fn user_trades(
        &self,
        _: &Addr,
        _: Option<i64>,
        _: Option<&str>,
    ) -> ApiResult<Page<FeedTrade>> {
        Ok(Page::default())
    }
    async fn activity(&self, _: &ActivityQuery, _: Option<&str>) -> ApiResult<Page<ActivityRow>> {
        Ok(Page::default())
    }
    async fn positions(
        &self,
        user: &Addr,
        status: PosStatus,
        _: Option<&str>,
    ) -> ApiResult<Page<PositionRow>> {
        self.log("positions", user);
        if self.fail_positions.contains(&(user.clone(), status)) {
            return Err(err("positions"));
        }
        let data = self
            .positions
            .get(&(user.clone(), status))
            .cloned()
            .unwrap_or_default();
        Ok(Page {
            data,
            next_cursor: None,
        })
    }
    async fn market_positions(
        &self,
        condition_id: &str,
        status: Option<PosStatus>,
        start: Option<i64>,
        cursor: Option<&str>,
    ) -> ApiResult<Page<PositionRow>> {
        assert_eq!(start, None);
        self.log(
            "market_positions",
            format!("{condition_id}/{status:?}/{}", cursor.unwrap_or("-")),
        );
        let rows = self
            .market_positions
            .get(&(condition_id.to_string(), status))
            .cloned()
            .unwrap_or_default();
        let from: usize = cursor.map_or(0, |c| c.parse().unwrap());
        let to = (from + MP_PAGE).min(rows.len());
        Ok(Page {
            data: rows[from.min(to)..to].to_vec(),
            next_cursor: (to < rows.len()).then(|| to.to_string()),
        })
    }
    async fn holders(&self, condition_id: &str, _: bool, _: u32) -> ApiResult<Vec<HolderGroup>> {
        self.log("holders", condition_id);
        Ok(Vec::new())
    }
    async fn user_stats(&self, user: &Addr) -> ApiResult<Option<UserStatsRow>> {
        self.log("user_stats", user);
        if self.fail_stats.contains(user) {
            return Err(err("user_stats"));
        }
        Ok(self.stats.get(user).cloned())
    }
    async fn value(&self, _: &Addr) -> ApiResult<f64> {
        Ok(0.0)
    }
    async fn leaderboard(
        &self,
        period: TimePeriod,
        sort: LeaderboardSort,
        _: Option<&str>,
    ) -> ApiResult<Page<LeaderboardRow>> {
        assert_eq!(sort, LeaderboardSort::Pnl);
        self.log("leaderboard", format!("{period:?}"));
        if self.fail_leaderboard {
            return Err(err("leaderboard"));
        }
        let data = if period == TimePeriod::Day {
            self.leaderboard.clone()
        } else {
            Vec::new()
        };
        Ok(Page {
            data,
            next_cursor: None,
        })
    }
    async fn biggest_winners(
        &self,
        period: TimePeriod,
        _: Option<&str>,
    ) -> ApiResult<Page<BiggestWinnerRow>> {
        self.log("biggest_winners", format!("{period:?}"));
        let data = if period == TimePeriod::Day {
            self.biggest.clone()
        } else {
            Vec::new()
        };
        Ok(Page {
            data,
            next_cursor: None,
        })
    }
    async fn tokens(&self, _: &str) -> ApiResult<Vec<TokenRow>> {
        Ok(Vec::new())
    }
    async fn prices_history(&self, token_id: &str, _: i64, _: i64) -> ApiResult<Vec<PricePoint>> {
        self.log("prices", token_id);
        Ok(self.prices.get(token_id).cloned().unwrap_or_default())
    }
    async fn status(&self) -> ApiResult<ApiStatus> {
        if self.fail_status {
            return Err(err("status"));
        }
        Ok(ApiStatus {
            lag_seconds: 7,
            max_synced_block: None,
        })
    }
    async fn recently_closed_markets(
        &self,
        since: i64,
        min_volume: f64,
        max: usize,
    ) -> ApiResult<Vec<MarketMeta>> {
        self.log("closed", since);
        assert_eq!((min_volume, max), (10_000.0, 2100));
        Ok(self.closed.clone())
    }
    async fn markets_by_condition(&self, ids: &[String]) -> ApiResult<Vec<MarketMeta>> {
        self.log("markets", ids.len());
        Ok(ids
            .iter()
            .filter_map(|i| self.markets.get(i).cloned())
            .collect())
    }
    async fn profile(&self, addr: &Addr) -> ApiResult<Option<Profile>> {
        self.log("profile", addr);
        if self.fail_profiles {
            return Err(err("profile"));
        }
        Ok(self.profiles.get(addr).cloned())
    }
    async fn block_number(&self) -> ApiResult<u64> {
        if self.fail_rpc {
            return Err(err("rpc"));
        }
        Ok(HEAD)
    }
    async fn factory_logs(&self, from: u64, to: u64) -> ApiResult<Vec<FactoryLog>> {
        self.log("factory_logs", format!("{from}-{to}"));
        Ok(self
            .logs
            .iter()
            .filter(|l| (from..=to).contains(&l.block))
            .cloned()
            .collect())
    }
    fn stats(&self) -> RequestStats {
        let mut s = RequestStats::default();
        s.requests
            .insert(RateClass::Data, self.calls.lock().unwrap().len() as u64);
        s
    }
}

// ---- builders ----

fn trade(w: &Addr, cid: &str, side: Side, size: f64, price: f64, ts: i64) -> FeedTrade {
    FeedTrade {
        proxy_wallet: w.clone(),
        side,
        token_id: format!("{cid}-yes"),
        condition_id: cid.into(),
        size,
        price,
        timestamp: ts,
        title: format!("Will {cid} happen?"),
        slug: format!("will-{cid}-happen"),
        event_slug: String::new(),
        outcome: "Yes".into(),
        outcome_index: 0,
        name: String::new(),
        pseudonym: String::new(),
        profile_image: String::new(),
        transaction_hash: format!("0xtx{ts}"),
    }
}

#[allow(clippy::too_many_arguments)]
fn pos(
    w: &Addr,
    cid: &str,
    avg: f64,
    size: f64,
    cur: f64,
    status: PosStatus,
    end: &str,
    entry: i64,
) -> PositionRow {
    let cost = avg * size;
    let value = if matches!(status, PosStatus::Open | PosStatus::Redeemable) {
        size * cur
    } else {
        0.0
    };
    PositionRow {
        proxy_wallet: w.clone(),
        token_id: format!("{cid}-yes"),
        condition_id: cid.into(),
        current_size: size,
        avg_price: avg,
        entry_cost_usdc: cost,
        total_cost_usdc: cost,
        current_price: cur,
        current_value: value,
        total_size: size,
        realized_pnl: 0.0,
        unrealized_pnl: value - cost,
        total_pnl: value - cost,
        percent_pnl: (value - cost) / cost * 100.0,
        status,
        redeemable: status == PosStatus::Redeemable,
        title: format!("Will {cid} happen?"),
        slug: cid.into(),
        event_slug: String::new(),
        outcome: "Yes".into(),
        outcome_index: 0,
        end_date: Some(end.into()),
        first_entry_at: Some(entry),
        last_event_at: Some(entry),
    }
}

fn meta(
    cid: &str,
    start: i64,
    end: i64,
    volume: f64,
    category: MarketCategory,
    winner: Option<usize>,
) -> MarketMeta {
    MarketMeta {
        condition_id: cid.into(),
        question: format!("Will {cid} happen?"),
        slug: cid.into(),
        event_slug: String::new(),
        start_date: Some(start),
        end_date: Some(end),
        closed_time: winner.map(|_| end),
        volume: Some(volume),
        liquidity: None,
        outcomes: vec!["Yes".into(), "No".into()],
        token_ids: vec![format!("{cid}-yes"), format!("{cid}-no")],
        outcome_prices: if winner.is_some() {
            vec![1.0, 0.0]
        } else {
            vec![0.5, 0.5]
        },
        winning_index: winner,
        category,
        neg_risk: false,
    }
}

fn stats(w: &Addr, joined: i64, trades: u64, markets: u64) -> UserStatsRow {
    stats_pnl(w, joined, trades, markets, 0.0)
}

fn stats_pnl(w: &Addr, joined: i64, trades: u64, markets: u64, realized_pnl: f64) -> UserStatsRow {
    UserStatsRow {
        proxy_wallet: w.clone(),
        trades: markets,
        biggest_win: 0.0,
        join_date: Some(joined),
        all_time_pnl: Some(AllTimePnl {
            trade_count: trades,
            realized_pnl,
            ..Default::default()
        }),
    }
}

/// A row of `/v2/positions?condition=` for wallet `w` on `token` (`r1-yes` / `r1-no`).
fn market_row(w: &Addr, token: &str, avg: f64, size: f64, status: PosStatus) -> PositionRow {
    let (cid, side) = token.rsplit_once('-').unwrap();
    let won = side == "yes";
    let mut p = pos(
        w,
        cid,
        avg,
        size,
        if won { 1.0 } else { 0.0 },
        status,
        "2026-10-20",
        NOW - 8 * DAY,
    );
    p.token_id = token.into();
    p.outcome_index = i64::from(!won);
    if status == PosStatus::Closed {
        p.current_size = 0.0;
        p.current_value = 0.0;
    }
    p
}

/// The planted world, with every timestamp relative to `now`.
fn scenario(now: i64) -> Mock {
    let mut m = Mock::default();
    let open = |m: &mut Mock, cid: &str| {
        m.markets.insert(
            cid.into(),
            meta(
                cid,
                now - 30 * DAY,
                now + 30 * DAY,
                1e6,
                MarketCategory::Politics,
                None,
            ),
        );
    };
    let put = |m: &mut Mock, w: &Addr, rows: Vec<PositionRow>| {
        for r in rows {
            m.positions
                .entry((w.clone(), r.status))
                .or_default()
                .push(r);
        }
    };

    // WHALE + SIB (same owner).
    open(&mut m, "whale-m");
    m.feed.push(trade(
        &whale(),
        "whale-m",
        Side::Buy,
        20_000.0,
        0.12,
        now - 3000,
    ));
    m.feed.push(trade(
        &whale(),
        "whale-m",
        Side::Buy,
        21_666.0,
        0.12,
        now - 2000,
    ));
    m.stats
        .insert(whale(), stats(&whale(), now - 10 * DAY, 3, 1));
    put(
        &mut m,
        &whale(),
        vec![pos(
            &whale(),
            "whale-m",
            0.12,
            41_666.0,
            0.2,
            PosStatus::Open,
            "2026-11-30",
            now - 3000,
        )],
    );
    m.stats.insert(sib(), stats(&sib(), now - 5 * DAY, 2, 1));
    put(
        &mut m,
        &sib(),
        vec![pos(
            &sib(),
            "whale-m",
            0.10,
            5_000.0,
            0.2,
            PosStatus::Open,
            "2026-11-30",
            now - 4 * DAY,
        )],
    );
    m.logs = vec![
        FactoryLog {
            proxy: whale(),
            owner: owner(),
            block: HEAD - 100,
            tx: "0x1".into(),
        },
        FactoryLog {
            proxy: sib(),
            owner: owner(),
            block: HEAD - 50,
            tx: "0x2".into(),
        },
    ];

    // SHARP: 12/15 at 0.25 plus three open markets bought in the feed.
    m.stats.insert(
        sharp(),
        stats_pnl(&sharp(), now - 400 * DAY, 300, 40, 5000.0),
    );
    let mut rows = Vec::new();
    for i in 0..15 {
        let cid = format!("sharp-r{i}");
        m.markets.insert(
            cid.clone(),
            meta(
                &cid,
                now - 200 * DAY,
                now - 100 * DAY,
                1e6,
                MarketCategory::Politics,
                Some(if i < 12 { 0 } else { 1 }),
            ),
        );
        let (cur, st) = if i < 12 {
            (1.0, PosStatus::Redeemable)
        } else {
            (0.0, PosStatus::RedeemableLost)
        };
        rows.push(pos(
            &sharp(),
            &cid,
            0.25,
            400.0,
            cur,
            st,
            "2026-06-01",
            now - 150 * DAY,
        ));
    }
    for i in 0..3 {
        let cid = format!("sharp-o{i}");
        open(&mut m, &cid);
        m.feed.push(trade(
            &sharp(),
            &cid,
            Side::Buy,
            800.0,
            0.25,
            now - 5000 - i * 100,
        ));
        rows.push(pos(
            &sharp(),
            &cid,
            0.25,
            800.0,
            0.3,
            PosStatus::Open,
            "2026-11-30",
            now - 5000,
        ));
    }
    put(&mut m, &sharp(), rows);

    // GAMBLER: 9/20 at 0.5, five open markets.
    m.stats.insert(
        gambler(),
        stats_pnl(&gambler(), now - 300 * DAY, 500, 60, -2000.0),
    );
    let mut rows = Vec::new();
    for i in 0..20 {
        let cid = format!("gam-r{i}");
        let (cur, st) = if i < 9 {
            (1.0, PosStatus::Redeemable)
        } else {
            (0.0, PosStatus::RedeemableLost)
        };
        rows.push(pos(
            &gambler(),
            &cid,
            0.5,
            400.0,
            cur,
            st,
            "2026-06-01",
            now - 150 * DAY,
        ));
    }
    for i in 0..5 {
        let cid = format!("gam-o{i}");
        open(&mut m, &cid);
        m.feed.push(trade(
            &gambler(),
            &cid,
            Side::Buy,
            400.0,
            0.5,
            now - 6000 - i * 100,
        ));
        rows.push(pos(
            &gambler(),
            &cid,
            0.5,
            400.0,
            0.5,
            PosStatus::Open,
            "2026-11-30",
            now - 6000,
        ));
    }
    put(&mut m, &gambler(), rows);

    // Noise that must never reach enrichment.
    let mut c = trade(
        &combo(),
        "0x038e536f830e26cc80c6b155230ea0a93c0000000000000000000000000000",
        Side::Buy,
        1e5,
        0.05,
        now - 100,
    );
    c.title.clear();
    m.feed.push(c);
    let mut u = trade(&updown(), "btc-15m", Side::Buy, 50_000.0, 0.1, now - 200);
    u.slug = "btc-updown-15m-1792582200".into();
    m.feed.push(u);
    let mut u = trade(&updown(), "eth-hourly", Side::Buy, 50_000.0, 0.1, now - 300);
    u.event_slug = "ethereum-up-or-down-october-21-7am-et".into();
    m.feed.push(u);
    m.feed.push(trade(
        &old(),
        "whale-m",
        Side::Buy,
        50_000.0,
        0.1,
        now - 5 * 3600,
    ));
    m.feed
        .push(trade(&gambler(), "gam-o0", Side::Sell, 10.0, 0.6, now - 60));

    // Resolved markets: r1 (longshot winner), r2 (crypto, skipped), r3 (favourite won),
    // r4 (sports longshot, skipped), r5 (longshot below the volume floor, skipped).
    let r1 = meta(
        "r1",
        now - 10 * DAY,
        now - DAY,
        5e5,
        MarketCategory::Politics,
        Some(0),
    );
    let r2 = meta(
        "r2",
        now - 30 * 3600,
        now - 3600,
        9e6,
        MarketCategory::Crypto,
        Some(0),
    );
    let r3 = meta(
        "r3",
        now - 5 * DAY,
        now - 2 * 3600,
        4e5,
        MarketCategory::Politics,
        Some(0),
    );
    let r4 = meta(
        "r4",
        now - 10 * DAY,
        now - DAY,
        8e5,
        MarketCategory::Sports,
        Some(0),
    );
    let r5 = meta(
        "r5",
        now - 10 * DAY,
        now - DAY,
        5_000.0,
        MarketCategory::Politics,
        Some(0),
    );
    m.closed = vec![r1, r2, r3, r4, r5];
    for cid in ["r4", "r5"] {
        m.prices.insert(
            format!("{cid}-yes"),
            vec![PricePoint {
                timestamp: now - 3 * DAY,
                price: 0.05,
            }],
        );
    }
    m.prices.insert(
        "r1-yes".into(),
        vec![
            PricePoint {
                timestamp: now - 6 * DAY,
                price: 0.08,
            },
            PricePoint {
                timestamp: now - 2 * DAY,
                price: 0.55,
            },
        ],
    );
    m.prices.insert(
        "r3-yes".into(),
        vec![PricePoint {
            timestamp: now - 3 * DAY,
            price: 0.6,
        }],
    );
    // Current holders (no status): the favourite buyer, the loser and a dust position.
    m.market_positions.insert(
        ("r1".into(), None),
        vec![
            market_row(&pro(), "r1-yes", 0.6, 50_000.0, PosStatus::Redeemable),
            market_row(&loser(), "r1-no", 0.05, 9_000.0, PosStatus::RedeemableLost),
            market_row(&dust(), "r1-yes", 0.05, 200.0, PosStatus::Redeemable),
        ],
    );
    // Redeemed / sold: HOLDER (twice: the lowest entry wins) and the loser again.
    m.market_positions.insert(
        ("r1".into(), Some(PosStatus::Closed)),
        vec![
            market_row(&holder(), "r1-yes", 0.2, 1_000.0, PosStatus::Closed),
            market_row(&loser(), "r1-no", 0.04, 3_000.0, PosStatus::Closed),
            market_row(&holder(), "r1-yes", 0.07, 10_000.0, PosStatus::Closed),
        ],
    );
    m.stats
        .insert(holder(), stats(&holder(), now - 200 * DAY, 50, 10));
    put(
        &mut m,
        &holder(),
        vec![pos(
            &holder(),
            "r1",
            0.07,
            10_000.0,
            1.0,
            PosStatus::Redeemable,
            "2026-10-20",
            now - 8 * DAY,
        )],
    );
    m.biggest = vec![BiggestWinnerRow {
        win_rank: 1,
        user_id: holder(),
        pnl: 9_300.0,
        initial_value: 700.0,
        final_value: 10_000.0,
        resolved_at: now - DAY,
        condition_id: "r1".into(),
        event_slug: "r1".into(),
        event_title: "r1".into(),
        user_name: String::new(),
    }];
    m.profiles.insert(
        holder(),
        Profile {
            name: "lucky".into(),
            ..Default::default()
        },
    );

    // LEADER: on the day board, nothing else.
    m.leaderboard = vec![LeaderboardRow {
        rank: 3,
        user_id: leader(),
        pnl: 50_000.0,
        volume: 1e6,
        user_name: String::new(),
        x_username: String::new(),
        verified: false,
    }];
    m.stats
        .insert(leader(), stats(&leader(), now - 20 * DAY, 10, 4));
    let rows = (0..4)
        .map(|i| {
            let cid = format!("lead-m{i}");
            open(&mut m, &cid);
            pos(
                &leader(),
                &cid,
                0.6,
                1000.0,
                0.7,
                PosStatus::Open,
                "2026-11-30",
                now - DAY,
            )
        })
        .collect();
    put(&mut m, &leader(), rows);
    m
}

fn opts(now: i64) -> DiscoverOpts {
    DiscoverOpts {
        now,
        ..Default::default()
    }
}

fn data_dir() -> (tempfile::TempDir, DataDir) {
    let t = tempfile::tempdir().unwrap();
    let d = DataDir::open(t.path()).unwrap();
    (t, d)
}

fn never_enriched() -> [Addr; 7] {
    [combo(), updown(), old(), pro(), loser(), dust(), owner()]
}

#[tokio::test]
async fn first_run_flags_planted_wallets_and_persists() {
    let api = scenario(NOW);
    let (_t, data) = data_dir();
    let mut assets = StateAssets::default();
    let cfg = Config::default();
    let rep = run(&api, &data, &mut assets, &cfg, &opts(NOW))
        .await
        .unwrap();

    // Noise never reached enrichment.
    for w in never_enriched() {
        assert_eq!(
            api.count("user_stats", w.as_str()),
            0,
            "{w} must not be enriched"
        );
    }

    // Watchlist: sharp, longshot holder, whale and its sibling; not the gambler or the leader.
    let wl = data.watchlist().unwrap();
    let on: HashMap<Addr, u8> = wl
        .entries
        .iter()
        .map(|e| (e.addr.clone(), e.score))
        .collect();
    for w in [sharp(), holder(), whale(), sib()] {
        assert!(
            on.contains_key(&w),
            "{w} should be watched; report: {:#?}",
            rep.top
        );
    }
    assert!(!on.contains_key(&gambler()));
    assert!(!on.contains_key(&leader()));
    let entry = |w: &Addr| wl.entries.iter().find(|e| &e.addr == w).unwrap();
    assert_eq!(entry(&sharp()).reason, SignalKind::OldAccountWinRate);
    assert_eq!(entry(&holder()).reason, SignalKind::LongshotWinner);
    assert_eq!(entry(&holder()).added_at, NOW);
    assert_eq!(entry(&holder()).last_qualified, NOW);

    // Wallet files with sensible scores.
    let sharp_w = data.load_wallet(&sharp()).unwrap().unwrap();
    assert!(
        (55..=85).contains(&sharp_w.score.total),
        "sharp score {}",
        sharp_w.score.total
    );
    assert!(sharp_w.resolved.z > 4.0);
    assert_eq!(sharp_w.resolved.wins, 12.0);
    assert!(
        sharp_w
            .positions
            .iter()
            .all(|p| p.market_duration_h.is_some()),
        "market fields filled"
    );
    let holder_w = data.load_wallet(&holder()).unwrap().unwrap();
    assert!(
        holder_w.score.total >= 60,
        "holder score {}",
        holder_w.score.total
    );
    assert!(holder_w
        .signals
        .iter()
        .any(|s| s.kind == SignalKind::LongshotWinner));
    assert!(
        holder_w.sources.contains(&Source::LongshotHolder)
            && holder_w.sources.contains(&Source::BiggestWinner)
    );
    assert_eq!(holder_w.profile.as_ref().unwrap().name, "lucky");
    assert!(data.load_wallet(&gambler()).unwrap().is_none());

    // Whale and sibling linked by the factory owner, with the cluster bonus.
    let whale_w = data.load_wallet(&whale()).unwrap().unwrap();
    let sib_w = data.load_wallet(&sib()).unwrap().unwrap();
    assert!(whale_w.cluster_id.is_some());
    assert_eq!(whale_w.cluster_id, sib_w.cluster_id);
    assert!(whale_w
        .signals
        .iter()
        .any(|s| s.kind == SignalKind::LinkedAccounts));
    assert!(whale_w
        .signals
        .iter()
        .any(|s| s.kind == SignalKind::NewAccountLongshot));
    assert_eq!(whale_w.profile.as_ref().unwrap().owner_eoa, Some(owner()));
    assert!(sib_w.sources.contains(&Source::NewWallet));
    assert!(
        whale_w.sources.contains(&Source::NewWallet) && whale_w.sources.contains(&Source::Feed)
    );
    let clusters = data.clusters().unwrap();
    assert_eq!(clusters.len(), 1);
    assert_eq!(clusters[0].members, vec![sib(), whale()]);

    // Cursors advanced.
    let cur = data.cursors().unwrap();
    assert_eq!(cur.feed_high_water_ts, NOW - 60);
    assert_eq!(cur.factory_last_block, HEAD);
    assert_eq!(cur.resolved_markets_since, NOW);
    assert_eq!(cur.last_discover, NOW);
    assert_eq!(
        api.args("factory_logs"),
        vec![format!("{}-{HEAD}", HEAD - 1800)]
    );
    assert_eq!(api.args("closed"), vec![(NOW - 2 * DAY).to_string()]);

    // Longshot source: r2/r4/r5 filtered, r3 not a longshot → only r1's positions read,
    // both statuses, following the cursor; `/v2/holders` is not used.
    let mut mp = api.args("market_positions");
    mp.sort();
    assert_eq!(
        mp,
        vec![
            "r1/None/-".to_string(),
            "r1/None/2".to_string(),
            "r1/Some(Closed)/-".to_string(),
            "r1/Some(Closed)/2".to_string(),
        ]
    );
    assert!(api.args("holders").is_empty());
    for cid in ["r2-yes", "r4-yes", "r5-yes"] {
        assert_eq!(api.count("prices", cid), 0, "{cid}");
    }
    let cache = data.markets_cache().unwrap();
    assert!(cache.markets.contains_key("r1") && cache.markets.contains_key("whale-m"));
    assert_eq!(cache.fetched_at.get("whale-m"), Some(&NOW));

    // State assets.
    for w in [whale(), sharp(), gambler(), holder(), leader(), sib()] {
        assert_eq!(
            assets.evaluated.entries.get(&w).map(|e| e.last_eval_ts),
            Some(NOW),
            "{w}"
        );
    }
    assert_eq!(
        assets
            .proxy_owner
            .entries
            .get(&sib())
            .map(|(o, _)| o.clone()),
        Some(owner())
    );

    // Report.
    assert_eq!(rep.feed_lag_s, 7);
    assert!(!rep.feed_truncated);
    assert_eq!(rep.feed_rows, 15);
    assert_eq!(rep.feed_pages, 4);
    assert_eq!(rep.prefiltered, 3);
    // The losing gambler is dropped by the user-stats gate before positions are fetched.
    assert_eq!(rep.enriched, 5);
    assert_eq!(rep.new_factory_wallets, 2);
    assert_eq!(rep.clusters, 1);
    assert_eq!(rep.watchlist_added, 4);
    assert_eq!(rep.watchlist_size, 4);
    let src = |k: &str| rep.sources.get(k).copied().unwrap_or(0);
    assert_eq!(
        (
            src("feed"),
            src("leaderboard"),
            src("biggest_winner"),
            src("longshot_holder"),
            src("new_wallet")
        ),
        (3, 1, 1, 1, 2)
    );
    assert_eq!(rep.flagged_medium + rep.flagged_high, 4);
    assert!(rep.top.len() >= 4 && rep.top.windows(2).all(|w| w[0].score >= w[1].score));
    assert!(rep.top.iter().filter(|r| r.score >= 45).all(|r| r.watched));
    assert!(rep.requests.requests.values().sum::<u64>() > 0);
}

#[tokio::test]
async fn dry_run_writes_nothing() {
    let api = scenario(NOW);
    let (t, data) = data_dir();
    let mut assets = StateAssets::default();
    let o = DiscoverOpts {
        dry_run: true,
        ..opts(NOW)
    };
    let rep = run(&api, &data, &mut assets, &Config::default(), &o)
        .await
        .unwrap();
    assert_eq!(rep.watchlist_added, 4);
    assert!(!rep.top.is_empty());
    let root: Vec<_> = std::fs::read_dir(t.path())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(root, vec![std::ffi::OsString::from("wallets")]);
    assert_eq!(
        std::fs::read_dir(t.path().join("wallets")).unwrap().count(),
        0
    );
}

#[tokio::test]
async fn second_run_respects_ttl_and_cursor() {
    let (_t, data) = data_dir();
    let mut assets = StateAssets::default();
    let cfg = Config::default();
    let first = scenario(NOW);
    run(&first, &data, &mut assets, &cfg, &opts(NOW))
        .await
        .unwrap();

    let later = NOW + 4 * 3600;
    let second = scenario(later);
    let rep = run(&second, &data, &mut assets, &cfg, &opts(later))
        .await
        .unwrap();
    // Evaluated recently and not watched → skipped.
    assert_eq!(second.count("user_stats", gambler().as_str()), 0);
    assert_eq!(second.count("user_stats", leader().as_str()), 0);
    // Watchlist members are always re-scored; the longshot holder too.
    for w in [sharp(), whale(), holder()] {
        assert_eq!(second.count("user_stats", w.as_str()), 1, "{w}");
    }
    // Sib is on the watchlist now: re-enriched once as a watchlist member, not again as a sibling.
    assert_eq!(second.count("user_stats", sib().as_str()), 1);

    // The feed stopped at the old high-water mark: OLD's row (later − 5 h) is not reached.
    let cur = data.cursors().unwrap();
    assert_eq!(cur.feed_high_water_ts, later - 60);
    assert_eq!(cur.factory_last_block, HEAD, "nothing new on chain");
    assert_eq!(second.args("closed"), vec![NOW.to_string()]);
    assert_eq!(rep.watchlist_added, 0);
    let wl = data.watchlist().unwrap();
    assert_eq!(wl.entries.len(), 4);
    assert!(wl
        .entries
        .iter()
        .all(|e| e.added_at == NOW && e.last_qualified == later));
}

#[tokio::test]
async fn shards_partition_the_candidates() {
    let mut seen: Vec<HashSet<String>> = Vec::new();
    for i in 0..2 {
        let api = scenario(NOW);
        let (_t, data) = data_dir();
        let mut assets = StateAssets::default();
        let o = DiscoverOpts {
            dry_run: true,
            shard: Some((i, 2)),
            ..opts(NOW)
        };
        run(&api, &data, &mut assets, &Config::default(), &o)
            .await
            .unwrap();
        let asked: HashSet<String> = api.args("user_stats").into_iter().collect();
        for w in &asked {
            let w = Addr::parse(w).unwrap();
            // Siblings follow their flagged owner's shard, everything else is sharded by address.
            assert!(
                w == sib() || in_shard(&w, Some((i, 2))),
                "{w} not in shard {i}"
            );
        }
        seen.push(asked);
    }
    let candidates: HashSet<String> = [whale(), sharp(), gambler(), holder(), leader()]
        .iter()
        .map(|a| a.to_string())
        .collect();
    let union: HashSet<String> = seen[0]
        .union(&seen[1])
        .filter(|a| **a != sib().to_string())
        .cloned()
        .collect();
    assert_eq!(union, candidates);
    assert!(seen[0]
        .intersection(&seen[1])
        .all(|a| *a == sib().to_string()));
}

#[tokio::test]
async fn feed_truncates_at_page_cap() {
    let api = scenario(NOW);
    let (_t, data) = data_dir();
    let mut cfg = Config::default();
    cfg.discovery.feed_max_pages = 2;
    let rep = run(&api, &data, &mut StateAssets::default(), &cfg, &opts(NOW))
        .await
        .unwrap();
    assert!(rep.feed_truncated);
    assert_eq!(rep.feed_pages, 2);
    assert_eq!(api.args("trades"), vec!["0".to_string(), "1".to_string()]);
    // The high-water mark is still the newest row seen.
    assert_eq!(data.cursors().unwrap().feed_high_water_ts, NOW - 60);
}

#[tokio::test]
async fn failing_calls_degrade_gracefully() {
    let mut api = scenario(NOW);
    api.fail_stats.insert(gambler());
    api.fail_positions
        .insert((sharp(), PosStatus::RedeemableLost));
    api.fail_profiles = true;
    api.fail_leaderboard = true;
    api.fail_rpc = true;
    api.fail_status = true;
    let (_t, data) = data_dir();
    let mut assets = StateAssets::default();
    let rep = run(&api, &data, &mut assets, &Config::default(), &opts(NOW))
        .await
        .unwrap();

    // Failed wallets are not marked evaluated (retried next run) and not saved.
    assert!(!assets.evaluated.entries.contains_key(&gambler()));
    assert!(!assets.evaluated.entries.contains_key(&sharp()));
    assert!(data.load_wallet(&sharp()).unwrap().is_none());
    // Everything else still went through.
    let wl = data.watchlist().unwrap();
    assert!(wl.entries.iter().any(|e| e.addr == holder()));
    assert!(wl.entries.iter().any(|e| e.addr == whale()));
    assert_eq!(rep.feed_lag_s, 0);
    assert_eq!(rep.sources.get("leaderboard"), None);
    assert_eq!(rep.new_factory_wallets, 0);
    let cur = data.cursors().unwrap();
    assert_eq!(
        cur.factory_last_block, 0,
        "factory cursor unchanged when RPC fails"
    );
    assert_eq!(cur.feed_high_water_ts, NOW - 60);
    // Holder profile failed: no name, but the wallet is still flagged.
    let h = data.load_wallet(&holder()).unwrap().unwrap();
    assert!(h.profile.is_none());
}

#[tokio::test]
async fn first_feed_page_failure_is_fatal() {
    let mut api = scenario(NOW);
    api.fail_feed = true;
    let (t, data) = data_dir();
    let res = run(
        &api,
        &data,
        &mut StateAssets::default(),
        &Config::default(),
        &opts(NOW),
    )
    .await;
    assert!(res.is_err());
    assert_eq!(
        std::fs::read_dir(t.path().join("wallets")).unwrap().count(),
        0
    );
    assert!(data.cursors().unwrap() == Default::default());
}

#[tokio::test]
async fn seeds_bypass_ttl_and_gate() {
    let api = scenario(NOW);
    let (_t, data) = data_dir();
    let mut assets = StateAssets::default();
    // Seed is unknown to user-stats and was evaluated an hour ago: still enriched.
    let seed = a('9');
    assets.evaluated.entries.insert(
        seed.clone(),
        types::state::Evaluated {
            last_eval_ts: NOW - 3600,
            last_score: 0,
        },
    );
    let o = DiscoverOpts {
        seed: vec![seed.clone()],
        ..opts(NOW)
    };
    let rep = run(&api, &data, &mut assets, &Config::default(), &o)
        .await
        .unwrap();
    assert_eq!(api.count("user_stats", seed.as_str()), 1);
    assert_eq!(api.count("positions", seed.as_str()), 4);
    assert_eq!(rep.sources.get("seed"), Some(&1));
    assert_eq!(assets.evaluated.entries[&seed].last_eval_ts, NOW);
}
