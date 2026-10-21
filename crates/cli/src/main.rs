//! `polywatch`: Polymarket insider-candidate detector.
//!
//! Layout on disk (what the workflows check out / download before running):
//! - `--data-dir` (default `data/`): the orphan `data` branch.
//! - `--state-dir` (default `state/`): release `state-latest` assets
//!   (`evaluated.bin`, `proxy_owner.bin`, `trades_archive/`).
//!
//! Git commits and release uploads are done by the workflows, not by this binary.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use store::{DataDir, MetaInput};
use types::api::PolyApi;
use types::{Addr, Config};

#[derive(Parser)]
#[command(name = "polywatch", version, about)]
struct Cli {
    #[arg(long, global = true, default_value = "polywatch.toml")]
    config: PathBuf,
    #[arg(long, global = true, default_value = "data")]
    data_dir: PathBuf,
    #[arg(long, global = true, default_value = "state")]
    state_dir: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Find and score candidate wallets; update watchlist, clusters and dashboard files.
    Discover {
        #[arg(long)]
        dry_run: bool,
        /// File with one address per line to evaluate regardless of TTL.
        #[arg(long)]
        seed_file: Option<PathBuf>,
        /// `i/n`: enrich only shard i of n.
        #[arg(long)]
        shard: Option<String>,
        /// Write the JSON report here (also printed to stdout).
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Poll watchlist wallets for new trades; comment on issues for big trades.
    Track {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        report: Option<PathBuf>,
    },
    /// Open/update/close one GitHub issue per flagged wallet.
    Alert {
        #[arg(long)]
        dry_run: bool,
    },
    /// Rebuild the dashboard JSON files from the data directory.
    RenderSite,
    /// Prune stale wallet files from the data directory.
    Compact,
    /// Close every open issue labelled `--old-label` and tag it `legacy`.
    LegacyClose {
        #[arg(long, default_value = "insider-alert")]
        old_label: String,
        #[arg(long)]
        dry_run: bool,
    },
}

const EVALUATED: &str = "evaluated.bin";
const PROXY_OWNER: &str = "proxy_owner.bin";
const ARCHIVE_DIR: &str = "trades_archive";

fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn run_id() -> Option<String> {
    std::env::var("GITHUB_RUN_ID").ok()
}

fn github() -> Result<alerts::GitHubClient> {
    let token = std::env::var("GITHUB_TOKEN").context("GITHUB_TOKEN is not set")?;
    let repo = std::env::var("GITHUB_REPOSITORY").context("GITHUB_REPOSITORY is not set")?;
    Ok(alerts::GitHubClient::new(token, repo))
}

fn read_seed(path: &Path) -> Result<Vec<Addr>> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(text.lines().filter_map(|l| Addr::parse(l).ok()).collect())
}

fn parse_shard(s: &str) -> Result<(u32, u32)> {
    let (i, n) = s.split_once('/').context("--shard must look like i/n")?;
    let (i, n): (u32, u32) = (i.parse()?, n.parse()?);
    if n == 0 || i >= n {
        bail!("--shard {s}: need 0 <= i < n");
    }
    Ok((i, n))
}

fn emit_report<T: serde::Serialize>(report: &T, path: Option<&Path>) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    println!("{json}");
    if let Some(p) = path {
        std::fs::write(p, &json).with_context(|| format!("writing {}", p.display()))?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let cfg = Config::load(&cli.config)?;
    let data = DataDir::open(&cli.data_dir)?;
    std::fs::create_dir_all(&cli.state_dir)?;

    match cli.cmd {
        Cmd::Discover { dry_run, seed_file, shard, report } => {
            let api = api::Client::new(&cfg.api)?;
            let mut assets = discovery::StateAssets {
                evaluated: store::bin::load_evaluated(&cli.state_dir.join(EVALUATED))?,
                proxy_owner: store::bin::load_proxy_owner(&cli.state_dir.join(PROXY_OWNER))?,
            };
            let opts = discovery::DiscoverOpts {
                dry_run,
                seed: seed_file.as_deref().map(read_seed).transpose()?.unwrap_or_default(),
                shard: shard.as_deref().map(parse_shard).transpose()?,
                now: now(),
            };
            let rep = discovery::run(&api, &data, &mut assets, &cfg, &opts).await?;
            if !dry_run {
                store::bin::save_evaluated(&cli.state_dir.join(EVALUATED), &assets.evaluated)?;
                store::bin::save_proxy_owner(&cli.state_dir.join(PROXY_OWNER), &assets.proxy_owner)?;
                store::render_site(
                    &data,
                    &cfg,
                    &MetaInput {
                        now: opts.now,
                        run_id: run_id(),
                        api: api.stats(),
                        feed_lag_s: rep.feed_lag_s,
                        feed_truncated: rep.feed_truncated,
                        wallets_scanned: rep.enriched,
                        trades_tracked: 0,
                    },
                )?;
            }
            emit_report(&rep, report.as_deref())?;
        }
        Cmd::Track { dry_run, report } => {
            let api = api::Client::new(&cfg.api)?;
            let opts = tracker::TrackOpts { dry_run, now: now() };
            let rep = tracker::run(&api, &data, &cfg, &opts).await?;
            if !dry_run {
                store::archive::append(&cli.state_dir.join(ARCHIVE_DIR), &rep.archive)?;
                if !rep.events.is_empty() {
                    match github() {
                        Ok(gh) => {
                            let aopts = alerts::AlertOpts { dry_run, now: opts.now };
                            alerts::comment_trades(&gh, &data, &cfg, &rep.events, &aopts).await?;
                        }
                        Err(e) => tracing::warn!("skipping trade comments: {e:#}"),
                    }
                }
                let meta = data.read::<types::site::Meta>(store::paths::META)?.unwrap_or_default();
                store::render_site(
                    &data,
                    &cfg,
                    &MetaInput {
                        now: opts.now,
                        run_id: run_id(),
                        api: api.stats(),
                        feed_lag_s: meta.feed_lag_s,
                        feed_truncated: meta.feed_truncated,
                        wallets_scanned: meta.counts.wallets_scanned_last_run,
                        trades_tracked: rep.new_trades,
                    },
                )?;
            }
            emit_report(&rep, report.as_deref())?;
        }
        Cmd::Alert { dry_run } => {
            let gh = github()?;
            let rep = alerts::sync(&gh, &data, &cfg, &alerts::AlertOpts { dry_run, now: now() }).await?;
            emit_report(&rep, None)?;
        }
        Cmd::RenderSite => {
            let meta = data.read::<types::site::Meta>(store::paths::META)?.unwrap_or_default();
            let summary = store::render_site(
                &data,
                &cfg,
                &MetaInput {
                    now: now(),
                    run_id: run_id(),
                    feed_lag_s: meta.feed_lag_s,
                    feed_truncated: meta.feed_truncated,
                    ..Default::default()
                },
            )?;
            emit_report(&summary, None)?;
        }
        Cmd::Compact => {
            let rep = store::compact(&data, &cfg, now())?;
            emit_report(&rep, None)?;
        }
        Cmd::LegacyClose { old_label, dry_run } => {
            let gh = github()?;
            let n = alerts::legacy_close(&gh, &cfg, &old_label, dry_run).await?;
            println!("{} {n} issues labelled {old_label:?}", if dry_run { "would close" } else { "closed" });
        }
    }
    Ok(())
}
