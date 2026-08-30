use crate::state::{Phase, VoteChoice};
use anchor_lang::prelude::*;

#[event]
pub struct GameInitialized {
    pub game: Pubkey,
    pub game_id: u64,
    pub entry_fee: u64,
    pub phase_duration: i64,
}

#[event]
pub struct FactionJoined {
    pub game: Pubkey,
    pub faction: Pubkey,
    pub name: String,
    pub count: u8,
}

#[event]
pub struct Produced {
    pub game: Pubkey,
    pub faction: Pubkey,
    pub goods: u16,
}

#[event]
pub struct Sold {
    pub game: Pubkey,
    pub faction: Pubkey,
    pub units: u16,
    pub revenue: u64,
}

#[event]
pub struct GoodsBought {
    pub game: Pubkey,
    pub faction: Pubkey,
    pub units: u16,
    pub cost: u64,
}

#[event]
pub struct BribeGiven {
    pub game: Pubkey,
    pub from: Pubkey,
    pub to: Pubkey,
    pub amount: u64,
    pub influence_gained: u16,
}

#[event]
pub struct VoteCast {
    pub game: Pubkey,
    pub faction: Pubkey,
    pub choice: VoteChoice,
}

#[event]
pub struct PhaseAdvanced {
    pub game: Pubkey,
    pub round: u8,
    pub phase: Phase,
}

#[event]
pub struct LawResult {
    pub game: Pubkey,
    pub round: u8,
    pub yes: u32,
    pub no: u32,
    pub passed: bool,
}

#[event]
pub struct LawDrawn {
    pub game: Pubkey,
    pub round: u8,
    pub card: u8,
}

#[event]
pub struct LawVetoed {
    pub game: Pubkey,
    pub round: u8,
    pub card: u8,
}

#[event]
pub struct VetoCast {
    pub game: Pubkey,
    pub president: Pubkey,
    pub card: u8,
}

#[event]
pub struct DonkeyBought {
    pub game: Pubkey,
    pub faction: Pubkey,
    pub price: u64,
}

#[event]
pub struct Payout {
    pub game: Pubkey,
    pub wallet: Pubkey,
    pub rank: u8,
    pub amount: u64,
}

#[event]
pub struct Settled {
    pub game: Pubkey,
    pub pot: u64,
    pub rake: u64,
    pub paid: u64,
}
