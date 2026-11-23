# Blue ocean scout: buyer-pain canvas for the agent-infrastructure space

Date: 2026-11-17. Method: complaint mining over raw buyer speech (not vendor
marketing), then an as-is canvas redrawn from those pains, an ERRC hypothesis
for ALASHI, and the four strategic-sequence gates with their current status.
This is field reconnaissance, not a verdict: every strategic claim below is
labelled [quote] (raw buyer speech), [my analysis], or [number].

Trigger: the ALASHI Agent37 evaluation (2026-11-16/17 chat session) concluded
Agent37 competes in a red ocean on price. The professional follow-up is not to
grade vendors harder but to mine what buyers of agent infrastructure actually
hurt from, and check whether ALASHI's planned curve diverges from that field.

## Source material (raw, with provenance)

- `data/hn-thread-complaints.md` — 136 comments of the Agent37 Launch HN thread
  (item 50059096, 2026-10-12..15), flattened from the Algolia API, categorized.
  This thread is a rare artifact: 100+ practitioners arguing about what they
  actually need when they run agents.
- `data/competitor-issues.md` — top-commented issues of E2B (424 total) and
  Daytona (1257 total), pulled from the GitHub search API on 2026-11-17.
- Not collected (boundary): Reddit r/LocalLLaMA and Discord threads; the three
  non-customer interview tiers are not yet conducted (guides prepared, see
  `interview-guide.md`).

## The six buyer pains the field actually voices

Ranked by how many independent voices raised them, not by vendor marketing:

| # | Pain (buyer's words) | Stage of experience | Evidence |
|---|---|---|---|
| 1 | Credentials and blast radius, not the box: "sandbox protects the host. it does not protect your tokens"; "the agent doing something allowed but dumb with a grant that was too broad" | use | [quote] talkarimli, ekorondy, ryanascend, arbor-group, cagster, radim_h (HN ids in data file) |
| 2 | Value capture is wrong: "sell the value of the work you are enabling... not the cost of the underlying electrons"; "get closer to the actual work" | purchase | [quote] mparis x2, ekorondy "I would pay more for that broker layer than for another 2x cheaper sandbox" |
| 3 | State that outlives the sandbox: "a sandbox is disposable but the agent's context shouldn't be"; "Paused sandbox is not persisting file changes" | use/maintenance | [quote] openABandMemory; E2B issue #1080-family |
| 4 | Reproducibility for debugging: "are runs deterministic and replayable for debugging?" asked unprompted in a pricing thread | maintenance | [quote] Sky_Joy3 |
| 5 | Setup/teardown tax and silent hangs: "most of my agent work isn't compute heavy, it's the setup and teardown cycle that eats the time"; "a stuck npm install behind a proxy fails silently" | use | [quote] mohamed01121986, select1dev; E2B template-build issues |
| 6 | Sleep/lifecycle traps: outbound-only agents die when the box sleeps; "crons every 15 minutes accidentally spend a ton" | use | [quote] uzairhaq, modo_, pgcosta |

[my analysis] Pains 1-4 are not compute pains. No amount of cheaper vCPU
addresses them. The field is shouting for a trust-and-evidence layer, and the
sandbox vendors answer with a price curve. That gap is where a divergent curve
is possible.

## As-is canvas, redrawn from pains

Factors the buyers themselves name (left column), and where each offering
stands on them, in buyer perception:

| Factor buyers name | Sandboxes (Agent37/E2B/Daytona) | Cloud consoles (AWS/GCP) | Agent arenas/benchmarks | ALASHI (planned) |
|---|---|---|---|---|
| $ per minute of compute | the defining peak | high | free for user | eliminated as a factor: charge per experiment, not per minute |
| Credential blast radius | unsolved, openly discussed | IAM, heavy | n/a | bounded action menus, rake, per-run budgets |
| Deterministic replay for debugging | absent | absent | partial (fixed tasks) | core: exact replay is the product |
| Evidence of what happened | logs, billing ledger | logs | leaderboard score | dated evidence catalog with status labels |
| State across runs | disk persistence, fragile | user's problem | n/a | canonical journal + snapshots |
| Time from question to tested answer | hours-days of glue | days | weeks (new task design) | target: under an hour for a supported scenario |
| Trust in claims | low ("what constraints are you loosening?") | high, expensive | medium | explicit limits per claim |

[my analysis] The divergence ALASHI should hold is not "cheaper arena" or
"more agents": it is selling a tested decision with replayable evidence while
everyone else sells either minutes or a model ranking.

## ERRC hypothesis for ALASHI (to be validated by interviews, not by us)

| Action | Candidate move | Status |
|---|---|---|
| Eliminate | competing on $/minute and on agent count; model rankings as the output | hypothesis |
| Reduce | setup time for a comparison (target: from a team's 10-20h DIY to <1h of owner time); price of being wrong (local experiment path) | hypothesis; the 10-20h DIY figure must come from interviews, we have no external number yet |
| Raise | evidential standard: replay, dated claims, explicit limits; comparability across different people's agents via canonical rules | hypothesis; the replay half is [number]-grade already (100-agent exact replays, 2026-11-09) |
| Create | a market for tested decisions (mechanism cards); per-run spending ceilings for external agents (already in Agent37's billing, worth copying) | hypothesis |

## Strategic sequence gates

- Gate 1, buyer utility: **not passed for external buyers.** We hold internal
  numbers (a 108-game series cost ~6.7M tokens and ~2-3h of active work,
  NUMBERS.md) but not one external team's measured "today we spend N hours to
  test an instruction change". The interview guide exists to obtain exactly
  that number. No product work before this gate.
- Gate 2, strategic price: **open.** The alternative form is a team's engineer
  hours (order $500-3000 per DIY experiment at common rates [my estimate, no
  external quote yet]). A price corridor of roughly $50-200 per comparison
  would make the choice defensible; must be confirmed against what interviewees
  already pay.
- Gate 3, target cost: **passed on current numbers.** Token cost of a full
  100-agent comparison series is single-digit dollars at OpenRouter prices
  [number: 6.66M tokens, series 2026-11-15]; compute is negligible; margin
  holds even at a $20 price point.
- Gate 4, adoption: **known barriers, not yet addressed**: simulation-vs-reality
  trust ("does a game result predict my production outcome?"), provider key
  custody, privacy of private instructions. Each maps to an existing artifact
  (local experiments path, owner-submitted usage), none is validated with an
  external team.

## What would prove this right or wrong (outside us)

The three-tier interview set (10-15 conversations) yielding at least three
teams reporting a measured hourly price of DIY hypothesis testing; then a
three-offer field test with real traffic and reply counts per tier. Guides and
offers are prepared in `interview-guide.md` and `offers.md`. A tier that stays
silent is dropped, with the reason recorded.

## Boundary

This scout does not authorize product changes. Its output is: a pain map, a
divergence hypothesis, gate statuses, and field instruments. The next concrete
step belongs to the owner: run the interviews.
