# polywatch

Finds Polymarket wallets that look like they trade on inside information. It looks for:

- new accounts betting big on longshots
- accounts with statistically improbable win rates
- leaderboard newcomers
- linked accounts
- well-timed bets

It then tracks the flagged wallets' trades every ~8 minutes.

**Where to look**

- Dashboard: https://danielseisenbacher.github.io/poly_position_scraper/
- Notifications: issues labelled [`insider-candidate`](https://github.com/danielseisenbacher/poly_position_scraper/issues?q=label%3Ainsider-candidate). Each flagged wallet gets one issue, and new findings and big trades are added as comments on it.

**How it runs** (GitHub Actions, one static Rust binary, public APIs only, no keys)

| Workflow | When | Does |
|---|---|---|
| `discover.yml` | every 4 h | scores new wallets, opens and updates issues, writes the `data` branch |
| `track.yml` | every 8 min | polls watched wallets for new trades |
| `pages.yml` | on `site/` changes | deploys the dashboard (it reads its data from the `data` branch) |
| `maintenance.yml` | weekly | prunes stale wallets, squashes the `data` branch |
| `build.yml` | on code changes | tests, then publishes the binary to release `bin-latest` |

**Layout:** `crates/` holds the Rust workspace (`cli` is the `polywatch` binary), `site/` the
static dashboard, `fixtures/` the recorded API responses for tests, and `scripts/ci/` the
workflow helpers.

**Run locally:** `cargo run -p polywatch -- discover --dry-run --data-dir /tmp/pw`

**Config:** `polywatch.toml`. Defaults are in `crates/types/src/config.rs`.

These are heuristic leads, not proof. The signals also catch lucky, skilled or
contrarian traders.

Migrating from the old Python pipeline: see [MIGRATION.md](MIGRATION.md).
