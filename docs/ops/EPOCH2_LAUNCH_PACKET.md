# Epoch 2 devnet release record and next gate

Status: **program and API deployed; one bounded Game technically aborted and refunded; verified donkey draw still unproven.** This replaces the obsolete 530,256-byte program, 500-byte config, external Commit/Reveal and `Clock.slot + 1` instructions previously in this file. Git history retains that earlier plan. Do not reuse its budget or commands.

## Deployed bytes

| Item | Verified value |
| --- | --- |
| Network | Solana devnet, genesis `EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG` |
| Alashi program | `3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC` |
| ProgramData | `HcGSeuzKQVdCTM3H4zYhPRuYzsubSLHCqgoTewpdVxTp`; finalized deploy slot `510023387` |
| Deployed ELF | 603,400 bytes, SHA-256 `4b197c36bc6b3913a52141b767a92cb6e3255aff071351a7a2ad629392becee4` |
| Local rollback ELF | 464,392 bytes, SHA-256 `de492e03624449a36e10a9267563ed46bbaf4eb26204f60306f2c5e10020cde2` |
| Upgrade authority | `64WA7wFVJK3GkfSAkfiAWR37bgjCw1RYcj17qEDj4AVJ`; matching local signer was verified and used for this authorized devnet upgrade |
| Current chain API | `/opt/alashi-chain-api/chain_api`, Linux amd64, 5,549,448 bytes, SHA-256 `70d1ef6afc5a54dcd2a5b0b5b45d71d04c2ad280cc4bc7845d4187f1d76ba4ad` |
| Immutable API rollback | `/opt/alashi-chain-api/releases/chain_api-beeda185`, 5,553,864 bytes, SHA-256 `beeda1857216ca530166431cb5770e300baa54e5edb249adcfc24a1687b27a77` |

The public upgrade receipt records exact ProgramData bytes, unchanged authority, closed upload buffer and signature `3MnVW3G5toxSKfnifVn7XxAD2VspH8fLWePwUjVdvj2ewEA1PKPYAekMxj4RYyhytEKUSVLK9VR6wTPmhWsxoSA`. Its task-owned local artifact is `epoch2-devnet-program-upgrade-public-receipt.json`; no private key or signer path belongs in this document or on the server.

The API hotfix filters Faction discovery by discriminator and Game. This excludes the 693-byte `DonkeyOracleConfig`, which also contains the Game key at offset 8. The historical [Game replay](https://alashi.network/chain/devnet/games/GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b) remained HTTP 200 with 72 events and byte-identical 29,704-byte JSON during the switch. The [epoch-2 Game snapshot](https://alashi.network/chain/devnet/games/2gcsxYV6jA9o3mD667mdjYmgMvBmVbVgTaXmbKKcUaZW) is now HTTP 200, `history_complete:true`, `Aborted`, `settled:true`; a final GET on 2026-10-11 was 7,507 bytes, SHA-256 `efa5a5b81e558c02a648f2a9e93ec9e8bc12421cd1f3f1f156237c5000059e5c`. Its `fetched_at` field makes later response hashes differ.

## Actual bounded Game

The only Game was `2gcsxYV6jA9o3mD667mdjYmgMvBmVbVgTaXmbKKcUaZW` (ID `1791752225`), epoch 2, two Factions, 20-second phases and a **chosen** 50,000,000-lamport entry deposit per player. Config PDA `FP15iJv8KAFvWusD82Dc9mg8BytJ2USeyJMdQqpZtXMo` pins six source PDAs and six oracles under queue `EYiAmGSdsQTuCw413V5BzaruWuCCSDgTPtBGvLkXHbe7`. Task-owned public setup manifest `epoch2-one-game-public-setup.json` is 4,924 bytes, SHA-256 `20cea4f653f4422d4a824d850061a9b147912bd2dda95d2b3735dd7bf06a07b4`. Independent pre-Join verification found six pristine provider-owned sources, exact config order and no Factions before the two joins.

Alashi `Advance` committed round 1 through a provider CPI, signature `3WpE2oioo9TpQexe2DDuAxCNMw8GToV4pLm78JW4FTye5GzsbRH1G2Q69eARfv7ftoWzVXMZWihoV2WnHNdFGzzY`, slot `510039067`. The provider set `seed_slot = 510039066 = Clock.slot - 1`. This is the deployed Alashi-owned protocol: preinitialized source PDA and config-PDA authority, one guarded Commit CPI in `Advance`, then a single Alashi `RevealDonkeyLaw` CPI with pinned source/oracle and ordered ticket Factions. Operators must not submit external provider Commit or Reveal instructions.

Exactly **one** bounded gateway proof POST returned the helper's generic `gateway_proof_timeout_or_unavailable`. The helper did not retain elapsed time, HTTP status or DNS/connect/TLS stage, so the precise failure category is unknown. It wrote no proof file; there was no `RevealDonkeyLaw`, `DonkeyAttempted`, `DonkeyResolved`, verified random value or 60/40 outcome. Stale queue/oracle heartbeat metadata did not prevent the observed Commit, but it did not establish gateway availability. A focused diagnostic and unrevealed-Law runner guard are in progress. Do not infer a 40% empty outcome from this technical failure.

The Game followed fixed Law retries to `game_aborted`, then permissionless `SettleRefund` signature `Lma4ZjPNjsGtQ88F3yryKX1C6hQYhNizJN1FWXGxXfDEu8bBwtAah9XV2CT8GsfrsEVvmGUwVjev2woArkC4nc6`, slot `510040167`, `meta.err:null`, returned exactly 50,000,000 lamports to each player. No second Game, source swap, recommit, second gateway POST or new funding followed.

Task-owned cost receipt `epoch2-one-game-final-chain-costs.json`, 6,311 bytes, SHA-256 `afba247b770f4d77d5d29390eb2af11f73d668e1b8416f5dd8a105f8bca12e41`, enumerates all 20 landed Game transactions, all `meta.err:null` and 5,000-lamport fees: **100,000 lamports actual Game base fees**. Sixteen rejected vote signatures have null chain status and are not charged in this accounting. Both 50,000,000-lamport entry deposits were returned. Accounted retained rent is 49,235,360 lamports: Game 5,267,960; config 4,170,680; two Factions 3,799,840; six sources 18,531,840; six source escrows 8,930,640; six LUTs 8,534,400. Thus the Game's net role-wallet debit is 49,335,360 lamports, retained rent plus fees. This is observed setup/balance accounting, not a price for a future successful provider reveal.

The selected task wallets received 0.31 testSOL of internal funding before Game creation (0.25 for crank, 0.06 for opponent); that transfer is not a consumed Game cost. Registration Memo cost 5,000 lamports, two funding transactions cost 5,000 each, and the separate program upgrade accounted for 706,160,640 lamports added ProgramData rent plus 3,160,000 lamports signature fees. These scopes are distinct from provider pricing. No live reveal fee or gateway/provider charge was observed; future cost is **unknown, not zero**.

## Next release gate

1. Merge and release the focused unrevealed-Law runner guard and sanitized one-request gateway diagnostics only after exact-source review, local tests and required CI. The runner must not send Law votes before a revealed card; source-free Law timeout/abort/refund must remain available. Preserve this Game and its technical-abort receipt as evidence; do not retry it or present it as a successful playtest.
2. Before any **new** Game, verify fresh devnet genesis, deployed ProgramData/API hashes, queue and six selected oracle accounts, provider state, source/config ownership, task payer balances and a bounded provider cost/fee cap. Public account membership or a successful unsigned Commit simulation is not gateway proof. Freeze sources/oracles before Join; do not swap them after bets.
3. For a separately bounded acceptance Game, prove a confirmed `DonkeyAttempted` after cash bootstrap, Alashi Commit CPI with seed strictly after the latest attempt, provider-backed `RevealDonkeyLaw`, exact `DonkeyResolved` win/empty receipt, and terminal settle/refund. If proof is unavailable, use the distinct technical abort path. Publish signatures, slots, account deltas, rent and actual fees separately. This needs its own fresh quote; the old `epoch2_launch_preflight.cjs` remains a read-only **fixed-only/unavailable** helper based on the earlier candidate, not a current all-in launch approval.

The [crank runbook](DONKEY_EPOCH2_CRANK.md) describes the Alashi-owned CPI account protocol and fail-closed worker. Its source and mocked tests do not prove live gateway reveal. Frontend historical replay remains available, but no public UI label should claim a verified epoch-2 lottery result for this aborted Game.
