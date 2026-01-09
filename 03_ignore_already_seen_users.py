import os

# Load all unique users
all_users = set()
with open("all_users.txt", "r") as f:
    for line in f:
        clean_line = line.strip()
        if len(clean_line) == 42:
            all_users.add(clean_line)  # Add directly to set

print(f"All unique users: {len(all_users)}")

# Load already analyzed users
already_analysed_set = set()
if os.path.exists("users_already_seen.txt"):
    with open("users_already_seen.txt", "r") as f:
        for line in f:
            clean_line = line.strip()
            already_analysed_set.add(clean_line)  # Add directly to set
else:
    print("FIRST RUN DETECTED")
    already_analysed_set = set()

# Get new users using set difference (faster)
new_users = list(all_users - already_analysed_set)
print(f"FOUND {len(new_users)} NEW USERS SINCE LAST RUN")

# Write all_users to users_already_seen.txt to save for the next time
with open(f"users_already_seen.txt", "w") as f:
    for elem in all_users:
        f.write(elem + "\n")

# Save only the new users for this run in the workflow
with open(f"new_users.txt", "w") as f:
    for elem in new_users:
        f.write(elem + "\n")
