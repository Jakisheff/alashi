use crate::{constants::*, error::GameError, events::*, state::*};
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
            elect_president(game, &checked)?;
            draw_law(game, gkey, &ctx.accounts.hashes.to_account_info())?;
            game.veto_pending = false;
        }
        Phase::Law => {
            require!(now >= game.phase_ends_at, GameError::TooEarly);
            let mut yes: u32 = 0;
            let mut no: u32 = 0;
            for f in checked.iter() {
                if !f.alive || f.voted_stamp != stamp {
                    continue;
                }
                match f.vote {
                    VoteChoice::Yes => yes += f.influence as u32,
                    VoteChoice::No => no += f.influence as u32,
                    VoteChoice::Abstain => {}
                }
            }
            let voted_yes = yes > no;
            let vetoed = game.veto_pending && voted_yes;
            let passed = voted_yes && !game.veto_pending;
            game.yes_influence = yes;
            game.no_influence = no;
            game.last_law_passed = passed;
            if passed {
                game.laws_passed += 1;
                apply_law(game, &mut checked)?;
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

fn elect_president(game: &mut Game, factions: &[Account<'_, Faction>]) -> Result<()> {
    let mut best: Option<&Account<'_, Faction>> = None;
    for f in factions.iter() {
        if !f.alive {
            continue;
        }
        best = Some(match best {
            None => f,
            Some(b) => {
                if f.influence > b.influence || (f.influence == b.influence && f.wallet < b.wallet)
                {
                    f
                } else {
                    b
                }
            }
        });
    }
    if let Some(b) = best {
        game.president = b.wallet;
    }
    Ok(())
}

fn draw_law(game: &mut Game, gkey: Pubkey, slot_hashes: &AccountInfo) -> Result<()> {
    let seed = draw_seed(slot_hashes)?;
    if game.laws_used_mask == 0xFF {
        game.laws_used_mask = 0;
    }
    let mut idx = seed % DECK_SIZE as u64;
    while game.laws_used_mask & (1 << idx) != 0 {
        idx = (idx + 1) % DECK_SIZE as u64;
    }
    game.law_card = idx as u8;
    game.laws_used_mask |= 1 << idx;
    emit!(LawDrawn {
        game: gkey,
        round: game.round,
        card: game.law_card,
    });
    Ok(())
}

fn apply_law(game: &mut Game, factions: &mut [Account<'_, Faction>]) -> Result<()> {
    match game.law_card {
        LAW_TAX_10 => game.active_tax_bps = 1_000,
        LAW_TAX_20 => game.active_tax_bps = 2_000,
        LAW_SUBSIDY_PRODUCE => game.active_subsidy_goods = 1,
        LAW_SUBSIDY_POOR => {
            let mut best: Option<usize> = None;
            for (i, f) in factions.iter().enumerate() {
                if !f.alive {
                    continue;
                }
                best = Some(match best {
                    None => i,
                    Some(b) => {
                        if factions[i].cash < factions[b].cash
                            || (factions[i].cash == factions[b].cash
                                && factions[i].wallet < factions[b].wallet)
                        {
                            i
                        } else {
                            b
                        }
                    }
                });
            }
            if let Some(i) = best {
                factions[i].influence += 1;
            }
        }
        LAW_SUBSIDY_RICH => {
            let mut best: Option<usize> = None;
            for (i, f) in factions.iter().enumerate() {
                if !f.alive {
                    continue;
                }
                best = Some(match best {
                    None => i,
                    Some(b) => {
                        if factions[i].cash > factions[b].cash
                            || (factions[i].cash == factions[b].cash
                                && factions[i].wallet < factions[b].wallet)
                        {
                            i
                        } else {
                            b
                        }
                    }
                });
            }
            if let Some(i) = best {
                factions[i].influence += 1;
            }
        }
        LAW_EMBARGO => game.pending_price_shift = -2,
        LAW_BOOM => game.pending_boom = 2,
        _ => {}
    }
    Ok(())
}
