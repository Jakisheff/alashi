// Инварианты канонических констант и функций правил, от которых зависит
// клиентский WASM-слой (rules-wasm). Полный перебор там, где он дешевле
// property-based теста: price_at и law_wire_name перебираются целиком.

use alashi_rules::constants::*;

#[test]
fn payout_shares_sum_to_100_percent() {
    assert_eq!(PAYOUT_SHARES.iter().sum::<u64>(), 100, "shares are percentages");
}

#[test]
fn donkey_percentages_partition_outcomes_exactly() {
    assert!(DONKEY_RISK_SUCCESS_COUNT <= DONKEY_RISK_OUTCOMES);
    // 3/5 и 2/5 дают целые проценты; если будущая калибровка сломает это,
    // клиентский процент-расчёт rules-wasm должен быть пересмотрен.
    let (success, outcomes) = (DONKEY_RISK_SUCCESS_COUNT as u16, DONKEY_RISK_OUTCOMES as u16);
    assert_eq!((success * 100) % outcomes, 0);
    assert_eq!((outcomes - success) * 100 % outcomes, 0);
}

#[test]
fn donkey_reward_never_overflows_goods() {
    // u16::MAX - REWARD — граница eligibility, опубликованная клиенту.
    assert!(u16::MAX - DONKEY_RISK_REWARD > 0, "at least one goods slot must remain");
}

#[test]
fn every_law_card_has_a_unique_wire_name() {
    let mut seen = std::collections::HashSet::new();
    for card in 0..=LAW_AMNESTY {
        let wire = law_wire_name(card);
        assert_ne!(wire, "no_law", "card {card} must be named");
        assert!(seen.insert(wire), "duplicate wire name {wire} for card {card}");
    }
    assert_eq!(law_wire_name(NO_LAW), "no_law");
    for foreign in (LAW_AMNESTY + 1)..=u8::MAX {
        if foreign == NO_LAW { continue }
        assert_eq!(law_wire_name(foreign), "no_law", "foreign card {foreign} must not be named");
    }
}

#[test]
fn price_table_is_exhaustive_bounded_and_monotone() {
    // Полный перебор области определения (u16): цена всегда в таблице,
    // нестрого убывает, край таблицы — пол для больших sold.
    let mut previous = price_at(0);
    assert_eq!(previous, 12, "opening price");
    for sold in 0..=u16::MAX {
        let price = price_at(sold);
        assert!((1..=12).contains(&price), "price {price} out of range at sold={sold}");
        assert!(price <= previous, "price must not increase at sold={sold}");
        previous = price;
    }
    assert_eq!(price_at(u16::MAX), *PRICE_TABLE.last().unwrap());
}

#[test]
fn epoch_rules_match_spec_epochs() {
    assert!(has_90s_rules(EPOCH_90S));
    assert!(has_90s_rules(EPOCH_DONKEY_RISK));
    assert!(!has_90s_rules(EPOCH_CLASSIC));
    assert!(!has_90s_rules(EPOCH_DONKEY_RISK + 1), "future epoch is never enabled by range");
}

#[test]
fn rules_version_is_present() {
    assert!(RULES_VERSION >= 1, "client freshness gate depends on this value");
}
