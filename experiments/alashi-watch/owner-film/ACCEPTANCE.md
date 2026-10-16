# Owner film acceptance

Status: visual concept with canonical local rule execution. LLM performance and
token savings are not validated. The video carries this distinction throughout.

## Reference fidelity

The designer PDF's written prompts take priority over the JPG variations.
The arena uses minimal 2.5D geometry, six locations, six original Degenie and a
persistent roster. Matching agent colors connect map selection to the list.
Task paths are free curves. A brief binary start cue disappears after 1.2 seconds;
it has no effect on game outcomes. There is no blocking wheel or numbered path.

The original Chief concept is retained as two command platforms. The third worker
in each team takes the later verifier task, keeping six visible Degenie. Location
names describe this workflow, rather than inventing mechanics from temporary
labels in the designer's PDF. English captions support a presentation; Russian
chapter explanations accompany the video. These are deliberate adaptations.

## Checks completed before playback

- Canonical rules: six scripted sales reexecuted in Rust with identical receipts.
 The three pairs finish at 139M, 118.9M and 113.6M game cash respectively.
- Temporal checks: all 3600 timestamps tested for map bounds and dependency order.
 Six workers overlap. Verifiers start after reports; execution follows verification.
- Rewind: frame 25 is pixel-identical after seeking to 119, 3 and back to 25.
- Source checks: `cargo test --locked -p alashi-rules`, frontend build and
 `npm run check:animations` passed. The latter includes 1532 purchase, 827 legacy,
 1099 reaction and 5196 scenario frames. These validate the source rig, not the
 audience's understanding of the film.
- Existing RTS foundation: 18 runtime checks, export checks and visual checks
 passed. This film is a separately scripted presentation, not a recording of
 that live host.

## Animation review

The per-principle contract is in STAGING.md. The original atlas preserves rigid
body dimensions and smoke motion. A directional cue precedes departure; travel
follows an eased curve. Work, returning a report and executing a sale have distinct
stages. A completed gesture settles back to idle. Route pulses remain subordinate
to the robot, and the camera preserves the whole arena.

Data badges represent reports. The scene does not claim that a hand holds a
physical tablet or that a box changes hands. A sale is shown as an order to the
market followed by its rules receipt. Chief labels were moved above the platforms
after inspection found overlap with waiting robots.

Portrait phone inspection at 390 pixels shows the whole map and distinct robot
silhouettes; embedded small labels and faces are too small for detailed reading.
The player supplies readable Russian explanations outside the video. Landscape
or fullscreen is required for inspecting the map. Physical-phone playback and a
new-viewer comprehension test remain untested. No autoplay is used; pausing keeps
the information visible. The MP4 has no separate reduced-motion track.

## Measurement boundary

No model-generated choices appear in the delivered concept. No token percentage
or dollar saving is claimed. The prepared paired experiment needs authorized z.ai
requests and complete provider usage receipts before a measured version can ship.
The failed sandbox transport attempt is retained under `evidence/transport-preflight`.
It contains no successful model responses and is not benchmark evidence.

## Delivered video and browser

observed: FFmpeg 7.1 decoded the complete file with `-xerror`: 3600 frames,
120.000 seconds, H.264/yuv420p, 1920x1080 at 30 fps. The MP4 is 8,170,430 bytes.
Its hash is recorded in `evidence/video.json` and `evidence/decode-checks.json`.

The browser reached time 120 with ended=true, paused=true, readyState=4 and no
media error. Chapter seeking to 47 seconds displayed the sale; reverse seeking
to 23 seconds removed that future result. Both frames remained stable on pause.
Key movement phases were inspected during playback. The application viewport
was measured at 390x844 CSS pixels with scrollWidth=390. Chapter buttons measure
44 CSS pixels high. The temporary viewport override was reset.

Immediate pause during a pending play request initially produced AbortError in
the console. The player now catches that expected cancellation. Repeating the
same action after reload added no errors. Text cleaning initially altered two
CSS selector commas; they were restored. Text-only extraction is used for the
final prose check, leaving executable code intact.

GitHub CI runs the film receipt and artifact checks. See the PR checks for the
latest commit; the decoded MP4 is identical to the copy delivered in Downloads.
