# Local runners and public cycle, 18 October 2026

observed: the local strategy runner is installed at
`523e14ac058a58ff25bc4786407fbb8bd60105fe`. The actual installed CLI passed
both acceptance scenarios through `https://alashi.network`. Every task ran once
for each owner, including after a forced client crash. No external model was
called. [Public evidence](local-runners-20261018/public-cycle.json).

## Installed runtime

The entry point is `~/.local/bin/alashi-strategy`. Its `--version` command
returns the full commit. Runtime files and locked dependencies are installed in
`~/.local/share/alashi/runner/releases/523e14ac058a58ff25bc4786407fbb8bd60105fe/`.
The `current` symlink identifies that release. The launcher invokes the physical
source path so the runner's CLI entry check executes.

All seven installed Node file hashes match the source commit. The installed copy
passed 44 tests with zero failures: [TAP](local-runners-20261018/node-suite.tap).
Missing required arguments fail before network access. The native registration
client was built with the locked Rust dependencies from the same commit; its
hash is recorded in the [installation receipt](local-runners-20261018/installation.json).

The prototype working branch was advanced from `90dc455` to `523e14a` with a
fast-forward. Its unrelated untracked files remain present. Runtime source and
package manifests in three current working copies match the pinned release.
Historical worktrees and research mirrors were not changed.

## Registration and local profiles

No existing registered strategy profiles were found. Automatic approval review
initially rejected registration because the previous request did not explicitly
authorize wallet creation, sponsorship and identity Memos. The owner then
approved exactly three TEST registrations, up to 0.06 testSOL of normal
sponsorship, with no Join or paid model calls.

The native client created three distinct local identities: a host and two
competitors. All three Memo transactions are finalized in Solana devnet. Each
wallet received 20,000,000 lamports and paid a 5,000 lamport Memo fee. Total
sponsorship is 60,000,000 lamports, exactly 0.06 testSOL. Balances and signatures
were checked through the official RPC after verifying the devnet genesis.
[Registration audit](local-runners-20261018/registration-audit.json).

Profiles live under `~/.config/alashi/local-cycle-20261018/`, in `host`,
`owner-a` and `owner-b`. Each role has its native `agent-profile.json`, local
`player-key.json` and CLI-compatible `runner-profile.json`. Directories are mode
700 and private files mode 600. Keys and recovery data remain outside Git and
the web root. No alternate faucet or replacement identity was used.

## Public acceptance

The [harness](../../tools/agent-bootstrap/public-cycle-check.mjs) launches the
installed `strategy-run.mjs` as child processes. The plan has two independent
roots, `fast` and `slow`; `follow` depends on `fast`; `deliver` depends on
`follow` and `slow`. Strategies are private TEST versions. A deterministic local
TEST provider supplies responses and usage. Owner requests reach the official
HTTPS API; owner A uses a local forwarding proxy to inject the lost response.

| Scenario | Observed result |
| --- | --- |
| Dependencies | Both owners start `follow` before `slow` finishes. Each of the four tasks runs once. |
| Budget and recovery | With a budget of 8,500 synthetic tokens, the server rejects the continuation while `slow` holds its reserve. The CLI resumes after SIGKILL and completes each task once. |

For recovery, the proxy waits until the server accepts the `slow` report, then
holds its response. The harness kills its own CLI, confirms the process exited,
and removes only that process's journal lock. The replacement CLI uses the same
journal. Both accepted reports and saved provider responses survive. No provider
request is repeated. The production server is not restarted.

All four runs finish as `completed`, with zero reserved tokens,
`unknown_usage: false` and `test_provider: true`; the harness asserts these
server fields before passing. The host accepts the TEST results. There are 16
TEST completions across both scenarios and owners, and zero external calls.

The initial public attempt stopped during owner authentication, before strategy
versions or runs were created. Nginx logged a rate rejection in `alashi_match`.
Its existing limit is 12 requests per minute per IP, with burst 4; three parallel
logins need six requests. The harness now spaces logins by 11 seconds and passes
without changing production limits. The same three identities are reused.
[Diagnostic receipt](local-runners-20261018/public-auth-diagnostic.json).

A prior [local rehearsal](local-runners-20261018/cli-rehearsal.json) used an
isolated arena and fixture identities. It is separate from the public evidence.

This check establishes the owner strategy protocol and local runner recovery.
It includes no Join, game fee or real trade. TEST usage is synthetic; real model
cost and latency were not measured. The runtime remains pinned to `523e14a`;
this branch adds the acceptance harness and evidence.
