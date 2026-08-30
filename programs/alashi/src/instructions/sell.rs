use crate::{constants::*, error::GameError, events::*, state::*};
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
    require!(game.phase == Phase::Market, GameError::WrongPhase);
    require!(faction.alive, GameError::NotAlive);
    require!(faction.acted_stamp != game.stamp(), GameError::AlreadyActed);
    require!(units > 0, GameError::NoUnits);
    require!(units <= faction.goods, GameError::NotEnoughGoods);

    let mut revenue: u64 = 0;
    for _ in 0..units {
        revenue += price_at(game.sold_this_round) * PESO;
        game.sold_this_round += 1;
    }
    faction.goods -= units;
    faction.cash += revenue;
    faction.acted_stamp = game.stamp();

    emit!(Sold {
        game: game.key(),
        faction: faction.key(),
        units,
        revenue,
    });
    Ok(())
}
