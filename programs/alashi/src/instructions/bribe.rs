use crate::{constants::*, error::GameError, events::*, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Bribe<'info> {
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
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), target.wallet.as_ref()],
        bump = target.bump,
        has_one = game
    )]
    pub target: Account<'info, Faction>,
}

pub fn handle_bribe(ctx: Context<Bribe>, amount: u64) -> Result<()> {
    let game = &ctx.accounts.game;
    let faction = &mut ctx.accounts.faction;
    let target = &mut ctx.accounts.target;
    require!(game.phase == Phase::Action, GameError::WrongPhase);
    require!(faction.alive, GameError::NotAlive);
    require!(target.alive, GameError::NotAlive);
    require!(faction.acted_stamp != game.stamp(), GameError::AlreadyActed);
    require!(faction.wallet != target.wallet, GameError::SelfBribe);

    let influence_gain = amount / BRIBE_PRICE;
    require!(influence_gain >= 1, GameError::BribeTooSmall);
    require!(faction.cash >= amount, GameError::NotEnoughCash);
    require!(
        faction.influence as u64 + influence_gain <= MAX_INFLUENCE as u64,
        GameError::BribeTooBig
    );

    faction.cash -= amount;
    target.cash += amount;
    faction.influence += influence_gain as u16;
    faction.acted_stamp = game.stamp();

    emit!(BribeGiven {
        game: game.key(),
        from: faction.key(),
        to: target.key(),
        amount,
        influence_gained: influence_gain as u16,
    });
    Ok(())
}
