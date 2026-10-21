# Migration runbook: Python pipeline → polywatch

Run the steps in order. Each step lists how to verify it and how to undo it.
Commands assume bash, an authenticated `gh`, and:

```bash
export REPO=danielseisenbacher/poly_position_scraper
export URL=https://github.com/$REPO.git
export WORK=~/polywatch-migration       # scratch space outside any clone
export LOCAL=~/Desktop/Privat/Programmierung/poly_position_scraper   # current local clone
mkdir -p "$WORK"
```

**Prerequisite:** every work package has been merged and committed on the local
`polywatch` branch, and `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace` is green there.

## 0. Stop the old workflow

The old workflow commits `users_already_seen.txt` back to `main` every 4 h, which
would re-add the file right after the purge.

```bash
gh workflow disable PositionScraper.yml -R $REPO
gh run list -R $REPO --workflow PositionScraper.yml --status in_progress   # wait until empty
```

- Verify: `gh workflow list -R $REPO --all` shows `PositionScraper` as `disabled_manually`.
- Undo: `gh workflow enable PositionScraper.yml -R $REPO`.

## 1. Snapshot

```bash
cd "$WORK"
git clone --mirror $URL legacy.git
git -C legacy.git bundle create ../legacy.bundle --all
git bundle verify legacy.bundle
gh issue list -R $REPO --state all --limit 1000 \
  --json number,title,state,labels,body,createdAt > legacy_issues.json
jq -r '.[].body' legacy_issues.json | grep -oE '0x[0-9a-fA-F]{40}' \
  | tr 'A-F' 'a-f' | sort -u > legacy_seed.txt
```

- Verify: `git bundle verify` ends with "is okay"; `jq length legacy_issues.json`
  is about 440; `wc -l legacy_seed.txt` is non-zero. (The old `newcomers.txt` was always empty,
  so the issues are the only seed source.)
- Undo: nothing to undo. Keep `legacy.bundle` and `legacy_issues.json` until the new
  system has run cleanly for a few weeks.

## 2. Purge history

`users_already_seen.txt` (40 MB, committed about 750 times) makes the repo 2.9 GB.
The purge runs in a throwaway mirror clone. The local `polywatch` branch is fetched
into it first, so it is rewritten in the same pass. Its two unpushed parent commits
(`8a3decf`, `28b7d0e`) also touch `users_already_seen.txt`. If you rebased instead,
they would conflict (modify/delete), so we rewrite them in the same pass.

```bash
# git-filter-repo needs git >= 2.36. Install one of:
sudo apt-get install -y git-filter-repo          # Ubuntu 22.04+
# pipx install git-filter-repo                    # or
# curl -fsSLo ~/.local/bin/git-filter-repo https://raw.githubusercontent.com/newren/git-filter-repo/main/git-filter-repo && chmod +x ~/.local/bin/git-filter-repo

cd "$WORK"
git clone --mirror $URL purge.git
cd purge.git
git fetch "$LOCAL" polywatch:refs/heads/polywatch
git count-objects -vH                             # before: size-pack ≈ 2.9 GiB
# --force only because the extra fetch makes the clone look "not fresh".
git filter-repo --force --invert-paths --path users_already_seen.txt --path newcomers.txt
git count-objects -vH                             # after: expect a few MiB at most
git log --all --oneline -- users_already_seen.txt newcomers.txt | wc -l   # expect 0
git log --oneline --graph main polywatch | head   # polywatch must sit on top of main

git remote add origin $URL                        # filter-repo removes origin
git push --force --mirror origin
```

The mirror push also creates the `polywatch` branch on GitHub. The repo has no PRs
today. If it had any, `refs/pull/*` would be rejected ("deny updating a hidden ref").
That is expected and harmless.

Then:

- **Retire the old local clone now.** Its `main` and `polywatch` still contain the
  purged blobs, and one push from it would bring them back.
  `git -C "$LOCAL" remote remove origin` (or delete it), then re-clone:
  `git clone $URL ~/Desktop/Privat/Programmierung/poly_position_scraper.new`.
  Everyone else must re-clone too.
- Optional: GitHub keeps unreachable objects until it garbage-collects. To shrink the
  reported size right away, open a ticket at https://support.github.com
  ("please run GC on $REPO after a history rewrite").

- Verify: `git ls-remote $URL` lists only `main` and `polywatch` with the new SHAs.
  `git clone $URL /tmp/pwcheck && du -sh /tmp/pwcheck/.git` gives a few MiB.
  `gh api repos/$REPO --jq .size` (KB) only drops after GitHub's GC.
- Undo: step 8 (restore from `legacy.bundle`).

## 3. Land the new code on `main`

The history was rewritten, so the new commits cannot be merged from the old
`polywatch` SHAs. Step 2 already rewrote `polywatch` on top of the new `main`.
Landing it is a plain fast-forward. The new code only adds new files and deletes the
old ones, so there is nothing to resolve.

```bash
cd ~/Desktop/Privat/Programmierung/poly_position_scraper.new
git fetch origin
git merge-base --is-ancestor origin/main origin/polywatch && echo fast-forward-ok
git push origin origin/polywatch:refs/heads/main
# Keep the first scheduled discovery from running before steps 6-7:
gh workflow disable discover.yml -R $REPO
git push origin --delete polywatch     # optional, once main has it
```

Push at a time that is not just before hh:23 of a 4-hour slot (00:23, 04:23, … UTC).
That gives you time to disable `discover.yml` before its cron fires. `track.yml` and
`maintenance.yml` can stay enabled: they exit 0 until the `data` branch exists.

- Verify: `gh workflow list -R $REPO` shows build, discover (disabled), track,
  pages, maintenance and legacy-close. `PositionScraper.yml` is gone from `main`.
- Undo: `git push --force origin <old-main-sha>:main` (the SHA printed by `git ls-remote` in step 2).

## 4. First build

The push in step 3 triggers `build.yml` (test, then the musl release build).

```bash
gh run watch -R $REPO "$(gh run list -R $REPO --workflow build.yml --limit 1 --json databaseId --jq '.[0].databaseId')"
gh release view bin-latest -R $REPO --json assets --jq '.assets[].name'   # polywatch, polywatch.sha256
gh release download bin-latest -R $REPO -p polywatch -D /tmp/pwbin --clobber && chmod +x /tmp/pwbin/polywatch && /tmp/pwbin/polywatch --version
```

If the build did not start, run `gh workflow run build.yml -R $REPO --ref main`.

- Verify: the release `bin-latest` is a pre-release whose notes name the `main` SHA,
  and the binary runs.
- Undo: `gh release delete bin-latest -R $REPO --cleanup-tag -y`.

## 5. Enable GitHub Pages

```bash
gh api -X POST repos/$REPO/pages -f build_type=workflow   # or Settings → Pages → Source: GitHub Actions
gh workflow run pages.yml -R $REPO --ref main
gh api repos/$REPO/pages --jq '{build_type, html_url, status}'
```

- Verify: https://danielseisenbacher.github.io/poly_position_scraper/ loads. It
  stays empty until the `data` branch exists (step 7).
- Undo: `gh api -X DELETE repos/$REPO/pages`.

## 6. Close the legacy issues

`legacy-close.yml` closes every open `insider-alert` issue and labels it `legacy`.

```bash
gh workflow run legacy-close.yml -R $REPO -f dry_run=true
gh run view -R $REPO --log "$(gh run list -R $REPO --workflow legacy-close.yml --limit 1 --json databaseId --jq '.[0].databaseId')" | grep 'would close'
# expect "would close 433 issues labelled \"insider-alert\"" (433 open as of 2026-10-21)
gh workflow run legacy-close.yml -R $REPO -f dry_run=false
```

- Verify: `gh issue list -R $REPO --label insider-alert --state open --json number --jq length` is `0`.
  `gh issue list -R $REPO --label legacy --state closed --limit 1000 --json number --jq length` is about 433.
- Undo:
  ```bash
  gh issue list -R $REPO --label legacy --state closed --limit 1000 --json number --jq '.[].number' \
    | xargs -I{} gh issue reopen {} -R $REPO
  ```

## 7. First discovery

```bash
cd ~/Desktop/Privat/Programmierung/poly_position_scraper.new
cp "$WORK/legacy_seed.txt" . && git add legacy_seed.txt && git commit -m "Add legacy seed addresses" && git push

gh workflow enable discover.yml -R $REPO
gh workflow run discover.yml -R $REPO -f dry_run=true
RUN=$(gh run list -R $REPO --workflow discover.yml --limit 1 --json databaseId --jq '.[0].databaseId')
gh run watch -R $REPO "$RUN"
gh run download -R $REPO "$RUN" -n "discover-report-$RUN" -D /tmp/pwreport
jq '{feed_rows, feed_truncated, enriched, flagged_high, flagged_medium, duration_s, requests}' /tmp/pwreport/discover.report.json
jq -r '.top[] | "\(.score) \(.addr) \(.signals | map(.[0]) | join(","))"' /tmp/pwreport/discover.report.json
```

Check that the run took well under 25 minutes, the 429 count is low, and a handful of
`top` wallets look plausible on polymarket.com. Then do the real seeded run:

```bash
gh workflow run discover.yml -R $REPO -f seed_file=legacy_seed.txt
```

After that, the crons run on their own.

- Verify:
  - `git ls-remote $URL data` exists and `gh release view state-latest -R $REPO` lists `evaluated.bin` and `proxy_owner.bin`.
  - `gh issue list -R $REPO --label insider-candidate` shows the new issues.
  - The dashboard shows data.
  - Within about 10 minutes, `gh run list -R $REPO --workflow track.yml --limit 3` succeeds, and
    `git fetch origin data && git log --oneline -3 origin/data` shows a `track: N new trades` commit.
- Undo:
  ```bash
  gh workflow disable discover.yml -R $REPO; gh workflow disable track.yml -R $REPO
  git push origin --delete data
  gh release delete state-latest -R $REPO --cleanup-tag -y
  gh issue list -R $REPO --label insider-candidate --limit 1000 --json number --jq '.[].number' \
    | xargs -I{} gh issue close {} -R $REPO
  ```

## 8. Full rollback

This restores the old history and issues. The old pipeline itself cannot be revived:
its data source (Goldsky positions-subgraph) is gone, and Data API v1 shuts down on
2026-10-24.

```bash
for w in discover track maintenance pages build; do gh workflow disable $w.yml -R $REPO; done
# Delete the releases first: the mirror push below removes their tags.
gh release delete bin-latest -R $REPO --cleanup-tag -y; gh release delete state-latest -R $REPO --cleanup-tag -y
cd "$WORK"
git clone --mirror legacy.bundle restore.git
cd restore.git
git remote set-url origin $URL
git push --force --mirror origin    # also deletes the data and polywatch branches
gh issue list -R $REPO --label legacy --state closed --limit 1000 --json number --jq '.[].number' \
  | xargs -I{} gh issue reopen {} -R $REPO
gh api -X DELETE repos/$REPO/pages
```

## Operating

| Task | Command |
|---|---|
| Pause tracking or discovery | `gh workflow disable track.yml -R $REPO` (or `discover.yml`). Resume with `gh workflow enable …`. |
| Change thresholds | Edit `polywatch.toml` on `main` (defaults are in `crates/types/src/config.rs`). Each run reads it from `main`, so no rebuild is needed. |
| Re-run discovery now | `gh workflow run discover.yml -R $REPO` (add `-f dry_run=true` for a report only, or `-f seed_file=<path>` to force-evaluate addresses). |
| Poll trades now | `gh workflow run track.yml -R $REPO` |
| Squash `data` now | `gh workflow run maintenance.yml -R $REPO` |
| Recent runs | `gh run list -R $REPO --limit 20`. Each run has a job summary with the report highlights. |

- **60-day rule:** GitHub disables scheduled workflows in public repos after 60 days
  without repository activity. The bot's commits to `data` (every 8 min) should keep
  them alive. If `gh workflow list -R $REPO --all` ever shows `disabled_inactivity`,
  run `gh workflow enable discover.yml -R $REPO`, and the same for `track.yml` and `maintenance.yml`.
- **Serialization:** discover, track and maintenance share the `data-writer` concurrency
  group, so only one of them writes `data` at a time. GitHub keeps only the newest
  pending run per group. Once in a while, a discover run that is waiting behind a
  tracker run gets replaced by the next tracker tick, and the following discover
  run (4 h later) catches up.
- **State:** `bin-latest` holds the binary. `state-latest` holds `evaluated.bin`,
  `proxy_owner.bin` and `trades-YYYY-MM.jsonl.gz` (the monthly trade archive). Don't
  delete `state-latest`: without it, every wallet gets re-evaluated and account links
  are lost. If `ubuntu-slim` is unavailable or too slow, switch `runs-on` to
  `ubuntu-latest` in `track.yml`, `maintenance.yml` and `legacy-close.yml`.
