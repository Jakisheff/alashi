use crate::{
    constants::*,
    donkey_state::{oracle_queue, DonkeyOracleConfig, DONKEY_SOURCE_SEED},
    error::GameError,
    events::*,
    state::*,
};
use alashi_rules::actions;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};
use switchboard_on_demand::accounts::RandomnessAccountData;
use switchboard_on_demand::ON_DEMAND_DEVNET_PID;

const RANDOMNESS_INIT_DISCRIMINATOR: [u8; 8] = [9, 9, 204, 33, 50, 116, 113, 15];
const TOKEN_PROGRAM: Pubkey = anchor_lang::pubkey!("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ASSOCIATED_TOKEN_PROGRAM: Pubkey =
    anchor_lang::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const NATIVE_MINT: Pubkey = anchor_lang::pubkey!("So11111111111111111111111111111111111111112");
const LOOKUP_TABLE_PROGRAM: Pubkey =
    anchor_lang::pubkey!("AddressLookupTab1e1111111111111111111111111");

/// Initialize one round's provider-owned PDA before any faction can join.
/// The provider requires both the new source and the source authority to sign;
/// only this program can sign the two Alashi PDAs. The authority PDA need not
/// be funded yet, because configure_donkey_oracles creates it after all six
/// sources have been initialized.
#[derive(Accounts)]
#[instruction(round: u8, recent_slot: u64)]
pub struct InitDonkeySource<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()], bump = game.bump)]
    pub game: Account<'info, Game>,
    /// CHECK: deterministic, uninitialized source PDA; provider creates it by CPI.
    #[account(mut, seeds = [DONKEY_SOURCE_SEED, game.key().as_ref(), &[round]], bump)]
    pub randomness: UncheckedAccount<'info>,
    /// CHECK: deterministic authority PDA, signed only by this program.
    #[account(seeds = [DONKEY_ORACLE_SEED, game.key().as_ref()], bump)]
    pub authority: UncheckedAccount<'info>,
    /// CHECK: canonical off-curve native-mint ATA, checked before CPI.
    #[account(mut)]
    pub reward_escrow: UncheckedAccount<'info>,
    /// CHECK: Switchboard queue, owner checked before CPI.
    #[account(mut)]
    pub queue: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    /// CHECK: fixed SPL Token program id.
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: fixed associated-token program id.
    pub associated_token_program: UncheckedAccount<'info>,
    /// CHECK: fixed native mint id.
    pub native_mint: UncheckedAccount<'info>,
    /// CHECK: provider-owned state, validated by provider CPI.
    pub switchboard_state: UncheckedAccount<'info>,
    /// CHECK: provider LUT signer PDA, checked before CPI.
    pub lut_signer: UncheckedAccount<'info>,
    /// CHECK: address lookup table derived from signer and recent slot.
    #[account(mut)]
    pub lut: UncheckedAccount<'info>,
    /// CHECK: fixed address-lookup-table program id.
    pub lut_program: UncheckedAccount<'info>,
    /// CHECK: fixed deployed Switchboard devnet program id.
    pub switchboard_program: UncheckedAccount<'info>,
}

pub fn handle_init_donkey_source(
    ctx: Context<InitDonkeySource>,
    round: u8,
    recent_slot: u64,
) -> Result<()> {
    let game = &ctx.accounts.game;
    require!(
        game.epoch == EPOCH_DONKEY_RISK && game.entropy_mode == ENTROPY_SWITCHBOARD,
        GameError::DonkeyOracleInvalid
    );
    require!(
        game.phase == Phase::Lobby && game.faction_count == 0,
        GameError::GameNotInLobby
    );
    require_keys_eq!(game.admin, ctx.accounts.admin.key(), GameError::NotAdmin);
    require!(round > 0 && round <= ROUNDS, GameError::DonkeyOracleInvalid);
    let clock_slot = Clock::get()?.slot;
    require!(
        recent_slot > 0 && recent_slot <= clock_slot && clock_slot - recent_slot <= 150,
        GameError::DonkeyOracleInvalid
    );
    let source = ctx.accounts.randomness.to_account_info();
    require_keys_eq!(*source.owner, System::id(), GameError::DonkeyOracleInvalid);
    require!(source.data_is_empty(), GameError::DonkeyOracleInvalid);
    require_keys_eq!(
        ctx.accounts.token_program.key(),
        TOKEN_PROGRAM,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        ctx.accounts.associated_token_program.key(),
        ASSOCIATED_TOKEN_PROGRAM,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        ctx.accounts.native_mint.key(),
        NATIVE_MINT,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        ctx.accounts.lut_program.key(),
        LOOKUP_TABLE_PROGRAM,
        GameError::DonkeyOracleInvalid
    );
    let provider_id = Pubkey::new_from_array(ON_DEMAND_DEVNET_PID.to_bytes());
    require_keys_eq!(
        ctx.accounts.switchboard_program.key(),
        provider_id,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        *ctx.accounts.queue.to_account_info().owner,
        provider_id,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        *ctx.accounts.switchboard_state.to_account_info().owner,
        provider_id,
        GameError::DonkeyOracleInvalid
    );
    let escrow = Pubkey::find_program_address(
        &[
            source.key.as_ref(),
            TOKEN_PROGRAM.as_ref(),
            NATIVE_MINT.as_ref(),
        ],
        &ASSOCIATED_TOKEN_PROGRAM,
    )
    .0;
    require_keys_eq!(
        ctx.accounts.reward_escrow.key(),
        escrow,
        GameError::DonkeyOracleInvalid
    );
    let lut_signer =
        Pubkey::find_program_address(&[b"LutSigner", source.key.as_ref()], &provider_id).0;
    require_keys_eq!(
        ctx.accounts.lut_signer.key(),
        lut_signer,
        GameError::DonkeyOracleInvalid
    );
    let lut = Pubkey::find_program_address(
        &[lut_signer.as_ref(), &recent_slot.to_le_bytes()],
        &LOOKUP_TABLE_PROGRAM,
    )
    .0;
    require_keys_eq!(ctx.accounts.lut.key(), lut, GameError::DonkeyOracleInvalid);

    let mut data = RANDOMNESS_INIT_DISCRIMINATOR.to_vec();
    data.extend_from_slice(&recent_slot.to_le_bytes());
    let ix = Instruction {
        program_id: provider_id,
        accounts: vec![
            AccountMeta::new(*source.key, true),
            AccountMeta::new(escrow, false),
            AccountMeta::new_readonly(ctx.accounts.authority.key(), true),
            AccountMeta::new(ctx.accounts.queue.key(), false),
            AccountMeta::new(ctx.accounts.admin.key(), true),
            AccountMeta::new_readonly(System::id(), false),
            AccountMeta::new_readonly(TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(ASSOCIATED_TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(NATIVE_MINT, false),
            AccountMeta::new_readonly(ctx.accounts.switchboard_state.key(), false),
            AccountMeta::new_readonly(lut_signer, false),
            AccountMeta::new(lut, false),
            AccountMeta::new_readonly(LOOKUP_TABLE_PROGRAM, false),
        ],
        data,
    };
    let game_key = game.key();
    let round_bytes = [round];
    let source_bump = [ctx.bumps.randomness];
    let authority_bump = [ctx.bumps.authority];
    let source_seeds: &[&[u8]] = &[
        DONKEY_SOURCE_SEED,
        game_key.as_ref(),
        &round_bytes,
        &source_bump,
    ];
    let authority_seeds: &[&[u8]] = &[DONKEY_ORACLE_SEED, game_key.as_ref(), &authority_bump];
    let infos = [
        source,
        ctx.accounts.reward_escrow.to_account_info(),
        ctx.accounts.authority.to_account_info(),
        ctx.accounts.queue.to_account_info(),
        ctx.accounts.admin.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.associated_token_program.to_account_info(),
        ctx.accounts.native_mint.to_account_info(),
        ctx.accounts.switchboard_state.to_account_info(),
        ctx.accounts.lut_signer.to_account_info(),
        ctx.accounts.lut.to_account_info(),
        ctx.accounts.lut_program.to_account_info(),
        ctx.accounts.switchboard_program.to_account_info(),
    ];
    invoke_signed(&ix, &infos, &[source_seeds, authority_seeds])?;
    let created_info = ctx.accounts.randomness.to_account_info();
    let created = RandomnessAccountData::parse(created_info.data.borrow())
        .map_err(|_| GameError::RandomnessMismatch)?;
    require!(
        created.seed_slot == 0 && created.reveal_slot == 0,
        GameError::DonkeyOracleInvalid
    );
    require!(
        created.authority.as_ref() == ctx.accounts.authority.key().as_ref()
            && created.queue.as_ref() == ctx.accounts.queue.key().as_ref(),
        GameError::DonkeyOracleInvalid
    );
    Ok(())
}

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
    require!(
        game.phase == Phase::Lobby && game.faction_count == 0,
        GameError::GameNotInLobby
    );
    require_keys_eq!(game.admin, ctx.accounts.admin.key(), GameError::NotAdmin);
    require!(
        ctx.remaining_accounts.len() == (ROUNDS as usize) * 2,
        GameError::DonkeyOracleInvalid
    );

    let mut accounts = [Pubkey::default(); ROUNDS as usize];
    let mut oracles = [Pubkey::default(); ROUNDS as usize];
    let mut queue: Option<Pubkey> = None;
    let mut authority: Option<Pubkey> = None;
    let game_key = game.key();
    let authority_key = ctx.accounts.oracle.key();
    for (i, ai) in ctx.remaining_accounts[..ROUNDS as usize].iter().enumerate() {
        let round = [(i as u8) + 1];
        let expected = Pubkey::find_program_address(
            &[DONKEY_SOURCE_SEED, game_key.as_ref(), &round],
            &crate::id(),
        )
        .0;
        require_keys_eq!(*ai.key, expected, GameError::DonkeyOracleInvalid);
        require!(
            !accounts[..i].contains(ai.key) && *ai.key != Pubkey::default(),
            GameError::DonkeyOracleInvalid
        );
        require!(
            ai.owner.as_ref() == ON_DEMAND_DEVNET_PID.as_ref(),
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
                && data.authority.as_ref() == authority_key.as_ref(),
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
        // The provider leaves Randomness.oracle unset at Init. Freeze the
        // actual chosen oracle separately before any entry fee or ticket.
        let oracle_ai = &ctx.remaining_accounts[ROUNDS as usize + i];
        require_keys_eq!(
            *oracle_ai.owner,
            Pubkey::new_from_array(ON_DEMAND_DEVNET_PID.to_bytes()),
            GameError::DonkeyOracleInvalid
        );
        require_keys_eq!(
            oracle_queue(oracle_ai)?,
            Pubkey::new_from_array(data.queue.to_bytes()),
            GameError::DonkeyOracleInvalid
        );
        oracles[i] = *oracle_ai.key;
    }
    let oracle = &mut ctx.accounts.oracle;
    oracle.game = game.key();
    oracle.queue = queue.ok_or(GameError::DonkeyOracleInvalid)?;
    oracle.authority = authority.ok_or(GameError::DonkeyOracleInvalid)?;
    oracle.accounts = accounts;
    oracle.oracles = oracles;
    oracle.committed_rounds_mask = 0;
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
        rng.owner.as_ref() == ON_DEMAND_DEVNET_PID.as_ref(),
        GameError::RandomnessMismatch
    );
    let data = RandomnessAccountData::parse(rng.data.borrow())
        .map_err(|_| GameError::RandomnessMismatch)?;
    require!(
        oracle.committed_rounds_mask & (1 << (game.round - 1)) == 0,
        GameError::DonkeyOracleInvalid
    );
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
