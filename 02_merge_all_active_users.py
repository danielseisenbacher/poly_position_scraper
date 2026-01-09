from pathlib import Path


def merge_user_files():
    """Merge all user files from all_results directory into unique_users.txt"""

    all_users = set()
    results_dir = Path("all_results")

    # Read all user files
    for user_file in results_dir.glob("users_*.txt"):
        print(f"Reading {user_file.name}...", flush=True)
        with open(user_file, "r") as f:
            for line in f:
                clean_line = line.strip()
                if clean_line:  # Skip empty lines
                    all_users.add(clean_line)

    # Sort and write to output
    sorted_users = sorted(all_users)

    with open("all_users.txt", "w") as f:
        for user in sorted_users:
            f.write(user + "\n")

    print(f"Combined unique users file created", flush=True)
    print(f"Total unique users: {len(sorted_users)}", flush=True)


merge_user_files()