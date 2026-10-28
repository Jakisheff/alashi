# Solana account decoder patch

These directories contain the `solana-account-decoder` and
`solana-account-decoder-client-types` 3.1.14 crates from Anza Agave commit
`3134055b562e95902233be308453fffa1c4a8902`, licensed under Apache-2.0.
Their Rust sources are unchanged. Both manifests use zstd 0.14.3 to address
RUSTSEC-2026-0391, RUSTSEC-2026-0398, RUSTSEC-2026-0403 and
RUSTSEC-2026-0410. The decoder manifest pins solana-rent 3.0.0 because its
3.1 fields are deprecated and the decoder denies warnings. The client-types
manifest pins solana-account 3.2.0, where the decoder’s account constructor
is available without a deprecation warning. Bots enable the five8 `std`
feature required by solana-keypair under that dependency graph.

Remove these patches when the project can upgrade to a compatible Solana
release that includes the zstd fix. Test compressed RPC account round trips
and audit both indexer and bots lockfiles when changing the patch.
