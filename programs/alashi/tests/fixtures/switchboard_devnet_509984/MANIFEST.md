# Public devnet Switchboard fixtures for local LiteSVM tests

These are public program/account bytes, with no wallet, keypair, signature, or private game data. The tests load these bytes offline and assert each length and SHA-256 before use. The directory name identifies the devnet observation series; the provider ELF and account snapshots were read at different slots.

| File | Public address | Origin/context | Bytes | SHA-256 |
| --- | --- | --- | ---: | --- |
| `provider.elf` | `Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2` | Public devnet ProgramData ELF, finalized slot 509987043 | 2,108,896 | `fbb0ba2f36af9be4b16b2a94c41e8678e9ec73a638ad8a71ac3a11f1737c1451` |
| `queue.bin` | `EYiAmGSdsQTuCw413V5BzaruWuCCSDgTPtBGvLkXHbe7` | Public devnet `getMultipleAccounts`, context slot 509990988 | 6,280 | `261a50cdad3f2b35e81cdddf5da638e36f02fbcd7df90de5705a8fdc4f6a578a` |
| `state.bin` | `4UFmCebEmzESoDTtHrmaftXj7YAsAH4HMios3yMWyVUT` | Same public devnet account response | 2,136 | `515b99f11aac13cfc21e4808722b6320445069070182eac508b283819209aa4e` |
| `oracle.bin` | `Hdu1niJgqVGhesoxgy37p6WBunVDoBacZJZVK7VRRevg` | Same public devnet account response | 4,816 | `47ae6d6d33f1ad6da6b20a7154c30803a2599a37f9aae9b18b45f20817c83616` |
| `native_mint.bin` | `So11111111111111111111111111111111111111112` | Same public devnet account response | 82 | `b0f919425a529f4971ef8dabd535aec34947d5c8c4885df80dae91635b98c5f3` |

The provider account is owned by Solana's upgradeable loader; the first four data accounts above are provider-owned except `native_mint.bin`, which is Token Program-owned. The test fixes a synthetic Clock/SlotHashes and never submits a transaction to devnet. A successful local provider Init/Commit CPI is **not** a live oracle roundtrip or approval to deploy epoch 2.
