# State-machine reproductions 6 and 7

These probes run the current production React modules in headless Chrome. They do not change application source. The default command exits 0 only when the known faulty behavior is observed and the healthy controls pass. This exit status means reproduced, not fixed. `--assert-fixed` checks the desired behavior and currently exits 1.

Run from `frontend/` with Node >=22.18, the locked frontend dependencies installed, Google Chrome, and Playwright 1.62.1 available:

```sh
ALASHI_PLAYWRIGHT_MODULE=/absolute/path/to/playwright/index.mjs node scripts/state-repros/run.mjs 6
ALASHI_PLAYWRIGHT_MODULE=/absolute/path/to/playwright/index.mjs node scripts/state-repros/run.mjs 7
ALASHI_PLAYWRIGHT_MODULE=/absolute/path/to/playwright/index.mjs node scripts/state-repros/run.mjs all --assert-fixed
```

If Playwright resolves from the frontend directory, omit `ALASHI_PLAYWRIGHT_MODULE`. The recorded runs used the Codex bundled Playwright and Node runtime. No package dependency was added. The command needs permission to bind `127.0.0.1` on an ephemeral port and launch Chrome with a temporary profile. Vite ignores the application config and environment files. Browser requests to other origins are blocked, and unexpected application fetches fail the fixture. No backend, wallet, provider, or live game is contacted.

| Finding | Actual code exercised | Controlled boundary | Observed result |
| --- | --- | --- | --- |
| 6 | `StreamPage`, its `StreamHeroBoundary`, `usePublicStream`, API response decoder, production stage visibility checks | Public HTTP replies; renderer child completes normally or throws during an active action. Navigation, private owner controls and GPU scene are test adapters. | Healthy buy/sell both finish. After the child throws during buy, the real boundary shows its fallback. Sell reaches the public feed, but after 20 simulated seconds the scheduler still exposes active buy and never starts sell. |
| 7 | `useArenaFeed`, event cursor/text functions, React Query, Zustand scene store | Valid snapshots enter the real query cache; an in-memory fetch supplies later polls. The same hook remains mounted when game/mode changes. | Healthy A playback emits speech and accepted reaction. A-to-B and live-to-off each emit old A work after switching, both with two queued actions and with the first result timer pending plus one queued action. |

The browser clock controls production timers. Finding 7 checks the mode change before advancing time. Its pending-result cases capture the old accepted reaction 1.1 seconds after switching, before the next queue tick, then capture the remaining queued action. Control failures and fixture errors fail the command; they are not accepted as a reproduction.

Finding 6 establishes the scheduler's response to a renderer child error at the real React error boundary. It does not establish that a particular GPU or model fails in production. Finding 7 establishes hook behavior when mounted across a source change. Route retention and callers were reviewed separately in the state-machine audit. These are lifecycle tests, with no visual animation acceptance or production fix.

Recorded stdout, observed state and exit status are in `docs/ops/state-machines-20261107/finding-6-*.log` and `finding-7-*.log` at the repository root. These probes are separate from normal CI so their success cannot be presented as a repaired regression.
