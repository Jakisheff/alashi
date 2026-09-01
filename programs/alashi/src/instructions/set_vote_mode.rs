use crate::{constants::*, error::GameError, events::*, state::*};
use anchor_lang::prelude::*;

/// Взнос-как-голос (SPEC_VOTE_CONTRIBUTION.md): переключение режима
/// веса голоса. Только админ и только в Lobby, чтобы режим нельзя было
/// менять посреди партии. 0 = legacy (откат), 1 = contribution.
#[derive(Accounts)]
pub struct SetVoteMode<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
}

pub fn handle_set_vote_mode(ctx: Context<SetVoteMode>, mode: u8) -> Result<()> {
    require!(
        ctx.accounts.admin.key() == ctx.accounts.game.admin,
        GameError::NotAdmin
    );
    require!(
        ctx.accounts.game.phase == Phase::Lobby,
        GameError::GameNotInLobby
    );
    require!(
        mode == VOTE_WEIGHT_LEGACY || mode == VOTE_WEIGHT_CONTRIB,
        GameError::InvalidVoteWeightMode
    );
    let game = &mut ctx.accounts.game;
    let prev = game.vote_weight_mode;
    game.vote_weight_mode = mode;
    emit!(VoteModeSet {
        game: game.key(),
        mode,
    });
    msg!("vote_weight_mode: {} -> {}", prev, mode);
    Ok(())
}
