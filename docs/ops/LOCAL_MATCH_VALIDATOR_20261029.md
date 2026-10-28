# Full-match local-validator check, 29 October 2026

Observed: `match_host::tests::signed_games_finish_with_solo_two_players_and_empty_lobby` passed. The Rust harness ran one test with zero failures and zero ignored tests. The watchdog wrapper measured 94.883 seconds. [Structured evidence](LOCAL_MATCH_VALIDATOR_20261029.json) records the game accounts and artifact hashes.

| Scenario | Confirmed result |
| --- | --- |
| One visitor plus host bot | Six rounds, two factions, settled payout to the visitor |
| Two visitors | Six rounds, two factions, no host bot, settled payouts to both visitors |
| Empty lobby | Aborted at round zero, settled, no bot faction |

The passing test assertions check all six rounds, participant actions and positive visitor balance changes during settlement. They also check serialized state recovery after round two. Independent RPC reads found 117 successful game transactions and only the rent reserve on each final Game account. The stdout log contains 63 confirmed host receipts; visitor actions and funding are outside that count.

The client test was built from `125393d934000f0218863d2ef881d932e3d81c08` with Rust 1.89 and warnings denied. The local validator loaded the public ALASHI executable copied read-only from devnet program `3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC`, deployment slot 510023387. Its loaded bytes matched the downloaded ELF. The program digest is in the structured evidence.

The runtime was Agave 4.2.1 with a process-local macOS bind interposer. Test-validator assigns wildcard RPC/PubSub addresses in its [upstream source](https://raw.githubusercontent.com/anza-xyz/agave/v4.2.1/test-validator/src/lib.rs). The helper uses the [dyld interposition mechanism](https://raw.githubusercontent.com/apple-oss-distributions/dyld/main/include/mach-o/dyld-interposing.h) to substitute loopback addresses for wildcard binds in this validator process. Actual socket inventory and a refused same-host LAN request verified the restriction before any match. Program bytes, signing and transaction execution were unchanged.

The test used new in-memory participant keys, an isolated ledger and its faucet. The validator was stopped after evidence collection, and RPC, WebSocket and faucet ports were confirmed closed. Private evidence, the test executable, helper source and compressed ledger remain in `~/.local/share/alashi/runner/updates/20261029-0203/`. Temporary build and ledger directories were removed after the archive and executable hashes were checked.

The phases lasted one second. State recovery was serialization/deserialization inside one process. The empty lobby had no participants to receive refund transfers. No transaction was sent to a public cluster.
