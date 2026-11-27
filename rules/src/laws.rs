//! Законы сохранения экономики Alashi как типы.
//!
//! Каждый закон — маленький объект с бинарным вердиктом `holds` для пары
//! снимков «до/после» и человеческим объяснением `describe_violation`
//! (объяснение осмысленно только при `holds == false`).
//!
//! Скоуп законов — переходы фаз (`transitions::advance`): верификатор
//! `verify_all_laws` зовётся после каждого перехода и помнит предыдущий
//! снимок партии. Сброс памяти — `reset_law_verifier`; чужой `game_id`
//! сравнением не является (новая партия проверяется только структурными
//! законами). Акционные каналы ценности (продажа/покупка у рынка,
//! рыночный налог, спред валютчика, плата за осла и инсайд) происходят
//! между снимками и законами этого модуля не проверяются.
//!
//! Почему в сумме денег есть `bid`, но нет `promissory` и `hard`:
//! ставка — это кэш, снятый в эскроу и вернувшийся на вскрытии аукциона
//! (движения кэш→bid→prize_pot обязаны быть нейтральными); вексель —
//! будущие деньги: гашение — приход, амнистия сжигает вексель, не трогая
//! сумму (оба события нейтральны для закона «не убывает»); твёрдая
//! валюта — отдельная валюта, на переходах фаз не движется.

use std::cell::RefCell;
use std::sync::OnceLock;

use crate::constants::{
    has_90s_rules, price_at, DEPRECIATION_DEN, DEPRECIATION_NUM, PAYOUT_SHARES, PRICE_TABLE,
    ROOF_RED,
};
use crate::logic::market_price;
use crate::state::{Faction, Game, Phase};

/// Закон сохранения: проверяется между снимками состояния «до» и
/// «после» перехода. Законы, не зависящие от пары снимков (структурные),
/// игнорируют аргументы.
pub trait ConservationLaw {
    fn name(&self) -> &'static str;
    fn holds(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> bool;
    fn describe_violation(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> String;
}

// ---------- деньги ----------

/// Деньги не исчезают: сумма кэша фракций, эскроу ставок, банка приза,
/// дохода лицензии и оплаченного VRF не убывает. Единственный переход,
/// сжигающий деньги, — вход в новый раунд эпохи 90-х: девальвация M1
/// (`cash = cash / 100 * 62`). Её сжигание разрешено ровно в размере
/// формулы девальвации, снятой со снимка «до».
pub struct MoneyConservation;

impl MoneyConservation {
    fn violation(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> Option<String> {
        let before_total = money_total(before, before_factions);
        let after_total = money_total(after, after_factions);
        let burn = sanctioned_burn(before, after, before_factions);
        if after_total + burn >= before_total {
            return None;
        }
        Some(format!(
            "MoneyConservation: деньги исчезли: было {}, стало {}, \
             допустимое сжигание девальвации {}, пропало {}",
            before_total,
            after_total,
            burn,
            before_total - (after_total + burn)
        ))
    }
}

impl ConservationLaw for MoneyConservation {
    fn name(&self) -> &'static str {
        "MoneyConservation"
    }
    fn holds(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> bool {
        self.violation(before, before_factions, after, after_factions)
            .is_none()
    }
    fn describe_violation(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> String {
        self.violation(before, before_factions, after, after_factions)
            .unwrap_or_else(|| format!("{}: нарушения нет", self.name()))
    }
}

/// Экономический след игры: кэш фракций, эскроу ставок аукциона, банк
/// приза, доход лицензии, оплаченный VRF. Суммирование в u128 — закон
/// не должен паниковать на переполнении даже в вырожденных состояниях.
fn money_total(game: &Game, factions: &[Faction]) -> u128 {
    let mut total = game.prize_pot as u128 + game.license_yield as u128 + game.vrf_spent as u128;
    for f in factions {
        total += f.cash as u128;
        total += f.bid as u128;
    }
    total
}

/// Единственный санкционированный канал убывания денег на переходах —
/// девальвация M1 при входе в новый раунд (Law→Market, round+1) эпох
/// 90-х: `cash = cash / DEPRECIATION_DEN * DEPRECIATION_NUM`. Формула
/// взята из transitions дословно, иначе граница разъедется с кодом.
fn sanctioned_burn(before: &Game, after: &Game, before_factions: &[Faction]) -> u128 {
    let enters_next_round = before.phase == Phase::Law
        && after.phase == Phase::Market
        && after.round as i32 == before.round as i32 + 1
        && has_90s_rules(before.epoch);
    if !enters_next_round {
        return 0;
    }
    before_factions
        .iter()
        .map(|f| (f.cash - f.cash / DEPRECIATION_DEN * DEPRECIATION_NUM) as u128)
        .sum()
}

// ---------- товар ----------

/// Товар не исчезает вне закрытия границы. `grey_goods` — маркер серой
/// ЧАСТИ товара (grey ≤ goods, изымается таможней), в сумму входит как
/// вес: на закрытии Action легальны два убытия — сброс маркера и изъятие
/// серой части (до 2×grey на фракцию); красная крыша при «беспределе»
/// может сжечь весь товар фракции. На остальных переходах сумма
/// не убывает.
pub struct GoodsConservation;

impl GoodsConservation {
    fn violation(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> Option<String> {
        if closes_action(before, after) {
            for f in before_factions {
                let Some(after_f) = after_factions.iter().find(|x| x.wallet == f.wallet) else {
                    return Some(format!(
                        "GoodsConservation: фракция {} исчезла вместе с товаром",
                        f.wallet
                    ));
                };
                let before_val = f.goods as u128 + f.grey_goods as u128;
                let after_val = after_f.goods as u128 + after_f.grey_goods as u128;
                // красная крыша живёт до конца партии: беспредел мог сжечь всё
                let allowed = if after_f.roof_tariff == ROOF_RED {
                    before_val
                } else {
                    2 * f.grey_goods as u128
                };
                if before_val > after_val + allowed {
                    return Some(format!(
                        "GoodsConservation: у фракции {} пропал товар на закрытии границы: \
                         было {} (товар {} + серый {}), стало {}, допустимо {}",
                        f.wallet, before_val, f.goods, f.grey_goods, after_val, allowed
                    ));
                }
            }
            None
        } else {
            let before_total = goods_total(before_factions);
            let after_total = goods_total(after_factions);
            if after_total >= before_total {
                None
            } else {
                Some(format!(
                    "GoodsConservation: товар исчез вне закрытия границы: было {}, стало {}",
                    before_total, after_total
                ))
            }
        }
    }
}

impl ConservationLaw for GoodsConservation {
    fn name(&self) -> &'static str {
        "GoodsConservation"
    }
    fn holds(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> bool {
        self.violation(before, before_factions, after, after_factions)
            .is_none()
    }
    fn describe_violation(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> String {
        self.violation(before, before_factions, after, after_factions)
            .unwrap_or_else(|| format!("{}: нарушения нет", self.name()))
    }
}

fn goods_total(factions: &[Faction]) -> u128 {
    factions
        .iter()
        .map(|f| f.goods as u128 + f.grey_goods as u128)
        .sum()
}

/// Закрытие фазы Action — единственный переход, потребляющий товар:
/// таможня изымает серую часть и сбрасывает маркер, красная крыша
/// горит при беспределе.
fn closes_action(before: &Game, after: &Game) -> bool {
    before.phase == Phase::Action && after.phase == Phase::Law
}

// ---------- делёж ----------

/// Доли финального дележа — проценты, суммирующиеся ровно в 100.
/// Структурный закон: от состояния партии не зависит, ломается только
/// правкой константы (compute_settlement делит банк пропорционально
/// этим долям).
pub struct PayoutPartition;

impl ConservationLaw for PayoutPartition {
    fn name(&self) -> &'static str {
        "PayoutPartition"
    }
    fn holds(
        &self,
        _before: &Game,
        _before_factions: &[Faction],
        _after: &Game,
        _after_factions: &[Faction],
    ) -> bool {
        PAYOUT_SHARES.iter().sum::<u64>() == 100
    }
    fn describe_violation(
        &self,
        _before: &Game,
        _before_factions: &[Faction],
        _after: &Game,
        _after_factions: &[Faction],
    ) -> String {
        let sum: u64 = PAYOUT_SHARES.iter().sum();
        format!(
            "PayoutPartition: PAYOUT_SHARES суммируются в {}%, а не в 100: {:?}",
            sum, PAYOUT_SHARES
        )
    }
}

// ---------- цена ----------

/// `price_at(sold)` не возрастает по sold (полный домен u16, включая
/// пол таблицы для больших sold), и действующие сдвиг цены с бумом не
/// ломают монотонность рынка в состояниях «до» и «после».
pub struct PriceMonotone;

fn price_at_monotone_over_full_domain() -> bool {
    static VERDICT: OnceLock<bool> = OnceLock::new();
    *VERDICT.get_or_init(|| {
        let mut previous = price_at(0);
        (1..=u16::MAX).all(|sold| {
            let price = price_at(sold);
            let monotone = price <= previous;
            previous = price;
            monotone
        })
    })
}

/// Первый разрыв монотонности рынка с параметрами состояния, если есть.
fn market_price_break(game: &Game) -> Option<(u16, u64, u64)> {
    let shift = game.active_price_shift;
    let boom = game.active_boom;
    let mut previous = market_price(0, shift, boom);
    for sold in 1..=PRICE_TABLE.len() as u16 {
        let price = market_price(sold, shift, boom);
        if price > previous {
            return Some((sold, previous, price));
        }
        previous = price;
    }
    None
}

impl PriceMonotone {
    fn violation(
        &self,
        before: &Game,
        _before_factions: &[Faction],
        after: &Game,
        _after_factions: &[Faction],
    ) -> Option<String> {
        if !price_at_monotone_over_full_domain() {
            for sold in 0..PRICE_TABLE.len() as u16 {
                if price_at(sold + 1) > price_at(sold) {
                    return Some(format!(
                        "PriceMonotone: цена выросла на sold={} ({} → {})",
                        sold,
                        price_at(sold),
                        price_at(sold + 1)
                    ));
                }
            }
            return Some("PriceMonotone: price_at возрастает вне домена таблицы".to_string());
        }
        for (label, game) in [("до", before), ("после", after)] {
            if let Some((sold, prev, price)) = market_price_break(game) {
                return Some(format!(
                    "PriceMonotone: цена рынка {} перехода (сдвиг {}, бум {}) выросла на sold={} ({} → {})",
                    label, game.active_price_shift, game.active_boom, sold, prev, price
                ));
            }
        }
        None
    }
}

impl ConservationLaw for PriceMonotone {
    fn name(&self) -> &'static str {
        "PriceMonotone"
    }
    fn holds(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> bool {
        self.violation(before, before_factions, after, after_factions)
            .is_none()
    }
    fn describe_violation(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> String {
        self.violation(before, before_factions, after, after_factions)
            .unwrap_or_else(|| format!("{}: нарушения нет", self.name()))
    }
}

// ---------- жизнь ----------

/// Смерть необратима: мёртвая фракция не воскресает и не исчезает из
/// списка. Фракции сопоставляются по кошельку — порядок массива у
/// кранкера не каноничен.
pub struct AliveMonotone;

impl AliveMonotone {
    fn violation(
        &self,
        _before: &Game,
        before_factions: &[Faction],
        _after: &Game,
        after_factions: &[Faction],
    ) -> Option<String> {
        for f in before_factions {
            if f.alive {
                continue;
            }
            match after_factions.iter().find(|x| x.wallet == f.wallet) {
                Some(after_f) if after_f.alive => {
                    return Some(format!(
                        "AliveMonotone: мёртвая фракция {} воскресла",
                        f.wallet
                    ));
                }
                Some(_) => {}
                None => {
                    return Some(format!(
                        "AliveMonotone: фракция {} исчезла из списка",
                        f.wallet
                    ));
                }
            }
        }
        None
    }
}

impl ConservationLaw for AliveMonotone {
    fn name(&self) -> &'static str {
        "AliveMonotone"
    }
    fn holds(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> bool {
        self.violation(before, before_factions, after, after_factions)
            .is_none()
    }
    fn describe_violation(
        &self,
        before: &Game,
        before_factions: &[Faction],
        after: &Game,
        after_factions: &[Faction],
    ) -> String {
        self.violation(before, before_factions, after, after_factions)
            .unwrap_or_else(|| format!("{}: нарушения нет", self.name()))
    }
}

// ---------- верификатор ----------

/// Все законы модуля в порядке проверки.
pub fn all_laws() -> Vec<Box<dyn ConservationLaw>> {
    vec![
        Box::new(MoneyConservation),
        Box::new(GoodsConservation),
        Box::new(PayoutPartition),
        Box::new(PriceMonotone),
        Box::new(AliveMonotone),
    ]
}

/// Проверка всех законов между двумя явными снимками.
/// Пустой Vec — все законы стоят.
pub fn verify_transition(
    before_game: &Game,
    before_factions: &[Faction],
    after_game: &Game,
    after_factions: &[Faction],
) -> Vec<String> {
    all_laws()
        .into_iter()
        .filter(|law| !law.holds(before_game, before_factions, after_game, after_factions))
        .map(|law| law.describe_violation(before_game, before_factions, after_game, after_factions))
        .collect()
}

struct LawSnapshot {
    game: Game,
    factions: Vec<Faction>,
}

thread_local! {
    static PREVIOUS: RefCell<Option<LawSnapshot>> = RefCell::new(None);
}

/// Сбросить память верификатора: новая партия или новое тест-условие.
pub fn reset_law_verifier() {
    PREVIOUS.with(|slot| *slot.borrow_mut() = None);
}

/// Верификатор «после каждого перехода»: помнит предыдущий снимок той же
/// партии (тот же game_id) и сверяет с ним законы «до/после». Первый
/// снимок и чужая партия проверяются только структурными законами.
/// Пустой Vec — все законы стоят.
pub fn verify_all_laws(game: &Game, factions: &[Faction]) -> Vec<String> {
    let previous = PREVIOUS.with(|slot| slot.borrow_mut().take());
    let violations = match previous {
        Some(prev) if prev.game.game_id == game.game_id => {
            verify_transition(&prev.game, &prev.factions, game, factions)
        }
        _ => verify_transition(game, factions, game, factions),
    };
    PREVIOUS.with(|slot| {
        *slot.borrow_mut() = Some(LawSnapshot {
            game: game.clone(),
            factions: factions.to_vec(),
        });
    });
    violations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{
        AUCTION_ROUND, ENTROPY_SLOTHASH, EPOCH_90S, EPOCH_CLASSIC, EPOCH_DONKEY_RISK, MAX_FACTIONS,
        PESO, ROUNDS,
    };
    use crate::sim::Simulator;
    use crate::transitions;
    use anchor_lang::prelude::Pubkey;
    use proptest::collection::vec;
    use proptest::prelude::*;

    fn wallet(i: u8) -> Pubkey {
        Pubkey::new_from_array([i; 32])
    }

    fn game_in(epoch: u8, phase: Phase, round: u8) -> Game {
        let mut g = Game::default();
        g.epoch = epoch;
        g.phase = phase;
        g.round = round;
        g
    }

    fn live_faction(idx: u8, cash: u64, goods: u16, grey: u16) -> Faction {
        let mut f = Faction::default();
        f.wallet = wallet(idx);
        f.alive = true;
        f.cash = cash;
        f.goods = goods;
        f.grey_goods = grey;
        f
    }

    // ---------- детерминированные проверки самих законов ----------

    #[test]
    fn fresh_state_passes_structural_laws() {
        reset_law_verifier();
        let game = Game::default();
        let factions = vec![live_faction(1, 0, 0, 0)];
        assert!(verify_all_laws(&game, &factions).is_empty());
        assert!(verify_transition(&game, &factions, &game, &factions).is_empty());
    }

    #[test]
    fn verifier_remembers_the_previous_snapshot() {
        reset_law_verifier();
        let game = Game::default();
        let mut factions = vec![live_faction(1, 10 * PESO, 3, 0)];
        assert!(verify_all_laws(&game, &factions).is_empty());
        factions[0].cash -= 3 * PESO; // кража между снимками
        let violations = verify_all_laws(&game, &factions);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].starts_with("MoneyConservation:"));
    }

    #[test]
    fn verifier_switches_games_by_game_id() {
        reset_law_verifier();
        let mut g1 = Game::default();
        g1.game_id = 1;
        let rich = vec![live_faction(1, 10 * PESO, 0, 0)];
        assert!(verify_all_laws(&g1, &rich).is_empty());
        // другая партия: сравнение с чужим снимком не выполняется
        let mut g2 = Game::default();
        g2.game_id = 2;
        let poor = vec![live_faction(1, 0, 0, 0)];
        assert!(verify_all_laws(&g2, &poor).is_empty());
    }

    #[test]
    fn round_increment_allows_exactly_the_depreciation_burn() {
        let before_game = game_in(EPOCH_90S, Phase::Law, 1);
        let after_game = game_in(EPOCH_90S, Phase::Market, 2);
        let before = vec![live_faction(1, 100 * PESO, 0, 0)];
        // 100M → 62M: девальвация сожгла 38M — закон стоит
        let after = vec![live_faction(1, 62 * PESO, 0, 0)];
        assert!(MoneyConservation.holds(&before_game, &before, &after_game, &after));
        // ещё одна единица сверх девальвации — ловится
        let stolen = vec![live_faction(1, 62 * PESO - 1, 0, 0)];
        assert!(!MoneyConservation.holds(&before_game, &before, &after_game, &stolen));
        // классика не девальвирует: любое убывание — нарушение
        let classic_before = game_in(EPOCH_CLASSIC, Phase::Law, 1);
        let classic_after = game_in(EPOCH_CLASSIC, Phase::Market, 2);
        assert!(!MoneyConservation.holds(&classic_before, &before, &classic_after, &after));
    }

    #[test]
    fn action_close_allows_only_sanctioned_goods_consumption() {
        let before_game = game_in(EPOCH_90S, Phase::Action, 1);
        let after_game = game_in(EPOCH_90S, Phase::Law, 1);
        let before = vec![live_faction(1, 0, 10, 3)];
        // таможня изъяла серую тройку и сбросила маркер: 13 → 7
        let seized = vec![live_faction(1, 0, 7, 0)];
        assert!(GoodsConservation.holds(&before_game, &before, &after_game, &seized));
        // только маркер (льготная граница или чёрная крыша): 13 → 10
        let marker_reset = vec![live_faction(1, 0, 10, 0)];
        assert!(GoodsConservation.holds(&before_game, &before, &after_game, &marker_reset));
        // сгорело больше серого, чем было: 13 → 5 — ловится
        let over = vec![live_faction(1, 0, 5, 0)];
        assert!(!GoodsConservation.holds(&before_game, &before, &after_game, &over));
        // красная крыша: беспредел может сжечь весь товар
        let mut burned = live_faction(1, 0, 0, 0);
        burned.roof_tariff = ROOF_RED;
        let burned = vec![burned];
        assert!(GoodsConservation.holds(&before_game, &before, &after_game, &burned));
    }

    #[test]
    fn alive_monotone_catches_resurrection_but_allows_death() {
        let game = Game::default();
        let alive = vec![live_faction(1, 0, 0, 0)];
        let mut dead = alive.clone();
        dead[0].alive = false;
        assert!(AliveMonotone.holds(&game, &alive, &game, &dead)); // смерть легальна
        let mut risen = dead.clone();
        risen[0].alive = true;
        assert!(!AliveMonotone.holds(&game, &dead, &game, &risen));
        assert!(AliveMonotone.holds(&game, &dead, &game, &dead));
    }

    // ---------- proptest: мутационное тестирование законов ----------

    #[derive(Clone, Copy, Debug)]
    enum Mutation {
        Cash(i64),
        Goods(i32),
        Grey(i32),
        FlipAlive,
    }

    fn mutation_strategy() -> impl Strategy<Value = Mutation> {
        prop_oneof![
            4 => (-1_000_000_000i64..1_000_000_000).prop_map(Mutation::Cash),
            3 => (-1_000i32..1_000).prop_map(Mutation::Goods),
            2 => (-1_000i32..1_000).prop_map(Mutation::Grey),
            1 => Just(Mutation::FlipAlive),
        ]
    }

    fn apply_mutation(f: &mut Faction, mutation: Mutation) {
        match mutation {
            Mutation::Cash(d) => {
                f.cash = (f.cash as i128 + d as i128).clamp(0, u64::MAX as i128) as u64;
            }
            Mutation::Goods(d) => {
                f.goods = (f.goods as i32 + d).clamp(0, u16::MAX as i32) as u16;
            }
            Mutation::Grey(d) => {
                f.grey_goods = (f.grey_goods as i32 + d).clamp(0, u16::MAX as i32) as u16;
            }
            Mutation::FlipAlive => {
                f.alive = !f.alive;
            }
        }
    }

    /// Game/Faction не реализуют Debug (макрос #[account] его не даёт),
    /// поэтому стратегии генерируют кортежи примитивов, а состояние
    /// собирается уже в теле теста.
    type GameSeed = (u64, usize, u8, u8);

    fn arb_game_seed() -> impl Strategy<Value = GameSeed> {
        (
            any::<u64>(),
            0usize..6,
            1u8..=ROUNDS,
            0u8..=EPOCH_DONKEY_RISK,
        )
    }

    fn game_from_seed((game_id, phase_idx, round, epoch): GameSeed) -> Game {
        let mut g = Game::default();
        g.game_id = game_id;
        g.phase = [
            Phase::Lobby,
            Phase::Market,
            Phase::Action,
            Phase::Law,
            Phase::Finished,
            Phase::Aborted,
        ][phase_idx];
        g.round = round;
        g.epoch = epoch;
        g
    }

    type FactionFields = (u64, u16, u16, bool, u64, bool);

    fn faction_fields() -> impl Strategy<Value = FactionFields> {
        (
            0..(u64::MAX / 64),
            0..100u16,
            0..100u16,
            any::<bool>(),
            0..(u64::MAX / 64),
            any::<bool>(),
        )
    }

    proptest! {
        /// Мутации полей фракций (кэш, товар, серый товар, жизнь) между
        /// снимками одного состояния: законы обязаны отреагировать ровно
        /// на те мутации, которые по семантике являются нарушением.
        /// Ожидание считается независимой школьной арифметикой (суммы
        /// полей), а не формулами самих законов.
        #[test]
        fn laws_flag_mutations_exactly_when_expected(
            game_seed in arb_game_seed(),
            fields in vec(faction_fields(), 2..=6),
            mutations in vec((any::<usize>(), mutation_strategy()), 1..12),
        ) {
            let game = game_from_seed(game_seed);
            let mut factions: Vec<Faction> = fields
                .iter()
                .enumerate()
                .map(|(i, &(cash, goods, grey, alive, bid, red))| {
                    let mut f = Faction::default();
                    f.wallet = wallet(i as u8 + 1);
                    f.cash = cash;
                    f.goods = goods;
                    f.grey_goods = grey;
                    f.alive = alive;
                    f.bid = bid;
                    f.roof_tariff = if red { ROOF_RED } else { 0 };
                    f
                })
                .collect();
            for &(target, mutation) in &mutations {
                let before_factions = factions.clone();
                let cash_before: u128 = factions.iter().map(|f| f.cash as u128).sum();
                let goods_before: u128 =
                    factions.iter().map(|f| f.goods as u128 + f.grey_goods as u128).sum();
                let dead: Vec<Pubkey> = factions
                    .iter()
                    .filter(|f| !f.alive)
                    .map(|f| f.wallet)
                    .collect();

                let idx = target % factions.len();
                apply_mutation(&mut factions[idx], mutation);

                let cash_after: u128 = factions.iter().map(|f| f.cash as u128).sum();
                let goods_after: u128 =
                    factions.iter().map(|f| f.goods as u128 + f.grey_goods as u128).sum();
                let mut expected: Vec<&str> = Vec::new();
                if cash_after < cash_before {
                    expected.push("MoneyConservation");
                }
                if goods_after < goods_before {
                    expected.push("GoodsConservation");
                }
                if factions.iter().any(|f| dead.contains(&f.wallet) && f.alive) {
                    expected.push("AliveMonotone");
                }

                // игра между снимками не менялась: все допуски законов нулевые
                let violations =
                    verify_transition(&game, &before_factions, &game, &factions);
                let names: Vec<&str> =
                    violations.iter().map(|v| v.split(':').next().unwrap_or("?")).collect();
                prop_assert_eq!(
                    names,
                    expected,
                    "мутация {:?} на {}: {:?}",
                    mutation,
                    target % factions.len(),
                    violations
                );
            }
        }
    }

    // ---------- proptest: законы стоят на случайных партиях ----------

    proptest! {
        /// Требование задачи: 10 случайных sim.advance() подряд, законы
        /// проверяются после каждого перехода. Случайная эпоха (включая
        /// девальвацию и таможню 90-х), случайный стартовый кэш и серой
        /// товар — допуски законов exercised ненулевыми суммами.
        #[test]
        fn laws_hold_across_ten_random_advances(
            game_id in 1..u64::MAX,
            epoch in 0u8..=EPOCH_DONKEY_RISK,
            faction_count in 2usize..=MAX_FACTIONS as usize,
            cash in 0..200u64,
            goods in 0..30u16,
            grey in 0..30u16,
            seeds in vec(any::<u64>(), 10),
        ) {
            let mut sim = Simulator::new(game_id, 0, 0, ENTROPY_SLOTHASH);
            sim.game.epoch = epoch;
            for i in 1..=faction_count {
                sim.join(wallet(i as u8), "F").unwrap();
            }
            for f in sim.factions.iter_mut() {
                f.cash = cash * PESO;
                f.goods = goods;
                f.grey_goods = grey.min(goods);
            }
            reset_law_verifier();
            prop_assert!(verify_all_laws(&sim.game, &sim.factions).is_empty());
            let mut now = 0i64;
            for step in 0..seeds.len() {
                now += 100;
                let _ = sim.advance(now, seeds[step]);
                let violations = verify_all_laws(&sim.game, &sim.factions);
                prop_assert!(violations.is_empty(), "шаг {}: {:?}", step, violations);
            }
        }
    }

    proptest! {
        /// Полная партия до Finished с эскроу ставок аукциона: деньги
        /// движутся по цепочке кэш→bid→prize_pot и девальвация жжёт кэш
        /// каждый раунд — закон обязан всё это различать.
        #[test]
        fn laws_hold_across_a_full_random_game_with_auction(
            game_id in 1..u64::MAX,
            epoch in 0u8..=EPOCH_DONKEY_RISK,
            faction_count in 2usize..=MAX_FACTIONS as usize,
            starting_cash in 0..500u64,
            goods in 0..10u16,
            seeds in vec(any::<u64>(), 40),
        ) {
            let mut sim = Simulator::new(game_id, 0, 0, ENTROPY_SLOTHASH);
            sim.game.epoch = epoch;
            for i in 1..=faction_count {
                sim.join(wallet(i as u8), "F").unwrap();
            }
            for f in sim.factions.iter_mut() {
                f.cash = starting_cash * PESO;
                f.goods = goods;
            }
            reset_law_verifier();
            prop_assert!(verify_all_laws(&sim.game, &sim.factions).is_empty());
            let mut now = 0i64;
            for &seed in &seeds {
                now += 100;
                let _ = sim.advance(now, seed);
                // легальный эскроу: ставка снимает кэш в bid до вскрытия
                if has_90s_rules(sim.game.epoch)
                    && sim.game.phase == Phase::Action
                    && sim.game.round == AUCTION_ROUND
                {
                    for i in 0..sim.factions.len() {
                        let all_in = sim.factions[i].cash;
                        if all_in > 0 {
                            let _ = sim.bid_license(i, all_in);
                        }
                    }
                }
                let violations = verify_all_laws(&sim.game, &sim.factions);
                prop_assert!(violations.is_empty(), "t={} фаза {:?}: {:?}", now, sim.game.phase, violations);
                if sim.game.phase == Phase::Finished {
                    break;
                }
            }
            prop_assert_eq!(sim.game.phase, Phase::Finished);
        }
    }

    proptest! {
        /// Допуск девальвации численно совпадает с формулой перехода:
        /// закрытие закона r1 → вход в r2. Вексель гасится в кэш
        /// (приход), девальвация жжёт floor-остаток (допуск).
        #[test]
        fn depreciation_burn_is_sanctioned_and_exact(
            cash in 0..1_000u64,
            promissory in 0..50u64,
        ) {
            let mut game = game_in(EPOCH_90S, Phase::Law, 1);
            game.phase_ends_at = 10;
            let mut f = live_faction(1, cash * PESO, 0, 0);
            f.promissory = promissory * PESO;
            let before = vec![f];
            let before_game = game.clone();
            let mut factions = before.clone();
            let mut refs: Vec<&mut Faction> = factions.iter_mut().collect();
            transitions::advance(&mut game, &mut refs, 20, 0, 7, None).unwrap();

            let violations = verify_transition(&before_game, &before, &game, &factions);
            prop_assert!(violations.is_empty(), "{:?}", violations);
            let expected_cash =
                (cash * PESO / DEPRECIATION_DEN) * DEPRECIATION_NUM + promissory * PESO;
            prop_assert_eq!(factions[0].cash, expected_cash);

            // кража сверх девальвации И гашения векселя ловится всегда
            // (единичная кража при promissory>0 невидима закону «не убывает»:
            // приход векселя больше кражи — это граница закона, не дыра)
            if cash >= 1 {
                let mut stolen = factions.clone();
                let theft = promissory * PESO + 1;
                stolen[0].cash = stolen[0].cash.saturating_sub(theft);
                prop_assert!(!MoneyConservation.holds(&before_game, &before, &game, &stolen));
            }
        }
    }
}
