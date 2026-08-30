use crate::{constants::*, error::GameError, events::*, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Join<'info> {
    #[account(mut)]
    pub player: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        init,
        payer = player,
        space = 8 + Faction::INIT_SPACE,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump
    )]
    pub faction: Account<'info, Faction>,
    pub system_program: Program<'info, System>,
}

pub fn handle_join(ctx: Context<Join>, name: String) -> Result<()> {
    require!(name.len() <= MAX_NAME, GameError::NameTooLong);
    require!(
        ctx.accounts.game.phase == Phase::Lobby,
        GameError::GameNotInLobby
    );
    require!(
        ctx.accounts.game.faction_count < MAX_FACTIONS,
        GameError::GameFull
    );

    let fee = ctx.accounts.game.entry_fee;
    let cpi_accounts = anchor_lang::system_program::Transfer {
        from: ctx.accounts.player.to_account_info(),
        to: ctx.accounts.game.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(anchor_lang::system_program::ID, cpi_accounts);
    anchor_lang::system_program::transfer(cpi_ctx, fee)?;

    let game = &mut ctx.accounts.game;
    game.faction_count += 1;
    let count = game.faction_count;

    let faction = &mut ctx.accounts.faction;
    faction.game = game.key();
    faction.wallet = ctx.accounts.player.key();
    faction.name = name.clone();
    faction.cash = 0;
    faction.goods = 0;
    faction.influence = 1;
    faction.acted_stamp = 0;
    faction.voted_stamp = 0;
    faction.vote = VoteChoice::Abstain;
    faction.is_president = false;
    faction.alive = true;
    faction.bump = ctx.bumps.faction;

    emit!(FactionJoined {
        game: game.key(),
        faction: faction.key(),
        name,
        count,
    });
    Ok(())
}
