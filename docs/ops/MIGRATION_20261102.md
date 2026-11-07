# AWS migration, 2 November 2026

Status: observed, cutover completed. DNS points to `32.236.8.133`. The new
arena, chain API, match host, timer and nginx are active and enabled. Old
application services and the old timer are stopped and disabled. The new
endpoint accepts authenticated writes. Public verification and the certbot
renewal rehearsal passed. Application binaries were copied without rebuilding.

The owner requested [this migration](https://gist.github.com/konon4/b44f5c6d25e808520b830e88437aa7f3)
before the end of 4 November, Astana time. See the
[verification record](MIGRATION_20261102.json).

## Cutover result and interruption

Ivan changed DNS before the requested GO. After Amir explicitly authorized
completion, the source write hold was applied and zero HTTP/managed matches
were confirmed. Old writers stopped at `2026-11-02T14:09:50Z`. The preview
writers were stopped, then the final snapshot, sequence, archive and host
state were streamed from the stopped source. The final snapshot bytes matched
the source checkpoint before activation. Thirteen registrations and the
proposal-key identity were preserved.

Activation was delayed by an SSH timeout. The laptop's public IP had changed
from the address permitted by the new server's single SSH `/32` rule. EC2
status checks remained healthy. That rule was replaced with the current
address; SSH was never opened to all addresses. The retry started the new
services and removed the write hold at `2026-11-02T15:39:57Z`.

The 10-15 minute window was exceeded. The gap between the old-stop and
new-activation checkpoints is 90 minutes 7 seconds. The source-side agent
observed API 502 and write-hold 503 at `14:44:50Z`; static pages still returned
200. The first recorded public API success was at `15:40:44Z`. A full HTTP
availability trace was not collected. Local clock readings moved from 19:08
to 20:40 between checks; active work and waiting cannot be reliably separated
from those readings. The external 502 observation is retained rather than
treating this as a successful short maintenance window.

All 15 normal-DNS HTTPS/API/security probes then passed. The authenticated
write route rejected an empty JSON request with 403 instead of the former
503 hold. The new host listens on loopback 8098, with zero restarts; the arena
and chain API also had zero restarts. Observed available memory was 570 MiB.
`certbot renew --dry-run --non-interactive --no-random-sleep-on-renew` passed.

The final encrypted snapshot `snap-0af453da46c4b1e39` completed. The preparation
snapshot is retained. Snapshots are incremental; a conservative full-size
upper bound for both 10 GiB snapshots is $1.10/month, keeping the verified
base plus snapshot storage below $16 before traffic and taxes.

At `15:43:27Z`, `ivan/agent` posted `MIGRATION-20261102-DNS-DONE`: Google,
Cloudflare and all four authoritative Porkbun nameservers resolved the new
IP; public pages and API probes passed with valid TLS. Old services were
confirmed inactive. Amir's agent acknowledged this in `MIGRATION-20261102-09`.

## Destination and cost

| Item | Observed configuration |
| --- | --- |
| AWS project | Amir's active FREE plan, expires 2 May 2027 |
| Region | `ap-southeast-2`, Sydney, selected by Amir in this task |
| Instance | `i-0d41d049ae2181ffc`, `t3.micro`, x86_64, `ap-southeast-2a` |
| Elastic IP | 32.236.8.133 |
| AMI | `ami-010aeaecac4ab773a`, Canonical Ubuntu 26.04, image dated 28 October |
| Root volume | `vol-04b7db42ec03a5064`, encrypted gp3, 10 GiB |
| Access | Dedicated SSH key; port 22 restricted to Amir's current IPv4 `/32` |
| Public ports | TCP 80 and 443, IPv4 and IPv6 rules; no IPv6 address assigned |
| Runtime controls | IMDSv2 required, termination protection, Standard CPU credits, basic monitoring |
| Swap | 1 GiB; observed available memory 581 MiB, no OOM events |
| Budget | `alashi-monthly-12-usd`, $15 monthly since 3 November (Astana), email thresholds 80% and 100% |
| Preparation snapshot | `snap-0ef21e7200146454f`, encrypted, completed |

The dedicated EC2 key pair is `alashi-aws-20261102`. Its private key remains on
Amir's Mac and was not committed. Operator access:

```sh
ssh -i ~/.ssh/alashi-aws-20261102 ubuntu@32.236.8.133
```

The laptop's public IPv4 can change. If HTTPS works but SSH times out, compare
the current address with the single port-22 rule in `sg-0ffe42ee312f805c8` and
replace that `/32` through AWS administration. Keep SSH restricted to the owner.

Frankfurt EC2 reads returned an explicit SCP denial. Amir approved Sydney while
keeping the FREE plan. `t3.nano` passed a dry-run but the actual launch was
rejected as ineligible for Free Tier. The requested fallback, `t3.micro`, was
launched. No nano instance was created and the plan was not upgraded.

AWS Pricing API rates were checked on 2 November for Sydney. Monthly estimates
below use 730 hours and exclude traffic, snapshots and applicable taxes.

| Item | Verified rate | Monthly estimate |
| --- | --- | --- |
| Linux `t3.micro` | $0.0132/hour | $9.636 |
| One public IPv4 | $0.005/hour | $3.650 |
| gp3, 10 GiB | $0.096/GB-month | $0.960 |
| Base total | | $14.246 |
| Standard snapshot storage | $0.055/GB-month | At most $0.550 for one fully occupied 10 GiB snapshot |

The base plus one full-size snapshot is below $16/month. Traffic and additional
snapshots consume the remaining margin. The budget counts usage before credits
and refunds; it sends alerts rather than enforcing a spending limit. Review the
plan by 2 April 2027, one month before expiry.

On 3 November (Astana), Amir requested raising the initial $12 budget to $15.
`update-budget` and the subsequent read confirmed the new monthly amount;
notification thresholds and email subscribers were preserved. The existing
budget name still contains `12`. Actual-cost warnings now trigger above $12
and $15. The $12 warning can still arrive during a normal month with the
verified $14.246 base cost. This change does not alter the FREE plan.

Pricing references: [EC2](https://aws.amazon.com/ec2/pricing/on-demand/),
[EBS](https://aws.amazon.com/ebs/pricing/),
[public IPv4](https://aws.amazon.com/vpc/pricing/). Regional compute, volume and
snapshot values came from `aws pricing get-products`.

## Preparation copy and acceptance

| Runtime binary | SHA-256, source running process = source file = destination file |
| --- | --- |
| `/opt/alashi-arena/arenad` | `b7682bb8757b6b7ed9e00ad9da515322889f2b00dfb25c4682cd97b5ae0d8edb` |
| `/opt/alashi-chain-api/chain_api` | `ff1c1c08f1369ff0268c18811a52728eaaf19985e8b200e026b7123b1aeeaf1e` |
| `/opt/alashi-host/bots` | `77b96bf091f637b4aece565c3c618f76e1cb1188c8d9ffbc77c2f2e843845646` |

No binaries or frontend assets were built on either server. Dynamic libraries
resolve on the new Ubuntu image.

The actual source releases were newer than the task's inventory:

| Pointer | Release | Files | Bytes |
| --- | --- | --- | --- |
| current | `about-ef6552d-20261031` | 358 | 74,709,227 |
| previous | `site-copy-60b03e9-20261031` | 358 | 74,709,240 |

All 716 file hashes match between servers. The current HTML's immutable asset
references point to the copied previous release. The deck and downloads are
retained within both releases.

Copied the systemd units, nginx site, both active nginx rate-limit files,
Let's Encrypt directory, publisher journal, arena snapshot and sequence, chain
archive, current host key and current host funding state. Private configuration,
units and TLS content matched across 26 files; private digests were not retained.
Transfers used SSH streams without local secret archives. `host.env` and the
current hot key have mode 0600; the hot key belongs to `alashi-host`.

The copied arena retained 13 registrations and zero active HTTP games. Its
proposal-key fingerprint matches the copied unit's expected value. The arena
started successfully through its existing typed snapshot reader. The copied
certificate expires on 4 January 2027.

Excluded staging, historical release directories, source clones, old binaries,
logs, old arena snapshots and historical host wallets under `previous-*`.

During preparation, the new arena, chain API and nginx served the preview.
The new match host, close-settled service and timer stayed inactive; application
units were disabled until cutover. A temporary nginx rule returned 503 for
methods other than GET, HEAD and OPTIONS. The pristine source site is
retained at `/etc/nginx/sites-available/alashi.network.source` on the new server.

Fifteen HTTPS checks passed with certificate verification using
`curl --resolve alashi.network:443:32.236.8.133`: home, devnet, strategies, About,
version metadata, games, capabilities, both public libraries, an archived chain
game and an immutable JavaScript asset. The private research route returned 401
with no-store, a matching POST returned 503, and `.env` and `/api/` returned 404.

Before cutover, the source arena, chain API and match host retained their
original PIDs 1863379, 2400970 and 1166134. The old timer was still active.
DNS was observed as `15.134.10.182` with a 599-second remaining resolver TTL.

[CI run 41455735385](https://github.com/Jakisheff/alashi/actions/runs/41455735385)
passed all five jobs for report commit `5d76e171`. This includes the Rust tests
and compile checks, frontend checks, fresh SBF execution, RustSec and Gitleaks.
Application source is unchanged.

## Coordination channel

The owner updated the task to use `/home/ubuntu/team/BOARD.md` on the old server.
Append only with `>>`, using the UTC board format in `~/team/AGENTS.md`.
Do not put secrets or private digests there. The old recurring sync remains
disabled; this is the current migration's explicit coordination channel.

At `2026-11-02T13:54:34Z`, Amir's agent appended preparation and cost records
`MIGRATION-20261102-01` and `MIGRATION-20261102-02`. The readiness record
`MIGRATION-20261102-DNS` gives `32.236.8.133` and states
`CUTOVER_TIME_UTC=NOT_AGREED`. It requests an IP acknowledgment and a UTC window,
with a hold on DNS and old services until Amir approves the window.

At `2026-11-02T13:56:14Z`, `ivan/agent` acknowledged the IP in
`MIGRATION-20261102-03`. It independently verified HTTP 200 for home, devnet and
strategies through `--resolve`, the certificate expiry and the home HTML hash
against the old server. It confirmed that the A record TTL is already 600.
That exchange left the window unapproved. Ivan subsequently proposed an
immediate window in `MIGRATION-20261102-05` and reported the early DNS change
in `MIGRATION-20261102-06`. Amir then explicitly authorized completion.

## Cutover runbook

Ivan controls the A record at Porkbun. The intended order below puts DNS after
the final state copy and health checks. On this occasion DNS changed first;
the observed sequence and interruption are recorded above. TTL was already 600.

1. Put the old endpoint on a write hold. Check both HTTP games and the managed
   host's current book for active work. Drain any active games. If
   `alashi-close-settled.service` is running, let it finish before stopping its
   timer. Follow the continuity constraints in
   [the prior runbook](FFS_DEPLOY_20261026.md).
2. Stop and disable the old match host, close-settled timer/service, arena and
   chain API. Confirm they are inactive. Stop the preview arena and chain API
   on the new server before replacing any data.
3. Recheck binary and configuration identity against preparation. Restage any
   changed artifacts before proceeding. Transfer the final arena snapshot and
   sequence, chain archive, current host book, funding state and publisher
   state. Include `matches.json` if it has been created since preparation.
   Preserve owners and 0600 permissions. Validate the snapshot, sequence and
   unchanged proposal-key identity before startup.
4. Start and enable the new arena, chain API, match host, close-settled timer
   and nginx. Verify that only the new match host runs with the hot key.
   Restore the retained source nginx site to remove the temporary write hold,
   run `nginx -t`, and reload. Recheck the new IP through `--resolve`.
5. Ivan changes the A record. Check HTTPS pages and APIs through normal DNS.
   Amir then tests the demo: sign in, agents, match and result. Run
   `certbot renew --dry-run` after DNS resolves to the new endpoint. Take a
   fresh snapshot of the final synchronized server.
6. Leave old services stopped and preserve the old files for rollback until
   the old project closes. Do not delete anything from the source server.

## Recovery and remaining work

For rollback, freeze new writes and stop the new match host and timer before
restarting an old one. Capture coherent new state with the new arena and chain
API stopped. Synchronize the latest state and host books back to the source,
which retains the same binaries. Reconcile pending devnet transactions before
host restart. Restore the old nginx site from
`/var/backups/alashi/migration-20261102/nginx-before-cutover.conf`, test and
reload nginx, and enable the old services only after synchronization. Ivan
then returns DNS to the old IP. Do not restore an old snapshot over newer
registrations or decisions. This source-server rollback is available only
while the old AWS project remains accessible.

The preparation snapshot is a crash-consistent infrastructure copy. A restore
rehearsal has not been performed. It predates the required final data sync.

Amir's authenticated demo remains pending. No application source was changed;
Rust/SBF suites passed in CI rather than being rebuilt locally for the copy.
Domain transfer and program upgrade-authority transfer remain Ivan's tasks.

On 3 November (Astana), a separate upgrade-authority key was generated on
Amir's Mac. Its public address is
`8YvNucj8cWM1VNu6kCgRT3Fe8Esh4mAFMJzpYihF4i2U`. Only this address was
sent through BOARD as `MIGRATION-20261102-AUTH` at `2026-11-02T19:17:09Z`.
The default Solana wallet was unchanged. The backup key returned the same
public address, and BIP39 seed derivation matched the generated private key.
An encrypted macOS login-keychain recovery record was saved and read back;
both key and seed matched exactly. Apple Passwords import stalled and the
macOS UI connection stopped responding. Amir chose to keep the verified
login-keychain backup. Temporary plaintext seed, CSV and duplicate key files
were removed after a second exact comparison with the encrypted record.
The working key remains in the owner-requested location. An independent
copy outside this Mac has not been verified. No secrets or private recovery
files are in this repository. A read-only devnet query still
reported Ivan's authority `64WA7wFVJK3GkfSAkfiAWR37bgjCw1RYcj17qEDj4AVJ`;
Amir's agent submitted no authority-transfer transaction.

## Off-server release builds

[The release workflow](../../.github/workflows/release-server.yml) builds the
three server binaries with Rust 1.89.0 from `rust-toolchain.toml` and locked
dependencies on `ubuntu-24.04` x86_64. It builds the frontend with Node 22.18.0,
production live API enabled and immutable `/releases/server-<commit>/` assets.
It does not deploy or use AWS credentials.

Tags matching `server-v*` trigger it. `workflow_dispatch` supports manual runs
after the workflow is available on the default branch. The verification tag is
`server-v20261102-verify`, source commit `03bd4fe0`.

The artifact `alashi-server-linux-x86_64-<commit>` contains a tar archive and
its external `SHA256SUMS`. Inside are `bin/arenad`, `bin/chain_api`, `bin/bots`,
`frontend/`, `BUILD.txt` and per-file `SHA256SUMS`. The workflow extracts its
archive and verifies every checksum before uploading it. Retention is 14 days.

Download with `gh run download <run-id> --name <artifact-name>`, then run
`sha256sum -c SHA256SUMS` beside the tar file and verify the inner checksums
after extraction. Installation remains a reviewed operation: copy the binaries,
preserve service permissions, and restart only the affected services during a
safe window. For frontend publication, retain the current deck/downloads,
prepare truthful `version.json` provenance and use the existing
[static publisher](STATIC_RELEASE.md). Do not replace `current` with an
unprepared raw build directory.

[Verification build 41475693563](https://github.com/Jakisheff/alashi/actions/runs/41475693563)
passed the complete package job, including archive extraction and every
checksum. Artifact `13157967029` is 22,163,850 bytes and was available at
acceptance. Its GitHub upload digest is
`015ed756888dbbaaac168067aefb66f5b2cee49324fc5b7311279191569c4393`.
This digest describes the GitHub artifact; the tar's own checksum is inside it.
[PR CI 41475703568](https://github.com/Jakisheff/alashi/actions/runs/41475703568)
also passed all five jobs for `03bd4fe0`. `actionlint` and a YAML/schema check
passed locally. Crapper, Dryer and Mutator reported no supported source files
for this YAML file; no complexity or mutation score is claimed.
