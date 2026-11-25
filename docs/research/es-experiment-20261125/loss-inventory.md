# Loss inventory: ES experiment 2026-11-25

## What was lost, precisely

The instance `alashi-es-exp` (Brev, GCP L4) was stopped after training
completed and restarted later for evidence export. Brev's restart
re-provisioned the machine: `/tmp` was wiped; `/home/ubuntu` survived.
All experiment outputs had been written under `/tmp`.

Lost, by file class (one set per episode directory, structure defined by
`tools/agent-bootstrap/world-run.mjs`):

| Class | Files | Content semantics | Extent |
|---|---|---|---|
| Episode journals | `calls/*.json` (one per decision, ~112/episode) | full request messages (game observation JSON), model response text, http status, usage, error | 456 episodes × ~112 calls ≈ 51,000 decision records |
| Phase records | `phases/*.json` | submitted decision batch, engine receipts, accepted/rejected per actor | 456 episodes × ~20 phases |
| State snapshots | `snapshots/*.json`, `final-snapshot.json` | full world state per phase, final settlement | 456 episodes |
| Episode summaries | `summary.json` | leaderboard, payout, invalid/rejected counts | 456 episodes |
| Training log | `/tmp/es-train.log` | per-member reward/invalid lines for all 30 iterations × 15 members | complete file |
| Training aggregates | `/tmp/es-worlds/es-train-log.jsonl` | one line per iteration: mean/best reward, mean invalid | 30 lines |
| Trained weights | (never written) | final center parameters after 30 ES iterations | not checkpointed at all |

Episode count: 30 iterations × 15 members + 6 baseline episodes = 456.

## Recoverability

The instance was deleted after the loss was discovered (to stop storage
billing). Brev does not snapshot instance disks by default; GCP disks of
deleted instances are unrecoverable. The raw journals are gone for good.

## What survives

- Committed aggregates and the result table: `docs/research/preprint-notes-passk-20261124.md`
  (commits dded2009, 99a58bd3) — baseline table, final-iteration table, limits.
- `recovery-data.json` in this directory: every value that appeared in the
  session transcript — 6 complete baseline episode records, 10 of 30
  iteration aggregates, 30 member-level records across iterations 0/28/29,
  with a consistency check against the reported means.
- The driver and harness code (reproducible): any future run regenerates
  journals of identical structure.

## Root causes, ranked

1. Outputs written to `/tmp` (ephemeral by OS semantics on restart).
2. No checkpoint of the trained weights at end of run.
3. No off-box export step in the run protocol before stopping the instance.
4. The stop itself was for billing reasons; the export-before-stop step
   simply did not exist in the protocol.

## Prevention (patches applied 2026-11-25)

- `es-reward.mjs` writes episodes under `~/es-evidence/episodes/` by
  default (home survived the restart — verified by this incident).
- `es-train.py`: logs and aggregates to `~/es-evidence/`; weight
  checkpoint every 10 iterations and at the end (`~/es-evidence/checkpoints/`).
- `es-runbook.md`: mandatory off-box export command before any instance
  stop or delete; instances are disposable, the data pipeline is the product.
