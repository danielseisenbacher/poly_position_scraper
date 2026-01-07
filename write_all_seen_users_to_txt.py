all_unique_users = []

with open("users_already_seen_0.txt", "r") as f:
    for line in f:
        clean_line = line.strip()
        if len(clean_line) == 42:
            all_unique_users.append(clean_line)

with open("users_already_seen.txt", "w") as f:
    for user in all_unique_users:
        f.write(user + "\n")