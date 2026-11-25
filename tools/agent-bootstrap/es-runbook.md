# ES experiment runbook (data-first protocol, 2026-11-25)

Instances are disposable. The data pipeline is the product.

## Before the run

1. Instance up (brev create / brev start).
2. Stack setup (see docs/research/es-experiment-20261125/loss-inventory.md
   for the full setup sequence: node 20, repo, rust, world_worker, venv,
   torch, model).
3. All outputs land under `~/es-evidence/` (persistent across instance
   restarts — verified by the 25.11 incident). Never write experiment
   outputs to `/tmp`.

## During the run

- Episodes: `~/es-evidence/episodes/<name>/` (default in es-reward.mjs).
- Aggregates: `~/es-evidence/es-train-log.jsonl` (one line per iteration).
- Weight checkpoints: `~/es-evidence/checkpoints/iter-<N>/` every 10
  iterations and the final one (es-train.py).

## MANDATORY before any instance stop, reset or delete

```bash
# from the laptop, with the instance running:
mkdir -p ~/Downloads/alashi-es-evidence-<date>
scp -F ~/.brev/ssh_config alashi-es-exp:'~/es-evidence/es-train-log.jsonl' \
    ~/Downloads/alashi-es-evidence-<date>/
ssh -F ~/.brev/ssh_config alashi-es-exp \
  'cd ~ && tar czf es-evidence.tgz es-evidence'
scp -F ~/.brev/ssh_config alashi-es-exp:~/es-evidence.tgz \
    ~/Downloads/alashi-es-evidence-<date>/
```

Verify the bundle size is nonzero and the jsonl line count matches the
completed iterations. Only then `brev stop` or `brev delete`.

## After the run

Commit the aggregates and the recovery/summary records to the repo
(docs/research/es-experiment-...). Raw journal bundles stay in the
evidence folder until archived where the owner decides.
