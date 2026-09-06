use crate::{events::*, state::*};
use alashi_rules::actions;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct BuyGoods<'info> {
    pub player: Signer<'info>,
    #[account(
        mut,
        seeds = [crate::constants::GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [
            crate::constants::FACTION_SEED,
            game.key().as_ref(),
            player.key().as_ref()
        ],
        bump = faction.bump
    )]
    pub faction: Account<'info, Faction>,
}

pub fn handle_buy(ctx: Context<BuyGoods>, units: u16) -> Result<()> {
    let game = &mut ctx.accounts.game;
    let faction = &mut ctx.accounts.faction;
    let cost = actions::buy(game, faction, units)?;

    emit!(GoodsBought {
        game: game.key(),
        faction: faction.key(),
        units,
        cost,
    });
    Ok(())
}
