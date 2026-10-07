# Private Node two-harness check

This is an operator-only developer integration test, not public user onboarding. The public agent instruction is [docs/agent.md](../agent.md). The reviewed controller candidate is `tools/agent-bootstrap/private_match.py` at `39cfad6`; freeze and review its exact code before a Mac run. Development and offline tests occur in the server code clone; execute the frozen copy on the Mac that already holds the two private profiles and provider CLIs.

## Boundaries

- Use two distinct, already registered Node profiles only. Their directories must be mode 700 and `agent.json` / game sessions mode 600. Do not print, copy to the server, or share their contents.
- The controller requires `http://127.0.0.1:18094` and an operator-owned SSH tunnel to staging `127.0.0.1:8094` on the server. Never point it at canonical 8095 or the public domain.
- It calls Node `start --existing-only true`, then `state`, `act`, and exact pending-operation `retry` if needed. It must not create a wallet, use Solana RPC, send a Memo, fund a wallet, or register a new identity.
- Model choice is limited to a bounded subset of market/action/law candidates; the backend remains the final authority on legality. Codex Sol and OpenCode GLM 5.3 Flash have tool permissions disabled; there is no heuristic choice fallback.
- Each model has at most 20 decision attempts within one 30-minute run; each phase choice is capped at 20 seconds and shortened by the live phase deadline. Do not rerun automatically after failure or fund a profile to make the test pass.

## Operator sequence on Mac

Prerequisites: Python 3, Node.js 20+, npm, configured `codex` and `opencode` CLIs, and the existing private profiles. Keep provider credentials in their normal local CLI stores. Verify the Mac copy has the same reviewed Git SHA or matching file hashes for the controller, `alashi.mjs`, and `package-lock.json`; do not execute a moving server checkout.

~~~sh
npm --prefix tools/agent-bootstrap ci --ignore-scripts
ssh -N -L 127.0.0.1:18094:127.0.0.1:8094 ivan-aws
~~~

In a separate Mac terminal, with `ALASHI_CODEX_PROFILE` and `ALASHI_OPENCODE_PROFILE` already set to the respective existing private directories:

~~~sh
umask 077
python3 tools/agent-bootstrap/private_match.py --url http://127.0.0.1:18094 --codex-home "$ALASHI_CODEX_PROFILE" --opencode-home "$ALASHI_OPENCODE_PROFILE" > "$ALASHI_PRIVATE_RESULT_LOG" 2>&1
~~~

Set `ALASHI_PRIVATE_RESULT_LOG` to a new private local file before the run; retain the command exit code. Do not put profile paths, record IDs, wallet keys, signatures, tokens, or provider credentials in a shared report. Keep raw output local; share only redacted event counts, terminal status, duration, code SHA, and the exact stop reason.

## Reading evidence

`model_decisions` counts attempts, including timed-out calls; it does not mean valid choices or accepted moves. Read `model_timeouts`, `stale_choices`, `rejected_actions`, and `accepted_actions` separately. `both_models_acted` means each model had at least one accepted action; it alone does not prove a finished match. Claim a completed match only with a valid `status: finished` terminal result plus both-model accepted-action evidence. A stopped run is a stopped run even if earlier actions were accepted.

On 07 Oct, the first stopped run accepted one action from each model, then a generic OpenCode decision failure; its subtype remains unproven. The bounded recovery run finished private Game 3 / party 21 from controller source `39cfad6`: Codex had 17 attempts, 16 accepted actions, and 1 timeout; OpenCode had 17 attempts, 11 accepted actions, and 6 timeouts. Both had 0 stale choices and 0 rejected actions. The private backend terminal result agreed, saved profile hashes were unchanged, and no new Memo or funding occurred. Preserve the first failure separately. Seven model timeouts remain observed, and this private result is not an external HTTPS or public onboarding check.
