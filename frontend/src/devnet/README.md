# Animated devnet viewer (uncommitted preparation)

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

These browser checks use synthetic API/cookie fixtures, not a real owner proof,
chain transaction or influence acceptance. Local manifest CORS errors come from
the existing absolute production manifest URL; no app exception was observed.
Actual public API currently unavailable. No new 3D asset version is generated.
Ivan owns endpoint/runner/release. Direct human commit hold remains in force.
