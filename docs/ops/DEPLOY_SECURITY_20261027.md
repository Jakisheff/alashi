# Security maintenance, 27 October 2026

Status: application deployment blocked. No application binary or frontend release was replaced. The dependency branch updates `zstd-sys` for the indexer and bots, and updates the frontend build tool dependency `zod`. The frontend output-module inventory contains no `zod` modules.

The separate Tauri companion lockfile also updates `rustls` to 0.23.46, `rustls-webpki` to 0.103.16, and `webpki-roots` to 1.0.10.

The RustSec audit still reports [RUSTSEC-2026-0391](https://rustsec.org/advisories/RUSTSEC-2026-0391), [RUSTSEC-2026-0398](https://rustsec.org/advisories/RUSTSEC-2026-0398), and [RUSTSEC-2026-0403](https://rustsec.org/advisories/RUSTSEC-2026-0403) in the indexer and bots lockfiles. The required fixed versions are outside the version constraints of Solana 3.1.14 and its `zstd` dependency. No audit exception was added. The running match host must not be rebuilt under this maintenance scope. The exact source revision of the staging arena is unverified, and it must not be rebuilt under this scope.

The full runtime path comparison from the chain API revision `523e14a` to the branch includes arena source changes, so the deployment's dependency-only precondition does not hold for that revision. The comparison from arena revision `39bda66` contained only the changed lockfiles. No application deployment was attempted while the audit gate remained red.

Server cleanup removed reproducible build outputs and caches. Free space rose from 2.3 GiB to 19 GiB; clones, archives, application backups and releases were retained. The four application services retained their process IDs and executable hashes. Security updates were installed for `libbrotli1`, `nginx`, and `nginx-common`; nginx performed its package binary upgrade and remained active, and HTTPS and a fresh SSH connection passed. The existing reboot-required marker was retained; no reboot was performed. Needrestart listed deferred service restarts. Old nginx and Brotli packages were saved for rollback; no rollback was needed.

The associated dependency review is [PR #62](https://github.com/Jakisheff/alashi/pull/62). Exact maintenance evidence is summarized in [the companion record](DEPLOY_SECURITY_20261027.json).
