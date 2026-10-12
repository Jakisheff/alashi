//! Synthetic confirmed projection: no Solana RPC, wallet signing, or Game write.
use alashi_rules::anchor_lang::prelude::Pubkey;
use arena::api::{load_snapshot, new_state_with_files, save_snapshot, serve_on, RegisteredAgent};
use arena::registration::Receipt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const RECORD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const ORIGIN: &str = "https://alashi.network";

fn request(port: u16, method: &str, path: &str, body: Value, origin: Option<&str>) -> (u16, Value) {
    let payload = if method == "POST" {
        body.to_string()
    } else {
        String::new()
    };
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    let raw=format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        origin.map(|o|format!("Origin: {o}\r\n")).unwrap_or_default(),payload.len());
    stream.write_all(raw.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        serde_json::from_str(body).unwrap(),
    )
}
fn mock_projection(view: Arc<Mutex<Value>>) -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    std::thread::spawn(move || {
        for incoming in listener.incoming() {
            let Ok(mut stream) = incoming else { continue };
            let mut request = [0; 1024];
            if stream.read(&mut request).is_err() {
                continue;
            }
            count.fetch_add(1, Ordering::SeqCst);
            let body = view.lock().unwrap().to_string();
            let header=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
            let _ = stream
                .write_all(header.as_bytes())
                .and_then(|_| stream.write_all(body.as_bytes()));
        }
    });
    (port, reads)
}

// Contract-level synthetic projection, not evidence of real agent conversation.
fn row(response: &Value) -> &Value {
    response
        .get("message")
        .or_else(|| response.get("entry"))
        .expect("public dialogue message")
}
#[test]
fn two_bound_authors_publish_durable_context_checked_dialogue() {
    let dir = std::env::temp_dir().join(format!(
        "alashi_dialogue_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let state = new_state_with_files(dir.join("state.json"), dir.join("seq"));
    let game = Pubkey::new_from_array([3; 32]).to_string();
    let a = Pubkey::new_from_array([4; 32]).to_string();
    let b = Pubkey::new_from_array([5; 32]).to_string();
    let wa = Pubkey::new_from_array([7; 32]).to_string();
    let wb = Pubkey::new_from_array([8; 32]).to_string();
    let rb = "b".repeat(64);
    for (record, wallet, byte) in [
        (RECORD.to_string(), wa.clone(), 7u8),
        (rb.clone(), wb.clone(), 8u8),
    ] {
        state.registrations.lock().unwrap().insert(
            record,
            RegisteredAgent {
                wallet: wallet.clone(),
                owner_id: format!("{byte:x}").repeat(64),
                character_id: format!("{:x}", byte + 1).repeat(64),
                recovery_hash: format!("{:x}", Sha256::digest([byte; 32])),
                challenge: "3".repeat(64),
                receipt: Some(Receipt {
                    mode: "agent_lifecycle_v2".into(),
                    network: "devnet".into(),
                    wallet,
                    signature: "synthetic-registration".into(),
                    slot: 1,
                    fee_lamports: "0".into(),
                    commitment: "confirmed".into(),
                }),
                created_at: 1,
            },
        );
    }
    save_snapshot(&state).unwrap();
    let deadline = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 600;
    let view = Arc::new(Mutex::new(
        json!({"ok":true,"cluster":"devnet","program_id":"3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC","game_pda":game,
        "game":{"phase":"Market","round":2,"phase_ends_at":deadline,"settled":false,"epoch":1},"history_complete":true,
        "factions":[{"pda":a,"wallet":wa,"alive":true},{"pda":b,"wallet":wb,"alive":true}],
        "events":[{"type":"phase_advanced","round":2,"id":format!("{}:2", "1".repeat(64)),"game":game,"slot":100}]}),
    ));
    let (upstream, reads) = mock_projection(view.clone());
    std::env::set_var("ALASHI_CHAIN_API_PORT", upstream.to_string());
    let port = serve_on(state.clone(), "127.0.0.1:0", 60).unwrap().port();
    let path = format!("/chain/devnet/games/{game}/runner/dialogue");
    let public = format!("/chain/devnet/games/{game}/dialogue?after=0&limit=100");
    let body = |token: &str, id: &str, to: &str| json!({"runner_token":token,"client_message_id":id,"expected_round":2,"expected_phase":"Market","expected_phase_ends_at":deadline,"to_faction_pda":to,"text":"What price would you accept?"});
    let before = reads.load(Ordering::SeqCst);
    let denied = request(
        port,
        "POST",
        &path,
        body(&"f".repeat(64), "bad", &b),
        Some(ORIGIN),
    );
    assert_ne!(denied.1["ok"], true);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        before,
        "auth must reject before projection request"
    );
    let bind = |record: &str, secret: &str, faction: &str| {
        let result = request(
            port,
            "POST",
            &format!("/chain/devnet/games/{game}/runner/bind"),
            json!({"agent_record_id":record,"recovery_secret":secret,"faction_pda":faction}),
            None,
        );
        assert_eq!(result.1["ok"], true, "{}", result.1);
        result.1["runner_token"].as_str().unwrap().to_string()
    };
    let ta = bind(RECORD, &"07".repeat(32), &a);
    let tb = bind(&rb, &"08".repeat(32), &b);
    for (field, value) in [
        ("author_faction_pda", json!(b)),
        (
            "to_faction_pda",
            json!(Pubkey::new_from_array([9; 32]).to_string()),
        ),
        ("reply_to_message_id", json!("missing")),
        ("related_offer_entry_id", json!("missing")),
        ("expected_round", json!(1)),
    ] {
        let mut invalid = body(&ta, &format!("invalid-{field}"), &b);
        invalid[field] = value;
        assert_ne!(
            request(port, "POST", &path, invalid, Some(ORIGIN)).1["ok"],
            true,
            "must reject {field}"
        );
    }
    let other_game = Pubkey::new_from_array([10; 32]).to_string();
    assert_ne!(
        request(
            port,
            "POST",
            &format!("/chain/devnet/games/{other_game}/runner/dialogue"),
            body(&ta, "wrong-game", &b),
            Some(ORIGIN)
        )
        .1["ok"],
        true
    );
    let first_body = body(&ta, "a-first", &b);
    let first = request(port, "POST", &path, first_body.clone(), Some(ORIGIN)).1;
    assert_eq!(first["ok"], true, "{first}");
    assert_eq!(row(&first)["author_faction_pda"], a);
    assert_eq!(row(&first)["source"], "runner_reported");
    assert!(
        row(&first).get("receipt").is_none(),
        "text has no transaction receipt"
    );
    let mid = row(&first)["message_id"].as_str().unwrap();
    let mut reply = body(&tb, "b-reply", &a);
    reply["reply_to_message_id"] = json!(mid);
    reply["text"] = json!("I will check the available goods.");
    let second = request(port, "POST", &path, reply, Some(ORIGIN)).1;
    assert_eq!(second["ok"], true, "{second}");
    assert_eq!(row(&second)["author_faction_pda"], b);
    assert_eq!(row(&second)["reply_to_message_id"], mid);
    view.lock().unwrap()["game"]["phase"] = json!("Action");
    let before_retry = reads.load(Ordering::SeqCst);
    assert_eq!(
        request(port, "POST", &path, first_body.clone(), Some(ORIGIN)).1,
        first,
        "retry survives phase transition"
    );
    assert_eq!(
        reads.load(Ordering::SeqCst),
        before_retry,
        "idempotency before projection"
    );
    let mut conflict = first_body;
    conflict["text"] = json!("different text");
    assert_ne!(
        request(port, "POST", &path, conflict, Some(ORIGIN)).1["ok"],
        true
    );
    assert_ne!(
        request(
            port,
            "POST",
            &path,
            body(&ta, "stale-new", &b),
            Some(ORIGIN)
        )
        .1["ok"],
        true
    );
    let page = request(port, "GET", &public, json!({}), None).1;
    assert_eq!(page["schema"], "alashi.chain_dialogue.v1");
    assert_eq!(page["entries"].as_array().unwrap().len(), 2);
    assert!(!page.to_string().contains(&ta));
    assert!(!page.to_string().contains(&tb));
    let restored = new_state_with_files(dir.join("state.json"), dir.join("seq"));
    load_snapshot(&restored).unwrap();
    let restored_port = serve_on(restored, "127.0.0.1:0", 60).unwrap().port();
    assert_eq!(
        request(restored_port, "GET", &public, json!({}), None).1["entries"],
        page["entries"]
    );
    // An unwritable temporary snapshot must roll back a new author/context message.
    view.lock().unwrap()["game"]["round"] = json!(3);
    let mut save_fail = body(&ta, "disk-failure", &b);
    save_fail["expected_round"] = json!(3);
    save_fail["expected_phase"] = json!("Action");
    std::thread::sleep(Duration::from_secs(6)); // pass the real five-second admission interval
    std::fs::create_dir(dir.join("state.json.tmp")).unwrap();
    let refused = request(port, "POST", &path, save_fail, Some(ORIGIN)).1;
    assert_eq!(
        refused["error"], "storage_failed",
        "must reach persistence rollback: {refused}"
    );
    assert_eq!(
        request(port, "GET", &public, json!({}), None).1["entries"],
        page["entries"],
        "failed save must not publish"
    );
    std::fs::remove_dir(dir.join("state.json.tmp")).unwrap();
    let mut corrupted: Value =
        serde_json::from_slice(&std::fs::read(dir.join("state.json")).unwrap()).unwrap();
    corrupted["chain_dialogue"] = json!({"unexpected_field":true});
    std::fs::write(dir.join("state.json"), corrupted.to_string()).unwrap();
    let rejected = new_state_with_files(dir.join("state.json"), dir.join("seq"));
    assert!(
        load_snapshot(&rejected).is_err(),
        "unknown dialogue snapshot field must fail closed"
    );
    let _ = std::fs::remove_dir_all(dir);
}
