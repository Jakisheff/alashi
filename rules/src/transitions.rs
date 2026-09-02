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
    /// SPEC_EPOCH_90S: таможня изъяла серой товар в этом раунде.
    pub customs_seized: bool,
    /// крыша погасила анти-богатый закон (контракт сгорел).
    pub roof_blocked: bool,
    /// «взаимозачёт» сжёг векселя.
    pub amnesty_burned: bool,
    /// сколько песо съела девальвация при входе в новый раунд.
    pub depreciation_burned: u64,
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
        customs_seized: false,
        roof_blocked: false,
        amnesty_burned: false,
        depreciation_burned: 0,
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
            // M9: доход лицензии фиксируется сидом входа в раунд аукциона,
            // инсайдеры могут узнать его до ставок
            if game.epoch == EPOCH_90S
                && game.round == AUCTION_ROUND
                && game.license_yield == 0
            {
                game.license_yield = LICENSE_MIN_YIELD + (seed % LICENSE_YIELD_SPAN);
            }
        }
        Phase::Action => {
            if now < game.phase_ends_at {
                return Err(GameError::TooEarly);
            }
            game.phase = Phase::Law;
            let president = elect_president_factions(factions);
            game.president = president;
            if game.epoch == EPOCH_90S {
                // SPEC_EPOCH_90S M3+M7+M8: граница раунда.
                // Президент выбрал режим вслепую (customs_tight, M8);
                // при отсутствии решения — старый RNG-режим M3.
                let mayhem = ((seed >> 24) & 0xFF) < RED_MAYHEM_THRESHOLD;
                let rng_seizure = ((seed >> 8) & 0xFF) < CUSTOMS_THRESHOLD;
                let tight = if game.customs_decided {
                    game.customs_tight
                } else {
                    rng_seizure
                };
                let pres_idx = factions
                    .iter()
                    .position(|f| f.wallet == game.president);
                let mut tribute_total: u64 = 0;
                for f in factions.iter_mut() {
                    if f.grey_goods == 0 && f.roof_tariff != ROOF_RED {
                        continue;
                    }
                    // M7 чёрная крыша: полная гарантия, граница не страшна
                    if f.roof_tariff == ROOF_BLACK {
                        f.grey_goods = 0;
                        continue;
                    }
                    // M7 красная крыша: p≈25% беспредела — горит весь товар
                    if f.roof_tariff == ROOF_RED && mayhem && f.goods > 0 {
                        f.goods = 0;
                        f.grey_goods = 0;
                        res.customs_seized = true;
                        continue;
                    }
                    if f.grey_goods > 0 {
                        if tight {
                            f.goods = f.goods.saturating_sub(f.grey_goods);
                            res.customs_seized = true;
                        } else {
                            // M8 льготная граница: дань президенту с серого хода
                            let pay = f.cash.min(CUSTOMS_TRIBUTE);
                            f.cash -= pay;
                            tribute_total += pay;
                        }
                        f.grey_goods = 0;
                    }
                }
                if let Some(pi) = pres_idx {
                    factions[pi].cash += tribute_total;
                }
                // M9 слепой аукцион: раунд аукциона, вскрытие ставок
                if game.round == AUCTION_ROUND && !game.license_sold {
                    let mut best: Option<usize> = None;
                    let mut pot: u64 = 0;
                    for (i, f) in factions.iter().enumerate() {
                        if f.bid > 0 {
                            pot += f.bid;
                        }
                        if f.bid > 0 && best.map_or(true, |b| f.bid > factions[b].bid) {
                            best = Some(i);
                        }
                    }
                    // платит только победитель, остальным возврат
                    for (i, f) in factions.iter_mut().enumerate() {
                        if Some(i) == best {
                            game.prize_pot += f.bid;
                        } else if f.bid > 0 {
                            f.cash += f.bid;
                        }
                        f.bid = 0;
                    }
                    if let Some(b) = best {
                        game.license_holder = b as u8;
                        game.license_sold = true;
                    }
                }
            }
            for f in factions.iter_mut() {
                f.grey_goods = 0;
            }
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
                // SPEC_EPOCH_90S M4: карта «взаимозачёт» (id 8) — раз за
                // партию, бит сида, только в эпохе 90-х
                let card = if game.epoch == EPOCH_90S
                    && !game.amnesty_used
                    && card != LAW_AMNESTY
                    && ((seed >> 16) & 1) == 1
                {
                    game.amnesty_used = true;
                    LAW_AMNESTY
                } else {
                    card
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
                    // M10 скупка голосов: голос проданного идёт по выбору
                    // покупателя, если покупатель проголосовал
                    let choice = if game.epoch == EPOCH_90S && f.vote_sold {
                        let buyer = &factions[f.vote_sold_to as usize];
                        if buyer.voted_stamp == stamp && buyer.alive {
                            buyer.vote
                        } else {
                            return None; // сделка не сработала, продавец молчит
                        }
                    } else {
                        f.vote
                    };
                    Some((weight, choice))
                })
                .flatten()
                .collect();
            let (yes, no) = tally_votes(&votes);
            let voted_yes = yes > no;
            res.vetoed = game.veto_pending && voted_yes;
            let mut passed = voted_yes && !game.veto_pending;
            // SPEC_EPOCH_90S M2: крыша гасит первый анти-богатый закон
            // против хозяина (он богатейший), контракт сгорает
            if passed
                && game.epoch == EPOCH_90S
                && (game.law_card == ANTI_RICH_TAX10 || game.law_card == ANTI_RICH_TAX20)
            {
                let rich = factions
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.alive)
                    .max_by_key(|(_, f)| (f.cash, f.wallet))
                    .map(|(i, _)| i);
                if let Some(ri) = rich {
                    if factions[ri].roof_armed {
                        passed = false;
                        res.roof_blocked = true;
                        factions[ri].roof_armed = false;
                    }
                }
            }
            game.yes_influence = yes;
            game.no_influence = no;
            game.last_law_passed = passed;
            if passed {
                game.laws_passed += 1;
                if game.epoch == EPOCH_90S && game.law_card == LAW_AMNESTY {
                    // M4 «взаимозачёт»: непогашенные векселя сгорают
                    game.amnesty_used = true;
                    for f in factions.iter_mut() {
                        if f.promissory > 0 {
                            f.promissory = 0;
                            res.amnesty_burned = true;
                        }
                    }
                }
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
                if game.epoch == EPOCH_90S {
                    // M1 девальвация: кэш ×0.85 в начале каждого раунда
                    // (твёрдая валюта M6 не девальвирует)
                    for f in factions.iter_mut() {
                        let before = f.cash;
                        f.cash = f.cash / DEPRECIATION_DEN * DEPRECIATION_NUM;
                        res.depreciation_burned += before - f.cash;
                        // M4: гашение векселей в кэш
                        if f.promissory > 0 {
                            f.cash += f.promissory;
                            f.promissory = 0;
                        }
                    }
                }
            }
            game.law_card = NO_LAW;
            game.veto_pending = false;
            // M8/M10: сброс посюраундовых флагов
            game.customs_decided = false;
            game.customs_tight = false;
            for f in factions.iter_mut() {
                f.vote_sold = false;
                f.vote_sold_to = 0;
                f.vote_offer_to = VOTE_OFFER_NONE;
                f.vote_offer_price = 0;
            }
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

    // ---------- SPEC_EPOCH_90S ----------

    #[test]
    fn m1_depreciation_burns_cash_not_goods() {
        // r1 закон закрыт → вход в r2: в 90s кэш ×0.85, товар цел
        let (mut g, mut fs) = factions2();
        g.epoch = EPOCH_90S;
        g.phase_ends_at = 10;
        fs[0].cash = 100 * PESO;
        fs[0].goods = 3;
        let res = advance_inner(&mut g, &mut fs, 20, None, 7).unwrap();
        assert_eq!(g.round, 2);
        assert_eq!(fs[0].cash, 85 * PESO);
        assert_eq!(fs[0].goods, 3);
        assert_eq!(res.depreciation_burned, 15 * PESO);
        // classic: без изменений
        let (mut g, mut fs) = factions2();
        g.epoch = EPOCH_CLASSIC;
        g.phase_ends_at = 10;
        fs[0].cash = 100 * PESO;
        let _ = advance_inner(&mut g, &mut fs, 20, None, 7).unwrap();
        assert_eq!(fs[0].cash, 100 * PESO);
    }

    #[test]
    fn m2_roof_blocks_first_anti_rich_law() {
        // богатейший проголосовал ПРОТИВ, закон прошёл бы 1:1? нет:
        // даём перевес ЗА через влияние, крыша гасит
        let (mut g, mut fs) = factions2();
        g.epoch = EPOCH_90S;
        g.phase_ends_at = 10;
        fs[1].cash = 90 * PESO; // богатейший — против
        fs[0].cash = 10 * PESO;
        fs[0].influence = 3; // да 3 против нет 1: прошёл бы
        fs[1].roof_armed = true; // крыша куплена
        fs[1].roof_to = 0;
        let res = advance_inner(&mut g, &mut fs, 20, None, 7).unwrap();
        assert!(res.roof_blocked);
        assert!(!g.last_law_passed, "закон должен быть погашен крышей");
        assert!(!fs[1].roof_armed, "контракт сгорает");
        // без крыши тот же расклад проходит
        let (mut g, mut fs) = factions2();
        g.epoch = EPOCH_90S;
        g.phase_ends_at = 10;
        fs[0].influence = 3;
        let res = advance_inner(&mut g, &mut fs, 20, None, 7).unwrap();
        assert!(!res.roof_blocked);
        assert!(g.last_law_passed);
    }

    #[test]
    fn m3_customs_seizes_grey_goods() {
        // закрытие Action с серым ходом: seed с (seed>>8)&0xFF < 64 → изъятие
        let mut g = Game::default();
        g.epoch = EPOCH_90S;
        g.round = 1;
        g.phase = Phase::Action;
        g.phase_ends_at = 10;
        let mut f = Faction::default();
        f.alive = true;
        f.goods = 5;
        f.grey_goods = 3; // 3 из 5 — серые
        let fs = vec![f];
        // seed: (seed>>8)&0xFF = 0x10 < 64
        let seed = 0x10_00;
        let mut fs = fs;
        let res = advance_inner(&mut g, &mut fs, 20, None, seed).unwrap();
        assert!(res.customs_seized);
        assert_eq!(fs[0].goods, 2, "серой товар изъят, легальный цел");
        // seed с байтом >= 64: таможня спит
        let mut g = Game::default();
        g.epoch = EPOCH_90S;
        g.round = 1;
        g.phase = Phase::Action;
        g.phase_ends_at = 10;
        let mut f = Faction::default();
        f.alive = true;
        f.goods = 5;
        f.grey_goods = 3;
        let mut fs = vec![f];
        let seed = 0x80_00; // (>>8)&0xFF = 0x80 = 128 >= 64
        let res = advance_inner(&mut g, &mut fs, 20, None, seed).unwrap();
        assert!(!res.customs_seized);
        assert_eq!(fs[0].goods, 5);
        assert_eq!(fs[0].grey_goods, 0, "маркер сбрасывается в любом случае");
    }

    #[test]
    fn m4_amnesty_burns_promissory_and_redemption_pays() {
        // карта «взаимозачёт» (8): сжигает векселя до гашения
        let mut g = Game::default();
        g.epoch = EPOCH_90S;
        g.round = 2;
        g.phase = Phase::Law;
        g.law_card = LAW_AMNESTY;
        g.phase_ends_at = 10;
        let stamp = g.stamp();
        let mut f = Faction::default();
        f.alive = true;
        f.influence = 1;
        f.vote = VoteChoice::Yes;
        f.voted_stamp = stamp;
        f.promissory = 50 * PESO;
        f.cash = 10 * PESO;
        let mut fs = vec![f];
        let res = advance_inner(&mut g, &mut fs, 20, Some(LAW_AMNESTY), 0).unwrap();
        assert!(res.amnesty_burned);
        assert_eq!(fs[0].promissory, 0);
        // вексель сгорел, кэш прошёл девальвацию нового раунда: 10M × 0.85
        assert_eq!(fs[0].cash, 10 * PESO / 100 * 85);
        assert!(g.amnesty_used, "карта одноразовая");
        // обычный закон: вексель гасится при входе в новый раунд
        let mut g = Game::default();
        g.epoch = EPOCH_90S;
        g.round = 1;
        g.phase = Phase::Law;
        g.law_card = LAW_STATUS_QUO;
        g.phase_ends_at = 10;
        let stamp = g.stamp();
        let mut f = Faction::default();
        f.alive = true;
        f.influence = 1;
        f.vote = VoteChoice::Yes;
        f.voted_stamp = stamp;
        f.promissory = 40 * PESO;
        f.cash = 10 * PESO;
        let mut fs = vec![f];
        let _ = advance_inner(&mut g, &mut fs, 20, Some(LAW_STATUS_QUO), 0).unwrap();
        assert_eq!(fs[0].promissory, 0);
        assert_eq!(fs[0].cash, 10 * PESO / 100 * 85 + 40 * PESO, "девальвация, затем гашение");
    }

    #[test]
    fn m6_exchanger_spread_and_hard_survives_depreciation() {
        use crate::sim::Simulator;
        let mut sim = Simulator::new(1, 10 * PESO, 0, 0);
        sim.game.epoch = EPOCH_90S;
        sim.join(Pubkey::new_from_array([9; 32]), "A").unwrap();
        sim.join(Pubkey::new_from_array([8; 32]), "B").unwrap();
        sim.factions[0].cash = 100 * PESO;
        assert!(sim.exchange(0, true).is_err(), "в лобби обмен закрыт");
        sim.advance(100, 0).unwrap();
        let got = sim.exchange(0, true).unwrap();
        assert_eq!(got, 80 * PESO, "спред 20%");
        assert_eq!((sim.factions[0].cash, sim.factions[0].hard), (0, 80 * PESO));
        let back = sim.exchange(0, false).unwrap();
        assert_eq!(back, 64 * PESO, "обратный обмен ещё минус 20%");
        sim.factions[0].cash = 100 * PESO;
        sim.exchange(0, true).unwrap();
        sim.advance(200, 1).unwrap();
        sim.advance(300, 2).unwrap();
        sim.advance(400, 3).unwrap(); // вход в r2: кэш бы девальвнул
        assert_eq!(sim.factions[0].cash, 0);
        assert_eq!(sim.factions[0].hard, 80 * PESO, "твёрдая валюта не тает");
        let mut sim2 = Simulator::new(2, 10 * PESO, 0, 0);
        sim2.game.epoch = EPOCH_CLASSIC;
        sim2.join(Pubkey::new_from_array([7; 32]), "A").unwrap();
        sim2.join(Pubkey::new_from_array([6; 32]), "B").unwrap();
        sim2.advance(100, 0).unwrap();
        assert!(sim2.exchange(0, true).is_err(), "только в эпохе 90-х");
    }

    #[test]
    fn m78_customs_president_and_tariffs() {
        use crate::sim::Simulator;
        let mut sim = Simulator::new(1, 10 * PESO, 0, 0);
        sim.game.epoch = EPOCH_90S;
        sim.join(Pubkey::new_from_array([1; 32]), "A").unwrap();
        sim.join(Pubkey::new_from_array([2; 32]), "B").unwrap();
        sim.advance(100, 0).unwrap(); // market r1
        sim.factions[0].cash = 100 * PESO;
        sim.factions[1].cash = 100 * PESO;
        sim.advance(200, 0).unwrap(); // → action r1, president elected
        // обе фракции везут серое
        sim.shuttle(0).unwrap();
        sim.shuttle(1).unwrap();
        // A покупает чёрную крышу у B (30%), B — красную у A (10%)
        sim.roof(0, 1, ROOF_BLACK).unwrap();
        assert_eq!(sim.factions[0].roof_tariff, ROOF_BLACK);
        assert_eq!(sim.factions[0].cash, 70 * PESO);
        // красная крыша B сработает только на бите mayhem
        sim.roof(1, 0, ROOF_RED).unwrap();
        // B получил 30M от A и заплатил 10% от 130M = 13M
        assert_eq!(sim.factions[1].cash, 117 * PESO);
        sim.advance(300, 0).unwrap(); // action → law: таможня
        // при seed 0: tight-режима нет (customs_decided=false),
        // rng_seizure = 0 < 64 → tight: серой горит у B (red не спасает
        // отtight), у A чёрная — проходит
        assert_eq!(sim.factions[0].goods, 3, "чёрная крыша прошла границу");
        assert!(sim.factions[1].goods < 3, "красная крыша не даёт гарантии");
    }

    #[test]
    fn m8_president_sets_tight_blindly_and_gets_tribute() {
        use crate::sim::Simulator;
        let mut sim = Simulator::new(2, 10 * PESO, 0, 0);
        sim.game.epoch = EPOCH_90S;
        sim.join(Pubkey::new_from_array([1; 32]), "A").unwrap();
        sim.join(Pubkey::new_from_array([2; 32]), "B").unwrap();
        sim.advance(100, 0).unwrap();
        sim.factions[1].cash = 50 * PESO; // B богатейший, A назначим президентом руками
        sim.advance(200, 0).unwrap(); // action r1
        sim.shuttle(1).unwrap(); // B везёт серое без крыши
        sim.game.president = sim.factions[0].wallet; // A президент
        sim.set_customs(0, false).unwrap(); // льготная граница
        assert!(sim.set_customs(0, true).is_err(), "раз в раунд");
        assert!(sim.set_customs(1, false).is_err(), "не президент");
        let cash_b_before = sim.factions[1].cash;
        sim.advance(300, 0).unwrap(); // таможня: loose → дань президенту
        assert_eq!(sim.factions[1].goods, 3, "товар прошёл");
        assert_eq!(sim.factions[1].cash, cash_b_before - CUSTOMS_TRIBUTE);
        assert_eq!(sim.factions[0].cash, CUSTOMS_TRIBUTE, "дань дошла");
        // флаг живёт до конца Law-фазы, сбрасывается при входе в раунд
        sim.factions[0].voted_stamp = sim.game.stamp();
        sim.factions[0].vote = VoteChoice::Abstain;
        sim.advance(400, 0).unwrap(); // law → market r2
        assert!(!sim.game.customs_decided, "флаг сброшен к следующему раунду");
    }

    #[test]
    fn m9_blind_auction_and_insider() {
        use crate::sim::Simulator;
        let mut sim = Simulator::new(3, 10 * PESO, 0, 0);
        sim.game.epoch = EPOCH_90S;
        sim.join(Pubkey::new_from_array([1; 32]), "A").unwrap();
        sim.join(Pubkey::new_from_array([2; 32]), "B").unwrap();
        sim.factions[0].cash = 100 * PESO;
        sim.factions[1].cash = 100 * PESO;
        // доходим до action r4 (аукцион)
        for r in 1..=3u8 {
            sim.advance(100 + r as i64 * 100, r as u64).unwrap(); // →action? нет: advance двигает на шаг
        }
        // быстрее: подгоним фазы напрямую
        sim.game.round = AUCTION_ROUND;
        sim.game.phase = Phase::Market;
        sim.advance(900, 42).unwrap(); // market→action r4: yield зафиксирован
        let y = sim.game.license_yield;
        assert!(y >= LICENSE_MIN_YIELD && y < LICENSE_MIN_YIELD + LICENSE_YIELD_SPAN);
        // инсайд: A платит 5M и видит доход
        let seen = sim.inspect_license(0).unwrap();
        assert_eq!(seen, y, "инсайдер видит точный доход");
        assert_eq!(sim.factions[0].cash, 95 * PESO);
        // ставки: A 20M, B 10M; платит только победитель
        sim.bid_license(0, 20 * PESO).unwrap();
        sim.bid_license(1, 10 * PESO).unwrap();
        sim.advance(1000, 0).unwrap(); // action → law: вскрытие
        assert_eq!(sim.game.license_holder, 0);
        assert_eq!(sim.factions[0].cash, 75 * PESO, "A: -20 ставка, победитель");
        assert_eq!(sim.factions[1].cash, 100 * PESO, "B: ставка вернулась");
        assert_eq!(sim.game.prize_pot, 20 * PESO);
        // ставки вне раунда аукциона закрыты
        sim.game.round = AUCTION_ROUND + 1;
        sim.game.phase = Phase::Market;
        sim.advance(1100, 0).unwrap();
        assert!(sim.bid_license(0, PESO).is_err());
    }

    #[test]
    fn m10_vote_offer_requires_accept() {
        use crate::sim::Simulator;
        // фикс по дебрифу: без акцепта покупателя деньги не списываются
        let mut sim = Simulator::new(5, 10 * PESO, 0, 0);
        sim.game.epoch = EPOCH_90S;
        sim.join(Pubkey::new_from_array([1; 32]), "A").unwrap();
        sim.join(Pubkey::new_from_array([2; 32]), "B").unwrap();
        sim.advance(100, 0).unwrap();
        sim.advance(200, 1).unwrap(); // → action
        sim.advance(300, 2).unwrap(); // → law
        sim.factions[1].cash = 10 * PESO; // B богатый покупатель
        // A предлагает голос за 2M: деньги B не тронуты
        sim.offer_vote(0, 1, 2 * PESO).unwrap();
        assert_eq!(sim.factions[1].cash, 10 * PESO, "офер ничего не списывает");
        // B акцептует: деньги переходят, голос A делегирован B
        sim.accept_vote_offer(1).unwrap();
        assert_eq!(sim.factions[1].cash, 8 * PESO);
        assert_eq!(sim.factions[0].cash, 2 * PESO);
        assert!(sim.factions[0].vote_sold);
        assert_eq!(sim.factions[0].vote_sold_to, 1);
    }

    #[test]
    fn m10_no_accept_no_steal() {
        use crate::sim::Simulator;
        // гриферский сценарий из живой партии: продавец «продаёт» голос
        // богатому без его согласия — кэш покупателя не должен шевелиться
        let mut sim = Simulator::new(6, 10 * PESO, 0, 0);
        sim.game.epoch = EPOCH_90S;
        sim.join(Pubkey::new_from_array([1; 32]), "A").unwrap();
        sim.join(Pubkey::new_from_array([2; 32]), "B").unwrap();
        sim.advance(100, 0).unwrap();
        sim.advance(200, 1).unwrap();
        sim.advance(300, 2).unwrap(); // law
        sim.factions[1].cash = 50 * PESO;
        sim.offer_vote(0, 1, 15 * PESO).unwrap(); // офер, не кража
        assert_eq!(sim.factions[1].cash, 50 * PESO);
        assert!(!sim.factions[0].vote_sold, "без акцепта делегации нет");
        // закрытие закона сбрасывает офер
        sim.advance(400, 3).unwrap(); // law → market r2
        assert_eq!(sim.factions[0].vote_offer_to, VOTE_OFFER_NONE);
    }

    #[test]
    fn m11_barter_direct_exchange() {
        use crate::sim::Simulator;
        let mut sim = Simulator::new(4, 10 * PESO, 0, 0);
        sim.game.epoch = EPOCH_90S;
        sim.join(Pubkey::new_from_array([1; 32]), "A").unwrap();
        sim.join(Pubkey::new_from_array([2; 32]), "B").unwrap();
        sim.advance(100, 0).unwrap(); // market r1
        sim.factions[0].goods = 4;
        sim.factions[1].cash = 30 * PESO;
        let id = sim.barter_propose(0, None, 2, 10 * PESO).unwrap();
        // чужой кэш недостаточен? у B 30M хватает
        sim.barter_accept(1, id).unwrap();
        assert_eq!(sim.factions[0].goods, 2);
        assert_eq!(sim.factions[1].goods, 2);
        assert_eq!(sim.factions[1].cash, 20 * PESO);
        assert_eq!(sim.factions[0].cash, 10 * PESO);
        assert!(sim.barter_offers.is_empty(), "офер снят");
        // повторный accept того же офера невозможен
        assert!(sim.barter_accept(1, id).is_err());
    }
}
