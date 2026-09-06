use crate::{constants::*, events::*, state::*};
use alashi_rules::actions;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct BuyDonkey<'info> {
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

pub fn handle_buy_donkey(ctx: Context<BuyDonkey>) -> Result<()> {
    let game = &ctx.accounts.game;
    let faction = &mut ctx.accounts.faction;
    actions::donkey(game, faction)?;
    let price = DONKEY_PRICE * PESO;

    emit!(DonkeyBought {
        game: game.key(),
        faction: faction.key(),
        price,
    });
    Ok(())
}
