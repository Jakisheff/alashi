# Coin and Dance review, 7 November 2026

Observed: the local frontend acceptance checks passed for the reviewed combination of PR 67 playback fixes, PR 24 coin and PR 25 Dance. This evidence covers Chrome at 1366×900 and 390×900 CSS pixels. Exact-head GitHub CI and the required Rust/SBF checks remain separate merge requirements coordinated by the main task.

The coin renderer follows the [owner's selected concept](../../ALASHI_COIN_20261010.md). Dance follows the existing pose and intent specification in PR 25: gather the fists, alternate their forward movement with a weight shift, change the mouth/eyes, then return to idle. It remains a local preview. Public gesture cues and game rules are unchanged.

## Source mapping

| Item | Reviewed source |
| --- | --- |
| Coin candidate, without Dance | `02c367b05357775dd26c4bb59899be6c62c7200d` |
| Combined Dance candidate | `c0e74cd8fc3eb2c3e531606dd2e24aa353af64ad` |
| Build and complete animation suite | `026dabe435ad82c179fde8abf0e0c7089c57e264` |
| Later difference from the built source | Two assertions in `frontend/scripts/brand-coin-check.ts`; application code, assets and lockfiles unchanged |
| Playback evidence subsequently included | `1244cc9f2f38dff7ddced4c6e65dc4d972f15206`, docs only |

The browser runs used the combined runtime. The coin factory and its trade/scenario callers match the coin-only candidate byte for byte; their Git blob IDs are in [manifest.json](manifest.json). The entire frontend is not claimed identical between the two PRs: Dance adds its reaction and desktop card positioning. The refreshed branches preserve the current animation checks and both journal histories. Later evidence commits do not change application code.

## Checks and browser result

[Build](logs/frontend-build.log) and the [complete animation suite](logs/animations.log) passed. The latter includes 1532 trade-contact frames, 827 legacy frames, 1256 reaction frames and 5196 scenario frames, followed by stream, playback, smoke and map checks. Scoped lint, diff checks and a redacted Gitleaks scan passed. Build retains the existing bundle-size warning.

The coin test now compares the actual field and rim surfaces on both faces. It [fails on a mutant that raises the field](logs/coin-field-recess-red.log) and [passes on the original model](logs/coin-field-recess-green.log). No model code changed for this addition. [Mutation review](MUTATION_REVIEW.md) classifies the remaining survivors. Crapper ran without coverage, so CRAP scores are unavailable; coin CC is 12 and living-rig CC is 75. Dryer at 0.8 found no duplicate candidates in the selected files. These are reported limitations, not clean coverage or mutation gates.

The harness served built files on isolated ephemeral loopback ports, localized static icon URLs and blocked all external requests. JavaScript was unchanged. No backend, RPC, game or wallet was used. Chrome rendered through Intel UHD 617 / ANGLE Metal.

| Scene | 1366 px | 390 px |
| --- | --- | --- |
| BUY, SELL and Mule | Payment pose, pause, backward seek and complete return to idle passed | Same checks passed |
| Dance | Thinking transition, key poses, pause, backward seek, restart and complete return to idle passed | Same checks passed |
| Dance reduced motion | Pause preserved the canvas exactly; explicit seek changed 46,637 pixels; resume returned to idle | Pause preserved the canvas exactly; explicit seek changed 31,646 pixels; resume returned to idle |

No page errors, console errors or external requests were recorded in the accepted scenarios. Neither viewport overflowed horizontally. The full [desktop and coin observations](browser/desktop-and-coin-observations.json) and [phone Dance observations](browser/phone-dance-observations.json) retain the source SHA and measured differences.

Two fixture failures remain recorded. The first synthetic seek to zero was sent while the control still read zero, so React emitted no change event; the harness now waits for actual advancement first. The first phone Dance precondition timed out after controls were used. The isolated repeat explicitly put the hero in view before waiting for advancement, respecting the existing offscreen frame guard, and passed. Accepted desktop and coin results were retained. No application change was made for either fixture failure. See the [first failure](browser/first-fixture-failure.log), [combined run](browser/desktop-and-coin.log) and [phone repeat](browser/phone-dance.log).

## Lasseter review

The reference establishes appearance; the following review checks the rendered movement. Screenshots and recorded frame sequences were inspected alongside the full-cycle browser assertions.

| Principle | Observed application |
| --- | --- |
| Squash and stretch | The rigid casing and limb lengths remain unchanged; pose and tail show the weight shift. Geometry checks preserve bone lengths. |
| Timing | The authored 5.2-second reaction completes and restores idle; coin exchange timing is unchanged. |
| Anticipation | The 0.45-second preparation gathers the fists before the alternating movement. |
| Staging | Face and fists remain visible at both sizes; the coin remains distinguishable above the palm. |
| Follow through / overlapping action | Opposite fists and torso shift in sequence; the tail follows the body in the recorded cycle. |
| Pose to pose | Preparation, opposite fists, changed mouth and ease-out poses are captured below. |
| Slow in / out | The recorded cycle gathers into the rhythm and settles before returning to idle. |
| Arcs | Hands move through the staged poses with unchanged limb lengths and no new hand/body overlap candidates. |
| Secondary action | Eyes, mouth and tail support the dance; no fireworks or music were added. |
| Exaggeration | The opposing fists and body shift remain distinct at 390 px without changing casing scale. |
| Appeal | Dance has a distinct fist silhouette and expression; coin shape and palette follow the selected reference. |

Personality remains a preview performance. It does not assert a real agent emotion, a win or a confirmed game event. Receipt provenance and action paths are unchanged.

## Evidence

Coin: [BUY](frames/390-coin-buy-1.35.png), [SELL](frames/390-coin-sell-3.8.png), [Mule](frames/390-coin-mule-1.9.png). Dance: [idle](frames/390-dance-0.png), [preparation](frames/390-dance-0.45.png), [first fist](frames/390-dance-0.9.png), [opposite fist](frames/390-dance-1.37.png), [mouth](frames/390-dance-2.8.png), [ease out](frames/390-dance-4.1.png), [reduced pose](frames/390-dance-reduced-motion.png). [Desktop pose](frames/1366-dance-1.37.png).

[Desktop recording, 17.72 seconds](1366-dance-excerpt.webm) and [phone recording, 28.96 seconds](390-dance-excerpt.webm) are codec-copy excerpts of the actual runs. No frames were altered. Full intermediate videos remain local and are excluded from this compact bundle.

Not validated: a new viewer's comprehension/recognition, physical-phone performance, Safari, touch hardware or changing the operating system's accessibility setting. Reduced motion was emulated through the browser media setting. No deployment occurred. Mandatory Rust/SBF and exact-head CI outcomes must be read from the main task's final verification record.
