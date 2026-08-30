use crate::{error::GameError, events::*, logic::compute_purchase, state::*};
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
    require!(game.phase == Phase::Market, GameError::WrongPhase);
    require!(faction.alive, GameError::NotAlive);
    require!(faction.acted_stamp != game.stamp(), GameError::AlreadyActed);
    require!(units > 0, GameError::NoUnits);

    let trade = compute_purchase(
        units,
        game.sold_this_round,
        game.active_price_shift,
        game.active_boom,
    );
    require!(faction.cash >= trade.gross, GameError::NotEnoughCash);

    faction.cash -= trade.gross;
    faction.goods += units;
    game.sold_this_round = trade.counter_after;
    faction.acted_stamp = game.stamp();

    emit!(GoodsBought {
        game: game.key(),
        faction: faction.key(),
        units,
        cost: trade.gross,
    });
    Ok(())
}
