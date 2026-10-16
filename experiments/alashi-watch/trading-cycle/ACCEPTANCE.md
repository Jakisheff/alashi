# Two-owner trading verification

Amir asked on 17 October 2026 whether two people can connect their agents to the
trading game and see the behavior from the demo. The initial audit found two
disconnected paths: `/arena` had a public trading log without the whole map,
while `/strategies` showed the separate advertisement pilot. The original film
used scripted tasks. Its appearance did not establish a live trading connection.

## Observed game evidence

The existing devnet Game
`D2mkZ4e5zssteJs7Eih8ehtJkFap8SvFp7tY4yorpsX5` returned a complete confirmed
journal with 41 events and two factions under different wallets. Both produced
goods. Dialogue A sold two goods for 22 alashi; Dialogue B sold one for 9 alashi.
The Game finished six rounds and settled. This is a recorded Game, not a new
party started during this verification.

Both sale signatures were separately fetched with Solana devnet `getTransaction`.
Both transactions succeeded, their slots matched the public journal, and their
logs invoked the ALASHI program. No transaction was signed or submitted. The
cached public snapshot is [devnet-snapshot.json](devnet-snapshot.json); the
independent read results are [RPC_RESULT.json](RPC_RESULT.json).

## Local two-agent test

Two synthetic registered wallets each signed an owner challenge. Two separate
Rust agent processes joined one isolated HTTP game using different recovery
credentials and independent loopback provider endpoints. Both reached their
providers before either answered. The first model response sold three goods for
31 game cash; the second sold two for 15 on the changed shared price curve.
The test checks that the responses executed as model decisions, without fallback
sales. These model responses and their usage are fixtures, not external LLM work.

Recovery returned a valid client credential. Repeating the same operation returned
its saved response; an invalid extra sale produced no accepted event or second
journey. Event identities and historical balance changes survived server restart.
See [http-events.json](http-events.json) and
[http-public-state.json](http-public-state.json). These are simulated game values;
PESO is 1,000,000 base units, not model tokens or devnet settlement.

The restart test exposed a public read before the snapshot commit completed.
The game and personal journals now wait on the arena transaction lock. A separate
regression holds an uncommitted event, verifies both readers block, rolls it back
and verifies neither reader sees it. A prior regression also failed because
accepted action events omitted historical results. They now carry sanitized
parameters and rule-produced cash, goods and influence changes.

## Map consumers

The `/devnet` viewer projects confirmed program events through its existing
snapshot decoder. Initial active-game backfill stays still. New confirmed tail
events move; settled Games have an explicitly recorded replay. Sale and purchase
journeys require the receipt quantities and revenue or cost. Transaction links
and latest account balances remain available. The replay does not reconstruct
historical account balances.

The legacy `/arena` viewer projects durable HTTP action receipts. Old events
without historical results remain in the log without fabricated journeys. Both
maps reuse the owner-approved terrain, Degenie atlas and motion renderer. Workers
travel to market, production, parliament or exchange stations according to the
accepted action. A visual journey is staged after its confirmation; it does not
claim to show the preceding model inference.

Browser review showed both sale journeys at 6.000 seconds on the devnet map.
The whole map was visible at 1280px and 390px. Phone page width remained 390px.
Two new fixture votes appeared in an already-open HTTP viewer, with event IDs
live-10 and live-11 and no reload. With the local event server stopped, the map
held 1.100 seconds across two observations while Play was active. The same saved
server state was restored afterward. See [BROWSER_RESULT.json](BROWSER_RESULT.json),
[desktop](devnet-map-desktop.png) and [phone](devnet-map-phone.png).

## Verification and remaining work

The complete local arena suite passed 134 tests, including the two-process test,
operation retry, restart and transaction visibility checks. Frontend build and
trading projection checks passed. The animation suite passed, including original
pose/contact checks, 827 legacy frames, 1,099 reaction frames and 5,196 action
scenario frames. CI also runs the match-host admission tests: two wallets share
an eligible lobby, and a platform opponent is not added when two players joined.
The dedicated-validator admission test remains ignored unless explicitly enabled.

Different wallets do not prove different human owners. A fresh two-person public
onboarding session was not performed; it would require their own local runners
and approved devnet entry. The existing Match host and private owner access were
not replaced. The local agent test uses registration fixtures, not new registration
Memo transactions.

This does not instrument an agent's internal subagents. Idle workers on the map
are not counted as completed research tasks. Trading token usage and savings are
unmeasured. The public strategy derivative rule belongs to the advertisement
pilot and has not been connected to trading strategies. Those parts of the film
remain a product target.

The original WebGL player below the overview reported unavailable graphics in
this browser; the whole map uses Canvas2D and rendered. Physical-phone frame
rate, a new viewer's understanding and the operating system reduced-motion toggle
were not checked. Code and evidence are published to PR 57. No production deploy,
merge or new video upload is part of this verification.
