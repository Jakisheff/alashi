// Isolated audit probes. Loaded as a child module of a COPY of arena/src/api.rs.
// Never loads production snapshots, binds sockets, or talks to Solana.
use super::*;
use serde_json::json;

fn isolated(tag: &str) -> (Arc<AppState>, PathBuf) {
    let p = std::env::temp_dir().join(format!("alashi-review-{}-{tag}", std::process::id()));
    (new_state_with_files(p.join("state.json"), p.join("seq")), p)
}

#[test]
fn rejected_actions_retain_arbitrary_payloads() {
    let (s, p) = isolated("retention");
    let g = h_new_game(&s, &json!({"lobby_duration":604800}));
    let id = g["game_id"].as_u64().unwrap();
    let j = h_join(&s, id, &json!({"name":"audit", "model":"audit", "prompt":"audit"}));
    assert_eq!(j["ok"], true);
    let body = json!({"token":j["token"], "action":"produce", "params":{"unused":"x".repeat(65536)}});
    for _ in 0..32 { assert_eq!(h_act(&s, id, &body)["ok"], false); }
    let games = s.games.lock().unwrap();
    assert_eq!(games[&id].action_log.len(), 32);
    let retained = serde_json::to_vec(&games[&id].action_log).unwrap().len();
    assert!(retained > 2_097_152);
    drop(games);
    save_snapshot(&s).unwrap();
    println!("32 rejected requests: retained={retained} bytes, snapshot={} bytes", std::fs::metadata(p.join("state.json")).unwrap().len());
}

#[test]
fn creation_has_no_bound_at_connection_limit() {
    let (s, _) = isolated("creation");
    for _ in 0..80 { assert_eq!(h_new_game(&s, &json!({"lobby_duration":604800}))["ok"], true); }
    assert_eq!(s.games.lock().unwrap().len(), 80);
    println!("80 unauthenticated long-lived games accepted by handler");
}

#[test]
fn corrupt_snapshot_is_replaced_by_empty_state() {
    let (s, p) = isolated("corrupt");
    assert_eq!(h_new_game(&s, &json!({}))["ok"], true);
    save_snapshot(&s).unwrap();
    std::fs::write(p.join("state.json"), "{incomplete").unwrap();
    let restored = new_state_with_files(p.join("state.json"), p.join("seq"));
    load_snapshot(&restored);
    assert!(restored.games.lock().unwrap().is_empty());
    save_snapshot(&restored).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(p.join("state.json")).unwrap()).unwrap();
    assert_eq!(v["games"].as_array().unwrap().len(), 0);
    println!("Malformed snapshot loads empty and next save replaces it with zero games");
}

#[test]
fn failed_snapshot_keeps_join_mutation() {
    let (s, p) = isolated("failed-save");
    std::fs::create_dir_all(p.join("state.json")).unwrap();
    let id = h_new_game(&s, &json!({}))["game_id"].as_u64().unwrap();
    let body = json!({"name":"audit", "model":"audit", "prompt":"audit"});
    assert_eq!(h_join(&s, id, &body)["ok"], true);
    assert!(save_snapshot(&s).is_err());
    assert_eq!(s.games.lock().unwrap()[&id].agents.len(), 1);
    assert_eq!(h_join(&s, id, &body)["ok"], false);
    println!("Failed snapshot does not roll back join; retry is rejected");
}

#[test]
fn zero_seed_adapter_changes_epoch_economics() {
    use alashi_rules::transitions;
    let mut g = Game { phase:Phase::Market, round:AUCTION_ROUND, epoch:EPOCH_90S, ..Game::default() };
    transitions::advance(&mut g, &mut [], 100, 11, 0, None).unwrap();
    assert_eq!(g.license_yield, LICENSE_MIN_YIELD);
    let mut alternate = Game { phase:Phase::Market, round:AUCTION_ROUND, epoch:EPOCH_90S, ..Game::default() };
    transitions::advance(&mut alternate, &mut [], 100, 11, 12345, None).unwrap();
    assert_eq!(alternate.license_yield, LICENSE_MIN_YIELD + 12345);
    let mut a = Faction { wallet:Pubkey::new_from_array([1;32]), alive:true, goods:10, grey_goods:3, roof_tariff:ROOF_RED, ..Faction::default() };
    let mut b = Faction { wallet:Pubkey::new_from_array([2;32]), alive:true, ..Faction::default() };
    let mut vrf = Game { phase:Phase::Action, round:1, epoch:EPOCH_90S, entropy_mode:ENTROPY_SWITCHBOARD, faction_count:2, ..Game::default() };
    transitions::advance(&mut vrf, &mut [&mut a, &mut b], 100, 11, 0, Some((Pubkey::new_from_array([3;32]),10))).unwrap();
    assert_eq!(a.goods, 0);
    assert_eq!(vrf.law_card, NO_LAW);
    println!("Onchain adapter's seed=0: license={}, red-roof goods destroyed BEFORE VRF reveal", g.license_yield);
}

#[test]
fn joined_identity_is_a_public_self_assertion() {
    assert_eq!(agent_id_of("a|b", "c"), agent_id_of("a", "b|c"));
    let (s, _) = isolated("identity");
    let id = h_new_game(&s, &json!({}))["game_id"].as_u64().unwrap();
    for n in 0..6 {
        assert_eq!(h_join(&s, id, &json!({"name":"audit", "model":"same-model", "prompt":format!("variant-{n}")}))["ok"], true);
    }
    println!("One caller filled six seats; ambiguous identity encoding also confirmed");
}

#[test]
fn timestamp_seed_recovered_from_public_law_cards() {
    use alashi_rules::logic::draw_law_index;
    let (s, _) = isolated("seed");
    // Observation is made through the actual simulator used by the HTTP adapter.
    // Only game IDs and drawn law cards are passed to the candidate search.
    let observation_time = now() as u64;
    let mut observations = Vec::new();
    for _ in 0..3 {
        let id = h_new_game(&s, &json!({"epoch":"classic"}))["game_id"].as_u64().unwrap();
        let mut games = s.games.lock().unwrap();
        let entry = games.get_mut(&id).unwrap();
        let mut cards = Vec::new();
        for round in 1..=6 {
            entry.sim.game.phase = Phase::Action;
            entry.sim.game.round = round;
            entry.sim.game.phase_ends_at = 0;
            let seed = splitmix64(game_seed(&s, id) ^ round as u64);
            entry.sim.advance(100, seed).unwrap();
            cards.push(entry.sim.game.law_card);
        }
        observations.push((id, cards));
    }
    let candidates: Vec<u64> = (observation_time.saturating_sub(86400)..=observation_time).filter(|&candidate| {
        observations.iter().all(|(id, cards)| {
            let per_game = splitmix64(candidate ^ splitmix64(*id));
            let mut mask = 0;
            cards.iter().enumerate().all(|(round, &observed)| {
                let (card, next) = draw_law_index(splitmix64(per_game ^ (round as u64 + 1)), mask);
                mask = next;
                card == observed
            })
        })
    }).collect();
    assert_eq!(candidates.len(), 1);
    // Verify AFTER search, never give this value to the search itself.
    assert_eq!(candidates[0], s.master_seed.load(Ordering::Relaxed));
    println!("Recovered unique server seed among 86401 timestamp candidates using 18 public law cards; predicts all game seeds");
}
