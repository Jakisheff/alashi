use crate::{constants::*, events::*, state::*};
use alashi_rules::actions;
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
    let before = faction.influence;
    actions::bribe(game, faction, target, amount)?;
    let influence_gain = faction.influence - before;

    emit!(BribeGiven {
        game: game.key(),
        from: faction.key(),
        to: target.key(),
        amount,
        influence_gained: influence_gain,
    });
    Ok(())
}
