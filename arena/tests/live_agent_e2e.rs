//! Offline HTTP integration for the v2 harness/live client. No chain, model, or paid API.
use arena::api::{new_state_with_files, save_snapshot, serve_on, AppState, RegisteredAgent};
use arena::registration::Receipt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use solana_ed25519::ed_sigs::{SigningKey, VerificationKey};
use solana_signature::Signature;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
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

fn read_provider_request(stream: &mut TcpStream) -> Value {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut content_length = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse::<usize>().unwrap();
        }
    }
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body).unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn respond_provider(stream: &mut TcpStream, body: &Value) {
    let body = body.to_string();
    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
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

const RECORD_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn register_v2_fixture(
    state: &AppState,
    record: &str,
    secret_bytes: [u8; 32],
    seed: [u8; 32],
) -> String {
    let key = SigningKey::from(seed);
    let vk: [u8; 32] = VerificationKey::from(&key).into();
    let wallet = alashi_rules::anchor_lang::prelude::Pubkey::new_from_array(vk).to_string();
    state.registrations.lock().unwrap().insert(
        record.to_string(),
        RegisteredAgent {
            wallet: wallet.clone(),
            owner_id: format!("{:064x}", seed[0]),
            character_id: format!("{:064x}", seed[0] + 1),
            recovery_hash: format!("{:x}", Sha256::digest(secret_bytes)),
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
    format!("{:02x}", secret_bytes[0]).repeat(32)
}

fn post_game_message(port: u16, game_id: u64, token: &str, phase_id: &str, body: Value) -> Value {
    let mut body = body;
    body["token"] = json!(token);
    body["phase_instance_id"] = json!(phase_id);
    request(
        port,
        "POST",
        &format!("/game/{game_id}/live/messages"),
        Some(&body),
    )
}

struct KillChild(std::process::Child);

impl Drop for KillChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn fixture() -> (u16, u64, String, String, String, String, PathBuf) {
    let (state, root) = isolated_state();
    let addr = serve_on(state.clone(), "127.0.0.1:0", 60).unwrap();
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
    let mut cue_conflict = message.clone();
    cue_conflict["gesture_cue"] = json!("facepalm");
    assert_eq!(
        request(
            port,
            "POST",
            &format!("/game/{game_id}/live/messages"),
            Some(&cue_conflict)
        )["error"],
        "message_conflict"
    );

    let mut invalid_cue = message.clone();
    invalid_cue["client_message_id"] = json!("msg_invalid_cue");
    invalid_cue["gesture_cue"] = json!("wink");
    assert_eq!(
        request(
            port,
            "POST",
            &format!("/game/{game_id}/live/messages"),
            Some(&invalid_cue)
        )["error"],
        "bad_gesture_cue"
    );
    std::thread::sleep(Duration::from_millis(5100));
    let cued = request(
        port,
        "POST",
        &format!("/game/{game_id}/live/messages"),
        Some(&json!({
            "token": game_token, "client_message_id": "msg_with_cue",
            "phase_instance_id": phase_id, "text": "message with a cue",
            "gesture_cue": "thumbsUp"
        })),
    );
    assert_eq!(cued["ok"], true, "{cued}");

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
    assert!(published.get("gesture_cue").is_none());
    let cued_event = rows
        .iter()
        .find(|e| e["message_id"] == cued["message_id"])
        .unwrap();
    assert_eq!(cued_event["gesture_cue"], "thumbsUp");
    assert!(!rows.iter().any(|e| e["message_id"] == "msg_invalid_cue"));
    let resumed = request(
        port,
        "GET",
        &format!(
            "/game/{game_id}/live/events?after={}&limit=20",
            cued["seq"].as_u64().unwrap()
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
            cued["seq"].as_u64().unwrap()
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
            "gesture_cue":"thumbsUp","to_agent_record_id":SECOND
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
    assert_eq!(initial["gesture_cue"], "thumbsUp");

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
            "gesture_cue":"facepalm","to_agent_record_id":RECORD,"reply_to_message_id":first["message_id"]
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
    assert_eq!(reply_event["gesture_cue"], "facepalm");
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

#[test]
fn actual_agent_process_keeps_wish_private_and_voluntarily_publishes_cued_reply() {
    const WISH: &str = "PROCESS_PRIVATE_WISH_SENTINEL";
    const PRIVATE_REPLY: &str = "PROCESS_OWNER_REPLY_SENTINEL";
    const PRIVATE_CUE: &str = "facepalm";
    const PUBLIC_REPLY: &str = "A voluntary public reply";
    const PUBLIC_CUE: &str = "realization";

    let root = std::env::temp_dir().join(format!("alashi_runner_process_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let state = new_state_with_files(root.join("state.json"), root.join("sequence"));
    let recovery_a = register_v2_fixture(&state, RECORD, [0x11; 32], [7; 32]);
    let recovery_b = register_v2_fixture(&state, RECORD_B, [0x22; 32], [8; 32]);
    save_snapshot(&state).unwrap();
    let addr = serve_on(state.clone(), "127.0.0.1:0", 60).unwrap();
    let port = addr.port();
    let created = request(
        port,
        "POST",
        "/game/new",
        Some(&json!({
            "entry_fee":1,"phase_duration":120,"lobby_duration":5,"grace_s":0,
            "label":"SYNTHETIC_RUNNER_TEST"
        })),
    );
    assert_eq!(created["ok"], true, "{created}");
    let game_id = created["game_id"].as_u64().unwrap();

    let recovery_path = root.join("agent-a-recovery");
    std::fs::write(&recovery_path, &recovery_a).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&recovery_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    let mock_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mock_addr = mock_listener.local_addr().unwrap();
    let (model_tx, model_rx) = mpsc::channel();
    let (first_response_tx, first_response_rx) = mpsc::channel();
    let mock = thread::spawn(move || {
        let (mut stream, _) = mock_listener.accept().unwrap();
        let first_request = read_provider_request(&mut stream);
        model_tx.send(first_request).unwrap();
        let first_content = first_response_rx
            .recv_timeout(Duration::from_secs(20))
            .expect("test releases first mock response");
        respond_provider(
            &mut stream,
            &json!({
                "choices":[{"message":{"content":first_content}}],
                "usage":{"prompt_tokens":2,"completion_tokens":3}
            }),
        );

        let (mut stream, _) = mock_listener.accept().unwrap();
        let second_request = read_provider_request(&mut stream);
        model_tx.send(second_request).unwrap();
        respond_provider(
            &mut stream,
            &json!({
                "choices":[{"message":{"content":json!({
                    "public_message":PUBLIC_REPLY,
                    "gesture_cue":PUBLIC_CUE
                }).to_string()}}],
                "usage":{"prompt_tokens":2,"completion_tokens":2}
            }),
        );
    });

    let child = Command::new(env!("CARGO_BIN_EXE_agent"))
        .args([
            "--url",
            &format!("http://127.0.0.1:{port}"),
            "--game",
            &game_id.to_string(),
            "--name",
            "RunnerA",
            "--model",
            "mock-runner",
            "--agent-record-id",
            RECORD,
            "--recovery-file",
            recovery_path.to_str().unwrap(),
            "--public-speech",
        ])
        .env("ALASHI_ALLOW_INSECURE_HTTP", "1")
        .env("ALASHI_LLM_KEY", "mock-test-key-local-only")
        .env("ALASHI_TEST_MOCK_LLM_BASE", format!("http://{mock_addr}"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let _child = KillChild(child);

    let joined_before_deadline = Instant::now() + Duration::from_secs(8);
    let mut joined = false;
    while Instant::now() < joined_before_deadline {
        let state = request(port, "GET", &format!("/game/{game_id}/state"), None);
        if state["state"]["factions"]
            .as_array()
            .is_some_and(|v| !v.is_empty())
        {
            joined = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    assert!(joined, "actual runner did not join its isolated game");

    let (owner_key, _) = test_owner();
    let challenge = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/challenge"),
        Some(&json!({})),
        None,
        Some(OWNER_ORIGIN),
    );
    assert_eq!(challenge["ok"], true, "{challenge}");
    let signature = Signature::from(
        owner_key
            .sign(challenge["message"].as_str().unwrap().as_bytes())
            .to_bytes(),
    )
    .to_string();
    let owner_login = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/session"),
        Some(&json!({
            "challenge_id":challenge["challenge_id"], "signature":signature
        })),
        None,
        Some(OWNER_ORIGIN),
    );
    assert_eq!(owner_login["ok"], true, "{owner_login}");
    let owner_session = owner_login["owner_session"].as_str().unwrap().to_string();
    let admission = request_with_headers(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/wishes"),
        Some(&json!({
            "game_id":game_id, "client_wish_id":"runner-wish-1", "text":WISH
        })),
        Some(&owner_session),
        Some(OWNER_ORIGIN),
    );
    assert_eq!(admission["ok"], true, "{admission}");

    // Give the first faction one legal market sale in this isolated fixture.
    {
        let mut games = state.games.lock().unwrap();
        games.get_mut(&game_id).unwrap().sim.factions[0].goods = 3;
    }
    save_snapshot(&state).unwrap();

    // Starting B releases the two-agent lobby; the real runner is already polling.
    let strategy_hash = arena::api::agent_id_of("fixture-b", "base strategy only");
    let joined_b = request(
        port,
        "POST",
        &format!("/game/{game_id}/join"),
        Some(&json!({
            "agent_record_id":RECORD_B, "recovery_secret":recovery_b, "name":"RunnerB",
            "model":"fixture-b", "strategy_hash":strategy_hash
        })),
    );
    assert_eq!(joined_b["ok"], true, "{joined_b}");
    let first_request = model_rx
        .recv_timeout(Duration::from_secs(15))
        .expect("actual runner called loopback mock provider");
    assert!(first_request["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains(WISH));
    let current = request(port, "GET", &format!("/game/{game_id}/state"), None);
    let phase_id = current["state"]["phase_instance_id"].as_str().unwrap();
    let b_message = post_game_message(
        port,
        game_id,
        joined_b["token"].as_str().unwrap(),
        &phase_id,
        json!({
            "client_message_id":"runner-addressed-1",
            "text":"A public hello",
            "to_agent_record_id":RECORD
        }),
    );
    assert_eq!(b_message["ok"], true, "{b_message}");
    first_response_tx
        .send(
            json!({
                "action":"sell",
                "params":{"units":1,"privatewish":WISH},
                "public_message":WISH,
                "gesture_cue":PRIVATE_CUE,
                "owner_reply":PRIVATE_REPLY
            })
            .to_string(),
        )
        .unwrap();

    let until = Instant::now() + Duration::from_secs(20);
    let mut owner_replied = false;
    let mut public_reply = None;
    while Instant::now() < until {
        let owner_state = request_with_headers(
            port,
            "GET",
            &format!("/agents/{RECORD}/owner/wishes?after=0&limit=10"),
            None,
            Some(&owner_session),
            None,
        );
        owner_replied = owner_state["wishes"].as_array().is_some_and(|rows| {
            rows.iter().any(|w| {
                w["text"] == WISH && w["status"] == "replied" && w["reply"] == PRIVATE_REPLY
            })
        });
        let feed = request(
            port,
            "GET",
            &format!("/game/{game_id}/live/events?after=0&limit=100"),
            None,
        );
        if let Some(rows) = feed["events"].as_array() {
            public_reply = rows
                .iter()
                .find(|event| event["kind"] == "agent_message" && event["text"] == PUBLIC_REPLY)
                .cloned();
        }
        if owner_replied && public_reply.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    mock.join().unwrap();
    assert!(
        owner_replied,
        "owner journal did not reach a real replied status"
    );
    let public_reply = public_reply.expect("runner did not publish voluntary reply");
    assert_eq!(public_reply["author_agent_record_id"], RECORD);
    assert_eq!(public_reply["gesture_cue"], PUBLIC_CUE);
    assert_eq!(public_reply["to_agent_record_id"], RECORD_B);
    assert_eq!(public_reply["reply_to_message_id"], b_message["message_id"]);
    assert!(!public_reply.to_string().contains(WISH));
    let feed = request(
        port,
        "GET",
        &format!("/game/{game_id}/live/events?after=0&limit=100"),
        None,
    );
    assert!(
        !feed.to_string().contains(WISH),
        "private wish leaked to public feed"
    );
    assert!(
        !feed.to_string().contains(PRIVATE_REPLY),
        "owner reply leaked to public feed"
    );
    assert!(
        !feed.to_string().contains(PRIVATE_CUE),
        "private gesture cue leaked to public feed"
    );
    let second_request = model_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    assert!(second_request["messages"][1]["content"]
        .as_str()
        .unwrap()
        .contains("A public hello"));
    let _ = std::fs::remove_dir_all(root);
}
