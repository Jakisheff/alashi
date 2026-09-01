//! Чистая машина переходов фаз: та же логика, что исполняет on-chain
//! advance, но без аккаунтов, событий и sysvar. Перенос из
//! instructions/advance.rs (ARCH L1: переходы — часть правил).

use crate::constants::*;
use crate::error::GameError;
use crate::logic::{compute_law_effect, draw_law_index, tally_votes, FactionSnapshot};
use crate::state::{Faction, Game, Phase, VoteChoice};
use anchor_lang::prelude::Pubkey;

pub struct AdvanceResult {
    pub law_card_drawn: Option<u8>,
    pub vetoed: bool,
    pub aborted: bool,
    pub retried: Option<u8>,
    pub committed_vrf: Option<(Pubkey, u64)>,
}

pub fn advance(
    game: &mut Game,
    factions: &mut [Faction],
    now: i64,
    seed: u64,
) -> Result<AdvanceResult, GameError> {
    advance_inner(game, factions, now, None, seed)
}

/// Реплей из событий: карта известна из LawDrawn, seed не нужен.
#[allow(clippy::too_many_arguments)]
pub fn advance_with_card(
    game: &mut Game,
    factions: &mut [Faction],
    now: i64,
    card: u8,
) -> Result<AdvanceResult, GameError> {
    advance_inner(game, factions, now, Some(card), 0)
}

fn advance_inner(
    game: &mut Game,
    factions: &mut [Faction],
    now: i64,
    forced_card: Option<u8>,
    seed: u64,
) -> Result<AdvanceResult, GameError> {
    let mut res = AdvanceResult {
        law_card_drawn: None,
        vetoed: false,
        aborted: false,
        retried: None,
        committed_vrf: None,
    };
    let stamp = game.stamp();
    match game.phase {
        Phase::Lobby => {
            if game.faction_count < MIN_FACTIONS {
                return Err(GameError::NotEnoughFactions);
            }
            if !(now >= game.phase_ends_at || game.faction_count == MAX_FACTIONS) {
                return Err(GameError::TooEarly);
            }
            game.round = 1;
            game.phase = Phase::Market;
            game.law_card = NO_LAW;
            game.laws_used_mask = 0;
        }
        Phase::Market => {
            if now < game.phase_ends_at {
                return Err(GameError::TooEarly);
            }
            game.phase = Phase::Action;
        }
        Phase::Action => {
            if now < game.phase_ends_at {
                return Err(GameError::TooEarly);
            }
            game.phase = Phase::Law;
            let president = elect_president_factions(factions);
            game.president = president;
            if game.entropy_mode == ENTROPY_SWITCHBOARD {
                game.law_card = NO_LAW;
                res.committed_vrf = Some((game.vrf_account, game.commit_slot));
            } else {
                let (card, mask) = match forced_card {
                    Some(c) => {
                        let mut m = game.laws_used_mask;
                        if m == 0xFF {
                            m = 0;
                        }
                        m |= 1 << (c % 8);
                        (c, m)
                    }
                    None => draw_law_index(seed, game.laws_used_mask),
                };
                game.law_card = card;
                game.laws_used_mask = mask;
                res.law_card_drawn = Some(card);
            }
            game.veto_pending = false;
        }
        Phase::Law => {
            if now < game.phase_ends_at {
                return Err(GameError::TooEarly);
            }
            if game.entropy_mode == ENTROPY_SWITCHBOARD && game.law_card == NO_LAW {
                return Err(GameError::LawNotRevealed);
            }
            let votes: Vec<(u16, VoteChoice)> = factions
                .iter()
                .filter(|f| f.alive && f.voted_stamp == stamp)
                .map(|f| {
                    let mut weight = f.influence;
                    if game.vote_weight_mode == VOTE_WEIGHT_CONTRIB {
                        let action_stamp = ((game.round as u16) << 3) | Phase::Action as u16;
                        if f.acted_stamp != action_stamp {
                            // взнос-как-голос: пропуск Action = вес на этом законе
                            weight += SKIP_VOTE_WEIGHT;
                        }
                    }
                    (weight, f.vote)
                })
                .collect();
            let (yes, no) = tally_votes(&votes);
            let voted_yes = yes > no;
            res.vetoed = game.veto_pending && voted_yes;
            let passed = voted_yes && !game.veto_pending;
            game.yes_influence = yes;
            game.no_influence = no;
            game.last_law_passed = passed;
            if passed {
                game.laws_passed += 1;
                let snaps: Vec<FactionSnapshot> = factions
                    .iter()
                    .map(|f| FactionSnapshot {
                        wallet: f.wallet,
                        cash: f.cash,
                        influence: f.influence,
                        alive: f.alive,
                    })
                    .collect();
                let effect = compute_law_effect(game.law_card, &snaps);
                if let Some(tax) = effect.tax_bps {
                    game.active_tax_bps = tax;
                }
                game.active_subsidy_goods = effect.subsidy_goods;
                game.pending_price_shift = effect.pending_price_shift;
                game.pending_boom = effect.pending_boom;
                if let Some(i) = effect.influence_gain {
                    factions[i].influence += 1;
                }
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
        Phase::Finished => return Err(GameError::GameFinished),
        Phase::Aborted => return Err(GameError::GameAborted),
    }

    if game.phase == Phase::Market {
        game.sold_this_round = 0;
        game.active_price_shift = game.pending_price_shift;
        game.active_boom = game.pending_boom;
        game.pending_price_shift = 0;
        game.pending_boom = 0;
    }
    game.phase_ends_at = now.saturating_add(game.phase_duration);
    Ok(res)
}

fn elect_president_factions(factions: &[Faction]) -> Pubkey {
    factions
        .iter()
        .filter(|f| f.alive)
        .max_by(|a, b| {
            if a.influence != b.influence {
                a.influence.cmp(&b.influence)
            } else {
                b.wallet.cmp(&a.wallet)
            }
        })
        .map(|f| f.wallet)
        .unwrap_or_default()
}

pub fn reveal_law_card(game: &mut Game, card: u8) -> Result<u8, GameError> {
    if game.phase != Phase::Law {
        return Err(GameError::WrongPhase);
    }
    if game.law_card != NO_LAW {
        return Err(GameError::LawNotRevealed);
    }
    let mut mask = game.laws_used_mask;
    if mask == 0xFF {
        mask = 0;
    }
    mask |= 1 << (card % 8);
    game.law_card = card;
    game.laws_used_mask = mask;
    Ok(card)
}

pub fn reveal_law(game: &mut Game, seed: u64) -> Result<u8, GameError> {
    if game.entropy_mode != ENTROPY_SWITCHBOARD {
        return Err(GameError::InvalidEntropyMode);
    }
    if game.phase != Phase::Law {
        return Err(GameError::WrongPhase);
    }
    if game.law_card != NO_LAW {
        return Err(GameError::LawNotRevealed);
    }
    let (card, mask) = draw_law_index(seed, game.laws_used_mask);
    game.law_card = card;
    game.laws_used_mask = mask;
    Ok(card)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn factions2() -> (Game, Vec<Faction>) {
        let mut g = Game::default();
        g.round = 1;
        g.phase = Phase::Law;
        g.law_card = LAW_TAX_10;
        let law_stamp = g.stamp();
        let mut f1 = Faction::default();
        f1.wallet = Pubkey::new_from_array([1; 32]);
        f1.alive = true;
        f1.influence = 1;
        f1.vote = VoteChoice::Yes;
        f1.voted_stamp = law_stamp;
        f1.acted_stamp = ((1u16) << 3) | Phase::Action as u16; // действовал
        let mut f2 = Faction::default();
        f2.wallet = Pubkey::new_from_array([2; 32]);
        f2.alive = true;
        f2.influence = 1;
        f2.vote = VoteChoice::No;
        f2.voted_stamp = law_stamp;
        f2.acted_stamp = 0; // пропустил Action
        (g, vec![f1, f2])
    }

    #[test]
    fn legacy_tally_ignores_skip() {
        let (mut g, mut fs) = factions2();
        g.vote_weight_mode = VOTE_WEIGHT_LEGACY;
        g.phase_ends_at = 10;
        let res = advance_inner(&mut g, &mut fs, 20, None, 7).unwrap();
        // yes(1) > no(1)? нет, равенство → закон не прошёл в обоих случаях ниже
        // legacy: да 1 против нет 1 → не прошло
        assert!(!res.vetoed);
        assert!(!g.last_law_passed);
        assert_eq!((g.yes_influence, g.no_influence), (1, 1));
    }

    #[test]
    fn contribution_skip_adds_weight() {
        let (mut g, mut fs) = factions2();
        g.vote_weight_mode = VOTE_WEIGHT_CONTRIB;
        g.phase_ends_at = 10;
        let _ = advance_inner(&mut g, &mut fs, 20, None, 7).unwrap();
        // no(1+2=3) > yes(1) → не прошло, и вес виден в счётчиках
        assert_eq!((g.yes_influence, g.no_influence), (1, 3));
        assert!(!g.last_law_passed);
    }

    #[test]
    fn contribution_no_vote_no_bonus() {
        // пропуск без голоса не участвует в подсчёте
        let (mut g, mut fs) = factions2();
        g.vote_weight_mode = VOTE_WEIGHT_CONTRIB;
        g.phase_ends_at = 10;
        fs[1].voted_stamp = 0; // не голосовал
        let _ = advance_inner(&mut g, &mut fs, 20, None, 7).unwrap();
        assert_eq!((g.yes_influence, g.no_influence), (1, 0));
        assert!(g.last_law_passed);
    }
}
