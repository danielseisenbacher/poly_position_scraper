use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use async_trait::async_trait;
use types::api::*;
use types::state::{Cursors, Watchlist};
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

fn entry(a: Addr, score: u8, last_activity_ts: i64, last_polled: i64) -> WatchlistEntry {
    WatchlistEntry {
        addr: a,
        added_at: NOW - 3 * DAY,
        reason: SignalKind::OldAccountWinRate,
        score,
        last_activity_ts,
        last_polled,
        last_qualified: NOW - DAY,
        issue: None,
    }
}

fn row(user: &Addr, ts: i64, kind: ActivityType, side: Option<Side>, usdc: f64) -> ActivityRow {
    ActivityRow {
        proxy_wallet: user.clone(),
        timestamp: ts,
        condition_id: "0xcond".into(),
        kind,
        size: usdc / 0.25,
        usdc_size: usdc,
        transaction_hash: format!("0xtx{ts}"),
        price: 0.25,
        token_id: "tok".into(),
        side,
        outcome_index: 0,
        title: "Will it happen?".into(),
        slug: "will-it-happen".into(),
        outcome: "Yes".into(),
        is_combo: false,
    }
}

fn buy(user: &Addr, ts: i64, usdc: f64) -> ActivityRow {
    row(user, ts, ActivityType::Trade, Some(Side::Buy), usdc)
}

fn pos_row(user: &Addr, token: &str, status: PosStatus) -> PositionRow {
    serde_json::from_value(serde_json::json!({
        "proxy_wallet": user,
        "token_id": token,
        "condition_id": format!("0x{token}"),
        "current_size": 100.0,
        "avg_price": 0.2,
        "total_size": 100.0,
        "current_price": 0.3,
        "current_value": 30.0,
        "status": status,
        "title": format!("Market {token}"),
    }))
    .unwrap()
}

/// Simulates `/v2/activity`: rows filtered by `start`, newest first, paged by `page_size`
/// with the offset as cursor.
#[derive(Default)]
struct Mock {
    activity: HashMap<Addr, Vec<ActivityRow>>,
    page_size: usize,
    open: HashMap<Addr, Vec<PositionRow>>,
    values: HashMap<Addr, f64>,
    fail_activity: HashSet<Addr>,
    fail_positions: bool,
    sleep: Duration,
    queries: Mutex<Vec<(ActivityQuery, Option<String>)>>,
}

impl Mock {
    fn new() -> Self {
        Mock {
            page_size: 1000,
            ..Default::default()
        }
    }

    fn polled_users(&self) -> Vec<Addr> {
        let q = self.queries.lock().unwrap();
        let mut out: Vec<Addr> = Vec::new();
        for (q, _) in q.iter() {
            if !out.contains(&q.user) {
                out.push(q.user.clone());
            }
        }
        out
    }
}

#[async_trait]
impl PolyApi for Mock {
    async fn trades_page(&self, _: &TradesQuery, _: Option<&str>) -> ApiResult<Page<FeedTrade>> {
        unimplemented!()
    }
    async fn user_trades(
        &self,
        _: &Addr,
        _: Option<i64>,
        _: Option<&str>,
    ) -> ApiResult<Page<FeedTrade>> {
        unimplemented!()
    }
    async fn activity(
        &self,
        q: &ActivityQuery,
        cursor: Option<&str>,
    ) -> ApiResult<Page<ActivityRow>> {
        self.queries
            .lock()
            .unwrap()
            .push((q.clone(), cursor.map(String::from)));
        if !self.sleep.is_zero() {
            tokio::time::sleep(self.sleep).await;
        }
        if self.fail_activity.contains(&q.user) {
            return Err(ApiError::Http {
                status: 500,
                url: "activity".into(),
                body: String::new(),
            });
        }
        let mut rows: Vec<ActivityRow> = self
            .activity
            .get(&q.user)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| q.start.is_none_or(|s| r.timestamp >= s))
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.timestamp));
        let off: usize = cursor.map(|c| c.parse().unwrap()).unwrap_or(0);
        let size = self.page_size.min(q.limit as usize);
        let data: Vec<_> = rows.iter().skip(off).take(size).cloned().collect();
        let next_cursor = (off + size < rows.len()).then(|| (off + size).to_string());
        Ok(Page { data, next_cursor })
    }
    async fn positions(
        &self,
        user: &Addr,
        status: PosStatus,
        _: Option<&str>,
    ) -> ApiResult<Page<PositionRow>> {
        assert_eq!(status, PosStatus::Open);
        if self.fail_positions {
            return Err(ApiError::Transport {
                url: "positions".into(),
                msg: "boom".into(),
            });
        }
        Ok(Page {
            data: self.open.get(user).cloned().unwrap_or_default(),
            next_cursor: None,
        })
    }
    async fn market_positions(
        &self,
        _: &str,
        _: Option<PosStatus>,
        _: Option<i64>,
        _: Option<&str>,
    ) -> ApiResult<Page<PositionRow>> {
        unimplemented!()
    }
    async fn holders(&self, _: &str, _: bool, _: u32) -> ApiResult<Vec<HolderGroup>> {
        unimplemented!()
    }
    async fn user_stats(&self, _: &Addr) -> ApiResult<Option<UserStatsRow>> {
        unimplemented!()
    }
    async fn value(&self, user: &Addr) -> ApiResult<f64> {
        if self.fail_positions {
            return Err(ApiError::Transport {
                url: "value".into(),
                msg: "boom".into(),
            });
        }
        Ok(self.values.get(user).copied().unwrap_or(0.0))
    }
    async fn leaderboard(
        &self,
        _: TimePeriod,
        _: LeaderboardSort,
        _: Option<&str>,
    ) -> ApiResult<Page<LeaderboardRow>> {
        unimplemented!()
    }
    async fn biggest_winners(
        &self,
        _: TimePeriod,
        _: Option<&str>,
    ) -> ApiResult<Page<BiggestWinnerRow>> {
        unimplemented!()
    }
    async fn tokens(&self, _: &str) -> ApiResult<Vec<TokenRow>> {
        unimplemented!()
    }
    async fn prices_history(&self, _: &str, _: i64, _: i64) -> ApiResult<Vec<PricePoint>> {
        unimplemented!()
    }
    async fn status(&self) -> ApiResult<ApiStatus> {
        unimplemented!()
    }
    async fn recently_closed_markets(
        &self,
        _: i64,
        _: f64,
        _: usize,
    ) -> ApiResult<Vec<MarketMeta>> {
        unimplemented!()
    }
    async fn markets_by_condition(&self, _: &[String]) -> ApiResult<Vec<MarketMeta>> {
        unimplemented!()
    }
    async fn profile(&self, _: &Addr) -> ApiResult<Option<Profile>> {
        unimplemented!()
    }
    async fn block_number(&self) -> ApiResult<u64> {
        unimplemented!()
    }
    async fn factory_logs(&self, _: u64, _: u64) -> ApiResult<Vec<FactoryLog>> {
        unimplemented!()
    }
    fn stats(&self) -> RequestStats {
        let mut s = RequestStats::default();
        s.requests.insert(
            RateClass::Positions,
            self.queries.lock().unwrap().len() as u64,
        );
        s
    }
}

fn opts() -> TrackOpts {
    TrackOpts {
        dry_run: false,
        now: NOW,
    }
}

fn save_watchlist(d: &DataDir, entries: Vec<WatchlistEntry>) {
    d.save_watchlist(&Watchlist { entries }).unwrap();
}

fn find(d: &DataDir, a: &Addr) -> WatchlistEntry {
    d.watchlist()
        .unwrap()
        .entries
        .into_iter()
        .find(|e| &e.addr == a)
        .unwrap()
}

fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p.clone());
                out.insert(p.display().to_string(), Vec::new());
            } else {
                out.insert(p.display().to_string(), std::fs::read(&p).unwrap());
            }
        }
    }
    out
}

// ---- pure functions ----

#[test]
fn from_activity_maps_kinds() {
    let u = addr(1);
    let k = |kind, side| from_activity(&row(&u, NOW, kind, side, 10.0)).map(|t| t.kind);
    assert_eq!(
        k(ActivityType::Trade, Some(Side::Buy)),
        Some(TradeKind::Buy)
    );
    assert_eq!(
        k(ActivityType::Trade, Some(Side::Sell)),
        Some(TradeKind::Sell)
    );
    assert_eq!(k(ActivityType::Trade, None), None);
    assert_eq!(k(ActivityType::Redeem, None), Some(TradeKind::Redeem));
    assert_eq!(k(ActivityType::Split, None), Some(TradeKind::Split));
    assert_eq!(k(ActivityType::Merge, None), Some(TradeKind::Merge));
    assert_eq!(k(ActivityType::Deposit, None), Some(TradeKind::Deposit));
    assert_eq!(k(ActivityType::Withdrawal, None), Some(TradeKind::Withdraw));
    assert_eq!(k(ActivityType::Reward, None), None);
    assert_eq!(k(ActivityType::Conversion, None), None);
    assert_eq!(k(ActivityType::Other, None), None);
}

#[test]
fn from_activity_usdc_fallback_and_combo_title() {
    let u = addr(1);
    let mut r = buy(&u, NOW, 0.0);
    r.size = 40.0;
    r.price = 0.25;
    r.title = String::new();
    let t = from_activity(&r).unwrap();
    assert_eq!(t.usdc, 10.0);
    assert_eq!(t.title, "");
    r.is_combo = true;
    let t = from_activity(&r).unwrap();
    assert_eq!(t.title, "(combo)");
    assert_eq!(
        t.key,
        trade_key(&r.transaction_hash, &r.token_id, TradeKind::Buy, 40.0, NOW)
    );
    assert_eq!(t.tx, r.transaction_hash);
    assert_eq!(t.outcome, "Yes");
}

#[test]
fn from_activity_recorded_fixture() {
    let p = format!(
        "{}/../../fixtures/live/activity_user.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap();
    let rows: Vec<ActivityRow> = serde_json::from_value(v["data"].clone()).unwrap();
    let trades: Vec<TrackedTrade> = rows.iter().filter_map(from_activity).collect();
    assert_eq!(trades.len(), rows.len());
    for (r, t) in rows.iter().zip(&trades) {
        assert_eq!(t.ts, r.timestamp);
        assert_eq!(t.size, r.size);
        assert!(t.usdc > 0.0 || r.kind == ActivityType::Redeem);
        assert!(!t.title.is_empty());
        assert_eq!(t.key.len(), 64);
    }
    assert_eq!(
        trades.iter().filter(|t| t.kind == TradeKind::Buy).count(),
        4
    );
    assert_eq!(
        trades
            .iter()
            .filter(|t| t.kind == TradeKind::Redeem)
            .count(),
        1
    );
    let first = &trades[0];
    assert_eq!(first.usdc, 5.999998);
    assert_eq!(
        first.tx,
        "0x6cddebfa49e387d4f6c35b3b7a6c79ca47cc849bd8012ee5fea4508d7fbb8226"
    );
}

#[test]
fn trade_key_is_stable() {
    let k = trade_key("0xabc", "123", TradeKind::Buy, 1.5, 1_700_000_000);
    // sha256("0xabc|123|Buy|1.500000|1700000000")
    let expect = hex::encode(Sha256::digest(b"0xabc|123|Buy|1.500000|1700000000"));
    assert_eq!(k, expect);
    assert_eq!(k.len(), 64);
    assert_eq!(
        k,
        trade_key("0xabc", "123", TradeKind::Buy, 1.5000001, 1_700_000_000)
    );
    assert_ne!(
        k,
        trade_key("0xabc", "123", TradeKind::Sell, 1.5, 1_700_000_000)
    );
    assert_ne!(
        k,
        trade_key("0xabc", "123", TradeKind::Buy, 1.5, 1_700_000_001)
    );
}

#[test]
fn eviction_rules() {
    let cfg = Config::default();
    let a = addr(1);
    let ok = WatchlistEntry {
        last_activity_ts: NOW - DAY,
        ..entry(a, 60, 0, 0)
    };
    assert!(!should_evict(&ok, &cfg, NOW));
    // Low score for > 14 d.
    let low = WatchlistEntry {
        added_at: NOW - 30 * DAY,
        last_qualified: NOW - 15 * DAY,
        ..ok.clone()
    };
    assert!(should_evict(&low, &cfg, NOW));
    // Recently added: added_at restarts the low-score clock.
    let fresh = WatchlistEntry {
        added_at: NOW - DAY,
        last_qualified: 0,
        last_activity_ts: 0,
        ..ok.clone()
    };
    assert!(!should_evict(&fresh, &cfg, NOW));
    // Inactive for > 60 d.
    let idle = WatchlistEntry {
        added_at: NOW - 90 * DAY,
        last_activity_ts: NOW - 61 * DAY,
        ..ok.clone()
    };
    assert!(should_evict(&idle, &cfg, NOW));
    let active = WatchlistEntry {
        added_at: NOW - 90 * DAY,
        last_activity_ts: NOW - 59 * DAY,
        ..ok
    };
    assert!(!should_evict(&active, &cfg, NOW));
}

#[test]
fn poll_order_score_then_stalest_then_addr() {
    let es = vec![
        entry(addr(1), 50, 0, 100),
        entry(addr(2), 80, 0, 300),
        entry(addr(3), 80, 0, 200),
        entry(addr(5), 50, 0, 100),
        entry(addr(4), 50, 0, 100),
    ];
    assert_eq!(poll_order(&es), vec![2, 1, 0, 4, 3]);
}

#[test]
fn big_trade_reasons() {
    let cfg = Config::default();
    let t = |kind, usdc| TrackedTrade {
        key: "k".into(),
        ts: NOW,
        kind,
        condition_id: "c".into(),
        token_id: "t".into(),
        title: "T".into(),
        outcome: "Yes".into(),
        price: 0.5,
        size: usdc * 2.0,
        usdc,
        tx: "0x".into(),
    };
    assert_eq!(
        big_trade_reason(&t(TradeKind::Buy, 500.0), 0.0, &cfg).as_deref(),
        Some("≥ $500")
    );
    assert_eq!(
        big_trade_reason(&t(TradeKind::Redeem, 900.0), 0.0, &cfg).as_deref(),
        Some("≥ $500")
    );
    assert_eq!(
        big_trade_reason(&t(TradeKind::Merge, 900.0), 0.0, &cfg),
        None
    );
    assert_eq!(big_trade_reason(&t(TradeKind::Buy, 499.0), 0.0, &cfg), None);
    assert_eq!(
        big_trade_reason(&t(TradeKind::Sell, 80.0), 250.0, &cfg).as_deref(),
        Some("32 % of wallet value")
    );
    assert_eq!(
        big_trade_reason(&t(TradeKind::Sell, 60.0), 250.0, &cfg),
        None
    );
    // Share rule is Buy/Sell only.
    assert_eq!(
        big_trade_reason(&t(TradeKind::Redeem, 80.0), 250.0, &cfg),
        None
    );
}

// ---- run ----

#[tokio::test]
async fn first_poll_starts_a_week_before_added_and_records_trades() {
    let (_t, d) = tmp();
    let a = addr(1);
    let e = entry(a.clone(), 60, 0, 0);
    let added = e.added_at;
    save_watchlist(&d, vec![e]);
    let mut api = Mock::new();
    api.activity.insert(
        a.clone(),
        vec![
            buy(&a, NOW - 11 * DAY, 5.0), // before start: filtered by the API
            buy(&a, NOW - 9 * DAY, 5.0),
            buy(&a, NOW - 2 * DAY, 7.0),
            row(&a, NOW - DAY, ActivityType::Reward, None, 1.0),
        ],
    );
    let rep = run(&api, &d, &Config::default(), &opts()).await.unwrap();

    let (q, cursor) = api.queries.lock().unwrap()[0].clone();
    assert_eq!(q.start, Some(added - 7 * DAY));
    assert_eq!(q.end, None);
    assert_eq!(q.limit, 1000);
    assert_eq!(cursor, None);

    assert_eq!(rep.polled, 1);
    assert_eq!(rep.new_trades, 2);
    assert_eq!(rep.archive.len(), 2);
    assert!(rep.archive.iter().all(|(x, _)| x == &a));
    assert_eq!(rep.requests.requests[&RateClass::Positions], 1);
    let w = d.load_wallet(&a).unwrap().unwrap();
    assert_eq!(w.first_seen, NOW);
    assert_eq!(
        w.trades.iter().map(|t| t.ts).collect::<Vec<_>>(),
        vec![NOW - 2 * DAY, NOW - 9 * DAY]
    );
    let e = find(&d, &a);
    // Reward rows are not tracked but still advance the cursor.
    assert_eq!(e.last_activity_ts, NOW - DAY);
    assert_eq!(e.last_polled, NOW);
    assert_eq!(d.cursors().unwrap().last_track, NOW);
}

#[tokio::test]
async fn second_run_dedups_and_follows_pages() {
    let (_t, d) = tmp();
    let a = addr(1);
    save_watchlist(&d, vec![entry(a.clone(), 60, NOW - 5 * DAY, NOW - 600)]);
    let mut api = Mock::new();
    api.page_size = 2;
    let rows: Vec<_> = (1..=5)
        .map(|i| buy(&a, NOW - 5 * DAY + i * 60, 10.0))
        .collect();
    api.activity.insert(a.clone(), rows);
    let cfg = Config::default();

    let rep = run(&api, &d, &cfg, &opts()).await.unwrap();
    assert_eq!(rep.new_trades, 5);
    let qs = api.queries.lock().unwrap().clone();
    assert_eq!(qs.len(), 3);
    assert_eq!(qs[0].0.start, Some(NOW - 5 * DAY + 1 - POLL_OVERLAP_S));
    assert_eq!(
        qs.iter().map(|(_, c)| c.clone()).collect::<Vec<_>>(),
        vec![None, Some("2".into()), Some("4".into())]
    );
    let w1 = d.load_wallet(&a).unwrap().unwrap();
    let ts: Vec<i64> = w1.trades.iter().map(|t| t.ts).collect();
    assert!(ts.windows(2).all(|w| w[0] > w[1]), "newest first: {ts:?}");
    assert_eq!(find(&d, &a).last_activity_ts, NOW - 5 * DAY + 300);

    // Same API state: nothing new (cursor moved past everything).
    let rep = run(&api, &d, &cfg, &opts()).await.unwrap();
    assert_eq!(rep.polled, 1);
    assert_eq!(rep.new_trades, 0);
    assert!(rep.archive.is_empty());
    assert_eq!(d.load_wallet(&a).unwrap().unwrap(), w1);

    // Even if the cursor is reset, key dedup prevents duplicates.
    save_watchlist(&d, vec![entry(a.clone(), 60, NOW - 5 * DAY, NOW - 600)]);
    let rep = run(&api, &d, &cfg, &opts()).await.unwrap();
    assert_eq!(rep.new_trades, 0);
    assert_eq!(d.load_wallet(&a).unwrap().unwrap().trades.len(), 5);
}

#[tokio::test]
async fn prepends_and_caps_trades() {
    let (_t, d) = tmp();
    let a = addr(1);
    save_watchlist(&d, vec![entry(a.clone(), 60, NOW - 5 * DAY, NOW - 600)]);
    let mut cfg = Config::default();
    cfg.tracker.max_trades_per_wallet = 3;
    let mut api = Mock::new();
    api.activity.insert(
        a.clone(),
        vec![buy(&a, NOW - 4 * DAY, 1.0), buy(&a, NOW - 3 * DAY, 1.0)],
    );
    run(&api, &d, &cfg, &opts()).await.unwrap();
    api.activity
        .get_mut(&a)
        .unwrap()
        .extend([buy(&a, NOW - 2 * DAY, 1.0), buy(&a, NOW - DAY, 1.0)]);
    let rep = run(&api, &d, &cfg, &opts()).await.unwrap();
    assert_eq!(rep.new_trades, 2);
    let ts: Vec<i64> = d
        .load_wallet(&a)
        .unwrap()
        .unwrap()
        .trades
        .iter()
        .map(|t| t.ts)
        .collect();
    assert_eq!(ts, vec![NOW - DAY, NOW - 2 * DAY, NOW - 3 * DAY]);
}

#[tokio::test]
async fn page_cap_advances_to_newest() {
    let (_t, d) = tmp();
    let a = addr(1);
    save_watchlist(&d, vec![entry(a.clone(), 60, NOW - 5 * DAY, NOW - 600)]);
    let mut api = Mock::new();
    api.page_size = 1;
    let rows: Vec<_> = (1..=15).map(|i| buy(&a, NOW - DAY + i, 1.0)).collect();
    api.activity.insert(a.clone(), rows);
    let rep = run(&api, &d, &Config::default(), &opts()).await.unwrap();
    assert_eq!(api.queries.lock().unwrap().len(), MAX_ACTIVITY_PAGES);
    assert_eq!(rep.new_trades, MAX_ACTIVITY_PAGES as u32);
    assert_eq!(find(&d, &a).last_activity_ts, NOW - DAY + 15);
}

#[tokio::test]
async fn evicts_and_reports() {
    let (_t, d) = tmp();
    let keep = addr(1);
    let low = addr(2);
    let idle = addr(3);
    save_watchlist(
        &d,
        vec![
            entry(keep.clone(), 60, NOW - DAY, 0),
            WatchlistEntry {
                added_at: NOW - 30 * DAY,
                last_qualified: NOW - 20 * DAY,
                ..entry(low.clone(), 30, NOW - DAY, 0)
            },
            WatchlistEntry {
                added_at: NOW - 90 * DAY,
                ..entry(idle.clone(), 60, NOW - 70 * DAY, 0)
            },
        ],
    );
    let api = Mock::new();
    let rep = run(&api, &d, &Config::default(), &opts()).await.unwrap();
    assert_eq!(rep.evicted, vec![low, idle]);
    assert_eq!(rep.watchlist_size, 1);
    assert_eq!(rep.polled, 1);
    assert_eq!(api.polled_users(), vec![keep.clone()]);
    let wl = d.watchlist().unwrap();
    assert_eq!(
        wl.entries
            .iter()
            .map(|e| e.addr.clone())
            .collect::<Vec<_>>(),
        vec![keep]
    );
}

#[tokio::test]
async fn time_budget_defers_lowest_priority() {
    let (_t, d) = tmp();
    // Scores 10..=50 (all fresh, so nothing is evicted).
    let entries: Vec<_> = (1..=5)
        .map(|i| entry(addr(i), i * 10, NOW - DAY, NOW - 600))
        .collect();
    save_watchlist(&d, entries);
    let mut api = Mock::new();
    api.sleep = Duration::from_millis(400);
    for i in 1..=5 {
        api.activity
            .insert(addr(i), vec![buy(&addr(i), NOW - 100, 1.0)]);
    }
    let mut cfg = Config::default();
    cfg.api.max_concurrency = 1;
    cfg.tracker.time_budget_s = 1;
    let rep = run(&api, &d, &cfg, &opts()).await.unwrap();
    // Polls start at ≈0, 0.4, 0.8 s; the 4th would start at 1.2 s > budget.
    assert_eq!(rep.polled, 3);
    assert_eq!(rep.deferred, 2);
    assert_eq!(api.polled_users(), vec![addr(5), addr(4), addr(3)]);
    // Deferred entries keep their cursor and poll time.
    for i in [1, 2] {
        let e = find(&d, &addr(i));
        assert_eq!((e.last_activity_ts, e.last_polled), (NOW - DAY, NOW - 600));
        assert!(d.load_wallet(&addr(i)).unwrap().is_none());
    }
    assert_eq!(find(&d, &addr(5)).last_polled, NOW);
}

#[tokio::test]
async fn zero_budget_defers_everything() {
    let (_t, d) = tmp();
    save_watchlist(
        &d,
        vec![
            entry(addr(1), 60, NOW - DAY, 0),
            entry(addr(2), 60, NOW - DAY, 0),
        ],
    );
    let api = Mock::new();
    let mut cfg = Config::default();
    cfg.tracker.time_budget_s = 0;
    let rep = run(&api, &d, &cfg, &opts()).await.unwrap();
    assert_eq!((rep.polled, rep.deferred), (0, 2));
    assert!(api.queries.lock().unwrap().is_empty());
}

#[tokio::test]
async fn one_failing_wallet_does_not_stop_the_run() {
    let (_t, d) = tmp();
    let bad = addr(1);
    let good = addr(2);
    save_watchlist(
        &d,
        vec![
            entry(bad.clone(), 90, NOW - DAY, NOW - 600),
            entry(good.clone(), 60, NOW - DAY, NOW - 600),
        ],
    );
    let mut api = Mock::new();
    api.fail_activity.insert(bad.clone());
    api.activity
        .insert(good.clone(), vec![buy(&good, NOW - 100, 1.0)]);
    let rep = run(&api, &d, &Config::default(), &opts()).await.unwrap();
    assert_eq!(rep.polled, 1);
    assert_eq!(rep.new_trades, 1);
    let e = find(&d, &bad);
    assert_eq!((e.last_activity_ts, e.last_polled), (NOW - DAY, NOW - 600));
    assert!(d.load_wallet(&bad).unwrap().is_none());
    assert_eq!(find(&d, &good).last_activity_ts, NOW - 100);
}

#[tokio::test]
async fn big_trade_events_absolute_and_share() {
    let (_t, d) = tmp();
    let a = addr(1);
    let mut e = entry(a.clone(), 60, NOW - DAY, NOW - 600);
    e.issue = None;
    save_watchlist(&d, vec![e]);
    let mut w = Wallet::new(a.clone(), NOW - 10 * DAY);
    w.issue = Some(42);
    d.save_wallet(&w).unwrap();

    let mut api = Mock::new();
    api.values.insert(a.clone(), 200.0);
    api.activity.insert(
        a.clone(),
        vec![
            buy(&a, NOW - 500, 600.0),                                       // absolute
            row(&a, NOW - 400, ActivityType::Trade, Some(Side::Sell), 64.0), // 32 % of 200
            buy(&a, NOW - 300, 20.0),                                        // 10 %: none
            row(&a, NOW - 200, ActivityType::Redeem, None, 100.0), // redeem: absolute only
            row(&a, NOW - 100, ActivityType::Merge, None, 5000.0), // merge: never
        ],
    );
    let rep = run(&api, &d, &Config::default(), &opts()).await.unwrap();
    assert_eq!(rep.new_trades, 5);
    let ev: Vec<(i64, &str)> = rep
        .events
        .iter()
        .map(|e| (e.trade.ts, e.reason.as_str()))
        .collect();
    assert_eq!(
        ev,
        vec![(NOW - 400, "32 % of wallet value"), (NOW - 500, "≥ $500")]
    );
    assert!(rep
        .events
        .iter()
        .all(|e| e.issue == Some(42) && e.wallet_value == 200.0 && e.addr == a));
    assert_eq!(d.load_wallet(&a).unwrap().unwrap().value, 200.0);

    // Entry issue takes precedence over the wallet's.
    let mut wl = d.watchlist().unwrap();
    wl.entries[0].issue = Some(7);
    d.save_watchlist(&wl).unwrap();
    api.activity
        .get_mut(&a)
        .unwrap()
        .push(buy(&a, NOW - 50, 1000.0));
    let rep = run(&api, &d, &Config::default(), &opts()).await.unwrap();
    assert_eq!(rep.events.len(), 1);
    assert_eq!(rep.events[0].issue, Some(7));
}

#[tokio::test]
async fn refresh_replaces_only_open_positions() {
    let (_t, d) = tmp();
    let a = addr(1);
    save_watchlist(&d, vec![entry(a.clone(), 60, NOW - DAY, NOW - 600)]);
    let mut w = Wallet::new(a.clone(), NOW - 10 * DAY);
    w.positions = vec![
        pos_row(&a, "old-open", PosStatus::Open).into(),
        pos_row(&a, "closed", PosStatus::Closed).into(),
        pos_row(&a, "won", PosStatus::Redeemable).into(),
    ];
    w.value = 5.0;
    d.save_wallet(&w).unwrap();

    let mut api = Mock::new();
    api.activity
        .insert(a.clone(), vec![buy(&a, NOW - 100, 1.0)]);
    api.open.insert(
        a.clone(),
        vec![
            pos_row(&a, "new-open-1", PosStatus::Open),
            pos_row(&a, "new-open-2", PosStatus::Open),
        ],
    );
    api.values.insert(a.clone(), 60.0);
    run(&api, &d, &Config::default(), &opts()).await.unwrap();

    let w = d.load_wallet(&a).unwrap().unwrap();
    let mut toks: Vec<(&str, PosStatus)> = w
        .positions
        .iter()
        .map(|p| (p.token_id.as_str(), p.status))
        .collect();
    toks.sort();
    assert_eq!(
        toks,
        vec![
            ("closed", PosStatus::Closed),
            ("new-open-1", PosStatus::Open),
            ("new-open-2", PosStatus::Open),
            ("won", PosStatus::Redeemable),
        ]
    );
    assert_eq!(w.value, 60.0);
}

#[tokio::test]
async fn refresh_errors_keep_snapshot_and_no_refresh_without_trades() {
    let (_t, d) = tmp();
    let a = addr(1);
    save_watchlist(&d, vec![entry(a.clone(), 60, NOW - DAY, NOW - 600)]);
    let mut w = Wallet::new(a.clone(), NOW - 10 * DAY);
    w.positions = vec![pos_row(&a, "old-open", PosStatus::Open).into()];
    w.value = 5.0;
    d.save_wallet(&w).unwrap();

    let mut api = Mock::new();
    api.fail_positions = true;
    // No new trades: no refresh, no wallet write.
    let rep = run(&api, &d, &Config::default(), &opts()).await.unwrap();
    assert_eq!(rep.new_trades, 0);
    assert_eq!(d.load_wallet(&a).unwrap().unwrap(), w);

    api.activity
        .insert(a.clone(), vec![buy(&a, NOW - 100, 1.0)]);
    let rep = run(&api, &d, &Config::default(), &opts()).await.unwrap();
    assert_eq!(rep.new_trades, 1);
    let got = d.load_wallet(&a).unwrap().unwrap();
    assert_eq!(got.positions, w.positions);
    assert_eq!(got.value, 5.0);
    assert_eq!(got.trades.len(), 1);
}

#[tokio::test]
async fn dry_run_writes_nothing() {
    let (t, d) = tmp();
    let a = addr(1);
    let b = addr(2);
    save_watchlist(
        &d,
        vec![
            entry(a.clone(), 60, NOW - DAY, NOW - 600),
            WatchlistEntry {
                added_at: NOW - 90 * DAY,
                ..entry(b.clone(), 60, NOW - 70 * DAY, 0)
            },
        ],
    );
    d.save_cursors(&Cursors {
        last_track: NOW - 480,
        ..Default::default()
    })
    .unwrap();
    d.save_wallet(&Wallet::new(a.clone(), NOW - DAY)).unwrap();
    let before = snapshot(t.path());

    let mut api = Mock::new();
    api.activity
        .insert(a.clone(), vec![buy(&a, NOW - 100, 1000.0)]);
    let rep = run(
        &api,
        &d,
        &Config::default(),
        &TrackOpts {
            dry_run: true,
            now: NOW,
        },
    )
    .await
    .unwrap();
    assert_eq!(rep.new_trades, 1);
    assert_eq!(rep.events.len(), 1);
    assert_eq!(rep.evicted, vec![b]);
    assert_eq!(snapshot(t.path()), before);
}

#[tokio::test]
async fn first_poll_backfill_emits_no_events() {
    let (_t, d) = tmp();
    let a = addr(1);
    save_watchlist(&d, vec![entry(a.clone(), 60, 0, 0)]);
    let mut api = Mock::new();
    api.values.insert(a.clone(), 200.0);
    api.activity
        .insert(a.clone(), vec![buy(&a, NOW - 2 * DAY, 5000.0)]);
    let rep = run(&api, &d, &Config::default(), &opts()).await.unwrap();
    assert_eq!(rep.new_trades, 1);
    assert!(rep.events.is_empty(), "backfilled trades are not news");
}
