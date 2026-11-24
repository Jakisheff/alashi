# Site audit, 24 November 2026

Black-box audit of https://alashi.network plus repository drift analysis,
run on top of the 23.11 23:24 STATUS baseline (pages/API 200, no 5xx,
prod release `about-ef6552d-20261031`). Scope per the agreed prompt: delta
from baseline, prod-vs-main drift, claims vs README, machine audience,
deep links, infra and security, performance, freeze checks. No deploy was
performed; deployment remains the owner's decision.

## Live release identification

- `/` loads assets from `/releases/site-copy-60b03e9-20261031/assets/index-DN267sdc.js`
  and `last-modified: Sat, 31 Oct 2026`.
- `git fetch origin ef6552d6f07c96d8c53f4df581671b0821a8410f` resolves to
  `docs(web): remove internal ownership history from About` (31.10 12:59 UTC).
  MIGRATION_20261102.md line 130 confirms current release
  `about-ef6552d-20261031` (358 files), previous `site-copy-60b03e9-20261031`
  (same file count, immutable asset references preserved).

## Baseline delta (vs 23.11 23:24)

| Check | Result | Command |
|---|---|---|
| `/`, `/about/`, `/strategies`, `/deck/`, `/agent.md`, `/robots.txt`, `/sitemap.xml`, `/live` | 200 | `curl -s -o /dev/null -w "%{http_code}" <url>` |
| `/devnet?game=GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b` | 200 (SPA shell) | same |
| `/strategy-library` 17,953 B; `/chain/devnet/games/GqHZ…` 29,720 B | 200, 1.0–1.4 s | same |
| `/games` | 200, 22 B (empty list; no public HTTP games) | same |
| 5xx | none observed | all above |

No regression from the 23.11 baseline.

## Freeze checks

- `/` sha256 `c3ad4f569c012d0988bd597da7e00256d4081c774523df07517a484b25403224`
  equals the value recorded in SITE_COPY_DEPLOY_20261031.json. `/about/` sha
  differs from that record, consistent with the later `about-` release
  changing only the About page.
- `/agent.md` is byte-identical to `git show ef6552d6:frontend/public/agent.md`.
- `frontend/dist` is not tracked in git; per-asset byte verification relies on
  the CI publisher check at deploy time (120/120 files verified on 31.10).
  Today's HTML-level sha match plus the immutable asset URL from the record
  are the observable confirmation. Limit, not a failure.

## Prod-vs-main drift

110 commits between `ef6552d6` and `origin/main`; 7 touch `frontend/`,
17 touch `arena/`. Route compatibility (67 = 67), law wire names and sha256
v2 tokens were already verified on 23.11.

Not yet deployed, user- or agent-visible:

1. `agent.md` in main adds the client-rebuild warning for the fee update
   (0.05/0.1 -> 0.005/0.02 testSOL, commit 14c63b2). The fee update IS an
   ancestor of the deployed release, so prod already enforces new fees:
   agents running pre-14c63b2 binaries get rejections that the deployed
   guide does not explain. Severity: medium (machine audience).
2. Hidden-tab timer fixes (bf0d03c, 76063128, 6fff01f5): FRONTEND_PERF_20261114
   measures -43% script time on the /live demo. Not user-visible as a bug,
   performance only.
3. Rules-WASM canonical terms (76063128): UI term drift risk only.

## Claims vs README

- `/` states "Available now" scope (private comparisons on a static English
  advertising brief, registered owner + local runner required; general
  scenarios planned; customer behavior and sales impact not measured).
  Matches README "Product direction" and its `not_validated` status.
- `/about/` carries explicit hedges ("do not establish demand", "remain
  unvalidated", "not an implemented simulator"). Matches README.
- `/deck/` is dated 11 October 2026, labeled "Earlier pitch" from the landing,
  and states that slides distinguish recorded demonstrations from concepts.
  Matches its archive status.

No claim stronger than README was found.

## Machine audience

- `/agent.md` served as `text/plain; charset=utf-8`, 10,284 B, no JS needed.
- `robots.txt` disallows `/game/`, `/agents/`, `/api/`; `/agent.md` is open.
- `sitemap.xml` lists `/`, `/about/`, `/deck/` only; `/strategies` and
  `/agent.md` are absent. Minor.

## Infra and security

| Item | State | Command |
|---|---|---|
| nginx package | `1.28.3-2ubuntu1.14` (STATUS 23.11) is the current fixed build for Ubuntu 26.04: USN-9159-1 (18.11, NGINX2Root RCE set) fixed in `.13`, USN-9182-1 (20.11, CVE-2026-118899/123640/125283) fixed in `.14`. No newer USN found as of 24.11 | ubuntu.com/security/notices/USN-9159-1, USN-9182-1 |
| TLS | TLSv1.3, chain verifies, Let's Encrypt, expires 04 Jan 2027 | `openssl s_client -connect alashi.network:443 -servername alashi.network` |
| Compression | brotli 1,941 B vs 6,638 B raw for `/` | `curl -H "Accept-Encoding: br,gzip" -o /dev/null -w "%{size_download}"` |
| Asset caching | `public, max-age=31536000, immutable` on release assets; HTML `no-store` | `curl -sI <asset-url>` |
| HTTP/2 | yes | `curl -sI -o /dev/null -w "%{http_version}"` |
| Security headers | present: `x-content-type-options: nosniff`, `referrer-policy: strict-origin-when-cross-origin`; **absent: `strict-transport-security`, `content-security-policy`, `x-frame-options`** | `curl -sSI https://alashi.network/ \| grep -iE "strict\|security\|frame"` |
| TTFB | 1.05–1.09 s from Almaty (RTT-dominated; static file, `no-store`) | `curl -s -o /dev/null -w "%{time_starttransfer}"` |

## Performance

- PageSpeed Insights API returned 429 (anonymous quota exhausted); no fresh
  lab run today. Lab reference remains FRONTEND_PERF_20261114.md: LCP 2480 ms
  against a 2500 ms budget on the /live demo under 4x CPU throttle and
  software rendering; smoke-*.js (1070 KB, lazy) off the critical path.
- Field (CrUX) data not collected: no API key and traffic likely below the
  CrUX inclusion threshold. Gap, not a pass.

## Findings

| # | Finding | Severity | Fix direction |
|---|---|---|---|
| 1 | No HSTS (and no CSP / X-Frame-Options) on public pages | medium | add `strict-transport-security` in nginx; consider CSP + frame policy |
| 2 | Deployed `agent.md` lacks the fee-rebuild warning present in main | medium | ship the main `agent.md` text (doc-only publish) or deploy |
| 3 | Prod 110 commits behind main (7 frontend, 17 arena) | medium | owner deploy decision; compatibility already checked 23.11 |
| 4 | Sitemap omits `/strategies` | low | add route to sitemap source |
| 5 | `/games` empty (22 B) | info | expected: no public HTTP games |
| 6 | TTFB ~1.05 s from Almaty | info | RTT-dominated, no action implied |

## Limits

- Server-side state (systemd units, package versions) is taken from the
  23.11 STATUS record and Ubuntu notices; no SSH session was opened today.
- No fresh Lighthouse lab run (PSI quota); no field data.
- `frontend/dist` untracked in git restricts asset-level freeze verification
  to the publisher's deploy-time check.

## Verification commands

All checks above are reproducible with the curl/openssl/git commands shown
in each row, from any machine with network access to alashi.network and the
Jakisheff/alashi repository.
