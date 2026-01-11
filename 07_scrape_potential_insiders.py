import requests

recent_big_fish=[]
with open("recent_big_fish.txt", "r") as f:
    for line in f:
        clean_line = line.strip()
        if len(clean_line) == 42:
            recent_big_fish.append(clean_line)

print(f"Count of recent fish: {len(recent_big_fish)}", flush=True)

market_tracker = {}
insider_candidates = []
for count, user in enumerate(recent_big_fish, start=1):

    if count % 100 == 0:
        print(f"Iteration {count}", flush=True)

    try:
        response = requests.get(
            "https://data-api.polymarket.com/positions",
            params={'user': user},
            timeout=10
        )

        if response.status_code == 200:
            data = response.json()

            risk_sum = 0
            valid_positions = 0

            for position in data:
                if position["percentPnl"] <= -99:
                    continue

                valid_positions += 1
                risk_sum += position["avgPrice"]

                cid = position["conditionId"]
                value = float(position["currentValue"])
                avg_price = float(position["avgPrice"])

                # Track market exposure globally
                if cid not in market_tracker:
                    market_tracker[cid] = {
                        "count": 0,
                        "total_value": 0.0,
                        "users": []
                    }

                market_tracker[cid]["count"] += 1
                market_tracker[cid]["total_value"] += value
                market_tracker[cid]["users"].append(user)

            if valid_positions == 0 or risk_sum / valid_positions > 0.3:
                continue

            insider_candidates.append(user)
            print(f"Potential Insider Found: {user}", flush=True)

        else:
            print(f"User: {user} | Error: {response.status_code}")

    except Exception as e:
        print(f"Failed to fetch data for {user}: {str(e)}")

with open("insider_candidates.txt", "w") as f:
    print("-"*50, flush=True)
    print("Found potential Insiders", len(insider_candidates), flush=True)
    print("-" * 50, flush=True)

    for row in insider_candidates:
        f.write(f"{row}\n")


with open("market_clusters.txt", "w") as f:
    f.write("conditionId,count,total_value,users\n")

    for cid, data in sorted(
        market_tracker.items(),
        key=lambda x: x[1]["total_value"],
        reverse=True
    ):
        if data["count"] > 1 or data["total_value"] >= 2500:
            f.write(
                f"{cid},{data['count']},{round(data['total_value'],2)},"
                f"{'|'.join(set(data['users']))}\n"
            )

