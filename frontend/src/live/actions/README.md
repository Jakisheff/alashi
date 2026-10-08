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
Only the existing hero GLB is fetched. Props use owned Three geometries/materials;
the cloned hero and cached loader assets are not disposed or mutated globally.

## Choreography and frame checks

| Action | Key moments (seconds) | Visual QA focus |
| --- | --- | --- |
| Mule | .8 / 1.6 glance; 2.35 parcel above sack; 2.95 concealed; 3.65 carry; 4.3 turn toward recipient; 4.9–5.25 transfer; 5.55 departure | Knit cap, parcel drops into sack; support under sack; receiver lifts its hand before lowering cargo; backward scrub restores parcel. |
| Bribe | 1.6 anticipation; 2.5 offer; 3.7 receiving arm; 4.15 envelope changes hands; 4.8 withdrawal; 5.4 on official's side | Faceless official behind side desk, wax-sealed envelope stays above hands and desktop; lean approaches desk, relaxes after exchange. |
| Vote | 1.5 lift; 2.8 body turns to urn; 3.35 hold; 3.5 release; 3.75 falling; 4.1 inside; 5.3 relax | Urn remains under release point; fingers retract before paper falls through actual split lid; paper rests above interior floor. |

Also inspect 0, 6.2 and 7.2, both entries, desktop and 9:16 framing. Whole-body
yaw/lean accompanies each action; the vertical hover stays within ±.07 units.
The official/desk extend toward the right edge and the Mule receiver toward the
left: final portrait camera/cropping checks belong to the parent UI integration.

## Verification

Run from `frontend` (no full build or browser ownership required):

```sh
node --experimental-strip-types src/live/actions/validate.mjs
./node_modules/.bin/tsc --ignoreConfig --noEmit --incremental false --target es2023 --lib ES2023,DOM --module esnext --moduleResolution bundler --jsx react-jsx --skipLibCheck --allowImportingTsExtensions --types vite/client --noUnusedLocals --noUnusedParameters src/live/actions/index.ts
./node_modules/.bin/oxlint src/live/actions
```

The numerical QA loads the actual hero GLB, samples 5,196 frames at 60 Hz across
all actions, both entries and reduced/normal motion, and checks finite transforms,
unchanged limb lengths, bounded hover, exact idle recovery, history-independent
scrubbing/action switching, ballot-slot clearance and conservative hand/prop AABB
overlaps over .015 scene units. This is geometry QA, not a rendered screenshot
review; parent still needs to inspect the listed frames in the integrated page.
