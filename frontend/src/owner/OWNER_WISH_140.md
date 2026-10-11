# Private instruction admission

Contract: DIN-OWNER-WISH-140-20261011-01, Ivan docs d4a8d5ff16cfddc3314a7481638b3bb7ca962476.

Each agent has three private instructions per match. The panels trim only outer
whitespace for a new submission, then count Unicode scalar values (not bytes,
UTF-16 units or graphemes). Combining marks and ZWJ each count. The maximum is
140; empty, control-containing or malformed surrogate text is rejected before
request dispatch. Over-limit input stays editable and is never truncated.

Both panels share the counter and validation with their API clients. Pending
retries retain their exact text and submission ID; authentication, quota from
the server and historical journal/reply projection are preserved. Reading an
old instruction does not apply the new admission limit.

Run from `frontend/`:

```sh
node --experimental-strip-types --no-warnings scripts/owner-wish-140-check.ts
node --experimental-strip-types --no-warnings scripts/live-api-check.ts
node --experimental-strip-types --no-warnings scripts/owner-browser-session-check.ts
node --experimental-strip-types --no-warnings scripts/devnet-owner-check.ts
npm run build
npm run lint
```

Local panel QA used mock APIs only at widths 1440, 390 and 320. It covered
140/141 ASCII and emoji, outer trim, blocked Enter submission, exact network
retry, zero remaining quota, historical text/reply and hiding history on session
expiry. It did not use a real wallet, private session or server admission.

Ivan owns matching Rust validation and publication. In particular, 140 emoji
can exceed the previous server's 512-byte cap. Publish both admission contracts
together; this source delivery alone does not establish live server behavior.
