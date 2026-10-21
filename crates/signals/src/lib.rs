//! Insider-signal scoring. Pure functions, no I/O: everything a signal needs is in
//! [`ScoreInput`], gathered by `discovery`.
//!
//! Each signal looks at one way an informed trader differs from a lucky one (fresh
//! account betting big on longshots, a win rate far above what the entry prices
//! imply, entries right before resolution or before a price move, oversized stakes
//! in thin markets, …) and yields a strength in `0..=1`. Positions in markets shorter
//! than `min_market_duration_h` are ignored entirely; wiped-out open positions
//! ("dust") never count as value but do count as losses.
//!
//! `score = min(100, Σ weight_k · strength_k) × materiality`, where
//! `materiality = clamp(1 + slope·log10(basis / pivot), min, max)` (defaults: $100 → 0.8,
//! $1k → 1.0, $10k+ → 1.2; see [`types::config::SignalConfig`]).
//! Signal rules and thresholds: see the plan and [`types::config::SignalConfig`].

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use types::api::{BiggestWinnerRow, PricePoint, TimePeriod};
use types::config::SignalConfig;
use types::state::MarketCache;
use types::{
    Addr, Cluster, MarketCategory, PosStatus, Position, ResolvedSummary, Score, Signal, SignalKind,
    Tier, Wallet,
};

/// The wallet appeared on a PnL leaderboard this run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaderboardHit {
    pub period: TimePeriod,
    pub rank: u32,
    pub pnl: f64,
}

/// The wallet held the winning token of a recently resolved market that traded as a
/// longshot shortly before close.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LongshotHit {
    pub condition_id: String,
    pub title: String,
    pub token_id: String,
    /// The wallet's average entry price on the winning token.
    pub entry_price: f64,
    /// Lowest price the winning token traded at within the window before close.
    pub low_price_before_close: f64,
    pub closed_at: i64,
    /// Shares held.
    pub amount: f64,
}

pub struct ScoreInput<'a> {
    /// Must have `positions` (all four statuses), `stats`, and `sources` filled.
    /// `profile` is optional at this stage.
    pub wallet: &'a Wallet,
    pub markets: &'a MarketCache,
    pub leaderboard: &'a [LeaderboardHit],
    /// Rows of `/v2/biggest-winners` for this wallet.
    pub biggest_wins: &'a [BiggestWinnerRow],
    pub longshot_hits: &'a [LongshotHit],
    /// token_id → price path, only for tokens requested via [`premove_requests`].
    pub price_paths: &'a HashMap<String, Vec<PricePoint>>,
    pub now: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evaluation {
    pub signals: Vec<Signal>,
    pub score: Score,
    pub resolved: ResolvedSummary,
    /// Total current value of open positions (USDC), excluding dust.
    pub value: f64,
}

/// Evaluate every signal except `LinkedAccounts` (cluster-level, see
/// [`apply_cluster_bonus`]) and combine them into a score.
/// `PreMove` is only produced when `price_paths` contains the needed tokens.
pub fn evaluate(input: &ScoreInput<'_>, cfg: &SignalConfig) -> Evaluation {
    let ctx = Ctx::new(input.wallet, Some(input.markets), cfg);
    let stats = ctx.resolved_stats();
    let value = ctx.value();
    // Market makers and bots trade thousands of markets; none of the signals mean
    // anything for them (and a big z there is volume, not information).
    if input
        .wallet
        .stats
        .as_ref()
        .is_some_and(|s| s.markets_traded > cfg.max_markets_traded)
    {
        return Evaluation {
            signals: Vec::new(),
            score: combine(&[], value, cfg),
            resolved: stats.summary,
            value,
        };
    }

    let mut signals: Vec<Signal> = [
        ctx.new_account_longshot(input.now, value),
        ctx.old_account_win_rate(&stats),
        ctx.leaderboard_entrant(input),
        ctx.longshot_winner(input.longshot_hits),
        ctx.concentration(),
        ctx.resolution_timing(),
        ctx.pre_move(input.price_paths),
        ctx.niche_market(),
    ]
    .into_iter()
    .flatten()
    .collect();
    finish_signals(&mut signals);

    Evaluation {
        score: combine_with_record(&signals, value.max(ctx.won_cost()), &stats.summary, cfg),
        signals,
        resolved: stats.summary,
        value,
    }
}

/// A price-history fetch needed for `PreMove`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PriceRequest {
    pub token_id: String,
    pub start: i64,
    pub end: i64,
}

/// Which price paths to fetch for this wallet, given its provisional evaluation
/// (empty if the provisional signals' ungated `Σ weight · strength` is below
/// `cfg.premove_min_provisional`, so wallets with only supporting context still get
/// checked). At most `cfg.premove_max_positions` requests, largest positions first;
/// sports/crypto positions and positions resolving within the window are skipped.
///
/// Short markets are recognised by `Position::market_duration_h` only (no market
/// cache is available here).
pub fn premove_requests(
    wallet: &Wallet,
    provisional: &Evaluation,
    cfg: &SignalConfig,
) -> Vec<PriceRequest> {
    let evidence: f64 = provisional
        .signals
        .iter()
        .map(|s| weight(s.kind, cfg) * s.strength)
        .sum();
    if evidence < f64::from(cfg.premove_min_provisional) {
        return Vec::new();
    }
    let ctx = Ctx::new(wallet, None, cfg);
    let mut candidates: Vec<&Position> = ctx
        .positions
        .iter()
        .copied()
        .filter(|p| p.status != PosStatus::Closed && ctx.premove_eligible(p))
        .collect();
    candidates.sort_by(|a, b| b.cost.total_cmp(&a.cost));

    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|p| seen.insert(p.token_id.as_str()))
        .take(cfg.premove_max_positions)
        .filter_map(|p| {
            let start = p.first_entry_at?;
            Some(PriceRequest {
                token_id: p.token_id.clone(),
                start,
                end: start + hours(cfg.premove_window_h),
            })
        })
        .collect()
}

/// Win-rate statistics over resolved positions (see `OldAccountWinRate`).
pub fn resolved_summary(
    wallet: &Wallet,
    markets: &MarketCache,
    cfg: &SignalConfig,
) -> ResolvedSummary {
    Ctx::new(wallet, Some(markets), cfg)
        .resolved_stats()
        .summary
}

/// Primary evidence builds the score; the other kinds are supporting context that only
/// amplifies primary evidence (see [`combine_with_record`]).
pub fn is_primary(kind: SignalKind) -> bool {
    matches!(
        kind,
        SignalKind::NewAccountLongshot
            | SignalKind::OldAccountWinRate
            | SignalKind::LongshotWinner
            | SignalKind::PreMove
            | SignalKind::LinkedAccounts
    )
}

/// [`combine_with_record`] without a resolved track record (no losing-record penalty).
pub fn combine(signals: &[Signal], value: f64, cfg: &SignalConfig) -> Score {
    combine_with_record(signals, value, &ResolvedSummary::default(), cfg)
}

/// Combine signals into a [`Score`]:
/// `raw = base + support · min(1, base / support_gate)` where base sums primary
/// `weight · strength` and support the rest; cap 100; × materiality; × the
/// losing-record factor when the wallet has ≥ `min_resolved` resolved positions and
/// `z ≤ losing_z`; tier.
pub fn combine_with_record(
    signals: &[Signal],
    value: f64,
    resolved: &ResolvedSummary,
    cfg: &SignalConfig,
) -> Score {
    let (mut base, mut support) = (0.0, 0.0);
    for s in signals {
        let x = weight(s.kind, cfg) * s.strength;
        if is_primary(s.kind) {
            base += x;
        } else {
            support += x;
        }
    }
    let gate = if cfg.support_gate > 0.0 {
        (base / cfg.support_gate).min(1.0)
    } else {
        1.0
    };
    let raw = base + support * gate;
    let materiality = (1.0
        + cfg.materiality_slope * (value.max(1.0) / cfg.materiality_pivot_usd).log10())
    .clamp(cfg.materiality_min, cfg.materiality_max);
    let record = if resolved.n >= cfg.min_resolved && resolved.z <= cfg.losing_z {
        cfg.losing_record_factor
    } else {
        1.0
    };
    let total = (raw.min(100.0) * materiality * record)
        .round()
        .clamp(0.0, 100.0) as u8;
    let tier = if total >= cfg.tier_high {
        Tier::High
    } else if total >= cfg.tier_medium {
        Tier::Medium
    } else {
        Tier::Low
    };
    Score {
        total,
        tier,
        materiality: round3(materiality),
    }
}

/// Add/refresh the `LinkedAccounts` signal on every wallet that belongs to a cluster
/// with ≥ 2 members, set `wallet.cluster_id`, and recompute `wallet.score`.
/// Strength: `min(1, linked_bonus_per_member · (flagged_members − 1) / w_linked_accounts)`,
/// where flagged members are cluster members scoring ≥ `tier_medium` on their own.
///
/// Wallets in no such cluster lose any `LinkedAccounts` signal and their `cluster_id`.
/// A cluster without a second flagged member sets `cluster_id` but adds no signal.
pub fn apply_cluster_bonus(wallets: &mut [Wallet], clusters: &[Cluster], cfg: &SignalConfig) {
    for w in wallets.iter_mut() {
        w.signals.retain(|s| s.kind != SignalKind::LinkedAccounts);
    }
    let flagged: BTreeSet<Addr> = wallets
        .iter()
        .filter(|w| {
            combine_with_record(&w.signals, materiality_basis(w, cfg), &w.resolved, cfg).total
                >= cfg.tier_medium
        })
        .map(|w| w.addr.clone())
        .collect();
    let cluster_of: HashMap<&Addr, &Cluster> = clusters
        .iter()
        .filter(|c| c.members.len() >= 2)
        .flat_map(|c| c.members.iter().map(move |m| (m, c)))
        .collect();

    for w in wallets.iter_mut() {
        w.cluster_id = None;
        if let Some(c) = cluster_of.get(&w.addr) {
            w.cluster_id = Some(c.id.clone());
            let n_flagged = c.members.iter().filter(|m| flagged.contains(*m)).count();
            let strength = if cfg.w_linked_accounts > 0.0 {
                (cfg.linked_bonus_per_member * n_flagged.saturating_sub(1) as f64
                    / cfg.w_linked_accounts)
                    .min(1.0)
            } else {
                0.0
            };
            if strength > 0.0 {
                let others_flagged = n_flagged - usize::from(flagged.contains(&w.addr));
                w.signals.push(Signal {
                    kind: SignalKind::LinkedAccounts,
                    strength,
                    evidence: format!(
                        "linked to {} wallets ({others_flagged} flagged): {}",
                        c.members.len() - 1,
                        link_kinds(c, &w.addr)
                    ),
                    refs: Vec::new(),
                });
            }
        }
        finish_signals(&mut w.signals);
        w.score = combine_with_record(&w.signals, materiality_basis(w, cfg), &w.resolved, cfg);
    }
}

/// Distinct link kinds touching `addr`, or of the whole cluster if it is only linked transitively.
fn link_kinds(cluster: &Cluster, addr: &Addr) -> String {
    let mut kinds: BTreeSet<_> = cluster
        .links
        .iter()
        .filter(|l| &l.a == addr || &l.b == addr)
        .map(|l| l.kind)
        .collect();
    if kinds.is_empty() {
        kinds = cluster.links.iter().map(|l| l.kind).collect();
    }
    kinds
        .iter()
        .map(|k| format!("{k:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Materiality basis of an already-evaluated wallet (position-level market metadata only).
fn materiality_basis(wallet: &Wallet, cfg: &SignalConfig) -> f64 {
    let ctx = Ctx::new(wallet, None, cfg);
    wallet.value.max(ctx.won_cost())
}

fn weight(kind: SignalKind, cfg: &SignalConfig) -> f64 {
    match kind {
        SignalKind::NewAccountLongshot => cfg.w_new_account_longshot,
        SignalKind::OldAccountWinRate => cfg.w_old_account_win_rate,
        SignalKind::LeaderboardEntrant => cfg.w_leaderboard_entrant,
        SignalKind::LongshotWinner => cfg.w_longshot_winner,
        SignalKind::Concentration => cfg.w_concentration,
        SignalKind::ResolutionTiming => cfg.w_resolution_timing,
        SignalKind::PreMove => cfg.w_pre_move,
        SignalKind::NicheMarket => cfg.w_niche_market,
        SignalKind::LinkedAccounts => cfg.w_linked_accounts,
    }
}

/// Sort by kind and round strengths to 3 decimals.
fn finish_signals(signals: &mut [Signal]) {
    signals.sort_by_key(|s| s.kind);
    for s in signals.iter_mut() {
        s.strength = round3(s.strength);
    }
}

/// [`ResolvedSummary`] plus the share needed for the public-information discount.
struct ResolvedStats {
    summary: ResolvedSummary,
    /// Share of resolved weight in sports/crypto markets.
    share_public: f64,
    mean_entry: f64,
    won_refs: Vec<String>,
}

/// A wallet's positions after excluding short markets, with market metadata lookups.
struct Ctx<'a> {
    wallet: &'a Wallet,
    markets: Option<&'a MarketCache>,
    cfg: &'a SignalConfig,
    positions: Vec<&'a Position>,
}

impl<'a> Ctx<'a> {
    fn new(wallet: &'a Wallet, markets: Option<&'a MarketCache>, cfg: &'a SignalConfig) -> Self {
        let mut ctx = Ctx {
            wallet,
            markets,
            cfg,
            positions: Vec::new(),
        };
        let positions = wallet
            .positions
            .iter()
            .filter(|p| {
                ctx.duration_h(p)
                    .is_none_or(|d| d >= cfg.min_market_duration_h)
            })
            .collect();
        ctx.positions = positions;
        ctx
    }

    fn duration_h(&self, p: &Position) -> Option<f64> {
        p.market_duration_h
            .or_else(|| self.markets?.get(&p.condition_id)?.duration_h())
    }

    fn category(&self, p: &Position) -> MarketCategory {
        match p.category {
            MarketCategory::Other => self
                .markets
                .and_then(|m| m.get(&p.condition_id))
                .map_or(MarketCategory::Other, |m| m.category),
            c => c,
        }
    }

    fn volume(&self, p: &Position) -> Option<f64> {
        p.market_volume
            .or_else(|| self.markets?.get(&p.condition_id)?.volume)
    }

    /// Sports and crypto markets move on public information.
    fn is_public(&self, p: &Position) -> bool {
        matches!(
            self.category(p),
            MarketCategory::Sports | MarketCategory::Crypto
        )
    }

    /// A PreMove check is meaningful only for non-public markets that do not resolve
    /// within the window (a move caused by resolution itself is not a pre-move).
    fn premove_eligible(&self, p: &Position) -> bool {
        let Some(entry) = p.first_entry_at else {
            return false;
        };
        let resolves_in_window = p
            .end_date
            .is_some_and(|end| end + 86_400 - entry <= hours(self.cfg.premove_window_h));
        !self.is_public(p) && !resolves_in_window
    }

    fn is_dust(&self, p: &Position) -> bool {
        p.status == PosStatus::Open && p.percent_pnl <= self.cfg.dust_pnl_pct
    }

    /// Non-dust open positions.
    fn live_open(&self) -> impl Iterator<Item = &'a Position> + '_ {
        self.positions
            .iter()
            .copied()
            .filter(|p| p.status == PosStatus::Open && !self.is_dust(p))
    }

    fn value(&self) -> f64 {
        self.positions
            .iter()
            .filter(|p| {
                matches!(p.status, PosStatus::Open | PosStatus::Redeemable) && !self.is_dust(p)
            })
            .map(|p| p.current_value)
            .sum()
    }

    fn won(&self) -> impl Iterator<Item = &'a Position> + '_ {
        self.positions
            .iter()
            .copied()
            .filter(|p| p.won() == Some(true))
    }

    fn won_cost(&self) -> f64 {
        self.won().map(|p| p.cost).sum()
    }

    /// Resolution weight and outcome; dust counts as a full-weight loss.
    fn resolution(&self, p: &Position) -> Option<(f64, bool)> {
        if self.is_dust(p) {
            return Some((1.0, false));
        }
        let won = p.won()?;
        let w = if p.status == PosStatus::Closed {
            self.cfg.closed_weight
        } else {
            1.0
        };
        Some((w, won))
    }

    fn resolved_stats(&self) -> ResolvedStats {
        let mut s = ResolvedSummary::default();
        let (mut var, mut public, mut entry) = (0.0, 0.0, 0.0);
        let mut won_refs = Vec::new();
        for p in &self.positions {
            let Some((w, won)) = self.resolution(p) else {
                continue;
            };
            let price = p.avg_price.clamp(0.01, 0.99);
            s.n += w;
            s.expected_wins += w * price;
            var += w * price * (1.0 - price);
            entry += w * p.avg_price;
            if matches!(
                self.category(p),
                MarketCategory::Sports | MarketCategory::Crypto
            ) {
                public += w;
            }
            if won {
                s.wins += w;
                push_ref(&mut won_refs, &p.condition_id);
                if p.avg_price <= self.cfg.longshot_win_price {
                    s.longshot_wins += 1;
                }
            }
        }
        if var > 0.0 {
            s.z = round3((s.wins - s.expected_wins) / var.sqrt());
        }
        let ratio = |x: f64| if s.n > 0.0 { x / s.n } else { 0.0 };
        ResolvedStats {
            share_public: ratio(public),
            mean_entry: ratio(entry),
            summary: s,
            won_refs,
        }
    }

    fn new_account_longshot(&self, now: i64, value: f64) -> Option<Signal> {
        let cfg = self.cfg;
        let age = self
            .wallet
            .age_days(now)
            .filter(|a| *a <= cfg.new_account_days)?;
        let ps: Vec<&Position> = self
            .positions
            .iter()
            .copied()
            .filter(|p| {
                matches!(
                    p.status,
                    PosStatus::Open | PosStatus::Redeemable | PosStatus::RedeemableLost
                )
            })
            .collect();
        let refs = distinct_refs(&ps);
        let cost: f64 = ps.iter().map(|p| p.cost).sum();
        // Markets ever traded (user-stats), not just the ones still held: a hyperactive
        // trader with one open position is not a fresh account placing one big bet.
        let markets = self
            .wallet
            .stats
            .as_ref()
            .map_or(0, |s| s.markets_traded)
            .max(refs.len() as u64);
        if value < cfg.min_value_usd || markets > u64::from(cfg.max_markets_new) || cost <= 0.0 {
            return None;
        }
        let mean_p = ps.iter().map(|p| p.cost * p.avg_price).sum::<f64>() / cost;
        if mean_p > cfg.longshot_price {
            return None;
        }
        Some(Signal {
            kind: SignalKind::NewAccountLongshot,
            strength: 0.5 + 0.5 * (cfg.longshot_price - mean_p) / cfg.longshot_price,
            evidence: format!(
                "account {age:.0} d old, {} traded, cost-weighted entry {mean_p:.2}, value {}",
                plural(markets as usize, "market"),
                usd(value)
            ),
            refs,
        })
    }

    fn old_account_win_rate(&self, stats: &ResolvedStats) -> Option<Signal> {
        let cfg = self.cfg;
        let s = &stats.summary;
        let selected = self.wallet.sources.iter().any(|src| src.outcome_selected());
        let min_n = if selected {
            cfg.min_resolved_selected
        } else {
            cfg.min_resolved
        };
        if s.n < min_n || s.longshot_wins < cfg.min_longshot_wins {
            return None;
        }
        let edge = (s.wins - s.expected_wins) / s.n;
        if edge < cfg.min_edge {
            return None;
        }
        let z_eff = if selected {
            s.z - cfg.outcome_selected_z_penalty
        } else {
            s.z
        };
        let discount = 1.0 - (1.0 - cfg.public_info_discount) * stats.share_public;
        let strength = ((z_eff - cfg.z_floor) / cfg.z_span).clamp(0.0, 1.0) * discount;
        if strength <= 0.0 {
            return None;
        }
        let mut evidence = format!(
            "won {}/{} resolved, mean entry {:.2}, edge +{:.0} pts, z={:.1}",
            num(s.wins),
            num(s.n),
            stats.mean_entry,
            edge * 100.0,
            s.z
        );
        if selected {
            evidence.push_str(&format!(" (outcome-selected, z_eff={z_eff:.1})"));
        }
        if stats.share_public > 0.0 {
            evidence.push_str(&format!(
                ", {:.0}% sports/crypto",
                stats.share_public * 100.0
            ));
        }
        Some(Signal {
            kind: SignalKind::OldAccountWinRate,
            strength,
            evidence,
            refs: stats.won_refs.clone(),
        })
    }

    fn leaderboard_entrant(&self, input: &ScoreInput<'_>) -> Option<Signal> {
        let cfg = self.cfg;
        let young = self
            .wallet
            .age_days(input.now)
            .is_some_and(|a| a <= cfg.leaderboard_max_age_days);
        let few_markets = self
            .wallet
            .stats
            .as_ref()
            .is_some_and(|s| s.markets_traded <= cfg.leaderboard_max_markets);
        // A good week for an all-time loser is luck, not information.
        let profitable = self
            .wallet
            .stats
            .as_ref()
            .is_none_or(|s| s.realized_pnl >= 0.0);
        let best_rank = input
            .leaderboard
            .iter()
            .filter(|h| h.rank <= cfg.leaderboard_max_rank && (young || few_markets) && profitable)
            .min_by_key(|h| h.rank);
        let wins: Vec<(&BiggestWinnerRow, f64)> = input
            .biggest_wins
            .iter()
            .filter(|r| r.initial_value >= cfg.biggest_win_min_initial && r.initial_value > 0.0)
            .map(|r| (r, r.pnl / r.initial_value))
            .filter(|(_, m)| *m >= cfg.biggest_win_min_multiple)
            .collect();
        let best_win = wins.iter().max_by(|a, b| a.1.total_cmp(&b.1));

        let mut strength: f64 = 0.0;
        let mut parts = Vec::new();
        let mut refs = Vec::new();
        if let Some(h) = best_rank {
            strength = 1.0 - 0.5 * h.rank as f64 / (cfg.leaderboard_max_rank as f64 + 1.0);
            parts.push(format!(
                "rank {} on {} PnL leaderboard ({})",
                h.rank,
                period(h.period),
                usd(h.pnl)
            ));
        }
        if let Some((r, multiple)) = best_win {
            let s2 = 0.6 + 0.4 * ((multiple - cfg.biggest_win_min_multiple) / 15.0).min(1.0);
            strength = strength.max(s2);
            parts.push(format!(
                "biggest win {multiple:.1}x on {}",
                usd(r.initial_value)
            ));
            for (r, _) in &wins {
                push_ref(&mut refs, &r.condition_id);
            }
        }
        (strength > 0.0).then(|| Signal {
            kind: SignalKind::LeaderboardEntrant,
            strength,
            evidence: parts.join("; "),
            refs,
        })
    }

    fn longshot_winner(&self, hits: &[LongshotHit]) -> Option<Signal> {
        let max_entry = self.cfg.longshot_holder_max_entry;
        let hits: Vec<&LongshotHit> = hits.iter().filter(|h| h.entry_price <= max_entry).collect();
        let best = hits
            .iter()
            .min_by(|a, b| a.entry_price.total_cmp(&b.entry_price))?;
        let max_s = 0.6 + 0.4 * (1.0 - best.entry_price / max_entry);
        let mut refs = Vec::new();
        for h in &hits {
            push_ref(&mut refs, &h.condition_id);
        }
        Some(Signal {
            kind: SignalKind::LongshotWinner,
            strength: (max_s + 0.2 * (hits.len() - 1) as f64).min(1.0),
            evidence: format!(
                "held {}, best entry {:.2} (low {:.2} before close)",
                plural(hits.len(), "longshot winner"),
                best.entry_price,
                best.low_price_before_close
            ),
            refs,
        })
    }

    fn concentration(&self) -> Option<Signal> {
        let cfg = self.cfg;
        let open: Vec<&Position> = self.live_open().collect();
        let total: f64 = open.iter().map(|p| p.current_value).sum();
        let markets = distinct_refs(&open).len();
        let top = open
            .iter()
            .max_by(|a, b| a.current_value.total_cmp(&b.current_value))?;
        if total < cfg.min_value_usd
            || total <= 0.0
            || markets > cfg.concentration_max_markets as usize
        {
            return None;
        }
        let share = top.current_value / total;
        if share < cfg.concentration_share {
            return None;
        }
        let strength = if cfg.concentration_share < 1.0 {
            ((share - cfg.concentration_share) / (1.0 - cfg.concentration_share)).clamp(0.3, 1.0)
        } else {
            1.0
        };
        Some(Signal {
            kind: SignalKind::Concentration,
            strength,
            evidence: format!(
                "{:.0}% of {} open value in one position, {}",
                share * 100.0,
                usd(total),
                plural(markets, "market")
            ),
            refs: vec![top.condition_id.clone()],
        })
    }

    fn resolution_timing(&self) -> Option<Signal> {
        let cfg = self.cfg;
        let window = hours(cfg.timing_window_h);
        // Sports games are bet on game day by everyone: timing there is not evidence.
        let won: Vec<&Position> = self.won().filter(|p| !self.is_public(p)).collect();
        let timed: Vec<&Position> = won
            .iter()
            .copied()
            .filter(|p| {
                let (Some(entry), Some(end)) = (p.first_entry_at, p.end_date) else {
                    return false;
                };
                (0..=window).contains(&(end + 86_400 - entry))
                    && p.avg_price <= cfg.timing_max_price
            })
            .collect();
        if timed.is_empty() || timed.len() < cfg.timing_min_wins as usize {
            return None;
        }
        Some(Signal {
            kind: SignalKind::ResolutionTiming,
            strength: timed.len() as f64 / won.len() as f64,
            evidence: format!(
                "{}/{} wins entered <= {}h before close at <= {:.2}",
                timed.len(),
                won.len(),
                num(cfg.timing_window_h),
                cfg.timing_max_price
            ),
            refs: distinct_refs(&timed),
        })
    }

    fn pre_move(&self, paths: &HashMap<String, Vec<PricePoint>>) -> Option<Signal> {
        let cfg = self.cfg;
        let window = hours(cfg.premove_window_h);
        let (mut checked, mut moved, mut best_rise) = (0usize, Vec::new(), f64::NEG_INFINITY);
        for p in &self.positions {
            let (Some(path), Some(entry)) = (paths.get(&p.token_id), p.first_entry_at) else {
                continue;
            };
            if !self.premove_eligible(p) {
                continue;
            }
            checked += 1;
            let high = path
                .iter()
                .filter(|pt| pt.timestamp > entry && pt.timestamp <= entry + window)
                .map(|pt| pt.price)
                .fold(f64::NEG_INFINITY, f64::max);
            let rise = high - p.avg_price;
            if rise >= cfg.premove_delta {
                best_rise = best_rise.max(rise);
                moved.push(*p);
            }
        }
        // One lucky move among many checked positions is noise.
        if moved.is_empty() || moved.len() < checked.min(2) {
            return None;
        }
        Some(Signal {
            kind: SignalKind::PreMove,
            strength: moved.len() as f64 / checked as f64,
            evidence: format!(
                "{}/{checked} checked positions rose >= {:.2} within {}h of entry (max +{best_rise:.2})",
                moved.len(),
                cfg.premove_delta,
                num(cfg.premove_window_h)
            ),
            refs: distinct_refs(&moved),
        })
    }

    fn niche_market(&self) -> Option<Signal> {
        let cfg = self.cfg;
        let mut best: Option<(f64, f64, f64)> = None;
        let mut refs = Vec::new();
        for p in &self.positions {
            let Some(vol) = self
                .volume(p)
                .filter(|v| *v >= cfg.niche_min_volume && *v <= cfg.niche_max_volume)
            else {
                continue;
            };
            // Gamma volume is CLOB-only: a position larger than it means the volume is wrong.
            if p.cost < cfg.niche_min_share * vol
                || p.cost < cfg.min_value_usd / 4.0
                || p.cost > vol
                || p.avg_price > cfg.niche_max_entry
                || self.is_public(p)
            {
                continue;
            }
            let share = p.cost / vol;
            push_ref(&mut refs, &p.condition_id);
            if best.is_none_or(|(s, _, _)| share > s) {
                best = Some((share, p.cost, vol));
            }
        }
        let (share, cost, vol) = best?;
        let mut evidence = format!(
            "{} = {:.0}% of a {}-volume market",
            usd(cost),
            share * 100.0,
            usd(vol)
        );
        if refs.len() > 1 {
            evidence.push_str(&format!(
                " (+{})",
                plural(refs.len() - 1, "more niche market")
            ));
        }
        Some(Signal {
            kind: SignalKind::NicheMarket,
            strength: (0.5 + 5.0 * share).clamp(0.5, 1.0),
            evidence,
            refs,
        })
    }
}

fn distinct_refs(ps: &[&Position]) -> Vec<String> {
    let mut refs = Vec::new();
    for p in ps {
        push_ref(&mut refs, &p.condition_id);
    }
    refs
}

fn push_ref(refs: &mut Vec<String>, cid: &str) {
    if !refs.iter().any(|r| r == cid) {
        refs.push(cid.to_string());
    }
}

fn hours(h: f64) -> i64 {
    (h * 3600.0).round() as i64
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

fn period(p: TimePeriod) -> &'static str {
    match p {
        TimePeriod::Day => "day",
        TimePeriod::Week => "week",
        TimePeriod::Month => "month",
        TimePeriod::All => "all-time",
    }
}

fn plural(n: usize, word: &str) -> String {
    if n == 1 {
        format!("1 {word}")
    } else {
        format!("{n} {word}s")
    }
}

/// Integer without decimals, otherwise one decimal (CLOSED positions count fractionally).
fn num(x: f64) -> String {
    if x.fract().abs() < 1e-9 {
        format!("{x:.0}")
    } else {
        format!("{x:.1}")
    }
}

/// `$1,234`, `$45.6k`, `$1.2M`.
fn usd(x: f64) -> String {
    let sign = if x < 0.0 { "-" } else { "" };
    let a = x.abs();
    if a >= 1e6 {
        format!("{sign}${:.1}M", a / 1e6)
    } else if a >= 1e4 {
        format!("{sign}${:.1}k", a / 1e3)
    } else {
        let n = a.round() as u64;
        if n >= 1000 {
            format!("{sign}${},{:03}", n / 1000, n % 1000)
        } else {
            format!("{sign}${n}")
        }
    }
}

#[cfg(test)]
mod tests;
