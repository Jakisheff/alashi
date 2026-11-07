// Core behavior documentation for audit 20260927, finding S1
// (docs/ops/audit_20260927/REPORT.md). The onchain adapter used to feed
// seed = 0 into the shared core for most economy events; the adapter now
// derives economy randomness from the slot hash on every advance and the
// VRF reveal routes through the core (see programs replay tests). These
// tests pin the core mapping seed -> economy outcome, including the
// boundary value 0.

use alashi_rules::anchor_lang::prelude::Pubkey;
use alashi_rules::constants::{
    AUCTION_ROUND, ENTROPY_SWITCHBOARD, EPOCH_90S, LICENSE_MIN_YIELD, NO_LAW, ROOF_RED,
};
use alashi_rules::state::{Faction, Game, Phase};
use alashi_rules::transitions;

#[test]
fn zero_seed_adapter_changes_epoch_economics() {
    let mut g = Game {
        phase: Phase::Market,
        round: AUCTION_ROUND,
        epoch: EPOCH_90S,
        ..Game::default()
    };
    transitions::advance(&mut g, &mut [], 100, 11, 0, None).unwrap();
    assert_eq!(g.license_yield, LICENSE_MIN_YIELD);

    let mut alternate = Game {
        phase: Phase::Market,
        round: AUCTION_ROUND,
        epoch: EPOCH_90S,
        ..Game::default()
    };
    transitions::advance(&mut alternate, &mut [], 100, 11, 12345, None).unwrap();
    assert_eq!(alternate.license_yield, LICENSE_MIN_YIELD + 12345);

    let mut a = Faction {
        wallet: Pubkey::new_from_array([1; 32]),
        alive: true,
        goods: 10,
        grey_goods: 3,
        roof_tariff: ROOF_RED,
        ..Faction::default()
    };
    let mut b = Faction {
        wallet: Pubkey::new_from_array([2; 32]),
        alive: true,
        ..Faction::default()
    };
    let mut vrf = Game {
        phase: Phase::Action,
        round: 1,
        epoch: EPOCH_90S,
        entropy_mode: ENTROPY_SWITCHBOARD,
        faction_count: 2,
        ..Game::default()
    };
    transitions::advance(
        &mut vrf,
        &mut [&mut a, &mut b],
        100,
        11,
        0,
        Some((Pubkey::new_from_array([3; 32]), 10)),
    )
    .unwrap();
    assert_eq!(a.goods, 0);
    assert_eq!(vrf.law_card, NO_LAW);
}
