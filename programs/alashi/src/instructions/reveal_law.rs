use crate::instructions::donkey::{NATIVE_MINT, TOKEN_PROGRAM};
use crate::{
    constants::*, donkey_state::DonkeyOracleConfig, error::GameError, events::*, state::*,
};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};
use solana_sha256_hasher::hashv;
use switchboard_on_demand::accounts::RandomnessAccountData;
use switchboard_on_demand::ON_DEMAND_DEVNET_PID;
use switchboard_on_demand::ON_DEMAND_MAINNET_PID;

#[derive(Accounts)]
pub struct RevealLaw<'info> {
    pub crank: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    /// CHECK: ключ сверен с game.vrf_account, владелец — программа Switchboard
    /// On-Demand, данные парсятся RandomnessAccountData::parse; значение
    /// валидируется get_value в слоте reveal (клиент бандлит revealIx с
    /// этой инструкцией одной транзакцией, подпись оракула проверяет
    /// программа Switchboard в revealIx).
    pub randomness: UncheckedAccount<'info>,
}

/// Epoch-2 reveal signs for the configured source authority by CPI, then
/// resolves the law and every due ticket in this same transaction/slot.
#[derive(Accounts)]
pub struct RevealDonkeyLaw<'info> {
    #[account(mut)]
    pub crank: Signer<'info>,
    #[account(mut, seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()], bump = game.bump)]
    pub game: Account<'info, Game>,
    /// CHECK: exact pinned provider-owned source, validated before and after CPI.
    #[account(mut)]
    pub randomness: UncheckedAccount<'info>,
    /// CHECK: exact oracle frozen before Join, validated against provider ownership.
    pub oracle: UncheckedAccount<'info>,
    /// CHECK: exact queue frozen before Join.
    pub queue: UncheckedAccount<'info>,
    /// CHECK: canonical provider stats PDA.
    #[account(mut)]
    pub stats: UncheckedAccount<'info>,
    /// CHECK: canonical sysvar.
    pub recent_slothashes: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
    /// CHECK: canonical source native-mint ATA.
    #[account(mut)]
    pub reward_escrow: UncheckedAccount<'info>,
    /// CHECK: fixed token program.
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: fixed native mint.
    pub native_mint: UncheckedAccount<'info>,
    /// CHECK: canonical provider state PDA.
    pub provider_state: UncheckedAccount<'info>,
    /// CHECK: fixed executable Switchboard devnet program.
    pub provider_program: UncheckedAccount<'info>,
}

const RANDOMNESS_REVEAL_DISCRIMINATOR: [u8; 8] = [197, 181, 187, 10, 30, 58, 20, 73];
const ASSOCIATED_TOKEN_PROGRAM: Pubkey =
    anchor_lang::pubkey!("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");

pub fn handle_reveal_donkey_law<'info>(
    ctx: Context<'info, RevealDonkeyLaw<'info>>,
    signature: [u8; 64],
    recovery_id: u8,
    value: [u8; 32],
) -> Result<()> {
    let game = &ctx.accounts.game;
    require!(
        game.epoch == EPOCH_DONKEY_RISK,
        GameError::DonkeyOracleInvalid
    );
    require!(
        game.entropy_mode == ENTROPY_SWITCHBOARD,
        GameError::InvalidEntropyMode
    );
    require!(
        game.phase == Phase::Law && game.law_card == NO_LAW,
        GameError::WrongPhase
    );
    require!(
        game.round > 0 && game.round <= ROUNDS,
        GameError::DonkeyOracleInvalid
    );
    let provider = Pubkey::new_from_array(ON_DEMAND_DEVNET_PID.to_bytes());
    require_keys_eq!(
        ctx.accounts.provider_program.key(),
        provider,
        GameError::DonkeyOracleInvalid
    );
    require!(
        ctx.accounts.provider_program.executable,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        ctx.accounts.randomness.key(),
        game.vrf_account,
        GameError::RandomnessMismatch
    );
    require_keys_eq!(
        *ctx.accounts.randomness.to_account_info().owner,
        provider,
        GameError::RandomnessMismatch
    );
    let config_ai = ctx
        .remaining_accounts
        .first()
        .ok_or(GameError::DonkeyOracleInvalid)?;
    require!(config_ai.is_writable, GameError::DonkeyOracleInvalid);
    let config = Account::<DonkeyOracleConfig>::try_from(config_ai)?;
    let (config_key, bump) =
        Pubkey::find_program_address(&[DONKEY_ORACLE_SEED, game.key().as_ref()], &crate::id());
    require_keys_eq!(config.key(), config_key, GameError::DonkeyOracleInvalid);
    require_keys_eq!(config.game, game.key(), GameError::DonkeyOracleInvalid);
    require_keys_eq!(config.authority, config_key, GameError::DonkeyOracleInvalid);
    require!(config.bump == bump, GameError::DonkeyOracleInvalid);
    let index = (game.round - 1) as usize;
    require!(
        config.committed_rounds_mask & (1 << index) != 0,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        config.accounts[index],
        ctx.accounts.randomness.key(),
        GameError::RandomnessMismatch
    );
    require_keys_eq!(
        config.oracles[index],
        ctx.accounts.oracle.key(),
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        config.queue,
        ctx.accounts.queue.key(),
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        *ctx.accounts.oracle.to_account_info().owner,
        provider,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        *ctx.accounts.queue.to_account_info().owner,
        provider,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        ctx.accounts.recent_slothashes.key(),
        solana_sysvar::slot_hashes::ID,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        ctx.accounts.token_program.key(),
        TOKEN_PROGRAM,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        ctx.accounts.native_mint.key(),
        NATIVE_MINT,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        ctx.accounts.stats.key(),
        Pubkey::find_program_address(
            &[b"OracleRandomnessStats", ctx.accounts.oracle.key().as_ref()],
            &provider
        )
        .0,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        ctx.accounts.provider_state.key(),
        Pubkey::find_program_address(&[b"STATE"], &provider).0,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        ctx.accounts.reward_escrow.key(),
        Pubkey::find_program_address(
            &[
                ctx.accounts.randomness.key().as_ref(),
                TOKEN_PROGRAM.as_ref(),
                NATIVE_MINT.as_ref()
            ],
            &ASSOCIATED_TOKEN_PROGRAM
        )
        .0,
        GameError::DonkeyOracleInvalid
    );
    {
        let rng = ctx.accounts.randomness.to_account_info();
        let data = RandomnessAccountData::parse(rng.data.borrow())
            .map_err(|_| GameError::RandomnessMismatch)?;
        require!(
            data.seed_slot == game.commit_slot && data.reveal_slot == 0,
            GameError::RandomnessMismatch
        );
        require!(
            data.authority.as_ref() == config_key.as_ref()
                && data.queue.as_ref() == config.queue.as_ref()
                && data.oracle.as_ref() == ctx.accounts.oracle.key().as_ref(),
            GameError::RandomnessMismatch
        );
    }
    // The provider verifies the gateway signature. An error in either CPI or
    // Alashi's later complete-ticket resolution rolls the whole transaction back.
    let mut data = RANDOMNESS_REVEAL_DISCRIMINATOR.to_vec();
    data.extend_from_slice(&signature);
    data.push(recovery_id);
    data.extend_from_slice(&value);
    let ix = Instruction {
        program_id: provider,
        accounts: vec![
            AccountMeta::new(ctx.accounts.randomness.key(), false),
            AccountMeta::new_readonly(ctx.accounts.oracle.key(), false),
            AccountMeta::new_readonly(ctx.accounts.queue.key(), false),
            AccountMeta::new(ctx.accounts.stats.key(), false),
            AccountMeta::new_readonly(config_key, true),
            AccountMeta::new(ctx.accounts.crank.key(), true),
            AccountMeta::new_readonly(ctx.accounts.recent_slothashes.key(), false),
            AccountMeta::new_readonly(System::id(), false),
            AccountMeta::new(ctx.accounts.reward_escrow.key(), false),
            AccountMeta::new_readonly(TOKEN_PROGRAM, false),
            AccountMeta::new_readonly(NATIVE_MINT, false),
            AccountMeta::new_readonly(ctx.accounts.provider_state.key(), false),
        ],
        data,
    };
    let game_key = game.key();
    let bump_seed = [bump];
    let seeds: &[&[u8]] = &[DONKEY_ORACLE_SEED, game_key.as_ref(), &bump_seed];
    invoke_signed(
        &ix,
        &[
            ctx.accounts.randomness.to_account_info(),
            ctx.accounts.oracle.to_account_info(),
            ctx.accounts.queue.to_account_info(),
            ctx.accounts.stats.to_account_info(),
            config_ai.clone(),
            ctx.accounts.crank.to_account_info(),
            ctx.accounts.recent_slothashes.to_account_info(),
            ctx.accounts.system_program.to_account_info(),
            ctx.accounts.reward_escrow.to_account_info(),
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.native_mint.to_account_info(),
            ctx.accounts.provider_state.to_account_info(),
            ctx.accounts.provider_program.to_account_info(),
        ],
        &[seeds],
    )?;
    drop(config);
    resolve_law(
        &mut ctx.accounts.game,
        &ctx.accounts.randomness.to_account_info(),
        ctx.remaining_accounts,
    )
}

pub fn handle_reveal_law<'info>(ctx: Context<'info, RevealLaw<'info>>) -> Result<()> {
    resolve_law(
        &mut ctx.accounts.game,
        &ctx.accounts.randomness.to_account_info(),
        ctx.remaining_accounts,
    )
}

fn resolve_law<'info>(
    game: &mut Account<'info, Game>,
    rng: &AccountInfo<'info>,
    remaining_accounts: &'info [AccountInfo<'info>],
) -> Result<()> {
    require!(
        game.entropy_mode == ENTROPY_SWITCHBOARD,
        GameError::InvalidEntropyMode
    );
    require!(game.phase == Phase::Law, GameError::WrongPhase);
    require!(game.law_card == NO_LAW, GameError::LawNotRevealed);
    require!(*rng.key == game.vrf_account, GameError::RandomnessMismatch);
    let owner = rng.owner;
    require!(
        if game.epoch == EPOCH_DONKEY_RISK {
            owner.as_ref() == ON_DEMAND_DEVNET_PID.as_ref()
        } else {
            owner.as_ref() == ON_DEMAND_DEVNET_PID.as_ref()
                || owner.as_ref() == ON_DEMAND_MAINNET_PID.as_ref()
        },
        GameError::RandomnessMismatch
    );
    let data = RandomnessAccountData::parse(rng.data.borrow())
        .map_err(|_| GameError::RandomnessMismatch)?;
    require!(
        data.seed_slot == game.commit_slot,
        GameError::RandomnessMismatch
    );
    let clock = Clock::get()?;
    let value = data
        .get_value(clock.slot)
        .map_err(|_| GameError::RandomnessNotReady)?;
    let is_risk = game.epoch == EPOCH_DONKEY_RISK;
    let mut oracle = if is_risk {
        require!(
            game.round > 0 && game.round <= ROUNDS,
            GameError::DonkeyOracleInvalid
        );
        let ai = remaining_accounts
            .first()
            .ok_or(GameError::DonkeyOracleInvalid)?;
        require!(ai.is_writable, GameError::DonkeyOracleInvalid);
        let config = Account::<DonkeyOracleConfig>::try_from(ai)?;
        let expected =
            Pubkey::find_program_address(&[DONKEY_ORACLE_SEED, game.key().as_ref()], &crate::id())
                .0;
        require_keys_eq!(config.key(), expected, GameError::DonkeyOracleInvalid);
        require_keys_eq!(config.game, game.key(), GameError::DonkeyOracleInvalid);
        require!(
            config.committed_rounds_mask & (1 << (game.round - 1)) != 0,
            GameError::DonkeyOracleInvalid
        );
        require_keys_eq!(
            config.accounts[(game.round - 1) as usize],
            rng.key(),
            GameError::RandomnessMismatch
        );
        require!(
            data.queue.as_ref() == config.queue.as_ref()
                && data.authority.as_ref() == config.authority.as_ref()
                && data.oracle.as_ref() == config.oracles[(game.round - 1) as usize].as_ref(),
            GameError::RandomnessMismatch
        );
        let due = config.round == game.round && !config.resolved;
        require!(due || config.resolved, GameError::DonkeyOracleInvalid);
        require!(
            remaining_accounts.len() == 1 + if due { config.ticket_count as usize } else { 0 },
            GameError::InvalidFactionSet
        );
        Some(config)
    } else {
        require!(remaining_accounts.is_empty(), GameError::InvalidFactionSet);
        None
    };
    let mut b = [0u8; 8];
    b.copy_from_slice(&value[..8]);
    let seed = u64::from_le_bytes(b);

    // Аудит 27.09 (S1): вывод карты — общее правило ядра, включая
    // особую карту «взаимозачёт» эпохи 90-х (раньше ветка жила только
    // в слот-хэш пути advance)
    let card = alashi_rules::transitions::reveal_law(game, seed)?;
    if let Some(config) = oracle.as_mut() {
        let due_count = if config.round == game.round && !config.resolved {
            config.ticket_count as usize
        } else {
            0
        };
        for (i, ticket) in config.tickets[..due_count].iter().enumerate() {
            let ai = &remaining_accounts[i + 1];
            require_keys_eq!(*ai.key, *ticket, GameError::InvalidFactionSet);
            require!(ai.is_writable, GameError::InvalidFactionSet);
            let mut faction = Account::<Faction>::try_from(ai)?;
            require_keys_eq!(faction.game, game.key(), GameError::InvalidFactionSet);
            let win = donkey_success(&value, &game.key(), ticket, game.round)?;
            let goods = alashi_rules::actions::donkey_risk_resolve(&mut faction, win)?;
            faction.exit(&crate::id())?;
            emit!(DonkeyResolved {
                game: game.key(),
                faction: *ticket,
                round: game.round,
                success: win,
                goods,
                fee: DONKEY_RISK_FEE,
                reason: 0,
            });
        }
        config.resolved = true;
        config.exit(&crate::id())?;
    }
    emit!(LawDrawn {
        game: game.key(),
        round: game.round,
        card,
    });
    Ok(())
}

/// Rejection sampling is exact: 2^256 - 1 is divisible by 5, so the only
/// rejected digest is all 0xff. A domain-separated rehash supplies the retry.
fn donkey_success(vrf: &[u8; 32], game: &Pubkey, faction: &Pubkey, round: u8) -> Result<bool> {
    let mut counter = 0u32;
    loop {
        let digest = hashv(&[
            b"alashi-donkey-risk-v1".as_slice(),
            vrf.as_slice(),
            game.as_ref(),
            faction.as_ref(),
            &[round],
            &counter.to_le_bytes(),
        ])
        .to_bytes();
        if let Some(success) = digest_success(&digest) {
            return Ok(success);
        }
        counter = counter
            .checked_add(1)
            .ok_or(GameError::RandomnessNotReady)?;
    }
}

fn digest_success(digest: &[u8; 32]) -> Option<bool> {
    if *digest == [0xff; 32] {
        return None;
    }
    let residue = digest.iter().fold(0u8, |r, b| {
        ((r as u16 * 256 + *b as u16) % DONKEY_RISK_OUTCOMES as u16) as u8
    });
    Some(residue < DONKEY_RISK_SUCCESS_COUNT)
}

#[cfg(test)]
mod donkey_rng_tests {
    use super::*;

    #[test]
    fn exact_three_of_five_and_rejection_boundary() {
        for (last, expected) in [(0, true), (1, true), (2, true), (3, false), (4, false)] {
            let mut digest = [0; 32];
            digest[31] = last;
            assert_eq!(digest_success(&digest), Some(expected));
        }
        let mut threshold_minus_one = [0xff; 32];
        threshold_minus_one[31] = 0xfe;
        assert_eq!(digest_success(&threshold_minus_one), Some(false));
        assert_eq!(digest_success(&[0xff; 32]), None);
    }

    #[test]
    fn same_inputs_are_stable_and_distinct_factions_are_domain_separated() {
        let vrf = [9; 32];
        let game = Pubkey::new_from_array([1; 32]);
        let a = Pubkey::new_from_array([2; 32]);
        let b = Pubkey::new_from_array([3; 32]);
        assert_eq!(
            donkey_success(&vrf, &game, &a, 2).unwrap(),
            donkey_success(&vrf, &game, &a, 2).unwrap()
        );
        let digest = |f: &Pubkey, round: u8| {
            hashv(&[
                b"alashi-donkey-risk-v1".as_slice(),
                vrf.as_slice(),
                game.as_ref(),
                f.as_ref(),
                &[round],
                &0u32.to_le_bytes(),
            ])
            .to_bytes()
        };
        assert_ne!(digest(&a, 2), digest(&b, 2));
        assert_ne!(digest(&a, 2), digest(&a, 3));
    }
}
