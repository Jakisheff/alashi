//! alashi-rules-wasm: тонкий клиентский слой над каноническими правилами.
//!
//! Единственный источник правил — крейт `alashi-rules`; здесь только
//! wasm-bindgen обёртка. Экспортируется семантика (wire-имена карт,
//! donkey-условия, эпохи), но не презентационные тексты: display-строки
//! живут во фронтенде и привязываются к wire-ключам, не к индексам.
//!
//! Числовой контракт повторяет уже принятый во фронте: деньги —
//! десятичные строки (u64 вне safe integer), количества — числа.

use wasm_bindgen::prelude::*;

use alashi_rules::constants::{
    self as rules, DONKEY_RISK_FEE, DONKEY_RISK_OUTCOMES, DONKEY_RISK_REWARD,
    DONKEY_RISK_SUCCESS_COUNT, EPOCH_DONKEY_RISK, ROUNDS, RULES_VERSION,
};

/// Версия правил, зашитая в этот артефакт. Клиент может показать её
/// или отказаться интерпретировать события чужой версии.
#[wasm_bindgen]
pub fn rules_version() -> u32 {
    RULES_VERSION
}

/// Wire-имя карты закона (канон `alashi_rules::constants::law_wire_name`).
#[wasm_bindgen]
pub fn law_wire_name(card: u8) -> String {
    rules::law_wire_name(card).into()
}

/// Плата за donkey-попытку в минорных единицах (строка: u64 вне JS safe integer).
#[wasm_bindgen]
pub fn donkey_fee_units() -> String {
    DONKEY_RISK_FEE.to_string()
}

/// Товаров при успехе donkey-попытки.
#[wasm_bindgen]
pub fn donkey_reward_goods() -> u16 {
    DONKEY_RISK_REWARD
}

/// Процент исходов с выигрышем: SUCCESS_COUNT/OUTCOMES, вычислен, не скопирован.
#[wasm_bindgen]
pub fn donkey_success_percent() -> u8 {
    (DONKEY_RISK_SUCCESS_COUNT as u16 * 100 / DONKEY_RISK_OUTCOMES as u16) as u8
}

/// Процент пустых исходов: (OUTCOMES-SUCCESS_COUNT)/OUTCOMES.
#[wasm_bindgen]
pub fn donkey_empty_percent() -> u8 {
    ((DONKEY_RISK_OUTCOMES - DONKEY_RISK_SUCCESS_COUNT) as u16 * 100
        / DONKEY_RISK_OUTCOMES as u16) as u8
}

/// Максимальный запас товара, при котором попытка ещё не переполняет u16.
#[wasm_bindgen]
pub fn donkey_max_goods_before_attempt() -> u16 {
    u16::MAX - DONKEY_RISK_REWARD
}

/// Последний раунд donkey-попытки (в ROUNDS нет следующей Market-фазы).
#[wasm_bindgen]
pub fn donkey_last_playable_round() -> u8 {
    ROUNDS - 1
}

/// Всего раундов в партии (граница валидации номера раунда на клиенте).
#[wasm_bindgen]
pub fn total_rounds() -> u8 {
    ROUNDS
}

/// Эпоха с donkey-лотереей (платная попытка, разрешение только из VRF).
#[wasm_bindgen]
pub fn epoch_donkey_risk() -> u8 {
    EPOCH_DONKEY_RISK
}

/// Правила экономики 90-х применяются к этой эпохе.
#[wasm_bindgen]
pub fn has_90s_rules(epoch: u8) -> bool {
    rules::has_90s_rules(epoch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terms_match_rule_constants() {
        assert_eq!(donkey_fee_units(), DONKEY_RISK_FEE.to_string());
        assert_eq!(donkey_reward_goods(), DONKEY_RISK_REWARD);
        assert_eq!(
            donkey_success_percent() as u16 * DONKEY_RISK_OUTCOMES as u16,
            DONKEY_RISK_SUCCESS_COUNT as u16 * 100
        );
        assert_eq!(donkey_success_percent() as u16 + donkey_empty_percent() as u16, 100);
    }

    #[test]
    fn limits_match_rule_constants() {
        assert_eq!(donkey_max_goods_before_attempt(), u16::MAX - DONKEY_RISK_REWARD);
        assert_eq!(donkey_last_playable_round(), ROUNDS - 1);
    }

    #[test]
    fn every_deck_card_has_a_wire_name() {
        for card in 0..=rules::LAW_AMNESTY {
            assert_ne!(rules::law_wire_name(card), "no_law", "card {card}");
        }
        assert_eq!(rules::law_wire_name(rules::NO_LAW), "no_law");
        assert_eq!(rules::law_wire_name(200), "no_law");
    }
}
