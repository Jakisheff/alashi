# Living Genie — isolated runtime layer

This directory supplies a Canvas child for the `/live` preview and the real stream.
The parent owns page integration, event/cue selection, controls and browser review.
No page, API, existing hero component, market/action runtime or Blender file is modified.

```tsx
import { LivingGenie, REACTION_SECONDS, REACTION_KEYFRAMES } from './reactions'
import type { ReactionPreview } from './reactions'

// Mount one hero at a time inside the existing Canvas/Scene.
<LivingGenie reaction={reaction} onTime={setSeconds} onFinished={() => setReaction(null)} />
```

`reaction?: { kind: 'thumbsUp' | 'realization' | 'facepalm'; take: number;
playing: boolean; speed: number; seek: number | null } | null`

- Missing/null reaction: normal sly eyes, blink, hover and smoke, with sparse whole-body
  glances on a deterministic 24-second schedule. A curious tilt follows the first glance.
- New `take` or `kind`: starts again at zero, or the provided `seek`. Increment `take`
  for replay (including replaying the same seek). Seek changes are edge-triggered.
- `playing: false` freezes all motion, including blink/smoke. `seek` clamps to 0–5.2s.
  `speed` affects playback only; zero stops advancement. `onTime` reports roughly every
  0.09 animation seconds and on seeks. Completion fires once when playback reaches 5.2s.
- All three reactions last `REACTION_SECONDS = 5.2`. Paused end-frame inspection does
  not fire completion. A backward seek permits playback/completion again.
- Idle phase is captured at the start of a take to avoid a snap from a glance. Within
  that take every seek is deterministic; use a fresh mount for an idle-phase-zero QA pass.
- Reduced motion reacts to OS preference changes: living idle is still, playback holds
  a representative pose until completion, and paused scrubs still expose exact frames.
- Only an explicit parent preview/agent cue triggers a reaction. Quiet agents keep
  their living idle; silence and network errors do not trigger facepalms automatically.
  Parent should allowlist the three kinds and decide queue/preemption rules.

The whole-body `body` bone parents the casing, face, arms and tail. Reactions layer torso
pitch/yaw/roll, weight shifts, gaze and tail counterbend onto sampled idle; arm IK rotates
joints without stretching or translating sockets. The actual two-segment thumb is aligned
up for thumbs-up. Realization holds both open palms outside the temples, then bows and
looks down. Facepalm brings the right palm in front of the upper face while the torso sighs
and the other hand counterbalances.

Every pose starts with rest transforms and directly sampled idle tracks (including constant
tracks), so backward scrubs cannot retain bent fingers or closed lids. Each instance owns
its skeleton and smoke material; cached GLB geometry/materials are not mutated or disposed.
The normal text anchor renders `ScreenText`; this module does not write text or store state.

## Exact frame review handoff

`REACTION_KEYFRAMES`, `REACTION_REDUCED_FRAME`, and `IDLE_KEYFRAMES` are exported for UI QA.
Use a fresh mount, pause, and seek. Capture front/three-quarter framing and mobile 9:16;
then inspect intermediate frames, backward scrubs, replay, interruption, return to idle,
two independent instances and reduced motion. Actual browser review is still pending.

| Scene | Times (seconds) | Inspect |
| --- | --- | --- |
| Thumbs-up | 0, 0.45, 1.25, 1.9, 2.65, 3.8, 5.2 | Anticipation, extended thumb, whole-body nod, smile, release |
| Realization | 0, 0.45, 1.3, 2.3, 3.1, 3.9, 5.2 | Recoil, both hands outside head, bow/down gaze, recovery |
| Facepalm | 0, 0.5, 1.45, 2.25, 3.1, 3.9, 5.2 | Palm approach, full-body sigh, hand outside screen, withdrawal |
| Living idle | 0, 4, 5.3, 6.3, 8.2, 14.8, 16.3, 19, 24 | Eye anticipation, both body turns, curious tilt, cycle seam |
| Reduced playback | thumbsUp 2.65; realization 2.3; facepalm 2.25 | Representative poses held still |

## Local checks

From `frontend`:

```sh
node --experimental-strip-types src/live/reactions/check-rig.mjs
./node_modules/.bin/tsc --noEmit -p tsconfig.app.json
./node_modules/.bin/oxlint src/live/reactions
```

The GLB check samples 471 frames (30 fps, all three reactions), checks finite transforms,
unchanged joint positions/lengths, hand bounds against casing/screen/ear mesh bounds,
backward seeks across reactions, both idle endpoints and cycle seam, actual thumb axis,
whole-body turns, reduced idle and cached scene isolation. These are conservative geometric
checks, not rendered proof of good contact, silhouette, facial expression or composition.

Changed paths: `LivingGenie.tsx`, `definitions.ts`, `rig.ts`, `index.ts`, `check-rig.mjs`,
and this `README.md`, all within `frontend/src/live/reactions/`.
