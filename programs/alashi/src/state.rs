use anchor_lang::prelude::*;

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub enum Phase {
    Lobby,
    Market,
    Action,
    Law,
    Finished,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq, InitSpace)]
pub enum VoteChoice {
    Yes,
    No,
    Abstain,
}

#[account]
#[derive(InitSpace)]
pub struct Game {
    pub admin: Pubkey,
    pub game_id: u64,
    pub phase: Phase,
    pub round: u8,
    pub phase_ends_at: i64,
    pub faction_count: u8,
    pub laws_passed: u16,
    pub sold_this_round: u16,
    pub entry_fee: u64,
    pub rake_bps: u16,
    pub phase_duration: i64,
    pub last_law_passed: bool,
    pub yes_influence: u32,
    pub no_influence: u32,
    pub bump: u8,
}

impl Game {
    pub fn stamp(&self) -> u16 {
        ((self.round as u16) << 3) | self.phase as u16
    }
}

#[account]
#[derive(InitSpace)]
pub struct Faction {
    pub game: Pubkey,
    pub wallet: Pubkey,
    #[max_len(16)]
    pub name: String,
    pub cash: u64,
    pub goods: u16,
    pub influence: u16,
    pub acted_stamp: u16,
    pub voted_stamp: u16,
    pub vote: VoteChoice,
    pub is_president: bool,
    pub alive: bool,
    pub bump: u8,
}
