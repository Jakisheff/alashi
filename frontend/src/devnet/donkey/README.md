# Donkey public snapshot adapter — candidate integration

Task `DIN-DONKEY-RISK-ADAPTER-20261011-01`, exact Ivan task docs
`3e413d578adfa588402260b4f67d649dcf9ef576`, handoff SHA256
`2e43bc585c06d84b0bbc24fd72d1d14acbdebef863cbe5bb46a2f054f4fa86de`.
Base main `cbb6459aa060de53d6183ec58e48a5c3f9a13242`. This integrates the previously
delivered pure UI `736d055` and homepage confirmed-chain replay `7f93bef` into
a separate frontend branch. It imports the adapter in `/devnet`; it is not a
production release or an independently verified oracle result.

Current exact candidate: draft PR27 `058caab85e22e00e44426d3684f8c4d065c57662`,
received at docs `6cd7c4a5346eef7559e667628bd11478045fa7aa`.
Spec SHA256 `73fc5c01fa4c5b77d9f7acd3df3aebfae595917f5fcd83c371fc5f2d33763eea`;
program events SHA256 `c52d0dbfb275abd336d74b285ba19c4d8b3b338d1e9dc9118a0da6abdcb82f2f`;
indexer events SHA256 `bacd845eec4a3042add31429a1662b2fa8c02bcc422ac4db6c64a591c3626dc0`.
Exact bytes read. The event DTO is unchanged from d596220; the spec now mandates
the +30-second no-RNG technical-abort cutoff. The browser never implements that
cutoff as a result timer. Failed-transaction receipt rejection is a backend gate.

## Intent and visual direction, before implementation

The spectator understands the choice between a guaranteed Market purchase and
a paid chance, then distinguishes committed fee/action from goods delivered
after Law reveal. Use a compact dark studio ledger with cream text, a quiet blue
pending status, green verified reward and amber empty/technical status. Status
words and amounts carry meaning without relying on color. Match existing studio
typography. On phones the ledger stacks below the scene; it never covers the
face, hands or contact. No canvas or character state is owned by this component.

Key states: terms/eligibility → confirmed commitment (fee/action spent, goods
pending) → verified draw success/empty OR technical oracle abort. There is no
inferred loss on silence, timer or network failure, and no retry button. Existing
epoch0/1 remains deterministic 1 good for 1 in-game alashi. No new character pose,
emotion, intention quotation, card ownership or factory acquisition is invented.

| Lasseter principle | Application in this delivery |
| --- | --- |
| Squash and stretch | N/A: no character or object motion changes. |
| Timing | Static state persists until supplied journal changes; no timed result. |
| Anticipation | Explain fee, Action and possible outcomes before the commitment. |
| Staging | One main status/amount cluster; details collapsed, clear on phones. |
| Follow through and overlapping action | N/A: no choreography changes. |
| Straight ahead / pose to pose | Semantic states specified above; no new poses. |
| Slow in and out | N/A: static DOM, no transitions or spinner. |
| Arcs | N/A: no trajectories. |
| Exaggeration | N/A: numeric outcomes, no exaggerated gesture or effect. |
| Secondary action | N/A: existing scene and animations remain independent. |
| Appeal | Readable typography, restrained hierarchy, no competing effects. |

## Transport, replay and truth boundary

`chainSnapshot` retains its existing anonymous GET transport and default decoder.
`useChainSnapshot` supplies `decodePublicDonkeySnapshot` as the decoder. No second
request or write endpoint was added. `candidate.ts` validates the existing program,
cluster, commitment, Game/Faction, canonical signature/log/slot, ordered journal,
fee string, reason/success/goods. Complete orphan or conflicting attempt/result
journals are rejected before transcript labels appear.

The production bundle contains no DTO fixtures. Only test scripts and the external
QA package contain local fixtures, intercepted in the browser with an explicit
LOCAL DTO QA banner. They are never inserted as public game history by the app.
Candidate epoch2 history always says `Candidate API · indexed history`; indexed
receipts have canonical explorer anchors but do not assert independently checked
oracle cryptography, deployed source or live fairness. `reason=0` win and empty
remain distinct from `reason=1` technical abort. Partial history yields unavailable
in the ledger, not a missing-resolution inference. Legacy epochs0/1 keep the
historical deterministic 1-good/1-alashi Mule label and existing animation mapping.

Replay freezes the full snapshot alongside selected events. Prefix anchors use
canonical event IDs in the full journal, including same-slot other-player events.
Seek backward removes future receipts. A fresh poll cannot insert a future result
into that frozen replay; Latest explicitly returns to the newly received snapshot.
At replay end, the final prefix remains visible and the existing scene is idle.
Switching player clears the old projection. Balances are latest account state,
not reconstructed replay balances. Action stamp, oracle readiness and current
Market quote are absent from this DTO, so they stay unknown. No eligible/retry/
refund button is invented. Terms are 5 in-game alashi + one Action, 60% five goods,
40% zero, both fee and Action spent. Entry-fee refunds after game abort are a
separate existing owner behavior; the donkey fee is not shown as refundable.

No new pose or lottery reaction: pending, empty, win and abort use static ledger
states and event captions. Shared six actions buy/sell/mule/bribe/vote/victory,
rig/assets, Login/owner/pairing and owner controls remain byte-identical to base.
All four homepage paths are byte-identical to previously delivered 7f93bef.

## Verification and limits

- 95 existing pure UI checks and 69 adapter/transport/replay checks pass using
  local DTOs and fetch mocks, including bad types, orphan/conflicting resolution,
  Game/player/round, legacy, prefix, eligibility unknown and anonymous request.
- Existing devnet viewer, devnet owner and browser-cookie owner checks pass.
- Build and lint exit0. Four existing warnings remain (DevnetPage warning shifted
  to47 after imports; other three unchanged); existing large smoke bundle warning.
- Actual `/devnet`: 24 state/layout cases (8 states ×1440/390/320), five seek cases,
  player switch, three API errors: 33 cases. No horizontal overflow, panel
  animations, invented submit controls or private/write API requests. Zero page
  exceptions. Twelve screenshots including inherited homepage replay. Reduced
  motion browser-emulated at320; author visually reviewed desktop, phone pending/
  abort and expanded transaction anchors. No physical-phone/new-viewer acceptance.
- Separate polling test observed three GETs: fresh resolution did not leak into
  frozen pending replay (one receipt); Latest showed success; completed replay
  returned idle while retaining final result. Conversations503 was deliberately
  mocked here and produces expected fetch console errors, not page exceptions.
- First QA harness used the visible Latest text instead of its accessible name;
  a request filter also counted JS module `/src/owner/` as API. Both harness-only
  assertions corrected. Poll harness initially counted React StrictMode's initial
  duplicate GETs as a new poll; corrected to require an actual resolution response.
  No app change was made for these harness issues. Vite companion alias scan
  warning remains; production build and the route work.

Backend review/CI merge, program/indexer/public API cutover, provider setup/funding
and a real confirmed attempt/resolve or technical-abort lifecycle remain Ivan's
release gates. No Game, transaction, faucet, provider, program upgrade, backend,
Nginx or frontend deployment was performed. No program/SBF test was run by Din.
Shared animation checks are not repeated because no motion/rig/mapping changed;
homepage replay includes previously validated six actions unchanged.
