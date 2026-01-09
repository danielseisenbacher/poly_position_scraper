import sys
import time
from gql import gql, Client
from gql.transport.requests import RequestsHTTPTransport

# 1. Capture the prefix from the command line argument
# Example: python scraper.py a -> will search for id_starts_with: "0xa"
prefix_char = sys.argv[1] if len(sys.argv) > 1 else "0"
search_prefix = f"0x{prefix_char}"
output_filename = f"users_{prefix_char}.txt"

QUERY_URL = "https://api.goldsky.com/api/public/project_cl6mb8i9h0003e201j6li0diw/subgraphs/positions-subgraph/0.0.7/gn"
MAX_RETRIES = 10
last_id = ""
unique_users = []
user_seen = ""
seen = 1

print(f"Starting scraper for prefix: {search_prefix}")

count = 0
while True:
    count += 1
    if count % 100 == 0:
        print(f"Iteration {count}")

    # 2. Use the dynamic prefix in the where clause
    where_clause = f'where: {{ id_starts_with: "{search_prefix}"'
    if last_id:
        where_clause += f', id_gt: "{last_id}"'
    where_clause += ' }'

    q_string = f"""
    query MyQuery {{
      userBalances(first: 1000, orderBy: id, orderDirection: asc, {where_clause}) {{
        id
        user
      }}
    }}
    """

    # ... [Your existing GQL client/transport setup] ...
    query = gql(q_string)
    transport = RequestsHTTPTransport(url=QUERY_URL, verify=True, retries=5, timeout=30)
    client = Client(transport=transport)

    res = None
    for attempt in range(MAX_RETRIES):
        try:
            res = client.execute(query)
            break
        except Exception as e:
            wait_time = min(60, 2 ** attempt)
            time.sleep(wait_time)
            if attempt == MAX_RETRIES - 1: raise e

    balances = res.get("userBalances", [])

    if not balances:
        break  # Exit loop if no more records for this prefix

    for balance in balances:
        current_user = balance["user"].split("-")[0]
        if current_user == user_seen:
            seen += 1
        else:
            if user_seen != "" and seen < 6:
                unique_users.append(user_seen)
            user_seen = current_user
            seen = 1

    last_id = balances[-1]['id']

# Handle the very last user
if user_seen != "" and seen < 6:
    unique_users.append(user_seen)

# 3. Save results to a specific file for this worker
with open(output_filename, "w") as f:
    for user in unique_users:
        f.write(f"{user}\n")

print(f"Finished. Found {len(unique_users)} users for {search_prefix}")