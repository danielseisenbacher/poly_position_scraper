//! Account linking: detect evidence that different wallets belong to one person, then
//! cluster wallets with union-find over links of confidence ≥ `min_confidence`.
//!
//! Link kinds (confidence from [`types::config::LinkingConfig`]):
//! - `SameOwnerEoa`: two proxies created by the same owner EOA (from `ProxyOwnerMap`
//!   or `Profile::owner_eoa`). A wallet whose address *is* another wallet's owner EOA
//!   is linked to it as well.
//! - `SameXUsername`, `SameProfileImageId`: identical non-empty values (X handles are
//!   compared trimmed, case-insensitive, without a leading `@`).
//! - `SameNamePseudonym`: identical non-generic `name` (case-insensitive), or identical
//!   `pseudonym`. Generic = empty, shorter than 3 chars, `0x…`, equal to the address, or
//!   purely numeric (`Adjective-Noun` pseudonyms are auto-assigned by Polymarket and are
//!   unique per account, so matching them IS evidence).
//! - `SequentialUserId`: `|id_a − id_b| ≤ sequential_id_max_gap` and created within
//!   `sequential_max_created_gap_s`.
//! - `FactoryCoCreation`: proxies created within `factory_cocreation_s` (≈ blocks × 2 s)
//!   by different owners that share at least one market among their positions.
//! - `CoTrade`: same condition + same side within `cotrade_window_s` on ≥
//!   `cotrade_min_occasions` distinct occasions spanning ≥ 2 markets. Windows with more
//!   than [`COTRADE_MAX_WALLETS_PER_WINDOW`] distinct wallets are ignored (busy moments
//!   in hot markets are not evidence).
//!
//! Attribute groups (same X handle / image / name) larger than
//! [`MAX_ATTRIBUTE_GROUP`] are skipped: a value shared by that many candidates is a
//! default or a common word, not evidence of one person.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use types::api::{FactoryLog, FeedTrade, Side};
use types::config::LinkingConfig;
use types::state::ProxyOwnerMap;
use types::{Addr, Cluster, Link, LinkKind, Wallet};

/// Co-trade windows with more distinct wallets than this are skipped.
pub const COTRADE_MAX_WALLETS_PER_WINDOW: usize = 50;

/// Same-attribute groups (X handle, profile image, name, pseudonym) with more wallets
/// than this are skipped.
pub const MAX_ATTRIBUTE_GROUP: usize = 50;

/// Polygon block time used to convert `factory_cocreation_s` into blocks.
const POLYGON_BLOCK_S: i64 = 2;

pub struct LinkInput<'a> {
    /// Candidate wallets (profiles filled where available).
    pub wallets: &'a [Wallet],
    pub proxy_owner: &'a ProxyOwnerMap,
    /// Factory creations seen this run (for co-creation), oldest first.
    pub factory_logs: &'a [FactoryLog],
    /// Global-feed trades from this run, already restricted to `wallets`.
    pub feed: &'a [FeedTrade],
}

type LinkKey = (Addr, Addr, LinkKind);

/// Collects links keyed by `(a, b, kind)` with `a < b`; the first evidence for a key wins.
struct Links<'c> {
    cfg: &'c LinkingConfig,
    map: BTreeMap<LinkKey, Link>,
}

impl<'c> Links<'c> {
    fn new(cfg: &'c LinkingConfig) -> Self {
        Links {
            cfg,
            map: BTreeMap::new(),
        }
    }

    fn add(&mut self, x: &Addr, y: &Addr, kind: LinkKind, evidence: impl FnOnce() -> String) {
        if x == y {
            return;
        }
        let (a, b) = if x < y {
            (x.clone(), y.clone())
        } else {
            (y.clone(), x.clone())
        };
        let confidence = confidence(self.cfg, kind);
        self.map
            .entry((a.clone(), b.clone(), kind))
            .or_insert_with(|| Link {
                a,
                b,
                kind,
                confidence,
                evidence: evidence(),
            });
    }

    /// Link every pair within each group (groups of size 2..=`max`).
    fn add_groups<K>(
        &mut self,
        groups: &BTreeMap<K, BTreeSet<&Addr>>,
        kind: LinkKind,
        max: usize,
        evidence: impl Fn(&K) -> String,
    ) {
        for (key, members) in groups {
            if members.len() < 2 || members.len() > max {
                continue;
            }
            let members: Vec<&Addr> = members.iter().copied().collect();
            for (i, x) in members.iter().enumerate() {
                for y in &members[i + 1..] {
                    self.add(x, y, kind, || evidence(key));
                }
            }
        }
    }

    fn into_vec(self) -> Vec<Link> {
        self.map.into_values().collect()
    }
}

fn confidence(cfg: &LinkingConfig, kind: LinkKind) -> f64 {
    match kind {
        LinkKind::SameOwnerEoa => cfg.conf_same_owner,
        LinkKind::SameXUsername => cfg.conf_same_x,
        LinkKind::SameProfileImageId => cfg.conf_same_image,
        LinkKind::SameNamePseudonym => cfg.conf_same_name,
        LinkKind::SequentialUserId => cfg.conf_sequential_id,
        LinkKind::FactoryCoCreation => cfg.conf_factory_cocreation,
        LinkKind::CoTrade => cfg.conf_cotrade,
    }
}

/// Every link between two distinct wallets in `input.wallets`. Deterministic order
/// (sorted by `(a, b, kind)` with `a < b`), no duplicates.
pub fn find_links(input: &LinkInput<'_>, cfg: &LinkingConfig) -> Vec<Link> {
    // Deduplicate wallets by address; the first occurrence wins.
    let mut wallets: BTreeMap<&Addr, &Wallet> = BTreeMap::new();
    for w in input.wallets {
        wallets.entry(&w.addr).or_insert(w);
    }

    let mut links = Links::new(cfg);
    same_owner(&wallets, input.proxy_owner, &mut links);
    same_attributes(&wallets, &mut links);
    sequential_user_ids(&wallets, cfg, &mut links);
    factory_cocreation(&wallets, input.factory_logs, cfg, &mut links);
    cotrade(&wallets, input.feed, cfg, &mut links);
    links.into_vec()
}

fn same_owner(wallets: &BTreeMap<&Addr, &Wallet>, map: &ProxyOwnerMap, links: &mut Links<'_>) {
    let mut by_owner: BTreeMap<&Addr, BTreeSet<&Addr>> = BTreeMap::new();
    for (&addr, w) in wallets {
        let owner = map
            .entries
            .get(addr)
            .map(|(o, _)| o)
            .or_else(|| w.profile.as_ref().and_then(|p| p.owner_eoa.as_ref()));
        if let Some(owner) = owner {
            if owner != addr {
                by_owner.entry(owner).or_default().insert(addr);
            }
        }
    }
    // Owner EOAs are hard evidence, so groups are not capped.
    links.add_groups(&by_owner, LinkKind::SameOwnerEoa, usize::MAX, |o| {
        format!("same owner EOA {}", o.short())
    });
    for (owner, proxies) in &by_owner {
        if wallets.contains_key(owner) {
            for proxy in proxies {
                links.add(owner, proxy, LinkKind::SameOwnerEoa, || {
                    format!("{} is the owner EOA of {}", owner.short(), proxy.short())
                });
            }
        }
    }
}

fn normalize_x(handle: &str) -> Option<String> {
    let h = handle.trim();
    let h = h.strip_prefix('@').unwrap_or(h).trim();
    (!h.is_empty()).then(|| h.to_lowercase())
}

/// `true` for names that carry no identity: empty, very short, `0x…`, the wallet's own
/// address, or purely numeric.
pub fn is_generic_name(name: &str, addr: &Addr) -> bool {
    let n = name.trim();
    n.chars().count() < 3
        || n.get(..2).is_some_and(|p| p.eq_ignore_ascii_case("0x"))
        || n.eq_ignore_ascii_case(addr.as_str())
        || n.chars().all(|c| c.is_ascii_digit())
}

fn same_attributes(wallets: &BTreeMap<&Addr, &Wallet>, links: &mut Links<'_>) {
    let mut by_x: BTreeMap<String, BTreeSet<&Addr>> = BTreeMap::new();
    let mut by_image: BTreeMap<u64, BTreeSet<&Addr>> = BTreeMap::new();
    let mut by_pseudonym: BTreeMap<&str, BTreeSet<&Addr>> = BTreeMap::new();
    let mut by_name: BTreeMap<String, BTreeSet<&Addr>> = BTreeMap::new();
    for (&addr, w) in wallets {
        let Some(p) = &w.profile else { continue };
        if let Some(x) = normalize_x(&p.x_username) {
            by_x.entry(x).or_default().insert(addr);
        }
        if let Some(id) = p.profile_image_id {
            by_image.entry(id).or_default().insert(addr);
        }
        let pseudonym = p.pseudonym.trim();
        if !pseudonym.is_empty() {
            by_pseudonym.entry(pseudonym).or_default().insert(addr);
        }
        if !is_generic_name(&p.name, addr) {
            by_name
                .entry(p.name.trim().to_lowercase())
                .or_default()
                .insert(addr);
        }
    }
    let max = MAX_ATTRIBUTE_GROUP;
    links.add_groups(&by_x, LinkKind::SameXUsername, max, |x| {
        format!("same X handle @{x}")
    });
    links.add_groups(&by_image, LinkKind::SameProfileImageId, max, |id| {
        format!("same profile image id {id}")
    });
    links.add_groups(&by_pseudonym, LinkKind::SameNamePseudonym, max, |p| {
        format!("same pseudonym \"{p}\"")
    });
    links.add_groups(&by_name, LinkKind::SameNamePseudonym, max, |n| {
        format!("same name \"{n}\"")
    });
}

fn sequential_user_ids(
    wallets: &BTreeMap<&Addr, &Wallet>,
    cfg: &LinkingConfig,
    links: &mut Links<'_>,
) {
    let mut rows: Vec<(u64, i64, &Addr)> = wallets
        .iter()
        .filter_map(|(&addr, w)| {
            let p = w.profile.as_ref()?;
            Some((p.user_id?, p.created_at?, addr))
        })
        .collect();
    rows.sort();
    for (i, &(id_a, created_a, a)) in rows.iter().enumerate() {
        for &(id_b, created_b, b) in &rows[i + 1..] {
            if id_b - id_a > cfg.sequential_id_max_gap {
                break;
            }
            let dt = (created_b - created_a).abs();
            if dt <= cfg.sequential_max_created_gap_s {
                links.add(a, b, LinkKind::SequentialUserId, || {
                    format!(
                        "user ids {id_a}/{id_b} (Δ{}), created {dt} s apart",
                        id_b - id_a
                    )
                });
            }
        }
    }
}

fn factory_cocreation(
    wallets: &BTreeMap<&Addr, &Wallet>,
    logs: &[FactoryLog],
    cfg: &LinkingConfig,
    links: &mut Links<'_>,
) {
    // First creation log per candidate proxy.
    let mut created: BTreeMap<&Addr, &FactoryLog> = BTreeMap::new();
    for log in logs {
        if wallets.contains_key(&log.proxy) {
            created.entry(&log.proxy).or_insert(log);
        }
    }
    let mut rows: Vec<&FactoryLog> = created.into_values().collect();
    rows.sort_by(|x, y| (x.block, &x.proxy).cmp(&(y.block, &y.proxy)));

    let window = (cfg.factory_cocreation_s / POLYGON_BLOCK_S).max(0) as u64;
    let markets: HashMap<&Addr, BTreeSet<&str>> = rows
        .iter()
        .map(|l| {
            let w = wallets[&l.proxy];
            (
                &l.proxy,
                w.positions
                    .iter()
                    .map(|p| p.condition_id.as_str())
                    .collect(),
            )
        })
        .collect();

    for (i, x) in rows.iter().enumerate() {
        for y in &rows[i + 1..] {
            let gap = y.block - x.block;
            if gap > window {
                break;
            }
            if x.owner == y.owner {
                continue;
            }
            let (mx, my) = (&markets[&x.proxy], &markets[&y.proxy]);
            let shared = mx.intersection(my).count();
            if shared > 0 {
                links.add(&x.proxy, &y.proxy, LinkKind::FactoryCoCreation, || {
                    format!(
                        "created {gap} blocks apart by different owners, {shared} shared market(s)"
                    )
                });
            }
        }
    }
}

fn side_rank(side: Side) -> u8 {
    match side {
        Side::Buy => 0,
        Side::Sell => 1,
    }
}

/// `(condition_id, side, bucket)` identifying one co-trade occasion.
type Occasion<'a> = (&'a str, u8, i64);

fn cotrade(
    wallets: &BTreeMap<&Addr, &Wallet>,
    feed: &[FeedTrade],
    cfg: &LinkingConfig,
    links: &mut Links<'_>,
) {
    let window = cfg.cotrade_window_s.max(0);
    let bucket_len = window.max(1);

    let mut groups: BTreeMap<(&str, u8), Vec<(i64, &Addr)>> = BTreeMap::new();
    for t in feed {
        if wallets.contains_key(&t.proxy_wallet) {
            groups
                .entry((t.condition_id.as_str(), side_rank(t.side)))
                .or_default()
                .push((t.timestamp, &t.proxy_wallet));
        }
    }

    let mut occasions: BTreeMap<(&Addr, &Addr), BTreeSet<Occasion<'_>>> = BTreeMap::new();
    for (&(condition, side), trades) in &mut groups {
        trades.sort();
        // Sliding window [i, end): trades within `window` seconds after trade i.
        let mut in_window: HashMap<&Addr, usize> = HashMap::new();
        let mut end = 0;
        for i in 0..trades.len() {
            if i > 0 {
                let prev = trades[i - 1].1;
                if let Some(n) = in_window.get_mut(prev) {
                    *n -= 1;
                    if *n == 0 {
                        in_window.remove(prev);
                    }
                }
            }
            end = end.max(i);
            while end < trades.len() && trades[end].0 - trades[i].0 <= window {
                *in_window.entry(trades[end].1).or_default() += 1;
                end += 1;
            }
            if in_window.len() > COTRADE_MAX_WALLETS_PER_WINDOW {
                continue;
            }
            let (ts, me) = trades[i];
            let key = (condition, side, ts.div_euclid(bucket_len));
            for &other in in_window.keys() {
                if other != me {
                    let pair = if me < other { (me, other) } else { (other, me) };
                    occasions.entry(pair).or_default().insert(key);
                }
            }
        }
    }

    for ((a, b), occ) in &occasions {
        let markets: BTreeSet<&str> = occ.iter().map(|o| o.0).collect();
        if occ.len() >= cfg.cotrade_min_occasions as usize && markets.len() >= 2 {
            links.add(a, b, LinkKind::CoTrade, || {
                format!(
                    "{} co-trades (same market and side within {window} s) across {} markets",
                    occ.len(),
                    markets.len()
                )
            });
        }
    }
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        UnionFind {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, x: usize, y: usize) {
        let (rx, ry) = (self.find(x), self.find(y));
        if rx != ry {
            // Smaller index becomes the root: deterministic regardless of link order.
            let (lo, hi) = if rx < ry { (rx, ry) } else { (ry, rx) };
            self.parent[hi] = lo;
        }
    }
}

/// Union-find over links with `confidence ≥ cfg.min_confidence`. Only clusters with ≥ 2
/// members are returned; members sorted; `id = "c-" + first 10 hex chars of the smallest
/// member` (after `0x`). Each cluster carries the links among its members.
///
/// Links below `min_confidence` never merge wallets, but are attached to a cluster as
/// supporting evidence when both endpoints ended up in it. Clusters are sorted by id.
pub fn cluster(links: &[Link], cfg: &LinkingConfig) -> Vec<Cluster> {
    let strong: Vec<&Link> = links
        .iter()
        .filter(|l| l.confidence >= cfg.min_confidence && l.a != l.b)
        .collect();
    let nodes: BTreeSet<&Addr> = strong.iter().flat_map(|l| [&l.a, &l.b]).collect();
    let index: HashMap<&Addr, usize> = nodes.iter().enumerate().map(|(i, &a)| (a, i)).collect();
    let nodes: Vec<&Addr> = nodes.into_iter().collect();

    let mut uf = UnionFind::new(nodes.len());
    for l in &strong {
        uf.union(index[&l.a], index[&l.b]);
    }

    // Root → members; nodes are in address order, so members come out sorted.
    let mut groups: BTreeMap<usize, Vec<Addr>> = BTreeMap::new();
    for (i, &addr) in nodes.iter().enumerate() {
        groups.entry(uf.find(i)).or_default().push(addr.clone());
    }

    let mut clusters: Vec<Cluster> = Vec::new();
    let mut cluster_of: HashMap<&Addr, usize> = HashMap::new();
    for members in groups.into_values().filter(|m| m.len() >= 2) {
        let id = format!("c-{}", &members[0].as_str()[2..12]);
        clusters.push(Cluster {
            id,
            members,
            links: Vec::new(),
        });
    }
    clusters.sort_by(|x, y| x.id.cmp(&y.id));
    for (ci, c) in clusters.iter().enumerate() {
        for m in &c.members {
            cluster_of.insert(m, ci);
        }
    }

    let mut attached: Vec<Vec<Link>> = vec![Vec::new(); clusters.len()];
    for l in links {
        if let (Some(&ca), Some(&cb)) = (cluster_of.get(&l.a), cluster_of.get(&l.b)) {
            if ca == cb && l.a != l.b {
                attached[ca].push(l.clone());
            }
        }
    }
    for (c, mut ls) in clusters.iter_mut().zip(attached) {
        ls.sort_by(|x, y| (&x.a, &x.b, x.kind).cmp(&(&y.a, &y.b, y.kind)));
        c.links = ls;
    }
    clusters
}

/// Merge new factory logs into the map and drop entries older than `retention_blocks`
/// before `head_block`.
pub fn update_proxy_owner(
    map: &mut ProxyOwnerMap,
    logs: &[FactoryLog],
    head_block: u64,
    retention_blocks: u64,
) {
    for log in logs {
        map.entries
            .insert(log.proxy.clone(), (log.owner.clone(), log.block));
    }
    map.entries
        .retain(|_, (_, block)| block.saturating_add(retention_blocks) >= head_block);
}

/// Every proxy in `map` created by `owner`, sorted.
pub fn proxies_of(map: &ProxyOwnerMap, owner: &Addr) -> Vec<Addr> {
    let mut out: Vec<Addr> = map
        .entries
        .iter()
        .filter(|(_, (o, _))| o == owner)
        .map(|(p, _)| p.clone())
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests;
