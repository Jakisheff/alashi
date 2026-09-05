use alashi_rules::{constants::*, logic::compute_settlement_epoch, sim::Simulator, state::Phase};
use alashi_rules::anchor_lang::prelude::Pubkey;

fn two_players() -> Simulator {
    let mut sim = Simulator::new(1, 10 * PESO, 1, ENTROPY_SLOTHASH);
    sim.game.epoch = EPOCH_90S;
    for i in 1..=2 {
        sim.join(Pubkey::new_from_array([i; 32]), "Demo").unwrap();
    }
    sim.factions[0].cash = 10 * PESO;
    sim.factions[1].cash = 5 * PESO;
    sim.factions[1].influence = 2;
    sim
}

#[test]
fn factory_uses_distributable_pot_and_preserves_rent() {
    let sim = two_players();
    let reserve = 1_234_567;
    let pot = 100 * PESO;
    let plan = compute_settlement_epoch(&sim.game, &sim.factions, pot + reserve, reserve, false, false).unwrap();
    assert_eq!(plan.lines[1].factory_bonus, 5 * PESO);
    assert_eq!(plan.lines[0].factory_bonus, 0);
    assert_eq!(plan.rake, 0);
    assert_eq!(plan.lines.iter().map(|line| line.total).sum::<u64>(), pot);
}

#[test]
fn large_bank_does_not_overflow_intermediate_share_calculations() {
    let sim = two_players();
    let reserve = 1_234_567;
    let plan = compute_settlement_epoch(&sim.game, &sim.factions, u64::MAX, reserve, false, false).unwrap();
    assert_eq!(plan.lines.iter().map(|line| line.total as u128).sum::<u128>() + plan.rake as u128 + reserve as u128, u64::MAX as u128);
    assert!(plan.lines[1].factory_bonus > 0);
}

#[test]
fn premature_insight_neither_charges_nor_consumes_the_purchase() {
    let mut sim = two_players();
    sim.game.phase = Phase::Market;
    sim.game.round = 3;
    let cash = sim.factions[0].cash;
    assert!(sim.inspect_license(0).is_err());
    assert_eq!(sim.factions[0].cash, cash);
    assert!(!sim.factions[0].insider);
    sim.game.phase = Phase::Action;
    sim.game.round = AUCTION_ROUND;
    sim.game.license_yield = 25 * PESO;
    assert_eq!(sim.inspect_license(0).unwrap(), 25 * PESO);
    assert_eq!(sim.factions[0].cash, cash - LICENSE_INSIGHT_PRICE);
    assert!(sim.inspect_license(0).is_err());
    // Keep the existing documented ability to buy after the auction.
    sim.game.phase = Phase::Law;
    assert_eq!(sim.inspect_license(1).unwrap(), 25 * PESO);
}
