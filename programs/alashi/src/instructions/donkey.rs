use crate::{constants::*, error::GameError, events::*, state::*};
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
    require!(game.phase == Phase::Action, GameError::WrongPhase);
    require!(faction.alive, GameError::NotAlive);
    require!(faction.acted_stamp != game.stamp(), GameError::AlreadyActed);
    let price = DONKEY_PRICE * PESO;
    require!(faction.cash >= price, GameError::NotEnoughCash);

    faction.cash -= price;
    faction.goods += 1;
    faction.acted_stamp = game.stamp();

    emit!(DonkeyBought {
        game: game.key(),
        faction: faction.key(),
        price,
    });
    Ok(())
}
