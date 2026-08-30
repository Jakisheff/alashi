use crate::{
    constants::*,
    error::GameError,
    events::*,
    logic::{compute_law_effect, draw_law_index, elect_president, tally_votes, FactionSnapshot},
    state::*,
};
use anchor_lang::prelude::*;

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

pub fn handle_advance(ctx: Context<Advance>) -> Result<()> {
    let clock = Clock::get()?;
    let now = clock.unix_timestamp;
    let gkey = ctx.accounts.game.key();

    let mut checked: Vec<Account<'_, Faction>> = ctx
        .remaining_accounts
        .iter()
        .map(|ai| Account::<Faction>::try_from(ai))
        .collect::<anchor_lang::Result<Vec<_>>>()?;

    let game = &mut ctx.accounts.game;
    require!(
        checked.len() == game.faction_count as usize,
        GameError::InvalidFactionSet
    );

    for (i, f) in checked.iter().enumerate() {
        require!(f.game == game.key(), GameError::InvalidFactionSet);
        for (j, g) in checked.iter().enumerate() {
            if i != j {
                require!(f.key() != g.key(), GameError::InvalidFactionSet);
            }
        }
    }

    let stamp = game.stamp();
    match game.phase {
        Phase::Lobby => {
            require!(
                game.faction_count >= MIN_FACTIONS,
                GameError::NotEnoughFactions
            );
            require!(
                now >= game.phase_ends_at || game.faction_count == MAX_FACTIONS,
                GameError::TooEarly
            );
            game.round = 1;
            game.phase = Phase::Market;
            game.law_card = NO_LAW;
            game.laws_used_mask = 0;
        }
        Phase::Market => {
            require!(now >= game.phase_ends_at, GameError::TooEarly);
            game.phase = Phase::Action;
        }
        Phase::Action => {
            require!(now >= game.phase_ends_at, GameError::TooEarly);
            game.phase = Phase::Law;
            let snapshots: Vec<FactionSnapshot> = checked.iter().map(|f| f.into()).collect();
            game.president = elect_president(&snapshots).unwrap_or_default();
            draw_law(game, gkey, &ctx.accounts.hashes.to_account_info())?;
            game.veto_pending = false;
        }
        Phase::Law => {
            require!(now >= game.phase_ends_at, GameError::TooEarly);
            let votes: Vec<(u16, VoteChoice)> = checked
                .iter()
                .filter(|f| f.alive && f.voted_stamp == stamp)
                .map(|f| (f.influence, f.vote))
                .collect();
            let (yes, no) = tally_votes(&votes);
            let voted_yes = yes > no;
            let vetoed = game.veto_pending && voted_yes;
            let passed = voted_yes && !game.veto_pending;
            game.yes_influence = yes;
            game.no_influence = no;
            game.last_law_passed = passed;
            if passed {
                game.laws_passed += 1;
                let snapshots: Vec<FactionSnapshot> = checked.iter().map(|f| f.into()).collect();
                let effect = compute_law_effect(game.law_card, &snapshots);
                if let Some(tax) = effect.tax_bps {
                    game.active_tax_bps = tax;
                }
                game.active_subsidy_goods = effect.subsidy_goods;
                game.pending_price_shift = effect.pending_price_shift;
                game.pending_boom = effect.pending_boom;
                if let Some(i) = effect.influence_gain {
                    checked[i].influence += 1;
                }
            }
            emit!(LawResult {
                game: gkey,
                round: game.round,
                yes,
                no,
                passed,
            });
            if vetoed {
                emit!(LawVetoed {
                    game: gkey,
                    round: game.round,
                    card: game.law_card,
                });
            }
            if game.round >= ROUNDS {
                game.phase = Phase::Finished;
            } else {
                game.round += 1;
                game.phase = Phase::Market;
            }
            game.law_card = NO_LAW;
            game.veto_pending = false;
        }
        Phase::Finished => return Err(GameError::GameFinished.into()),
    }

    if game.phase == Phase::Market {
        game.sold_this_round = 0;
        game.active_price_shift = game.pending_price_shift;
        game.active_boom = game.pending_boom;
        game.pending_price_shift = 0;
        game.pending_boom = 0;
    }
    game.phase_ends_at = now.saturating_add(game.phase_duration);

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

fn draw_law(game: &mut Game, gkey: Pubkey, slot_hashes: &AccountInfo) -> Result<()> {
    require_keys_eq!(
        slot_hashes.key(),
        solana_sysvar::slot_hashes::ID,
        GameError::InvalidSettleSet
    );
    let data = slot_hashes.data.borrow();
    if data.len() < 8 + 8 + 32 {
        return Err(GameError::NoSlotHashes.into());
    }
    let count = u64::from_le_bytes(data[0..8].try_into().unwrap()) as usize;
    if count == 0 {
        return Err(GameError::NoSlotHashes.into());
    }
    let first_hash = &data[16..48];
    let seed = u64::from_le_bytes(first_hash[..8].try_into().unwrap());
    drop(data);

    let (card, mask) = draw_law_index(seed, game.laws_used_mask);
    game.law_card = card;
    game.laws_used_mask = mask;
    emit!(LawDrawn {
        game: gkey,
        round: game.round,
        card,
    });
    Ok(())
}
