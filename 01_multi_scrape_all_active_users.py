import sys
import time
from gql import gql, Client
from gql.transport.requests import RequestsHTTPTransport

prefix_char = sys.argv[1] if len(sys.argv) > 1 else "0"
search_prefix = f"0x{prefix_char}"
output_filename = f"users_{prefix_char}.txt"

QUERY_URL = "https://api.goldsky.com/api/public/project_cl6mb8i9h0003e201j6li0diw/subgraphs/positions-subgraph/0.0.7/gn"

PAGE_SIZES = [1000, 500, 250, 100, 50, 25, 10]  # shrink ladder on repeated timeouts
MAX_RETRIES_PER_SIZE = 5
MAX_BACKOFF_SECONDS = 60
REQUEST_PACING_SECONDS = 0.3  # small delay between successful requests to go easy on the indexer

last_id = ""
unique_users = []
user_seen = ""
seen = 1

transport = RequestsHTTPTransport(url=QUERY_URL, verify=True, retries=5, timeout=30)
client = Client(transport=transport)

print(f"Starting scraper for prefix: {search_prefix}", flush=True)


def save_partial():
    """Write out whatever we have so far, for debugging/artifact purposes.
    This is NOT a resume checkpoint -- the next scheduled run always starts
    from scratch to avoid missing users with smaller ids created in between."""
    with open(output_filename, "w") as f:
        for user in unique_users:
            f.write(f"{user}\n")


count = 0
page_size_idx = 0

try:
    while True:
        count += 1
        if count % 100 == 0:
            print(f"Iteration {count} (last_id={last_id})", flush=True)

        page_size = PAGE_SIZES[page_size_idx]

        where_clause = f'where: {{ id_starts_with: "{search_prefix}"'
        if last_id:
            where_clause += f', id_gt: "{last_id}"'
        where_clause += ' }'

        q_string = f"""
        query MyQuery {{
          userBalances(first: {page_size}, orderBy: id, orderDirection: asc, {where_clause}) {{
            id
            user
          }}
        }}
        """
        query = gql(q_string)
        res = None
        succeeded = False
        last_error = None

        for attempt in range(MAX_RETRIES_PER_SIZE):
            try:
                res = client.execute(query)
                succeeded = True
                break
            except Exception as e:
                last_error = e
                wait_time = min(MAX_BACKOFF_SECONDS, 2 ** attempt)
                print(
                    f"Error (page_size={page_size}) attempt {attempt + 1}/{MAX_RETRIES_PER_SIZE}: {e}",
                    flush=True,
                )
                time.sleep(wait_time)

        if not succeeded:
            if page_size_idx < len(PAGE_SIZES) - 1:
                page_size_idx += 1
                print(
                    f"Shrinking page size to {PAGE_SIZES[page_size_idx]} and retrying at last_id={last_id}...",
                    flush=True,
                )
                continue
            else:
                # Smallest page size still failing -- this is a genuine outage
                # or a range the indexer can't serve. Do not silently truncate
                # results: fail loudly so the workflow marks this run as failed.
                save_partial()
                raise RuntimeError(
                    f"Giving up at last_id={last_id} for prefix {search_prefix} "
                    f"after exhausting page-size ladder {PAGE_SIZES}. "
                    f"Last error: {last_error}"
                )

        # Success -- ease back toward the largest page size gradually
        if page_size_idx > 0:
            page_size_idx -= 1

        balances = res.get("userBalances", [])
        if not balances:
            break  # No more records for this prefix -- genuinely done

        for balance in balances:
            current_user = balance["user"].split("-")[0]
            if current_user == user_seen:
                seen += 1
            else:
                if user_seen != "" and seen < 6:
                    unique_users.append(user_seen)
                user_seen = current_user
                seen = 1

        last_id = balances[-1]["id"]

        time.sleep(REQUEST_PACING_SECONDS)

    # Handle the very last user
    if user_seen != "" and seen < 6:
        unique_users.append(user_seen)

    save_partial()
    print(f"Finished. Found {len(unique_users)} users for {search_prefix}", flush=True)

except Exception as e:
    print(f"FATAL: {e}", flush=True)
    sys.exit(1)