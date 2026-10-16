# Confirmed trading actions on the whole map

The owner-approved terrain and original Degenie atlas remain the visual reference.
The trading page consumes the arena's durable public game-action receipts. It
shows confirmed rule outcomes after acceptance. It does not show a model thinking
or claim that an animation happened before the server accepted an action.

Intent: a Degenie carries a confirmed instruction to the corresponding station,
shows its receipt and returns to its owner. The poses are attention, departure,
station work, receipt return and settled idle. A sale travels to the market;
production travels to the production station; voting travels to parliament.
The visible document is a receipt marker, not a commodity held in the hands.

The Lasseter applications in [strategy staging](../strategies/DIRECTION.md) apply
here: rigid sprite proportions, anticipation before departure, short travel and
handoff, original overlapping atlas motion, explicit poses, smooth endpoints,
curved paths, restrained receipt accent, secondary smoke and readable silhouettes.
Personality follows the station task and confirmed outcome. Physical goods contact
is inapplicable to the receipt marker. The whole map stays visible on a phone.

Each accepted action has one durable event ID. Redelivery cannot add a journey.
Historical cash and goods changes come from that event, not the latest balance.
Old events without result data remain in the log and are not animated. Both
owners can act concurrently; journeys for one owner and station are serialized
only to prevent visually overwriting a previous receipt. This is presentation
timing. It does not alter the server clock or order of accepted actions.

The map initially stays still until a receipt is received. Offline mode freezes.
Replay and seek use the same receipts. Reduced motion holds a meaningful pose.
No task graph, token usage or private reasoning is inferred from a trade. The
current map depicts the first two factions; other factions remain in the log.

Acceptance: signed owners with separate agent processes and loopback test
providers, exact historical balance assertions, duplicate/restart checks, browser
motion on desktop and 390px, offline freeze, frontend build and animation checks.
Two human operators, production wallet onboarding and real LLM economics require
separate acceptance and are not established by fixture wallets.

## Solana devnet consumer

The primary `/devnet` viewer also uses the whole map. Its existing decoder checks
the confirmed snapshot, program, faction associations and stable transaction/log
IDs before the map receives events. Initial active-game backfill stays still;
new confirmed tail events animate. A complete settled Game opens a labelled
recorded replay. Partial history is labelled, with no invented balances. Sale
and purchase journeys require the receipt's quantities and revenue or cost.
Latest account balances remain separate from replay receipts. HTTP balance
changes are never presented as Solana transactions.
