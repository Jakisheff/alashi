# ALASHI prototype status

Updated 16.10.2026, 19:25, Asia/Almaty.

The current deliverable is [the English founder demo source](presenter-demo/README.md). Amir's original voice and Tomorrow School introduction are preserved. The approved whole-map RTS scene uses fast Degenie and a circular founder camera inset. The demo interface is English, ALASHI is spelled with a normal A, and A/B badges are upright.

The final video passes complete media decoding: 120 seconds, 3,600 frames, H.264/AAC. [Acceptance](presenter-demo/ACCEPTANCE.md) records motion review, framing, token evidence and limitations. The 2,634-token receipt is real usage from a failed-quality test; no savings percentage is claimed. The sharing loop remains a product concept.

UI commit 0a5e3a2 passed all five GitHub CI checks. The video commit is prepared for publication; final-head CI is pending. The first export failed due to disk space and was replaced by the verified final file.

Plan: 1 hour until 19:21, revised to 1.25 hours until 19:36 after the export failure. Факт: 1,07 часа на монтаж и локальную проверку; publication and CI tracking follow.

Earlier untracked `rts-overview/film/` work and `rules/examples/rts_film_episode.rs` are preserved and are not the current delivered film.

Owner correction: publish code and changes only. The generated founder MP4 stays local and is removed from the current branch tree; the earlier upload remains in Git history.

19:49: Implementing the strategy reuse cycle in the platform. Plan: 3 hours, until 22:49. The demo remains local. Live provider calls are not authorized for this phase; use clearly labelled offline test providers while implementing the real provider path.

20:12: six backend cycle tests pass, including independent adoption, unknown usage,
parallel reservations and storage rollback. Canonical revenue now calls rules::compute_sale.
The local model runner and platform /strategies route are in implementation.
No real provider calls. Next: signed-owner HTTP integration and browser acceptance.

20:20: signed HTTP cycle passed with three independent test wallets. Eleven local
provider requests, peak concurrency three, three canonical sales at 13,600,000 PESO.
Interrupted delivery resumed from private response journal without repeating market
calls. These are synthetic usage fixtures, not new GLM measurements. The platform
route builds; browser motion review and failure-path hardening follow.

21:09: owner clarified the actual competition: two marketers use their own tools
and agents to create an English ALASHI ad for agent owners, with GeoGuessr-style
shared rounds and hidden submissions. The sell-action integration was removed.
The receipt journal and version lineage are retained. Implementing timed duel,
simultaneous reveal, host review and token ranking; money/conversions stay unknown.
Plan remains until 22:49; no external provider calls or video publication.

21:20: the new duel checks protect sealed ads and private original instructions.
Local tools are executable by owner-approved names; public strategies cannot supply
commands. Agent task graphs now run in parallel when dependencies allow. Frontend
build, full arena checks and staged motion review are in progress. No live model calls.

21:32: both advertising duels passed the signed HTTP test, including sealed-output
privacy, recovery and adoption. Desktop shows both teams and their token receipts.
The old ambient mock failed independently with WouldBlock; its test-only reader now
sets blocking mode and a two-second read timeout. Re-running the full arena suite.

21:40: phone viewport 390px reviewed: full map visible, no horizontal overflow.
Added pause/seek for inspection and explicit TEST labels next to counters. Both
teams' parallel work and report return are visible. Final build and animation
checks are running. No real provider bills or campaign effectiveness measured.

21:44: source and local runner checks pass. Both original and improved teams use
the shared brief; sealed ads and token accounting are covered through signed HTTP.
Two old test-only races were reproduced: nonblocking mock reads and a five-second
lobby expiring before process startup. Both are corrected; full arena rerun is active.
Added persistent A/B seats so team positions cannot swap when a second owner joins.

21:52: full arena suite passed after the two fixture fixes. Rules, Node runner,
frontend build, animation suite and source-reference staging checks passed.
Offline playback froze at 1.500 seconds across two observations with Play active.
Restarted the isolated review service with persisted A/B seats. Preparing source,
PNG/JSON evidence and GitHub publication; no generated video is included.

21:58: implementation pushed as ad9a1d0 to PR #57. Only source and PNG/JSON
acceptance evidence were added. All local checks are complete; verifying the
new-head GitHub CI now. Production deployment and real provider measurements
remain outside this delivery.

22:04: all five GitHub jobs passed on ad9a1d0. Implementation/local-acceptance/first-CI
phase completed in 2.25 hours. Final UTF-8 boundary correction has six passing Node
tests and is being published separately. Waiting for that final-head CI remains.
Production deployment and real advertising measurements were not performed.

22:20: cf682f0 passed all five GitHub checks. A final privacy regression then
confirmed that private original task names were still present in public traces.
The fixed projection uses opaque labels and hides tool/provider/response metadata,
while preserving the owner's full trace. All four cycle tests and frontend build
pass. Running the final full arena suite before publishing this privacy correction.

22:35: final local acceptance complete. All 132 arena tests passed after the
private-trace correction and removal of the double-advance fixture race.
The updated frontend build passed. Amir clarified the product as an arena for
skills and agents, with advertising only the first task. Owner/team labels,
headline, home link and guides now reflect that scope; this pilot still validates
advertisement artifacts only. Updated desktop and 390px evidence, with no phone
horizontal overflow. Publishing the final correction and checking its CI.
Факт: 2.77 часа на реализацию и локальную приёмку с 19:49 до 22:35;
ожидание итогового GitHub CI учитывается отдельно в финальном отчёте.
