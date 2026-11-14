# Frontend performance measurement, 14 November 2026

Measurement pass after the Modern Web Guidance change (commit bf0d03c:
hidden-tab guards on seven timer sites plus two CSS modernizations).
Numbers below come from scripted Playwright sessions on the built /live
demo page, CPU throttled 4x, headless Chrome with software rendering
(swiftshader). Software rendering and throttling inflate absolute
values; the paired before/after comparison is the reliable signal.

## Hidden-tab effect of bf0d03c (paired, 60 s, CPU 4x)

| Measure | before (2b667db) | after (bf0d03c) | delta |
|---|---:|---:|---:|
| CDP ScriptDuration | 40.8 s | 23.3 s | -43% |
| CDP TaskDuration | 55.2 s | 34.4 s | -38% |
| Long animation frames | 113 (12.1 s) | 47 (6.8 s) | -58% count |
| CDP LayoutCount | 167 | 8 | -95% |
| CDP RecalcStyleCount | 1960 | 11 | -99% |

The remaining hidden-tab script time is the three.js requestAnimationFrame
loop, which this harness cannot stop: the page only reports itself
hidden, while a real browser pauses rAF natively on a backgrounded tab.
Real-world savings are therefore larger than measured here.

## Budget check (after, visible 60 s, CPU 4x, swiftshader)

| Metric | Value | Budget | Verdict |
|---|---:|---:|---|
| LCP | 2480 ms | 2500 ms | pass (barely; software rendering inflates) |
| Worst interaction (event timing, 5 clicks) | 456 ms | 200 ms | fail under 4x throttle + swiftshader |
| CLS | not captured | - | open |

The failing interaction number is a throttled-software-rendering figure,
not field INP; it flags the pause/resume control as the interaction to
watch, with real-device RUM as the honest next step.

## Chunk composition (source-map attribution, scripts/chunk-report.mjs)

The build warns about a chunk over 500 kB: `smoke-*.js` at 1070 KB,
which is the 3D stack (three.js 665 KB, @react-three/fiber 204 KB,
troika-three-text 125 KB, DRACOLoader 63 KB, app smoke.ts 12 KB). It is
already a separate lazy chunk behind a Suspense fallback, off the
critical path of first paint. Further splitting would move no bytes off
the critical path; DRACO/troika are the micro-split candidates if
real-user LCP ever breaches budget. No code-splitting change made.

## Tooling added

- `src/perf/vitals.ts`: opt-in console vitals (?perf=1) with web-vitals
  attribution (CLS/INP/LCP) and a long-animation-frame summary; wired
  into /live and /stream.
- `scripts/hidden-tab-check.mjs`: a test that can fail: it emulates
  document.hidden and asserts the demo clock and feed freeze while
  hidden and resume without loss; added to the check:animations chain.
- `scripts/perf-trace-check.mjs`: the scripted trace used above (CDP
  Performance metrics, LoAF, event timing, LCP; visible/hidden modes).
- `scripts/chunk-report.mjs`: source-map chunk attribution without
  external tools.

## Open

CLS unmeasured; interaction latency needs real-device RUM; the
hidden-tab measurement cannot observe native rAF pausing.
