# poly_position_scraper

Flags new Polymarket wallets that might be trading on inside information. A GitHub Actions workflow ([PositionScraper.yml](.github/workflows/PositionScraper.yml)) runs the pipeline every 4 hours and opens an issue whenever it flags wallets.

## How it works

The numbered scripts run in order, each narrowing down the list of wallets:

1. **01–03**: Collect all wallets with fewer than 6 positions from Polymarket's positions subgraph, and keep only those not seen in the previous run (tracked in `users_already_seen.txt`).
2. **04–05**: Keep wallets holding more than $500.
3. **06**: Keep wallets whose account is less than 60 days old.
4. **07**: Flag wallets whose average entry price is 30¢ or less, meaning mostly long-shot bets.

Flagged wallets are posted as an issue labeled `insider-alert` and added to `newcomers.txt`.

These are leads, not proof: the filters also catch lucky or contrarian traders.
