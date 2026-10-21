#!/usr/bin/env bash
# Shallow-clone the orphan `data` branch into data/ and set the bot identity.
#
#   clone-data.sh            missing branch -> exists=false, exit 0 (nothing cloned)
#   clone-data.sh --create   missing branch -> init an empty orphan branch in data/
#
# Writes `exists=true|false` to $GITHUB_OUTPUT when that is set.
# Env: GH_TOKEN, GITHUB_REPOSITORY.
set -euo pipefail

create=false
case "${1:-}" in
  --create) create=true ;;
  "") ;;
  *) echo "usage: $0 [--create]" >&2; exit 2 ;;
esac

url="https://x-access-token:${GH_TOKEN:?GH_TOKEN is not set}@github.com/${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is not set}.git"

output() { if [[ -n "${GITHUB_OUTPUT:-}" ]]; then echo "$1" >> "$GITHUB_OUTPUT"; fi; }

identity() {
  git -C data config user.name "github-actions[bot]"
  git -C data config user.email "41898182+github-actions[bot]@users.noreply.github.com"
}

# --exit-code: 0 = branch found, 2 = no such branch, anything else = real error.
rc=0
git ls-remote --exit-code --heads "$url" data > /dev/null || rc=$?

if [[ $rc -eq 0 ]]; then
  git clone --quiet --depth 1 --branch data "$url" data
  identity
  echo "data branch at $(git -C data rev-parse --short HEAD)"
  output "exists=true"
elif [[ $rc -eq 2 ]]; then
  if $create; then
    echo "::notice::The data branch does not exist yet; creating an empty orphan branch."
    git init --quiet --initial-branch data data
    git -C data remote add origin "$url"
    identity
    git -C data commit --quiet --allow-empty -m "init data branch"
    output "exists=true"
  else
    echo "::notice::The data branch does not exist yet (no discovery has run); nothing to do."
    output "exists=false"
  fi
else
  echo "::error::git ls-remote failed with exit code $rc" >&2
  exit "$rc"
fi
