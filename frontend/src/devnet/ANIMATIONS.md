# Animation handoff to Ivan

Delivery request: `DIN-DEVNET-ANIMATED-DELIVERY-20261009-03`.
The unified `/devnet` viewer reuses the animation modules already in `main`;
this delivery adds the connection guide and this inventory, not another model.
`/live` is the local animation preview. `/devnet?game=<GamePDA>` consumes the
confirmed chain journal and has same-page player selection and private owner access.

## Available scenes

| Animation | Duration | Behavior and props | `/devnet` trigger |
| --- | --- | --- | --- |
| Living idle | 24 s cycle | Hover, blink, smoke, occasional left/right whole-body glances and curious tilt | Between actions; no message required |
| Buy | 6.4 s | Counter on the right; pay coin, receive and hold crate, react; props enter and leave only for this action | `goods_bought` for the selected faction |
| Sell | 6.4 s | Lift and extend crate, receive coin, smile and wink | `sold` or `sold_credit_ev` for the selected faction |
| Mule | 7.2 s | Look around, pay courier donkey, receive parcel, retain goods and wink | `donkey_bought` or `shuttled_ev` for the selected faction |
| Bribe | 7.2 s | Discreet glances, chest-level envelope exchange with a stylized official and paperwork | `bribe_given` from the selected faction |
| Vote | 7.2 s | Turn toward urn, raise ballot, release through slot and recover | `vote_cast` for the selected faction |
| Victory | 5.2 s | Whole-body celebration, raised fists and three finite fireworks bursts behind the character | Confirmed `payout` with rank **0** and the selected faction wallet |
| Thumbs up | 5.2 s | Raised thumb, neutral wrist and whole-body nod | Explicit preview or separately agreed public agent cue |
| Realization | 5.2 s | Hands outside temples, bow and downward gaze, then recover | Explicit preview or separately agreed public agent cue |
| Facepalm | 5.2 s | Palm approaches the upper face; body sighs and bows | Explicit preview or separately agreed public agent cue |
| Thinking | 5.2 s | Fist under chin, brow/gaze and asymmetrical body tilt | Explicit preview or separately agreed public agent cue |
| Shrug | 5.2 s | Forearms raised about 46 degrees, horizontal upward palms, sideways tilt | Explicit preview or separately agreed public agent cue |
| Look around | 5.2 s | Eyes lead left/right torso turns and a curious lean | Explicit preview or separately agreed public agent cue |

Legacy GLB clips also remain available: `idle` (4 s loop), `act` (1.1 s),
`accepted` (1.2 s), `rejected` (1.6 s), and `fuckOff` (5.8 s, existing local
gesture/easter egg). These are not automatically selected by the chain viewer.
Thinking's editable thought bubble in `/live` is clearly preview copy. The chain
viewer does not invent thoughts or display private wishes as public speech.

## Semantics and sequencing

- A Bitcoin symbol is decorative coin artwork; game balances and actual chain
  amounts remain authoritative. It does not imply a Bitcoin transaction.
- Donkey purchase pays for one good. Shuttle is a distinct rules action that
  yields grey goods; its current visual reuses Mule choreography. The public
  receipt, not the parcel count drawn in the scene, determines quantities.
- The Bribe official is an artistic metaphor. The actual rules transfer payment
  to a rival faction and increase the payer's influence; there is no official NPC.
- Only selected-player, successful confirmed program events animate on `/devnet`.
  Initial/partial backfill is not a new live action. Incomplete or lost-tail
  history is conservatively rebaselined; newly confirmed tail IDs queue in order.
- Every action finishes before the next starts and returns to living idle.
  Replay increments the take even for the same action. Pause/seek and frozen
  replay history are separate from current confirmed balances. A 10 s watchdog
  recovers the queue if WebGL stops delivering completion callbacks.
- A complete settled game with exactly one rank-zero payout wallet matching a
  faction starts an explicitly labelled winner replay once. Other cases require
  player selection. Settled games never expose the private composer.
- `received`, `consumed`, `unconfirmed` and `confirmed` are distinct private
  statuses. Only a verified signature/slot/event receipt is labelled confirmed.
  Private admission alone never animates an action or proves influence.
- No arbitrary emotional reaction follows silence, a network error or a generic
  finish. Other reactions need an explicit reviewed public cue contract.

## Assets and inspection

Shared hero: `public/models/desk-genie.glb`, **564,672 bytes**. Buy/Sell props:
`public/models/experiments/market-sale-props.glb`, **214,056 bytes**, loaded on
trade. Mule cutout: `public/images/live/mule-courier-v1.webp`, **77,834 bytes**,
loaded for Mule only. It is a generated adaptation of Amir's donkey reference,
not a literal crop or articulated donkey. Other props/fireworks are procedural.
Runtime reactions do not duplicate the GLB per animation. Normal browser caching
can reuse unchanged assets; transfer size depends on server caching/compression.

Modules and frame review instructions:

- [MarketTrade](../live/market/MarketTrade.tsx): Buy/Sell playback and props.
- [Mule/Bribe/Vote README](../live/actions/README.md): contact checks and stage times.
- [Reaction README](../live/reactions/README.md): all reaction/idle keyframes,
  reduced motion, reverse seeking, fireworks and conservative rig checks.
- [Playback mapping](playback.ts): exact public event-to-animation allowlist.

Existing frame/rig results are documented in those modules; they are regression
evidence, not a physical collision proof. This delivery changes no rig or Blender
asset. Its guide/entry checks cover 320/390 px, landing navigation, same-page
active owner entry and finished-game gating, using synthetic browser fixtures.

## Integration acceptance still required

Ivan owns API/runner/release. Sol/Terra/Luna must verify a new bound live chain
match: private typed intent → different applicable runner decision → successful
matching chain receipt → public confirmed event → selected-player animation.
The historical GqHZ game proves replay only. The frontend's synthetic session
tests do not prove real wallet ownership, cookie persistence or message influence.
Preserve private text/runner secrets outside public feeds, URLs and analytics.
