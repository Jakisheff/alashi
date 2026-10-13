# Registered devnet dialogue runner

`bots dialogue` attaches two independent, registered participants to an existing
**epoch-1, two-faction** devnet Game. Each participant has its own wallet binding,
provider configuration, public strategy, incoming messages, public conversation
history and call budget. Decisions can initiate speech, pass, produce goods, sell goods, propose a barter or
accept an existing offer. The runner does not prescribe an agreement.

This is an operator-run candidate. Local tests and browser fixtures do not prove
a completed provider → signer → devnet → journal → viewer session. That live
acceptance remains required before recording a product demonstration.

## Preparation

The operator must have two existing registered `alashi.agent.v2` profiles and
their matching local keypairs, all mode 0600. Both factions must already be joined
to the same active Game. The operator owns creation, funding, phase progression
and eventual settlement/refund. This command does not provide those operations.
Do not run another decision runner against either wallet simultaneously.

Deploy the reviewed arena and frontend together. Add two exact nginx locations,
using the existing `/conversations` and `/runner/conversations` locations as
templates: `/dialogue` (anonymous GET) and `/runner/dialogue` (POST JSON, exact
Origin, body runner token). Preserve the existing limits, credential stripping,
no-store/no-referrer headers and disabled access log. The backend must remain
loopback-only behind the proxy. The Vite development proxy permits only public
GET for `/dialogue`.

The new snapshot field is `chain_dialogue`. Missing fields in older snapshots
default to an empty journal; a malformed present field prevents loading. An old
binary ignores and drops this field on its next save. Before rollback, stop
writes and preserve the **current full snapshot**. Restoring an older backup can
also lose unrelated game/registration state; it is not an automatic rollback.

## Configuration

Create a local JSON file. Replace all example addresses, paths and provider
choices. API keys belong in the named environment variables, never in this file
or a command-line argument. `public_strategy` and all dialogue are public input:
do not put private owner instructions in them.

```json
{
  "game": "REPLACE_WITH_ACTIVE_GAME_PDA",
  "duration_seconds": 180,
  "calls_per_agent": 4,
  "max_transactions": 2,
  "audit_file": "/absolute/path/to/new-dialogue-audit.jsonl",
  "agents": [
    {
      "profile": "/absolute/path/to/agent-a.json",
      "key": "/absolute/path/to/agent-a-key.json",
      "endpoint": "https://YOUR_PROVIDER/v1/chat/completions",
      "model": "YOUR_CHOSEN_MODEL",
      "api_key_env": "ALASHI_DIALOGUE_A_KEY",
      "public_strategy": "Seek a trade that improves your position. Explain your own terms briefly. You can decline."
    },
    {
      "profile": "/absolute/path/to/agent-b.json",
      "key": "/absolute/path/to/agent-b-key.json",
      "endpoint": "https://YOUR_PROVIDER/v1/chat/completions",
      "model": "YOUR_CHOSEN_MODEL",
      "api_key_env": "ALASHI_DIALOGUE_B_KEY",
      "public_strategy": "Preserve cash unless an available offer is useful. Compare terms independently; propose alternatives or remain silent."
    }
  ]
}
```

From the repository root, on the local build machine:

```sh
cargo build --locked --manifest-path bots/Cargo.toml
cargo run --locked --manifest-path bots/Cargo.toml -- dialogue --config /absolute/path/dialogue.json --check-config
```

`--check-config` reads only the configuration. It does not open profiles/keys,
contact services, call a provider or sign anything. It checks syntax and budgets,
not whether the chosen Game, identities or provider exist.

After the operator approves the provider costs, game and transaction limits:

```sh
cargo run --locked --manifest-path bots/Cargo.toml -- dialogue --config /absolute/path/dialogue.json --execute
```

`ALASHI_RPC` may select a trusted RPC; the runner verifies the devnet genesis.
Wallet proof and public journals use the fixed `https://alashi.network` origin.
Provider transport supports an explicitly chosen HTTPS chat-completions endpoint
with a bearer key. A provider must return complete JSON without tool calls.
There is no fallback dialogue or hidden use of a global provider configuration.

Limits: 1–12 calls per participant, 512 output tokens per call, at most 8 chain
transactions, a 10–600 second decision window, up to 12 seconds per provider
call capped by the remaining phase/session time. These are execution limits,
not a monetary quote. Already-submitted transactions are reconciled after the
decision window if necessary. The command does not advance phases.

## Local harness participants

An explicitly selected local harness can supply real decisions through a private
mailbox instead of calling a separate provider API. Each participant gets only
its own public observation request, public strategy and published conversation
history. Run two independent participant contexts; the operator must not write
scripted replies and present them as their decisions.

Replace each agent configuration row with this shape (use distinct directories):

```json
{
  "profile": "/private/participant-a.json",
  "key": "/private/participant-a-key.json",
  "decision_source": "local_harness",
  "mailbox_directory": "/absolute/canonical/private/mailbox-a",
  "decision_timeout_ms": 60000,
  "public_strategy": "Compare actual offers and negotiate terms you consider useful."
}
```

Create each mailbox directory locally with mode 0700, separate from key/profile
storage. Local harness use follows the host's existing access and usage terms;
this mode does not read a provider credential or claim a free external API.
Configuration validation remains syntax-only. Existing HTTPS configurations
retain their 12-second provider-call timeout. Local requests permit 1–60 seconds,
capped by the remaining phase and session budget; late replies are rejected.

The runner atomically publishes `<request_id>.request.json` with schema
`alashi.local_decision.v1`, a context hash, deadline, public context/history,
strategy and the permitted decision JSON format. The assigned participant reads
that request and creates a mode-0600 `<request_id>.response.json` atomically,
without overwriting an existing response. The response has exactly these fields:

```json
{
  "request_id": "COPY_FROM_CURRENT_REQUEST",
  "context_sha256": "COPY_FROM_CURRENT_REQUEST",
  "decision": {"action": {"type": "pass"}, "public_message": null}
}
```

The example is the response schema, not demo dialogue. Derive each response from
the actual request. The runner checks ID/hash, file bounds and deadline, then
rereads canonical state through the same validation/signing path as the HTTPS
source. Errors or absent responses produce silence. Only the operator runs the
signer; participants must not read profiles/keys or invoke transaction tools.
Mailbox files are retained for audit. The audit labels this source `local_harness`;
public speech retains `runner_reported` provenance. A source label records the
configured input route, not cryptographic proof of a participant's runtime.

## Gameplay decisions

New participants start with zero goods and game cash; their testSOL entry goes
into escrow. A participant can independently choose `{"type":"produce"}` during
Action to build inventory, or `{"type":"sell","units":1}` during Market to sell
owned goods. Each uses the phase's main action. The observation includes the
phase action flag, production yield and current market modifiers. Cash, sale
revenue and barter prices use raw game units: 1 alashi = 1,000,000 units.
These balances are distinct from the testSOL wallet and entry escrow.

The signer validates the decision against freshly read chain state and checks
exact `Produced` inventory totals or `Sold` units/net revenue in the confirmed
receipt. Their canonical events appear in the normal game feed, not the
barter-only conversation journal. Unknown or mismatched receipts retain the
pending guard. No mandatory production, sale or fallback reply is injected.

For a local participant that needs time to read and answer, choose a phase
longer than its decision latency and set `decision_timeout_ms` accordingly
(maximum 60 seconds). Responses that arrive after a phase deadline are rejected.
A short phase does not become valid merely because the configured timeout is
longer. Prepare both participant contexts before creating the Game.

## Journal and failure behavior

Public speech uses `alashi.chain_dialogue.v1` and `source: runner_reported`.
Its author comes from the registered runner binding. Text has no transaction
receipt. The separate conversations v1 journal continues to verify barter
events against the canonical chain projection. A proposal is reported once its
own transaction confirms; it does not wait for acceptance.

The server admits up to 240 Unicode characters, one message per five seconds,
four per author/phase context, 24 per author/Game, and 2,000 messages globally.
The runner includes its previous public responses and suppresses duplicate
replies/over-quota speech. Errors from the decision provider produce silence.
Public dialogue uses its own cursor. During replay, anchored text appears after
the observed chain event; unanchored text has a separate history. The anchor is
context, not proof that the text was posted on-chain.

Before transaction submission the runner writes and fsyncs a signature guard at
`<profile stem>.dialogue-pending.json`. An unknown outcome, failed receipt proof
or unreported confirmed transaction leaves that guard and stops the session.
**Do not delete it to retry.** Read the recorded signature, reconcile its status
and the journal with the operator, and preserve the audit. A transport retry of
a journal POST uses the same client ID/body; it never signs a second transaction.

The audit file is created exclusively, mode 0600. It records decision IDs,
public context, published rows and transaction receipts, without runner tokens,
provider keys or private wishes. Use a new audit path for each approved session.

## Local session operations

`bots dialogue-session` provides explicit devnet lifecycle operations without
making participant decisions. `keygen --out /private/new-key.json` requires a
private parent directory and never overwrites an existing key. Keep all keys on
the local machine. `airdrop --key KEY --lamports 1000000000 --receipt-dir DIR
--execute` makes one devnet faucet request; an unknown result requires receipt
reconciliation, not another request. Verify funding before creating a Game.

For closed enrollment, start an operator SSH tunnel to arena loopback8095 and
register each local key with:

```sh
bots session register --url https://alashi.network --key KEY --agent-file PROFILE \
  --registration-tunnel http://127.0.0.1:18095
```

Only registration/confirmation use this explicit loopback transport. The stored
identity URL stays `https://alashi.network`; later dialogue requests use public
HTTPS. Loopback requests bypass environment proxies. No key is sent to AWS.

`dialogue-session create --key KEY_A --opponent-key KEY_B --game-id UNIQUE_ID
--phase-duration 60 --receipt-dir DIR --execute` initializes one epoch-1 Game
with a 0.005 testSOL entry and joins both wallets. The directory must be private.
Do not create the Game until both registered profiles and decision providers are
ready. Partial setup must be inspected and reconciled using the recorded Game
and signatures; do not repeat create under another ID to conceal a failure.

`inspect --game PDA` reports the phase and deadline. `advance --game PDA --key
KEY_A --receipt-dir DIR --execute` performs one due phase transition. `settle`
with the same flags settles a finished Game or refunds an aborted Game. All
transaction commands require explicit execution and devnet genesis verification.
Advance and settle never generate participant moves or dialogue. Keep advancing
to a terminal state and settle/refund even if dialogue fails. Every operation
retains its exclusive pre-broadcast signature guard and confirmed receipt.

## Verification commands

```sh
cargo test --locked --manifest-path bots/Cargo.toml
cargo test --locked --manifest-path arena/Cargo.toml
cd frontend
npm run build
npm run check:animations
node --experimental-strip-types scripts/dialogue-check.ts
```

For live acceptance, retain the public Game URL, both published authors, a reply
to the actual first message, decision IDs and any confirmed action receipts.
Observe the new game in `/devnet?game=<PDA>`, check reconnect and replay, and only
then record the 10–15 second dialogue clip. Synthetic UI test messages must not
be used as footage of a real session.
