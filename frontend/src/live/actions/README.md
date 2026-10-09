# Mule / Bribe / Vote visual previews

Local modules only. No changes to MarketTrade, LivePage, routes, CSS, shared
reaction logic, canonical Blender files, server docs or Git state.

## Integration

```tsx
import { ScenarioAction, ACTION_SECONDS, ACTION_META, actionStageAt } from './actions'
import type { ScenarioPreview } from './actions'

// Render inside the existing Scene/Canvas in place of the other hero, not beside it.
<ScenarioAction preview={preview} onTime={setTime} onFinished={finishPreview} />
```

`ScenarioPreview` is `{ action: 'mule' | 'bribe' | 'vote', take: number,
playing: boolean, speed: number, entry: 'bottom' | 'side', seek: number | null }`.
Increment `take` to replay; set `seek` to seconds to scrub (pause for screenshots).
All three last `ACTION_SECONDS = 7.2`. Props enter .35–1.1s, leave 5.75–6.5s;
the ending frame restores the starting idle pose. Timing is deterministic,
including smoke, body turns and hover; no wall-clock or random particle state.

The parent owns screen copy: `actionStageAt(action, time)` returns
`{ at, label, text }`, all in English. The component portals the existing
ScreenText into the hero's text anchor but never writes the shared speech store.
Use `ACTION_META[action].label`, `.keyframes` and `.stages` for controls/QA.
These are previews and do not report any backend transaction or economy success.

Reduced-motion playback holds a representative scene then restores idle;
paused scrubbing still exposes each frame. Preference changes are observed live.
The hero uses the existing GLB. Procedural props own their Three geometries and
materials; the Mule reference texture is loaded separately as described below.
Cached loader assets are not disposed when a preview unmounts.

## Asset provenance

[`mule-courier-v1.webp`](../../../public/images/live/mule-courier-v1.webp) is a
transparent, generated/edit-derived adaptation of Amir's
`degenie-five-actions-concept-20261008.png`. It reconstructs the masked donkey
with packbags from that reference; it is **not a literal pixel crop**.
The supplied WebP is **640 × 768 pixels, 77,834 bytes**, with alpha transparency.

`MuleReference.tsx` mounts only for Mule and lazily loads the asset through Drei
`useTexture` using `BASE_URL`. Bribe, Vote and reactions do not request it.
The billboard uses an sRGB, unlit material with alpha testing and disabled depth
writing. Its material belongs to the preview; the cached texture belongs to Drei
and is retained across replays. The final scene contains no procedural donkey.

## Game semantics and artistic interpretation

- **Mule / Donkey:** [`donkey`](../../../../rules/src/actions.rs) charges
  `DONKEY_PRICE * PESO`, with `DONKEY_PRICE = 1`, and adds **one good**. The
  animation therefore pays one peso, receives one parcel and keeps it beside
  the agent's own sack. It does not depict selling or delivering outgoing goods.
  **Shuttle is distinct:** its base reward is three grey goods, with the active
  goods subsidy applied by the rules. Mule does not mark its received good grey.
- **Bribe:** the official, desk and paperwork are an **artistic metaphor** for a
  discreet exchange. The actual `bribe` rule transfers the payment to another
  (rival) faction and increases the paying faction's influence by
  `amount / BRIBE_PRICE`, subject to rule validation. There is no official NPC
  receiving the money in the game state.
- **Vote:** the original ballot gesture, timing and public API are preserved.
  Only scene scale (.86) and horizontal offset (-.10) change to fit the whole urn.

All scenes remain visual previews; an animation is not evidence that an action
was accepted by the server.

## Choreography and frame checks

| Action | Key moments (seconds) | Visual QA focus |
| --- | --- | --- |
| Mule | .8 / 1.4 glance; 1.9 coin on palm; 2.0–2.38 payment flick; 2.8–3.3 parcel returns; 3.55 catch dip; 4.3 goods retained; 4.65–5.12 wink | Reference donkey's illustrated hoof contacts, one coin out and one box in, agent retains its sack, fitted cap and actual finger support. |
| Bribe | 1.35 / 2.0 / 2.7 look around; 3.65 offer; 3.7–4.05 transfer; 4.5–5.2 withdrawal; 5.4 paperwork covers envelope | Larger expressive official, connected arms, chest-height exchange leaving the face visible, donor withdraws after acceptance, envelope and paperwork supported by hands. |
| Vote | 1.5 lift; 2.8 body turns to urn; 3.35 hold; 3.5 release; 3.75 falling; 4.1 inside; 5.3 relax | Urn remains under release point; fingers retract before paper falls through actual split lid; paper rests above interior floor. |

Also inspect 0, 6.2 and 7.2, both entries, desktop and 9:16 framing. Whole-body
yaw/lean accompanies each action; the vertical hover stays within ±.07 units.
All three actions use bounded compositions for the current portrait camera. Props
may cross the viewport boundary during their intentional entrance/exit fades.
The donkey is a billboard with subtle body movement, not an articulated model;
the agent supplies the glances, torso lean, catch dip, tail response and wink.

## Verification

Run from `frontend` (no full build or browser ownership required):

```sh
node --experimental-strip-types src/live/actions/validate.mjs
./node_modules/.bin/tsc --ignoreConfig --noEmit --incremental false --target es2023 --lib ES2023,DOM --module esnext --moduleResolution bundler --jsx react-jsx --skipLibCheck --allowImportingTsExtensions --types vite/client --noUnusedLocals --noUnusedParameters src/live/actions/index.ts
./node_modules/.bin/oxlint src/live/actions
```

The numerical QA loads the actual hero GLB and samples 5,196 frames at 60 Hz across
all actions, both entries and reduced/normal motion. It checks finite transforms,
unchanged limb/finger/thumb lengths, bounded hover, idle recovery, deterministic
scrubbing/action switching and ballot-slot clearance. Mule/Bribe support uses
raycasts against actual hand surfaces rather than the highest point of the whole
hand's bounding box. Contact checks also cover the cutout's measured hoof UVs,
the official's elbow connection and the paperwork's supporting hand.

An AABB broad phase is followed by hand-mesh vertex tests against procedural prop
solids at a .015-unit penetration threshold. These sampled tests are not a full
triangle-intersection proof and do not replace visual review. Opaque action
frames are projected at 20 Hz through four camera aspects (.60, .80, 1, 1.20);
the final maximum normalized extent was .9792 (Vote), below the .98 limit.

Completed browser review includes captured sequences from **two full playbacks
per action**, 31 paused keyframes, 38 transition frames in .1-second increments,
reverse scrubs, both entrances, eight mobile frames and reduced-motion playback
plus paused scrubbing. Enlarged contact crops were inspected after correcting
the floating props and cap. There were no page errors or mobile horizontal
overflow; the donkey texture was requested once, only after entering Mule.
Evidence and the capture log are in `/tmp/alashi-astra-frames/final/`.

After the final composition feedback, Bribe was lowered to chest height and its
official and receiving hand enlarged by 15%. Two further full playback capture
sequences, slow transitions and mobile frames were reviewed in
`/tmp/alashi-astra-frames/bribe-chest/`. The face remains unobscured during the
exchange. Vote's framing-only correction was checked in 21 desktop, 390px mobile
and 320px narrow frames, including 4.1 seconds, in
`/tmp/alashi-astra-frames/vote-fit/`; the whole hero and urn remain visible.

Scoped TypeScript, lint and numerical checks passed. Full integration/build and
publication remain with the parent task. The previously observed mobile Vote
urn clipping is fixed. The optional ginger cat was deferred.
