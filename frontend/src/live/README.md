# Live D preview

`/live` is a separate React route for Din's responsive 9:16 stream design.
It reuses the current Degenie GLB and scene, with a green background and viewer
interaction disabled. The home page and desktop companion keep their defaults.

The five scenarios use local fixtures based on the team's
`LIVE-STREAM-IVAN-20261008-03` package, docs SHA
`b8accd20ceda935bc819e54ca14a5cbb6ebcabe6`. They perform no game mutations,
agent model calls or live-channel requests. The B0 scenario is explicitly a
repeat-probe hypothesis, separate from Classic. Playback stops after 45 seconds.
The real clock keeps running while the demonstration is paused.

Before real integration, Ivan must deliver the implemented event/presence APIs
and fixtures (I3). Keep game, harness presence and browser connection separate.
Preserve event IDs, canonical sequence, privacy and cursor/gap behavior;
server facts must determine deadlines and action acceptance. The demonstration's
timestamps and receipts are illustrative, not production server confirmations.

## Market sale preview

The English page includes a 6.4-second sale sequence: counter enters, the agent
lifts and offers a crate, receives a Bitcoin-marked coin, smiles and winks once,
then returns to its idle hover. Props are hidden outside this sequence. The coin
mark is decorative; it adds no Bitcoin balance, payment or transaction.

Use **Market sale / Replay sale** to play it, the timeline to inspect frames,
and **Entrance** to choose below or right. A successful sale by Alpha in the
conversation fixture also triggers it. Reduced motion shows a representative
pose while playing. Integration with actual accepted arena events remains I3.

`sale/MarketSale.tsx` owns a cloned hero and prop materials, so the temporary
pose layer does not mutate the shared GLB. Idle joint samples are restored before
each procedural pose; the wink keeps the other eye open even during an idle blink.
The original character source is preserved. Prop source/library/raw export are
in `art/experiments/`; the meshopt asset is in `public/models/experiments/`.
