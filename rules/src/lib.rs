//! alashi-rules: единственный источник истины правил игры.
//! Ончейн-программа и off-chain симулятор зависят от этого крейта;
//! расхождение правил между ними невозможно по построению
//! (ARCH_TRAINING_CAMP.md, слой L1).

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
pub use anchor_lang;
