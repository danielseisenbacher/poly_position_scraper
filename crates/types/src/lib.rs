//! Shared contracts for every polywatch crate.
//!
//! - [`addr`]: validated wallet address.
//! - [`api`]: rows returned by Polymarket Data API v2, Gamma and Polygon RPC, plus the
//!   [`api::PolyApi`] trait every API consumer codes against (mock it in tests).
//! - [`domain`]: the scored-wallet model persisted on the `data` branch.
//! - [`state`]: the other files on the `data` branch (watchlist, alerts, cursors, caches).
//! - [`site`]: the JSON files the GitHub Pages dashboard reads.
//! - [`config`]: every tunable threshold, with the defaults from the plan.
//!
//! All timestamps are unix seconds (`i64`, UTC). All money is USD(C) as `f64`.
//! Prices are probabilities in `0.0..=1.0`.

pub mod addr;
pub mod api;
pub mod config;
pub mod domain;
pub mod site;
pub mod state;

pub use addr::{Addr, AddrError};
pub use config::Config;
pub use domain::*;
