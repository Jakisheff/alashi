use crate::{
    constants::*,
    error::GameError,
    events::*,
    state::*,
};
use alashi_rules::transitions;
use anchor_lang::prelude::*;
use switchboard_on_demand::accounts::RandomnessAccountData;
use switchboard_on_demand::ON_DEMAND_DEVNET_PID;
use switchboard_on_demand::ON_DEMAND_MAINNET_PID;

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

pub fn handle_advance(ctx: Context<Advance>) -> Result<()> {
    let clock = Clock::get()?;
    let now = clock.unix_timestamp;
    let gkey = ctx.accounts.game.key();

    let game = &mut ctx.accounts.game;
    let wants_rng = game.phase == Phase::Action && game.entropy_mode == ENTROPY_SWITCHBOARD;
    let expected = game.faction_count as usize + usize::from(wants_rng);
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

    // энтропия фазы: слот-хэш для карты закона, VRF-коммит для режима 1
    let vrf_commit = if wants_rng {
        let rng_ai = ctx.remaining_accounts[game.faction_count as usize].clone();
        Some(verify_switchboard(&rng_ai, clock.slot)?)
    } else {
        None
    };
    let seed = if game.phase == Phase::Action && game.entropy_mode != ENTROPY_SWITCHBOARD {
        draw_seed(&ctx.accounts.hashes.to_account_info())?
    } else {
        0
    };

    // вся машина фаз (M1-M11 включительно) — в rules::transitions,
    // единый источник для ончейна и симулятора (replay байт-в-байт)
    let prev_phase = game.phase;
    let prev_card = game.law_card;
    let prev_round = game.round;
    let mut refs: Vec<&mut Faction> = checked.iter_mut().map(|a| &mut **a).collect();
    let res = transitions::advance(game, &mut refs, now, clock.slot, seed, vrf_commit)?;

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

    emit!(PhaseAdvanced {
        game: gkey,
        round: game.round,
        phase: game.phase,
    });
    Ok(())
}
