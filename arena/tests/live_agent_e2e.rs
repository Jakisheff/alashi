//! Offline HTTP integration for the v2 harness/live client. No chain, model, or paid API.
use arena::api::{new_state_with_files, save_snapshot, serve_on, AppState, RegisteredAgent};
use arena::registration::Receipt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use solana_ed25519::ed_sigs::{SigningKey, VerificationKey};
use solana_signature::Signature;
use std::{
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

const RECORD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OWNER_ORIGIN: &str = "https://alashi.network";
fn test_owner() -> (SigningKey, String) {
    let key = SigningKey::from([7u8; 32]);
    let vk: [u8; 32] = VerificationKey::from(&key).into();
    (
        key,
        alashi_rules::anchor_lang::prelude::Pubkey::new_from_array(vk).to_string(),
    )
}

fn isolated_state() -> (std::sync::Arc<AppState>, PathBuf) {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let root = std::env::temp_dir().join(format!(
        "alashi_live_agent_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let state = new_state_with_files(root.join("state.json"), root.join("sequence"));
    let recovery_hash = format!("{:x}", Sha256::digest([0x11u8; 32]));
    let (_, wallet) = test_owner();
    state.registrations.lock().unwrap().insert(
        RECORD.into(),
        RegisteredAgent {
            wallet: wallet.clone(),
            owner_id: "b".repeat(64),
            character_id: "c".repeat(64),
            recovery_hash,
            challenge: "d".repeat(64),
            receipt: Some(Receipt {
                mode: "agent_lifecycle_v2".into(),
                network: "devnet".into(),
                wallet,
                signature: "offline-fixture".into(),
                slot: 0,
                fee_lamports: "0".into(),
                commitment: "confirmed".into(),
            }),
            created_at: 1,
        },
    );
    save_snapshot(&state).unwrap();
    // Simulate a pre-live v1 snapshot: absent optional journal roots load as empty.
    let snapshot_path = root.join("state.json");
    let mut snapshot: Value =
        serde_json::from_slice(&std::fs::read(&snapshot_path).unwrap()).unwrap();
    snapshot.as_object_mut().unwrap().remove("live");
    snapshot.as_object_mut().unwrap().remove("owner_auth");
    snapshot.as_object_mut().unwrap().remove("wishes");
    std::fs::write(&snapshot_path, serde_json::to_vec(&snapshot).unwrap()).unwrap();
    (state, root)
}

fn request(port: u16, method: &str, path: &str, body: Option<&Value>) -> Value {
    request_with_headers(port, method, path, body, None, None)
}

fn request_with_headers(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&Value>,
    authorization: Option<&str>,
    origin: Option<&str>,
) -> Value {
    let raw_body = body.map(Value::to_string).unwrap_or_default();
    let auth = authorization
        .map(|v| format!("Authorization: Bearer {v}\r\n"))
        .unwrap_or_default();
    let origin = origin
        .map(|v| format!("Origin: {v}\r\n"))
        .unwrap_or_default();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect local server");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n{auth}{origin}Content-Length: {}\r\nConnection: close\r\n\r\n{}",
        raw_body.len(), raw_body
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let text = String::from_utf8_lossy(&bytes);
    let body_at = text.find("\r\n\r\n").unwrap() + 4;
    serde_json::from_str(&text[body_at..]).unwrap()
}

fn fixture() -> (u16, u64, String, String, String, String, PathBuf) {
    let (state, root) = isolated_state();
    let addr = serve_on(state, "127.0.0.1:0", 60).unwrap();
    let created = request(
        addr.port(),
        "POST",
        "/game/new",
        Some(&json!({
            "entry_fee": 1, "phase_duration": 120, "lobby_duration": 120, "grace_s": 0
        })),
    );
    assert_eq!(created["ok"], true, "{created}");
    let game_id = created["game_id"].as_u64().unwrap();
    let strategy_hash = arena::api::agent_id_of("fixture-model", "base strategy only");
    let joined = request(
        addr.port(),
        "POST",
        &format!("/game/{game_id}/join"),
        Some(&json!({
            "agent_record_id": RECORD, "recovery_secret": "11".repeat(32), "name": "Fixture",
            "model": "fixture-model", "strategy_hash": strategy_hash
        })),
    );
    assert_eq!(joined["ok"], true, "{joined}");
    assert_eq!(joined["agent_record_id"], RECORD);
    let game_token = joined["token"].as_str().unwrap().to_string();
    let phase_id = joined["state"]["phase_instance_id"]
        .as_str()
        .unwrap()
        .to_string();
    (
        addr.port(),
        game_id,
        game_token,
        phase_id,
        "11".repeat(32),
        strategy_hash,
        root,
    )
}

#[test]
fn v2_live_and_game_messages_are_scoped_idempotent_and_cursor_readable() {
    let (port, game_id, game_token, phase_id, secret, _, root) = fixture();

    let session = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/live/session"),
        Some(&json!({
            "recovery_secret": secret, "ambient_replies_enabled": true
        })),
    );
    assert_eq!(session["ok"], true, "{session}");
    let live_token = session["live_token"].as_str().unwrap();
    assert_eq!(session["stream_id"], RECORD);
    assert_eq!(session["scope"], json!(["presence", "ambient_write"]));

    let wrong_session = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/live/session"),
        Some(&json!({
            "recovery_secret": "22".repeat(32)
        })),
    );
    assert_eq!(wrong_session["error"], "bad_credentials");

    let message = json!({
        "token": game_token, "client_message_id": "msg_test_1",
        "phase_instance_id": phase_id, "text": "public game message"
    });
    let first = request(
        port,
        "POST",
        &format!("/game/{game_id}/live/messages"),
        Some(&message),
    );
    assert_eq!(first["ok"], true, "{first}");
    let replay = request(
        port,
        "POST",
        &format!("/game/{game_id}/live/messages"),
        Some(&message),
    );
    assert_eq!(replay["seq"], first["seq"]);
    assert_eq!(replay["message_id"], first["message_id"]);

    let mut conflict = message.clone();
    conflict["text"] = json!("different request");
    assert_eq!(
        request(
            port,
            "POST",
            &format!("/game/{game_id}/live/messages"),
            Some(&conflict)
        )["error"],
        "message_conflict"
    );

    let mut wrong_author = message.clone();
    wrong_author["token"] = json!("00".repeat(32));
    assert_eq!(
        request(
            port,
            "POST",
            &format!("/game/{game_id}/live/messages"),
            Some(&wrong_author)
        )["error"],
        "bad_token"
    );

    let events = request(
        port,
        "GET",
        &format!("/game/{game_id}/live/events?after=0&limit=20"),
        None,
    );
    assert_eq!(events["ok"], true, "{events}");
    let rows = events["events"].as_array().unwrap();
    let published = rows
        .iter()
        .find(|e| e["message_id"] == first["message_id"])
        .unwrap();
    assert_eq!(published["author_agent_record_id"], RECORD);
    assert_eq!(published["text"], "public game message");
    let resumed = request(
        port,
        "GET",
        &format!(
            "/game/{game_id}/live/events?after={}&limit=20",
            first["seq"].as_u64().unwrap()
        ),
        None,
    );
    assert!(resumed["events"].as_array().unwrap().is_empty());

    let personal = request(
        port,
        "GET",
        &format!("/agents/{RECORD}/live/events?after=0&limit=20"),
        None,
    );
    assert_eq!(personal["ok"], true, "{personal}");
    let serialized = personal.to_string();
    assert!(
        !serialized.contains(live_token),
        "live credential must not appear in readable events"
    );
    assert!(
        !serialized.contains("private owner wish"),
        "private input must never enter public events"
    );
    let presence = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/live/presence"),
        Some(&json!({
            "live_token": live_token, "client_instance_id": "fixture_client"
        })),
    );
    assert_eq!(presence["ok"], true, "{presence}");
    let invalid_claim = request(
        port,
        "POST",
        &format!("/game/{game_id}/owner/wishes/claim"),
        Some(&json!({
            "token": "00".repeat(32), "after": 0, "limit": 1
        })),
    );
    assert_eq!(invalid_claim["error"], "game_token_invalid");
    let empty_claim = request(
        port,
        "POST",
        &format!("/game/{game_id}/owner/wishes/claim"),
        Some(&json!({
            "token": game_token, "after": 0, "limit": 1
        })),
    );
    assert_eq!(empty_claim["ok"], true, "{empty_claim}");
    assert!(empty_claim["wishes"].as_array().unwrap().is_empty());
    let unauthorized_private = request(
        port,
        "GET",
        &format!("/agents/{RECORD}/owner/wishes?after=0&limit=20"),
        None,
    );
    assert_eq!(unauthorized_private["error"], "owner_session_invalid");

    // A fresh process reloads the journal and resumes after the same global cursor.
    let restarted = new_state_with_files(root.join("state.json"), root.join("sequence"));
    let addr2 = serve_on(restarted, "127.0.0.1:0", 60).unwrap();
    let after_restart = request(
        addr2.port(),
        "GET",
        &format!("/game/{game_id}/live/events?after=0&limit=20"),
        None,
    );
    let restored = after_restart["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["message_id"] == first["message_id"])
        .unwrap();
    assert_eq!(restored["author_agent_record_id"], RECORD);
    let resumed_after_restart = request(
        addr2.port(),
        "GET",
        &format!(
            "/game/{game_id}/live/events?after={}&limit=20",
            first["seq"].as_u64().unwrap()
        ),
        None,
    );
    assert!(resumed_after_restart["events"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn owner_ed25519_wishes_are_private_quota_atomic_idempotent_and_durable() {
    let (port, game_id, game_token, _, secret, _, root) = fixture();
    let (key, _wallet) = test_owner();

    let challenge = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/challenge"),
        Some(&json!({})),
        None,
        Some(OWNER_ORIGIN),
    );
    assert_eq!(challenge["ok"], true, "{challenge}");
    let exact_message = challenge["message"].as_str().unwrap();
    let signature = Signature::from(key.sign(exact_message.as_bytes()).to_bytes()).to_string();
    let owner_login = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/session"),
        Some(&json!({"challenge_id":challenge["challenge_id"],"signature":signature})),
        None,
        Some(OWNER_ORIGIN),
    );
    assert_eq!(owner_login["ok"], true, "{owner_login}");
    let owner_session = owner_login["owner_session"].as_str().unwrap().to_string();

    // Wrong origin and wrong signature do not mint owner sessions.
    let bad_origin = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/challenge"),
        Some(&json!({})),
        None,
        Some("https://alashi.network.attacker.example"),
    );
    assert_eq!(bad_origin["error"], "owner_origin_forbidden");
    let bad_challenge = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/challenge"),
        Some(&json!({})),
        None,
        Some(OWNER_ORIGIN),
    );
    let bad_proof = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/session"),
        Some(
            &json!({"challenge_id":bad_challenge["challenge_id"],"signature":Signature::from(key.sign(b"wrong message").to_bytes()).to_string()}),
        ),
        None,
        Some(OWNER_ORIGIN),
    );
    assert_eq!(bad_proof["error"], "owner_signature_invalid");

    let mut workers = Vec::new();
    for i in 0..4 {
        let owner = owner_session.clone();
        workers.push(std::thread::spawn(move || {
            request_with_headers(
                port,
                "POST",
                &format!("/agents/{RECORD}/owner/wishes"),
                Some(
                    &json!({"game_id":game_id,"client_wish_id":format!("race-{i}"),
                    "text":format!("PRIVATE_WISH_MARKER_{i}")}),
                ),
                Some(&owner),
                Some(OWNER_ORIGIN),
            )
        }));
    }
    let results: Vec<Value> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(
        results.iter().filter(|r| r["ok"] == true).count(),
        3,
        "{results:?}"
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| r["error"] == "wish_quota_exhausted")
            .count(),
        1,
        "{results:?}"
    );

    let accepted = results.iter().find(|r| r["ok"] == true).unwrap();
    let accepted_id = accepted["wish_id"].as_str().unwrap();
    let (accepted_row, owner_admission_cursor) = {
        let page = request_with_headers(
            port,
            "GET",
            &format!("/agents/{RECORD}/owner/wishes?after=0&limit=20"),
            None,
            Some(&owner_session),
            None,
        );
        assert_eq!(page["ok"], true, "{page}");
        let wishes = page["wishes"].as_array().unwrap();
        assert_eq!(wishes.len(), 3);
        assert!(wishes.iter().all(|w| w["text"]
            .as_str()
            .unwrap()
            .starts_with("PRIVATE_WISH_MARKER_")));
        (
            wishes
                .iter()
                .find(|w| w["wish_id"] == accepted_id)
                .unwrap()
                .clone(),
            page["next_cursor"].as_u64().unwrap(),
        )
    };
    let retry_text = accepted_row["text"].as_str().unwrap();
    let suffix = retry_text.rsplit('_').next().unwrap();
    let client_wish_id = format!("race-{suffix}");
    let retry = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/wishes"),
        Some(&json!({"game_id":game_id,"client_wish_id":client_wish_id,
            "text":retry_text})),
        Some(&owner_session),
        Some(OWNER_ORIGIN),
    );
    assert_eq!(retry["wish_id"], accepted_id);
    assert_eq!(retry["seq"], accepted_row["seq"]);

    // Admission text is absent from every public projection before and after consumption.
    let public_before = request(
        port,
        "GET",
        &format!("/agents/{RECORD}/live/events?after=0&limit=100"),
        None,
    );
    assert!(!public_before.to_string().contains("PRIVATE_WISH_MARKER_"));

    let restarted = new_state_with_files(root.join("state.json"), root.join("sequence"));
    let addr = serve_on(restarted, "127.0.0.1:0", 60).unwrap();
    let port = addr.port();

    // An exact retry after restart still returns the original receipt; the fourth distinct wish is blocked.
    let retry_after_restart = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/wishes"),
        Some(&json!({"game_id":game_id,"client_wish_id":client_wish_id,
            "text":retry_text})),
        Some(&owner_session),
        Some(OWNER_ORIGIN),
    );
    assert_eq!(retry_after_restart["wish_id"], accepted_id);
    let over_quota = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/wishes"),
        Some(&json!({"game_id":game_id,"client_wish_id":"fourth","text":"must not be admitted"})),
        Some(&owner_session),
        Some(OWNER_ORIGIN),
    );
    assert_eq!(over_quota["error"], "wish_quota_exhausted");

    let claimed = request(
        port,
        "POST",
        &format!("/game/{game_id}/owner/wishes/claim"),
        Some(&json!({
            "token":game_token,"after":0,"limit":1
        })),
    );
    assert_eq!(claimed["ok"], true, "{claimed}");
    let leased = &claimed["wishes"][0];
    assert_eq!(leased["status"], "received");
    assert!(leased["text"]
        .as_str()
        .unwrap()
        .starts_with("PRIVATE_WISH_MARKER_"));
    let deferred = request(
        port,
        "POST",
        &format!(
            "/game/{game_id}/owner/wishes/{}/status",
            leased["wish_id"].as_str().unwrap()
        ),
        Some(&json!({
            "token":game_token,"lease_id":leased["lease_id"],"status":"deferred"
        })),
    );
    assert_eq!(deferred["ok"], true, "{deferred}");
    let reclaimed = request(
        port,
        "POST",
        &format!("/game/{game_id}/owner/wishes/claim"),
        Some(&json!({
            "token":game_token,"after":0,"limit":1
        })),
    );
    assert_eq!(reclaimed["wishes"][0]["wish_id"], leased["wish_id"]);
    let leased_again = &reclaimed["wishes"][0];
    let consumed = request(
        port,
        "POST",
        &format!(
            "/game/{game_id}/owner/wishes/{}/status",
            leased_again["wish_id"].as_str().unwrap()
        ),
        Some(&json!({
            "token":game_token,"lease_id":leased_again["lease_id"],"status":"consumed"
        })),
    );
    assert_eq!(consumed["ok"], true, "{consumed}");

    let private_reply = request(
        port,
        "POST",
        &format!(
            "/game/{game_id}/owner/wishes/{}/status",
            leased_again["wish_id"].as_str().unwrap()
        ),
        Some(&json!({
            "token":game_token,"lease_id":leased_again["lease_id"],"status":"replied","reply":"PRIVATE_REPLY_MARKER"
        })),
    );
    assert_eq!(private_reply["ok"], true, "{private_reply}");
    let owner_after = request_with_headers(
        port,
        "GET",
        &format!("/agents/{RECORD}/owner/wishes?after=0&limit=20"),
        None,
        Some(&owner_session),
        None,
    );
    assert!(owner_after.to_string().contains("PRIVATE_REPLY_MARKER"));
    let owner_status_page = request_with_headers(
        port,
        "GET",
        &format!("/agents/{RECORD}/owner/wishes?after={owner_admission_cursor}&limit=20"),
        None,
        Some(&owner_session),
        None,
    );
    assert!(owner_status_page
        .to_string()
        .contains("PRIVATE_REPLY_MARKER"));
    let public_after = request(
        port,
        "GET",
        &format!("/agents/{RECORD}/live/events?after=0&limit=100"),
        None,
    );
    assert!(!public_after.to_string().contains("PRIVATE_WISH_MARKER_"));
    assert!(!public_after.to_string().contains("PRIVATE_REPLY_MARKER"));
    let no_refund = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/wishes"),
        Some(
            &json!({"game_id":game_id,"client_wish_id":"after-consumption","text":"no free slot"}),
        ),
        Some(&owner_session),
        Some(OWNER_ORIGIN),
    );
    assert_eq!(no_refund["error"], "wish_quota_exhausted");
    assert_eq!(
        request(
            port,
            "POST",
            &format!("/game/{game_id}/owner/wishes/claim"),
            Some(&json!({
                "token":game_token,"after":0,"limit":100
            }))
        )["wishes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(secret.len(), 64);
}

#[test]
fn ambient_messages_project_across_personal_streams_and_reply_as_recipient() {
    const SECOND: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let (state, _root) = isolated_state();
    let key2 = SigningKey::from([8u8; 32]);
    let vk2: [u8; 32] = VerificationKey::from(&key2).into();
    let wallet2 = alashi_rules::anchor_lang::prelude::Pubkey::new_from_array(vk2).to_string();
    state.registrations.lock().unwrap().insert(
        SECOND.into(),
        RegisteredAgent {
            wallet: wallet2.clone(),
            owner_id: "e".repeat(64),
            character_id: "f".repeat(64),
            recovery_hash: format!("{:x}", Sha256::digest([0x22u8; 32])),
            challenge: "1".repeat(64),
            receipt: Some(Receipt {
                mode: "agent_lifecycle_v2".into(),
                network: "devnet".into(),
                wallet: wallet2,
                signature: "offline-fixture".into(),
                slot: 0,
                fee_lamports: "0".into(),
                commitment: "confirmed".into(),
            }),
            created_at: 1,
        },
    );
    save_snapshot(&state).unwrap();
    let addr = serve_on(state, "127.0.0.1:0", 60).unwrap();
    let game = request(
        addr.port(),
        "POST",
        "/game/new",
        Some(&json!({"entry_fee":1,"phase_duration":120,"lobby_duration":120,"grace_s":0})),
    );
    let game_id = game["game_id"].as_u64().unwrap();
    for (record, secret, model, strategy) in [
        (RECORD, "11".repeat(32), "fixture-a", "strategy-a"),
        (SECOND, "22".repeat(32), "fixture-b", "strategy-b"),
    ] {
        let joined = request(
            addr.port(),
            "POST",
            &format!("/game/{game_id}/join"),
            Some(&json!({
                "agent_record_id":record,"recovery_secret":secret,"name":if record == RECORD { "Ada" } else { "Bea" },
                "model":model,"strategy_hash":arena::api::agent_id_of(model,strategy)
            })),
        );
        assert_eq!(joined["ok"], true, "{joined}");
    }
    let live_a = request(
        addr.port(),
        "POST",
        &format!("/agents/{RECORD}/live/session"),
        Some(&json!({
            "recovery_secret":"11".repeat(32),"ambient_replies_enabled":true
        })),
    );
    let live_b = request(
        addr.port(),
        "POST",
        &format!("/agents/{SECOND}/live/session"),
        Some(&json!({
            "recovery_secret":"22".repeat(32),"ambient_replies_enabled":true
        })),
    );
    let token_a = live_a["live_token"].as_str().unwrap();
    let token_b = live_b["live_token"].as_str().unwrap();

    let first = request(
        addr.port(),
        "POST",
        &format!("/agents/{RECORD}/live/messages"),
        Some(&json!({
            "live_token":token_a,"client_message_id":"ambient-a-1","text":"hello, B",
            "to_agent_record_id":SECOND
        })),
    );
    assert_eq!(first["ok"], true, "{first}");
    let b_feed = request(
        addr.port(),
        "GET",
        &format!("/agents/{SECOND}/live/events?after=0&limit=20"),
        None,
    );
    let initial = b_feed["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["message_id"] == first["message_id"])
        .unwrap();
    assert_eq!(initial["event_id"], first["event_id"]);
    assert_eq!(initial["seq"], first["seq"]);
    assert_eq!(initial["room_id"], format!("agent:{RECORD}"));
    assert_eq!(initial["context_kind"], "ambient");

    let forged = request(
        addr.port(),
        "POST",
        &format!("/agents/{RECORD}/live/messages"),
        Some(&json!({
            "live_token":token_b,"client_message_id":"forged-a","text":"not A",
            "to_agent_record_id":SECOND
        })),
    );
    assert_eq!(forged["ok"], false);

    let reply = request(
        addr.port(),
        "POST",
        &format!("/agents/{RECORD}/live/messages"),
        Some(&json!({
            "live_token":token_b,"client_message_id":"ambient-b-1","text":"hello, A",
            "to_agent_record_id":RECORD,"reply_to_message_id":first["message_id"]
        })),
    );
    assert_eq!(reply["ok"], true, "{reply}");
    let a_feed = request(
        addr.port(),
        "GET",
        &format!("/agents/{RECORD}/live/events?after=0&limit=20"),
        None,
    );
    let reply_event = a_feed["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["message_id"] == reply["message_id"])
        .unwrap();
    assert_eq!(reply_event["author_agent_record_id"], SECOND);
    assert_eq!(reply_event["reply_to_message_id"], first["message_id"]);
    let b_again = request(
        addr.port(),
        "GET",
        &format!(
            "/agents/{SECOND}/live/events?after={}&limit=20",
            first["seq"].as_u64().unwrap()
        ),
        None,
    );
    assert_eq!(
        b_again["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["message_id"] == reply["message_id"])
            .count(),
        1
    );
}
