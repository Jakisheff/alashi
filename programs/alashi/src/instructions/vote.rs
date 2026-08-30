use crate::{constants::*, error::GameError, events::*, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Vote<'info> {
    pub player: Signer<'info>,
    #[account(
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump
    )]
    pub faction: Account<'info, Faction>,
}

pub fn handle_vote(ctx: Context<Vote>, choice: VoteChoice) -> Result<()> {
    let game = &ctx.accounts.game;
    let faction = &mut ctx.accounts.faction;
    require!(game.phase == Phase::Law, GameError::WrongPhase);
    require!(faction.alive, GameError::NotAlive);
    require!(faction.voted_stamp != game.stamp(), GameError::AlreadyVoted);

    faction.vote = choice;
    faction.voted_stamp = game.stamp();

    emit!(VoteCast {
        game: game.key(),
        faction: faction.key(),
        choice,
    });
    Ok(())
}
