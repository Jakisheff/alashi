use crate::{
    constants::*,
    donkey_state::{oracle_queue, DonkeyOracleConfig},
    error::GameError,
    events::*,
    state::*,
};
use alashi_rules::transitions;
use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke_signed,
};
use switchboard_on_demand::accounts::RandomnessAccountData;
use switchboard_on_demand::ON_DEMAND_DEVNET_PID;
use switchboard_on_demand::ON_DEMAND_MAINNET_PID;

const RANDOMNESS_COMMIT_DISCRIMINATOR: [u8; 8] = [52, 170, 152, 201, 179, 133, 242, 141];

#[derive(Accounts)]
pub struct Advance<'info> {
    pub crank: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    /// CHECK: адрес проверяется вручную в draw_seed (require_keys_eq с slot_hashes::ID), данные парсятся без deserialization-by- type
    pub hashes: UncheckedAccount<'info>,
}

fn draw_seed(ai: &AccountInfo) -> Result<u64> {
    require_keys_eq!(
        ai.key(),
        solana_sysvar::slot_hashes::ID,
        GameError::InvalidSettleSet
    );
    let data = ai.data.borrow();
    if data.len() < 8 + 8 + 32 {
        return Err(GameError::NoSlotHashes.into());
    }
    let count = u64::from_le_bytes(data[0..8].try_into().unwrap()) as usize;
    if count == 0 {
        return Err(GameError::NoSlotHashes.into());
    }
    let first_hash = &data[16..48];
    Ok(u64::from_le_bytes(first_hash[..8].try_into().unwrap()))
}

/// Проверка владельца, нераскрытого значения и seed предыдущего слота.
/// Возвращает (ключ аккаунта, seed_slot) для transitions::advance.
fn verify_switchboard(rng: &AccountInfo, clock_slot: u64) -> Result<(Pubkey, u64)> {
    let owner = rng.owner;
    require!(
        owner.as_ref() == ON_DEMAND_DEVNET_PID.as_ref()
            || owner.as_ref() == ON_DEMAND_MAINNET_PID.as_ref(),
        GameError::RandomnessMismatch
    );
    let data = RandomnessAccountData::parse(rng.data.borrow())
        .map_err(|_| GameError::RandomnessNotReady)?;
    transitions::validate_vrf_seed_slot(data.seed_slot, clock_slot)?;
    require!(data.reveal_slot == 0, GameError::RandomnessNotReady);
    Ok((*rng.key, data.seed_slot))
}

fn commit_donkey_round_rng<'info>(
    rng: &AccountInfo<'info>,
    queue: &AccountInfo<'info>,
    selected_oracle: &AccountInfo<'info>,
    provider: &AccountInfo<'info>,
    hashes: &AccountInfo<'info>,
    oracle: &Account<'info, DonkeyOracleConfig>,
    game_key: Pubkey,
    round: u8,
    clock_slot: u64,
) -> Result<(Pubkey, u64)> {
    require!(round > 0 && round <= ROUNDS, GameError::DonkeyOracleInvalid);
    let index = (round - 1) as usize;
    require!(
        oracle.committed_rounds_mask & (1 << index) == 0,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        oracle.accounts[index],
        *rng.key,
        GameError::RandomnessMismatch
    );
    require_keys_eq!(*queue.key, oracle.queue, GameError::DonkeyOracleInvalid);
    require_keys_eq!(
        *selected_oracle.key,
        oracle.oracles[index],
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        *provider.key,
        Pubkey::new_from_array(ON_DEMAND_DEVNET_PID.to_bytes()),
        GameError::DonkeyOracleInvalid
    );
    require!(
        provider.executable && rng.is_writable && selected_oracle.is_writable,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(*rng.owner, *provider.key, GameError::RandomnessMismatch);
    require_keys_eq!(*queue.owner, *provider.key, GameError::DonkeyOracleInvalid);
    require_keys_eq!(
        *selected_oracle.owner,
        *provider.key,
        GameError::DonkeyOracleInvalid
    );
    require_keys_eq!(
        *hashes.key,
        solana_sysvar::slot_hashes::ID,
        GameError::InvalidSettleSet
    );
    require_keys_eq!(
        oracle_queue(selected_oracle)?,
        oracle.queue,
        GameError::DonkeyOracleInvalid
    );
    let before = RandomnessAccountData::parse(rng.data.borrow())
        .map_err(|_| GameError::RandomnessMismatch)?;
    require!(
        before.seed_slot == 0 && before.reveal_slot == 0,
        GameError::DonkeyOracleInvalid
    );
    require!(
        before.queue.as_ref() == oracle.queue.as_ref()
            && before.authority.as_ref() == oracle.key().as_ref(),
        GameError::RandomnessMismatch
    );
    require_keys_eq!(
        oracle.authority,
        oracle.key(),
        GameError::DonkeyOracleInvalid
    );
    // `parse` holds a Ref into source data; the provider needs a mutable
    // borrow of the same account during CPI.
    drop(before);
    let ix = Instruction {
        program_id: *provider.key,
        accounts: vec![
            AccountMeta::new(*rng.key, false),
            AccountMeta::new_readonly(*queue.key, false),
            AccountMeta::new(*selected_oracle.key, false),
            AccountMeta::new_readonly(*hashes.key, false),
            AccountMeta::new_readonly(oracle.key(), true),
        ],
        data: RANDOMNESS_COMMIT_DISCRIMINATOR.to_vec(),
    };
    let bump = [oracle.bump];
    let seeds: &[&[u8]] = &[DONKEY_ORACLE_SEED, game_key.as_ref(), &bump];
    invoke_signed(
        &ix,
        &[
            rng.clone(),
            queue.clone(),
            selected_oracle.clone(),
            hashes.clone(),
            oracle.to_account_info(),
            provider.clone(),
        ],
        &[seeds],
    )?;
    let after = RandomnessAccountData::parse(rng.data.borrow())
        .map_err(|_| GameError::RandomnessMismatch)?;
    transitions::validate_donkey_commit_slot(after.seed_slot, clock_slot)?;
    require!(after.reveal_slot == 0, GameError::RandomnessNotReady);
    require!(
        after.queue.as_ref() == oracle.queue.as_ref()
            && after.authority.as_ref() == oracle.key().as_ref()
            && after.oracle.as_ref() == selected_oracle.key.as_ref(),
        GameError::RandomnessMismatch
    );
    require!(
        after.seed_slot > oracle.max_attempt_slot,
        GameError::RandomnessNotReady
    );
    Ok((*rng.key, after.seed_slot))
}

fn needs_donkey_config(game: &Game) -> bool {
    game.epoch == EPOCH_DONKEY_RISK
        && !(game.phase == Phase::Lobby && game.faction_count < MIN_FACTIONS)
}

pub fn handle_advance<'info>(ctx: Context<'info, Advance<'info>>) -> Result<()> {
    let clock = Clock::get()?;
    let now = clock.unix_timestamp;
    let gkey = ctx.accounts.game.key();

    let game = &mut ctx.accounts.game;
    let past_commit_grace = game.epoch == EPOCH_DONKEY_RISK
        && game.phase == Phase::Action
        && game.entropy_mode == ENTROPY_SWITCHBOARD
        && now
            >= game
                .phase_ends_at
                .saturating_add(DONKEY_COMMIT_GRACE_SECONDS);
    let can_abort_uncommitted =
        past_commit_grace && ctx.remaining_accounts.len() == game.faction_count as usize + 1;
    let wants_rng = game.phase == Phase::Action
        && game.entropy_mode == ENTROPY_SWITCHBOARD
        && !past_commit_grace;
    // A Lobby with too few factions must still be able to abort if an admin
    // never configured the oracle. It cannot enter Market without the PDA.
    let needs_config = needs_donkey_config(game);
    let epoch2_commit = wants_rng && game.epoch == EPOCH_DONKEY_RISK;
    // Epoch 2 commits inside this instruction: configured source, queue,
    // preselected oracle, and provider executable follow the config PDA.
    let expected = game.faction_count as usize
        + usize::from(needs_config)
        + usize::from(wants_rng)
        + if epoch2_commit { 3 } else { 0 };
    require!(
        ctx.remaining_accounts.len() == expected,
        GameError::InvalidFactionSet
    );

    let mut checked: Vec<Account<'_, Faction>> = ctx.remaining_accounts
        [..game.faction_count as usize]
        .iter()
        .map(|ai| Account::<Faction>::try_from(ai))
        .collect::<anchor_lang::Result<Vec<_>>>()?;

    for (i, f) in checked.iter().enumerate() {
        require!(f.game == game.key(), GameError::InvalidFactionSet);
        for (j, g) in checked.iter().enumerate() {
            if i != j {
                require!(f.key() != g.key(), GameError::InvalidFactionSet);
            }
        }
    }

    let mut oracle = if needs_config {
        let ai = &ctx.remaining_accounts[game.faction_count as usize];
        require!(ai.is_writable, GameError::DonkeyOracleInvalid);
        let config = Account::<DonkeyOracleConfig>::try_from(ai)?;
        let expected_pda =
            Pubkey::find_program_address(&[DONKEY_ORACLE_SEED, gkey.as_ref()], &crate::id()).0;
        require_keys_eq!(config.key(), expected_pda, GameError::DonkeyOracleInvalid);
        require_keys_eq!(config.game, gkey, GameError::DonkeyOracleInvalid);
        require!(
            game.entropy_mode == ENTROPY_SWITCHBOARD,
            GameError::DonkeyOracleInvalid
        );
        Some(config)
    } else {
        None
    };
    if let Some(config) = oracle.as_ref() {
        if config.round == game.round && !config.resolved {
            for ticket in &config.tickets[..config.ticket_count as usize] {
                require!(
                    checked.iter().any(|f| f.key() == *ticket),
                    GameError::InvalidFactionSet
                );
            }
        }
        if game.phase == Phase::Law && game.law_card != NO_LAW {
            require!(
                config.resolved || config.ticket_count == 0,
                GameError::DonkeyOracleInvalid
            );
        }
    }

    // энтропия фазы: VRF-коммит для карты закона в режиме 1
    let vrf_commit = if wants_rng {
        let source_index = game.faction_count as usize + usize::from(needs_config);
        let rng_ai = ctx.remaining_accounts[source_index].clone();
        Some(if epoch2_commit {
            let config = oracle.as_ref().ok_or(GameError::DonkeyOracleInvalid)?;
            commit_donkey_round_rng(
                &rng_ai,
                &ctx.remaining_accounts[source_index + 1],
                &ctx.remaining_accounts[source_index + 2],
                &ctx.remaining_accounts[source_index + 3],
                &ctx.accounts.hashes.to_account_info(),
                config,
                gkey,
                game.round,
                clock.slot,
            )?
        } else {
            verify_switchboard(&rng_ai, clock.slot)?
        })
    } else {
        None
    };
    // Аудит 27.09 (S1): случайность — явный вход каждого игрового
    // события. Экономические эффекты (доход лицензии, беспредел крыш,
    // таможня) всегда получают слот-хэш; ноль больше не обозначает
    // «нет данных». Карта закона в режиме VRF раскрывается только
    // проверенным reveal.
    let seed = draw_seed(&ctx.accounts.hashes.to_account_info())?;

    // вся машина фаз (M1-M11 включительно) — в rules::transitions,
    // единый источник для ончейна и симулятора (replay байт-в-байт)
    let prev_phase = game.phase;
    let prev_card = game.law_card;
    let prev_round = game.round;
    let mut refs: Vec<&mut Faction> = checked.iter_mut().map(|a| &mut **a).collect();
    let res = if can_abort_uncommitted {
        transitions::abort_uncommitted_donkey(game, now)?
    } else {
        transitions::advance(game, &mut refs, now, clock.slot, seed, vrf_commit)?
    };
    if epoch2_commit && res.committed_vrf.is_some() {
        let config = oracle.as_mut().ok_or(GameError::DonkeyOracleInvalid)?;
        config.committed_rounds_mask |= 1 << (prev_round - 1);
    }

    // события из результата перехода
    if let Some((randomness, commit_slot)) = res.committed_vrf {
        emit!(LawCommitted {
            game: gkey,
            randomness,
            commit_slot,
        });
    }
    if let Some(card) = res.law_card_drawn {
        emit!(LawDrawn {
            game: gkey,
            round: game.round,
            card,
        });
    }
    if let Some(attempt) = res.retried {
        emit!(VrfRetry {
            game: gkey,
            round: game.round,
            attempt,
        });
    }
    if res.aborted {
        if let Some(config) = oracle.as_mut() {
            if config.round == prev_round && !config.resolved {
                for ticket in &config.tickets[..config.ticket_count as usize] {
                    emit!(DonkeyResolved {
                        game: gkey,
                        faction: *ticket,
                        round: prev_round,
                        success: false,
                        goods: 0,
                        fee: DONKEY_RISK_FEE,
                        reason: 1,
                    });
                }
                config.resolved = true;
            }
            config.exit(&crate::id())?;
        }
        for f in checked.iter_mut() {
            f.exit(&crate::id())?;
        }
        emit!(GameAbortedEvent {
            game: gkey,
            round: game.round,
        });
        return Ok(());
    }
    if prev_phase == Phase::Law && res.retried.is_none() {
        // подсчёт закона состоялся в transitions (веса — из Game)
        emit!(LawResult {
            game: gkey,
            round: prev_round,
            yes: game.yes_influence,
            no: game.no_influence,
            passed: game.last_law_passed,
        });
        if res.vetoed {
            emit!(LawVetoed {
                game: gkey,
                round: prev_round,
                card: prev_card,
            });
        }
    }

    for f in checked.iter_mut() {
        f.exit(&crate::id())?;
    }
    if let Some(config) = oracle.as_mut() {
        config.exit(&crate::id())?;
    }

    emit!(PhaseAdvanced {
        game: gkey,
        round: game.round,
        phase: game.phase,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_epoch2_lobby_can_abort_but_cannot_enter_market_without_config() {
        let mut game = Game::default();
        game.epoch = EPOCH_DONKEY_RISK;
        game.phase = Phase::Lobby;
        game.faction_count = MIN_FACTIONS - 1;
        assert!(!needs_donkey_config(&game));
        game.faction_count = MIN_FACTIONS;
        assert!(needs_donkey_config(&game));
        game.phase = Phase::Action;
        assert!(needs_donkey_config(&game));
    }
}
