//! Markdown for issues and comments. Pure functions (snapshot-tested).

use std::collections::BTreeSet;

use types::{Addr, Config, Position, Signal, SignalKind, Tier, TradeEvent, TradeKind, Wallet};

/// Signals at or above this strength get a `sig:<slug>` label and count as "new
/// signal kinds" for rescore comments.
pub const SIG_LABEL_MIN: f64 = 0.3;
/// Start of the hidden marker every issue body ends with.
pub const MARKER_PREFIX: &str = "<!-- polywatch:addr=";
const TOP_POSITIONS: usize = 10;

/// `<!-- polywatch:addr=0x… -->`.
pub fn marker(addr: &Addr) -> String {
    format!("{MARKER_PREFIX}{addr} -->")
}

/// The address in an issue body's marker, if any.
pub fn parse_marker(body: &str) -> Option<Addr> {
    let start = body.rfind(MARKER_PREFIX)? + MARKER_PREFIX.len();
    let rest = &body[start..];
    let end = rest.find("-->")?;
    Addr::parse(rest[..end].trim()).ok()
}

pub(crate) fn sig_label(kind: SignalKind) -> String {
    format!("sig:{}", kind.slug())
}

pub(crate) fn kind_from_label(label: &str) -> Option<SignalKind> {
    let slug = label.strip_prefix("sig:")?;
    SignalKind::ALL.into_iter().find(|k| k.slug() == slug)
}

/// Signal kinds strong enough to be labelled, sorted and deduplicated.
pub(crate) fn labeled_kinds(w: &Wallet) -> Vec<SignalKind> {
    let set: BTreeSet<SignalKind> = w
        .signals
        .iter()
        .filter(|s| s.strength >= SIG_LABEL_MIN)
        .map(|s| s.kind)
        .collect();
    set.into_iter().collect()
}

/// Signals by contribution to the score (weight × strength), so the evidence that
/// drives the score comes before supporting context.
fn signals_by_contribution<'w>(w: &'w Wallet, cfg: &Config) -> Vec<&'w Signal> {
    let c = |s: &Signal| cfg.signals.weight(s.kind) * s.strength;
    let mut s: Vec<&Signal> = w.signals.iter().collect();
    s.sort_by(|a, b| c(b).total_cmp(&c(a)).then(a.kind.cmp(&b.kind)));
    s
}

fn tier_label(tier: Tier) -> Option<&'static str> {
    match tier {
        Tier::High => Some("tier:high"),
        Tier::Medium => Some("tier:medium"),
        Tier::Low => None,
    }
}

pub(crate) fn title(w: &Wallet, cfg: &Config) -> String {
    let top: Vec<String> = signals_by_contribution(w, cfg)
        .iter()
        .take(2)
        .map(|s| s.kind.to_string())
        .collect();
    let mut t = format!("[candidate] {} · score {}", w.addr.short(), w.score.total);
    if !top.is_empty() {
        t.push_str(" · ");
        t.push_str(&top.join("+"));
    }
    t
}

pub(crate) fn labels(w: &Wallet, cfg: &Config) -> Vec<String> {
    let mut l = vec![cfg.alerts.label_candidate.clone()];
    if let Some(t) = tier_label(w.score.tier) {
        l.push(t.to_string());
    }
    l.extend(labeled_kinds(w).into_iter().map(sig_label));
    if w.cluster_id.is_some() {
        l.push("cluster".to_string());
    }
    l
}

pub(crate) fn issue(w: &Wallet, cfg: &Config) -> (String, String, Vec<String>) {
    (title(w, cfg), body(w, cfg), labels(w, cfg))
}

fn body(w: &Wallet, cfg: &Config) -> String {
    let a = &w.addr;
    let mut out = String::new();

    out.push_str(&format!(
        "**[{short}](https://polymarket.com/profile/{a})** · [dashboard]({dash}#w/{a}) · \
         [polygonscan](https://polygonscan.com/address/{a})\n\n",
        short = a.short(),
        dash = cfg.repo.dashboard_url,
    ));

    // Profile / stats line.
    let tier = match w.score.tier {
        Tier::High => "high",
        Tier::Medium => "medium",
        Tier::Low => "low",
    };
    let mut facts = vec![format!("**score {}** ({tier})", w.score.total)];
    if let Some(p) = &w.profile {
        let mut who = Vec::new();
        if !p.name.is_empty() {
            who.push(code_span(&p.name));
        }
        if !p.pseudonym.is_empty() && p.pseudonym != p.name {
            who.push(code_span(&p.pseudonym));
        }
        if !who.is_empty() {
            facts.push(who.join(" / "));
        }
    }
    facts.push(match w.age_days(w.last_scored) {
        Some(d) => format!("age {:.0} d", d.floor()),
        None => "age unknown".into(),
    });
    facts.push(format!("value {}", usd(w.value)));
    if let Some(s) = &w.stats {
        facts.push(format!("realized PnL {}", signed_usd(s.realized_pnl)));
    }
    let r = &w.resolved;
    facts.push(if r.n > 0.0 {
        format!("win rate {}/{} (z={:.1})", count(r.wins), count(r.n), r.z)
    } else {
        "win rate n/a".into()
    });
    out.push_str(&facts.join(" · "));
    out.push_str("\n\n");

    // Signals.
    let sigs = signals_by_contribution(w, cfg);
    if !sigs.is_empty() {
        out.push_str("| signal | strength | evidence |\n|---|---|---|\n");
        for s in sigs {
            out.push_str(&format!(
                "| {} | {:.2} | {} |\n",
                s.kind,
                s.strength,
                cell(&s.evidence)
            ));
        }
        out.push('\n');
    }

    // Positions.
    let mut pos: Vec<&Position> = w.positions.iter().collect();
    pos.sort_by(|a, b| {
        b.cost
            .total_cmp(&a.cost)
            .then_with(|| a.title.cmp(&b.title))
    });
    if !pos.is_empty() {
        let shown = pos.len().min(TOP_POSITIONS);
        out.push_str(&format!(
            "**Top {shown} of {} positions by cost**\n\n",
            pos.len()
        ));
        out.push_str(
            "| market | outcome | status | avg price | size | current price | pnl |\n\
             |---|---|---|---|---|---|---|\n",
        );
        for p in pos.into_iter().take(TOP_POSITIONS) {
            let market = if p.slug.is_empty() {
                cell(&p.title)
            } else {
                format!(
                    "[{}](https://polymarket.com/market/{})",
                    link_text(&p.title),
                    p.slug
                )
            };
            out.push_str(&format!(
                "| {market} | {} | {} | {} | {} | {} | {} |\n",
                cell(&p.outcome),
                status(p),
                price(p.avg_price),
                shares(p.total_size),
                price(p.current_price),
                signed_usd(p.total_pnl),
            ));
        }
        out.push('\n');
    }

    if let Some(c) = &w.cluster_id {
        out.push_str(&format!(
            "**Cluster** `{}`: linked accounts, see the [dashboard]({}#w/{a}).\n\n",
            c.replace('`', ""),
            cfg.repo.dashboard_url
        ));
    }

    out.push_str("_Heuristic lead, not proof of insider trading._\n\n");
    out.push_str(&marker(a));
    out
}

fn status(p: &Position) -> &'static str {
    match p.won() {
        Some(true) => "won",
        Some(false) => "lost",
        None => match p.status {
            types::PosStatus::Open => "open",
            _ => "closed",
        },
    }
}

/// One line per trade, e.g.
/// `- 🟢 BUY **$1,250** · Yes · Will X happen? @ 0.21 · 2026-10-21 14:03 UTC · [tx](…) · reason`.
pub(crate) fn trade_line(e: &TradeEvent, _cfg: &Config) -> String {
    let t = &e.trade;
    let kind = match t.kind {
        TradeKind::Buy => "🟢 BUY",
        TradeKind::Sell => "🔴 SELL",
        TradeKind::Redeem => "💰 REDEEM",
        TradeKind::Split => "✂️ SPLIT",
        TradeKind::Merge => "🔀 MERGE",
        TradeKind::Deposit => "⬇️ DEPOSIT",
        TradeKind::Withdraw => "⬆️ WITHDRAW",
        TradeKind::Other => "⚪ OTHER",
    };
    let mut parts = vec![format!("{kind} **{}**", usd(t.usdc))];
    if !t.outcome.is_empty() {
        parts.push(cell(&t.outcome));
    }
    let mut market = cell(&t.title);
    if t.price > 0.0 {
        market.push_str(&format!(" @ {}", price(t.price)));
    }
    if !market.is_empty() {
        parts.push(market);
    }
    parts.push(format!("{} UTC", utc(t.ts)));
    if !t.tx.is_empty() {
        parts.push(format!("[tx](https://polygonscan.com/tx/{})", t.tx));
    }
    if !e.reason.is_empty() {
        parts.push(cell(&e.reason));
    }
    format!("- {}", parts.join(" · "))
}

// ---- formatting helpers ----

/// Text for a table cell / inline text: no pipes, no newlines, no HTML, no @mentions.
pub(crate) fn cell(s: &str) -> String {
    s.replace(['\r', '\n'], " ")
        .replace('|', "\\|")
        .replace('<', "&lt;")
        .replace('@', "@\u{200B}")
        .trim()
        .to_string()
}

fn link_text(s: &str) -> String {
    cell(s).replace('[', "\\[").replace(']', "\\]")
}

/// Inline code: user-chosen names cannot ping anyone or inject markdown.
fn code_span(s: &str) -> String {
    format!("`{}`", s.replace(['`', '\n', '\r'], ""))
}

fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn abs_usd(x: f64) -> String {
    let a = x.abs();
    if a < 100.0 && a.fract() != 0.0 {
        format!("${a:.2}")
    } else {
        format!("${}", thousands(a.round() as u64))
    }
}

/// `$12,345`, `$5.25`, `-$300`.
pub(crate) fn usd(x: f64) -> String {
    let a = abs_usd(x);
    if x < 0.0 && !is_zero(&a) {
        format!("-{a}")
    } else {
        a
    }
}

fn is_zero(formatted: &str) -> bool {
    formatted == "$0" || formatted == "$0.00"
}

/// `+$543`, `-$1,234`, `$0`.
pub(crate) fn signed_usd(x: f64) -> String {
    let a = abs_usd(x);
    if is_zero(&a) {
        "$0".into()
    } else if x > 0.0 {
        format!("+{a}")
    } else {
        format!("-{a}")
    }
}

fn price(p: f64) -> String {
    if p > 0.0 && p < 0.1 {
        format!("{p:.3}")
    } else {
        format!("{p:.2}")
    }
}

fn shares(x: f64) -> String {
    if x.abs() < 10.0 && x.fract() != 0.0 {
        format!("{x:.2}")
    } else {
        thousands(x.abs().round() as u64)
    }
}

/// Whole numbers without decimals, else one decimal (win counts may be fractional).
fn count(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{x:.0}")
    } else {
        format!("{x:.1}")
    }
}

/// `YYYY-MM-DD HH:MM` (UTC).
pub(crate) fn utc(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        secs / 3600,
        secs % 3600 / 60
    )
}
