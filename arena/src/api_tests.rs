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
    let recovered = h_join(&state, gid, &json!({"name": "Demo0", "model": "demo-0", "prompt": "regression", "recover": true, "recovery_secret": joined["recovery_secret"]}));
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
    let joined = h_join(&state, gid, &json!({"name":"Victim", "model":"victim", "prompt":"public", "recovery_secret":client_secret}));
    assert_eq!(joined["ok"], true);
    let token = joined["token"].as_str().unwrap().to_string();
    for secret in [Value::Null, json!("cd".repeat(32)), json!("short")] {
        let attack = h_join(&state, gid, &json!({"name":"Victim", "model":"victim", "prompt":"public", "recover":true, "recovery_secret":secret}));
        let err = attack["error"].as_str().unwrap();
        // неверный секрет отклоняется на любом этапе: bad_params (формат),
        // unknown_agent (чужая личность) или bad_recovery_secret (подбор)
        assert!(matches!(err, "bad_params" | "unknown_agent" | "bad_recovery_secret"), "{err}");
        assert_eq!(attack["ok"], false);
        assert_eq!(state.games.lock().unwrap()[&gid].agents[0].token, token);
    }
    save_snapshot(&state).unwrap();
    let snapshot = std::fs::read_to_string(&state.snapshot_path).unwrap();
    assert!(!snapshot.contains(&client_secret));
    let restored = restore(&state);
    let recovered = h_join(&restored, gid, &json!({"name":"Victim", "model":"victim", "prompt":"public", "recover":true, "recovery_secret":client_secret}));
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
    let mut body = json!({"name":"Demo0", "model":"demo-0", "prompt":"regression", "recover":true});
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

#[test]
fn public_action_ids_survive_repeated_polls_trim_and_restart() {
    let state = isolated();
    let gid = create(&state);
    let joined = join(&state, gid, 0);
    let empty = h_state(&state, gid);
    assert_eq!(empty["state"]["recent_actions_range"], json!({"first_seq":null, "last_seq":null, "retained_first_seq":null, "limit":12}));
    // Identical rejected attempts are distinct events, even within one clock second.
    let body = json!({"token":joined["token"], "action":"produce"});
    for seq in 1..=(MAX_ACTION_LOG as u64 + 14) {
        let response = h_act(&state, gid, &body);
        assert_eq!(response["ok"], false, "{response}");
        assert_eq!(response["state"]["recent_actions"].as_array().unwrap().last().unwrap()["seq"], seq);
    }
    let observed = h_state(&state, gid)["state"].clone();
    let actions = observed["recent_actions"].as_array().unwrap();
    let last = MAX_ACTION_LOG as u64 + 14;
    assert_eq!(actions.len(), 12);
    assert_eq!(observed["recent_actions_range"], json!({"first_seq":last-11, "last_seq":last, "retained_first_seq":15, "limit":12}));
    // A client whose cursor is less than first_seq - 1 missed actions.
    assert!(observed["recent_actions_range"]["first_seq"].as_u64().unwrap() > 2);
    assert_eq!(state.games.lock().unwrap()[&gid].action_log.len(), MAX_ACTION_LOG);
    assert_eq!(h_state(&state, gid)["state"]["recent_actions"], observed["recent_actions"]);
    let restored = restore(&state);
    assert_eq!(h_state(&restored, gid)["state"]["recent_actions"], observed["recent_actions"]);
    let next = h_act(&restored, gid, &body);
    let event = next["state"]["recent_actions"].as_array().unwrap().last().unwrap();
    assert_eq!(event["seq"], last+1);
    assert_eq!(event["event_id"], format!("{}:{}:{}", gid, observed["party_no"], last+1));
    let other_gid = create(&restored);
    let other = join(&restored, other_gid, 0);
    let other_response = h_act(&restored, other_gid, &json!({"token":other["token"], "action":"produce"}));
    assert_eq!(other_response["state"]["recent_actions"][0]["seq"], 1);
    assert_ne!(other_response["state"]["recent_actions"][0]["event_id"], actions[0]["event_id"]);
}

#[test]
fn public_action_legacy_migration_is_deterministic_and_rejects_invalid_sequences() {
    let state = isolated();
    let gid = create(&state);
    let joined = join(&state, gid, 0);
    let body = json!({"token":joined["token"], "action":"produce"});
    h_act(&state, gid, &body);
    h_act(&state, gid, &body);
    let mut legacy: Value = serde_json::from_str(&std::fs::read_to_string(&state.snapshot_path).unwrap()).unwrap();
    for action in legacy["games"][0]["action_log"].as_array_mut().unwrap() {
        action.as_object_mut().unwrap().remove("seq");
    }
    std::fs::write(&state.snapshot_path, legacy.to_string()).unwrap();
    let migrated = restore(&state);
    let first = h_state(&migrated, gid)["state"]["recent_actions"].clone();
    assert_eq!(first[0]["seq"], 1);
    assert_eq!(first[1]["seq"], 2);
    assert_eq!(h_state(&restore(&state), gid)["state"]["recent_actions"], first);
    h_act(&migrated, gid, &body);
    assert_eq!(h_state(&restore(&state), gid)["state"]["recent_actions"][2]["seq"], 3);
    for (a, b) in [(json!(1), json!(1)), (json!(1), Value::Null), (json!(0), json!(1)), (json!(1), json!(3))] {
        let broken = isolated();
        let mut snapshot = legacy.clone();
        snapshot["games"][0]["action_log"][0]["seq"] = a;
        snapshot["games"][0]["action_log"][1]["seq"] = b;
        std::fs::create_dir_all(broken.snapshot_path.parent().unwrap()).unwrap();
        std::fs::write(&broken.snapshot_path, snapshot.to_string()).unwrap();
        assert!(load_snapshot(&broken).is_err(), "invalid sequence was accepted");
    }
}

#[test]
fn public_action_storage_failure_rolls_back_events_and_rule_changes() {
    let state = isolated();
    let gid = create(&state);
    let joined = join(&state, gid, 0);
    let body = json!({"token":joined["token"], "action":"produce"});
    h_act(&state, gid, &body);
    let before = h_state(&state, gid)["state"]["recent_actions"].clone();
    let tmp = state.snapshot_path.with_extension("json.tmp");
    std::fs::create_dir(&tmp).unwrap();
    assert_eq!(h_act(&state, gid, &body)["error"], "storage_failed");
    assert_eq!(h_state(&state, gid)["state"]["recent_actions"], before);
    assert_eq!(h_state(&restore(&state), gid)["state"]["recent_actions"], before);
    state.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase = Phase::Action;
    let factions = borsh::to_vec(&state.games.lock().unwrap()[&gid].sim.factions).unwrap();
    assert_eq!(h_act(&state, gid, &body)["error"], "storage_failed");
    assert_eq!(borsh::to_vec(&state.games.lock().unwrap()[&gid].sim.factions).unwrap(), factions);
    assert_eq!(h_state(&state, gid)["state"]["recent_actions"], before);
    std::fs::remove_dir(&tmp).unwrap();
    let retry = h_act(&state, gid, &body);
    assert_eq!(retry["ok"], true);
    assert_eq!(retry["state"]["recent_actions"][1]["seq"], 2);
    let saved = h_state(&restore(&state), gid);
    assert_eq!(saved["state"]["recent_actions"], retry["state"]["recent_actions"]);
}

#[test]
fn public_action_projection_stays_private_and_sequence_never_wraps() {
    let state = isolated();
    let gid = create(&state);
    let joined = join(&state, gid, 0);
    let body = json!({"token":joined["token"], "action":"produce"});
    h_act(&state, gid, &body);
    {
        let mut games = state.games.lock().unwrap();
        let action = &mut games.get_mut(&gid).unwrap().action_log[0];
        action["params"] = json!({"tight":true, "to":4, "license_yield":999});
        action["detail"] = json!({"secret":"hidden"});
        action["seq"] = json!(u64::MAX);
    }
    let public = h_state(&state, gid);
    let event = &public["state"]["recent_actions"][0];
    let keys: std::collections::BTreeSet<_> = event.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["event_id", "seq", "round", "phase", "actor", "action", "by", "ok", "ts"].into_iter().collect());
    let before = borsh::to_vec(&state.games.lock().unwrap()[&gid].sim.game).unwrap();
    assert_eq!(h_act(&state, gid, &body)["error"], "event_seq_exhausted");
    assert_eq!(borsh::to_vec(&state.games.lock().unwrap()[&gid].sim.game).unwrap(), before);
    assert_eq!(state.games.lock().unwrap()[&gid].action_log.len(), 1);
}

#[test]
fn concurrent_public_actions_have_unique_persisted_sequences() {
    let state = isolated();
    let gid = create(&state);
    let joined = join(&state, gid, 0);
    let mut threads = Vec::new();
    for _ in 0..24 {
        let state = Arc::clone(&state);
        let body = json!({"token":joined["token"], "action":"produce"});
        threads.push(std::thread::spawn(move || {
            let response = h_act(&state, gid, &body);
            assert_eq!(response["ok"], false);
            response["state"]["recent_actions"].as_array().unwrap().last().unwrap()["seq"].as_u64().unwrap()
        }));
    }
    let mut seqs: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
    seqs.sort_unstable();
    assert_eq!(seqs, (1..=24).collect::<Vec<_>>());
    assert_eq!(state.games.lock().unwrap()[&gid].action_log, restore(&state).games.lock().unwrap()[&gid].action_log);
}

#[test]
fn public_action_finished_export_keeps_live_event_identity() {
    let state = isolated();
    let gid = create(&state);
    let joined = join(&state, gid, 0);
    join(&state, gid, 1);
    let response = h_act(&state, gid, &json!({"token":joined["token"], "action":"produce"}));
    let event_id = response["state"]["recent_actions"][0]["event_id"].clone();
    for _ in 0..19 {
        state.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase_ends_at = 0;
        crank_once(&state);
    }
    let finished = h_state(&state, gid);
    assert_eq!(finished["finished"], true);
    assert_eq!(finished["result"]["actions"][0]["seq"], 1);
    let events = finished["result"]["events"].as_array().unwrap();
    let movement = events.iter().find(|e| e["type"] == "MOVE").unwrap();
    assert_eq!(movement["seq"], 1);
    assert_eq!(movement["event_id"], event_id);
    assert_eq!(movement["ok"], false);
    assert_eq!(h_export(&restore(&state)), h_export(&state));
}
