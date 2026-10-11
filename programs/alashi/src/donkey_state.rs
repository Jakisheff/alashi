use anchor_lang::prelude::*;
use switchboard_on_demand::accounts::OracleAccountData;

const ORACLE_DISCRIMINATOR: [u8; 8] = [128, 30, 16, 241, 170, 73, 55, 54];

/// Read the queue from the pinned provider's repr(C) oracle layout. Its
/// AccountInfo type comes from a different Solana crate version, so the SDK's
/// `new` helper cannot accept Anchor's AccountInfo. Check the exact provider
/// discriminator and size before reading the field offset.
pub fn oracle_queue(ai: &AccountInfo) -> Result<Pubkey> {
    let data = ai.try_borrow_data()?;
    let start = 8 + core::mem::offset_of!(OracleAccountData, queue);
    require!(
        data.len() >= 8 + core::mem::size_of::<OracleAccountData>()
            && data[..8] == ORACLE_DISCRIMINATOR,
        crate::error::GameError::DonkeyOracleInvalid
    );
    let bytes: [u8; 32] = data[start..start + 32]
        .try_into()
        .map_err(|_| crate::error::GameError::DonkeyOracleInvalid)?;
    Ok(Pubkey::new_from_array(bytes))
}

/// Provider-owned randomness account, uniquely derived for one game round.
pub const DONKEY_SOURCE_SEED: &[u8] = b"donkey-source";

/// Epoch-2-only account. The six entropy account identities and their queue/
/// authority are fixed before Lobby exits; tickets are mutable per round.
#[account]
#[derive(InitSpace)]
pub struct DonkeyOracleConfig {
    pub game: Pubkey,
    pub queue: Pubkey,
    pub authority: Pubkey,
    pub accounts: [Pubkey; 6],
    /// Chosen before any faction joins; a caller cannot choose an oracle
    /// after learning which factions paid for a donkey attempt.
    pub oracles: [Pubkey; 6],
    /// A source can be committed once for its round, even if the provider
    /// accepts another commit invocation for that account.
    pub committed_rounds_mask: u8,
    pub round: u8,
    pub ticket_count: u8,
    pub tickets: [Pubkey; 6],
    pub max_attempt_slot: u64,
    pub resolved: bool,
    pub bump: u8,
}
