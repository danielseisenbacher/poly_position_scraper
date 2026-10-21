//! Decision logic for `sync`, `comment_trades` and `legacy_close`, operating on
//! in-memory state so it can be tested without a data directory.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use anyhow::Context;
use types::state::{AlertRecord, AlertsState, Watchlist};
use types::{Addr, Config, SignalKind, TradeEvent, Wallet};

use crate::render::{self, kind_from_label, labeled_kinds, parse_marker, sig_label};
use crate::{Action, AlertOpts, AlertReport, GitHubApi, IssueSummary, IssueUpdate};

/// Labels `sync` relies on, with colours and descriptions.
pub(crate) fn label_specs(cfg: &Config) -> Vec<(String, &'static str, String)> {
    let a = &cfg.alerts;
    let mut v = vec![
        (
            a.label_candidate.clone(),
            "d93f0b",
            "polywatch: possible insider-trading wallet".into(),
        ),
        (
            "tier:high".into(),
            "b60205",
            format!("polywatch: score ≥ {}", cfg.signals.tier_high),
        ),
        (
            "tier:medium".into(),
            "fbca04",
            format!("polywatch: score ≥ {}", cfg.signals.tier_medium),
        ),
    ];
    for k in SignalKind::ALL {
        v.push((sig_label(k), "c5def5", format!("polywatch signal: {k}")));
    }
    v.push((
        "cluster".into(),
        "5319e7",
        "polywatch: wallet linked to other accounts".into(),
    ));
    v.push((
        a.label_resolved.clone(),
        "0e8a16",
        "polywatch: wallet left the watchlist".into(),
    ));
    v
}

pub(crate) struct SyncState {
    pub wallets: Vec<Wallet>,
    pub alerts: AlertsState,
    pub watchlist: Watchlist,
    pub dirty_wallets: BTreeSet<Addr>,
    pub watchlist_dirty: bool,
}

impl SyncState {
    pub fn new(wallets: Vec<Wallet>, alerts: AlertsState, watchlist: Watchlist) -> Self {
        SyncState {
            wallets,
            alerts,
            watchlist,
            dirty_wallets: BTreeSet::new(),
            watchlist_dirty: false,
        }
    }

    /// Point the wallet and its watchlist entry at `issue` (no-op if already so).
    fn link_issue(&mut self, wallet_idx: usize, issue: u64) {
        let w = &mut self.wallets[wallet_idx];
        if w.issue != Some(issue) {
            w.issue = Some(issue);
            self.dirty_wallets.insert(w.addr.clone());
        }
        let addr = w.addr.clone();
        if let Some(e) = self.watchlist.entries.iter_mut().find(|e| e.addr == addr) {
            if e.issue != Some(issue) {
                e.issue = Some(issue);
                self.watchlist_dirty = true;
            }
        }
    }
}

/// Rebuild `alerts.json` from issues carrying our marker. Prefers an open issue per
/// wallet, then the oldest.
pub(crate) fn rebuild_alerts(issues: &[IssueSummary], now: i64) -> AlertsState {
    let mut best: BTreeMap<Addr, &IssueSummary> = BTreeMap::new();
    for i in issues {
        let Some(addr) = parse_marker(&i.body) else {
            continue;
        };
        let better = match best.get(&addr) {
            None => true,
            Some(cur) => {
                let (open, cur_open) = (i.state == "open", cur.state == "open");
                (open && !cur_open) || (open == cur_open && i.number < cur.number)
            }
        };
        if better {
            best.insert(addr, i);
        }
    }
    let wallets = best
        .into_iter()
        .map(|(addr, i)| {
            let mut kinds: Vec<SignalKind> =
                i.labels.iter().filter_map(|l| kind_from_label(l)).collect();
            kinds.sort();
            kinds.dedup();
            let rec = AlertRecord {
                issue: i.number,
                opened_at: now,
                last_score: parse_title_score(&i.title).unwrap_or(0),
                signal_kinds: kinds,
                last_comment_ts: 0,
                closed: i.state == "closed",
            };
            (addr, rec)
        })
        .collect();
    AlertsState { wallets }
}

/// `78` from `[candidate] 0x… · score 78 · …`.
pub(crate) fn parse_title_score(title: &str) -> Option<u8> {
    let rest = &title[title.find("score ")? + "score ".len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn union(a: &[SignalKind], b: &[SignalKind]) -> Vec<SignalKind> {
    let s: BTreeSet<SignalKind> = a.iter().chain(b).copied().collect();
    s.into_iter().collect()
}

fn kinds_list(kinds: &[SignalKind]) -> String {
    kinds
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Labels for an issue whose wallet file is gone.
fn record_labels(rec: &AlertRecord, cfg: &Config) -> Vec<String> {
    let mut l = vec![cfg.alerts.label_candidate.clone()];
    l.extend(rec.signal_kinds.iter().map(|k| sig_label(*k)));
    l
}

struct LabelGuard {
    ready: bool,
}

impl LabelGuard {
    /// Create the labels on first use, so a run with nothing to do makes no calls.
    async fn ensure(&mut self, gh: &dyn GitHubApi, cfg: &Config, dry: bool) -> anyhow::Result<()> {
        if !self.ready && !dry {
            for (name, color, desc) in label_specs(cfg) {
                gh.ensure_label(&name, color, &desc).await?;
            }
        }
        self.ready = true;
        Ok(())
    }
}

pub(crate) async fn sync_state(
    gh: &dyn GitHubApi,
    st: &mut SyncState,
    cfg: &Config,
    opts: &AlertOpts,
    report: &mut AlertReport,
) -> anyhow::Result<()> {
    let ac = &cfg.alerts;
    let dry = opts.dry_run;
    let now = opts.now;

    if st.alerts.wallets.is_empty() && !dry {
        let issues = gh.list_issues(&ac.label_candidate, "all").await?;
        st.alerts = rebuild_alerts(&issues, now);
        if !st.alerts.wallets.is_empty() {
            tracing::info!(
                n = st.alerts.wallets.len(),
                "rebuilt alerts.json from issue markers"
            );
        }
    }

    let watched: BTreeSet<Addr> = st
        .watchlist
        .entries
        .iter()
        .map(|e| e.addr.clone())
        .collect();
    let mut candidates: Vec<usize> = (0..st.wallets.len())
        .filter(|&i| {
            let w = &st.wallets[i];
            w.score.total >= ac.issue_min_score && watched.contains(&w.addr)
        })
        .collect();
    candidates.sort_by(|&a, &b| {
        let (wa, wb) = (&st.wallets[a], &st.wallets[b]);
        wb.score
            .total
            .cmp(&wa.score.total)
            .then_with(|| wa.addr.cmp(&wb.addr))
    });

    let mut labels = LabelGuard { ready: false };
    let mut opened = 0usize;
    for i in candidates {
        let w = &st.wallets[i];
        let addr = w.addr.clone();
        let score = w.score.total;
        let kinds = labeled_kinds(w);
        match st.alerts.wallets.get(&addr).cloned() {
            None => {
                if opened >= ac.max_new_issues_per_run {
                    report.capped += 1;
                    continue;
                }
                let (title, body, lbls) = render::issue(w, cfg);
                labels.ensure(gh, cfg, dry).await?;
                let number = if dry {
                    0
                } else {
                    gh.create_issue(&title, &body, &lbls)
                        .await
                        .with_context(|| format!("opening issue for {addr}"))?
                };
                opened += 1;
                st.alerts.wallets.insert(
                    addr.clone(),
                    AlertRecord {
                        issue: number,
                        opened_at: now,
                        last_score: score,
                        signal_kinds: kinds,
                        last_comment_ts: 0,
                        closed: false,
                    },
                );
                st.link_issue(i, number);
                report.actions.push(Action::Open { addr, title });
            }
            Some(rec) => {
                let new: Vec<SignalKind> = kinds
                    .iter()
                    .filter(|k| !rec.signal_kinds.contains(k))
                    .copied()
                    .collect();
                let rose =
                    u16::from(score) >= u16::from(rec.last_score) + u16::from(ac.rescore_delta);
                if !rec.closed && !rose && new.is_empty() {
                    st.link_issue(i, rec.issue);
                    continue;
                }
                let mut reason = if rec.closed {
                    format!(
                        "Reopened: back on the watchlist, score {} → {score}",
                        rec.last_score
                    )
                } else {
                    format!("Score {} → {score}", rec.last_score)
                };
                if !new.is_empty() {
                    reason.push_str(&format!("; new signals: {}", kinds_list(&new)));
                }
                let (title, body, lbls) = render::issue(w, cfg);
                labels.ensure(gh, cfg, dry).await?;
                if !dry {
                    let update = IssueUpdate {
                        title: Some(title),
                        body: Some(body),
                        labels: Some(lbls),
                        state: rec.closed.then(|| "open".to_string()),
                    };
                    gh.update_issue(rec.issue, &update)
                        .await
                        .with_context(|| format!("updating issue #{} for {addr}", rec.issue))?;
                    gh.comment(rec.issue, &reason).await.with_context(|| {
                        format!("commenting on issue #{} for {addr}", rec.issue)
                    })?;
                }
                let r = st.alerts.wallets.get_mut(&addr).expect("record exists");
                r.last_score = score;
                r.signal_kinds = union(&rec.signal_kinds, &kinds);
                r.closed = false;
                st.link_issue(i, rec.issue);
                report.actions.push(Action::Comment {
                    addr,
                    issue: rec.issue,
                    reason,
                });
            }
        }
    }

    // Close issues of wallets that left the watchlist.
    let by_addr: HashMap<&Addr, &Wallet> = st.wallets.iter().map(|w| (&w.addr, w)).collect();
    let mut closes: Vec<(Addr, u64, Vec<String>)> = Vec::new();
    for (addr, rec) in &st.alerts.wallets {
        if rec.closed || watched.contains(addr) {
            continue;
        }
        let mut lbls = match by_addr.get(addr) {
            Some(w) => render::labels(w, cfg),
            None => record_labels(rec, cfg),
        };
        if !lbls.contains(&ac.label_resolved) {
            lbls.push(ac.label_resolved.clone());
        }
        closes.push((addr.clone(), rec.issue, lbls));
    }
    for (addr, issue, lbls) in closes {
        labels.ensure(gh, cfg, dry).await?;
        if !dry {
            gh.comment(
                issue,
                "Dropped off the watchlist; closing. polywatch no longer tracks this wallet.",
            )
            .await
            .with_context(|| format!("commenting on issue #{issue} for {addr}"))?;
            let update = IssueUpdate {
                labels: Some(lbls),
                state: Some("closed".into()),
                ..Default::default()
            };
            gh.update_issue(issue, &update)
                .await
                .with_context(|| format!("closing issue #{issue} for {addr}"))?;
        }
        if let Some(r) = st.alerts.wallets.get_mut(&addr) {
            r.closed = true;
        }
        report.actions.push(Action::Close { addr, issue });
    }
    Ok(())
}

pub(crate) async fn comment_state(
    gh: &dyn GitHubApi,
    alerts: &mut AlertsState,
    cfg: &Config,
    events: &[TradeEvent],
    opts: &AlertOpts,
    report: &mut AlertReport,
) -> anyhow::Result<()> {
    let ac = &cfg.alerts;
    // Group by wallet, first appearance order, then biggest trade first so the cap
    // keeps the most important comments.
    let mut groups: Vec<(Addr, Vec<&TradeEvent>)> = Vec::new();
    for e in events {
        match groups.iter_mut().find(|(a, _)| *a == e.addr) {
            Some((_, g)) => g.push(e),
            None => groups.push((e.addr.clone(), vec![e])),
        }
    }
    let max_usdc = |g: &[&TradeEvent]| g.iter().map(|e| e.trade.usdc).fold(0.0, f64::max);
    groups.sort_by(|a, b| max_usdc(&b.1).total_cmp(&max_usdc(&a.1)));

    let mut posted = 0usize;
    for (addr, mut group) in groups {
        let rec = alerts.wallets.get(&addr);
        let Some(issue) = group.iter().find_map(|e| e.issue).or(rec.map(|r| r.issue)) else {
            tracing::debug!(%addr, "trade event without an issue");
            continue;
        };
        if let Some(r) = rec {
            if r.closed {
                continue;
            }
            if r.last_comment_ts > 0 && opts.now - r.last_comment_ts < ac.min_comment_interval_s {
                report.capped += 1;
                continue;
            }
        }
        if posted >= ac.max_comments_per_run {
            report.capped += 1;
            continue;
        }
        group.sort_by_key(|e| e.trade.ts);
        let mut seen = BTreeSet::new();
        group.retain(|e| seen.insert(e.trade.key.clone()));
        let body: Vec<String> = group.iter().map(|e| render::trade_line(e, cfg)).collect();
        if !opts.dry_run {
            gh.comment(issue, &body.join("\n"))
                .await
                .with_context(|| format!("commenting trades on issue #{issue} for {addr}"))?;
        }
        posted += 1;
        if let Some(r) = alerts.wallets.get_mut(&addr) {
            r.last_comment_ts = opts.now;
        }
        let mut reasons: Vec<&str> = Vec::new();
        for e in &group {
            if !reasons.contains(&e.reason.as_str()) {
                reasons.push(&e.reason);
            }
        }
        let reason = if group.len() == 1 {
            reasons.join("; ")
        } else {
            format!("{} trades: {}", group.len(), reasons.join("; "))
        };
        report.actions.push(Action::Comment {
            addr,
            issue,
            reason,
        });
    }
    Ok(())
}

pub(crate) async fn legacy_close(
    gh: &dyn GitHubApi,
    cfg: &Config,
    old_label: &str,
    dry_run: bool,
) -> anyhow::Result<usize> {
    let legacy = &cfg.alerts.label_legacy;
    let issues = gh.list_issues(old_label, "open").await?;
    if dry_run || issues.is_empty() {
        return Ok(issues.len());
    }
    gh.ensure_label(legacy, "cccccc", "Issue from the pre-polywatch scraper")
        .await?;
    let mut closed = 0;
    for i in &issues {
        let mut labels = i.labels.clone();
        if !labels.contains(legacy) {
            labels.push(legacy.clone());
        }
        let update = IssueUpdate {
            labels: Some(labels),
            state: Some("closed".into()),
            ..Default::default()
        };
        gh.update_issue(i.number, &update).await.with_context(|| {
            format!(
                "closing legacy issue #{} ({closed} of {} closed so far)",
                i.number,
                issues.len()
            )
        })?;
        closed += 1;
    }
    Ok(closed)
}
