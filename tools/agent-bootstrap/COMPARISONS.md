# Owner hypothesis comparisons

Comparison mode tests one owner's proposed instruction change against their own
baseline. It uses the existing static English advertisement brief. The two runs
have the same brief and token budget, with one shared deadline per attempt.
They have separate dependency reports and private journals. Comparison runs do
not enter duel standings.

Use an existing registered owner profile. The [duel guide](STRATEGY_DUELS.md#api-and-deployment)
describes wallet challenge authentication and owner sessions. Its
[local execution instructions](STRATEGY_DUELS.md#local-execution) cover provider
credentials and the private profile. The [26 October deployment](../../docs/ops/HUMAN_COMPARISON_DEPLOY_20261026.md)
provides the public workspace and a compatible versioned runner archive.

## Freeze the question before execution

Create baseline and candidate versions owned by the same wallet. Within their
task plans, only the instruction of one model task may differ. Task names,
dependencies and tool bindings must stay identical. The server records the
actual instruction diff. A private version can be used without publication.
Publishing a derivative still requires its parent to be public. Creating a
private comparison never publishes either version automatically.

The supported local tools are `brief_extract` and `copy_check`. Custom
executables and trading tasks are rejected in comparison mode.

Create the comparison through the owner workspace or authenticated
`POST /agents/:record/owner/comparisons`:

```json
{
  "challenge_id": "CHALLENGE_ID",
  "baseline_version": "BASELINE_VERSION_ID",
  "candidate_version": "CANDIDATE_VERSION_ID",
  "hypothesis": "Using the source fact first will make the draft easier to review.",
  "prediction": "I expect the candidate to state the source fact in its headline.",
  "decision_criterion": "Keep the candidate only if I prefer its draft after checking both against the brief.",
  "changed_task": "deliver",
  "profile": {
    "source": "provider_reported",
    "provider": "https://provider.example",
    "model": "MODEL_ID",
    "temperature": 0,
    "max_tokens": 256
  },
  "attempt_limit": 1
}
```

Use real IDs and write your own question, prediction and decision rule. These
fields, the selected versions, brief and provider profile are frozen in the
comparison ID. Changing them requires a new comparison. The attempt limit is
chosen before execution; the server accepts 1 through 5 attempts. Start with 1.

`profile.provider` is the endpoint's origin, including any non-default port.
For `https://provider.example/v1/chat/completions`, it is
`https://provider.example`. `ALASHI_MODEL_URL` must contain the full chat
completions endpoint; `ALASHI_MODEL` must match `profile.model` exactly. Supply
`ALASHI_MODEL_KEY` privately through the local environment.

The runner sends `temperature: 0` and `max_tokens: 256` on every model request.
A different provider, model or generation profile stops the runner before it
creates or recovers an attempt. An attempt prepared in the browser already has
a running deadline. Temperature zero does not guarantee identical outputs.
For a loopback mock, use `source: "test_provider"` and add
`--test-provider yes`; remote mock endpoints are rejected.

## Run the pair

Install the existing bootstrap dependency as described in the duel guide.
Create a private directory first, preferably with mode 700. The profile must be
a regular file with mode 600. Run from the repository root:

```sh
node tools/agent-bootstrap/strategy-run.mjs \
  --api https://alashi.network \
  --profile /absolute/private/profile.json \
  --comparison COMPARISON_ID \
  --attempt-client hypothesis-attempt-01 \
  --journal-prefix /absolute/private/comparisons/experiment-01 \
  --allow-provider yes
```

The example selects the deployed public arena. Use an isolated loopback arena
for test providers and development. `--attempt-client`
must contain 8 through 80 ASCII letters, digits, hyphens or underscores.

The runner creates or recovers the attempt with that client ID. It checks both
runs before starting them concurrently. Each model call still requires its own
server reservation. The owner authorizes provider requests for both sides with
`--allow-provider yes`; provider charges are external to the arena token budget.
Each reservation belongs to the random `execution_id` saved in that task's
private journal. A competing journal cannot reserve or report that task with
another ID. The runner checks the server's `execution_claimed` response before
calling a provider. Use an arena and runner that both support this protocol.
Do not combine comparison flags with `--run`, `--journal`, `--tools` or trade
executor flags.

The browser's Prepare paired attempt button starts the shared deadline
immediately. Configure the local runner and provider before using it, then use
the generated client ID. You can also start directly with the command above;
it creates the paired attempt when the runner is ready.

For the prefix above, the runner creates these mode-600 journals:

```text
/absolute/private/comparisons/experiment-01.baseline.json
/absolute/private/comparisons/experiment-01.candidate.json
```

They contain execution IDs, task responses and usage receipts. Keep them private.
Do not run concurrent copies of the same journal on different machines: its
execution IDs identify the same executor, and file locks protect only one path. Comparison questions, reports and decisions are available through the authenticated owner
API. Existing challenge briefs remain in the public challenge list, and a
version explicitly published by its owner remains public.

## Resume the same attempt

Repeat the same command with the same profile, comparison ID, attempt client ID
and journal prefix. An accepted task or completed half is not called again.
A saved response is resubmitted without another provider request, including
after the deadline. If a request may have reached the provider but its response
was not saved, the runner records unknown usage and stops instead of reissuing
it. Unknown usage is not zero cost.

If a reservation response arrives after another task fails, the runner closes
the still-prepared task with a failed receipt and zero usage. Its provider call
has not started. The same recovery applies to a prepared claimed task on a
stopped or expired run, including when the first cancellation report is lost.
These are owner-submitted records; the arena does not independently verify
provider usage. Calls already marked inflight retain unknown usage when no
response was saved.

For journals created before execution claims, saved responses can still be
reported. An unresolved reservation without an execution ID cannot be reclaimed
for a new provider call. Even a legacy prepared entry is treated as unknown
because the old protocol did not establish exclusive execution ownership.

Do not delete a journal or use a new client ID to recover a failed request.
A new client ID creates another attempt and consumes the frozen attempt limit.
After an interruption, verify that the old process has stopped before removing
its stale `.comparison.lock` or journal `.lock` files. The runner waits for
already started work in both halves before releasing its locks, even when one fails.

## Record the owner's decision

Inspect both outputs and their receipt status. Submit a reason with
`keep_baseline`, `adopt_candidate` or `inconclusive` through the workspace or
`POST /agents/:record/owner/comparisons/:comparison/attempts/:attempt/decision`.

Unresolved reservations block the decision. Both halves must be stopped or past
their deadline. Unknown usage permits only `inconclusive`. Choosing a version
requires that version's run to be completed. A saved decision cannot be changed;
an exact retry is idempotent. The decision freezes further run changes while
still allowing exact retries of already accepted reports and completed finishes.

The decision records the owner's choice. Format checks establish schema,
length, required terms and CTA compliance. They do not measure persuasive
quality or business effects. Provider usage is owner-submitted; the server does
not independently attest which model produced an answer. USD costs are not
calculated. This static scenario requires no blockchain transaction and makes
no claim that Solana is needed for its comparison.
