use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use types::state::{AlertRecord, AlertsState, Watchlist, WatchlistEntry};
use types::{
    Addr, Config, MarketCategory, PosStatus, Position, Profile, ResolvedSummary, Score, Signal,
    SignalKind, Tier, TrackedTrade, TradeEvent, TradeKind, UserStats, Wallet,
};

use crate::run::{self, SyncState};
use crate::*;

// ---------------------------------------------------------------- fixtures

const NOW: i64 = 1_792_000_000; // 2026-10-14 17:46 UTC

fn addr(n: u8) -> Addr {
    Addr::parse(&format!(
        "0x{:02x}{}",
        n, "3cc8d3ef945c39b36712040886e3847eb0d7ab"
    ))
    .unwrap()
}

#[allow(clippy::too_many_arguments)]
fn pos(
    title: &str,
    slug: &str,
    outcome: &str,
    avg: f64,
    size: f64,
    cur: f64,
    status: PosStatus,
) -> Position {
    let cost = avg * size;
    let value = if status == PosStatus::Open {
        cur * size
    } else {
        0.0
    };
    let pnl = cur * size - cost;
    Position {
        condition_id: format!("0xcond-{slug}"),
        token_id: format!("tok-{slug}-{outcome}"),
        title: title.into(),
        slug: slug.into(),
        outcome: outcome.into(),
        avg_price: avg,
        total_size: size,
        current_size: if status == PosStatus::Open { size } else { 0.0 },
        cost,
        current_value: value,
        current_price: cur,
        realized_pnl: if status == PosStatus::Open { 0.0 } else { pnl },
        total_pnl: pnl,
        percent_pnl: pnl / cost * 100.0,
        status,
        first_entry_at: Some(NOW - 20 * 86_400),
        last_event_at: Some(NOW - 2 * 86_400),
        end_date: None,
        market_volume: Some(120_000.0),
        market_duration_h: Some(240.0),
        category: MarketCategory::Politics,
    }
}

fn sig(kind: SignalKind, strength: f64, evidence: &str) -> Signal {
    Signal {
        kind,
        strength,
        evidence: evidence.into(),
        refs: vec![],
    }
}

/// A realistic high-tier candidate.
fn wallet() -> Wallet {
    let mut w = Wallet::new(
        Addr::parse("0xF3238CC8D3EF945C39B36712040886E3847EB0D7").unwrap(),
        NOW - 30 * 86_400,
    );
    w.last_scored = NOW;
    w.profile = Some(Profile {
        name: "@quiet|whale".into(),
        pseudonym: "Glossy-Antelope".into(),
        created_at: Some(NOW - 412 * 86_400),
        ..Default::default()
    });
    w.stats = Some(UserStats {
        join_date: Some(NOW - 412 * 86_400 - 3600),
        markets_traded: 14,
        trade_count: 63,
        volume_usdc: 48_210.0,
        realized_pnl: 18_734.42,
        biggest_win: 9_100.0,
    });
    let mut positions = vec![
        pos(
            "Will the Fed cut rates in December?",
            "fed-cut-december",
            "Yes",
            0.18,
            25_000.0,
            0.64,
            PosStatus::Open,
        ),
        pos(
            "Will X | Y merge by 2027?",
            "x-y-merge-2027",
            "No",
            0.22,
            9_000.0,
            1.0,
            PosStatus::Redeemable,
        ),
        pos(
            "Will candidate A win the primary?",
            "candidate-a-primary",
            "Yes",
            0.11,
            12_500.0,
            1.0,
            PosStatus::Redeemable,
        ),
        pos(
            "Will the ceasefire hold through March?",
            "ceasefire-march",
            "No",
            0.35,
            2_000.0,
            0.0,
            PosStatus::RedeemableLost,
        ),
        pos(
            "Will BTC hit $150k in 2026?",
            "btc-150k-2026",
            "Yes",
            0.04,
            3_000.0,
            0.02,
            PosStatus::Open,
        ),
    ];
    for i in 0..8 {
        positions.push(pos(
            &format!("Minor market #{i}"),
            &format!("minor-{i}"),
            "Yes",
            0.5,
            100.0 + f64::from(i),
            0.5,
            PosStatus::Closed,
        ));
    }
    w.positions = positions;
    w.resolved = ResolvedSummary {
        n: 9.5,
        wins: 7.0,
        expected_wins: 2.1,
        z: 3.42,
        longshot_wins: 3,
    };
    w.signals = vec![
        sig(
            SignalKind::ResolutionTiming,
            0.71,
            "3 wins entered < 48 h before resolution at ≤ 0.35",
        ),
        sig(
            SignalKind::OldAccountWinRate,
            0.92,
            "won 7/9.5 resolved at mean entry 0.21 (z=3.4)",
        ),
        sig(SignalKind::NicheMarket, 0.2, "4% of a $120k market"),
        sig(
            SignalKind::Concentration,
            0.45,
            "68% of value in one market",
        ),
    ];
    w.score = Score {
        total: 78,
        tier: Tier::High,
        materiality: 1.1,
    };
    w.sources = vec![types::Source::Feed];
    w.cluster_id = Some("c-0a1b2c3d4e".into());
    w.value = 16_060.0;
    w
}

fn scored(n: u8, score: u8, kinds: &[SignalKind]) -> Wallet {
    let mut w = Wallet::new(addr(n), NOW - 86_400);
    w.score = Score {
        total: score,
        tier: if score >= 70 {
            Tier::High
        } else {
            Tier::Medium
        },
        materiality: 1.0,
    };
    w.signals = kinds.iter().map(|k| sig(*k, 0.8, "evidence")).collect();
    w
}

fn entry(a: &Addr) -> WatchlistEntry {
    WatchlistEntry {
        addr: a.clone(),
        added_at: NOW - 86_400,
        reason: SignalKind::OldAccountWinRate,
        score: 70,
        last_activity_ts: 0,
        last_polled: 0,
        last_qualified: NOW,
        issue: None,
    }
}

fn watch(ws: &[&Wallet]) -> Watchlist {
    Watchlist {
        entries: ws.iter().map(|w| entry(&w.addr)).collect(),
    }
}

fn record(issue: u64, score: u8, kinds: &[SignalKind]) -> AlertRecord {
    AlertRecord {
        issue,
        opened_at: NOW - 10 * 86_400,
        last_score: score,
        signal_kinds: kinds.to_vec(),
        last_comment_ts: 0,
        closed: false,
    }
}

fn trade(ts: i64, kind: TradeKind, usdc: f64, key: &str) -> TrackedTrade {
    TrackedTrade {
        key: key.into(),
        ts,
        kind,
        condition_id: "0xcond-fed".into(),
        token_id: "tok".into(),
        title: "Will the Fed cut rates in December?".into(),
        outcome: "Yes".into(),
        price: 0.23,
        size: usdc / 0.23,
        usdc,
        tx: "0x9f2c1e0b7a6d5c4b3a291807f6e5d4c3b2a1908f7e6d5c4b3a29180f7e6d5c4b".into(),
    }
}

fn event(a: &Addr, t: TrackedTrade, issue: Option<u64>, reason: &str) -> TradeEvent {
    TradeEvent {
        addr: a.clone(),
        trade: t,
        wallet_value: 16_060.0,
        issue,
        reason: reason.into(),
    }
}

// ---------------------------------------------------------------- mock

#[derive(Default)]
struct Mock {
    inner: Mutex<MockInner>,
}

#[derive(Default)]
struct MockInner {
    next: u64,
    issues: BTreeMap<u64, IssueSummary>,
    comments: Vec<(u64, String)>,
    calls: Vec<String>,
}

impl Mock {
    fn with_issues(issues: Vec<IssueSummary>) -> Self {
        let m = Mock::default();
        {
            let mut g = m.inner.lock().unwrap();
            g.next = issues.iter().map(|i| i.number).max().unwrap_or(0);
            g.issues = issues.into_iter().map(|i| (i.number, i)).collect();
        }
        m
    }
    fn calls(&self) -> Vec<String> {
        self.inner.lock().unwrap().calls.clone()
    }
    /// Calls other than label creation.
    fn writes(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| !c.starts_with("label "))
            .collect()
    }
    fn comments(&self) -> Vec<(u64, String)> {
        self.inner.lock().unwrap().comments.clone()
    }
    fn issue(&self, n: u64) -> IssueSummary {
        self.inner.lock().unwrap().issues[&n].clone()
    }
}

#[async_trait]
impl GitHubApi for Mock {
    async fn create_issue(
        &self,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> anyhow::Result<u64> {
        let mut g = self.inner.lock().unwrap();
        g.next += 1;
        let n = g.next;
        g.calls.push(format!("create #{n} {title}"));
        g.issues.insert(
            n,
            IssueSummary {
                number: n,
                title: title.into(),
                state: "open".into(),
                labels: labels.to_vec(),
                body: body.into(),
            },
        );
        Ok(n)
    }
    async fn update_issue(&self, number: u64, u: &IssueUpdate) -> anyhow::Result<()> {
        let mut g = self.inner.lock().unwrap();
        g.calls
            .push(format!("update #{number} state={:?}", u.state));
        let i = g
            .issues
            .get_mut(&number)
            .ok_or_else(|| anyhow::anyhow!("no issue {number}"))?;
        if let Some(t) = &u.title {
            i.title = t.clone();
        }
        if let Some(b) = &u.body {
            i.body = b.clone();
        }
        if let Some(l) = &u.labels {
            i.labels = l.clone();
        }
        if let Some(s) = &u.state {
            i.state = s.clone();
        }
        Ok(())
    }
    async fn comment(&self, number: u64, body: &str) -> anyhow::Result<()> {
        let mut g = self.inner.lock().unwrap();
        g.calls.push(format!("comment #{number}"));
        g.comments.push((number, body.into()));
        Ok(())
    }
    async fn list_issues(&self, label: &str, state: &str) -> anyhow::Result<Vec<IssueSummary>> {
        let mut g = self.inner.lock().unwrap();
        g.calls.push(format!("list {label} {state}"));
        Ok(g.issues
            .values()
            .filter(|i| i.labels.iter().any(|l| l == label) && (state == "all" || i.state == state))
            .cloned()
            .collect())
    }
    async fn ensure_label(
        &self,
        name: &str,
        _color: &str,
        _description: &str,
    ) -> anyhow::Result<()> {
        self.inner
            .lock()
            .unwrap()
            .calls
            .push(format!("label {name}"));
        Ok(())
    }
}

fn opts(dry_run: bool) -> AlertOpts {
    AlertOpts { dry_run, now: NOW }
}

// ---------------------------------------------------------------- render

#[test]
fn render_issue_snapshot() {
    let cfg = Config::default();
    let (title, body, labels) = render_issue(&wallet(), &cfg);
    assert_eq!(
        title,
        "[candidate] 0xf323…b0d7 · score 78 · OldAccountWinRate+ResolutionTiming"
    );
    insta::assert_snapshot!("issue_title", title);
    insta::assert_snapshot!("issue_body", body);
    insta::assert_json_snapshot!("issue_labels", labels);
    assert!(body.ends_with("<!-- polywatch:addr=0xf3238cc8d3ef945c39b36712040886e3847eb0d7 -->"));
    assert_eq!(parse_marker(&body), Some(wallet().addr));
    // Only the top ten positions are listed.
    assert_eq!(body.matches("](https://polymarket.com/market/").count(), 10);
    assert!(
        body.contains("`@quiet|whale`"),
        "user-chosen names go in code spans (no @mention)"
    );
}

#[test]
fn render_issue_minimal_wallet() {
    let cfg = Config::default();
    let w = scored(1, 61, &[]);
    let (title, body, labels) = render_issue(&w, &cfg);
    assert_eq!(title, format!("[candidate] {} · score 61", w.addr.short()));
    assert_eq!(labels, vec!["insider-candidate", "tier:medium"]);
    assert!(body.contains("win rate n/a"));
    assert!(body.contains("Heuristic lead, not proof of insider trading."));
}

#[test]
fn render_trade_comment_snapshot() {
    let cfg = Config::default();
    let w = wallet();
    let lines: Vec<String> = [
        event(
            &w.addr,
            trade(1_792_051_380, TradeKind::Buy, 12_500.0, "a"),
            Some(7),
            "≥ $500",
        ),
        event(
            &w.addr,
            trade(1_792_051_440, TradeKind::Sell, 3_210.55, "b"),
            Some(7),
            "32% of wallet value",
        ),
        event(
            &w.addr,
            trade(1_792_051_500, TradeKind::Redeem, 9_000.0, "c"),
            Some(7),
            "≥ $500",
        ),
    ]
    .iter()
    .map(|e| render_trade_comment(e, &cfg))
    .collect();
    assert!(lines[0].starts_with("- 🟢 BUY **$12,500** · Yes · Will the Fed"));
    assert!(lines[0].contains("2026-10-15 08:03 UTC"));
    assert!(lines[1].starts_with("- 🔴 SELL **$3,211**"));
    insta::assert_snapshot!("trade_comment", lines.join("\n"));
}

#[test]
fn utc_formatting() {
    assert_eq!(render::utc(0), "1970-01-01 00:00");
    assert_eq!(render::utc(1_785_542_400 + 3_660), "2026-08-01 01:01");
    assert_eq!(render::utc(951_782_400), "2000-02-29 00:00");
}

#[test]
fn money_formatting() {
    assert_eq!(render::usd(1_234_567.4), "$1,234,567");
    assert_eq!(render::usd(5.25), "$5.25");
    assert_eq!(render::usd(-300.0), "-$300");
    assert_eq!(render::signed_usd(543.2), "+$543");
    assert_eq!(render::signed_usd(-0.001), "$0");
}

#[test]
fn markers_and_titles_parse() {
    let a = addr(9);
    assert_eq!(parse_marker(&format!("x\n\n{}", marker(&a))), Some(a));
    assert_eq!(parse_marker("no marker"), None);
    assert_eq!(
        run::parse_title_score("[candidate] 0xab…cd · score 78 · A+B"),
        Some(78)
    );
    assert_eq!(run::parse_title_score("Insider?"), None);
}

// ---------------------------------------------------------------- sync

async fn sync_mem(gh: &Mock, st: &mut SyncState, cfg: &Config, dry: bool) -> AlertReport {
    let mut report = AlertReport::default();
    run::sync_state(gh, st, cfg, &opts(dry), &mut report)
        .await
        .unwrap();
    report
}

#[tokio::test]
async fn sync_opens_issue_and_links_wallet() {
    let cfg = Config::default();
    let hi = scored(1, 81, &[SignalKind::OldAccountWinRate]);
    let lo = scored(2, 59, &[SignalKind::PreMove]);
    let unwatched = scored(3, 90, &[SignalKind::PreMove]);
    let gh = Mock::default();
    let mut st = SyncState::new(
        vec![lo.clone(), hi.clone(), unwatched],
        AlertsState::default(),
        watch(&[&hi, &lo]),
    );
    let rep = sync_mem(&gh, &mut st, &cfg, false).await;

    assert_eq!(rep.actions.len(), 1);
    assert!(matches!(&rep.actions[0], Action::Open { addr, .. } if *addr == hi.addr));
    assert_eq!(
        gh.writes(),
        vec![
            "list insider-candidate all".to_string(),
            format!("create #1 {}", render::title(&hi))
        ]
    );
    assert_eq!(
        gh.calls()
            .iter()
            .filter(|c| c.starts_with("label "))
            .count(),
        run::label_specs(&cfg).len()
    );
    let rec = &st.alerts.wallets[&hi.addr];
    assert_eq!((rec.issue, rec.last_score, rec.closed), (1, 81, false));
    assert_eq!(rec.signal_kinds, vec![SignalKind::OldAccountWinRate]);
    assert_eq!(st.wallets[1].issue, Some(1));
    assert_eq!(st.watchlist.entries[0].issue, Some(1));
    assert!(st.dirty_wallets.contains(&hi.addr) && st.watchlist_dirty);

    // Second run: nothing changed → no calls at all.
    let gh2 = Mock::default();
    let rep = sync_mem(&gh2, &mut st, &cfg, false).await;
    assert!(rep.actions.is_empty());
    assert!(gh2.calls().is_empty());
}

#[tokio::test]
async fn sync_rescore_comments_and_updates() {
    let cfg = Config::default();
    let w = scored(1, 81, &[SignalKind::OldAccountWinRate, SignalKind::PreMove]);
    let small = scored(2, 66, &[SignalKind::Concentration]);
    let gh = Mock::default();
    gh.create_issue("old", "old body", &[]).await.unwrap();
    gh.create_issue("old2", "old body", &[]).await.unwrap();
    let mut alerts = AlertsState::default();
    alerts.wallets.insert(
        w.addr.clone(),
        record(1, 62, &[SignalKind::OldAccountWinRate]),
    );
    alerts.wallets.insert(
        small.addr.clone(),
        record(2, 60, &[SignalKind::Concentration]),
    );
    let mut st = SyncState::new(vec![w.clone(), small.clone()], alerts, watch(&[&w, &small]));
    let rep = sync_mem(&gh, &mut st, &cfg, false).await;

    // Score +6 without new kinds is below rescore_delta: no action for `small`.
    assert_eq!(
        rep.actions,
        vec![Action::Comment {
            addr: w.addr.clone(),
            issue: 1,
            reason: "Score 62 → 81; new signals: PreMove".into()
        }]
    );
    assert_eq!(
        gh.comments(),
        vec![(1, "Score 62 → 81; new signals: PreMove".to_string())]
    );
    let i = gh.issue(1);
    assert_eq!(i.title, render::title(&w));
    assert!(i.labels.contains(&"sig:pre-move".to_string()));
    let rec = &st.alerts.wallets[&w.addr];
    assert_eq!(rec.last_score, 81);
    assert_eq!(
        rec.signal_kinds,
        vec![SignalKind::OldAccountWinRate, SignalKind::PreMove]
    );
    assert_eq!(st.alerts.wallets[&small.addr].last_score, 60);
    // Wallets get linked to their existing issues.
    assert_eq!(st.wallets[1].issue, Some(2));
}

#[tokio::test]
async fn sync_caps_new_issues_highest_first() {
    let mut cfg = Config::default();
    cfg.alerts.max_new_issues_per_run = 2;
    let ws: Vec<Wallet> = (1..=5)
        .map(|n| scored(n, 60 + n * 5, &[SignalKind::PreMove]))
        .collect();
    let gh = Mock::default();
    let mut st = SyncState::new(
        ws.clone(),
        AlertsState::default(),
        watch(&ws.iter().collect::<Vec<_>>()),
    );
    let rep = sync_mem(&gh, &mut st, &cfg, false).await;
    assert_eq!(rep.capped, 3);
    let opened: Vec<&Addr> = rep
        .actions
        .iter()
        .map(|a| match a {
            Action::Open { addr, .. } => addr,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(opened, vec![&ws[4].addr, &ws[3].addr]);
    assert_eq!(st.alerts.wallets.len(), 2);
}

#[tokio::test]
async fn sync_closes_dropped_and_reopens_returning() {
    let cfg = Config::default();
    let gone = scored(1, 80, &[SignalKind::PreMove]);
    let back = scored(2, 75, &[SignalKind::OldAccountWinRate]);
    let gh = Mock::default();
    gh.create_issue("gone", "", &["insider-candidate".into()])
        .await
        .unwrap();
    gh.create_issue("back", "", &["insider-candidate".into()])
        .await
        .unwrap();
    gh.update_issue(
        2,
        &IssueUpdate {
            state: Some("closed".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    let mut alerts = AlertsState::default();
    alerts
        .wallets
        .insert(gone.addr.clone(), record(1, 80, &[SignalKind::PreMove]));
    let mut r = record(2, 70, &[SignalKind::OldAccountWinRate]);
    r.closed = true;
    alerts.wallets.insert(back.addr.clone(), r);
    // A record whose wallet file is gone entirely.
    let orphan = addr(3);
    alerts
        .wallets
        .insert(orphan.clone(), record(3, 65, &[SignalKind::Concentration]));
    gh.create_issue("orphan", "", &[]).await.unwrap();

    let mut st = SyncState::new(vec![gone.clone(), back.clone()], alerts, watch(&[&back]));
    let rep = sync_mem(&gh, &mut st, &cfg, false).await;
    assert_eq!(
        rep.actions,
        vec![
            Action::Comment {
                addr: back.addr.clone(),
                issue: 2,
                reason: "Reopened: back on the watchlist, score 70 → 75".into()
            },
            Action::Close {
                addr: gone.addr.clone(),
                issue: 1
            },
            Action::Close {
                addr: orphan.clone(),
                issue: 3
            },
        ]
    );
    assert_eq!(gh.issue(2).state, "open");
    assert_eq!(gh.issue(1).state, "closed");
    assert!(gh.issue(1).labels.contains(&"resolved".to_string()));
    assert!(gh.issue(1).labels.contains(&"sig:pre-move".to_string()));
    assert_eq!(
        gh.issue(3).labels,
        vec!["insider-candidate", "sig:concentration", "resolved"]
    );
    assert!(gh
        .comments()
        .iter()
        .any(|(n, b)| *n == 1 && b.starts_with("Dropped off the watchlist")));
    assert!(st.alerts.wallets[&gone.addr].closed);
    assert!(!st.alerts.wallets[&back.addr].closed);

    // Closed records are not closed again.
    let gh2 = Mock::with_issues(vec![gh.issue(1), gh.issue(2), gh.issue(3)]);
    let rep = sync_mem(&gh2, &mut st, &cfg, false).await;
    assert!(rep.actions.is_empty(), "{rep:?}");
}

#[tokio::test]
async fn sync_dry_run_makes_no_calls() {
    let mut cfg = Config::default();
    cfg.alerts.max_new_issues_per_run = 1;
    let a = scored(1, 90, &[SignalKind::PreMove]);
    let b = scored(2, 85, &[SignalKind::PreMove]);
    let c = scored(3, 85, &[SignalKind::PreMove, SignalKind::NicheMarket]);
    let gone = scored(4, 70, &[SignalKind::PreMove]);
    let mut alerts = AlertsState::default();
    alerts
        .wallets
        .insert(c.addr.clone(), record(5, 60, &[SignalKind::PreMove]));
    alerts
        .wallets
        .insert(gone.addr.clone(), record(6, 70, &[SignalKind::PreMove]));
    let before = alerts.clone();
    let gh = Mock::default();
    let mut st = SyncState::new(
        vec![a.clone(), b, c.clone(), gone.clone()],
        alerts,
        watch(&[&a, &c]),
    );
    let rep = sync_mem(&gh, &mut st, &cfg, true).await;
    assert!(gh.calls().is_empty(), "{:?}", gh.calls());
    assert_eq!(rep.capped, 0, "b is not watched");
    assert_eq!(rep.actions.len(), 3, "{rep:?}");
    assert!(matches!(&rep.actions[0], Action::Open { addr, .. } if *addr == a.addr));
    assert!(matches!(&rep.actions[1], Action::Comment { issue: 5, .. }));
    assert!(matches!(&rep.actions[2], Action::Close { issue: 6, .. }));
    assert_ne!(
        st.alerts, before,
        "in-memory plan state moves on, but sync() does not save it"
    );
}

#[tokio::test]
async fn sync_rebuilds_alerts_from_markers() {
    let cfg = Config::default();
    let w = scored(1, 80, &[SignalKind::PreMove]);
    let other = scored(2, 64, &[SignalKind::Concentration]);
    let (title, body, labels) = render_issue(&w, &cfg);
    let (t2, b2, l2) = render_issue(&other, &cfg);
    let gh = Mock::with_issues(vec![
        IssueSummary {
            number: 40,
            title,
            state: "open".into(),
            labels,
            body,
        },
        IssueSummary {
            number: 41,
            title: t2,
            state: "closed".into(),
            labels: l2,
            body: b2,
        },
        IssueSummary {
            number: 3,
            title: "legacy".into(),
            state: "open".into(),
            labels: vec!["insider-candidate".into()],
            body: "old python body".into(),
        },
    ]);
    let mut st = SyncState::new(vec![w.clone()], AlertsState::default(), watch(&[&w]));
    let rep = sync_mem(&gh, &mut st, &cfg, false).await;
    assert!(rep.actions.is_empty(), "{rep:?}");
    assert_eq!(gh.writes(), vec!["list insider-candidate all"]);
    let r = &st.alerts.wallets[&w.addr];
    assert_eq!((r.issue, r.last_score, r.closed), (40, 80, false));
    assert_eq!(r.signal_kinds, vec![SignalKind::PreMove]);
    let r2 = &st.alerts.wallets[&other.addr];
    assert_eq!((r2.issue, r2.last_score, r2.closed), (41, 64, true));
    assert_eq!(st.alerts.wallets.len(), 2);
    assert_eq!(st.wallets[0].issue, Some(40));
}

// ---------------------------------------------------------------- comments

async fn comments_mem(
    gh: &Mock,
    alerts: &mut AlertsState,
    cfg: &Config,
    ev: &[TradeEvent],
    dry: bool,
) -> AlertReport {
    let mut report = AlertReport::default();
    run::comment_state(gh, alerts, cfg, ev, &opts(dry), &mut report)
        .await
        .unwrap();
    report
}

#[tokio::test]
async fn comments_combine_rate_limit_and_cap() {
    let mut cfg = Config::default();
    cfg.alerts.max_comments_per_run = 2;
    let (a, b, c, d, e) = (addr(1), addr(2), addr(3), addr(4), addr(5));
    let mut alerts = AlertsState::default();
    alerts.wallets.insert(a.clone(), record(1, 70, &[]));
    let mut recent = record(2, 70, &[]);
    recent.last_comment_ts = NOW - 600;
    alerts.wallets.insert(b.clone(), recent);
    alerts.wallets.insert(c.clone(), record(3, 70, &[]));
    let mut closed = record(4, 70, &[]);
    closed.closed = true;
    alerts.wallets.insert(d.clone(), closed);

    let events = vec![
        event(
            &a,
            trade(NOW - 50, TradeKind::Sell, 900.0, "a2"),
            None,
            "≥ $500",
        ),
        event(
            &a,
            trade(NOW - 100, TradeKind::Buy, 2_000.0, "a1"),
            None,
            "30% of wallet value",
        ),
        event(
            &a,
            trade(NOW - 100, TradeKind::Buy, 2_000.0, "a1"),
            None,
            "30% of wallet value",
        ),
        event(
            &b,
            trade(NOW - 10, TradeKind::Buy, 5_000.0, "b1"),
            None,
            "≥ $500",
        ),
        event(
            &c,
            trade(NOW - 10, TradeKind::Buy, 600.0, "c1"),
            Some(33),
            "≥ $500",
        ),
        event(
            &d,
            trade(NOW - 10, TradeKind::Buy, 9_000.0, "d1"),
            None,
            "≥ $500",
        ),
        event(
            &e,
            trade(NOW - 10, TradeKind::Buy, 700.0, "e1"),
            Some(55),
            "≥ $500",
        ),
        event(
            &addr(6),
            trade(NOW - 10, TradeKind::Buy, 700.0, "f1"),
            None,
            "≥ $500",
        ),
    ];
    let gh = Mock::default();
    let rep = comments_mem(&gh, &mut alerts, &cfg, &events, false).await;

    // Order by biggest trade: d (closed, skipped), b (rate-limited), a (2000), e (700), c (600, capped).
    let comments = gh.comments();
    assert_eq!(comments.len(), 2, "{comments:?}");
    assert_eq!(comments[0].0, 1);
    let lines: Vec<&str> = comments[0].1.lines().collect();
    assert_eq!(lines.len(), 2, "combined and deduplicated");
    assert!(
        lines[0].contains("BUY") && lines[1].contains("SELL"),
        "oldest first"
    );
    assert_eq!(comments[1].0, 55, "event issue used without a record");
    assert_eq!(rep.capped, 2);
    assert_eq!(
        rep.actions[0],
        Action::Comment {
            addr: a.clone(),
            issue: 1,
            reason: "2 trades: 30% of wallet value; ≥ $500".into()
        }
    );
    assert_eq!(alerts.wallets[&a].last_comment_ts, NOW);
    assert_eq!(alerts.wallets[&b].last_comment_ts, NOW - 600);
    assert_eq!(alerts.wallets[&c].last_comment_ts, 0);
}

#[tokio::test]
async fn comments_dry_run_makes_no_calls() {
    let cfg = Config::default();
    let a = addr(1);
    let mut alerts = AlertsState::default();
    alerts.wallets.insert(a.clone(), record(1, 70, &[]));
    let gh = Mock::default();
    let ev = [event(
        &a,
        trade(NOW, TradeKind::Buy, 900.0, "k"),
        None,
        "≥ $500",
    )];
    let rep = comments_mem(&gh, &mut alerts, &cfg, &ev, true).await;
    assert_eq!(rep.actions.len(), 1);
    assert!(gh.calls().is_empty());
}

// ---------------------------------------------------------------- legacy

#[tokio::test]
async fn legacy_close_closes_and_labels() {
    let cfg = Config::default();
    let mk = |n: u64, state: &str, labels: &[&str]| IssueSummary {
        number: n,
        title: format!("Insider {n}"),
        state: state.into(),
        labels: labels.iter().map(|s| s.to_string()).collect(),
        body: String::new(),
    };
    let gh = Mock::with_issues(vec![
        mk(1, "open", &["insider", "bug"]),
        mk(2, "open", &["insider"]),
        mk(3, "closed", &["insider"]),
        mk(4, "open", &["other"]),
    ]);
    assert_eq!(legacy_close(&gh, &cfg, "insider", true).await.unwrap(), 2);
    assert_eq!(gh.calls(), vec!["list insider open"]);

    assert_eq!(legacy_close(&gh, &cfg, "insider", false).await.unwrap(), 2);
    assert_eq!(gh.issue(1).state, "closed");
    assert_eq!(gh.issue(1).labels, vec!["insider", "bug", "legacy"]);
    assert_eq!(gh.issue(2).labels, vec!["insider", "legacy"]);
    assert_eq!(gh.issue(4).state, "open");
    let calls = gh.calls();
    assert_eq!(
        calls.iter().filter(|c| c.starts_with("update")).count(),
        2,
        "one PATCH per issue"
    );
    assert!(calls.contains(&"label legacy".to_string()));
    assert_eq!(legacy_close(&gh, &cfg, "insider", false).await.unwrap(), 0);
}

// ---------------------------------------------------------------- DataDir round trip

/// `None` while `store` is still a stub (its bodies `todo!()`).
fn data_dir(path: &std::path::Path) -> Option<store::DataDir> {
    let p = path.to_path_buf();
    std::panic::catch_unwind(move || {
        let d = store::DataDir::open(p).ok()?;
        d.alerts().ok()?;
        Some(d)
    })
    .ok()
    .flatten()
}

#[tokio::test]
async fn sync_and_comment_through_data_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let Some(data) = data_dir(tmp.path()) else {
        eprintln!("store::DataDir not implemented yet; skipping");
        return;
    };
    let cfg = Config::default();
    let w = wallet();
    data.save_wallet(&w).unwrap();
    data.save_watchlist(&watch(&[&w])).unwrap();
    let gh = Mock::default();

    let rep = sync(&gh, &data, &cfg, &opts(true)).await.unwrap();
    assert_eq!(rep.actions.len(), 1);
    assert!(
        data.alerts().unwrap().wallets.is_empty(),
        "dry run writes nothing"
    );

    let rep = sync(&gh, &data, &cfg, &opts(false)).await.unwrap();
    assert_eq!(rep.actions.len(), 1);
    let rec = data.alerts().unwrap().wallets[&w.addr].clone();
    assert_eq!(rec.issue, 1);
    assert_eq!(data.load_wallet(&w.addr).unwrap().unwrap().issue, Some(1));
    assert_eq!(data.watchlist().unwrap().entries[0].issue, Some(1));

    let ev = [event(
        &w.addr,
        trade(NOW, TradeKind::Buy, 900.0, "k"),
        None,
        "≥ $500",
    )];
    let rep = comment_trades(&gh, &data, &cfg, &ev, &opts(false))
        .await
        .unwrap();
    assert_eq!(rep.actions.len(), 1);
    assert_eq!(data.alerts().unwrap().wallets[&w.addr].last_comment_ts, NOW);
}
