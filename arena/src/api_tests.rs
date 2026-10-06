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

fn registration_fixture(state: &AppState, gid: u64) -> (Value, Value, Value, Value) {
    let wallet = Pubkey::new_unique().to_string();
    let mut body = json!({"name":"Receipt", "model":"fixture", "prompt":"fixture strategy",
        "recovery_secret":"ab".repeat(32), "wallet":wallet});
    let proposal = h_registration(state, gid, &body);
    assert_eq!(proposal["ok"], true, "{proposal}");
    body.as_object_mut().unwrap().remove("wallet");
    body["registration"] = json!({"wallet":wallet,"signature":"2".repeat(88)});
    let status = json!({"value":[{"slot":123,"err":null,"confirmationStatus":"confirmed"}]});
    let tx = json!({"slot":123,"meta":{"err":null,"fee":5000},"transaction":{
        "signatures":[body["registration"]["signature"]],
        "message":{"accountKeys":[{"pubkey":wallet,"signer":true}],
            "instructions":[{"programId":registration::MEMO_PROGRAM_ID,"parsed":proposal["memo"]}]}
    }});
    (body,proposal,status,tx)
}

#[test]
fn registration_proposal_is_read_only_and_requires_exact_identity_fields() {
    let mut state = isolated();
    Arc::get_mut(&mut state).unwrap().require_devnet_registration = true;
    let gid = create(&state);
    let before = std::fs::read(&state.snapshot_path).unwrap();
    let (body,proposal,_,_) = registration_fixture(&state,gid);
    assert_eq!(proposal["required"],true);
    assert_eq!(root_doc(&state)["registration"]["required"],true);
    assert!(proposal.get("token").is_none());
    assert!(proposal.get("recovery_secret").is_none());
    assert!(!proposal.to_string().contains(body["recovery_secret"].as_str().unwrap()));
    assert_eq!(std::fs::read(&state.snapshot_path).unwrap(),before);
    assert!(state.games.lock().unwrap()[&gid].agents.is_empty());
    let mut request=body.clone();
    request["wallet"]=request["registration"]["wallet"].clone();
    request.as_object_mut().unwrap().remove("registration");
    for key in ["name","model","prompt","recovery_secret","wallet"] {
        let mut invalid=request.clone(); invalid.as_object_mut().unwrap().remove(key);
        assert_eq!(h_registration(&state,gid,&invalid)["ok"],false,"{key}");
    }
    request["owner_key"]=json!("cd".repeat(32));
    assert_eq!(h_registration(&state,gid,&request)["ok"],false);
}

#[test]
fn registration_is_persisted_public_and_recovery_and_actions_need_no_rpc() {
    let mut state = isolated();
    Arc::get_mut(&mut state).unwrap().require_devnet_registration=true;
    let gid=create(&state);
    let (body,_,status,tx)=registration_fixture(&state,gid);
    let joined=h_join_with_verifier(&state,gid,&body,|proof,memo| {
        // These would fail immediately if verification held either global lock.
        assert!(state.snapshot_lock.try_lock().is_ok());
        assert!(state.games.try_lock().is_ok());
        registration::validate_receipt(proof,memo,&status,&tx)
    });
    assert_eq!(joined["ok"],true,"{joined}");
    assert_eq!(joined["registration"]["fee_lamports"],"5000");
    let state=restore(&state);
    let receipt=joined["registration"].clone();
    assert_eq!(h_state(&state,gid)["state"]["factions"][0]["registration"],receipt);
    let mut recover=body.clone();
    recover["recover"]=json!(true);
    recover.as_object_mut().unwrap().remove("registration");
    // Model updates do not replace the registered strategy/session identity.
    recover["model"]=json!("updated model");
    let recovered=h_join_with_verifier(&state,gid,&recover,|_,_| panic!("recovery must not call RPC"));
    assert_eq!(recovered["ok"],true,"{recovered}");
    assert_eq!(recovered["registration"],receipt);
    assert_eq!(recovered["agent_id"],joined["agent_id"]);
    recover["registration"]=body["registration"].clone();
    assert_eq!(h_join_with_verifier(&state,gid,&recover,|_,_| panic!("stored proof needs no RPC"))["ok"],true);
    recover["registration"]["signature"]=json!("3".repeat(88));
    assert_eq!(h_join_with_verifier(&state,gid,&recover,|_,_| panic!("mismatch must not call RPC"))["error"],"registration_recovery_mismatch");
    recover.as_object_mut().unwrap().remove("registration");
    recover["recovery_secret"]=json!("ef".repeat(32));
    assert_eq!(h_join_with_verifier(&state,gid,&recover,|_,_| panic!("bad recovery must not call RPC"))["ok"],false);
    // Recovery revoked the old token. Actions use only the HTTP credential path.
    recover["recovery_secret"]=body["recovery_secret"].clone();
    let session=h_join_with_verifier(&state,gid,&recover,|_,_| panic!("no RPC"));
    state.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase=Phase::Action;
    let acted=h_act(&state,gid,&json!({"token":session["token"],"action":"produce"}));
    assert_eq!(acted["ok"],true,"{acted}");
    let public=h_state(&state,gid)["state"].clone();
    assert_eq!(public["execution_mode"],"http_simulated");
    assert_eq!(public["recent_actions"][0]["seq"],1);
    assert!(!public.to_string().contains(session["token"].as_str().unwrap()));
    assert!(!public.to_string().contains(body["recovery_secret"].as_str().unwrap()));
    state.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase=Phase::Finished;
    settle_and_record_locked_for_test(&state,gid);
    let completed=h_export(&state);
    let exported:Value=serde_json::from_str(&completed).unwrap();
    assert_eq!(exported["agents"][0]["registration"],receipt);
    assert_eq!(exported["execution_mode"],"http_simulated");
    assert_eq!(restore(&state).completed.lock().unwrap().len(),1);
}

fn settle_and_record_locked_for_test(state:&AppState,gid:u64) {
    let _lock=state.snapshot_lock.lock().unwrap();
    assert_eq!(settle_and_record_locked(state,gid),Ok(()));
}

#[test]
fn public_signature_replay_with_different_secret_game_or_strategy_is_rejected() {
    let state=isolated();
    let gid=create(&state);
    let (body,_,status,tx)=registration_fixture(&state,gid);
    for (field,value) in [("recovery_secret","cd".repeat(32)),("model","other".into()),("name","Other".into()),("prompt","other".into())] {
        let mut replay=body.clone();replay[field]=json!(value);
        let denied=h_join_with_verifier(&state,gid,&replay,|p,m|registration::validate_receipt(p,m,&status,&tx));
        assert_eq!(denied["error"],"registration_memo_mismatch","{denied}");
    }
    let other=create(&state);
    assert_eq!(h_join_with_verifier(&state,other,&body,|p,m|registration::validate_receipt(p,m,&status,&tx))["error"],"registration_memo_mismatch");
    let mut conflict=body.clone();conflict["owner_key"]=json!("cd".repeat(32));
    assert_eq!(h_join_with_verifier(&state,gid,&conflict,|_,_|panic!("owner key rejected before RPC"))["error"],"registration_owner_key_forbidden");
    assert!(state.games.lock().unwrap()[&gid].agents.is_empty());
}

#[test]
fn mandatory_registration_grandfathers_only_authenticated_legacy_recovery() {
    let mut state=isolated();
    let gid=create(&state);
    let legacy=join(&state,gid,0);
    Arc::get_mut(&mut state).unwrap().require_devnet_registration=true;
    assert_eq!(h_join_with_verifier(&state,gid,&json!({"name":"New","model":"new","prompt":""}),|_,_|panic!("missing proof no RPC"))["error"],"registration_required");
    let recovered=h_join_with_verifier(&state,gid,&json!({"name":"Demo0","model":"demo-0","prompt":"regression",
        "recover":true,"recovery_secret":legacy["recovery_secret"]}),|_,_|panic!("legacy recovery no RPC"));
    assert_eq!(recovered["ok"],true,"{recovered}");
    assert_eq!(recovered["registration"],Value::Null);
    assert_eq!(recovered["state"]["factions"][0]["registration"],Value::Null);
}

#[test]
fn verification_failure_and_party_change_do_not_mutate_membership() {
    let state=isolated();
    let gid=create(&state);
    let (body,_,status,tx)=registration_fixture(&state,gid);
    for code in ["registration_rpc_timeout","registration_transaction_failed","registration_not_confirmed"] {
        assert_eq!(h_join_with_verifier(&state,gid,&body,|_,_|Err(code))["error"],code);
    }
    assert!(state.games.lock().unwrap()[&gid].agents.is_empty());
    let denied=h_join_with_verifier(&state,gid,&body,|p,m| {
        let receipt=registration::validate_receipt(p,m,&status,&tx)?;
        state.games.lock().unwrap().get_mut(&gid).unwrap().party_no+=1;
        Ok(receipt)
    });
    assert_eq!(denied["error"],"registration_game_changed");
    assert!(state.games.lock().unwrap()[&gid].agents.is_empty());
}

#[test]
fn registration_storage_failure_rolls_back_receipt_and_allows_same_signature_retry() {
    let state=isolated();
    let gid=create(&state);
    let (body,_,status,tx)=registration_fixture(&state,gid);
    std::fs::remove_file(&state.snapshot_path).unwrap();
    std::fs::create_dir(&state.snapshot_path).unwrap();
    let failed=h_join_with_verifier(&state,gid,&body,|p,m|registration::validate_receipt(p,m,&status,&tx));
    assert_eq!(failed["error"],"storage_failed");
    assert!(state.games.lock().unwrap()[&gid].agents.is_empty());
    std::fs::remove_dir(&state.snapshot_path).unwrap();
    let retry=h_join_with_verifier(&state,gid,&body,|p,m|registration::validate_receipt(p,m,&status,&tx));
    assert_eq!(retry["ok"],true,"{retry}");
    assert_eq!(restore(&state).games.lock().unwrap()[&gid].agents[0].registration.as_ref().unwrap().signature,body["registration"]["signature"].as_str().unwrap());
}

#[test]
fn lifecycle_v2_golden_fixture_and_durable_two_game_join() {
    let state = isolated();
    let gid1 = create(&state);
    let gid2 = create(&state);
    let id = "cd".repeat(32);
    let secret = "ab".repeat(32);
    let wallet = Pubkey::new_unique().to_string();
    let request = json!({"agent_record_id":id,"wallet":wallet,"recovery_secret":secret});
    assert_eq!(owner_id_of(&secret), "d8d041d59e9d55c61790d37a8e2bc3f17b9c8f4d350062a090ea8b5d64a086fa");
    assert_eq!(character_id_v2(&owner_id_of(&secret), &id), "6d92bd091fb2d69e295fe5bba10caa3628abf2cac55bc80f7c74a018c4465c71");
    assert_eq!(agent_id_of("glm-5.3-flash", "test"), "bcf7a4c486fd2c390bdc97df49b4cd019aeb8db20a72fa65a05361f5210c2240");
    assert_eq!(sha256_hex(&json!({"action":"produce","by":"unknown","params":{}}).to_string()),
        "a88e262c2b076da69909339a813ff4bca5eb0942a18c43d4e6b8159f43a1a46a");
    let proposal = h_registration_v2(&state, &request);
    assert_eq!(proposal["ok"],true,"{proposal}");
    assert_eq!(h_registration_v2(&state,&request)["memo"],proposal["memo"]);
    let restored = restore(&state);
    assert_eq!(h_registration_v2(&restored,&request)["memo"],proposal["memo"]);
    assert_ne!(h_registration_v2(&restored,&json!({"agent_record_id":id,"wallet":wallet,
        "recovery_secret":"ef".repeat(32)}))["memo"],proposal["memo"]);
    let proof = json!({"agent_record_id":id,"wallet":wallet,"recovery_secret":secret,"signature":"2".repeat(88)});
    let confirmed = h_confirm_v2_with_verifier(&restored,&proof,|p,m| {
        assert_eq!(p.wallet,wallet);
        assert_eq!(m,proposal["memo"]);
        Ok(Receipt { mode:"agent_start_v1".into(), network:"devnet".into(),
            wallet:p.wallet.clone(),signature:p.signature.clone(),slot:42,
            fee_lamports:"5000".into(),commitment:"confirmed".into() })
    });
    assert_eq!(confirmed["registration"]["mode"],"agent_lifecycle_v2");
    assert_eq!(h_confirm_v2_with_verifier(&restored,&proof,|_,_|panic!("repeat must not call RPC"))["registration"],confirmed["registration"]);
    let waiting = h_agent_profile(&restored,&id);
    assert_eq!(waiting["registered"],true);
    assert_eq!(waiting["active_slots"],json!([]));
    assert_eq!(waiting["character_id"],proposal["character_id"]);
    assert_eq!(h_agent_profile(&restored,&"ef".repeat(32))["error"],"unknown_agent");
    assert_eq!(h_agent_profile(&restored,"bad-id")["error"],"bad_agent_record_id");
    let restored = restore(&restored);
    let join_body = json!({"agent_record_id":id,"recovery_secret":secret,"name":"Player",
        "model":"glm-5.3-flash","strategy_hash":agent_id_of("glm-5.3-flash","test")});
    let first = h_join(&restored,gid1,&join_body);
    let second = h_join(&restored,gid2,&join_body);
    assert_eq!(first["ok"],true,"{first}");
    assert_eq!(second["ok"],true,"{second}");
    assert_eq!(first["character_id"],second["character_id"]);
    assert_eq!(first["registration"],confirmed["registration"]);
    assert_eq!(second["registration"],confirmed["registration"]);
    let profile = h_agent_profile(&restored,&id);
    assert_eq!(profile["active_slots"].as_array().unwrap().len(),2);
    let public_profile = profile.to_string();
    for private in [secret.as_str(),wallet.as_str(),first["token"].as_str().unwrap(),
        proof["signature"].as_str().unwrap(),"glm-5.3-flash"] {
        assert!(!public_profile.contains(private),"public profile leaked: {private}");
    }
    for field in ["wallet","signature","recovery_hash","token","challenge","model","prompt"] {
        assert!(profile.get(field).is_none(),"public profile field: {field}");
    }
    assert_ne!(first["token"],second["token"]);
    assert_eq!(h_join(&restored,gid1,&join_body)["error"],"already_joined");
    let mut bad=join_body.clone();
    bad["recovery_secret"]=json!("ef".repeat(32));
    assert_eq!(h_join(&restored,gid2,&bad)["error"],"bad_recovery_secret");
    let snapshot=std::fs::read_to_string(&restored.snapshot_path).unwrap();
    assert!(!snapshot.contains(first["token"].as_str().unwrap()));
    assert!(!snapshot.contains(secret.as_str()));
    assert_eq!(h_state(&restored,gid1)["state"]["factions"][0]["agent_record_id"],id);
    let mut recovery=join_body.clone(); recovery["recover"]=json!(true);
    let recovered=h_join(&restored,gid1,&recovery);
    assert_eq!(recovered["ok"],true,"{recovered}");
    assert_ne!(recovered["token"],first["token"]);
    assert_eq!(h_act(&restored,gid1,&json!({"token":first["token"],"op_id":1,"action":"produce"}))["error"],"bad_token");
    let mut changed=recovery.clone(); changed["model"]=json!("other-model");
    changed["strategy_hash"]=json!("bb".repeat(32));
    let updated=h_join(&restored,gid1,&changed);
    assert_eq!(updated["ok"],true,"{updated}");
    assert_eq!(updated["agent_id"],"bb".repeat(32));
    assert_eq!(updated["character_id"],first["character_id"]);
    assert_eq!(updated["registration"],first["registration"]);
}

#[test]
fn lifecycle_v2_action_idempotency_survives_restart_and_recovery() {
    let state=isolated();
    let gid=create(&state);
    let id="cd".repeat(32);
    let secret="ab".repeat(32);
    let wallet=Pubkey::new_unique().to_string();
    let proposal=h_registration_v2(&state,&json!({"agent_record_id":id,"wallet":wallet,"recovery_secret":secret}));
    assert_eq!(proposal["ok"],true);
    let proof=json!({"agent_record_id":id,"wallet":wallet,"recovery_secret":secret,"signature":"2".repeat(88)});
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,|p,_|Ok(Receipt {
        mode:"agent_start_v1".into(),network:"devnet".into(),wallet:p.wallet.clone(),
        signature:p.signature.clone(),slot:42,fee_lamports:"5000".into(),commitment:"confirmed".into(),
    }))["ok"],true);
    let join_body=json!({"agent_record_id":id,"recovery_secret":secret,"name":"Player",
        "model":"m","strategy_hash":"aa".repeat(32)});
    let joined=h_join(&state,gid,&join_body);
    assert_eq!(joined["ok"],true,"{joined}");
    state.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase=Phase::Action;
    let act=json!({"token":joined["token"],"op_id":1,"action":"produce","by":"llm"});
    let first=h_act(&state,gid,&act);
    assert_eq!(first["op_consumed"],true,"{first}");
    assert_eq!(h_act(&state,gid,&act),first);
    assert_eq!(state.games.lock().unwrap()[&gid].action_log.len(),1);
    let mut conflict=act.clone(); conflict["action"]=json!("donkey");
    assert_eq!(h_act(&state,gid,&conflict)["error"],"op_conflict");
    let mut gap=act.clone(); gap["op_id"]=json!(3);
    assert_eq!(h_act(&state,gid,&gap)["error"],"op_out_of_order");
    let restored=restore(&state);
    assert_eq!(h_act(&restored,gid,&act),first);
    let mut recovery=join_body.clone(); recovery["recover"]=json!(true);
    let rotated=h_join(&restored,gid,&recovery);
    let mut next=act.clone(); next["token"]=rotated["token"].clone(); next["op_id"]=json!(2);
    let second=h_act(&restored,gid,&next);
    assert_eq!(second["op_consumed"],true,"{second}");
    assert_eq!(restored.games.lock().unwrap()[&gid].action_log.len(),2);
    let mut old=act.clone(); old["token"]=rotated["token"].clone();
    assert_eq!(h_act(&restored,gid,&old),first);
    let mut cross=next.clone();
    let other=create(&restored); cross["op_id"]=json!(1);
    assert_eq!(h_act(&restored,other,&cross)["error"],"bad_token");
}

#[test]
fn lifecycle_v2_fail_closed_platform_and_proposal_storage() {
    let mut state=isolated();
    Arc::get_mut(&mut state).unwrap().require_platform_v2=true;
    let gid=create(&state);
    assert_eq!(h_join(&state,gid,&json!({"name":"legacy","model":"m","prompt":"p"}))["error"],"platform_registration_required");
    assert_eq!(h_registration(&state,gid,&json!({}))["error"],"platform_registration_required");
    let id="cd".repeat(32); let secret="ab".repeat(32);
    let wallet=Pubkey::new_unique().to_string();
    let req=json!({"agent_record_id":id,"wallet":wallet,"recovery_secret":secret});
    std::fs::remove_file(&state.snapshot_path).unwrap();
    std::fs::create_dir(&state.snapshot_path).unwrap();
    assert_eq!(h_registration_v2(&state,&req)["error"],"storage_failed");
    assert!(state.registrations.lock().unwrap().is_empty());
    std::fs::remove_dir(&state.snapshot_path).unwrap();
    let proposal=h_registration_v2(&state,&req);
    assert_eq!(proposal["ok"],true,"{proposal}");
    let proof=json!({"agent_record_id":id,"wallet":wallet,"recovery_secret":secret,"signature":"2".repeat(88)});
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,|_,_|Err("registration_memo_mismatch"))["error"],"registration_memo_mismatch");
    assert!(state.registrations.lock().unwrap().is_empty());
}

#[test]
fn lifecycle_v2_session_expires_and_old_op_ids_cannot_replay_after_cache_trim() {
    let state=isolated(); let gid=create(&state);
    let id="cd".repeat(32); let secret="ab".repeat(32);
    let wallet=Pubkey::new_unique().to_string();
    assert_eq!(h_registration_v2(&state,&json!({"agent_record_id":id,"wallet":wallet,"recovery_secret":secret}))["ok"],true);
    let proof=json!({"agent_record_id":id,"wallet":wallet,"recovery_secret":secret,"signature":"2".repeat(88)});
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,|p,_|Ok(Receipt {
        mode:"agent_start_v1".into(),network:"devnet".into(),wallet:p.wallet.clone(),
        signature:p.signature.clone(),slot:42,fee_lamports:"5000".into(),commitment:"confirmed".into(),
    }))["ok"],true);
    let body=json!({"agent_record_id":id,"recovery_secret":secret,"name":"Player",
        "model":"m","strategy_hash":"aa".repeat(32)});
    let joined=h_join(&state,gid,&body);
    assert_eq!(joined["ok"],true);
    state.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase=Phase::Action;
    for op_id in 1..=65u64 {
        let result=h_act(&state,gid,&json!({"token":joined["token"],"op_id":op_id,"action":"produce"}));
        assert_eq!(result["op_consumed"],true,"{result}");
    }
    assert_eq!(state.games.lock().unwrap()[&gid].op_state[&id].recent.len(),MAX_RECENT_OPS);
    assert_eq!(h_act(&state,gid,&json!({"token":joined["token"],"op_id":1,"action":"produce"}))["error"],"op_stale");
    let last=h_act(&state,gid,&json!({"token":joined["token"],"op_id":65,"action":"produce"}));
    assert_eq!(last["op_consumed"],true);
    state.games.lock().unwrap().get_mut(&gid).unwrap().agents[0].session_expires_at=Some(now()-1);
    assert_eq!(h_act(&state,gid,&json!({"token":joined["token"],"op_id":66,"action":"produce"}))["error"],"bad_token");
    let mut recovery=body.clone(); recovery["recover"]=json!(true);
    let rotated=h_join(&state,gid,&recovery);
    assert_eq!(rotated["ok"],true);
    assert_eq!(h_act(&state,gid,&json!({"token":joined["token"],"op_id":66,"action":"produce"}))["error"],"bad_token");
    assert_eq!(h_act(&state,gid,&json!({"token":rotated["token"],"op_id":66,"action":"produce"}))["op_consumed"],true);
}

#[test]
fn lifecycle_v2_two_owners_same_strategy_have_separate_game_sessions() {
    let state=isolated();
    let gid=create(&state);
    let strategy="aa".repeat(32);
    let mut joined=Vec::new();
    for (id,secret,name) in [
        ("cd".repeat(32),"ab".repeat(32),"Alpha"),
        ("de".repeat(32),"bc".repeat(32),"Beta"),
    ] {
        let wallet=Pubkey::new_unique().to_string();
        assert_eq!(h_registration_v2(&state,&json!({"agent_record_id":id,"wallet":wallet,"recovery_secret":secret}))["ok"],true);
        let proof=json!({"agent_record_id":id,"wallet":wallet,"recovery_secret":secret,"signature":"2".repeat(88)});
        assert_eq!(h_confirm_v2_with_verifier(&state,&proof,|p,_|Ok(Receipt {
            mode:"agent_start_v1".into(),network:"devnet".into(),wallet:p.wallet.clone(),
            signature:p.signature.clone(),slot:42,fee_lamports:"5000".into(),commitment:"confirmed".into(),
        }))["ok"],true);
        let result=h_join(&state,gid,&json!({"agent_record_id":id,"recovery_secret":secret,
            "name":name,"model":"same","strategy_hash":strategy}));
        assert_eq!(result["ok"],true,"{result}");
        joined.push(result);
    }
    assert_eq!(joined[0]["agent_id"],joined[1]["agent_id"]);
    assert_ne!(joined[0]["owner_id"],joined[1]["owner_id"]);
    assert_ne!(joined[0]["character_id"],joined[1]["character_id"]);
    assert_ne!(joined[0]["token"],joined[1]["token"]);
    state.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase=Phase::Action;
    for (i,player) in joined.iter().enumerate() {
        let result=h_act(&state,gid,&json!({"token":player["token"],"op_id":1,"action":"produce"}));
        assert_eq!(result["op_consumed"],true,"{result}");
        assert_eq!(state.games.lock().unwrap()[&gid].action_log[i]["actor"],i);
    }
}

#[test]
fn v2_stateless_proposals_do_not_allocate_under_public_flood() {
    let state = isolated();
    let secret = "ab".repeat(32);
    let wallet = Pubkey::new_unique().to_string();
    let first_id = "cd".repeat(32);
    let first = json!({"agent_record_id":first_id,"wallet":wallet,"recovery_secret":secret});
    let proposal = h_registration_v2(&state, &first);
    assert_eq!(proposal["ok"], true);
    let snapshot = std::fs::read(&state.snapshot_path).unwrap();
    assert!(state.registrations.lock().unwrap().is_empty());
    for i in 1..=MAX_REGISTERED_AGENTS + 1 {
        let id = format!("{i:064x}");
        let wallet = Pubkey::new_unique().to_string();
        assert_eq!(h_registration_v2(&state,&json!({"agent_record_id":id,
            "wallet":wallet,"recovery_secret":secret}))["ok"],true);
    }
    assert!(state.registrations.lock().unwrap().is_empty());
    assert_eq!(std::fs::read(&state.snapshot_path).unwrap(),snapshot);
    let mut old_snapshot: Value = serde_json::from_slice(&snapshot).unwrap();
    old_snapshot["saved_at"] = json!(now() - 7 * 86_400);
    std::fs::write(&state.snapshot_path, old_snapshot.to_string()).unwrap();
    let state = restore(&state);
    let repeated = h_registration_v2(&state,&first);
    assert_eq!(repeated["memo"],proposal["memo"]);
    let proof = json!({"agent_record_id":first_id,"wallet":wallet,
        "recovery_secret":secret,"signature":"2".repeat(88)});
    let confirmed = h_confirm_v2_with_verifier(&state,&proof,|p,m| {
        assert_eq!(m,proposal["memo"]);
        Ok(Receipt { mode:"agent_start_v1".into(), network:"devnet".into(),
            wallet:p.wallet.clone(),signature:p.signature.clone(),slot:42,
            fee_lamports:"5000".into(),commitment:"confirmed".into() })
    });
    assert_eq!(confirmed["ok"],true,"{confirmed}");
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,|_,_|panic!("retry must not call RPC")),confirmed);
    assert_eq!(state.registrations.lock().unwrap().len(),1);
    assert_eq!(restore(&state).registrations.lock().unwrap().len(),1);
}

#[test]
fn v2_stateless_challenge_has_exact_domain_and_framing() {
    let key = "11".repeat(32);
    let secret = "ab".repeat(32);
    let id = "cd".repeat(32);
    let owner = owner_id_of(&secret);
    let character = character_id_v2(&owner, &id);
    let wallet = "11111111111111111111111111111111";
    assert_eq!(lifecycle_challenge(&key,wallet,&owner,&id,&character),
        "ba257eea386be15c0384fb56c34498678008f31c4e1930535e535059ce953fa4");
    assert_ne!(lifecycle_challenge(&key,wallet,&owner,&id,&character),
        lifecycle_challenge(&key,&Pubkey::new_unique().to_string(),&owner,&id,&character));
}

#[test]
fn v2_stateless_confirm_rejects_wrong_signer_memo_and_secret() {
    let state = isolated();
    let id = "cd".repeat(32);
    let secret = "ab".repeat(32);
    let wallet = Pubkey::new_unique().to_string();
    let proposal = h_registration_v2(&state,&json!({"agent_record_id":id,
        "wallet":wallet,"recovery_secret":secret}));
    assert_eq!(proposal["ok"],true);
    let proof = json!({"agent_record_id":id,"wallet":wallet,
        "recovery_secret":secret,"signature":"2".repeat(88)});
    let status = json!({"value":[{"slot":123,"err":null,"confirmationStatus":"confirmed"}]});
    let tx = json!({"slot":123,"meta":{"err":null,"fee":5000},"transaction":{
        "signatures":[proof["signature"]],
        "message":{"accountKeys":[{"pubkey":wallet,"signer":true}],
            "instructions":[{"programId":registration::MEMO_PROGRAM_ID,"parsed":proposal["memo"]}]}}});
    let mut forged_signer = tx.clone();
    forged_signer["transaction"]["message"]["accountKeys"][0]["signer"] = json!(false);
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,|p,m|
        registration::validate_receipt(p,m,&status,&forged_signer))["error"],"registration_wallet_not_signer");
    let mut forged_memo = tx.clone();
    forged_memo["transaction"]["message"]["instructions"][0]["parsed"] = json!("wrong memo");
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,|p,m|
        registration::validate_receipt(p,m,&status,&forged_memo))["error"],"registration_memo_mismatch");
    let mut wrong_secret = proof.clone(); wrong_secret["recovery_secret"] = json!("ef".repeat(32));
    assert_eq!(h_confirm_v2_with_verifier(&state,&wrong_secret,|p,m|
        registration::validate_receipt(p,m,&status,&tx))["error"],"registration_memo_mismatch");
    assert!(state.registrations.lock().unwrap().is_empty());
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,|p,m|
        registration::validate_receipt(p,m,&status,&tx))["ok"],true);
    assert_eq!(state.registrations.lock().unwrap().len(),1);
}

#[test]
fn v2_concurrent_conflicting_confirms_store_only_one_identity() {
    let state = isolated();
    let id = "cd".repeat(32);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut workers = Vec::new();
    for (secret, signature) in [("ab".repeat(32),"2".repeat(88)),
        ("ef".repeat(32),"3".repeat(88))] {
        let wallet = Pubkey::new_unique().to_string();
        assert_eq!(h_registration_v2(&state,&json!({"agent_record_id":id,
            "wallet":wallet,"recovery_secret":secret}))["ok"],true);
        let proof = json!({"agent_record_id":id,"wallet":wallet,
            "recovery_secret":secret,"signature":signature});
        let state = state.clone();
        let barrier = barrier.clone();
        workers.push(std::thread::spawn(move || h_confirm_v2_with_verifier(&state,&proof,|p,_| {
            barrier.wait();
            Ok(Receipt { mode:"agent_start_v1".into(),network:"devnet".into(),
                wallet:p.wallet.clone(),signature:p.signature.clone(),slot:42,
                fee_lamports:"5000".into(),commitment:"confirmed".into() })
        })));
    }
    let results: Vec<Value> = workers.into_iter().map(|worker| worker.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|result| result["ok"] == true).count(),1,"{results:?}");
    assert_eq!(results.iter().filter(|result| result["error"] == "registration_conflict").count(),1,"{results:?}");
    assert_eq!(restore(&state).registrations.lock().unwrap().len(),1);
}

#[test]
fn v2_legacy_pending_challenge_remains_valid_after_stateless_upgrade() {
    let state = isolated();
    let id = "cd".repeat(32);
    let secret = "ab".repeat(32);
    let wallet = Pubkey::new_unique().to_string();
    let owner_id = owner_id_of(&secret);
    let character_id = character_id_v2(&owner_id,&id);
    let challenge = "ef".repeat(32);
    state.registrations.lock().unwrap().insert(id.clone(), RegisteredAgent {
        wallet:wallet.clone(), owner_id:owner_id.clone(), character_id:character_id.clone(),
        recovery_hash:recovery_hash(&secret).unwrap(), challenge:challenge.clone(),
        receipt:None,created_at:now()-7*86_400,
    });
    save_snapshot(&state).unwrap();
    let state = restore(&state);
    let proposal = h_registration_v2(&state,&json!({"agent_record_id":id,
        "wallet":wallet,"recovery_secret":secret}));
    assert_eq!(proposal["memo"],registration::lifecycle_memo(&owner_id,&id,&character_id,&challenge));
    assert!(state.proposal_key.lock().unwrap().is_none());
    let proof = json!({"agent_record_id":id,"wallet":wallet,
        "recovery_secret":secret,"signature":"2".repeat(88)});
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,|p,m| {
        assert_eq!(m,proposal["memo"]);
        Ok(Receipt { mode:"agent_start_v1".into(),network:"devnet".into(),
            wallet:p.wallet.clone(),signature:p.signature.clone(),slot:42,
            fee_lamports:"5000".into(),commitment:"confirmed".into() })
    })["ok"],true);
}

#[test]
fn v2_stateless_confirm_storage_failure_keeps_signature_retryable() {
    let state = isolated();
    let id = "cd".repeat(32);
    let secret = "ab".repeat(32);
    let wallet = Pubkey::new_unique().to_string();
    let proposal = h_registration_v2(&state,&json!({"agent_record_id":id,
        "wallet":wallet,"recovery_secret":secret}));
    assert_eq!(proposal["ok"],true);
    let proof = json!({"agent_record_id":id,"wallet":wallet,
        "recovery_secret":secret,"signature":"2".repeat(88)});
    std::fs::remove_file(&state.snapshot_path).unwrap();
    std::fs::create_dir(&state.snapshot_path).unwrap();
    let verify = |p:&Proof,m:&str| {
        assert_eq!(m,proposal["memo"]);
        Ok(Receipt { mode:"agent_start_v1".into(),network:"devnet".into(),
            wallet:p.wallet.clone(),signature:p.signature.clone(),slot:42,
            fee_lamports:"5000".into(),commitment:"confirmed".into() })
    };
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,verify)["error"],"storage_failed");
    assert!(state.registrations.lock().unwrap().is_empty());
    std::fs::remove_dir(&state.snapshot_path).unwrap();
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,verify)["ok"],true);
    assert_eq!(restore(&state).registrations.lock().unwrap().len(),1);
}

#[test]
fn finished_v2_action_replays_after_snapshot_reload_without_public_secrets() {
    let state = isolated();
    let gid = create(&state);
    let id = "cd".repeat(32);
    let secret = "ab".repeat(32);
    let wallet = Pubkey::new_unique().to_string();
    assert_eq!(h_registration_v2(&state,&json!({"agent_record_id":id,
        "wallet":wallet,"recovery_secret":secret}))["ok"],true);
    let proof = json!({"agent_record_id":id,"wallet":wallet,
        "recovery_secret":secret,"signature":"2".repeat(88)});
    assert_eq!(h_confirm_v2_with_verifier(&state,&proof,|p,_|Ok(Receipt {
        mode:"agent_start_v1".into(),network:"devnet".into(),wallet:p.wallet.clone(),
        signature:p.signature.clone(),slot:42,fee_lamports:"5000".into(),commitment:"confirmed".into(),
    }))["ok"],true);
    let joined = h_join(&state,gid,&json!({"agent_record_id":id,"recovery_secret":secret,
        "name":"Player","model":"m","strategy_hash":"aa".repeat(32)}));
    assert_eq!(joined["ok"],true,"{joined}");
    join(&state,gid,1);
    state.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase = Phase::Action;
    let act = json!({"token":joined["token"],"op_id":1,"action":"produce"});
    let first = h_act(&state,gid,&act);
    assert_eq!(first["op_consumed"],true,"{first}");
    state.games.lock().unwrap().get_mut(&gid).unwrap().sim.game.phase = Phase::Finished;
    settle_and_record_locked_for_test(&state,gid);
    assert_eq!(h_act(&state,gid,&act),first);
    let restored = restore(&state);
    assert_eq!(h_act(&restored,gid,&act),first);
    let mut conflict = act.clone(); conflict["action"] = json!("donkey");
    assert_eq!(h_act(&restored,gid,&conflict)["error"],"op_conflict");
    let mut next = act.clone(); next["op_id"] = json!(2);
    assert_eq!(h_act(&restored,gid,&next)["error"],"game_finished");
    assert_eq!(h_act(&restored,gid,&json!({"token":"ef".repeat(32),"op_id":1,
        "action":"produce"}))["error"],"bad_token");
    assert!(!h_export(&restored).contains(joined["token"].as_str().unwrap()));
    assert!(!h_export(&restored).contains(&sha256_hex(joined["token"].as_str().unwrap())));
}

#[test]
fn failed_snapshot_does_not_ack_terminal_advance_or_mutate_crank() {
    let state = isolated();
    let gid = create(&state);
    join(&state,gid,0); join(&state,gid,1);
    {
        let mut games = state.games.lock().unwrap();
        let entry = games.get_mut(&gid).unwrap();
        entry.sim.game.phase = Phase::Law;
        entry.sim.game.round = ROUNDS;
        entry.sim.game.phase_ends_at = 0;
    }
    save_snapshot(&state).unwrap();
    let before = borsh::to_vec(&state.games.lock().unwrap()[&gid].sim.game).unwrap();
    std::fs::remove_file(&state.snapshot_path).unwrap();
    std::fs::create_dir(&state.snapshot_path).unwrap();
    let advanced = h_advance(&state,gid);
    assert_eq!(advanced["error"],"storage_failed","{advanced}");
    assert_eq!(borsh::to_vec(&state.games.lock().unwrap()[&gid].sim.game).unwrap(),before);
    assert!(state.completed.lock().unwrap().is_empty());
    crank_once(&state);
    assert_eq!(borsh::to_vec(&state.games.lock().unwrap()[&gid].sim.game).unwrap(),before);
    assert!(state.completed.lock().unwrap().is_empty());
}
