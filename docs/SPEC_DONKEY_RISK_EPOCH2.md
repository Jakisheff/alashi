# Epoch 2 donkey risk: source contract (candidate)

Status: source candidate only. The program is not upgraded; no epoch-2 Game, paid oracle request, or live result has been verified. This document describes the intended wire for runner and frontend integration.

Epoch `2` inherits the epoch-1/90s market, laws, credit, shuttle, hard currency, license and settlement rules. It changes only donkey buying. Epochs `0` and `1` retain `BuyDonkey` and the historical `donkey_bought` event: one good for one in-game PESO, guaranteed.

In epoch 2, `AttemptDonkey` accepts one Action-phase attempt in rounds 1–5. It charges `5 * PESO` (in-game cash) and consumes the faction's Action before any outcome exists. An independently verified later draw succeeds with probability 3/5 and credits five goods, or returns zero goods with probability 2/5. An oracle abort is a distinct terminal outcome: zero goods, no random draw claim. There is no refund or same-turn retry in either zero-goods case. The ordinary Market purchase is still guaranteed at the current market price. Program preconditions reject before debit/stamp if the phase/round, cash, alive/acted state, inventory capacity, or oracle baseline is invalid.

## Program accounts and ordering

`Initialize(epoch=2)` requires Switchboard entropy mode. `ConfigureDonkeyOracles` must then run before any Join: `admin` signer, `game` readonly, canonical `oracle` PDA init writable (`[b"donkey-oracles", game]`), `system_program`, followed by exactly six **distinct** readonly Switchboard randomness accounts in round order. The config fixes source keys, queue and authority; each source must be uncommitted and unrevealed at configuration. Epoch-2 `Join` adds that canonical config PDA as its one readonly remaining account and rejects a missing or invalid config before transferring the entry fee. Legacy Join accounts are unchanged. An unconfigured empty Lobby can still time out and abort.

`AttemptDonkey` accounts: `player` signer, `game` readonly, `faction` writable, canonical `oracle` writable, configured current-round `randomness` readonly. The source must still be uncommitted and unrevealed when the fee and Action are committed. `Advance(Action→Law)` receives the complete faction set, then writable oracle config, then exact current-round randomness account. It accepts a fresh unrevealed provider commitment only from that configured source, with `seed_slot` after the last accepted attempt. `RevealLaw` receives the pinned randomness account in its fixed account list, then writable oracle config, then every due writable faction in ticket order. Provider reveal and `RevealLaw` must be bundled in one transaction: the pinned SDK exposes its full value only in the reveal slot. All due tickets resolve atomically, using the full value and a domain-separated exact 3/5 mapping. A terminal phase abort marks every due ticket technically aborted without goods; it does not report a 2/5 loss.

The current bots host has no built-in Switchboard configure/commit/reveal lifecycle. External oracle setup and crank readiness must be explicit and checked before an epoch-2 host starts. No normal runner may choose the draw or substitute a source. This is a deployment prerequisite, not a completed live integration.

## Anonymous public projection

Existing `GET /chain/devnet/games/<GamePDA>` is the only public read. It requires no owner wallet/cookie. `game.epoch` is numeric `2`. The event decoder adds only these confirmed events; the envelope already supplies canonical `id = "<signature>:<log_index>"`, `signature`, numeric `slot`, `log_index`, and `block_time`:

```json
{"type":"donkey_attempted","game":"<GamePDA>","faction":"<FactionPDA>","round":2,"fee":"5000000","id":"<signature>:<log_index>","signature":"<signature>","slot":123}
{"type":"donkey_resolved","game":"<GamePDA>","faction":"<FactionPDA>","round":2,"success":true,"goods":5,"fee":"5000000","reason":0,"id":"<signature>:<log_index>","signature":"<signature>","slot":124}
```

The examples above are schema examples, **not production events**. `reason=0` means a verified draw: `success=true,goods=5` is a win; `success=false,goods=0` is an empty draw. `reason=1,success=false,goods=0` means technical terminal oracle abort and does not imply the 40% draw occurred. An accepted attempt without a later resolution is pending only while confirmed history remains complete enough to support that conclusion. Never fabricate outcomes from timeout, partial history, UI animation, or private owner wishes. Historical `donkey_bought` remains a separate legacy event; do not reinterpret it as a risk attempt.

Frontend and runner adapters should key receipts by public Game/Faction/round and canonical event ID/signature/slot, and should distinguish pending, verified win, verified empty and technical abort. No synthetic fixture is a public Game event.
