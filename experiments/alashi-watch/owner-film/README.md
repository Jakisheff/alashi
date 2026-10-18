# Alashi owner film

120-second silent MP4 with English captions and a Russian chapter player.
Six original Degenie share a fixed, minimal 2.5D arena. Tasks move through
assignment, parallel work, returned reports, verification and a market sale.

The designer's written prompts in `ALASHI_visual_storyboard_prompts.pdf` are the
visual reference, as Amir requested on 16 October. Code draws simple modular
platforms with local highlights. A persistent six-agent roster and event log
track the action. The original GLB atlas supplies the characters. The JPGs'
large wheel and numbered board-game path are excluded according to the PDF.
Chief, market, rules, horizon and verification labels serve the actual workflow;
the PDF explicitly treats its own action labels as temporary.

## What is measured

observed: `evidence/demo.json` contains six scripted sale decisions executed by
the canonical Rust rules on three isolated final-market fixtures. Game cash
changes from 100M to 139M, 118.9M and 113.6M respectively. Each pair starts with
identical state. These are simulated game balances, not money earned by the owner.

not_validated: LLM decisions, provider concurrency and token savings in this film.
The visible workflow is scripted and labelled throughout. Animation timing is
edited. It does not establish model latency. No token-saving percentage appears.
The API connectivity probe used 28 tokens; it was not a workflow measurement.
The first benchmark attempt failed at DNS in the sandbox. Automatic approval
review then required explicit consent for 30 requests carrying game rules and
synthetic states to z.ai. The experiment remains pending that consent.

The owner's proposed benefit is visible delegation and a record linking each
result to a checked action. Context routing is a hypothesis for reducing repeated
input. Following the owner's Moreynis material, its useful comparison is the
cost of a correct result against the owner's current way of working. Parallel
motion alone establishes neither savings nor product demand.

## Reproduce the delivered concept

Requirements: Python with Pillow, a TrueType font, FFmpeg, and the repository's
Rust toolchain. `ALASHI_FONT`, `ALASHI_FONT_BOLD` and `ALASHI_FFMPEG` override local
tools. Installed tools and credentials are not committed.

```sh
cargo build --locked -p alashi-rules --example owner_film_oracle
python3 experiments/alashi-watch/owner-film/demo.py
python3 experiments/alashi-watch/owner-film/check_film.py --concept
python3 experiments/alashi-watch/owner-film/check_motion.py
python3 experiments/alashi-watch/owner-film/film.py --frames
python3 experiments/alashi-watch/owner-film/film.py
python3 experiments/alashi-watch/owner-film/check_film.py --concept --video
```

Serve this folder using the parent `serve-preview.py` RangeHandler on loopback
for seeking. The player provides pause, chapters, fullscreen and MP4 download.
On a narrow phone, use landscape/fullscreen for the map; Russian explanations
remain readable below the video. There is no autoplay.

## Prepared token experiment

`benchmark.py` makes 30 bounded model requests after authorized z.ai access.
Three parallel specialist calls precede the Chief and verifier for each workflow.
Three fixtures are paired between full context and task-specific worker context.
Both groups use the same model, roles and quality gate. The baseline imports the
current board agent's game rules without its player-specific response schema;
it does not assert that production already uses this specialist organization.

Use `benchmark.py --out NEW_DIRECTORY` to retain requests, responses and provider
usage. It stops on errors or missing usage. `check_film.py` without `--concept`
expects complete receipts in `evidence/benchmark`; it reconciles all calls and
reexecutes decisions. A new measured film also requires updating the player and
CI mode explicitly, followed by visual review. `chosen-config.json` is a candidate,
not a validated recommendation.

Input tokens, total tokens and billed cost differ. Cache billing, prices and
retries must be included before claiming dollar savings. Even a successful
three-fixture test would not establish general performance or game win rate.

See [staging](STAGING.md) for all Lasseter principles and
[acceptance](ACCEPTANCE.md) for checks and remaining limits.
