from datetime import datetime, timedelta
import requests

def get_recent_big_fish() -> None:

    new_big_fish = set()
    with open("new_big_fish.txt", "r") as f:
        for line in f:
            clean_line = line.strip()
            if clean_line:  # Skip empty lines
                new_big_fish.add(clean_line)

    new_big_fish = list(new_big_fish)
    two_months_ago = datetime.now() - timedelta(days=60)


    recent_big_fish = []
    for count, user in enumerate(new_big_fish, start=1):
        if count % 100 == 0:
            print("Current Count: ", count)

        try:
            print(user)
            response = requests.get(
                "https://gamma-api.polymarket.com/public-profile",
                params={'address': user},
                timeout=10
            )

            if response.status_code == 200:
                data = response.json()
                created_at_date = data["createdAt"]
                created_at_date = datetime.fromisoformat(created_at_date.replace("Z", "+00:00"))

                if created_at_date > two_months_ago:
                    recent_big_fish.append(user)
                    print(f"Recent Big Fish: {user} | Result: {data}")
            else:
                print(f"User: {user} | Error: {response.status_code}")

        except Exception as e:
            print(f"Failed to fetch data for {user}: {str(e)}")

    with open("recent_big_fish.txt", "w") as f:
        for big_fish in recent_big_fish:
            f.write(big_fish + "\n")
    return

get_recent_big_fish()
