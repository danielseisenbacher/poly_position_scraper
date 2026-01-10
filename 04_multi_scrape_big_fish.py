import requests
from math import floor
import sys


def get_big_fish(runner_index: int) -> None:
    print("Running: ", runner_index)

    new_users = set()
    with open("new_users.txt", "r") as f:
        for line in f:
            clean_line = line.strip()
            if len(clean_line) == 42:
                new_users.add(clean_line)  # Add directly to set

    new_users = list(new_users)

    workload_per_runner = floor(len(new_users)/20)
    if runner_index < 20:
        start_idx = workload_per_runner * runner_index
        end_idx = (
            workload_per_runner * (runner_index + 1)
            if runner_index < 19
            else len(new_users)
        )
        print(f"Start index: {start_idx}, End index: {end_idx}")
        workload = new_users[start_idx : end_idx]
    else:
        workload = new_users


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

    print(f"FOUND {len(big_fish)} BIG FISH")

    with open(f"big_fish_{runner_index}.txt", "w") as f:
        for fish in big_fish:
            f.write(fish + "\n")


run_index = int(sys.argv[1]) if len(sys.argv) > 1 else 99
get_big_fish(run_index)
