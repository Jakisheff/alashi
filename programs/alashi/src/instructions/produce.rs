use crate::{constants::*, error::GameError, events::*, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Produce<'info> {
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

pub fn handle_produce(ctx: Context<Produce>) -> Result<()> {
    let game = &ctx.accounts.game;
    let faction = &mut ctx.accounts.faction;
    require!(game.phase == Phase::Action, GameError::WrongPhase);
    require!(faction.alive, GameError::NotAlive);
    require!(faction.acted_stamp != game.stamp(), GameError::AlreadyActed);

    faction.goods += PRODUCE_YIELD;
    faction.acted_stamp = game.stamp();

    emit!(Produced {
        game: game.key(),
        faction: faction.key(),
        goods: faction.goods,
    });
    Ok(())
}
