# Finding 1 independent review

observed: read-only review of `5321258dbfcf89124c1c30b8b803e22c7527f5d2` against main `4cdfa4cb20ee6e6d7cd33f849884f062caa39881`. The original repair checkout remained clean. No builds or tests were run by this reviewer. Earlier RED/GREEN logs are historical evidence only.

No material production defect was found in the proposed fix. The production change is five lines in `bots/src/session_cli.rs:932`: release pending only when the response identifies the same operation, explicitly says `op_consumed=false` and `ok=false`, and reports `bad_action`, `bad_params` or `bad_choice`. The operation counter stays unchanged, allowing the corrected action to use that unconsumed ID.

## Verified contract

| Response or boundary | Required state | Source evidence |
| --- | --- | --- |
| Exact definitive unconsumed rejection | Clear pending, retain next operation ID, return the sanitized rejection | `session_cli.rs:932`; same policy in `tools/agent-bootstrap/alashi.mjs:307` |
| Transport failure, lost response or malformed JSON envelope | Retain the persisted operation ID and normalized payload for retry | Pending is saved before HTTP at `session_cli.rs:917`; HTTP errors return before any response mutation at `:927`; envelope validation at `:276` |
| Mismatched or missing operation identity, unknown/transient unconsumed error | Retain pending and prevent replacement with another action | Exact identity and allowlist checks at `:932`; replacement guard at `:904` |
| Matching consumed reply, including a consumed rule rejection with `ok=false` | Clear pending and advance next ID; do not reuse a consumed ID | Existing branch at `session_cli.rs:928`; server records consumed results at `arena/src/api.rs:2229` |

The last row clarifies the task wording: consumed replies are final acknowledgements, so their identity is not kept pending for another action. Only uncertain outcomes retain the same pending operation. The proposed fix preserves this existing consumed behavior.

The server returns unknown action before applying any rule (`arena/src/api.rs:2184`). Invalid action parameters return before consumption (`:2167`, `:2187`, `:2192`, `:2305`), and invalid vote choice returns without an action log (`:2512`). The wrapper marks responses without an action log unconsumed (`:2208`). Storage failure rolls back state and returns `storage_failed` with false consumption (`:2237`); that code is correctly excluded from the release allowlist. `op_conflict`, `op_stale` and `op_out_of_order` also remain outside it.

`act` for a v2 session and explicit `retry` both call this same function (`session_cli.rs:976`). Session locking and atomic private-file persistence are retained. `server_code` supports both a string error and an object containing `code` (`:289`). HTTP rejects malformed envelopes or inconsistent success status before reaching the branch (`:278`).

The server API, its relevant tests, bootstrap implementation/tests and CLI dispatcher have no diff between the reviewed commit and current main. The session source blobs are main `71bd76f5de2c530720a74b1467de893b2ac94c2f` and candidate `fcb825d4243b0518b6c0391cb08f00dcf03949c0`. `git diff --check` for that file passed.

## Existing coverage and small gaps

The candidate tests cover the typo, missing sell parameter and invalid vote choice; they check pending removal, unchanged ID, corrected request payload, and advancement only after consumption. The uncertainty matrix checks malformed identity/flags, missing error, storage failure, unknown code and inconsistent success. Separate tests cover refused transport and a server that receives a request but drops the response; the retry is checked for identical payload and ID. The existing bad-token test remains.

Two optional protocol regression cases would make the boundary more explicit, but neither exposes a bug in the reviewed code:

- A matching consumed rule rejection, such as `ok=false`, `error=WrongPhase`, `op_consumed=true`, must return an error while clearing pending and advancing the ID.
- Add `op_conflict`, `op_stale` and `op_out_of_order` to the existing uncertain-response table. Bootstrap already checks two of these; the Rust production allowlist already preserves all three.

No additional helper is needed for either case. Do not broaden the allowlist to all server errors or infer consumption from HTTP status alone.

## Fresh RED without a helper-related compile failure

The safest reuse is to copy the candidate test/fixture hunks into a checkout based on current main while leaving the five production lines absent. Include `prepared_v2` and `fixture_responses` if copying their callers. Run only the existing definitive-rejection test:

`cargo test --locked --manifest-path bots/Cargo.toml --bin bots session_cli::tests::v2_definitive_rejection_allows_corrected_action_at_same_id -- --exact --nocapture`

Expected RED: the test reaches the assertion that rejected pending is null and fails for the first `produe` response. A compilation error, connection error or fixture timeout is not the finding reproduced. Then add only the five production lines and rerun the same test, followed by the focused session suite and the required project checks.

An alternative with no new helper at all is to clone the existing main test `v2_pending_action_survives_rejection_and_retries_same_id`, which already has the complete inline session JSON and uses the existing `fixture`. Change its first response to a matching unconsumed `bad_action`, submit `produe`, assert pending is null and next ID is 1, then submit corrected `produce` and assert both requests used ID 1. This uses only helpers already present on main.

The existing test-only fixture changes merely support dropped responses and bounded waits. They are not part of the production fix. Findings 3, 5, 8 and game rules remain outside this review.
