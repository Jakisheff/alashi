use crate::{constants::*, events::*, state::*};
use alashi_rules::actions;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Veto<'info> {
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

pub fn handle_veto(ctx: Context<Veto>) -> Result<()> {
    let game = &mut ctx.accounts.game;
    let faction = &mut ctx.accounts.faction;
    actions::veto(game, faction)?;

    emit!(VetoCast {
        game: game.key(),
        president: faction.wallet,
        card: game.law_card,
    });
    Ok(())
}
