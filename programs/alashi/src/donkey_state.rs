use anchor_lang::prelude::*;

/// Epoch-2-only account. The six entropy account identities and their queue/
/// authority are fixed before Lobby exits; tickets are mutable per round.
#[account]
#[derive(InitSpace)]
pub struct DonkeyOracleConfig {
    pub game: Pubkey,
    pub queue: Pubkey,
    pub authority: Pubkey,
    pub accounts: [Pubkey; 6],
    pub round: u8,
    pub ticket_count: u8,
    pub tickets: [Pubkey; 6],
    pub max_attempt_slot: u64,
    pub resolved: bool,
    pub bump: u8,
}
