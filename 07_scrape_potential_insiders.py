import requests

recent_big_fish=[]
with open("recent_big_fish.txt", "r") as f:
    for line in f:
        clean_line = line.strip()
        if len(clean_line) == 42:
            recent_big_fish.append(clean_line)

print("Count of recent fish: ", recent_big_fish)

insider_candidates = []
for count, user in enumerate(recent_big_fish, start=1):

    if count % 100 == 0:
        print(f"Interation {count}")

    try:
        response = requests.get(
            "https://data-api.polymarket.com/positions",
            params={'user': user},
            timeout=10
        )

        if response.status_code == 200:
            data = response.json()

            risk_sum = 0
            for position in data:
                risk_sum += position["avgPrice"]

                if position["percentPnl"] <= -99:
                    continue

            if risk_sum/len(data) > 0.3:
                continue

            insider_candidates.append(user)

        else:
            print(f"User: {user} | Error: {response.status_code}")

    except Exception as e:
        print(f"Failed to fetch data for {user}: {str(e)}")

with open("insider_candidates.txt", "w") as f:
    for row in insider_candidates:
        f.write(f"{row}\n")
