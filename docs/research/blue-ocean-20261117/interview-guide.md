# Non-customer interview guide (three tiers, Mom Test rules)

Goal of each conversation: obtain the measured current price (hours and money)
of testing an agent-instruction or agent-behavior hypothesis WITHOUT ALASHI.
Not to pitch. If a conversation produces no number, it still counts: "no
measurable cost" is a gate-1 result too (it weakens the utility hypothesis).

Rules (Mom Test): talk about their life, not our idea; ask about specific past
events, not hypothetical futures; take notes verbatim; compliments and
hypothetical willingness-to-pay are recorded but carry no evidential weight.
Commitment signals that do count: they already pay money, they already spend
scheduled hours, they already built glue scripts.

## Screening (2 minutes)

"Tell me about the last time you changed how your agent(s) work — a prompt, a
tool, a model, a instruction — because you hoped it would work better."

## Core questions (all tiers)

1. What exactly did you change, and why? (verbatim)
2. How did you check whether it actually worked? Walk me through that day.
3. How long did the check take, end to end? Whose hours?
4. What did you use: same model twice? Different runs? Just eyeballed outputs?
5. What did that checking cost — tools, tokens, servers, subscriptions?
6. What did you do when the two runs disagreed? Who decided?
7. The last time you skipped testing altogether and shipped anyway — why?
8. If you wanted to prove to a colleague that variant A beats variant B, what
   evidence would they accept?

## Tier-specific probes

Tier 1 — minimizers (run agents on VPS/Proxmox/self-hosted glue; HN thread
category F is full of them):
- "What does your setup cost per month in money and in your maintenance
  hours?" "What breaks most often?"
- "When you changed your agent's instructions recently, did you test it against
  the old version, or did you just deploy?"

Tier 2 — refusers (evaluated cloud sandboxes/arenas and walked away):
- "You looked at [sandbox/arena] and didn't adopt. What was the actual deal-
  breaker: price, trust, lock-in, or that it didn't answer your question?"
- "What would that product have had to show you to change your mind?"

Tier 3 — unexplored (teams using AI but with no infrastructure person):
- "When you want to know if a prompt change helps your workflow, who does it
  and how long does it take?"
- "What do you do today when two people on the team disagree about which
  instruction is better?"

## Recording

One file per conversation into `interviews/` (not created yet): tier, date,
role, verbatim notes, and one line at the end: measured cost found / not found,
number if found. No names required. Target: 10-15 conversations, at least 3
per tier before any tier is judged.

## Gate decision after the set

- Gate 1 passes if >=3 independent teams report a measurable, recurring DIY
  testing cost (hours or money).
- Gate 1 fails if the dominant answer is "we just eyeball it and it's fine":
  then the comparison product has no buyer-utility floor, and the ERRC's
  "reduce setup time" row loses its anchor. Record and stop.
