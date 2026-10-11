use crate::{
    constants::*, donkey_state::DonkeyOracleConfig, error::GameError, events::*, state::*,
};
use alashi_rules::actions;
use anchor_lang::prelude::*;
use switchboard_on_demand::accounts::RandomnessAccountData;
use switchboard_on_demand::{ON_DEMAND_DEVNET_PID, ON_DEMAND_MAINNET_PID};

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

#[derive(Accounts)]
pub struct ConfigureDonkeyOracles<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        init,
        payer = admin,
        space = 8 + DonkeyOracleConfig::INIT_SPACE,
        seeds = [DONKEY_ORACLE_SEED, game.key().as_ref()],
        bump
    )]
    pub oracle: Account<'info, DonkeyOracleConfig>,
    pub system_program: Program<'info, System>,
}

pub fn handle_configure_donkey_oracles(ctx: Context<ConfigureDonkeyOracles>) -> Result<()> {
    let game = &ctx.accounts.game;
    require!(
        game.epoch == EPOCH_DONKEY_RISK && game.entropy_mode == ENTROPY_SWITCHBOARD,
        GameError::DonkeyOracleInvalid
    );
    require!(game.phase == Phase::Lobby, GameError::GameNotInLobby);
    require_keys_eq!(game.admin, ctx.accounts.admin.key(), GameError::NotAdmin);
    require!(
        ctx.remaining_accounts.len() == ROUNDS as usize,
        GameError::DonkeyOracleInvalid
    );

    let mut accounts = [Pubkey::default(); ROUNDS as usize];
    let mut queue: Option<Pubkey> = None;
    let mut authority: Option<Pubkey> = None;
    for (i, ai) in ctx.remaining_accounts.iter().enumerate() {
        require!(
            !accounts[..i].contains(ai.key) && *ai.key != Pubkey::default(),
            GameError::DonkeyOracleInvalid
        );
        require!(
            ai.owner.as_ref() == ON_DEMAND_DEVNET_PID.as_ref()
                || ai.owner.as_ref() == ON_DEMAND_MAINNET_PID.as_ref(),
            GameError::RandomnessMismatch
        );
        let data = RandomnessAccountData::parse(ai.data.borrow())
            .map_err(|_| GameError::RandomnessMismatch)?;
        require!(
            data.seed_slot == 0 && data.reveal_slot == 0,
            GameError::DonkeyOracleInvalid
        );
        require!(
            data.queue.as_ref() != Pubkey::default().as_ref()
                && data.authority.as_ref() != Pubkey::default().as_ref(),
            GameError::DonkeyOracleInvalid
        );
        if let Some(q) = queue {
            require!(
                data.queue.as_ref() == q.as_ref(),
                GameError::DonkeyOracleInvalid
            );
        }
        if let Some(a) = authority {
            require!(
                data.authority.as_ref() == a.as_ref(),
                GameError::DonkeyOracleInvalid
            );
        }
        queue = Some(Pubkey::new_from_array(data.queue.to_bytes()));
        authority = Some(Pubkey::new_from_array(data.authority.to_bytes()));
        accounts[i] = *ai.key;
    }
    let oracle = &mut ctx.accounts.oracle;
    oracle.game = game.key();
    oracle.queue = queue.ok_or(GameError::DonkeyOracleInvalid)?;
    oracle.authority = authority.ok_or(GameError::DonkeyOracleInvalid)?;
    oracle.accounts = accounts;
    oracle.round = 0;
    oracle.ticket_count = 0;
    oracle.tickets = [Pubkey::default(); MAX_FACTIONS as usize];
    oracle.max_attempt_slot = 0;
    oracle.resolved = true;
    oracle.bump = ctx.bumps.oracle;
    Ok(())
}

#[derive(Accounts)]
pub struct AttemptDonkey<'info> {
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
        seeds = [DONKEY_ORACLE_SEED, game.key().as_ref()],
        bump = oracle.bump
    )]
    pub oracle: Account<'info, DonkeyOracleConfig>,
    /// CHECK: must be the configured round source, owned by Switchboard,
    /// still uncommitted and unrevealed before the fee is taken.
    pub randomness: UncheckedAccount<'info>,
}

pub fn handle_attempt_donkey(ctx: Context<AttemptDonkey>) -> Result<()> {
    let game = &ctx.accounts.game;
    let oracle = &mut ctx.accounts.oracle;
    require_keys_eq!(oracle.game, game.key(), GameError::DonkeyOracleInvalid);
    require!(
        ctx.accounts.faction.game == game.key(),
        GameError::InvalidFactionSet
    );
    require!(
        game.round > 0 && game.round <= ROUNDS,
        GameError::DonkeyOracleInvalid
    );
    let rng = ctx.accounts.randomness.to_account_info();
    require_keys_eq!(
        *rng.key,
        oracle.accounts[(game.round - 1) as usize],
        GameError::RandomnessMismatch
    );
    require!(
        rng.owner.as_ref() == ON_DEMAND_DEVNET_PID.as_ref()
            || rng.owner.as_ref() == ON_DEMAND_MAINNET_PID.as_ref(),
        GameError::RandomnessMismatch
    );
    let data = RandomnessAccountData::parse(rng.data.borrow())
        .map_err(|_| GameError::RandomnessMismatch)?;
    require!(
        data.queue.as_ref() == oracle.queue.as_ref()
            && data.authority.as_ref() == oracle.authority.as_ref()
            && data.seed_slot == 0
            && data.reveal_slot == 0,
        GameError::DonkeyOracleInvalid
    );
    if oracle.round != game.round {
        require!(oracle.resolved, GameError::DonkeyOracleInvalid);
        oracle.round = game.round;
        oracle.ticket_count = 0;
        oracle.tickets = [Pubkey::default(); MAX_FACTIONS as usize];
        oracle.max_attempt_slot = 0;
        oracle.resolved = false;
    }
    require!(
        !oracle.resolved && oracle.ticket_count < MAX_FACTIONS,
        GameError::DonkeyOracleInvalid
    );
    require!(
        !oracle.tickets[..oracle.ticket_count as usize].contains(&ctx.accounts.faction.key()),
        GameError::AlreadyActed
    );
    actions::donkey_risk_attempt(game, &mut ctx.accounts.faction)?;
    let ticket_index = oracle.ticket_count as usize;
    oracle.tickets[ticket_index] = ctx.accounts.faction.key();
    oracle.ticket_count += 1;
    oracle.max_attempt_slot = oracle.max_attempt_slot.max(Clock::get()?.slot);
    emit!(DonkeyAttempted {
        game: game.key(),
        faction: ctx.accounts.faction.key(),
        round: game.round,
        fee: DONKEY_RISK_FEE,
    });
    Ok(())
}
