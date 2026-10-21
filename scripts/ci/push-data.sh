#!/usr/bin/env bash
# Commit everything in data/ and push it to the `data` branch, rebasing onto
# origin/data up to 3 times if the push is rejected. No-op when nothing changed.
#   push-data.sh "<commit message>"
set -euo pipefail

msg="${1:?usage: $0 <commit message>}"
cd data

git add -A
if git diff --cached --quiet; then
  echo "No data changes to commit."
  exit 0
fi
git commit --quiet -m "$msg"
echo "Committed: $msg ($(git show --stat --format= HEAD | tail -n1))"

for attempt in 0 1 2 3; do
  if [[ $attempt -gt 0 ]]; then
    echo "::warning::Push rejected; rebasing onto origin/data (retry $attempt of 3)."
    sleep $((attempt * 5))
    git pull --quiet --rebase origin data
  fi
  if git push --quiet origin HEAD:data; then
    echo "Pushed data at $(git rev-parse --short HEAD)."
    exit 0
  fi
done

echo "::error::Could not push the data branch after 3 rebase retries."
exit 1
