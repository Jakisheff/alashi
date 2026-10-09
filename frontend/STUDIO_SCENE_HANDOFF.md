# ALASHI studio scene — release handoff

Task DIN-STUDIO-SCENE-20261010-01. Direct Din instruction on 2026-10-10 authorizes source publication and frontend deployment, superseding the original local-only task restriction.

Delivery branch `codex/devnet-studio-release` applies the reviewed design delta to main `28bfb06db53978c8f358e9b3cfdbf7661d9904fe`. The earlier published brand branch remains frozen. Original checkout, preview and Blender work are preserved. Exact delivery source SHA and release checks are recorded in the server delivery receipt.

## Implementation

- Existing single Canvas, lighting, model materials, morphs, clips and idle retained. Transparent background and camera distance ×1.14 / target y=-.7 are explicit opt-ins only for the live/stream/devnet cards. No new dependency or continuous animation loop.
- Core kit WebP backdrop/platform and SVG shadow/halo/reflection copied unchanged (122,593 bytes total). Mark uses the existing shared official contour. Alpha and intrinsic 3:1 platform ratio preserved. Coordinates stay relative to the existing scene rectangle; model, animations and game clock semantics preserved. Subsequent portrait/feed refinements are described in DEVNET_PORTRAIT_HANDOFF.md.
- Halo lowered from .17 to .12; teal reflection from .30 to .16 so a stationary highlight stays quiet when the hero moves. Watermark .03. Original yellow eyes, ivory shell and teal tail are untouched.
- Existing true 3D coin retained: no flat sprite spun through 360°, no additional sprite request. At the existing receiving-palm point (sale time3.65), a new confirmed receipt produces a 160ms appearance, hold through450ms, gentle near-hand rise/fade through1050ms, plus one coalesced400ms platform pulse. No invented destination wallet, payment amount or balance.
- `/devnet`: fresh, verified confirmed-chain sale `current.id`; historical replay has no fresh-income receipt effect. `/stream`: final MAIN public accepted sale `event.id` (HTTP simulated economy, not Solana). `/live`: explicitly labeled demo ID only; manual animation controls are visual previews, not new receipts.
- Canonical receipt dedup; one active visual action and at most two pending, keeping the newest pending cues when overloaded. The public receipt history is retained independently. Hidden/offscreen queues dropped; the first chain snapshot after a visibility gap is treated as history. Resize/media-change/visibility/unmount cancel the new pulse/flight. Reduced motion leaves the existing accessible confirmation, without the new flight/pulse. Late frames cannot create a reward after its presentation window.
- Root webmanifest changed to same-origin `/site.webmanifest`, removing local-preview CORS errors while retaining root release serving.

## Inputs and provenance

`alashi-scene-kit.zip` is truncated: no end-of-central-directory, final brand-reference member incomplete. Complete local-header members were deflated and validated against their original CRC32/uncompressed sizes before use. All five used assets, manifest, approved-concept and overview are complete. Missing trailing brand reference / scene.css did not require generated substitutes: the existing shared official mark and supplied normalized coordinates were used. Source archive and source art unchanged. Integrated asset dimensions/alpha/SHA256 are in `public/scene/alashi/manifest.json`; only actually shipped files are listed.

## Checks

- `npm run build`: PASS. `npm run lint`: PASS, seven pre-existing warnings; existing large smoke chunk advisory remains.
- Devnet viewer identity/projection/action mapping/cursor/money/retry checks, Live API public/private projection and gesture checks: PASS.
- Scenario validator: 5,196 sampled frames, existing and opt-in studio camera; studio tested .60/.80/.9375/1.20/2.10 aspects, worst conservative extent .949041 (<.98). Palm/prop contact, no socket drift, deterministic scrubs and idle recovery: PASS.
- Living rig validator: 1,099 GLB frames plus bounds/fireworks under default and studio cameras: PASS.
- Browser `scripts/studio-ui-check.mjs`: PASS using only intercepted local GET fixtures. Fresh chain sale, repeated snapshot, bounded burst, unchanged confirmed cash, idle recovery, reduced motion, resize cancellation, hidden/offscreen return, hidden-gap snapshot suppression, SPA unmount, one Canvas, no page errors or API writes. Public HTTP burst retains all four receipts while showing one active + two pending cues, then idle.
- Real completed GqHZ game API and public conversation GET forwarded read-only into local page. Live/devnet each checked at320/390/768/1440 CSS px: one Canvas, no horizontal overflow or page errors. Five actions and seven reaction peaks viewed; original props and tail remain visible. Local preview console now has zero errors; the existing THREE.Clock deprecation warning remains.
- Short demo video: 11.84s /128 frames /493,037 bytes, local Chrome screencast + already installed ffmpeg. It is a demo, not acceptance of a new live game.
- Legacy `scripts/live-ui-check.mjs` could not complete: stale “Public agent ID”/“Verify wallet” UI assumptions do not match current label/owner flow. Script left unchanged; no auth workflow changes made for this scene. New scene integration/browser checks above pass independently.

Run new browser checks with the existing Playwright package available in your environment and an isolated enabled Vite preview:

```sh
VITE_LIVE_API_ENABLED=true VITE_LIVE_API_PATH=/live-api npm run dev -- --host 127.0.0.1 --port 5195 --strictPort
ALASHI_STUDIO_UI_URL=http://127.0.0.1:5195 ALASHI_PLAYWRIGHT_MODULE=/absolute/path/to/playwright/index.mjs node scripts/studio-ui-check.mjs
```

## Not claimed

No new live on-chain sale/message→decision→transaction acceptance, owner signing flow acceptance, mobile hardware GPU profiling or production release. New sale behavior is checked with contract-shaped browser fixtures; real historical GET/replay is separately checked. Backend/Nginx/Game/transactions remain outside this update. Frontend deployment is now authorized and delegated to Ivan; production acceptance requires his served-byte receipt. Original checkout, uncommitted Blender and other agents' changes preserved.

## Changed files

- `frontend/STUDIO_SCENE_HANDOFF.md`
- `frontend/index.html`
- `frontend/public/scene/alashi/backdrop.webp`
- `frontend/public/scene/alashi/ground-shadow.svg`
- `frontend/public/scene/alashi/halo-lavender.svg`
- `frontend/public/scene/alashi/manifest.json`
- `frontend/public/scene/alashi/platform.webp`
- `frontend/public/scene/alashi/tail-reflection.svg`
- `frontend/scripts/studio-ui-check.mjs`
- `frontend/src/devnet/DevnetPage.tsx`
- `frontend/src/genie/Scene.tsx`
- `frontend/src/live/LivePage.tsx`
- `frontend/src/live/StreamPage.tsx`
- `frontend/src/live/actions/validate.mjs`
- `frontend/src/live/api/usePublicStream.ts`
- `frontend/src/live/market/MarketTrade.tsx`
- `frontend/src/live/reactions/check-rig.mjs`
- `frontend/src/studio/StudioStage.tsx`
- `frontend/src/studio/confirmation.ts`
- `frontend/src/studio/studio.css`

Screenshots/video and exact working-tree file hashes: `/Users/dinmukhamed/.codex/visualizations/2026/10/07/01a117f6-0b8a-7690-b0a2-7d8ed26f5ace/studio-scene`.
