import requests
import json

# Load recent big fish
recent_big_fish = []
with open("recent_big_fish.txt", "r") as f:
    for line in f:
        clean_line = line.strip()
        if len(clean_line) == 42:
            recent_big_fish.append(clean_line)

print(f"Count of recent fish: {len(recent_big_fish)}", flush=True)

market_tracker = {}
insider_candidates = {}

for count, user in enumerate(recent_big_fish, start=1):

    if count % 100 == 0:
        print(f"Iteration {count}", flush=True)

    try:
        response = requests.get(
            "https://data-api.polymarket.com/positions",
            params={"user": user},
            timeout=10
        )

        if response.status_code != 200:
            print(f"User: {user} | Error: {response.status_code}")
            continue

        data = response.json()

        insider_value = 0.0
        risk_sum = 0.0
        valid_positions = 0
        user_markets = set()

        for position in data:
            # Ignore nuked / liquidated positions
            if position.get("percentPnl", 0) <= -99:
                continue

            value = float(position["currentValue"])
            avg_price = float(position["avgPrice"])
            market_question = position["title"]

            # Track user exposure
            user_markets.add(market_question)
            insider_value += value
            risk_sum += avg_price
            valid_positions += 1

            # Track market exposure
            if market_question not in market_tracker:
                market_tracker[market_question] = {
                    "count": 0,
                    "total_value": 0.0,
                    "users": {}
                }

            # Count this whale only once per market
            if user not in market_tracker[market_question]["users"]:
                market_tracker[market_question]["count"] += 1

            market_tracker[market_question]["total_value"] += value
            market_tracker[market_question]["users"][user] = {
                "value": value,
                "avg_price": avg_price
            }

        # Skip empty wallets
        if valid_positions == 0:
            continue

        mean_risk = risk_sum / valid_positions

        # Skip gamblers
        if mean_risk > 0.30:
            continue

        insider_candidates[user] = {
            "markets": list(user_markets),
            "value": insider_value,
            "meanRisk": mean_risk
        }

        print(f"Potential Insider Found: {user}", flush=True)

    except Exception as e:
        print(f"Failed to fetch data for {user}: {str(e)}")

# Write structured outputs for GitHub Actions
with open("insider_candidates.json", "w") as f:
    json.dump(insider_candidates, f, indent=2)

with open("market_tracker.json", "w") as f:
    json.dump(market_tracker, f, indent=2)
