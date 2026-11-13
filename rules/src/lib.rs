//! alashi-rules: единственный источник истины правил игры.
//! Ончейн-программа и off-chain симулятор зависят от этого крейта;
//! расхождение правил между ними невозможно по построению
//! (ARCH_TRAINING_CAMP.md, слой L1).

pub mod actions;
pub mod constants;
pub mod error;
pub mod logic;
pub mod sim;
pub mod state;
pub mod transitions;

pub use logic::*;
pub use state::*;

/// Реэкспорт для downstream-крейтов (arena, indexer): Pubkey без
/// прямой зависимости от anchor-lang.
#[cfg(feature = "chain")]
pub use anchor_lang;

#[cfg(feature = "chain")]
pub use anchor_lang::prelude::Pubkey;

/// Host-сборка без anchor: локальный Pubkey с теми же байтами и
/// семантикой равенства, что у solana Pubkey; Display в hex, не base58.
#[cfg(not(feature = "chain"))]
pub mod host_id {
    use core::fmt;
    use core::sync::atomic::{AtomicU64, Ordering};

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
    pub struct Pubkey(pub [u8; 32]);

    impl Pubkey {
        pub const fn new_from_array(a: [u8; 32]) -> Self {
            Self(a)
        }

        pub fn new_unique() -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let mut b = [0u8; 32];
            b[24..].copy_from_slice(&n.to_be_bytes());
            Self(b)
        }
    }

    impl AsRef<[u8]> for Pubkey {
        fn as_ref(&self) -> &[u8] {
            &self.0
        }
    }

    impl From<[u8; 32]> for Pubkey {
        fn from(a: [u8; 32]) -> Self {
            Self(a)
        }
    }

    impl borsh::ser::BorshSerialize for Pubkey {
        fn serialize<W: borsh::io::Write>(&self, writer: &mut W) -> borsh::io::Result<()> {
            use borsh::io::Write;
            writer.write_all(&self.0)
        }
    }

    impl borsh::de::BorshDeserialize for Pubkey {
        fn deserialize_reader<R: borsh::io::Read>(reader: &mut R) -> borsh::io::Result<Self> {
            use borsh::io::Read;
            let mut a = [0u8; 32];
            reader.read_exact(&mut a)?;
            Ok(Self(a))
        }
    }

    impl fmt::Display for Pubkey {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for byte in self.0.iter() {
                write!(f, "{:02x}", byte)?;
            }
            Ok(())
        }
    }
}

#[cfg(not(feature = "chain"))]
pub use host_id::Pubkey;
