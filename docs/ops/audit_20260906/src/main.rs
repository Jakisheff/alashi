use alashi_rules::{constants::*, logic::compute_settlement_epoch, sim::Simulator, state::{Game, Phase}};
use alashi_rules::anchor_lang::{prelude::Pubkey, Space};
use arena::api::{new_state, save_snapshot, load_snapshot, crank_once, GameEntry};
use std::sync::atomic::Ordering;

fn sim(fee: u64) -> Simulator {
    let mut s = Simulator::new(1, fee, 1, ENTROPY_SLOTHASH);
    s.join(Pubkey::new_from_array([1; 32]), "AuditA").unwrap();
    s.join(Pubkey::new_from_array([2; 32]), "AuditB").unwrap();
    s
}

fn entry(s: Simulator) -> GameEntry {
    GameEntry { entry_fee: s.game.entry_fee, sim: s, wallets: vec![], agents: vec![],
        created: 0, grace_s: 0, action_log: vec![], phase_log: vec![], insiders: Default::default(),
        party_no: 1, label: None }
}

fn main() {
    let out = std::env::var("AUDIT_DIR").expect("AUDIT_DIR required");
    std::fs::create_dir_all(&out).unwrap();
    let state_file = format!("{out}/snapshot.json");
    std::env::set_var("ALASHI_STATE_FILE", &state_file);
    std::env::set_var("ALASHI_SEQ_FILE", format!("{out}/seq"));

    let mut s = sim(10 * PESO);
    s.game.epoch = EPOCH_90S;
    let plan = compute_settlement_epoch(&s.game, &s.factions, 21 * PESO, PESO, false, false).unwrap();
    let bonus: u64 = plan.lines.iter().map(|p| p.factory_bonus).sum();
    let control = compute_settlement_epoch(&s.game, &s.factions, 20 * PESO, 0, false, false).unwrap();
    assert_eq!(bonus, 0);
    assert_eq!(control.lines.iter().map(|p| p.factory_bonus).sum::<u64>(), PESO);
    println!("FACTORY: pot=20000000 reserve=1000000 bonus={} rake={}; control reserve=0 bonus=1000000", bonus, plan.rake);

    let state = new_state();
    state.next_id.store(42, Ordering::SeqCst);
    state.completed.lock().unwrap().push(serde_json::json!({"game_id":41}).to_string());
    save_snapshot(&state);
    let restored = new_state();
    load_snapshot(&restored);
    assert_eq!(restored.next_id.load(Ordering::SeqCst), 1);
    println!("RESTART_ID: persisted next_id_hint=42, restored next_id=1, completed={}", restored.completed.lock().unwrap().len());

    let state = new_state();
    state.games.lock().unwrap().insert(1, entry(sim(10 * PESO)));
    save_snapshot(&state);
    crank_once(&state);
    assert_eq!(state.games.lock().unwrap()[&1].sim.game.phase, Phase::Market);
    let restored = new_state();
    load_snapshot(&restored);
    assert_eq!(restored.games.lock().unwrap()[&1].sim.game.phase, Phase::Lobby);
    println!("CRANK_SNAPSHOT: memory=Market, restored=Lobby after automatic phase transition");

    let mut s = sim(10 * PESO);
    assert!(s.join(Pubkey::new_from_array([1; 32]), "AuditA").is_err());
    s.game.phase = Phase::Market;
    let error = s.join(Pubkey::new_from_array([1; 32]), "AuditA").unwrap_err();
    assert!(matches!(error, alashi_rules::error::GameError::GameNotInLobby));
    println!("RECOVERY_GATE: duplicate player in Market returns {:?}, not DuplicateWallet", error);

    let mut s = sim(10 * PESO);
    s.game.phase = Phase::Market;
    s.game.epoch = EPOCH_90S;
    s.factions[0].goods = 1;
    for _ in 0..9 { s.barter_propose(0, None, 1, 1).unwrap(); }
    let size = borsh::to_vec(&s.game).unwrap().len();
    assert!(size > Game::INIT_SPACE);
    println!("BARTER_CAP: accepted=9 serialized_game={} allocated_payload={}", size, Game::INIT_SPACE);

    let mut s = sim(10 * PESO);
    s.game.phase = Phase::Action;
    s.game.round = 1;
    s.game.epoch = EPOCH_90S;
    s.factions[0].cash = 10 * PESO;
    s.shuttle(0).unwrap();
    let already_acted = s.factions[0].acted_stamp == s.game.stamp();
    s.roof(0, 1, ROOF_BLACK).unwrap();
    assert!(already_acted && s.factions[0].roof_armed);
    println!("ROOF_SECOND_ACTION: shuttle accepted then black roof accepted in same phase");

    let mut s = sim(10 * PESO);
    s.game.epoch = EPOCH_90S;
    s.game.round = 1;
    s.game.phase = Phase::Market;
    s.factions[0].cash = 10 * PESO;
    assert_eq!(s.inspect_license(0).unwrap(), 0);
    s.game.round = AUCTION_ROUND;
    s.game.phase = Phase::Action;
    s.game.license_yield = LICENSE_MIN_YIELD + 123;
    assert!(s.inspect_license(0).is_err());
    println!("EARLY_INSPECT: paid=5000000 yield=0; actual auction inspection rejected");

    let mut ratings = std::collections::HashMap::new();
    for name in ["a", "b", "c", "d", "e", "f"] { ratings.insert(name.to_string(), arena::rating::Rating::new()); }
    let players: Vec<_> = ["a", "b", "c", "d", "e", "f"].iter().enumerate().map(|(i, n)| (n.to_string(), i as u64)).collect();
    arena::rating::rate_party(&players, &mut ratings);
    let values: Vec<_> = ["a", "b", "c", "d", "e", "f"].iter().map(|n| ((*n).to_string(), ratings[*n].ordinal(), (ratings[*n].ordinal() * 1e6) as u64)).collect();
    let negative: Vec<_> = values.iter().filter(|(_, r, _)| *r < 0.0).collect();
    assert!(negative.len() >= 2 && negative.iter().all(|(_, _, key)| *key == 0));
    println!("LEADERBOARD_SIGN: {:?}", values);

    let state = new_state();
    state.games.lock().unwrap().insert(1, entry(sim(1 << 63)));
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        for _ in 0..20 {
            if let Some(e) = state.games.lock().unwrap().get_mut(&1) { e.sim.game.phase_ends_at = 0; }
            crank_once(&state);
        }
    }));
    assert!(failure.is_err());
    println!("SETTLEMENT_OVERFLOW: two players with accepted entry_fee=9223372036854775808 panic in crank_once");

    // This mirrors the seed supplied by the onchain adapter at Market -> Action.
    let mut s = sim(10 * PESO);
    s.game.epoch = EPOCH_90S;
    s.game.round = AUCTION_ROUND;
    s.game.phase = Phase::Market;
    s.advance(0, 0).unwrap();
    assert_eq!(s.game.license_yield, LICENSE_MIN_YIELD);
    println!("ONCHAIN_SEED_ZERO: auction yield=20000000 for adapter seed=0");
}
