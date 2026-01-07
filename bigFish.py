import requests
from math import floor
import sys
from datetime import datetime, timedelta


def get_big_fish(runner_index: int) -> list[str]:
    print("Running: ", runner_index)

    # Load all unique users
    all_unique_users = set()
    with open("unique_users.txt", "r") as f:
        for line in f:
            clean_line = line.strip()
            if len(clean_line) == 42:
                all_unique_users.add(clean_line)  # Add directly to set
    print(f"All unique users: {len(all_unique_users)}")

    # Load already analyzed users
    already_analysed_set = set()
    with open("users_already_seen.txt", "r") as f:
        for line in f:
            clean_line = line.strip()
            already_analysed_set.add(clean_line)  # Add directly to set

    # Get new users using set difference (faster)
    unique_users = list(all_unique_users - already_analysed_set)
    print(f"New users to analyze: {len(unique_users)}")

    # Write all_unique_users to a file
    with open(f"users_already_seen_{runner_index}.txt", "w") as f:
        for elem in all_unique_users:
            f.write(elem + "\n")

    workload_per_runner = floor(len(unique_users)/18)
    if runner_index < 18:
        start_idx = workload_per_runner * runner_index
        end_idx = (
            workload_per_runner * (runner_index + 1)
            if runner_index < 17
            else len(unique_users)
        )
        print(f"Start index: {start_idx}, End index: {end_idx}")
        workload = unique_users[start_idx : end_idx]
    else:
        workload = unique_users


    big_fish = []
    for count, user in enumerate(workload, start=1):
        if count % 100 == 0:
            print("Current Count: ", count)

        try:
            response = requests.get(
                "https://data-api.polymarket.com/value",
                params={'user': user},
                timeout=10
            )

            if response.status_code == 200:
                data = response.json()
                if data[0]["value"] > 500:
                    big_fish.append(user)
                    print(f"Big Fish: {user} | Result: {data}")
            else:
                print(f"User: {user} | Error: {response.status_code}")

        except Exception as e:
            print(f"Failed to fetch data for {user}: {str(e)}")

    return big_fish


def get_recent_big_fish(big_fish: list[str], runner_index: int) -> None:
    if len(big_fish) == 0:
        print("No big fish for analysis found")
        with open("recent_big_fish.txt", "w") as f:
            f.write("\n")
        return

    two_months_ago = datetime.now() - timedelta(days=60)

    workload_per_runner = floor(len(big_fish) / 18)
    if runner_index < 18:
        start_idx = workload_per_runner * runner_index
        end_idx = (
            workload_per_runner * (runner_index + 1)
            if runner_index < 17
            else len(big_fish)
        )
        print(f"Start index: {start_idx}, End index: {end_idx}")
        workload = big_fish[start_idx: end_idx]
    else:
        workload = big_fish

    recent_big_fish = []
    for count, user in enumerate(workload, start=1):
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


run_index = int(sys.argv[1]) if len(sys.argv) > 1 else 99

runner_big_fish = get_big_fish(run_index)
get_recent_big_fish(runner_big_fish, run_index)