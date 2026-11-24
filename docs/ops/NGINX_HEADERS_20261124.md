# nginx security headers and certbot check, 24 November 2026

Closes audit findings 1 and 4 from docs/ops/AUDIT_SITE_20261124.md.

## Change

`/etc/nginx/sites-enabled/alashi.network`: three headers added after every
`add_header Referrer-Policy` line (56 blocks: server level + every location),
with matching indentation, via a scripted insert. Backup before the change:
`/etc/nginx/alashi.network.bak-20261124-headers`. Applied with `nginx -t`
and `systemctl reload nginx` (reload, no service restarts).

Final headers on all public routes (verified by curl on /, /agent.md,
/strategies, /devnet, /strategy-library, immutable assets, 404):

- `Strict-Transport-Security: max-age=31536000` (no includeSubDomains, no preload)
- `X-Frame-Options: DENY`
- `Content-Security-Policy: default-src 'self'; script-src 'self' 'wasm-unsafe-eval' blob:; style-src 'self'; img-src 'self' data: blob:; font-src 'self' data:; connect-src 'self'; worker-src 'self' blob:; object-src 'none'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'`

## CSP iterations (recorded for honesty)

1. v1 `script-src 'self'`: broke the 3D stack live. DRACOLoader compiles
   WebAssembly, and WASM instantiation is governed by script-src; the
   desk-genie.glb model failed to load on / and /devnet. Static inspection
   of the main bundle had missed this because DRACO lives in the lazy
   smoke-*.js chunk.
2. v2 added `'wasm-unsafe-eval'`: WASM compiled and the scene rendered, but
   troika/DRACO workers call importScripts from blob: URLs, which inside a
   worker is also governed by script-src; text workers failed.
3. v3 final, added `blob:` to script-src. This is the standard minimum for
   the three.js + troika + DRACO stack; no 'unsafe-eval' or 'unsafe-inline'.

The site ran with the broken v1 for roughly two minutes before the browser
check caught it. The browser check is now a required step for any future
CSP change on this site.

## Browser acceptance (final CSP)

Chrome on Amir's MacBook, four pages, 5 s settle after load, console capture
via CDP: 0 errors, 0 CSP violations on all:

| Page | Status | Title | Canvases |
|---|---|---|---|
| / | 200 | ALASHI NETWORK \| Teams test their hypotheses | 1 (1440x897) |
| /devnet?game=GqHZ… | 200 | Devnet gameplay · alashi | 2 (1672x941) |
| /strategies | 200 | ALASHI NETWORK \| Teams test their hypotheses | 1 |
| /live | 200 | Alashi · personal stream | 1 (407x434) |

## certbot renewal check (audit finding 4)

- `certbot.timer` active: next run listed, last run 10 h before the check.
- Renewal config present: `/etc/letsencrypt/renewal/alashi.network.conf`.
- `sudo certbot renew --dry-run` (Let's Encrypt staging): success for
  alashi.network. The full renewal chain works on the migrated server.
- Certificate valid until 2027-01-04; renewal triggers within the normal
  30-day window.

## Rollback

```
sudo cp /etc/nginx/alashi.network.bak-20261124-headers /etc/nginx/sites-enabled/alashi.network
sudo nginx -t && sudo systemctl reload nginx
```

HSTS note for rollback: browsers cache the max-age for up to a year; removing
the header stops extending it but does not reset already-stored pins.

## Limits

- nginx config lives on the server only; this record documents the exact
  applied state.
- Console-error acceptance covered four representative pages, not every SPA
  route; owner-authenticated flows were not exercised.
