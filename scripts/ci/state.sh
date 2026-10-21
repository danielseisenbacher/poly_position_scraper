#!/usr/bin/env bash
# Move large state files between the runner and release `state-latest`.
# Each argument is ASSET:PATH (release asset names cannot contain `/`).
#
#   state.sh fetch ASSET:PATH...
#       Download each asset that exists on the release to PATH. A missing
#       release or asset is fine (first run); any other failure is fatal, so a
#       network error can never lead to a fresh file clobbering the real asset.
#   state.sh push [--append-new] ASSET:PATH...
#       Upload each PATH that exists and changed since `fetch` (creating the
#       release if needed). With --append-new, a PATH that was not fetched but
#       whose asset exists remotely is appended to the remote copy first
#       (for multi-member .gz archives, where concatenation is a valid append).
#
# Env: GH_TOKEN, GITHUB_REPOSITORY, RUNNER_TEMP (optional).
set -euo pipefail

repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is not set}"
tag=state-latest
tmp="${RUNNER_TEMP:-/tmp}"
manifest="$tmp/polywatch-state.sha256"
touch "$manifest"

# Names of the assets currently on the release (empty if the release is missing).
remote_assets() {
  local err out
  err="$(mktemp)"
  if out="$(gh release view "$tag" -R "$repo" --json assets --jq '.assets[].name' 2> "$err")"; then
    rm -f "$err"
    printf '%s\n' "$out"
  elif grep -qi 'not found' "$err"; then
    rm -f "$err"
  else
    cat "$err" >&2
    rm -f "$err"
    return 1
  fi
}

has_asset() { grep -Fxq "$1" <<< "$assets"; }
fetched() { awk -v p="$1" '$2 == p { found = 1 } END { exit !found }' "$manifest"; }
unchanged() { grep -Fxq "$(sha256sum "$1")" "$manifest"; }

cmd="${1:?usage: $0 fetch|push [--append-new] ASSET:PATH...}"
shift
append_new=false
if [[ "${1:-}" == "--append-new" ]]; then append_new=true; shift; fi

assets="$(remote_assets)"

case "$cmd" in
  fetch)
    for pair in "$@"; do
      asset="${pair%%:*}" path="${pair#*:}"
      if has_asset "$asset"; then
        mkdir -p "$(dirname "$path")"
        gh release download "$tag" -R "$repo" -p "$asset" -O "$path" --clobber
        sha256sum "$path" >> "$manifest"
        echo "Fetched $asset -> $path ($(du -h "$path" | cut -f1))"
      else
        echo "::notice::$tag has no asset $asset yet; starting without $path."
      fi
    done
    ;;
  push)
    stage="$(mktemp -d "$tmp/state-upload.XXXXXX")"
    for pair in "$@"; do
      asset="${pair%%:*}" path="${pair#*:}"
      [[ -f "$path" ]] || continue
      if unchanged "$path"; then
        echo "$asset unchanged."
        continue
      fi
      if $append_new && ! fetched "$path" && has_asset "$asset"; then
        echo "Appending $path to the existing remote $asset."
        gh release download "$tag" -R "$repo" -p "$asset" -O "$stage/$asset" --clobber
        cat "$path" >> "$stage/$asset"
      else
        cp "$path" "$stage/$asset"
      fi
    done
    shopt -s nullglob
    files=("$stage"/*)
    if [[ ${#files[@]} -eq 0 ]]; then
      echo "No state assets to upload."
      exit 0
    fi
    if [[ -z "$assets" ]] && ! gh release view "$tag" -R "$repo" > /dev/null 2>&1; then
      gh release create "$tag" -R "$repo" --prerelease --title "polywatch state" \
        --notes "Large state written by discover.yml and track.yml (not for humans). Do not delete."
    fi
    gh release upload "$tag" -R "$repo" --clobber "${files[@]}"
    echo "Uploaded: $(cd "$stage" && echo *)"
    ;;
  *)
    echo "unknown command $cmd" >&2
    exit 2
    ;;
esac
