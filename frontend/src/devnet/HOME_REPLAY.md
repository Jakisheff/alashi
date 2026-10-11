# Homepage confirmed replay review

Task: DIN-HOMEPAGE-CHAIN-REPLAY-20261011-01. Base: cbb6459aa060de53d6183ec58e48a5c3f9a13242.
This change replaces the bare homepage's sample feed with a replay of the existing completed public Game
`GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b`. It uses the same decoder, player projection and action mapping as /devnet.
The Game link carries the same PDA and selected faction. The selected player's complete journal is frozen while replaying.

Only App.tsx, HomeChainPreview.tsx, homeReplay.css and this review note change.
Onboarding.tsx, feed.ts, owner/pairing, DevnetPage, assets, rig and choreography remain byte-identical to the base.
App explicitly disables the optional HTTP sample producer. Explicit numeric HTTP watch links and manual pose/demo links keep their original paths.
No public free-text chain intent API exists; captions describe actual events, never private thoughts or invented dialogue.
Winner selection and the victory gesture require the existing confirmed rank-zero payout mapping.

## Motion intent and key poses

The previously recorded intent is attention → action/contact → visible consequence → idle.
Reuse the existing scenes and their key poses, with no new rig or decorative gestures:
buy preparation/payment/contact/result (about .8/1.4/3.8/5.3 seconds), sell receipt (3.8),
mule parcel handoff (3.55), bribe exchange (3.85), vote ballot (3.75), verified victory (2.3).
Other confirmed events get a factual caption with idle; no unsupported action or emotion is inferred.
One action runs at a time, followed by a 350 ms idle transition. Text events hold for 1.8 seconds.
These are shortened replay pauses, not the historical transaction timing. Replay ends in idle and shows Finished.

The [Lasseter canon](../../../docs/200_ZETTEL/20261010-lasseter-animation-canon.md) applies.
This integration reuses the following existing motion choices; it does not claim a fresh redesign of them.

| Principle | Application in this replay |
| --- | --- |
| Squash/stretch | Rigid body retains shape; existing arm/pose weight and smoke supply flexibility. No new body scaling. |
| Timing | Existing 6.4 s trades, 7.2 s scenarios and 5.2 s victory; preserved contact/result holds, explicit shortened inter-event pauses. |
| Anticipation | Existing gaze and preparation before payment, transfer or ballot; purchase preparation/payment screenshots reviewed. |
| Staging | One action and selected player; captions/controls below the scene, receipt below face/contact. Desktop and phone reviewed. |
| Follow-through/overlap | Existing arm, body and smoke recovery; idle transition prevents overlap between confirmed events. |
| Straight-ahead/pose-to-pose | Existing authored poses and contact trajectories; no new procedural replacement. |
| Slow in/out | Existing motion ramps retained; replay/seek resets rather than accumulating transforms. |
| Arcs | Existing hand/prop arcs retained; geometry checks and phone contact frames reviewed. |
| Secondary action | Existing face/smoke support the main action; replay emits no fresh confirmation pulse. |
| Exaggeration | Existing effort/celebration silhouettes retained; unsupported events get no invented reaction. |
| Appeal | Distinct buy/sell/mule/bribe/vote/victory silhouettes; face remains readable on phone. |

Pausing freezes the timeline. Resuming after an offscreen/hidden interval starts a new take at the last reported pose,
so the canvas cannot count its suspended interval as animation time. Backward seek and repeated Replay reset the take.
A stale completion callback cannot complete a newer take. The frozen complete history cannot be replaced mid-replay.
Mounting/unmounting the homepage clears any old HTTP/manual speech from the shared screen text.

## Author verification — 2026-10-11 UTC

- Public no-cookie GET: HTTP200, 29704 bytes, SHA256 `5fadc59280561f44af023ec4ed9dfd3715be25d86356eb6d3b80ed3ea3209085`.
  Finished/settled/complete, 72 confirmed events. Aibot projected journal 51 events, Botagul 52.
- Final `npm run build` and `npm run lint` pass. Build retains the existing large-chunk warning;
  lint retains four warnings in unchanged ArenaPage, Onboarding, StreamPage and DevnetPage.
- `npm run check:animations` passes: purchase/sale checks, 1532 buy frames, 827 legacy frames,
  1099 reaction frames, 5196 scenario frames. No shared motion files changed after this check.
- Existing `scripts/devnet-viewer-check.ts`, `devnet-owner-check.ts` and `owner-browser-session-check.ts`
  pass with local fetch fixtures, including identity/projection/private boundaries and owner transport.
- Playwright author review: 1440×900 desktop, 390×844 phone and 320×740 reduced-motion viewport,
  one canvas and no horizontal overflow. Phone contact frames reviewed for all six mapped actions.
  Real Botagul purchase shows 2 goods/19 alashi; Aibot sale shows 2 goods/22 alashi and bribe 5 alashi.
- Pause, repeated Replay, backward event seek, player changes, sequential final payout → idle → settled → Finished verified.
  Offscreen freeze/resume checked for trade, scenario and reaction; 2.2 s hidden interval adds no hidden time.
- Local route overrides only: HTTP503, incomplete journal, missing units/cost.
  Error/incomplete states substitute no actions. Missing fields show Goods received / Cost unavailable, no invented amounts.
- Anonymous root requests only the public chain GET, with credentials omitted. Login dialog still opens;
  pairing failure tested against an intercepted localhost POST, no server grant or game created.
  Explicit HTTP watch, manual demo and invalid agent links retain their respective paths.
- Clean final homepage navigation has no console errors; unchanged THREE.Clock deprecation warnings remain.
  During development, HMR hook edits and deliberate error fixtures produced transient errors; they are not counted as clean-run passes.

Screenshots and command logs are in the external delivery evidence manifest supplied with the branch/head handoff.
Author review is limited to the tested frames and sequences: no new-viewer comprehension study,
physical-phone FPS, OS-level reduced-motion toggle, end-to-end production owner login, new Game or transaction was run.
Reduced motion was emulated in the browser. No main/release/deploy claim.

## Publisher dependencies

Ivan owns normal PR integration, required CI, immutable frontend release and rollback.
This base includes merged PR23 source `168bea6c039aa0860fc5d15682a16719bbb795e5`, which is not yet proven active in production.
Its included animations received the scoped author frame checks above, not independent audience acceptance.
Before release, Ivan must resolve PR23 inclusion and the stale nested `devnet_viewer.index_sha256` release metadata.
The separately reported missing /deck assets remain Ivan's publication dependency; preserve restored deck assets when packaging.
No backend, Nginx, CI workflow, rules, economy or private API change is part of this branch.
