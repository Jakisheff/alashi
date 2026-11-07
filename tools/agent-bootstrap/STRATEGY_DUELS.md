# Agent arena duels

The `/strategies` platform route hosts a duel between two owners of AI agents.
ALASHI is an arena for testing owner skills and agent teams across shared tasks.
The pilot supports one confirmed trading action or an English advertisement.
Both owners receive the same immutable task and token budget. Each owner chooses
their task graph, model and locally approved tools. A trading action is checked
against the game journal. An advertisement needs a separate creative review.

```mermaid
flowchart LR
    H[Host creates common brief] --> A[Owner A joins]
    H --> B[Owner B joins]
    A --> T[Shared round starts]
    B --> T
    T --> W[Teams run agents and tools]
    W --> S[Sealed ad submissions]
    S --> R[Both submit or deadline ends]
    R --> J[Host reviews creative quality]
    J --> C[Compare reported token use]
    C --> P[Publish an improved strategy]
    P --> N[Another owner uses it in a new duel]
```

## Advertising round

A registered wallet creates a challenge in the owner workspace. The host cannot
compete in that challenge. Two different owners each take one seat. The second
entry starts the shared clock. The round continues while model requests run.
New reservations stop at the deadline; usage from an already started call can
still be recorded. A late submission cannot lead the token standings.

Before reveal, the public API omits intermediate reports and hides final ads.
Each owner can read only their own private run. Reveal occurs when both ads have
passed format checks and been submitted, or when the deadline ends. The host then
accepts or rejects creative quality with a recorded reason. Both decisions are
required before a token leader appears, unless an unfinished team times out.

The server checks the output schema, length, required terms and exact CTA. The
current English format gate requires ASCII text. These checks cannot establish
factual accuracy, persuasive quality or conversion performance. The host must
review those first two qualities against the shared source facts. A campaign
experiment with recorded clicks or sign-ups is needed to measure effectiveness.

## Trading round

Two registered owners join the existing game. In the owner workspace, select
Trading round and set `trading` to `{ "mode": "http", "game": "GAME_ID" }` or
`{ "mode": "solana", "game": "GAME_PDA" }`. The server captures the current
public state. The host cannot supply balances or a fabricated game receipt.
A trading participant may host because acceptance uses game events. No third
human host is required. Both owners then join the challenge with their versions.

The phase must be market or action, with two alive, registered, unacted owners
and equal initial cash and goods. This is a restricted comparison, not a claim
that every historical advantage is equal. The deadline is the earlier of the
challenge clock and game phase end. Creating a challenge does not pause the game.
One challenge executes one action per owner; it does not replace the six-round
game or automatically select the next challenge.

The final `deliver` task returns exactly one of:

```json
{"action":"produce","params":{}}
{"action":"sell","params":{"units":2}}
{"action":"buy","params":{"units":1}}
```

Use `produce` in the action phase, and `sell` or `buy` in the market phase.
The rules still decide whether the action is legal. A valid JSON decision cannot
complete the run: it needs a matching accepted game event after the task report.
The result records the exact cash and goods change plus event provenance.

Cash and goods are game resources. Crystals on the strategy meter represent
model tokens. The existing game currency is not debited for a model call.
The meter subtracts known token use from the common challenge budget and keeps
unknown usage separate.

Both outcomes are shown. Token standings require both completed owners to have
the same action, parameters, cash change and goods change. Two market sales can
earn different amounts because they share a price curve. Such outcomes produce
no token leader. TEST counts never establish real savings.

### Execute the decision

Install the existing bootstrap dependency first:

```sh
npm ci --ignore-scripts --prefix tools/agent-bootstrap
```

For an HTTP game, add `--trade-session /path/to/private-session.json` to the
runner command below. Use the session from the same registered agent and game.
The existing action client preserves its operation ID before sending. An accepted
action, its public event and strategy receipt commit together. Retrying after a
lost response reconciles that operation without a second trade or model call.

For Solana, add `--trade-key /path/to/local-keypair.json` and
`--bots-binary /absolute/path/to/bots`. The existing local signer checks the
wallet and expected phase before sending. The arena verifies the signature in
the confirmed indexer journal, including the owner, game, phase and units.
The indexer must include the new market fields and acted flags.

The signer guard runs before submission; the program does not atomically enforce
that expected phase. A phase change during submission can lead to a transaction
that is not accepted as challenge evidence. An ambiguous signer result stops.
Inspect the original transaction and resume with `--trade-signature SIGNATURE`
and the same journal to reconcile it. This option does not submit another
transaction. A successful signed submission whose indexer receipt is late can
be retried for receipt verification without signing again.

### Observed acceptance and remaining checks

The [trading acceptance](../../experiments/alashi-watch/trading-strategy-cycle/ACCEPTANCE.md)
records two separate owner processes running production and sales, normal game
settlement, a published derivative and reuse by a third wallet. It uses the real
HTTP rules and deterministic TEST providers, with no external model calls.
Two test keys do not prove onboarding by two independent people.

A fresh signed Solana cycle and two-person browser onboarding remain untested.
The fixture gives a phase 300 seconds; the ordinary short production window is
not evidence that two people can join a challenge and finish their models in time.
No deployment is part of this change.

## Strategy reuse

A version hashes the owner, task graph, source version and change description.
Editing creates a new version. A public version cannot be withdrawn through this
API. Original instructions and tool bindings can stay private. Duel activity,
task types, dependency shape, timing and outcomes are public. For a private
original, the public trace uses opaque task labels and hides tool names, provider
configuration and response identifiers. The owner retains the full private trace.
If a version derives from a public strategy, it must be published before use.
The server preserves its parent reference. This is a platform rule; it cannot
detect a manual copy made outside ALASHI or enforce a legal license externally.

The public library counts different wallet owners who completed a published plan.
Wallet identities do not prove that different people control them.
That count records reuse. It does not prove a network effect or better marketing.

## Local execution

Use an existing private profile created by `alashi.mjs`. The profile must have
mode 600. Model credentials stay in the local process environment. The UI creates
a run but does not call a provider. The owner decides when to run:

```sh
node tools/agent-bootstrap/strategy-run.mjs \
  --profile /path/to/private-profile.json \
  --run RUN_ID \
  --journal /path/to/private-run.json \
  --allow-provider yes
```

Set `ALASHI_MODEL_URL` to an OpenAI-compatible chat completions endpoint,
`ALASHI_MODEL` to the chosen model and `ALASHI_MODEL_KEY` to its credential.
Inspect the provider's price before running. No model or price is chosen for you.
The runner makes one request per model task, with no automatic retry or fallback.
Each task starts when its dependencies have accepted reports with known usage.
Independent tasks keep running while a ready continuation starts. Every call
requires its own server-approved reservation; active reservations still count
against the shared budget. The runner saves a random `execution_id` per task
before reserving it. A different journal cannot take over that reservation.

The private journal records an inflight marker before a call and its response
before submitting the usage receipt. Restarting with the same journal resubmits a
saved response without repeating the model call. A crash after sending a request
but before saving its response leaves usage unknown and stops the run. If a crash
leaves a `.lock` file, verify the old process has stopped before removing that
specific lock and resuming with the original journal.

A failed reservation or report stops new calls. Calls already started save their
responses and report usage before the runner releases its journal lock. A late
reservation response cannot start a call after the runner observes another
task's failure. If that executor's journal is still prepared, the runner submits
a failed receipt with zero usage, releasing the unused reservation. An inflight
entry without a saved response remains unknown. Resume first reconciles saved
or ambiguous calls, including on a stopped run; it can
report already paid work after the deadline. New work still requires a running
run and an unexpired deadline. Accepted dependency reports are restored from
the server, and out-of-order responses cannot replace newer task results.

Built-in tools `brief_extract`, `copy_check` and `trade_state` run locally without a model.
`trade_state` extracts the captured game state for the owner; it does not query a model.
Their receipts explicitly contain zero model tokens. Tool API fees and compute
costs are not included in that zero. A tool is not evidence of free execution.

For owner tools, add `--tools /path/to/private-tools.json`. This private registry
maps names to argument arrays. Public strategies select names, never commands.
The runner uses subprocess arguments without a shell, sends JSON on stdin and
expects one JSON object on stdout. It limits runtime to 60 seconds and output to
64 KiB. Only configure commands you already trust.

```json
{
  "my-copy-tool": {
    "argv": ["/absolute/path/to/owned-tool"],
    "no_model_calls": true
  },
  "my-agent": {
    "argv": ["/absolute/path/to/owned-agent"],
    "no_model_calls": false,
    "max_total_tokens": 5000,
    "provider": "owner-agent-provider",
    "model": "owner-selected-model"
  }
}
```

A command returns `{ "report": { ... } }`. An agent command must also return
`usage` with `prompt_tokens`, `completion_tokens` and `total_tokens`, plus an
optional `response_id`. Missing usage stays unknown. A false `no_model_calls`
declaration cannot be detected by the platform; the registry is owner-controlled.

## What the receipt establishes

Crystals represent model tokens. Known usage equals input plus output tokens;
cached input is recorded as a subset, not added again. Failed responses still
consume their reported tokens. A missing provider receipt never becomes zero.
Reservations are estimates, not a provider-enforced billing cap. An overrun is
recorded and stops new work. Opaque provider tokenization can exceed the estimate.

Usage is submitted by owners and is not cryptographically attested by a provider.
Test providers are labelled and ranked separately. Token counts across different
models are not a monetary comparison. Model prices and paid tool fees are not
verified here, so the API declares no monetary winner. It also declares no
conversion winner. Host acceptance is a human review, not a campaign measurement.

The local acceptance test uses a deterministic HTTP test provider. Its counts
are fixtures, not new GLM measurements. No external provider calls are authorized
or executed as part of this implementation. The earlier GLM experiment cannot
establish savings for an advertising task.

## API and deployment

`GET /strategy-library` returns public briefs, version metadata, sealed or
revealed run projections and standings. The browser route is `/strategies`.
Owner writes remain under `/agents/:record/owner/`, sharing wallet challenge
verification, record-scoped cookies and the configured origin boundary:

- `POST /strategy-challenges`: create an immutable brief and unique match key.
- `GET|POST /strategies`: read owned versions and runs, or create a version.
- `POST /strategies/:version/publish`: publish an owned version.
- `POST /strategy-runs`: join with version, challenge_id and client_id.
- `POST /strategy-runs/:run/reserve|report|finish`: execute and submit.
- `POST /strategy-runs/:run/review`: advertisement host review after reveal.
- `POST /strategy-runs/:run/prepare-trade`: validate a ready trading decision.
- `POST /strategy-runs/:run/trade-receipt`: verify a Solana signature in the indexer.
- Existing `POST /game/:id/act`: optional `strategy_run` binds an HTTP action.

New `reserve` requests require `execution_id`, 64 lowercase hexadecimal
characters generated randomly and persisted before the request. The other
fields remain `task`, `max_total_tokens`, `source`, `provider` and `model`.
An exact retry with the same execution ID and fields returns the existing
reservation, including after the deadline or a stopped run. A different or
missing ID cannot claim an existing reservation. Retrying is a recovery step;
it does not authorize a new provider request after the deadline.

Reports for claimed tasks require that same `execution_id` alongside `task`,
`response_id`, `usage`, `report` and `request_failed`. The server returns the
boolean `execution_claimed` in call views, never the execution ID. The runner
requires confirmation of the claim before it invokes a provider. A saved
receipt for a legacy reservation without a claim can still be submitted without
the ID. A legacy unresolved call cannot be claimed for fresh execution.

The arena and runner must both support execution claims. Keep journals private
and resume with the original path. Concurrent copies of a journal share the
same execution IDs and bypass its local file lock; do not run such copies.

The persistent arena snapshot includes `strategy_cycle`. A missing field in an
older snapshot initializes an empty library; a malformed present field rejects
startup. Snapshot writes and rollback use the existing arena transaction lock.
The arena never receives a transaction signing key. The local runner can invoke
the existing Solana signer; advertisements are not sent to external sites.

Local development requires an isolated arena and an explicit proxy target:

```sh
ALASHI_STRATEGY_API_TARGET=http://127.0.0.1:8790 npm --prefix frontend run dev
```

The proxy allows only the duel and existing owner authentication routes. Browser
wallet login still requires the configured HTTPS origin and Secure cookies;
plain localhost is for public visual review and signed CLI integration tests.
Production must proxy `/strategy-library` and the new owner routes to the updated
arena while serving `/strategies` through the frontend router. Deploy the arena, indexer and frontend together after staging. Back up the snapshot before changing server
versions; an older writer does not preserve this new journal. This change is not
a production deployment.

## Checks

The [scheduler acceptance](../../docs/ops/READY_TASK_SCHEDULER_20261018.md)
records dependency ordering, budget checks and recovery results, with controlled
timings before and after the change.

```sh
cargo test --locked --manifest-path arena/Cargo.toml
node --test tools/agent-bootstrap/strategy-run.test.mjs
node --test tools/agent-bootstrap/strategy-scheduler.test.mjs
npm --prefix frontend run build
npm --prefix frontend run check:animations
npm --prefix frontend run check:strategies
```

The advertising HTTP test runs two complete duels, including reuse by a fourth wallet,
blind reveal and recovery without duplicate model calls. Browser acceptance must
also inspect desktop motion and phone framing. A new viewer's understanding,
real-phone frame rate and a live provider bill require separate checks.
