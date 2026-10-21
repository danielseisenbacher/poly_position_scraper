# Seeds

Wallets flagged by the old Python pipeline (addresses from its 2026 "Insider Candidates
Detected" issues), split into three files so each seeded discovery run stays well inside
the job time limit. Run each once:

```bash
gh workflow run discover.yml -f seed_file=seeds/legacy-00.txt
```

Seeded wallets skip the re-evaluation window and the user-stats gate, so they are
re-scored with the current model regardless of when they were last seen.
