use alashi_rules::{actions, constants::*, error::GameError, logic::compute_settlement_epoch, sim::Simulator, state::{Faction, Game, Phase, VoteChoice}};
use alashi_rules::anchor_lang::prelude::Pubkey;

fn faction(key: u8) -> Faction {
    let mut f = Faction::default();
    f.wallet = Pubkey::new_from_array([key; 32]);
    f.alive = true;
    f.goods = 2;
    f.cash = 10 * PESO;
    f
}

#[test]
fn rejected_zero_purchase_and_dead_actions_do_not_mutate_state() {
    let mut g = Game::default();
    g.phase = Phase::Market;
    let mut f = faction(1);
    assert!(matches!(actions::buy(&mut g, &mut f, 0), Err(GameError::NoUnits)));
    assert_eq!((f.cash, f.goods, f.acted_stamp), (10 * PESO, 2, 0));
    f.alive = false;
    g.phase = Phase::Action;
    assert!(matches!(actions::produce(&g, &mut f), Err(GameError::NotAlive)));
    assert!(matches!(actions::donkey(&g, &mut f), Err(GameError::NotAlive)));
    g.epoch = EPOCH_90S;
    assert!(matches!(actions::shuttle(&g, &mut f), Err(GameError::NotAlive)));
    g.phase = Phase::Law;
    g.law_card = LAW_TAX_10;
    assert!(matches!(actions::vote(&g, &mut f, VoteChoice::Yes), Err(GameError::NotAlive)));
    assert_eq!((f.cash, f.goods, f.acted_stamp, f.voted_stamp), (10 * PESO, 2, 0, 0));
}

#[test]
fn barter_capacity_rejects_without_mutation_and_recovers_after_accept() {
    let mut g = Game::default();
    g.phase = Phase::Market;
    g.epoch = EPOCH_90S;
    let mut seller = faction(1);
    let mut buyer = faction(2);
    for _ in 0..MAX_BARTER_OFFERS {
        actions::barter_propose(&mut g, &seller, None, 1, PESO).unwrap();
    }
    let next = g.barter_next_id;
    assert!(matches!(actions::barter_propose(&mut g, &seller, None, 1, PESO), Err(GameError::TooManyOffers)));
    assert_eq!(g.barter_next_id, next);
    assert_eq!(g.barter_offers.len(), MAX_BARTER_OFFERS);
    actions::barter_accept(&mut g, &mut buyer, &mut seller, 0).unwrap();
    assert_eq!(actions::barter_propose(&mut g, &seller, None, 1, PESO).unwrap(), next);
}

#[test]
fn tied_license_bids_are_independent_of_account_order() {
    for keys in [[9, 2], [2, 9]] {
        let mut sim = Simulator::new(1, PESO, 1, ENTROPY_SLOTHASH);
        sim.game.epoch = EPOCH_90S;
        sim.game.phase = Phase::Action;
        sim.game.round = AUCTION_ROUND;
        sim.game.faction_count = 2;
        sim.factions = keys.into_iter().map(|k| { let mut f = faction(k); f.bid = PESO; f }).collect();
        sim.advance(1, 0).unwrap();
        assert_eq!(sim.game.license_holder, Pubkey::new_from_array([2; 32]));
        assert_eq!(sim.game.prize_pot, PESO);
        assert_eq!(sim.factions.iter().map(|f| f.cash).sum::<u64>(), 21 * PESO);
    }
}

#[test]
fn zero_payouts_do_not_change_ranking_or_factory_tiebreak() {
    let mut g = Game::default();
    g.rake_bps = DEFAULT_RAKE_BPS;
    let fs: Vec<_> = [6, 5, 4, 3, 2, 1].into_iter().map(faction).collect();
    for bank in [1, 100 * PESO] {
        let result = compute_settlement_epoch(&g, &fs, bank, 0, false, false).unwrap();
        assert_eq!(result.order, vec![5, 4, 3, 2, 1, 0]);
        assert_eq!(result.lines.iter().map(|l| l.total).sum::<u64>() + result.rake, bank);
    }
}
