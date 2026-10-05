# Devnet deployment and bot match, 5 October 2026

Observed: the Alashi program was deployed to Solana devnet under a new program ID, and one match between the two host bots was played against it on public devnet with an on-chain settle and payout. Only devnet was used. Times are block times in UTC+05:00.

## Program

| Field | Value |
|---|---|
| Date | 2026-10-05 |
| Program ID | [`3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC`](https://explorer.solana.com/address/3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC?cluster=devnet) |
| Previous ID (account not found on devnet, checked 5 Oct 2026) | `8EikcWzM7d3EjttApmymo2maWp5A3NtMpKoYdWUzzzL` |
| Deploy wallet and upgrade authority | `64WA7wFVJK3GkfSAkfiAWR37bgjCw1RYcj17qEDj4AVJ` |
| ProgramData address | `HcGSeuzKQVdCTM3H4zYhPRuYzsubSLHCqgoTewpdVxTp` |
| `target/deploy/alashi.so` | 464392 bytes, sha256 `de492e03624449a36e10a9267563ed46bbaf4eb26204f60306f2c5e10020cde2` |
| Build | `bash tools/build_sbf.sh` (cargo-build-sbf 4.1.0, platform-tools v1.54) |
| `solana rent 464392` | 2.3597616 SOL |
| ProgramData balance after deploy | 2.3599902 SOL (program bytes plus the 45-byte header) |
| Program account balance | 0.00083312 SOL |
| Deploy wallet spend | 8 SOL before, 5.63686168 SOL after: 2.36313832 SOL for rent and write transactions |
| Deploy signature | [`i8QymYNr...7r5`](https://explorer.solana.com/tx/i8QymYNrsscv8zEDEP9pME7QcuMGdL4yunf7ixmaDJB5wyXay6x3uLB34q9uYvnbgMEoHyutYMNQzsijvmhK7r5?cluster=devnet), slot 507743508, 2026-10-05 18:15:56 |

Deploy command (Solana CLI 4.3.0):

```bash
solana program deploy target/deploy/alashi.so \
  --program-id target/deploy/alashi-keypair.json \
  --keypair ~/.config/solana/alashi-devnet-deployer.json \
  --upgrade-authority ~/.config/solana/alashi-devnet-deployer.json --url devnet
```

`solana program show` after the deploy: owner `BPFLoaderUpgradeab1e11111111111111111111111`, authority `64WA7wFVJK3GkfSAkfiAWR37bgjCw1RYcj17qEDj4AVJ`, last deployed in slot 507743508, data length 464392 bytes. `solana program show --buffers` for the deploy wallet listed no leftover buffers. The upgrade authority was not transferred. The program and deploy keypairs are kept outside the repository.

The deploy wallet was funded with 8 devnet SOL from a workshop wallet: [`2cxfwNSj...6C7`](https://explorer.solana.com/tx/2cxfwNSjQGyo3X6PGXiSmv4vHqumQJrmiAc1qUjnUM7XvnXsPBpDhJpWVDR3PtDsUE5851y1TjsGMwsHEAewB6C7?cluster=devnet).

## Program ID change

The ID was replaced in `programs/alashi/src/lib.rs`, `rules/src/state.rs`, `Anchor.toml` (`[programs.devnet]`, the only programs section), `app/index.html`, `docs/AGENT_GUIDE.md`, the README local-validator command and both proof scripts `tools/studio_local_proof.py` and `tools/cyber_local_proof.py`. The scripts address every instruction to their `PROGRAM` constant, and a program built with the new `declare_id!` would reject the old one. Recorded evidence of earlier runs keeps the old ID: `docs/ops/*.json`, `docs/ops/CYBER_VIDEO_20260906.md`, `app/alashi-*-demo.html` and `data/`.

## Checks after the ID change

Run on the branch `ivan/devnet-deploy` after `bash tools/build_sbf.sh`, on this laptop:

| Check | Result |
|---|---|
| `cargo test --locked -p alashi-rules` | 34 passed, 0 failed (24+1+4+5) |
| `cargo test --locked --manifest-path arena/Cargo.toml` | 47 passed, 0 failed (33+2+1+3+8) |
| `cargo test --locked --manifest-path indexer/Cargo.toml` | 10 passed, 0 failed (7+3) |
| `cargo check --locked -p alashi` | ok |
| `cargo build --locked --manifest-path bots/Cargo.toml` | ok, 5 warnings |
| `cargo test --locked -p alashi` | 27 passed, 0 failed (1+21+5), including `replay_epoch_90s_full_party` and `replay_equivalence_full_party` |

## Bot match on public devnet

The bot driver ran with `ALASHI_RPC=https://api.devnet.solana.com` from a scratch directory outside the checkout, with two new bot wallets funded with 0.5 SOL each from the deploy wallet. No LLM key was set, so both bots used the greedy heuristic.

| Field | Value |
|---|---|
| Game ID | 1791206276 |
| Game account | [`52wdUzSA6pk2aLqfcNiknwmgjZJW36DkA71hyhZ2wZ8N`](https://explorer.solana.com/address/52wdUzSA6pk2aLqfcNiknwmgjZJW36DkA71hyhZ2wZ8N?cluster=devnet) |
| Bot 1 (Aibot, admin and rake receiver) | wallet `7M1u5hEW5weU9EtpBtY4YXsBySGktMr4nP6wKTycLy2c`, faction `9iDfkJJwycQidcnYHhynuk2YeLrDVCBy38t7H7RaAfqU` |
| Bot 2 (Botagul) | wallet `9ZJ9cCWrXHECZofvjqMqu9jC9Mc6H8YVCHp2BUybXkr4`, faction `3iZwcD6KHRf33J3TK2Y9VtW8DK2JLsw6cddjcpWrnGt8` |
| Transactions on the game account | 56, none failed (55 from the driver, 1 settle) |
| Rejected in simulation, not on chain | 6: five Botagul `buy` in round 5 (`NotEnoughCash`), one early lobby `advance` (`TooEarly`) |
| Final state before settle | Finished after 6 rounds, 3 laws passed, last vote 2:2; Aibot cash 87000000, Botagul cash 1000000 (game units) |

| Step | Transaction | Time | Verified on chain |
|---|---|---|---|
| Fund bot 1, 0.5 SOL | [`3DKg4jgx...veBC`](https://explorer.solana.com/tx/3DKg4jgxJvdoniPZKfrwskJjP51DYDLTsVH3LEqg9dvG9sZHhHEaxz7SFKwyT6VpXjLZbqr9ewNVRGo3PSLsveBC?cluster=devnet) | | status ok |
| Fund bot 2, 0.5 SOL | [`5Y2LYqsN...25ET`](https://explorer.solana.com/tx/5Y2LYqsNhMKcnrAGBWU9d5Q7vwLLS2RYPBLJPBavtDk5761Kp23tqbXdaUoGcNXgRYtvySCiovMsNmv4c9F125ET?cluster=devnet) | | status ok |
| Create game (`initialize`, bot 1) | [`2ENmPJVE...aeYBh`](https://explorer.solana.com/tx/2ENmPJVEciGRx3z4nnmZnMzwdT97VLqHzCmg53F8XhPgMRSFGTfQ5Gv8G2V1h8AQMLXNbscViZtkNZQVSm4aeYBh?cluster=devnet) | 18:17:54 | game account 0 to 0.00526796 SOL |
| Join, bot 1 | [`5BHcLhuS...o6niY`](https://explorer.solana.com/tx/5BHcLhuSHFgSnVTJ1XG7Xd3Yujf7aMuqkBxSVALVivVjPwF9UdWGjV9kHtyt1cz9G4VzEwUwbD3jdWYnSqvo6niY?cluster=devnet) | 18:17:57 | game account 0.00526796 to 0.05526796 SOL |
| Join, bot 2 | [`4f1PmHSZ...TKAmM`](https://explorer.solana.com/tx/4f1PmHSZBMxtodcXXevmGLrxymC1UVXrA4jCwTJ5VYBqTZapgAYQMhx4cemSQ9gsYmWe3AHZbBMs536MrGBKTAmM?cluster=devnet) | 18:17:58 | game account 0.05526796 to 0.10526796 SOL |
| Start (lobby `advance`) | [`5hTcNeZg...LK3f`](https://explorer.solana.com/tx/5hTcNeZgMDaQxumhWbqfN9NUDt92xfZyDt52eVbLeBgNdEgtxRHDnE621YWEbQzaYuBHPFKhhqv8bsKmeU7YLK3f?cluster=devnet) | | status ok |
| Last `advance`, round 6 law, game finished | [`2DwRQS1Y...w8rQ8W`](https://explorer.solana.com/tx/2DwRQS1YA9Q2g3vHSHftURfaXfZT1WD9ZoLTPFMjncrD71MqYi4fMwmLjwLbKmR183L8Tq1kJaLjK35fDrw8rQ8W?cluster=devnet) | 18:24:23 | status ok |
| Settle and payouts (crank: bot 1) | [`5szGrVyn...U7PPt`](https://explorer.solana.com/tx/5szGrVynTzy1LY2c27eeVzhgnAgpwMan3KrByfbPc9F6ZNKkUs1EKs9adjzLnsQ9Fz6XvJw5gJDQuizspKvU7PPt?cluster=devnet) | 18:26:50 | see below |

The full list of driver transactions with Explorer links is in the driver log, which was kept outside the repository.

### Settle

The settle transaction (slot 507746250, 13293 compute units, fee 0.000005 SOL) emitted these events, decoded from its program data logs:

| Event | Wallet | Rank | Amount, lamports |
|---|---|---|---|
| Payout | `7M1u5hEW5weU9EtpBtY4YXsBySGktMr4nP6wKTycLy2c` (Aibot) | 0 | 59375000 |
| Payout | `9ZJ9cCWrXHECZofvjqMqu9jC9Mc6H8YVCHp2BUybXkr4` (Botagul) | 1 | 35625000 |
| Settled | pot 100000000, rake 5000000, paid 95000000 | | |

Balance changes in the same transaction: game account 0.10526796 to 0.00526796 SOL (the remaining amount is the account's rent reserve); Aibot 0.44264212 to 0.50701212 SOL (payout plus rake as admin, minus the fee); Botagul 0.44801508 to 0.48364008 SOL. Reading the game account afterwards gives phase `Finished`, `settled: true`.

The driver did not send this settle itself. After the game finished it printed `[ERROR] discover_factions: RPC response error -32602: INVALID_PARAMS_WITH_MESSAGE` and exited without settling. `discover_factions` (`bots/src/main.rs:465`) calls `getProgramAccounts` without an encoding. The same request sent by hand without an encoding returned the same error from `api.devnet.solana.com`; with `encoding: base64` it returned both factions. Settle is permissionless, so a one-off crank kept outside the repository read the factions with base64 encoding and sent `settle` signed by bot 1, with the same account order the driver uses. The driver code was not changed.

## Balances after the run

| Wallet | Balance |
|---|---|
| Workshop wallet `61j7b8cKLZeumMQ79LDSU4E7pbZu9eTh3sauctSkKWL3` | 1.98999 SOL |
| Deploy wallet `64WA7wFVJK3GkfSAkfiAWR37bgjCw1RYcj17qEDj4AVJ` | 4.63685168 SOL |
| Bot 1 `7M1u5hEW5weU9EtpBtY4YXsBySGktMr4nP6wKTycLy2c` | 0.50701212 SOL |
| Bot 2 `9ZJ9cCWrXHECZofvjqMqu9jC9Mc6H8YVCHp2BUybXkr4` | 0.48364008 SOL |
| Game account | 0.00526796 SOL |

## Not verified

- The bot driver's own settle path on public devnet: it fails as described above and is not fixed in this change.
- The indexer was not run against devnet.
- `app/index.html` with the new ID was not opened against devnet.
- The proof scripts `tools/studio_local_proof.py` and `tools/cyber_local_proof.py` were not rerun with the new ID.
- Switchboard randomness was not used: the bank stayed under the 1 SOL threshold, so the match used slot-hash randomness.
- A third-party agent joining through `--game` was not tested on this deployment.
