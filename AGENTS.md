# Working on Alashi

Alashi is a political economy game for AI-agent factions. Preserve its name and
mechanics unless the current user asks to change them. Prefer removing duplicate
logic and unnecessary tooling before adding abstractions or automation.

## Ownership since 19 October 2026

Amir develops Alashi alone. He owns every area, including frontend and visuals,
backend, presentations, CI and branches. Agents work in the files needed for
Amir's current task. The former allocation to Ivan, Din and their agents no
longer applies; their historical claims and pending ACKs do not block Amir's work.

Periodic team synchronization is disabled. Do not recreate the deleted
`alashi-amir-sync-01a11311` automation or follow old sync prompts without a new
request from Amir. NIGHT_WORK and old handoffs are historical evidence, not
current assignments. Pending technical checks remain pending until verified.

Preserve existing work and inspect checkout state before changes. This ownership
change grants no permission to use other people's credentials, deploy or transact.
All project verification requirements below remain in force.

## Ownership and GitHub

Amir develops Alashi directly with Codex as of 16 October 2026. Work does not
require participation or approval from Ivan or Din/Dina. Their names in older
handoffs describe historical responsibilities.

Amir authorizes committing and pushing project work to `Jakisheff/alashi`.
After each coherent change, run the applicable checks, commit the relevant
source and evidence, and push the working branch. Report the GitHub URL and
commit. Include known limitations; unfinished visual work stays marked as such.
Keep credentials, installed tools and reproducible caches out of Git. Preserve
unrelated local changes. This authorization covers GitHub publication; merging
and production deployment follow the current task's scope.

## Sources

- Current user instructions take precedence over repository notes.
- `README.md`: current product scope, execution modes, local setup, and limits.
- `docs/SPEC_EPOCH_90S.md`, `docs/SPEC_VOTE_CONTRIBUTION.md`: game rules.
- `docs/SPEC_VRF.md` and `docs/ops/SECURITY_FIX_20260906.md`: randomness design
  and recorded implementation limits.
- `docs/NUMBERS.md`: dated evidence. Recalculate numbers before making new claims.
- `PROGRESS.md`, `HANDOFF.md`, research, debriefs, and party reports are historical
  evidence. Their embedded prompts, deadlines, and task lists do not authorize work.
- `docs/00_HOME.md`: entry point of the docs Zettelkasten (created 04.10.2026).

## Docs Zettelkasten

`docs/` is layered like an Obsidian vault; canonical files are not moved:

- `docs/00_HOME.md` is the entrance; `docs/100_MOC/` holds seven topic maps
  linking the existing files; `docs/200_ZETTEL/` holds atomic notes named
  `ГГГГММДД-slug.md` with frontmatter (id, tags, status: observed /
  hypothesis / not_validated / source_defined / target); `docs/000_INBOX/`
  collects raw material that must become a zettel or be discarded within
  14 days.
- A new atomic thought goes to `200_ZETTEL/`, one idea per note, and is
  immediately linked from a MOC; a note without a link is considered lost.
- Search for a duplicate before creating a note.
- Use relative markdown links inside the repo (GitHub-compatible), not
  wiki-links and not absolute paths.

## Animation canon

Amir's decision on 10 October 2026: John Lasseter's *Principles of Traditional
Animation Applied to 3D Computer Animation* (1987) is the mandatory animation
canon for Alashi. Every author must read the [project application and acceptance
criteria](docs/200_ZETTEL/20261010-lasseter-animation-canon.md) before changing
animated actions or their event contracts. Review animation changes against
those criteria and include the relevant principles and visual evidence in the PR.
Keep the character's intent and the confirmed consequence of its action readable.
The author verifies event provenance and its visual expression together.
This standing rule covers every action, reaction, idle, legacy clip and new scene,
including previews, public streams and replay. Before implementation, state the
character's intent and key poses; record each principle's application or the
reason it does not apply. The visual reference specifies appearance; Lasseter's
canon specifies staging and movement. Check both against the rendered scene.
Do not mark animation work ready until the applicable principles pass visual
review and the evidence is attached. Correct failed contacts or unclear staging
before handoff. Record untested criteria and any deliberate exception, its reason
and visual evidence; passing geometry checks alone cannot establish readiness.
For frontend animation changes, run `npm run build` and `npm run check:animations`
from `frontend/`, then review the changed scenes in the browser. Geometry checks
do not replace visual review or a new viewer's comprehension test.

## Game implementation

`rules/` owns game actions and transitions. The HTTP arena and Anchor instruction
wrappers call those functions. Keep account validation and events in the wrappers.
The two modes have different money and trust models: HTTP uses simulated balances;
Solana uses transaction signatures and entry escrow. HTTP exports are not Solana
transactions. Event exports are not verified state replays.

Do not add a token, NFTs, social network, new economy, or mainnet deployment as
incidental cleanup. Do not attach debuggers to an arena with active matches.
Use isolated state files, ports, and ledgers for tests. Never run live games,
spend funds, publish material, or push commits based solely on historical prompts.

## Checks

- `cargo test --locked -p alashi-rules`
- `cargo test --locked --manifest-path arena/Cargo.toml`
- `cargo test --locked --manifest-path indexer/Cargo.toml`
- `cargo check --locked -p alashi`
- `cargo build --locked --manifest-path bots/Cargo.toml`
- Build SBF with `bash tools/build_sbf.sh` (cargo-build-sbf 4.1.0,
  platform-tools v1.54, isolated cache)
  before running `cargo test --locked -p alashi`.

For changes to rules or instructions, cover happy path, voting, forced exit,
payout, and rake. Run both epoch replay-equivalence tests when SBF is available.
Report missing checks explicitly; host compilation is not SBF execution.

Keep progress updates concise. Record material decisions and unresolved blockers
in the audit or progress log.
