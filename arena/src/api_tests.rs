use super::*;
use serde_json::{json, Value};

fn isolated() -> Arc<AppState> {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let dir = std::env::temp_dir().join(format!(
        "alashi_demo_{}_{}_{}", std::process::id(),
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos(),
        NEXT.fetch_add(1, Ordering::SeqCst),
    ));
    new_state_with_files(dir.join("state.json"), dir.join("seq"))
}

fn create(state: &AppState) -> u64 {
    let result = h_new_game(state, &json!({"phase_duration": 30, "grace_s": 0}));
    assert_eq!(result["ok"], true, "{result}");
    result["game_id"].as_u64().unwrap()
}

fn join(state: &AppState, gid: u64, i: usize) -> Value {
    let result = h_join(state, gid, &json!({"name": format!("Demo{i}"), "model": format!("demo-{i}"), "prompt": "regression"}));
    assert_eq!(result["ok"], true, "{result}");
    result
}

fn restore(state: &AppState) -> Arc<AppState> {
    let other = new_state_with_files(state.snapshot_path.clone(), state.sequence_path.clone());
    load_snapshot(&other).unwrap();
    other
}

#[test]
fn invalid_creation_parameters_cannot_reach_the_crank() {
    let state = isolated();
    for body in [
        json!({"entry_fee": 1u64 << 63}), json!({"entry_fee": 0}),
        json!({"phase_duration": i64::MAX}), json!({"lobby_duration": i64::MAX}),
        json!({"lobby_duration": -1}), json!({"vote_weight_mode": 256}),
    ] {
        assert_eq!(h_new_game(&state, &body)["error"], "bad_params", "{body}");
    }
    assert!(state.games.lock().unwrap().is_empty());
    let good = h_new_game(&state, &json!({"entry_fee": MAX_ENTRY_FEE}));
    assert_eq!(good["ok"], true);
}

#[test]
fn recover_full_started_game_revokes_old_token_and_survives_restart() {
    let state = isolated();
    let gid = create(&state);
    let joined = join(&state, gid, 0);
    let old_token = joined["token"].as_str().unwrap().to_string();
    for i in 1..6 { join(&state, gid, i); }
    crank_once(&state);
    assert_eq!(state.games.lock().unwrap()[&gid].sim.game.phase, Phase::Market);
    let recovered = h_join(&state, gid, &json!({"model": "demo-0", "prompt": "regression", "recover": true, "recovery_secret": joined["recovery_secret"]}));
    assert_eq!(recovered["recovered"], true, "{recovered}");
    assert_eq!(state.games.lock().unwrap()[&gid].sim.factions.len(), 6);
    assert_eq!(h_act(&state, gid, &json!({"token": old_token, "action": "produce"}))["error"], "bad_token");
    save_snapshot(&state).unwrap();
    let restored = restore(&state);
    restored.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase = Phase::Action;
    assert_eq!(h_act(&restored, gid, &json!({"token": recovered["token"], "action": "produce"}))["ok"], true);
    assert_eq!(h_join(&restored, gid, &json!({"model": "stranger", "recover": true}))["error"], "unknown_agent");
    assert_eq!(restored.games.lock().unwrap()[&gid].sim.factions.len(), 6);
}

#[test]
fn completed_history_and_saved_hint_prevent_reused_ids() {
    let state = isolated();
    state.next_id.store(42, Ordering::SeqCst);
    state.completed.lock().unwrap().push(json!({"game_id": 17, "party_no": 90}).to_string());
    save_snapshot(&state).unwrap();
    let restored = restore(&state);
    assert_eq!(create(&restored), 42);
    assert_eq!(h_state(&restored, 17)["result"]["party_no"], 90);
    // Older snapshots might omit next_id_hint entirely.
    // Аудит 27.09 (S4): create теперь устойчиво пишет снимок, поэтому
    // legacy-файл собираем заново — нет hint, нет games, только completed.
    let legacy = json!({
        "v": 1,
        "games": [],
        "completed": [json!({"game_id": 17, "party_no": 90}).to_string()],
    });
    std::fs::write(&state.snapshot_path, legacy.to_string()).unwrap();
    let restored = restore(&state);
    assert_eq!(create(&restored), 18);
}

#[test]
fn automatic_phase_and_entropy_checkpoint_survive_without_a_post() {
    let state = isolated();
    state.master_seed.store(123456789, Ordering::SeqCst);
    let gid = create(&state);
    join(&state, gid, 0); join(&state, gid, 1);
    state.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase_ends_at = 0;
    save_snapshot(&state).unwrap();
    crank_once(&state);
    let restored = restore(&state);
    assert_eq!(restored.master_seed.load(Ordering::SeqCst), 123456789);
    let a = state.games.lock().unwrap();
    let b = restored.games.lock().unwrap();
    assert_eq!(a[&gid].sim.game.phase, Phase::Market);
    assert_eq!(borsh::to_vec(&a[&gid].sim.game).unwrap(), borsh::to_vec(&b[&gid].sim.game).unwrap());
    assert_eq!(a[&gid].phase_log, b[&gid].phase_log);
}

#[test]
fn invalid_legacy_settlement_does_not_stop_another_full_party() {
    let state = isolated();
    let bad = create(&state);
    let good = create(&state);
    for gid in [bad, good] { join(&state, gid, 0); join(&state, gid, 1); }
    {
        let mut games = state.games.lock().unwrap();
        let bad_game = games.get_mut(&bad).unwrap();
        bad_game.entry_fee = 1u64 << 63;
        bad_game.sim.game.phase = Phase::Finished;
    }
    for _ in 0..19 {
        state.games.lock().unwrap().get_mut(&good).unwrap().sim.game.phase_ends_at = 0;
        crank_once(&state);
    }
    let failed = h_state(&state, bad);
    assert!(failed["state"]["settlement_error"].is_string());
    let result = h_state(&state, good);
    assert_eq!(result["finished"], true, "{result}");
    let payouts: u64 = result["result"]["payouts"].as_array().unwrap().iter().map(|p| p.as_u64().unwrap()).sum();
    assert_eq!(payouts + result["result"]["rake"].as_u64().unwrap(), 20 * PESO);
    let restored = restore(&state);
    assert!(h_state(&restored, bad)["state"]["settlement_error"].is_string());
    assert_eq!(h_state(&restored, good)["finished"], true);
}

#[test]
fn negative_ratings_sort_by_score_and_not_hashmap_order() {
    let state = isolated();
    state.completed.lock().unwrap().push(json!({
        "game_id": 1, "ranks": [0,1,2,3,4,5], "payouts": [0,0,0,0,0,0],
        "agents": (0..6).map(|i| json!({"agent_id": format!("agent-{i}"), "name": "Demo", "model": "test"})).collect::<Vec<_>>()
    }).to_string());
    for _ in 0..10 {
        let result = h_leaderboard(&state);
        let rows = result["leaderboard"].as_array().unwrap();
        assert!(rows[4]["plackett_luce_ordinal"].as_f64().unwrap() < 0.0);
        assert_eq!(rows[4]["agent_id"], "agent-4");
        assert_eq!(rows[5]["agent_id"], "agent-5");
    }
}

#[test]
fn simultaneous_creators_get_distinct_party_numbers() {
    let state = isolated();
    let threads: Vec<_> = (0..12).map(|_| {
        let state = Arc::clone(&state);
        std::thread::spawn(move || create(&state))
    }).collect();
    for thread in threads { thread.join().unwrap(); }
    let games = state.games.lock().unwrap();
    assert_eq!(games.len(), 12);
    let parties: std::collections::HashSet<_> = games.values().map(|g| g.party_no).collect();
    assert_eq!(parties.len(), 12);
}

#[test]
fn recovery_requires_secret_and_persists_only_its_hash() {
    let state = isolated();
    let gid = create(&state);
    let client_secret = "ab".repeat(32);
    let joined = h_join(&state, gid, &json!({"model":"victim", "prompt":"public", "recovery_secret":client_secret}));
    assert_eq!(joined["ok"], true);
    let token = joined["token"].as_str().unwrap();
    for secret in [Value::Null, json!("cd".repeat(32)), json!("short")] {
        let attack = h_join(&state, gid, &json!({"model":"victim", "prompt":"public", "recover":true, "recovery_secret":secret}));
        assert_eq!(attack["error"], "bad_recovery_secret");
        assert_eq!(state.games.lock().unwrap()[&gid].agents[0].token, token);
    }
    save_snapshot(&state).unwrap();
    let snapshot = std::fs::read_to_string(&state.snapshot_path).unwrap();
    assert!(!snapshot.contains(&client_secret));
    let restored = restore(&state);
    let recovered = h_join(&restored, gid, &json!({"model":"victim", "prompt":"public", "recover":true, "recovery_secret":client_secret}));
    assert_eq!(recovered["ok"], true);
    assert_ne!(recovered["token"], token);
    assert!(!h_state(&restored, gid).to_string().contains(&client_secret));
}

#[test]
fn legacy_recovery_needs_current_token_to_enroll_secret() {
    let state = isolated();
    let gid = create(&state);
    let joined = join(&state, gid, 0);
    state.games.lock().unwrap().get_mut(&gid).unwrap().agents[0].recovery_hash = None;
    save_snapshot(&state).unwrap();
    let restored = restore(&state);
    let mut body = json!({"model":"demo-0", "prompt":"regression", "recover":true});
    assert_eq!(h_join(&restored, gid, &body)["error"], "bad_recovery_secret");
    body["token"] = joined["token"].clone();
    let enrolled = h_join(&restored, gid, &body);
    assert_eq!(enrolled["ok"], true);
    assert_eq!(enrolled["recovery_secret"].as_str().unwrap().len(), 64);
    assert_eq!(h_join(&restored, gid, &body)["error"], "bad_recovery_secret");
    body["recovery_secret"] = enrolled["recovery_secret"].clone();
    assert_eq!(h_join(&restored, gid, &body)["ok"], true);
}

#[test]
fn oversized_action_units_are_rejected_without_mutating_game() {
    let state = isolated();
    let gid = create(&state);
    let joined = join(&state, gid, 0);
    let before = borsh::to_vec(&state.games.lock().unwrap()[&gid].sim.game).unwrap();
    for (action, field) in [("sell","units"), ("sell_credit","units"), ("buy","units"), ("barter_propose","goods")] {
        for value in [json!(65536), json!(u64::MAX), json!(-1), json!("1")] {
            let params = json!({field: value});
            assert_eq!(h_act(&state, gid, &json!({"token": joined["token"], "action":action, "params":params}))["error"], "bad_params");
        }
    }
    assert_eq!(borsh::to_vec(&state.games.lock().unwrap()[&gid].sim.game).unwrap(), before);
}

#[test]
fn connection_permits_are_bounded_and_released() {
    let counter = Arc::new(AtomicU64::new(0));
    let permits: Vec<_> = (0..MAX_CONNECTIONS).map(|_| Permit::acquire(&counter, MAX_CONNECTIONS).unwrap()).collect();
    assert!(Permit::acquire(&counter, MAX_CONNECTIONS).is_none());
    drop(permits);
    assert_eq!(counter.load(Ordering::SeqCst), 0);
    assert!(Permit::acquire(&counter, MAX_CONNECTIONS).is_some());
}

#[test]
fn presidency_and_contribution_projection_follow_reachable_state() {
    let state = isolated();
    let gid = create(&state);
    let episode = crate::presidency::prepare(1, crate::presidency::Benefit::Positive);
    let mut games = state.games.lock().unwrap();
    let entry = games.get_mut(&gid).unwrap();
    entry.sim = episode.sim;
    let s = state_json(gid, entry);
    assert_eq!(s["president_idx"],1);
    assert_eq!(s["factions"][0]["is_president"],false);
    assert_eq!(s["factions"][1]["is_president"],true);
    assert_eq!(s["factions"][0]["vote_weight"],12);
    assert_eq!(s["factions"][1]["vote_weight"],11);
    assert_eq!(s["vote_weight_mode"],1);
    assert!(s["factions"][0]["acted_stamp"].is_u64());
}
