# Three field offers (one per non-customer tier)

Purpose: after (or alongside) interviews, test which tier answers a concrete
offer with real attention — a reply, a booked call, a "send me the link" —
not a click. Each offer speaks in the tier's own language from the pain map,
never in vendor factors ($/minute, sandbox count). Metric: conversations
started per tier. A tier that stays silent is dropped, reason recorded.

Placement: where the tier already lives (HN Show HN thread, r/LocalLLaMA,
agent Discords, team Slack communities); each placement logged with date and
URL in `offers-log.md` (to be created).

## Tier 1 — minimizers (self-hosters with working setups)

> "You already run your agent on your own box. When you change its
> instructions, how do you know the new one is actually better? We run your
> agent's decisions through a fixed rule engine with the same seed twice:
> old instruction vs new, exact replay, receipts for every decision. You keep
> your setup; we answer the A/B question. First comparison free while we're
> testing this with 5 teams."

Expected objection to watch for: "my agent is my harness; your rules engine
isn't my production". Reply (honest boundary): "correct — we test decision
policies under stated conditions, not your production pipeline; the run states
its limits up front."

## Tier 2 — refusers (looked at sandboxes/arenas, walked away)

> "You didn't need another sandbox, and you were right. What we answer is
> narrower: does this instruction change make the agent better or worse?
> One frozen environment, both variants, same seed, exact replay, and a dated
> report with what it can and cannot prove. No minutes to buy, no lock-in,
> private instructions can stay off our server."

## Tier 3 — unexplored (teams without an infrastructure person)

> "Before you roll out a new AI instruction to your team, test it the way
> you'd test an ad: two variants, one brief, decided on evidence, not vibes.
> No engineer needed: bring the question and the two drafts; get back both
> run results and a plain-language report of what they show. $X for the
> first one while in pilot."

($X placeholder: set only after gate 2 has an interviewed price anchor;
do not field tier-3 offer with an invented number.)

## Honest boundaries stated in every offer

- Results are inside a stated model, not a prediction of production outcomes.
- Same shared model by default; one variable changes per run.
- Exact replay and per-decision receipts are the evidence, not testimonials.

## What counts as a result

- Tier X "responds" = >=3 real conversations from that tier's placement.
- Tier X "converts" = >=1 team completes a free first comparison.
- Everything else is noise. No landing-page vanity metrics.
