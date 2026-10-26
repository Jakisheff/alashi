# Local experiments with a private brief

Observed implementation: `local-experiment.mjs` lets an owner compare one
instruction change on their own brief without sending the experiment to ALASHI.
It uses the existing task scheduler and provider adapter. All coordination, task
responses and the owner's decision stay in a local directory.

```text
Owner's question → frozen brief and instruction change → paired execution
→ owner reviews evidence → saved decision → selected local plan
                                      → optional reviewed public card
```

The current execution scenario produces a static English advertisement. Its
checks cover the output schema, word limits, required terms and CTA. They do not
measure customer response or business results. This is a local experiment, not
an onchain computation or an independently attested result.

## Data boundary

The runner makes no ALASHI HTTP requests and needs no ALASHI wallet or account.
The configured model provider receives the brief, task instructions and
dependency reports when you authorize `run --allow-provider yes`. A remote
provider therefore sees those private inputs. To keep that boundary on your
machine, configure a compatible model endpoint on loopback. Test providers must
use a loopback URL.

The provider key is read from `ALASHI_MODEL_KEY` and is not written into the
manifest or journals. Files use mode 600 and the experiment directory uses mode
700. They are not encrypted. A process with access to your account can read
them, and moving the directory into a synchronized folder can send it to that
folder's service.

Public export requires a separately authored card and explicit CC0-1.0 consent.
The runner copies only that card's allowed fields. It never summarizes or copies
the private brief, instructions, decision reason, reports or execution digest
into the export. Review your own card text for private information before
exporting it; the runner cannot establish whether your prose is safe to publish.

## Prepare and freeze

Use Node 20 or newer and the existing bootstrap dependencies described in
[STRATEGY_DUELS.md](STRATEGY_DUELS.md#local-execution). Run from the repository:

```sh
node tools/agent-bootstrap/local-experiment.mjs init --dir /absolute/private/experiment-01
```

The parent directory must exist. The command creates a new mode-700 directory
and a mode-600 `manifest.json`. It does not contact a model or ALASHI. Replace
the example brief and provider profile before execution.

The manifest records:

| Field | Owner's input |
| --- | --- |
| `hypothesis` | The proposed change and why it might help |
| `prediction` | The expected observable difference |
| `decision_criterion` | The condition for selecting a version |
| `failure_conditions` | What would invalidate the proposed inference |
| `brief` | The same private facts and task for both versions |
| `baseline`, `candidate` | Task plans with exactly one different model instruction |
| `profile` | Endpoint, model, source, temperature 0 and maximum 256 output tokens |
| `budget` | Maximum accounted tokens per version |
| `duration_seconds` | Shared attempt duration, between 60 and 900 seconds |

Each plan has up to eight tasks in dependency order. The last task is a model
task named `deliver`. The local tools are `brief_extract` and `copy_check`.
Arbitrary executables and trading operations are rejected. All tasks must feed
the final task. Plans must have identical names, dependencies and tool bindings.

The brief has these fields: `title`, `audience`, `task`, `facts`,
`required_terms`, `cta`, `max_words`. Keep private source material in these
fields; the runner does not upload them to an arena challenge. The final output
uses the existing English ASCII format contract.

Configure the exact provider endpoint in `profile.url` and the model in
`profile.model`. Use `source: "provider_reported"` for your model provider.
Use `source: "test_provider"` for a local mock, with a loopback endpoint.
Keep credentials out of the URL and JSON.

```sh
node tools/agent-bootstrap/local-experiment.mjs freeze --dir /absolute/private/experiment-01
```

Freeze saves the manifest with a random salt and a digest. A changed question,
brief, plan, budget or provider setting makes further execution fail. Create a
new directory for a new experiment. The digest detects changed inputs locally;
it is not external proof of when the owner first wrote the prediction.

## Execute and recover

Set the provider key privately in your environment, then run:

```sh
node tools/agent-bootstrap/local-experiment.mjs run \
  --dir /absolute/private/experiment-01 --allow-provider yes
```

The process starts the shared deadline and runs the two versions with separate
dependency reports. It records each reservation locally before starting work.
Known token usage is reported separately from `cost_usd: null`; this version
does not calculate monetary cost. Provider usage is a submitted receipt, not an
independent measurement.

Repeat the same command to recover the same attempt. Completed work and saved
responses do not trigger another model call. If a call was marked inflight but
its response was not saved, the result becomes `inconclusive` with unknown
usage. The runner does not retry that call. Unknown usage is not zero usage.

The experiment lock prevents concurrent operations in the same directory.
Per-version journal locks remain part of the existing runner. After a process
crash, verify that the old process has stopped before removing stale
`.experiment.lock`, `baseline.journal.json.lock` or
`candidate.journal.json.lock` files. Do not delete journals to retry a request.
Do not run copied directories concurrently on different machines: filesystem
locks do not coordinate copies.

## Review, decide and apply

The `report` command prints private evidence to your terminal:

```sh
node tools/agent-bootstrap/local-experiment.mjs report --dir /absolute/private/experiment-01
node tools/agent-bootstrap/local-experiment.mjs decide \
  --dir /absolute/private/experiment-01 \
  --choice adopt_candidate --reason "The headline states the supplied fact and I prefer this draft."
node tools/agent-bootstrap/local-experiment.mjs apply --dir /absolute/private/experiment-01
```

Choices are `keep_baseline`, `adopt_candidate` and `inconclusive`. A decision
cannot be changed; an identical retry returns the saved decision. Incomplete
execution or unknown usage permits only `inconclusive`. Outstanding
reservations must first be reconciled by `run`.

`apply` writes the selected plan to `active-strategy.json`. It does not mutate a
live game or cloud strategy. Use this file as the baseline of the next local
experiment:

```sh
node tools/agent-bootstrap/local-experiment.mjs init \
  --dir /absolute/private/experiment-02 \
  --baseline /absolute/private/experiment-01/active-strategy.json
```

The next manifest contains that selected baseline and a proposed single
instruction change. Review the next question and candidate before freezing it.

## Export a Public Goods card

Write a separate mode-600 JSON file, for example `reviewed-card.json`, containing
only these fields. The content below is an example, not a measured outcome:

```json
{
  "title": "Source fact first",
  "mechanism": "Move a supplied fact into the headline.",
  "task": "Review a static advertisement draft.",
  "conditions": "The brief and generation settings were held fixed.",
  "result": "The owner preferred the candidate in this local example.",
  "limitations": "No customer response or sales effect was measured."
}
```

After recording the decision, explicitly authorize the export:

```sh
node tools/agent-bootstrap/local-experiment.mjs export \
  --dir /absolute/private/experiment-01 \
  --card /absolute/private/reviewed-card.json \
  --out /absolute/private/public-card.json --consent yes
```

The output adds `client_id`, `consent: true` and `license: "CC0-1.0"`. The
client ID stays the same on an exact retry. The saved export intent cannot be
changed into a different card. No publication occurs in this command.
An existing output file is accepted only when it already contains that exact
export. A different existing file is preserved and the command fails.
The title is limited to 100 UTF-8 bytes; mechanism, task and conditions to 2000
bytes each; result and limitations to 4000 bytes each. These bounds match the
publication API.

Import `public-card.json` into the workspace's Public Goods library, review the
preview and confirm publication there. The server records the evidence status
as `owner_reported_local`. It does not accept a client-supplied claim that a
local run was independently verified. Publishing an inconclusive result is
allowed when the card explains its limitation.

## Verification

```sh
node --test tools/agent-bootstrap/local-experiment.test.mjs
```

The tests cover frozen inputs, private permissions, concurrent use, saved
response recovery, ambiguous requests, token budget, immutable decisions,
applying the chosen plan and an allowlisted export without a private sentinel.
The CLI test uses a temporary loopback HTTP provider and rejects fetches to any
other destination. It does not call a paid provider or establish behavior of
an external model service.
Saved-state checks detect inconsistent receipt totals, provider identities and
completed results. They do not prevent an owner from fabricating a consistent
local record. The Public Goods library therefore labels these cards as
owner-reported local evidence.
