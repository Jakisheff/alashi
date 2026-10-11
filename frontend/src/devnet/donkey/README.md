# Donkey risk UI — presentation boundary

Task `DIN-DONKEY-RISK-UI-20261011-01`, exact Ivan handoff docs
`89f080c0a202c56e9d5e1fb884049a436b4fc2d6`, SHA256
`ef8e87f90a54c5e0cf8526c99d09f5e546466d11ffebb0d6b83ff9eca4cdadce`.
This is a pure presentation candidate. It is not imported by a production route.
Compiled IDL, normalized public JSON, attempt identity, ordering, eligibility,
oracle availability and confirmed epoch2 receipts remain Ivan dependencies.

Candidate update: Ivan docs `b8fce9569ed3bdf38fe7a1e3532e06dd328dc3f7`,
draft PR27 exact source `1de5d3a5aa5c0d70c8224a79f8b470ff3b66580b`.
Spec SHA256 `d2f1b2da1efaee4801241acb95f3f60b0e33c11d4d676e35997c84e871c7e86f`;
program events SHA256 `c52d0dbfb275abd336d74b285ba19c4d8b3b338d1e9dc9118a0da6abdcb82f2f`;
indexer events SHA256 `bacd845eec4a3042add31429a1662b2fa8c02bcc422ac4db6c64a591c3626dc0`.
All three exact source files were personally read and hash checked. The fetched
backend tip advanced to `863e4eea0d5d02233bfbb5cbd751fe332f84f13a`; those three
schema files are unchanged between the heads. The adapter targets the handed-off
candidate explicitly, and never declares that candidate merged or deployed.

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

## Truth and integration boundary

`model.ts` accepts an internal presentation input, **not** program/indexer wire
JSON. An adapter must be written against Ivan's final verified public contract.
`candidate.ts` is a pure decoder for the handed-off source-candidate JSON. It
reuses existing envelope validation, preserves full journal position, associates
attempts by Game/Faction/round and returns `contract: 'candidate-only'`. It does
not fetch or automatically authorize confirmed-mode rendering. A reviewed,
deployed API/IDL/availability receipt remains required for production import.
Confirmed mode requires that verified contract and complete journal; local
fixtures have a separate source label and never expose transaction links.
Selecting a replay prefix is the caller's responsibility: projection is stateless
and respects supplied journal order, including multiple transactions in one slot.
It validates matching Game/Faction/round, attempt association, fee, result reason,
event identity/signature/slot, contradictory duplicates and multiple resolutions.
These checks do not independently prove oracle cryptography or chain inclusion.

Epoch2 target: 5 **in-game alashi**, one Action; 60% +5 goods / 40% zero goods.
Both outcomes retain the fee. Goods appear only after verified Law resolution.
Technical expiry/abort grants zero goods without being a 60/40 draw. Final round,
wrong phase, insufficient cash, overflow, inactive faction, already acted and unavailable oracle
are shown explicitly. Unknown eligibility inputs disable eligibility.
Market amount comes from a supplied dynamic quote; unavailable quote stays
unavailable. Presentation has no submit, wallet, owner session, pairing, RPC,
fetch, randomness generation, retry or storage effects.

## Verification

- `node --experimental-strip-types --no-warnings scripts/donkey-risk-ui-check.ts`:
  95 semantic assertions passed using isolated local fixtures. Covers both draws,
  technical abort, missing/mismatched provenance, contradictory duplicates,
  repeat delivery, same-slot journal order, backseek prefixes, all eligibility
  reasons, u64 money boundaries, legacy epochs and draft-contract denial.
  Candidate JSON checks cover decimal-string fee, Boolean success, numeric reason,
  epoch/actor/round/program/commitment validation and partial history; transaction
  event_index is correctly replaced by full journal position for presentation.
- `npm run build` and `npm run lint`: exit 0. Existing large smoke bundle warning
  and four warnings in unchanged files remain; none in these new files.
- Browser: 23 cases at 1440×900, 390×844 and 320×844 (69 state/layout checks),
  with browser-emulated reduced motion at 320. No horizontal overflow, no DOM
  animations in the ledger, no submission/retry controls, no fixture explorer
  links, no leaked result receipt after invalid/incomplete evidence. Pending
  goods stay pending; abort explicitly says no random draw. The development
  proof-mode fixture checks the shape of two transaction links, not real receipts.
- Author visually reviewed desktop terms, phone terms/pending/success/empty/abort
  and expanded proof details. Fifteen screenshot files accompany the handoff.
  Existing idle is shown only as a staging reference, with UI beside/below it.
  An initial standalone harness did not contain the absolute studio stage; its
  parent was corrected, screenshots repeated and status hit-testing added.
  Shared stage, scene, rig and animations were not edited. A transient harness
  HMR createRoot warning preceded the final reload; final checks collected no
  page exceptions or game/owner/API requests. Existing THREE.Clock warning remains.
- Temporary review HTML/TSX stays in the external evidence package and is removed
  from the checkout before delivery. Production build contains neither new UI
  markers nor fixture markers; no production route imports the component.
- Motion validation was not repeated: this delivery changes no animation,
  character pose, canvas, animation mapping or shared event decoder. The ledger
  has no timing-driven transitions. No program/Rust/SBF/chain tests were run.

Evidence directory: `/tmp/alashi-donkey-risk-ui-evidence/` (author machine).
Production API/transactions, owner login/submit, physical phone performance,
OS reduced-motion toggle, independent new-viewer comprehension and oracle
cryptography were not tested. This delivery is ready for review as a pure UI
candidate; final adapter, production wiring and real replay acceptance await
Ivan's frozen source/IDL/JSON/availability/confirmed-event receipts.
