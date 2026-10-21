//! Account linking: detect evidence that different wallets belong to one person, then
//! cluster wallets with union-find over links of confidence ≥ `min_confidence`.
//!
//! Link kinds (confidence from [`types::config::LinkingConfig`]):
//! - `SameOwnerEoa`: two proxies created by the same owner EOA (from `ProxyOwnerMap`
//!   or `Profile::owner_eoa`).
//! - `SameXUsername`, `SameProfileImageId`: identical non-empty values.
//! - `SameNamePseudonym`: identical non-generic `name`, or identical `pseudonym`.
//!   Generic = empty, equals the address, or auto-generated (`Adjective-Noun` pseudonyms are
//!   auto-assigned by Polymarket and are unique per account, so matching them IS evidence;
//!   but names like "" / "0x…" are not).
//! - `SequentialUserId`: `|id_a − id_b| ≤ sequential_id_max_gap` and created within
//!   `sequential_max_created_gap_s`.
//! - `FactoryCoCreation`: proxies created within `factory_cocreation_s` (≈ blocks × 2 s)
//!   by different owners that later trade the same market.
//! - `CoTrade`: same condition + same side within `cotrade_window_s` on ≥
//!   `cotrade_min_occasions` distinct occasions.
//!
//! STUB (WP0): signatures are final, bodies are filled in by WP7.

use types::api::{FactoryLog, FeedTrade};
use types::config::LinkingConfig;
use types::state::ProxyOwnerMap;
use types::{Cluster, Link, Wallet};

pub struct LinkInput<'a> {
    /// Candidate wallets (profiles filled where available).
    pub wallets: &'a [Wallet],
    pub proxy_owner: &'a ProxyOwnerMap,
    /// Factory creations seen this run (for co-creation), oldest first.
    pub factory_logs: &'a [FactoryLog],
    /// Global-feed trades from this run, already restricted to `wallets`.
    pub feed: &'a [FeedTrade],
}

/// Every link between two distinct wallets in `input.wallets`. Deterministic order
/// (sorted by `(a, b, kind)` with `a < b`), no duplicates.
pub fn find_links(_input: &LinkInput<'_>, _cfg: &LinkingConfig) -> Vec<Link> {
    todo!("WP7")
}

/// Union-find over links with `confidence ≥ cfg.min_confidence`. Only clusters with ≥ 2
/// members are returned; members sorted; `id = "c-" + first 10 hex chars of the smallest
/// member` (after `0x`). Each cluster carries the links among its members.
pub fn cluster(_links: &[Link], _cfg: &LinkingConfig) -> Vec<Cluster> {
    todo!("WP7")
}

/// Merge new factory logs into the map and drop entries older than `retention_blocks`
/// before `head_block`.
pub fn update_proxy_owner(_map: &mut ProxyOwnerMap, _logs: &[FactoryLog], _head_block: u64, _retention_blocks: u64) {
    todo!("WP7")
}
