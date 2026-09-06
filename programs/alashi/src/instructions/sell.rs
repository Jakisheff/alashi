use crate::{constants::*, events::*, state::*};
use alashi_rules::actions;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Sell<'info> {
    pub player: Signer<'info>,
    #[account(
        mut,
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

pub fn handle_sell(ctx: Context<Sell>, units: u16) -> Result<()> {
    let game = &mut ctx.accounts.game;
    let faction = &mut ctx.accounts.faction;
    let revenue = actions::sell(game, faction, units, false)?;

    emit!(Sold {
        game: game.key(),
        faction: faction.key(),
        units,
        revenue,
    });
    Ok(())
}
