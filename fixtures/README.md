# Fixtures

`live/` holds real responses recorded on 2026-10-21 from the public APIs polywatch uses
(Data API v2, Gamma, Polygon RPC via publicnode). Tests decode them and serve them
through wiremock; nothing in `cargo test` touches the network.

| file | request |
|---|---|
| `trades_global.json` | `GET /v2/trades?limit=200` (a few combo rows with zero-padded condition ids; `outcome_index` 999 also appears on ordinary rows and is not a combo marker) |
| `trades_user.json`, `activity_user.json` | `/v2/trades?user=…&taker_only=false`, `/v2/activity?user=…` |
| `positions_user_{OPEN,CLOSED,REDEEMABLE,REDEEMABLE_LOST}.json` | `/v2/positions?user=…&status=…` |
| `positions_condition.json` | `/v2/positions?condition=…` |
| `holders.json`, `holders_pnl.json` | `/v2/holders?condition=…[&include_pnl=true]` |
| `user_stats.json`, `value_user.json` | `/v2/user-stats?user=…`, `/v2/value?user=…` |
| `leaderboard.json`, `leaderboard_user.json` | `/v2/leaderboard?time_period=week&sort_by=PNL`, `?user=…` |
| `biggest_winners.json` | `/v2/biggest-winners?time_period=day` |
| `tokens.json`, `tokens_resolved.json` | `/v2/tokens?condition=…` |
| `prices_history.json` | `/v2/prices-history?token_id=…` |
| `status.json` | `/v2/status` |
| `gamma_markets_closed.json` | Gamma `/markets?closed=true&order=closedTime&ascending=false&limit=3` |
| `gamma_markets_by_condition.json` | Gamma `/markets?condition_ids=…` |
| `gamma_profile.json` | Gamma `/public-profile?address=…` |
| `factory_logs.json` | `eth_getLogs` on the Deposit Wallet Factory (5 logs kept) |
| `openapi_data_v2.json` | Data API v2 OpenAPI spec (reference) |

Quirks worth knowing: Gamma encodes `outcomes`, `outcomePrices` and `clobTokenIds` as
JSON strings inside JSON; activity rows send `"side": ""` for non-trades; per-market
endpoints return 400 for combo (zero-padded) condition ids.
