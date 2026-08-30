use crate::{constants::*, error::GameError, events::*, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
#[instruction(game_id: u64)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + Game::INIT_SPACE,
        seeds = [GAME_SEED, game_id.to_le_bytes().as_ref()],
        bump
    )]
    pub game: Account<'info, Game>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize(
    ctx: Context<Initialize>,
    game_id: u64,
    entry_fee: u64,
    phase_duration: i64,
    entropy_mode: u8,
) -> Result<()> {
    require!(entry_fee > 0, GameError::InvalidEntryFee);
    require!(phase_duration >= 0, GameError::InvalidPhaseDuration);
    require!(
        entropy_mode == ENTROPY_SLOTHASH || entropy_mode == ENTROPY_SWITCHBOARD,
        GameError::InvalidEntropyMode
    );
    // Конституция: банк выше порога обязан играть с VRF-энтропией
    // (SPEC_VRF.md п.4; fallback на slot-hash для крупных банков запрещён).
    if entry_fee.saturating_mul(MAX_FACTIONS as u64) > MAINNET_VRF_THRESHOLD {
        require!(entropy_mode == ENTROPY_SWITCHBOARD, GameError::VrfRequired);
    }

    let clock = Clock::get()?;
    let game = &mut ctx.accounts.game;
    game.admin = ctx.accounts.admin.key();
    game.game_id = game_id;
    game.phase = Phase::Lobby;
    game.round = 0;
    game.phase_ends_at = clock
        .unix_timestamp
        .saturating_add(phase_duration.saturating_mul(LOBBY_MULT));
    game.faction_count = 0;
    game.laws_passed = 0;
    game.sold_this_round = 0;
    game.entry_fee = entry_fee;
    game.rake_bps = DEFAULT_RAKE_BPS;
    game.phase_duration = phase_duration;
    game.last_law_passed = false;
    game.yes_influence = 0;
    game.no_influence = 0;
    game.bump = ctx.bumps.game;
    game.law_card = NO_LAW;
    game.laws_used_mask = 0;
    game.veto_pending = false;
    game.president = Pubkey::default();
    game.active_tax_bps = 0;
    game.active_subsidy_goods = 0;
    game.active_price_shift = 0;
    game.active_boom = 0;
    game.pending_tax_bps = 0;
    game.pending_subsidy_goods = 0;
    game.pending_price_shift = 0;
    game.pending_boom = 0;
    game.settled = false;
    game.entropy_mode = entropy_mode;
    game.vrf_account = Pubkey::default();
    game.commit_slot = 0;
    game.vrf_retries = 0;
    game.vrf_spent = 0;

    emit!(GameInitialized {
        game: game.key(),
        game_id,
        entry_fee,
        phase_duration,
    });
    Ok(())
}
