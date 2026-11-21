# Actor explorer: per-actor neutral view (паттерн Urban Game Theory)

Implementation and acceptance evidence for the deep UGT/alashi task on
21 November 2026. Branch `actor-explorer-ugt`, commit `e3b850ce`.

## Intended result and boundary

Borrow one mechanism from Urban Game Theory (actor-explorer.html): a neutral,
per-actor public view of who acted, who decided each action, and where the
evidence lives. Alashi already had the aggregate census
(`tools/census_build.py` → `docs/census.html`); this adds the missing
per-actor layer with controller attribution.

Boundary: this is a static, read-only view over already-committed artifacts.
It changes no game rules, adds no on-chain code, registers no players. It is
not a live service; it regenerates from files in one command.

## Sources (all committed raw data)

- `data/live/party*_90s_export.json` — 18 live HTTP-arena party exports;
  each action records `by: llm | fallback | unknown` (who decided) and
  per-actor balances/payouts.
- `data/registry.json` and `data/exports/parties.jsonl` — devnet wallets
  where recorded. Most party-export agents have no wallet recorded in the
  repository; the page says so instead of inventing links.
- `docs/ops/world-100-*/audit.json` — 7 public world-100 audits with
  per-agent participation, settlement, evidence hashes and limitations.

## What the page shows

- Summary: parties, actions, decisions split by controller, distinct actors.
- Per-actor rows (sorted by actions): name(s), model(s), agent id, devnet
  wallet link where known, parties played, actions, llm/fallback/unknown
  split, wins, payout sum.
- Per-party table with the same controller split and bank.
- World-100 table: agents, accepted/rejected, peer-reported model,
  settlement, checks vs issues, frozen digest and final state hash.
- Verbatim limitations from each audit; input digest footer.

## Acceptance evidence

- `python3 tools/test_actor_explorer_build.py` exits 0 on the branch:
  1. generator reproduces the page from committed data;
  2. page numbers match an independent recount by the test (parties,
     actions, controller totals, actor row count equals distinct agent ids);
  3. two builds are byte-identical (no clocks, fixed ordering);
  4. committed `docs/actor_explorer.html` is byte-identical to a fresh
     build (freeze gate).
- Page is deterministic end to end: footer carries sha256 over all input
  files (`9ae74dd065efc4d0…` at acceptance).
- No changes outside `tools/` and `docs/`; no existing file modified except
  the new page; no push performed.

## Result

**Completed on the branch.** One command regenerates the page; the CI guard
fails loudly if the committed page drifts from the data or is hand-edited.

Decision: no fresh world run was executed for this candidate. The explorer
is a view over committed artifacts, so its acceptance is regeneration and
recount (above); the exact-replay property belongs to the worlds themselves
and is already covered by their own audits (e.g. WORLD_100_20261109
replay.json, 24 reexecuted phase batches). Running a new world would test
the worker, not this view, and is therefore out of scope here.

## Limitations (explicit)

- Controller attribution comes from arena exports and is runner-reported;
  for on-chain games the receipt source remains Solana transactions, which
  this page links only when a wallet is recorded.
- World-100 model identities and token counts are peer-reported (BitLLM),
  not attested; the page repeats each audit's limitations verbatim.
- The page covers committed artifacts only. Live arena state not exported
  to the repository is invisible here by design.
- This is evidence display, not new evidence: it cannot prove an action
  happened on chain; it points at the artifacts that do.
