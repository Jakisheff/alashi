use crate::{constants::*, error::GameError, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CloseGame<'info> {
    pub crank: Signer<'info>,
    #[account(
        mut, close = admin,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()], bump = game.bump,
        constraint = game.settled @ GameError::NotFinished,
        constraint = matches!(game.phase, Phase::Finished | Phase::Aborted) @ GameError::NotFinished
    )]
    pub game: Account<'info, Game>,
    /// CHECK: receives only the remaining game balance, at the recorded payer address.
    #[account(mut, address = game.admin)]
    pub admin: UncheckedAccount<'info>,
}

/// All factions must close together, before the parent game is removed.
/// Remaining accounts: unique writable factions, then their writable wallets.
pub fn handle_close_game<'a>(ctx: Context<'a, CloseGame<'a>>) -> Result<()> {
    let count = ctx.accounts.game.faction_count as usize;
    let rem = ctx.remaining_accounts;
    require!(rem.len() == count * 2, GameError::InvalidFactionSet);
    let factions: Vec<Account<'_, Faction>> = rem[..count].iter()
        .map(Account::<Faction>::try_from)
        .collect::<Result<_>>()?;
    for (i, faction) in factions.iter().enumerate() {
        require!(faction.game == ctx.accounts.game.key(), GameError::InvalidFactionSet);
        require!(faction.wallet == rem[count + i].key(), GameError::InvalidFactionSet);
        require!(rem[i].is_writable && rem[count + i].is_writable, GameError::InvalidFactionSet);
        require!(!factions[..i].iter().any(|other| other.key() == faction.key()), GameError::InvalidFactionSet);
    }
    for (i, faction) in factions.iter().enumerate() {
        faction.close(rem[count + i].clone())?;
    }
    Ok(())
}
