use alashi_rules::{
    actions,
    constants::*,
    error::GameError,
    logic::compute_settlement_epoch,
    state::{Faction, Game, Phase},
};
use anchor_lang::prelude::Pubkey;

fn setup() -> (Game, Faction) {
    let mut game = Game::default();
    game.epoch = EPOCH_DONKEY_RISK;
    game.entropy_mode = ENTROPY_SWITCHBOARD;
    game.phase = Phase::Action;
    game.round = 2;
    let mut faction = Faction::default();
    faction.alive = true;
    faction.cash = DONKEY_RISK_FEE;
    (game, faction)
}

#[test]
fn attempt_commits_fee_and_action_before_either_result() {
    let (game, mut empty) = setup();
    actions::donkey_risk_attempt(&game, &mut empty).unwrap();
    assert_eq!(empty.cash, 0);
    assert_eq!(empty.acted_stamp, game.stamp());
    assert_eq!(actions::donkey_risk_resolve(&mut empty, false).unwrap(), 0);
    assert_eq!(empty.goods, 0);
    assert!(matches!(
        actions::donkey_risk_attempt(&game, &mut empty),
        Err(GameError::AlreadyActed)
    ));

    let (game, mut winner) = setup();
    actions::donkey_risk_attempt(&game, &mut winner).unwrap();
    assert_eq!(
        actions::donkey_risk_resolve(&mut winner, true).unwrap(),
        DONKEY_RISK_REWARD
    );
    assert_eq!((winner.cash, winner.goods), (0, DONKEY_RISK_REWARD));
}

#[test]
fn all_preconditions_reject_without_fee_or_turn() {
    for mode in 0..5 {
        let (mut game, mut f) = setup();
        match mode {
            0 => game.phase = Phase::Market,
            1 => game.round = ROUNDS,
            2 => f.cash -= 1,
            3 => f.goods = u16::MAX - DONKEY_RISK_REWARD + 1,
            _ => f.alive = false,
        }
        assert!(actions::donkey_risk_attempt(&game, &mut f).is_err());
        assert_eq!(f.cash, DONKEY_RISK_FEE.saturating_sub(u64::from(mode == 2)));
        assert_eq!(f.acted_stamp, 0);
    }
    let (game, mut f) = setup();
    assert!(matches!(
        actions::donkey(&game, &mut f),
        Err(GameError::DonkeyOracleInvalid)
    ));
    assert_eq!(f.cash, DONKEY_RISK_FEE);
}

#[test]
fn epoch2_inherits_90s_actions_and_settlement_but_replaces_donkey() {
    assert!(!has_90s_rules(EPOCH_CLASSIC));
    assert!(has_90s_rules(EPOCH_90S));
    assert!(has_90s_rules(EPOCH_DONKEY_RISK));
    assert!(!has_90s_rules(3));

    let mut game = Game::default();
    game.epoch = EPOCH_DONKEY_RISK;
    game.phase = Phase::Action;
    game.round = 2;
    let mut faction = Faction::default();
    faction.alive = true;
    assert_eq!(actions::shuttle(&game, &mut faction).unwrap(), SHUTTLE_GOODS);
    assert_eq!(faction.grey_goods, SHUTTLE_GOODS);
    assert!(matches!(
        actions::donkey(&game, &mut Faction::default()),
        Err(GameError::DonkeyOracleInvalid)
    ));

    let mut rich_in_hard = Faction::default();
    rich_in_hard.wallet = Pubkey::new_from_array([1; 32]);
    rich_in_hard.cash = PESO;
    rich_in_hard.hard = 20 * PESO;
    rich_in_hard.alive = true;
    let mut richer_in_cash = Faction::default();
    richer_in_cash.wallet = Pubkey::new_from_array([2; 32]);
    richer_in_cash.cash = 10 * PESO;
    richer_in_cash.alive = true;
    let factions = [rich_in_hard, richer_in_cash];
    let risk = compute_settlement_epoch(&game, &factions, 100 * PESO, 0, false, false).unwrap();
    assert_eq!(risk.order[0], 0, "epoch2 ranks cash plus hard currency");
    game.epoch = EPOCH_CLASSIC;
    let classic = compute_settlement_epoch(&game, &factions, 100 * PESO, 0, false, false).unwrap();
    assert_eq!(classic.order[0], 1, "classic ranks cash only");
}

#[test]
fn only_epoch2_can_abort_an_uncommitted_action_after_grace() {
    let (mut game, mut faction) = setup();
    game.phase_ends_at = 100;
    actions::donkey_risk_attempt(&game, &mut faction).unwrap();
    assert!(alashi_rules::transitions::abort_uncommitted_donkey(
        &mut game, 129).is_err());
    assert_eq!(game.phase, Phase::Action);
    let result = alashi_rules::transitions::abort_uncommitted_donkey(
        &mut game, 130).unwrap();
    assert!(result.aborted);
    assert_eq!(game.phase, Phase::Aborted);
    assert_eq!((faction.cash, faction.goods), (0, 0));
    assert!(alashi_rules::transitions::abort_uncommitted_donkey(
        &mut game, 130).is_err());
    game.phase = Phase::Action;
    game.epoch = EPOCH_90S;
    assert!(alashi_rules::transitions::abort_uncommitted_donkey(
        &mut game, 130).is_err());
}
