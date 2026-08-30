use crate::{constants::*, error::GameError, events::*, state::*};
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
    require!(game.phase == Phase::Law, GameError::WrongPhase);
    require!(game.president == faction.wallet, GameError::NotPresident);
    require!(!game.veto_pending, GameError::AlreadyVetoed);

    game.veto_pending = true;
    faction.is_president = true;

    emit!(VetoCast {
        game: game.key(),
        president: faction.wallet,
        card: game.law_card,
    });
    Ok(())
}
