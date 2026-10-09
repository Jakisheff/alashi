# Animated devnet viewer

React `/devnet?game=<GamePDA>` reuses the existing stream assets. The backend
validates on-chain receipts; the frontend projects a bounded public journal.
Backfill is not played as a new live action. New visual actions queue, finish,
and return to idle. Explicit replay shortens pauses and does not reconstruct
historical balances or invent public agent messages.

Owner access is separate from the public journal. It reuses the existing exact
signMessage envelope and HttpOnly session endpoints, checks the selected faction
wallet, then reads the canonical chain-wish binding and server quota. Current
Game/faction/record changes, expiry, wallet changes, logout and page exit hide
private state and abort requests. The composer offers explicit deterministic
intents; receipt admission and confirmed chain execution have separate statuses.

Validation:

- `node --experimental-strip-types --no-warnings frontend/scripts/devnet-viewer-check.ts`
- `node --experimental-strip-types --no-warnings frontend/scripts/devnet-owner-check.ts`
- `npm --prefix frontend run build` and `npm --prefix frontend run lint`
- Browser UI fixtures: repeated replay, pause/resume, selected-player reset,
  new Buy→Sell→idle queue, 320/390 widths, missing API status, owner wallet mismatch,
  terminal-game composer absence, uncertain delivery retry with identical body,
  private text absent from the public transcript/URL and server quota display.

The listed fixture checks are synthetic; their results alone do not establish
chain influence. Production release `48e5ec2` also serves the real confirmed
public chain API and React `/devnet` viewer. In bounded Game `1791559769`, a
private typed `vote_no` submitted through that page was confirmed by the runner
at slot `509225187`, matched the public `vote_cast` event, and appeared as the
Genie ballot animation. A `produce` wish was also confirmed; a `sell_one` wish
was declined and must not be attributed to a later normal sale. Final match
settlement and this follow-up guide release are documented separately. No new
3D asset version is generated.

Delivery: the landing “Watch devnet game” CTA already exists in the base route.
The same page contains player selection and registered owner entry. The new
“How to connect your agent” guide explains operator setup, an already joined
registered wallet and active runner binding, the public Game link and agent ID,
and wallet verification. It does not offer browser recovery-secret input or
claim HTTP enrollment joins Solana; finished games are replay-only.

See [ANIMATIONS.md](ANIMATIONS.md) for the full animation inventory, trigger
contract, asset sizes, scene semantics and remaining acceptance dependencies.
