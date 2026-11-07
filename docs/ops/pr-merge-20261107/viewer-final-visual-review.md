# Independent final viewer visual review

observed: reviewed the four completed recordings and captured PNGs from runtime head `321bf71d86ddf7fa683b5bd7fac55f4cfa5e4235`. I also read the fixture runner, its fetch adapter and renderer-failure wrapper. No browser or geometry suite was rerun. No product source was changed.

Evidence source: `docs/ops/pr-merge-20261107/viewer-final-visual/` in the root worktree. The recordings decode successfully: stream desktop 22.36s, stream phone 22.16s, legacy desktop 21.00s and legacy phone 21.44s. Extracted frame sequences are in `viewer-final-visual-review/`; labels are sample indices, not exact animation timestamps.

| Boundary | Observed result |
| --- | --- |
| Finding 6, stream renderer failure during action | At both widths, a normal Buy visibly pays with a coin, receives the crate and shows the purchase result. A later Sell is interrupted by the injected renderer exception. The actual fallback replaces 3D, the stale purchase receipt disappears, and the public feed continues with the subsequent Bribe record. The saved event IDs contain all three confirmed fixture records. |
| Finding 7, queued legacy actions across source switch | Source A first completes its visible act/accepted sequence. With further A work queued, switching to source B for the same game clears speech and holds idle beyond the old queue timers. No old A speech appears during that interval. A fresh B Vote then visibly plays act/accepted. Both traces return to idle; the phone recording also captures that final idle frame. |
| Existing coin/contact in the integrated scene | Full-size phone frames at video 12.0s and 12.5s show the coin face above the open palm and its transfer toward the stand. The later purchase image shows the crate supported by both hands. The face and the main gesture remain readable at width 390. No obvious hand/prop intersection appeared in the inspected payment and held-crate frames. |

The frame sequences show the changed lifecycle behavior, rather than only a DOM state label. Recorded state traces agree with the visible A/B boundary and the fallback feed. The stream receipt correctly says cost unavailable because these fixtures do not provide a cost; no amount is invented.

The observations record Chrome 156.0.8078.81, WebGL2 through ANGLE Metal Intel UHD Graphics 617, real timers, desktop width 1366 and phone viewport 390x844. There are no page errors or external requests. The two console errors are the deliberately injected exceptions, one in each stream case. They are expected evidence of the failure path.

Acceptance scope: no remaining visual blocker found for the reviewed findings 6/7 behavior. This does not claim a new choreography review, physical-phone performance, actual GPU-driver failure recovery or a visual test of every possible game/source combination. The visual queue case is specifically a same-game API-source switch. Other game-change cases remain the responsibility of the lifecycle tests.

Useful derived review images:

- [stream-390-sequence.jpg](viewer-final-visual-review/stream-390-sequence.jpg)
- [legacy-390-sequence.jpg](viewer-final-visual-review/legacy-390-sequence.jpg)
- [stream-1366-sequence.jpg](viewer-final-visual-review/stream-1366-sequence.jpg)
- [legacy-1366-sequence.jpg](viewer-final-visual-review/legacy-1366-sequence.jpg)
- [phone-buy-12.0s.png](viewer-final-visual-review/phone-buy-12.0s.png)
- [phone-buy-12.5s.png](viewer-final-visual-review/phone-buy-12.5s.png)
