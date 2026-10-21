#!/usr/bin/env bash
# Append a short markdown summary of a polywatch JSON report to $GITHUB_STEP_SUMMARY
# (or stdout when that is unset).
#   summary.sh discover|track <report.json> [dry-run true|false]
set -euo pipefail

kind="${1:?usage: $0 discover|track <report.json> [dry_run]}"
report="${2:?usage: $0 discover|track <report.json> [dry_run]}"
dry="${3:-false}"
out="${GITHUB_STEP_SUMMARY:-/dev/stdout}"
suffix=""
[[ "$dry" == "true" ]] && suffix=" (dry run)"

if [[ ! -s "$report" ]]; then
  echo "### $kind$suffix: no report (the run failed before writing one; see the log)" >> "$out"
  exit 0
fi

case "$kind" in
  discover)
    {
      echo "### discover$suffix"
      jq -r '
        def req: (.requests.requests // {} | to_entries | map("\(.key) \(.value)") | join(", "));
        "| | |", "|---|---|",
        "| feed | \(.feed_rows) rows / \(.feed_pages) pages, lag \(.feed_lag_s) s\(if .feed_truncated then " **(truncated)**" else "" end) |",
        "| sources | \(.sources // {} | to_entries | map("\(.key) \(.value)") | join(", ")) |",
        "| funnel | \(.prefiltered) prefiltered → \(.enriched) enriched → \(.scored) scored |",
        "| flagged | \(.flagged_high) high, \(.flagged_medium) medium |",
        "| watchlist | +\(.watchlist_added) → \(.watchlist_size) |",
        "| clusters | \(.clusters) (\(.new_factory_wallets) new factory wallets) |",
        "| requests | \(req); retries \(.requests.retries), 429s \(.requests.rate_limited), errors \(.requests.errors) |",
        "| duration | \(.duration_s | floor) s |"
      ' "$report"
      if [[ "$(jq '.top | length' "$report")" -gt 0 ]]; then
        echo
        echo "| wallet | score | tier | signals | PnL |"
        echo "|---|---:|---|---|---:|"
        jq -r '.top[:10][] |
          "| [\(.name // "" | gsub("[|\\[\\]`]"; "") | if . == "" then "" else . + " " end)`\(.addr[0:10])…`](https://polymarket.com/profile/\(.addr)) | \(.score) | \(.tier) | \(.signals | map(.[0] | tostring) | join(", ")) | \(.pnl | floor) |"
        ' "$report"
      fi
    } >> "$out"
    ;;
  track)
    jq -r --arg s "$suffix" '
      "track\($s): polled \(.polled)/\(.watchlist_size) wallets, **\(.new_trades) new trades**, \(.events | length) alert events, \(.deferred) deferred, \(.evicted | length) evicted, 429s \(.requests.rate_limited), \(.duration_s | floor) s"
    ' "$report" >> "$out"
    ;;
  *)
    echo "unknown report kind $kind" >&2
    exit 2
    ;;
esac
