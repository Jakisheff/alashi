# Epoch 2 devnet launch packet — preparation only

Status on 2026-10-11: source for epoch-2 donkey risk is merged; the upgraded Solana program, refreshed chain API, six real Switchboard sources, and a provider roundtrip are **not** deployed or proven. This packet does not authorize a transaction. The existing frontend can show old confirmed replay and candidate epoch-2 event types; it cannot prove a live epoch-2 Game.

## Immutable identities and local artifacts

| Item | Verified value |
| --- | --- |
| Devnet genesis | `EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG` |
| Alashi program | `3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC` |
| ProgramData | `HcGSeuzKQVdCTM3H4zYhPRuYzsubSLHCqgoTewpdVxTp`, current deployed slot `507743508` |
| Upgrade authority | `64WA7wFVJK3GkfSAkfiAWR37bgjCw1RYcj17qEDj4AVJ` (public authority verified; human key holder **unknown**) |
| Candidate program | `530256` bytes, SHA-256 `31117a3084318f5d9c8dbc3c3f291168545e474728d981f3089a7554c698978d` |
| Rollback program | `464392` bytes, SHA-256 `de492e03624449a36e10a9267563ed46bbaf4eb26204f60306f2c5e10020cde2` |
| Source | main `ce848cfb1c736d81cb7ab95bc992d2c66e30c8e7`, PR27 epoch-2 code merged earlier |
| Switchboard SDK | `@switchboard-xyz/on-demand` `3.10.6`, `spike/package-lock.json` SHA-256 `630381a34b75758075c3623f628897968751604b23dabe1857c8c8c73f3fc23c` |

The candidate and rollback `.so` are kept on the local Mac outside Git under the task-owned donkey build/rollback directories. Recheck their hashes and compare the finalized deployed ProgramData executable bytes to the rollback hash before any upgrade. Server-side key files are forbidden. The current checked-in crank tool uses the pinned SDK but its synthetic tests do not prove a real provider response.

## Budget for exactly one selected two-player Game

Fresh finalized devnet rent queries supplied these lamport amounts; rerun the read-only helper immediately before planning a transaction. `ENTRY_FEE=50,000,000` per player is this bounded bot-host plan, not a universal required stake.

| Category | Lamports | testSOL | Treatment |
| --- | ---: | ---: | --- |
| ProgramData extension, 464437→530301 bytes | 334589120 | 0.334589120 | Rent locked in upgraded account; recoverability depends on program lifecycle |
| Six 408-byte randomness sources | 16337280 | 0.016337280 | Source account rent; recovery only after explicit validated close |
| Game, 909 bytes | 5267960 | 0.005267960 | Game rent locked |
| Two Faction accounts, 246 bytes each | 3799840 | 0.003799840 | Faction rent locked |
| Oracle config, 500 bytes | 3190240 | 0.003190240 | Config rent locked |
| Two chosen entry deposits | 100000000 | 0.100000000 | Game escrow, distributed/refunded per verified settlement/abort; not a fee quote |
| **Fixed subtotal** | **463184440** | **0.463184440** | Across roles before provider, LUT, buffer, transaction fees |

A separate `530293`-byte upgrade buffer needs up to `2694538680` lamports rent while it coexists. The [Solana loader-v3 upgrade reference](https://solana.com/docs/core/programs/program-deployment) says a successful `Upgrade` drains/truncates the buffer. An interrupted CLI deployment may leave a funded buffer, so confirm the actual recipient/balance before reusing that liquidity.

| Ordered stage | Fixed lamports outstanding, excluding provider/LUT/network fees |
| --- | ---: |
| Upgrade, before successful buffer drain: buffer rent + ProgramData extension | `3029127800` (3.029127800 testSOL) |
| Post-upgrade, after confirmed drain and one selected Game setup: ProgramData extension + six sources + Game/Factions/config + entries | `463184440` (0.463184440 testSOL) |
| Conservative overlap if setup proceeds before buffer recovery | `3157723120` (3.157723120 testSOL) |

For the **specified sequential order**, the larger fixed stage is 3.029127800 testSOL, subject to actual buffer refund/recipient and separate wallets. The 3.157723120 figure is a conservative overlap cap, not the necessary peak. Do not add a provider fee cap to a provider prefund twice if the fee is paid out of that prefund; the required wallet peak is `max(prefund, fee cap)` plus other independent reserves. The aggregate must be apportioned to the actual fee payer, source authority, and two player wallets after their public IDs are supplied. There are no confirmed task-owned public wallet IDs yet, so no balance/deficit claim is possible.

**Unpriced categories:** six source creation message fees, commit/reveal/abort/game/upgrade write transaction fees and priority fee caps, provider request/escrow/prefund, optional LUT creation/extension/close rent and fees, exact upgrade write chunk count. These vary with unsigned messages, provider quote, chosen LUT, compute limit and operator wallet layout. The Switchboard quote helper must report each as measured or `unavailable`, never zero by assumption. A full all-in figure and wallet-specific minimum cannot be honestly stated until these public inputs and the pinned SDK/provider compatibility gate are resolved. See [Solana rent RPC](https://solana.com/docs/rpc/http/getminimumbalanceforrentexemption) and [unsigned message fee RPC](https://solana.com/docs/rpc/http/getfeeformessage).

Run only this **read-only** budget check on a Mac with the two verified local `.so` artifacts; an input JSON has `schema`, `rpc_url`, `program_id`, `expected_authority`, `binary_path`, `rollback_path`, and optionally public wallet IDs and a separately verified Switchboard quote. Do not put a credential in the RPC URL.

```sh
node tools/epoch2_launch_preflight.cjs --input /path/to/public-epoch2-preflight.json
node --test tools/epoch2_launch_preflight.test.cjs
```

The preflight has no execute mode and refuses a wrong genesis, program, current authority, deployed binary hash/length, candidate/rollback artifact hash, unsafe RPC URL, or mismatched/stale complete provider quote. Output `fixed_only` is a budget subtotal, never release readiness. A complete quote must bind the same program, source, rollback, SDK lock, finalized slot and short observation time, with explicit transaction/signature/retry/bootstrap/compute/priority caps. Its own account creation message fees are not a complete life-cycle fee estimate.

## Order of operations — human-local execution only after separate reviewed plan

1. **Provider compatibility first.** On a clean Mac install exactly `spike/package-lock.json` with `npm ci --prefix spike --ignore-scripts`. Compare the actual installed 3.10.6 `Randomness.commitIx` semantics against the epoch-2 atomic `commitIx` then Alashi `Advance` requirement: the configured source must commit for transaction slot +1 and stay unrevealed. A tutorial's previous-slot read pattern is not proof of this write path. Get the default devnet queue, provider authority policy, source size, SDK-built unsigned create messages and exact fee/escrow quote. If the live provider differs, stop before upgrading/funding a Game. No provider request is authorized by this packet.
2. **Prepare sources without spending yet.** Select six distinct public Switchboard source IDs, one for each round, under a known local human authority and queue. The checked-in `spike/vrf_spike.js` sends real create/commit/reveal transactions and must **not** be used as a budgeting or preparation command. The reviewed Switchboard quote helper will print unsigned SDK-built create messages and account metas; humans then review its exact source, fee payer, compute/LUT needs and capped cost before separately executing six account creations. Source accounts must be owned by the expected devnet Switchboard program, with matching queue/authority, uncommitted and unrevealed at Game configuration.
3. **Program and API release preparation.** Recheck finalized ProgramData address/authority/hash, backup rollback `.so`, verify local candidate hash, free disk and the signed release plan. Build `indexer` `chain_api` on a spacious local builder from the exact merged source and save its binary SHA and current `/opt/alashi-chain-api/chain_api` SHA as rollback. The indexer decoder changed for `DonkeyAttempted`/`DonkeyResolved`; the current server API binary predates those events. Stage/restart `alashi-chain-api.service` only after a reviewed artifact/rollback and observe public anonymous Game event projection. `arena` is the HTTP simulator and does not create epoch-2 games; do not enable an HTTP epoch-2 path. Server has <1 GiB free; no server build.
4. **Human-local program upgrade.** Current team rule: “Ключи кошельков Solana, в том числе devnet, на сервер не класть и в репо не коммитить. Деплой программы и транзакции с ключами делают люди по договорённости Амира и Ивана.” Only the confirmed local holder of `64WA7wFVJK3GkfSAkfiAWR37bgjCw1RYcj17qEDj4AVJ` may sign. The specific person is not established; addressed request `EPOCH2-UPGRADE-OPERATOR-20261011-01` is pending. Never send a key or its path in the handoff. Read-only checks:

   ```sh
   solana --url devnet program show 3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC
   shasum -a 256 /local/verified/alashi-epoch2.so /local/verified/alashi-before-epoch2.so
   ```

   After the provider gate, exact wallet budget, buffer plan and human operator agreement, the reviewed local command shape is `solana program deploy --url devnet --program-id 3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC --upgrade-authority "$LOCAL_AUTHORITY_SIGNER" --fee-payer "$LOCAL_FEE_PAYER" --buffer "$LOCAL_BUFFER_SIGNER" --max-len 530256 --with-compute-unit-price 0 --max-sign-attempts 1 /local/verified/alashi-epoch2.so`. This is **not** to be run from this packet. It requires a locally created/reviewed buffer and human-local signers. Recheck finalized ProgramData hash and slot after execution. Rollback uses the same reviewed authority/program/fee-payer/buffer flow with the verified old `.so`; a rollback transaction also needs separate signed review and post-check. Never use `--skip-preflight` or `--final`.
5. **One bounded devnet acceptance Game only after the above gates.** Prepare the two already registered/local task-owned player identities, choose `--phase-duration 20 --timeout 720 --no-llm --epoch2-donkey-risk --external-vrf-crank --donkey-randomness <six comma-separated public IDs>` with the existing self-host profile/key/opponent flags. The operator config binds the exact Game, ordered Faction PDAs, six immutable source IDs, queue/authority and optional verified LUT. Run `node tools/donkey_crank.cjs inspect` first. The eventual bounded `worker --execute --max-seconds 720` must use local signer storage and its durable private sidecar, in parallel with the one finite bot host; do not put keys/grants in argv/logs. Prove on chain one paid attempt, same configured source commit after attempt, verified reveal, exact win/empty receipt or distinct technical abort, final settlement/refund. No second Game, paid model or automatic provider retry. Stop on insufficient funds/quote, failed signed receipt, incompatible source, or unverified program/API versions.

## Rollback and evidence

Keep the old program bytes (`de492e03…cde2`) and current API binary as separate artifacts. Preserve the active frontend release; no frontend deployment is needed for this operator packet. Before upgrade record finalized ProgramData hash, authority, balance/size, source IDs/queue/authority and the provider/fee quote slot. After any real future upgrade compare finalized deployed bytes to candidate `31117a30…978d`, independently confirm program ID and authority, API version/binary hash, configured Game epoch/source list, and each confirmed signature with `meta.err == null`. A lost RPC answer reuses only the same persisted signed bytes while blockhash valid; a confirmed failure or expired unknown signature stops for manual exact-signature/on-chain reconciliation. If provider does not commit before Action deadline + 30 seconds, only the permissionless technical abort may progress; do not call that a 40% empty draw. Record locked rent, actual fees/provider debit and refunds separately. No live epoch-2 claim before a real verified roundtrip and one settled bounded Game.
