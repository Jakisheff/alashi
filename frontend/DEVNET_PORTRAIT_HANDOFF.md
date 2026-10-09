# Devnet portrait / admin log — release handoff

Run source: manual frontend delivery.
Release branch: codex/devnet-studio-release; base main 28bfb06db53978c8f358e9b3cfdbf7661d9904fe.
Direct Din instruction on 2026-10-10 authorizes publication to main and frontend deployment, superseding the previous local-only HOLD. Standing direct human exception from 2026-10-08 permits Alashi commits without an issue key; recorded in the session handoff. Ivan owns immutable frontend publication/rollback; backend/admin identity/edge remain his.
Original Projects/alashi checkout, design preview and Blender work preserved.

## Accepted UI decisions
- Reuse /live 9:16 stage for /devnet; right tabs: This player, All players, Game, Setup.
- Initial focus remains Setup per explicit instruction; Watch activates This player.
- Selected player's shortened public address plus copy control; full address copied.
- Translate the scene and its decorative platform down exactly 50 CSS px; no canvas resize or aspect-ratio change.
- Feed anchored inside portrait: sticky bottom:0 in an absolute full-frame flex anchor; expanded panel grows upward, never moves model.
- Chronological chat order: oldest at top, latest at bottom. Scroll upward reveals earlier confirmed actions in batches of20. Keep reading position; return-to-latest follows bottom.
- Collapsed feed uses22% of portrait height, reads available vertical space with ResizeObserver, shows as many last records as fit. Verified4 records at320px,5 at390px,8 at1440px/1100px tall. Latest row opacity1; upper row opacity.25.
- Closed feed denser, open feed more transparent, top fades gradually; no top border or visible scrollbar. Outside click/Escape dismiss, no Close control.
- Underlined me filters selected player; adjacent i opens an explanatory tooltip.
- Actor labels resolved from each canonical event, preserving other players' names. Verified public negotiations replace matching event rows by receipt event_id; runner replies stay separately labelled inside their offer thread.

## Checks
- Final build/typecheck passed after thin progress/law/statement refinement; inherited >500k smoke bundle warning persists.
- oxlint:7 existing warnings, no errors; diff --check passed.
- devnet-viewer-check.ts passed projection, identity, safe money, private-field exclusion, retry, counter-contract and stable receipt-ID checks.
- Browser actual public devnet snapshot:72 game actions,51 selected-player actions. Canonical order game initialized -> Game settled on-chain; no duplicate generic event IDs. Earlier load20->40 retains reading offset1020px; opened feed starts at latest bottom.
- Browser1440/390/320:9:16 preserved, no page horizontal overflow, collapsed entries fit, exact50px scene offset, sticky bottom0; expanding only moves panel top upward.
- Public address copy equals full address, me filter and tooltip work, outside click closes.
- Admin fixture UI checks:403 session performs no log requests; no implicit game1 fetch; authorised fixture log renders; log403 closes stale content; unknown route displays branded404. These are fixtures, not proof of server admin enforcement.

## Admin/server dependency — sent to Ivan
/log uses explicit numeric game ID and a light shared site design. Client fails closed on absent admin session, HTML fallback, errors or revoked access. Proposed endpoints remain unconfirmed: GET/admin/session -> {ok:true,role:'admin'} or401/403; GET/admin/games/:id/log must enforce admin authorization on every request. No public state fallback, no query/local-storage admin grant. Existing public spectator API is unchanged. Ivan owns real server authentication and protected log contract.
Root404/error/offline presentation is implemented. Ivan verified the active edge already returns true HTTP404 for unknown URLs with an exact SPA allowlist; no edge change requested. No roles/management backend added.
Coordination IDs: DIN-DEVNET-PORTRAIT-20261010-01, DIN-ADMIN-LOG-20261010-01; docs handoff commit245600051214ec7d25cea2f47cd315bc657f4cc7 already in central docs main.

Artifacts: /Users/dinmukhamed/.codex/visualizations/2026/10/07/01a117f6-0b8a-7690-b0a2-7d8ed26f5ace/devnet-portrait. log-design-fixture screenshot is simulated admin data only.

## Final phase/law/statement follow-up
- Phase status and 2px line reuse /live styling. Live timer uses snapshot phase_ends_at and verified game_initialized.phase_duration (actual historical game15s), no hardcoded30s assumption. If initialization/duration unavailable, do not invent a phase percentage. Expired deadline says awaiting next phase; unavailable snapshot does not keep a falsely live countdown. Replay seek is accessible native range over the same thin line; confirmed-action position is separately labelled.
- Real snapshot browser verifies law IDs rendered as named cards (e.g. richest subsidy, Tax10%, Status quo, Tax20%, Embargo, Boom), public weighted Yes/No outcome, goods quantity and exact sale/buy/bribe amounts. No current balances projected backward.
- Countdown UI isolated fixture12->10 seconds over2s, scaleX .8->.666667 using15s confirmed duration; no live game created, no page error.
- Existing public stream accepts explicit thinking gesture cue, renders separately labelled Shared thought text and existing thinking animation/ThoughtBubble, no generated internal monologue. Canonical event IDs, first-load/backfill/hidden suppression retained. Devnet chain_conversations.v1 has no published speech/thought contract, so /devnet does not fabricate those records. Ivan dependency DIN-DEVNET-PUBLIC-THOUGHT-20261010-01 needs exact game/player-bound PUBLIC statement contract/cue/replay anchor. Existing stream support is prepared frontend only; thinking cue server availability not proven.
- live-api-check.ts, live-gesture-check.ts including private thinking exclusion, devnet-viewer-check.ts including malformed law/timer/money rejection pass.
- Ivan exact admin reply22:14/true40422:15: no admin API exists; admin bootstrap absent, Ivan owns server identity/auth. Keep log unavailable/closed. Active edge already uses exact SPA allowlist and true404 fallback; edge dependency closed on publisher evidence, no new edge changes requested.

Coordination reply ACK: Ivan SHA2567fe6736abb4c5d156893f2767039cadefc1917892b24a8f38a39e9d936855345 at docs907463313749f2349a72aceb22ab960f6edc728e. Own manual receipt+public thought request committed docs4d2b99a0491fdf9735d9476ac91312ee1978669f and verified central main; no source commit. Replay phase/round derived from canonical phase transitions before the replay cursor, never from latest state. Author checks completed at 2026-10-09T22:24:05.103222+00:00; release-base checks are recorded in the server delivery receipt.
