//! Synthetic chain projection only: exercises owner admission, durable quota,
//! private runner claim and confirmed-event status without a devnet write.
use alashi_rules::anchor_lang::prelude::Pubkey;
use arena::api::{
    load_snapshot, new_state_with_files, save_snapshot, serve_on, AppState, RegisteredAgent,
};
use arena::registration::Receipt;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use solana_ed25519::ed_sigs::{SigningKey, VerificationKey};
use solana_signature::Signature;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const RECORD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const ORIGIN: &str = "https://alashi.network";

fn request(
    port: u16,
    method: &str,
    path: &str,
    body: Value,
    cookie: Option<&str>,
    origin: Option<&str>,
) -> (u16, String, Value) {
    request_with_bearer(port, method, path, body, cookie, origin, None)
}

fn request_with_bearer(
    port: u16,
    method: &str,
    path: &str,
    body: Value,
    cookie: Option<&str>,
    origin: Option<&str>,
    bearer: Option<&str>,
) -> (u16, String, Value) {
    let payload = if method == "POST" {
        body.to_string()
    } else {
        String::new()
    };
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(4)))
        .unwrap();
    let raw = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n{}{}{}Content-Length: {}\r\nConnection: close\r\n\r\n{payload}",
        cookie.map(|c| format!("Cookie: {c}\r\n")).unwrap_or_default(),
        origin.map(|o| format!("Origin: {o}\r\n")).unwrap_or_default(),
        bearer.map(|b| format!("Authorization: Bearer {b}\r\n")).unwrap_or_default(),payload.len());
    stream.write_all(raw.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    (
        head.split_whitespace().nth(1).unwrap().parse().unwrap(),
        head.to_string(),
        serde_json::from_str(body).unwrap(),
    )
}

fn fixture_registration(state: &AppState, record: &str, seed: u8) -> (SigningKey, String, String) {
    let signing = SigningKey::from([seed; 32]);
    let vk: [u8; 32] = VerificationKey::from(&signing).into();
    let wallet = Pubkey::new_from_array(vk).to_string();
    let recovery = format!("{seed:02x}").repeat(32);
    state.registrations.lock().unwrap().insert(
        record.into(),
        RegisteredAgent {
            wallet: wallet.clone(),
            owner_id: "1".repeat(64),
            character_id: "2".repeat(64),
            recovery_hash: format!("{:x}", Sha256::digest([seed; 32])),
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
    (signing, wallet, recovery)
}

fn mock_projection(view: Arc<Mutex<Value>>) -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let reads = Arc::new(AtomicUsize::new(0));
    let count = reads.clone();
    std::thread::spawn(move || {
        for incoming in listener.incoming() {
            let Ok(mut stream) = incoming else { continue };
            stream.set_read_timeout(Some(Duration::from_secs(3))).ok();
            let mut request = [0; 1024];
            if stream.read(&mut request).is_err() {
                continue;
            }
            count.fetch_add(1, Ordering::SeqCst);
            let body = view.lock().unwrap().to_string();
            let header = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = stream
                .write_all(header.as_bytes())
                .and_then(|_| stream.write_all(body.as_bytes()));
        }
    });
    (port, reads)
}

#[test]
fn owner_chain_wishes_are_private_durable_and_need_a_later_matching_receipt() {
    let dir = std::env::temp_dir().join(format!(
        "alashi_chain_wish_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let state = new_state_with_files(dir.join("state.json"), dir.join("seq"));
    let (signing, wallet, recovery) = fixture_registration(&state, RECORD, 7);
    fixture_registration(&state, OTHER, 8);
    save_snapshot(&state).unwrap();
    let game = Pubkey::new_from_array([3; 32]).to_string();
    let faction = Pubkey::new_from_array([4; 32]).to_string();
    let view = Arc::new(Mutex::new(json!({"ok":true,"cluster":"devnet",
        "program_id":"3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC","game_pda":game,
        "game":{"phase":"Action","settled":false},"snapshot_slot":100,"journal_through_slot":100,
        "factions":[{"pda":faction,"wallet":wallet,"alive":true}],"events":[]})));
    let (mock_port, reads) = mock_projection(view.clone());
    std::env::set_var("ALASHI_CHAIN_API_PORT", mock_port.to_string());
    let port = serve_on(state.clone(), "127.0.0.1:0", 60).unwrap().port();
    assert_eq!(request(port,"POST",&format!("/chain/devnet/games/{game}/runner/bind"),
        json!({"agent_record_id":RECORD,"recovery_secret":"f".repeat(64),"faction_pda":faction}),None,None).0,403);
    assert_eq!(
        request(
            port,
            "POST",
            &format!("/chain/devnet/games/{game}/runner/bind"),
            json!({"agent_record_id":RECORD,"recovery_secret":recovery,
            "faction_pda":Pubkey::new_from_array([5;32]).to_string()}),
            None,
            None
        )
        .0,
        403
    );
    let binding = request(
        port,
        "POST",
        &format!("/chain/devnet/games/{game}/runner/bind"),
        json!({"agent_record_id":RECORD,"recovery_secret":recovery,"faction_pda":faction}),
        None,
        None,
    );
    assert_eq!(binding.0, 200, "{:?}", binding.2);
    assert_eq!(binding.2["faction_wallet"], wallet);
    let mut token = binding.2["runner_token"].as_str().unwrap().to_string();
    let before_invalid = reads.load(Ordering::SeqCst);
    let invalid_claim = request(
        port,
        "POST",
        &format!("/chain/devnet/games/{game}/runner/wishes/claim"),
        json!({"runner_token":"f".repeat(64),"after":0,"limit":1}),
        None,
        None,
    );
    assert_eq!(invalid_claim.0, 403);
    assert_eq!(
        reads.load(Ordering::SeqCst),
        before_invalid,
        "invalid runner must not call chain RPC"
    );
    assert!(!std::fs::read_to_string(dir.join("state.json"))
        .unwrap()
        .contains(&token));

    let challenge = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/challenge"),
        json!({}),
        None,
        Some(ORIGIN),
    );
    assert_eq!(challenge.0, 200, "{:?}", challenge.2);
    let signature = Signature::from(
        signing
            .sign(challenge.2["message"].as_str().unwrap().as_bytes())
            .to_bytes(),
    )
    .to_string();
    let login = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/browser/session"),
        json!({"challenge_id":challenge.2["challenge_id"],"signature":signature}),
        None,
        Some(ORIGIN),
    );
    assert_eq!(login.0, 200, "{:?}", login.2);
    let cookie = login
        .1
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
        .unwrap()
        .split_once(':')
        .unwrap()
        .1
        .trim()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // A local runner proves the registered wallet again and keeps this short
    // bearer only in memory. It may bind through the public wallet-proof path
    // without sending its recovery secret over that path.
    let agent_challenge = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/challenge"),
        json!({}),
        None,
        Some(ORIGIN),
    );
    assert_eq!(agent_challenge.0, 200);
    let agent_signature = Signature::from(
        signing
            .sign(agent_challenge.2["message"].as_str().unwrap().as_bytes())
            .to_bytes(),
    )
    .to_string();
    let bearer_login = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/session"),
        json!({"challenge_id":agent_challenge.2["challenge_id"],"signature":agent_signature}),
        None,
        Some(ORIGIN),
    );
    assert_eq!(bearer_login.0, 200);
    let bearer = bearer_login.2["owner_session"]
        .as_str()
        .unwrap()
        .to_string();
    let remote_bind = request_with_bearer(
        port,
        "POST",
        &format!("/chain/devnet/games/{game}/runner/bind-wallet"),
        json!({"agent_record_id":RECORD,"faction_pda":faction}),
        None,
        Some(ORIGIN),
        Some(&bearer),
    );
    assert_eq!(remote_bind.0, 200, "{:?}", remote_bind.2);
    assert_eq!(remote_bind.2["faction_wallet"], wallet);
    token = remote_bind.2["runner_token"].as_str().unwrap().to_string();
    let wrong_owner = request_with_bearer(
        port,
        "POST",
        &format!("/chain/devnet/games/{game}/runner/bind-wallet"),
        json!({"agent_record_id":OTHER,"faction_pda":faction}),
        None,
        Some(ORIGIN),
        Some(&bearer),
    );
    assert_eq!(wrong_owner.0, 401);

    // Browser-created pairing survives a lost first Set-Cookie/body response:
    // the same HttpOnly nonce yields exactly the same owner cookie on retry.
    let wrong_reconnect = request(port,"POST","/owner/pairing/start",
        json!({"expected_record_id":OTHER}),None,Some(ORIGIN));
    assert_eq!(wrong_reconnect.0,200);
    assert!(wrong_reconnect.1.contains("Path=/owner/pairing"));
    assert!(wrong_reconnect.1.contains("Max-Age=1800"));
    assert!(wrong_reconnect.1.contains("Secure; HttpOnly; SameSite=Strict"));
    assert_eq!(request_with_bearer(port,"POST",&format!("/agents/{RECORD}/owner/pairing/complete"),
        json!({"pairing_grant":wrong_reconnect.2["pairing_grant"],"game_pda":game,"faction_pda":faction}),
        None,Some(ORIGIN),Some(&bearer)).0,403);
    let start = request(
        port,
        "POST",
        "/owner/pairing/start",
        json!({"expected_record_id":RECORD}),
        None,
        Some(ORIGIN),
    );
    assert_eq!(start.0, 200, "{:?}", start.2);
    let pair_cookie = start
        .1
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
        .unwrap()
        .split_once(':')
        .unwrap()
        .1
        .trim()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let grant = start.2["pairing_grant"].as_str().unwrap();
    assert_eq!(request(port,"POST","/owner/pairing/status",json!({}),
        Some(&format!("__Secure-alashi-pair={}","0".repeat(64))),Some(ORIGIN)).0,401);
    assert_eq!(
        request(
            port,
            "POST",
            "/owner/pairing/status",
            json!({}),
            Some(&pair_cookie),
            Some(ORIGIN)
        )
        .2["status"],
        "waiting"
    );
    let complete_body = json!({"pairing_grant":grant,"game_pda":game,"faction_pda":faction});
    assert_eq!(
        request_with_bearer(
            port,
            "POST",
            &format!("/agents/{RECORD}/owner/pairing/complete"),
            complete_body.clone(),
            None,
            Some("https://evil.example"),
            Some(&bearer)
        )
        .0,
        403
    );
    let completed = request_with_bearer(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/pairing/complete"),
        complete_body.clone(),
        None,
        Some(ORIGIN),
        Some(&bearer),
    );
    assert_eq!(completed.0, 200, "{:?}", completed.2);
    assert_eq!(
        request_with_bearer(
            port,
            "POST",
            &format!("/agents/{RECORD}/owner/pairing/complete"),
            complete_body,
            None,
            Some(ORIGIN),
            Some(&bearer)
        )
        .0,
        401
    );
    let fallback = completed.2["owner_url"].as_str().unwrap().to_string();
    assert!(fallback.contains(&format!("?game={game}&player={faction}#owner={RECORD}.")));
    let fallback_code = fallback.split("#owner=").nth(1).unwrap().split('.').nth(1).unwrap();
    assert_eq!(request(port,"POST",&format!("/agents/{OTHER}/owner/browser/handoff"),
        json!({"code":fallback_code,"game_pda":game}),None,Some(ORIGIN)).0,401);
    assert_eq!(request(port,"POST",&format!("/agents/{RECORD}/owner/browser/handoff"),
        json!({"code":fallback_code,"game_pda":Pubkey::new_from_array([6;32]).to_string()}),None,Some(ORIGIN)).0,401);
    let first_poll = request(
        port,
        "POST",
        "/owner/pairing/status",
        json!({}),
        Some(&pair_cookie),
        Some(ORIGIN),
    );
    assert_eq!(first_poll.2["status"], "paired");
    let second_poll = request(
        port,
        "POST",
        "/owner/pairing/status",
        json!({}),
        Some(&pair_cookie),
        Some(ORIGIN),
    );
    assert_eq!(second_poll.2["status"], "paired");
    let owner_cookie_line = |headers: &str| {
        headers
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
            .unwrap()
            .to_string()
    };
    assert_eq!(
        owner_cookie_line(&first_poll.1),
        owner_cookie_line(&second_poll.1)
    );
    assert!(first_poll.1.contains(&format!("Path=/agents/{RECORD}/owner")));
    assert!(first_poll.1.contains("Max-Age=604800; Secure; HttpOnly; SameSite=Strict"));
    let retry_state = new_state_with_files(dir.join("state.json"), dir.join("seq"));
    load_snapshot(&retry_state).unwrap();
    let retry_port = serve_on(retry_state, "127.0.0.1:0", 20).unwrap().port();
    let retry_after_restart = request(retry_port,"POST","/owner/pairing/status",json!({}),Some(&pair_cookie),Some(ORIGIN));
    assert_eq!(retry_after_restart.2["status"],"paired");
    assert_eq!(owner_cookie_line(&first_poll.1),owner_cookie_line(&retry_after_restart.1));
    let returned_cookie = owner_cookie_line(&first_poll.1)
        .split_once(':')
        .unwrap()
        .1
        .trim()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    assert_eq!(
        request(
            port,
            "GET",
            &format!("/agents/{RECORD}/owner/browser/session"),
            json!({}),
            Some(&returned_cookie),
            None
        )
        .0,
        200
    );
    assert_eq!(
        request(
            port,
            "POST",
            &format!("/agents/{RECORD}/owner/browser/handoff"),
            json!({"code":fallback_code,"game_pda":game}),
            None,
            Some(ORIGIN)
        )
        .0,
        401
    );

    // A second Copy in the same browser invalidates its prior pending grant.
    let old_start = request(port,"POST","/owner/pairing/start",json!({}),None,Some(ORIGIN));
    let old_cookie = old_start.1.lines().find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
        .unwrap().split_once(':').unwrap().1.trim().split(';').next().unwrap().to_string();
    let new_start = request(port,"POST","/owner/pairing/start",json!({}),Some(&old_cookie),Some(ORIGIN));
    assert_eq!(new_start.0,200);
    assert_eq!(request_with_bearer(port,"POST",&format!("/agents/{RECORD}/owner/pairing/complete"),
        json!({"pairing_grant":old_start.2["pairing_grant"],"game_pda":game,"faction_pda":faction}),
        None,Some(ORIGIN),Some(&bearer)).0,401);
    assert_eq!(
        request(
            port,
            "POST",
            &format!("/agents/{RECORD}/owner/browser/logout"),
            json!({}),
            Some(&returned_cookie),
            Some(ORIGIN)
        )
        .0,
        200
    );
    assert_eq!(
        request(
            port,
            "POST",
            "/owner/pairing/status",
            json!({}),
            Some(&pair_cookie),
            Some(ORIGIN)
        )
        .0,
        401
    );

    // Reversing completion order is equally exclusive: a redeemed fallback
    // invalidates the pending browser poll and cannot itself be replayed.
    let another = request(
        port,
        "POST",
        "/owner/pairing/start",
        json!({}),
        None,
        Some(ORIGIN),
    );
    let another_cookie = another
        .1
        .lines()
        .find(|line| line.to_ascii_lowercase().starts_with("set-cookie:"))
        .unwrap()
        .split_once(':')
        .unwrap()
        .1
        .trim()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let another_complete = request_with_bearer(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/pairing/complete"),
        json!({"pairing_grant":another.2["pairing_grant"],"game_pda":game,"faction_pda":faction}),
        None,
        Some(ORIGIN),
        Some(&bearer),
    );
    assert_eq!(another_complete.0, 200);
    let another_code = another_complete.2["owner_url"]
        .as_str()
        .unwrap()
        .split("#owner=")
        .nth(1)
        .unwrap()
        .split('.')
        .nth(1)
        .unwrap();
    // A process restart between completion and delivery preserves the hashed
    // grant/link state. The original server is no longer used for this path.
    let restored = new_state_with_files(dir.join("state.json"), dir.join("seq"));
    load_snapshot(&restored).unwrap();
    let restored_port = serve_on(restored, "127.0.0.1:0", 20).unwrap().port();
    let redeem = request(
        restored_port,
        "POST",
        &format!("/agents/{RECORD}/owner/browser/handoff"),
        json!({"code":another_code,"game_pda":game}),
        None,
        Some(ORIGIN),
    );
    assert_eq!(redeem.0, 200, "{:?}", redeem.2);
    assert_eq!(
        request(
            restored_port,
            "POST",
            "/owner/pairing/status",
            json!({}),
            Some(&another_cookie),
            Some(ORIGIN)
        )
        .0,
        401
    );
    assert_eq!(
        request(
            restored_port,
            "POST",
            &format!("/agents/{RECORD}/owner/browser/handoff"),
            json!({"code":another_code,"game_pda":game}),
            None,
            Some(ORIGIN)
        )
        .0,
        401
    );

    let path = format!("/agents/{RECORD}/owner/chain-wishes");
    let text = "Private: produce next action";
    let first = json!({"game_pda":game,"client_wish_id":"first","intent":"produce","text":text});
    assert_eq!(
        request(
            port,
            "POST",
            &path,
            first.clone(),
            Some(&cookie),
            Some("https://evil.example")
        )
        .0,
        403
    );
    let admitted = request(
        port,
        "POST",
        &path,
        first.clone(),
        Some(&cookie),
        Some(ORIGIN),
    );
    assert_eq!(admitted.0, 200, "{:?}", admitted.2);
    assert_eq!(admitted.2["remaining"], 2);
    assert_eq!(
        request(
            port,
            "POST",
            &path,
            first.clone(),
            Some(&cookie),
            Some(ORIGIN)
        )
        .2["wish_id"],
        admitted.2["wish_id"]
    );
    assert_eq!(
        request(
            port,
            "POST",
            &path,
            json!({"game_pda":game,"client_wish_id":"first","intent":"sell_one","text":text}),
            Some(&cookie),
            Some(ORIGIN)
        )
        .0,
        409
    );
    for i in 2..=3 {
        let row = request(
            port,
            "POST",
            &path,
            json!({"game_pda":game,"client_wish_id":format!("wish-{i}"),"intent":"produce","text":text}),
            Some(&cookie),
            Some(ORIGIN),
        );
        assert_eq!(row.0, 200, "{:?}", row.2);
        assert_eq!(row.2["remaining"], 3 - i);
    }
    assert_eq!(
        request(
            port,
            "POST",
            &path,
            json!({"game_pda":game,"client_wish_id":"fourth","intent":"produce","text":text}),
            Some(&cookie),
            Some(ORIGIN)
        )
        .0,
        429
    );
    assert_eq!(
        request(
            port,
            "GET",
            &format!("/agents/{OTHER}/owner/chain-wishes?game={game}"),
            json!({}),
            Some(&cookie),
            None
        )
        .0,
        401
    );
    let owner_rows = request(
        port,
        "GET",
        &format!("{path}?game={game}"),
        json!({}),
        Some(&cookie),
        None,
    );
    assert_eq!(owner_rows.0, 200, "{:?}", owner_rows.2);
    assert_eq!(owner_rows.2["wishes"].as_array().unwrap().len(), 3);
    assert_eq!(owner_rows.2["wishes"][0]["text"], text);
    assert!(!view.lock().unwrap().to_string().contains(text));

    let claim_path = format!("/chain/devnet/games/{game}/runner/wishes/claim");
    let claimed = request(
        port,
        "POST",
        &claim_path,
        json!({"runner_token":token,"after":0,"limit":1}),
        None,
        None,
    );
    assert_eq!(claimed.0, 200, "{:?}", claimed.2);
    assert_eq!(claimed.2["wishes"][0]["wish_id"], admitted.2["wish_id"]);
    let lease = claimed.2["wishes"][0]["lease_id"].as_str().unwrap();
    let wish_id = admitted.2["wish_id"].as_str().unwrap();
    let status_path = format!("/chain/devnet/games/{game}/runner/wishes/{wish_id}/status");
    let consumed_body = json!({"runner_token":token,"lease_id":lease,"status":"consumed"});
    let consumed = request(
        port,
        "POST",
        &status_path,
        consumed_body.clone(),
        None,
        None,
    );
    assert_eq!(consumed.0, 200, "{:?}", consumed.2);
    assert_eq!(consumed.2["consumed_after_slot"], 100);
    assert_eq!(
        request(port, "POST", &status_path, consumed_body, None, None).2["status_seq"],
        consumed.2["status_seq"]
    );
    let receipt = json!({"runner_token":token,"status":"confirmed","signature":"synthetic-confirmed-sig","slot":101});
    assert_eq!(
        request(port, "POST", &status_path, receipt.clone(), None, None).2["error"],
        "receipt_pending"
    );
    let uncertain = request(
        port,
        "POST",
        &status_path,
        json!({"runner_token":token,"status":"unconfirmed"}),
        None,
        None,
    );
    assert_eq!(uncertain.0, 200, "{:?}", uncertain.2);
    assert_eq!(uncertain.2["status"], "unconfirmed");
    view.lock().unwrap()["game"] = json!({"phase":"Finished","settled":true});
    view.lock().unwrap()["events"] = json!([{"id":"synthetic-confirmed-sig:4","signature":"synthetic-confirmed-sig",
        "slot":101,"game":game,"type":"produced","faction":faction,"goods":1}]);
    assert_eq!(
        request(
            port,
            "POST",
            &format!("/chain/devnet/games/{game}/runner/bind"),
            json!({"agent_record_id":OTHER,"recovery_secret":"8".repeat(64),"faction_pda":faction}),
            None,
            None
        )
        .0,
        403,
        "a different record cannot take the terminal receipt"
    );
    let rebound = request(
        port,
        "POST",
        &format!("/chain/devnet/games/{game}/runner/bind"),
        json!({"agent_record_id":RECORD,"recovery_secret":recovery,"faction_pda":faction}),
        None,
        None,
    );
    assert_eq!(
        rebound.0, 200,
        "terminal rebind must allow only the existing consumed receipt: {:?}",
        rebound.2
    );
    let new_token = rebound.2["runner_token"].as_str().unwrap();
    assert_eq!(
        request(port, "POST", &status_path, receipt.clone(), None, None).0,
        403,
        "rotated terminal capability revokes the old runner token"
    );
    let confirmed = request(
        port,
        "POST",
        &status_path,
        json!({"runner_token":new_token,"status":"confirmed","signature":"synthetic-confirmed-sig","slot":101}),
        None,
        None,
    );
    assert_eq!(confirmed.0, 200, "{:?}", confirmed.2);
    assert_eq!(confirmed.2["status"], "confirmed");
    assert_eq!(confirmed.2["event_id"], "synthetic-confirmed-sig:4");
    assert_eq!(request(port,"POST",&status_path,
        json!({"runner_token":new_token,"status":"confirmed","signature":"synthetic-confirmed-sig","slot":101}),
        None,None).2["status_seq"],confirmed.2["status_seq"]);
    let reconciled = request(
        port,
        "GET",
        &format!("{path}?game={game}"),
        json!({}),
        Some(&cookie),
        None,
    );
    assert_eq!(reconciled.0, 200, "{:?}", reconciled.2);
    assert_eq!(reconciled.2["chain_observation_available"], true);
    assert_eq!(
        reconciled.2["wishes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|w| w["status"] == "expired")
            .count(),
        2
    );
    let terminal_claim = request(
        port,
        "POST",
        &claim_path,
        json!({"runner_token":new_token,"after":0,"limit":1}),
        None,
        None,
    );
    assert_eq!(terminal_claim.0, 200, "{:?}", terminal_claim.2);
    assert!(terminal_claim.2["wishes"].as_array().unwrap().is_empty());
    // A returning owner can pair the same settled faction after the runner
    // heartbeat expires. Age the persisted fixture, then restart the server.
    let snapshot_path = dir.join("state.json");
    let mut snapshot: Value = serde_json::from_slice(&std::fs::read(&snapshot_path).unwrap()).unwrap();
    snapshot["chain_wishes"]["bindings"][format!("{RECORD}:{game}")]["last_seen_at"] = json!(0);
    std::fs::write(&snapshot_path, snapshot.to_string()).unwrap();
    let restarted = new_state_with_files(dir.join("state.json"), dir.join("seq"));
    load_snapshot(&restarted).unwrap();
    let restarted_port = serve_on(restarted, "127.0.0.1:0", 60).unwrap().port();
    let return_start = request(restarted_port, "POST", "/owner/pairing/start",
        json!({"expected_record_id":RECORD}), None, Some(ORIGIN));
    assert_eq!(return_start.0, 200);
    let return_complete = request_with_bearer(restarted_port, "POST",
        &format!("/agents/{RECORD}/owner/pairing/complete"),
        json!({"pairing_grant":return_start.2["pairing_grant"],"game_pda":game,"faction_pda":faction}),
        None, Some(ORIGIN), Some(&bearer));
    assert_eq!(return_complete.0, 200, "{:?}", return_complete.2);
    assert_eq!(return_complete.2["game_pda"], game);
    let persisted = request(
        restarted_port,
        "GET",
        &format!("{path}?game={game}"),
        json!({}),
        Some(&cookie),
        None,
    );
    assert_eq!(persisted.0, 200, "{:?}", persisted.2);
    assert_eq!(persisted.2["remaining"], 0);
    assert!(persisted.2["wishes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["wish_id"] == admitted.2["wish_id"] && w["status"] == "confirmed"));
    assert_eq!(
        persisted.2["wishes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|w| w["status"] == "expired")
            .count(),
        2
    );
    assert_eq!(
        request(
            restarted_port,
            "POST",
            &path,
            json!({"game_pda":game,"client_wish_id":"fifth","intent":"produce","text":text}),
            Some(&cookie),
            Some(ORIGIN)
        )
        .0,
        409
    );
    let _ = std::fs::remove_dir_all(dir);
}
