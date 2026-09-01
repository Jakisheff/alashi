use anchor_lang::prelude::*;

anchor_lang::declare_id!("8EikcWzM7d3EjttApmymo2maWp5A3NtMpKoYdWUzzzL");

#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq, InitSpace,
)]
pub enum Phase {
    #[default]
    Lobby,
    Market,
    Action,
    Law,
    Finished,
    Aborted,
}

#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq, InitSpace,
)]
pub enum VoteChoice {
    #[default]
    Yes,
    No,
    Abstain,
}

#[account]
#[derive(InitSpace, Default)]
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
    pub law_card: u8,
    pub laws_used_mask: u8,
    pub veto_pending: bool,
    pub president: Pubkey,
    pub active_tax_bps: u16,
    pub active_subsidy_goods: u8,
    pub active_price_shift: i8,
    pub active_boom: u8,
    pub pending_tax_bps: u16,
    pub pending_subsidy_goods: u8,
    pub pending_price_shift: i8,
    pub pending_boom: u8,
    pub settled: bool,
    pub entropy_mode: u8,
    pub vote_weight_mode: u8,
    pub vrf_account: Pubkey,
    pub commit_slot: u64,
    pub vrf_retries: u8,
    pub vrf_spent: u64,
}

impl Game {
    pub fn stamp(&self) -> u16 {
        ((self.round as u16) << 3) | self.phase as u16
    }
}

#[account]
#[derive(InitSpace, Default)]
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
