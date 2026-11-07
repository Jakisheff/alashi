# Independent review: terminal archive retry

Observed, 7 November 2026: no source blocker found in finding 2. This is a read-only review, not a fresh test result. No source edits, builds, runtime calls, pushes or merges were performed. The parent must still obtain fresh RED/GREEN evidence and the required project checks.

Reviewed commit: `f32168384dff8a8b0d30287605a5f50ab6c44c19`. Original repair checkout HEAD is `3ea380b5`; its `indexer/src/chain_api.rs` blob equals f321 exactly: `52cd27973bb89e3be089002a8caf60f12a5b79a7`. Current main `4cdfa4cb20ee6e6d7cd33f849884f062caa39881` has blob `0bb3edf9f7fd9c763a9ef00722012758b3fbdfbc`, identical to f321's parent. Thus the production delta against current main is still the original two hunks, plus the regression test.

The client result is narrow: a complete settled game whose first archive write failed can become archived on a later read using the same resident cached snapshot, after storage recovers. No restart, eviction or additional RPC is required. The close-settled caller continues rejecting responses without `archived: true`; its game rules and close instruction are unchanged.

## Control flow and serialization

| Location in reviewed file | Assessment |
| --- | --- |
| `indexer/src/chain_api.rs:128-134`, `ready_value` | Fresh or stale terminal entries without `archived: true` leave the fast path only when an archive directory is configured. Fresh live entries, archived terminal entries and disabled archival retain their previous behavior. A valid on-disk archive remains usable. |
| `indexer/src/chain_api.rs:161-184`, `get` | PDA parsing and canonical validation occur before any filesystem path. The retry is inside the existing per-game gate and after a second cache read. It calls `archive`, updates the cache and returns before `fetch`, so it does not require RPC or a live onchain account. |
| `indexer/src/chain_api.rs:135-145`, `game_gate` | The weak-reference map hands overlapping same-PDA requests the same mutex while either request retains its Arc. There is no new lock namespace or lock ordering. |
| `indexer/src/chain_api.rs:179-183` | The cloned cache entry retains its age and seen-signature set. Only a successful archive adds `archived: true`. A second request rechecks the updated cache after the first releases the gate and returns through the archived fast path. |
| `indexer/src/chain_api.rs:121-127,146-159` | The global cache mutex is held for clone/insert operations, not filesystem writes or RPC. An unrelated cached game does not wait for a terminal game's archive operation. Existing 32-entry eviction policy is unchanged. |
| `indexer/src/chain_api.rs:204-260` | Archive loading validates game identity, cluster/program, terminal/complete flags, event count and size. Saving creates/truncates the per-PDA temporary file, writes, syncs the file and renames. `archived` is set only if this sequence succeeds. These storage functions are unchanged. |
| `indexer/src/bin/chain_api.rs:61-84,110-143` | HTTP GET calls `api.get` on the single shared Arc. The retry remains serialized within that API instance; there are at most eight normal HTTP workers. |
| `bots/src/close_settled.rs:45-63` | The close decision still requires matching terminal/complete data and `archived: true`. Failed retries keep the previous snapshot unmarked, so they do not bypass this guard. |

A read of an archive can occur before acquiring the game gate. This is safe within the existing write protocol: readers use the final path, while writers publish by rename. The change adds no unlocked writer. Repeated failed writes now occur on subsequent GETs, as requested; there is no retry loop or background worker.

## Existing regression and fresh RED recipe

The test at `indexer/src/chain_api.rs:909-983` is behavioral. It creates a directory at the final archive-file path, forcing rename to fail after writing the temporary file. It seeds a terminal cache entry, confirms a read while storage is still obstructed, removes the obstruction, and makes two concurrent reads of that same API instance. It asserts successful archival, matching results, restart loading, an untouched sentinel on subsequent archived-cache access and no accepted RPC connections. It runs both age zero and age sixty seconds. A live-cache control is also present.

The historical logs show runtime RED failures (`archived` was Null rather than true) and a passing GREEN test. They are historical evidence only. The earlier denied-loopback environment failure is correctly excluded in the existing README. The source test does not depend on a new production helper: `archive`, `remember`, `load_archive`, `Cached` and all module imports already exist on current main.

For fresh evidence in the parent's isolated checkout:

1. Start from current main and transplant only this regression test, leaving both production hunks absent. Alternatively, retain the fixed test and reverse only the two production hunks. Do not revert the entire test-bearing file.
2. Run `cargo test --locked --manifest-path indexer/Cargo.toml --lib chain_api::tests::cached_terminal_game_retries_archive_after_storage_recovers_without_rpc -- --exact` using the parent's isolated target/cache and serialized Cargo policy.
3. Accept RED only if it reaches the explicit `cached terminal game must retry its failed archive` assertion with Null versus true. A compile error, bind denial, RPC timeout or missing helper is not this reproduction.
4. The first failure stops the loop, so an ordinary RED run demonstrates the fresh-cache case. If separate stale RED evidence is wanted, run a test-only variant with the age list containing only sixty seconds. Restore both ages for GREEN.
5. Restore the two minimal production hunks and run the same target, then the mandatory indexer/project checks. Keep the same test logic in RED and GREEN. No backend binary or live game is needed.

The fixture uses a unique temporary path and loopback port, overrides archive/RPC settings and disables proxies on its local client. A failing RED assertion occurs before its final cleanup; retain or remove only that run's owned fixture directory, without sweeping unrelated temporary state.

## Limits and optional additional coverage

No mandatory missing case was found for the reported resident-cache retry. These additional tests could improve coverage without changing behavior:

- A malformed or truncated final archive alongside a complete resident cache, then verify the retry replaces it and remains RPC-free.
- Direct terminal-cache reads with archival disabled; the source branch is correct, but the added regression does not explicitly assert that configuration's fast path.
- A deterministic held-gate test that blocks one game's archive attempt while confirming another cached game remains available. The current two-thread barrier tests concurrent outcomes but does not count writes or force overlap. Existing refresh serialization tests and source lock placement supplement it.

Existing boundaries remain: serialization is per API instance, not cross-process; persistent storage failure causes another serialized write attempt on each qualifying GET; an already archived cache entry remains available if its disk file later disappears, as explicitly tested and requested. No storage backoff, cross-process lock or archive-loss repair was added.

Power-loss durability is not tested. The unchanged writer syncs the temporary file before rename but has no parent-directory sync call. This review does not upgrade that pre-existing durability claim. None of these limits changes findings 3, 5 or 8, and they should not be folded into this minimal fix without a separate decision.
