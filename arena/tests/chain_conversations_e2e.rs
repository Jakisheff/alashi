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

#[test]
fn only_bound_confirmed_ordered_events_enter_the_durable_public_journal() {
    let dir = std::env::temp_dir().join(format!(
        "alashi_conversations_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let state = new_state_with_files(dir.join("state.json"), dir.join("seq"));
    let game = Pubkey::new_from_array([3; 32]).to_string();
    let primary = Pubkey::new_from_array([4; 32]).to_string();
    let opponent = Pubkey::new_from_array([5; 32]).to_string();
    let wallet = Pubkey::new_from_array([7; 32]).to_string();
    let other_wallet = Pubkey::new_from_array([8; 32]).to_string();
    let secret = "07".repeat(32);
    state.registrations.lock().unwrap().insert(
        RECORD.into(),
        RegisteredAgent {
            wallet: wallet.clone(),
            owner_id: "1".repeat(64),
            character_id: "2".repeat(64),
            recovery_hash: format!("{:x}", Sha256::digest([7; 32])),
            challenge: "3".repeat(64),
            receipt: Some(Receipt {
                mode: "agent_lifecycle_v2".into(),
                network: "devnet".into(),
                wallet: wallet.clone(),
                signature: "synthetic-registration".into(),
                slot: 1,
                fee_lamports: "0".into(),
                commitment: "confirmed".into(),
            }),
            created_at: 1,
        },
    );
    save_snapshot(&state).unwrap();
    let offer_sig = "3".repeat(88);
    let accept_sig = "4".repeat(88);
    let offer_id = format!("{offer_sig}:2");
    let accept_id = format!("{accept_sig}:2");
    let view = json!({"ok":true,"cluster":"devnet",
    "program_id":"3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC","game_pda":game,
    "game":{"phase":"Market","settled":false,"epoch":1},"history_complete":true,
    "factions":[{"pda":primary,"wallet":wallet,"alive":true},{"pda":opponent,"wallet":other_wallet,"alive":true}],
    "events":[
        {"type":"phase_advanced","round":2,"id":"advance:2"},
        {"type":"barter_proposed","game":game,"from":opponent,"offer":"0","goods":1,"price":"3000000",
         "signature":offer_sig,"slot":101,"id":offer_id},
        {"type":"barter_accepted","game":game,"by":primary,"from":opponent,"offer":"0",
         "signature":accept_sig,"slot":101,"id":accept_id}
    ]});
    let view = Arc::new(Mutex::new(view));
    let (upstream, reads) = mock_projection(view.clone());
    std::env::set_var("ALASHI_CHAIN_API_PORT", upstream.to_string());
    let port = serve_on(state.clone(), "127.0.0.1:0", 60).unwrap().port();
    let path = format!("/chain/devnet/games/{game}/runner/conversations");
    let empty = request(
        port,
        "GET",
        &format!("/chain/devnet/games/{game}/conversations"),
        json!({}),
        None,
    );
    assert_eq!(empty.0, 200);
    assert_eq!(empty.1["entries"].as_array().unwrap().len(), 0);
    let before = reads.load(Ordering::SeqCst);
    let denied = request(
        port,
        "POST",
        &path,
        json!({"runner_token":"f".repeat(64),"client_entry_id":"one",
        "kind":"accepted_confirmed","offer_id":"0","proposer_faction_pda":opponent,
        "signature":accept_sig,"slot":"101"}),
        Some(ORIGIN),
    );
    assert_eq!(denied.0, 403);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        before,
        "invalid token must not fan out to chain API"
    );
    let bound = request(
        port,
        "POST",
        &format!("/chain/devnet/games/{game}/runner/bind"),
        json!({"agent_record_id":RECORD,"recovery_secret":secret,"faction_pda":primary}),
        None,
    );
    assert_eq!(bound.0, 200, "{:?}", bound.1);
    let token = bound.1["runner_token"].as_str().unwrap();
    let body = json!({"runner_token":token,"client_entry_id":"one","kind":"accepted_confirmed",
        "offer_id":"0","proposer_faction_pda":opponent,"signature":accept_sig,"slot":"101"});
    let wrong = request(
        port,
        "POST",
        &path,
        json!({"runner_token":token,"client_entry_id":"wrong",
        "kind":"accepted_confirmed","offer_id":"0","proposer_faction_pda":opponent,
        "signature":"5".repeat(88),"slot":"101"}),
        Some(ORIGIN),
    );
    assert_eq!(wrong.1["error"], "receipt_pending");
    {
        let mut projected = view.lock().unwrap();
        projected["events"].as_array_mut().unwrap().swap(1, 2);
    }
    let reversed = request(port, "POST", &path, body.clone(), Some(ORIGIN));
    assert_eq!(
        reversed.1["error"], "receipt_pending",
        "later same-slot offer cannot support earlier acceptance"
    );
    {
        let mut projected = view.lock().unwrap();
        projected["events"].as_array_mut().unwrap().swap(1, 2);
    }
    let accepted = request(port, "POST", &path, body.clone(), Some(ORIGIN));
    assert_eq!(accepted.0, 200, "{:?}", accepted.1);
    assert_eq!(accepted.1["entry"]["receipt"]["event_id"], accept_id);
    let duplicate = request(port, "POST", &path, body, Some(ORIGIN));
    assert_eq!(
        duplicate.1["entry"]["entry_id"],
        accepted.1["entry"]["entry_id"]
    );
    let page = request(
        port,
        "GET",
        &format!("/chain/devnet/games/{game}/conversations?after=0&limit=50"),
        json!({}),
        None,
    );
    assert_eq!(page.0, 200);
    let rows = page.1["entries"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["kind"], "offer_confirmed");
    assert_eq!(rows[0]["receipt"]["event_id"], offer_id);
    assert_eq!(rows[1]["in_reply_to"], rows[0]["entry_id"]);
    assert_eq!(rows[1]["round"], 2);
    assert_eq!(page.1["history_complete"], false);
    let public = page.1.to_string();
    assert!(!public.contains(token));
    assert!(!public.contains(&secret));
    let persisted = new_state_with_files(dir.join("state.json"), dir.join("seq"));
    load_snapshot(&persisted).unwrap();
    let persisted_port = serve_on(persisted, "127.0.0.1:0", 60).unwrap().port();
    assert_eq!(
        request(
            persisted_port,
            "GET",
            &format!("/chain/devnet/games/{game}/conversations?after=1"),
            json!({}),
            None
        )
        .1["entries"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let mut corrupted: Value =
        serde_json::from_slice(&std::fs::read(dir.join("state.json")).unwrap()).unwrap();
    corrupted["chain_conversations"]["games"][&game]["entries"][1]["in_reply_to"] =
        json!("missing-offer");
    std::fs::write(dir.join("state.json"), corrupted.to_string()).unwrap();
    let rejected = new_state_with_files(dir.join("state.json"), dir.join("seq"));
    assert!(
        load_snapshot(&rejected).is_err(),
        "broken offer reference must abort snapshot load"
    );
    let _ = std::fs::remove_dir_all(dir);
}
