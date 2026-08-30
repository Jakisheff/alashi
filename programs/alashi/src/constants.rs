use anchor_lang::prelude::*;

#[constant]
pub const GAME_SEED: &[u8] = b"game";

#[constant]
pub const FACTION_SEED: &[u8] = b"faction";

pub const MIN_FACTIONS: u8 = 2;
pub const MAX_FACTIONS: u8 = 5;
pub const ROUNDS: u8 = 6;
pub const LOBBY_MULT: i64 = 5;
pub const PESO: u64 = 1_000_000;
pub const DEFAULT_RAKE_BPS: u16 = 500;
pub const PRODUCE_YIELD: u16 = 2;
pub const BRIBE_PRICE: u64 = 5 * PESO;
pub const MAX_INFLUENCE: u16 = 1_000;
pub const MAX_NAME: usize = 16;

#[constant]
pub const PRICE_TABLE: [u64; 16] = [12, 10, 9, 8, 7, 6, 5, 4, 3, 3, 2, 2, 2, 1, 1, 1];

pub fn price_at(sold: u16) -> u64 {
    PRICE_TABLE[(sold as usize).min(PRICE_TABLE.len() - 1)]
}
