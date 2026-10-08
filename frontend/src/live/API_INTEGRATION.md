# Live D frontend integration

Backend contract: PR #2 `ivan/live-channel-20261008`, exact
`e114c61568da2d397b4c053bdeb2a4712bd50ed6`. Server release contract docs `baa13b3`, staging delivery `0f23b8d`.

`/live` remains a local prepared-event/animation demo. `/stream` is a separately guarded real integration. Enable it only after backend and Nginx release gates:

- `VITE_LIVE_API_ENABLED=true` at frontend build time.
- Optional `VITE_LIVE_API_PATH`: a same-origin relative prefix, never a third-party origin. Production normally uses an empty prefix.
- Optional `VITE_LIVE_STAGE_LABEL`: clearly identifies a staging fixture.

Owner access uses the registered injected Solana wallet's off-chain `signMessage`, with the exact origin/agent/wallet/nonce/expiry envelope checked before signing. Session bearer, draft, retry ID and private journal exist only in the private panel's memory. Bearer is sent only in owner endpoint headers. Account change, disconnect, session expiry, agent change, reload and page exit clear private UI/session state. No harness/recovery/game tokens enter the browser. No analytics are added.

Public events are explicitly projected, cursor-validated and deduplicated. Initial backfill does not replay historical speech or actions. Reconnect retains history; expired/ahead cursors expose a gap and fetch retained server history. Browser connectivity and agent presence are independent. Server game context/deadline drives the phase display. Silence is valid.

Owner journal uses latest-status sequence merging for `received/consumed/replied/deferred/declined/expired`. Quota comes exclusively from `remaining_by_game` and admission receipts. Ambiguous submission failure retains the exact UUID/text/game for an idempotent retry. Submission pauses old journal polling to avoid stale quota overwrite. Private replies/statuses never enter the public scene store.

Confirmed public own-actor actions map Buy/Sell/Donkey(Mule)/Bribe/Vote to shared-model scene previews. Living idle looks left/right with the whole body. Gesture previews are explicit local controls. Backend e114 does not yet expose an agent-selected gesture field: dependency `DIN-LIVE-GESTURE-CONTRACT-20261008-01` requests a voluntary allowlisted public cue. Private wish processing must not supply public reaction metadata.

## Local staging

An already-authorized SSH tunnel forwards local 18096 to isolated backend loopback8096. Use trusted HTTPS origin **https://localhost:5173** and strict staging label. Set:

```
ALASHI_LIVE_API_TARGET=http://127.0.0.1:18096
VITE_LIVE_API_ENABLED=true
VITE_LIVE_API_PATH=/live-api
VITE_LIVE_STAGE_LABEL=STAGING · e114c615 · synthetic
```

The optional Vite `/live-api` proxy accepts only public profile/events and private owner challenge/session/revoke/wishes. It denies arbitrary arena writes. There is no production fallback. Local TLS override and keypair files are untracked and must not be published. The synthetic key stays in a Node test signer, never in a real wallet/browser seed.

## Validation receipt (2026-10-08)

Actual staging browser checks: HTTPS Vite → SSH → exact e114 backend, real Ed25519 owner signature/session; synthetic UI game2 server quota3→2→1→0; first accepted response deliberately lost, same-ID retry no extra admission; three Received journal entries; fourth submission disabled; reload/new session retains0; disconnect clears private UI; public/URL/storage/console isolation; mobile390×844 no horizontal overflow. **Game2 now has zero remaining wishes. Do not reset its ledger or repeat an initial3 test.** No real game, payment, chain transaction or model was run. Real agent processing/speech and later status transitions were not observed on the inert fixture.

Mocked UI checks cover wallet rejection, all six status projections, latest-status replacement without duplicate entries, reconnect/dedup/cursor-gap handling, session-expiry cleanup, private isolation. Every `/live-api` request is intercepted; these checks do not write to backend:

```
node --experimental-strip-types --no-warnings scripts/live-api-check.ts
ALASHI_PLAYWRIGHT_MODULE=/absolute/path/to/playwright/index.mjs node scripts/live-ui-check.mjs
```

The browser test uses `ALASHI_LIVE_UI_URL` (default https://localhost:5173) for an enabled local build. Playwright is a local QA runtime, not a new product dependency.

Module numerical checks and rendered desktop/mobile scrub previews passed; artistic refinement is a separate follow-up after API release. Staging acceptance does not prove production availability. Ivan owns backend merge/release, exact Origin, Nginx owner POST/read routes, rate limits and rollback. Frontend stays guarded until that release is verified.

## Download budget

The shared hero GLB is 564672B; shared Buy/Sell prop GLB214056B. Mule/Bribe/Vote props are procedural geometry, not separate downloaded models. Current shared scene/reaction runtime is about30KB minified /10KB gzip, separate from UI and common Three.js dependencies. More choreography adds motion code; unique high-resolution geometry/textures can dominate asset size. Twenty scenes do not imply twenty copies of the hero.

Production HEAD inspection on2026-10-08: the current `/releases/<release-id>/models/desk-genie.glb` returns `public, max-age=31536000, immutable`; the root model path returns no-store. Keep the build BASE pointing to its immutable release prefix, verify the exact served model bytes, and preserve HTML revalidation/private API no-store. A cached shared hero can be reused on later visits and across all scenes until its version changes. Never infer production model freshness solely from a query parameter or a local Vite version file.
