# Live D preview

Every animation change follows the mandatory [Lasseter canon](../../../docs/200_ZETTEL/20261010-lasseter-animation-canon.md).
State intent and key poses before implementation, review the applicable principles
in the rendered sequence, and attach phone-sized visual evidence before handoff.
Run `npm run build` and `npm run check:animations` from `frontend`.

`/live` is a separate React route for Din's responsive 9:16 stream design.
It reuses the current Degenie GLB and scene, with a green background and viewer
interaction disabled. The home page and desktop companion keep their defaults.

The five scenarios use local fixtures based on the team's
`LIVE-STREAM-IVAN-20261008-03` package, docs SHA
`b8accd20ceda935bc819e54ca14a5cbb6ebcabe6`. They perform no game mutations,
agent model calls or live-channel requests. The B0 scenario is explicitly a
repeat-probe hypothesis, separate from Classic. Playback stops after 45 seconds.
The real clock keeps running while the demonstration is paused.

The implemented backend is now available on isolated staging. The separate guarded
`/stream` integration and validation are documented in [API_INTEGRATION.md](API_INTEGRATION.md).
Keep game, harness presence and browser connection separate.
Preserve event IDs, canonical sequence, privacy and cursor/gap behavior;
server facts must determine deadlines and action acceptance. The demonstration's
timestamps and receipts are illustrative, not production server confirmations.

## Market sale preview

The English page includes a 6.4-second sale sequence: counter enters, the agent
lifts and offers a crate, receives a ALASHI ∀ coin, smiles and winks once,
then returns to its idle hover. Props are hidden outside this sequence. The coin
mark is decorative; it adds no Bitcoin balance, payment or transaction.

Use Sell / Replay Sell to play it, the timeline to inspect frames,
and Entrance to choose below or right. A successful sale by Alpha in the
conversation fixture also triggers it. Reduced motion shows a representative
pose while playing. The guarded `/stream` maps confirmed own-actor Buy/Sell events to these scenes.
Production backend/Nginx release remains a coordinated dependency.

`market/MarketTrade.tsx` owns a cloned hero and prop materials, so the temporary
pose layer does not mutate the shared GLB. Idle joint samples are restored before
each procedural pose; the wink keeps the other eye open even during an idle blink.
The original character source is preserved. Prop source/library/raw export are
in `art/experiments/`; the meshopt asset is in `public/models/experiments/`.

## Market buy preview

Buy / Replay Buy uses the same props and isolated runtime: a ALASHI ∀
coin leaves toward an off-screen seller at a counter on the right, turned sideways.
The crate waits on that counter until payment, then comes into both supporting
palms. The buyer stands beside the counter. A small lift, settling motion, smile and brief gold/mint
sparks emphasize the catch. Screen lines and stage labels are English.
The hero keeps its existing hover. Sparks are disabled with reduced motion.
The shared timeline can be scrubbed; playing again at the end restarts the take.
Buy is currently a local preview, with no production API or game mutation.

## Additional previews

Mule, Bribe and Vote controls use procedural props and the shared hero, with
7.2-second timelines. [Action frame guide](actions/README.md). Living idle and
Thumbs up / Realization / Facepalm move the whole body, with 5.2-second reaction
timelines and explicit keyframe controls. [Reaction frame guide](reactions/README.md).
Agent-selected public reaction metadata remains a backend contract dependency;
private wishes and network errors never select public gestures.
