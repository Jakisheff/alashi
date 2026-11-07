# Viewer lifecycle regressions 6 and 7

These checks execute the production React lifecycles in headless Chrome. The default command asserts recovery. `--assert-fixed` remains an alias; `--expect-bug` retains the historical reproduction mode and is expected to fail after the fixes. Healthy controls must pass in every mode.

Run from `frontend/` with Node >=22.18, the locked frontend dependencies installed, Chrome, and Playwright 1.62.1:

```sh
ALASHI_PLAYWRIGHT_MODULE=/absolute/path/to/playwright/index.mjs node scripts/state-repros/run.mjs 6
ALASHI_PLAYWRIGHT_MODULE=/absolute/path/to/playwright/index.mjs node scripts/state-repros/run.mjs 7
ALASHI_PLAYWRIGHT_MODULE=/absolute/path/to/playwright/index.mjs node scripts/state-repros/run.mjs all
```

If Playwright resolves from the frontend directory, omit `ALASHI_PLAYWRIGHT_MODULE`. The recorded runs use the Codex bundled Node and Playwright. No package dependency was added. `--case=sample` selects a check by label for diagnosis; a filter matching zero checks fails.

| Finding | Boundary and acceptance |
|---|---|
| 6 | Buy is active with sell queued when the renderer child throws. The actual stream error boundary shows fallback, active work and receipt overlay clear, and a late completion cannot restore the queue. Later confirmed events remain in the public transcript without starting an animation behind the fallback. A new stream mount can animate normally. |
| 7 | A-to-B and live-to-off cancel two queued actions, or an active action's delayed result plus another queued action. Old displayed output clears only when still owned by that feed. New B events play. |
| 7 controls | Same game ID at a different API resets the cursor even with an identical cached object. Unmount cancels work; newer manual output survives cleanup. Sample re-entry waits for new snapshots and plays new events without inheriting the prior cursor. |

The clock controls production timers. The sample control waits for a harness effect marker placed after the hook's ingestion effects, so advancing the clock cannot hide React scheduling behind one large virtual-time jump. Queue and result checks retain their original positive controls.

The fixture binds an ephemeral loopback port, uses a temporary browser profile and blocks requests to other origins. Vite ignores the application config and environment files. Fetches return synthetic records; unexpected requests fail. No backend, wallet, provider or live game is contacted. Navigation/private controls and GPU children are adapters in the lifecycle suite. These checks do not establish GPU reliability or general animation quality.

For mutation checks, `ALASHI_TEST_FRONTEND_ROOT` selects the isolated source checkout. The resolver loads its production feed and StreamPage directly; transform-time hashes must match the selected files. This prevents a symlinked test runner from silently testing the original checkout instead of its mutant. The runner never edits production files.

[Current repair evidence](../../../docs/ops/state-machines-20261107/fixes-6-7/README.md) includes RED/GREEN logs and a separate real-renderer browser review. [Initial reproduction evidence](https://github.com/Jakisheff/alashi/blob/3ea380b5c8cdfdf84decc816c5f5e9cdaf6920b8/docs/ops/state-machines-20261107/README.md) retains the old failing invariants. The historical default command's success meant reproduced, not fixed.

The separate real-renderer capture uses `node scripts/state-repros/visual-run.mjs` with the same Playwright environment variable. Set `ALASHI_VISUAL_OUTPUT=/absolute/output/path` to preserve earlier evidence during an integration review. It exercises stream failure and legacy source switching at widths 1366 and 390, records local fixture frames/video, and requires visual inspection after assertions pass.
