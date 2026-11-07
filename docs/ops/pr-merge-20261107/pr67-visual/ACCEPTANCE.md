# PR 67 reduced-motion browser acceptance

observed: review completed on 07.11.2026, 16:40 Asia/Almaty. The reviewed checkout stayed clean at `5a80a6ff37de1eba7b238ebf91ff61fa49c327e5`. No source changes, dependency installation, PR mutation or production contact occurred in this review.

The missing browser criterion is closed for the changed pause, seek and displayed-frame behavior. All five action previews and seven reactions passed at desktop width 1366 and phone width 390. The final resume checks used observed timeline advancement with the canvas visible. They were followed by a bounded phone recording. Final rendered frames were inspected for visible poses and hand/prop contact.

## Runtime and isolation

The actual frontend was built with its existing dependencies into `site/`. Vite app configuration, environment files and proxies were excluded. A static server listened on an ephemeral loopback port. Every browser request outside that origin was blocked by the harness; no such requests occurred in completed runs. The `/live` page used its existing local demo and preview controls. Purchase values remained visibly labelled as examples.

Node 24.22.1; Chrome 156.0.8078.81; WebGL2 via ANGLE Metal Intel UHD Graphics 617. Browser media emulation was `prefers-reduced-motion: reduce` and `matchMedia` was asserted. Viewports were 1366x1000 and 390x1000. This was real WebGL rendering, not a DOM-only scene mock.

## Completed evidence

| Check | Result | Evidence |
| --- | --- | --- |
| Pause, seek zero, forward canonical/example pose, backward seek | 12 desktop plus 12 phone cases completed. Every seek changed visible rendered pixels. | `desktop/observations.json`, `desktop/run.log`, `phone/observations.json`, `phone/run.log` |
| Paused frame held over time | Desktop pairs were identical. Phone maximum channel difference was 1/255; no phone pixel differed above 8/255. | Same observations and paired `paused`/`held` PNGs |
| Resume after seek with canvas visible | 24/24 completed after measured timeline advancement from 1.0 to 1.11 through 1.79 seconds. | `resume/run.log`, `resume/observations.json`, `resume/pose-analysis.json` |
| Reduced action pose after resume | Buy 4.25, Sell 4.25, Mule 3.65, Bribe 3.7 and Vote 3.3 matched explicitly sought canonical poses with no pixel difference above 8/255 at both widths. | `resume/*-canonical.png`, `resume/*-resumed.png` |
| Reduced reaction pose after resume | Seven poses matched at both widths. Four reactions had small differences only in the on-screen speech text, whose clock follows actual time. Difference bounding boxes were inspected; body, hands and props did not change. | `resume/pose-analysis.json` and PNG pairs |
| Purchase receipt | Visible with the reduced held-crate pose before actual contact time; absent at seek 0 and seek 1; restored at seek 4.25 and after visible resume. | Buy PNGs, `phone/390-buy-context.png`, final motion observations |
| Bounded phone sequence | 30.035s of pause/seek/resume; finalized video 34.36s including initial page load. Viewport 390, video 234x600. Exit 0, no runtime or console errors. | `phone-reduced-motion-34s.webm`, `motion-visible/run.log`, `motion-visible/observations.json` |

The desktop `selectedFrame` field in the first observations means the explicit seek target. It is not a claim that every target equals the reduced pose. In particular, that first pass used Bribe 3.65 and Vote 3.75. The final resume pass used their actual reduced poses 3.7 and 3.3.

The first desktop Mule paused at actual 6.19s. Its neutral terminal pose was correct because `displayTime` selects terminal 7.2 once actual time reaches 6s. Contact-pose acceptance instead uses the later canonical/resumed comparisons: desktop actual 1.79 and phone actual 1.29, both holding the parcel at displayed 3.65. These frames were inspected directly.

## Animation scope

The PR changes frame ownership, not choreography. Reduced motion intentionally holds meaningful poses; explicit seeking exposes exact frames. The inspected phone frames retain the Buy crate on the hands, the Mule parcel on the receiving palm, the Vote ballot above the box and the distinct reaction silhouettes. The receipt describes the displayed purchase pose.

Timing and pose-to-pose behavior were checked through pause, bidirectional seek and resume. Staging, appeal and prop contact were checked in the actual rendered frames. Secondary smoke remained fixed with paused poses. Existing body shape, anticipation, follow-through, easing, arcs and exaggeration were not redesigned; their normal-motion choreography remains covered by the existing source review, animation checks and earlier normal-motion evidence. This review does not claim a new comprehension study or a complete artistic reassessment of all animation.

## Attempts excluded from final acceptance

The initial strict screenshot hash comparison failed because GPU rasterization changed a small number of pixels by at most 2/255. The completed pause checks used a 4/255 maximum tolerance and a separate positive seek control; actual completed pairs were within 1/255.

The original all-session desktop recorder stalled during finalization. Its 20MB `1366-reduced.webm` is a partial recording without duration metadata. `desktop-pause-seek-35s.webm` is a finalized extracted segment, but the bounded phone clip is the preferred motion evidence. The first phone boot in that overloaded run timed out.

`phone-motion/` and `phone-motion-final/` are failed harness attempts, not acceptance runs. Clicking phone controls scrolled the canvas out of view. `Scene.tsx` deliberately stops frame invalidation offscreen, so a fixed 350ms wait or an advancement wait while still offscreen could not prove resume. The final runner scrolls the hero into view, waits for actual advancement and compares the resumed pose to its canonical frame. The corrected sweep passed all 24 cases.

`accepted/` was deliberately interrupted while replacing a redundant full sweep with the focused resume checks. Its trailing browser-closed error is expected from that interruption. Do not count it as a complete run. The complete final resume evidence is in `resume/`.

## Limits and integration

not_validated: physical phone performance, native OS reduced-motion setting changes, and comprehension by a new viewer. Browser media emulation was verified. No live games, funds, server submissions or onchain transactions were used.

The reviewed animation, map, replay and route source blobs match refreshed PR head `1605f03f`; its frontend differences from the reviewed head are index/about content, Onboarding and strategies/Page from main. Root owns final integrated-head CI and merge readiness. This visual review does not replace mandatory project checks. Historical CRAP measurements in this PR remain disclosed technical debt, not a passing project-wide quality gate.
