Independent final scope review of findings 1 and 2, 7 November 2026.

Reviewed head `1ac0315b7a2d29348d3816063641cd2e5570b0fc` against main `4cdfa4cb20ee6e6d7cd33f849884f062caa39881` in `/Users/amir/Documents/Codex/worktrees/alashi-session-archive-pr-20261107`. The checkout was clean. This review read the diff, callers and saved evidence; it did not rerun tests or change the repository.

No source or scope blocker found. Approval is conditional on the final PR head passing the required CI jobs. This report does not claim that gate has completed.

The application diff contains exactly two files. `bots/src/session_cli.rs:932` adds five production lines. `indexer/src/chain_api.rs:131` and `:177` add nine production lines. The remaining changes in those files are regression tests and their local fixtures. The only workflow change is the three-line session test step at `.github/workflows/ci.yml:74`. All other changed files are documentation, logs or historical test evidence. No source under `rules/`, `programs/`, `arena/`, `frontend/` or agent-bootstrap changes. Findings 3, 5 and 8 retain their main behavior.

For finding 1, the new branch clears `pending_act` only when the response has the matching numeric `op_id`, `op_consumed: false`, `ok: false` and one of `bad_action`, `bad_params`, `bad_choice`. It leaves `next_op_id` unchanged so a corrected action can use the same available ID. Transport failures, malformed replies and uncertain server outcomes retain the normalized pending request. The existing matching consumed-response branch still clears pending and advances the ID; this is the required final acknowledgement, including a consumed failure. The policy agrees with `tools/agent-bootstrap/alashi.mjs:307`. The fresh tests exercise corrected actions, ambiguous responses, refused transport and a received request whose response is lost.

For finding 2, an unarchived terminal cache entry with archive storage configured no longer takes the permanent cache-hit path. After acquiring the existing per-game gate and rereading the cache, `get` retries the archive from the resident value and stores the result without RPC. An unsuccessful write leaves `archived` unset. Live cache hits, configurations without archive storage and already archived cache hits retain their existing behavior. The regression covers recent and aged terminal entries, a failed rename, storage recovery, simultaneous callers, restart loading and zero RPC. It also verifies that an already archived cache entry does not start rewriting missing storage. The archive success marker still depends on successful write, sync and rename.

The complete source files match their original reviewed fixes by Git blob, SHA-256 and byte comparison:

| File | Original fix commit | Current blob |
|---|---|---|
| `bots/src/session_cli.rs` | `5321258dbfcf89124c1c30b8b803e22c7527f5d2` | `fcb825d4243b0518b6c0391cb08f00dcf03949c0` |
| `indexer/src/chain_api.rs` | `f32168384dff8a8b0d30287605a5f50ab6c44c19` | `52cd27973bb89e3be089002a8caf60f12a5b79a7` |

The local evidence under `docs/ops/session-archive-20261107/` is consistent with the claims:

| Log | Observed result |
|---|---|
| `finding-1-red.log` | Compiled; one behavioral failure, `bad_action must release the rejected action`; exit 101. |
| `finding-2-red.log` | Compiled; one behavioral failure, cached archive marker Null instead of true after recovery; exit 101. |
| `finding-1-green.log` | 19 session tests passed; exit 0. |
| `finding-2-green.log` | 22 indexer tests passed across five nonempty test binaries; exit 0. |
| `rules.log` | 40 rules tests passed; exit 0. |

The logged compilation paths point to this integration checkout. The five logs record commands and exits but do not embed a run timestamp or commit SHA. Their fresh execution is the root runner's record; this review independently checked their content and source consistency. All 25 published log hashes match `log-manifest.json`. Historical mutation evidence is explicitly separated and is not presented as a new run or complete coverage score. `git diff --check` passes.

The new CI command is `cargo test --locked --manifest-path bots/Cargo.toml session_cli::tests`. It selects the 19 session tests, closing the previous gap where bots compilation and other test filters did not execute them. Existing mandatory arena, indexer, rules and program checks remain intact, including `bash tools/build_sbf.sh` before `cargo test --locked -p alashi`. Workflow SHA-256 matches the prior CI review: `3540fb705c1c44b074e9656edda4e5cd9aa5ec14deb7b492fb27245ecf16f7fd`.

All 22 newly added or changed local Markdown link targets resolve. A broader scan of changed documents found one pre-existing broken target at `docs/100_MOC/MOC-Ops.md:70`, `../debriefs/`; that target already exists in the base text and is unrelated to this PR. The newly added GitHub links for PR59, PR33, PR69 and main run 42372824580 resolve through the GitHub API and agree with the closing record. PR59 and PR69 are merged, PR33 is closed, and the main run succeeded at `4cdfa4cb20ee6e6d7cd33f849884f062caa39881`.

Documentation leaves final PR CI and plan/fact closure pending. The MOC description still says GREEN is expected even though the linked report records GREEN completion; update that wording with the final CI closure. This is a documentation follow-up, not a source blocker. No live game, production RPC, funds, browser or new build was used by this reviewer.
