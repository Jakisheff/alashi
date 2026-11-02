# AWS migration preparation, 2 November 2026

Status: observed, ready for coordinated cutover. DNS still points to the old
server. The new server is a read-only preview. Final data synchronization and
activation of the new match host require a separate cutover window.

The owner requested [this migration](https://gist.github.com/konon4/b44f5c6d25e808520b830e88437aa7f3)
before the end of 4 November, Astana time. See the
[verification record](MIGRATION_20261102.json).

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
| Budget | `alashi-monthly-12-usd`, $12 monthly, email thresholds 80% and 100% |
| Preparation snapshot | `snap-0ef21e7200146454f`, encrypted, completed |

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

Pricing references: [EC2](https://aws.amazon.com/ec2/pricing/on-demand/),
[EBS](https://aws.amazon.com/ebs/pricing/),
[public IPv4](https://aws.amazon.com/vpc/pricing/). Regional compute, volume and
snapshot values came from `aws pricing get-products`.

## Copied material and acceptance

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

The new arena, chain API and nginx are active for preview. The new match host,
close-settled service and timer remain inactive. Application units remain
disabled for automatic startup until cutover. A temporary nginx rule returns
503 for methods other than GET, HEAD and OPTIONS. The pristine source site is
retained at `/etc/nginx/sites-available/alashi.network.source` on the new server.

Fifteen HTTPS checks passed with certificate verification using
`curl --resolve alashi.network:443:32.236.8.133`: home, devnet, strategies, About,
version metadata, games, capabilities, both public libraries, an archived chain
game and an immutable JavaScript asset. The private research route returned 401
with no-store, a matching POST returned 503, and `.env` and `/api/` returned 404.

The source arena, chain API and match host retained their original PIDs
1863379, 2400970 and 1166134. The old close-settled timer is still active.
DNS was observed as `15.134.10.182` with a 599-second remaining resolver TTL.

## Coordinated cutover, not executed

Ivan controls the A record at Porkbun. Give him 32.236.8.133 and agree a
10-15 minute window before the end of 4 November, Astana time. Have him confirm
TTL 600 at least one hour beforehand. Do not change DNS during preparation.

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

Before new writes are accepted, the old server remains the live source and
rollback is simply abandoning the preview. After cutover, freeze new writes
and stop the new match host before restarting an old one. If the new endpoint
has accepted writes or hosted matches, synchronize its latest state and host
books back before rollback. Reconcile pending devnet transactions before host
restart. Do not restore an old snapshot over newer registrations or decisions.

The preparation snapshot is a crash-consistent infrastructure copy. A restore
rehearsal has not been performed. It predates the required final data sync.

DNS cutover, final sync, new-host execution, post-DNS certbot dry-run and Amir's
authenticated demo remain pending. The post-migration
`.github/workflows/release-server.yml` is deferred to that phase, as requested:
build Rust 1.89.0 binaries with `--locked` and Node 22.18.0 frontend artifacts in
GitHub Actions, then deploy verified artifacts. No application source was
changed here; Rust/SBF suites were not rebuilt locally for this copy operation.
Domain transfer and program upgrade-authority transfer remain Ivan's tasks.
