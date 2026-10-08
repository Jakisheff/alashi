//! Browser owner-session integration: cookie scope, wallet proof, persistence and logout.
//! Runs against isolated local arena state; no chain, model, or external service.
use arena::api::{
    load_snapshot, new_state_with_files, save_snapshot, serve_on, AppState, RegisteredAgent,
};
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
const OTHER_RECORD: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const ORIGIN: &str = "https://alashi.network";

fn owner_identity() -> (SigningKey, String) {
    let key = SigningKey::from([7u8; 32]);
    let vk: [u8; 32] = VerificationKey::from(&key).into();
    (
        key,
        alashi_rules::anchor_lang::prelude::Pubkey::new_from_array(vk).to_string(),
    )
}

fn register(state: &AppState, record_id: &str, seed: [u8; 32], secret: [u8; 32]) -> String {
    let key = SigningKey::from(seed);
    let vk: [u8; 32] = VerificationKey::from(&key).into();
    let wallet = alashi_rules::anchor_lang::prelude::Pubkey::new_from_array(vk).to_string();
    state.registrations.lock().unwrap().insert(
        record_id.into(),
        RegisteredAgent {
            wallet: wallet.clone(),
            owner_id: format!("{:064x}", seed[0]),
            character_id: format!("{:064x}", seed[0] + 1),
            recovery_hash: format!("{:x}", Sha256::digest(secret)),
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
    format!("{:02x}", secret[0]).repeat(32)
}

fn isolated_state() -> (std::sync::Arc<AppState>, PathBuf) {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let root = std::env::temp_dir().join(format!(
        "alashi_owner_cookie_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    (
        new_state_with_files(root.join("state.json"), root.join("seq")),
        root,
    )
}

#[derive(Debug)]
struct Response {
    status: u16,
    headers: String,
    body: Value,
}
impl Response {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.lines().find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case(name).then(|| value.trim())
        })
    }
}

fn request(
    port: u16,
    method: &str,
    path: &str,
    body: Option<&Value>,
    cookie: Option<&str>,
    bearer: Option<&str>,
    origin: Option<&str>,
) -> Response {
    let raw_body = body.map(Value::to_string).unwrap_or_default();
    let cookie_header = cookie
        .map(|s| format!("Cookie: {s}\r\n"))
        .unwrap_or_default();
    let bearer_header = bearer
        .map(|s| format!("Authorization: Bearer {s}\r\n"))
        .unwrap_or_default();
    let origin_header = origin
        .map(|s| format!("Origin: {s}\r\n"))
        .unwrap_or_default();
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect isolated arena");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let raw = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n{cookie_header}{bearer_header}{origin_header}Content-Length: {}\r\nConnection: close\r\n\r\n{}", raw_body.len(), raw_body);
    stream.write_all(raw.as_bytes()).unwrap();
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).unwrap();
    let text = String::from_utf8(bytes).expect("HTTP response UTF-8");
    let (head, body) = text.split_once("\r\n\r\n").expect("response headers");
    let status = head
        .lines()
        .next()
        .unwrap()
        .split_whitespace()
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    Response {
        status,
        headers: head.to_string(),
        body: serde_json::from_str(body).expect("JSON response"),
    }
}

fn browser_login(port: u16, key: &SigningKey) -> (String, String, Value) {
    let challenge = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/challenge"),
        Some(&json!({})),
        None,
        None,
        Some(ORIGIN),
    );
    assert_eq!(challenge.status, 200, "{:?}", challenge.body);
    let challenge_id = challenge.body["challenge_id"].as_str().unwrap();
    let message = challenge.body["message"].as_str().unwrap();
    let signature = Signature::from(key.sign(message.as_bytes()).to_bytes()).to_string();
    let login = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/browser/session"),
        Some(&json!({"challenge_id":challenge_id,"signature":signature})),
        None,
        None,
        Some(ORIGIN),
    );
    assert_eq!(login.status, 200, "{:?}", login.body);
    let set_cookie = login
        .header("set-cookie")
        .expect("browser login sets HttpOnly cookie")
        .to_string();
    let cookie = set_cookie.split(';').next().unwrap().to_string();
    (cookie, set_cookie, login.body)
}

#[test]
fn browser_cookie_auth_is_scoped_persistent_revocable_and_csrf_checked() {
    let (state, root) = isolated_state();
    let secret = [0x11; 32];
    let recovery_secret = register(&state, RECORD, [7; 32], secret);
    let _ = register(&state, OTHER_RECORD, [8; 32], [0x22; 32]);
    save_snapshot(&state).unwrap();
    let server = serve_on(state.clone(), "127.0.0.1:0", 60).unwrap();
    let port = server.port();
    let (key, wallet) = owner_identity();

    // Login is an off-chain, exact-message signature and returns only the scoped cookie.
    let (cookie, set_cookie, login_body) = browser_login(port, &key);
    assert!(set_cookie.starts_with("__Secure-alashi-owner="));
    assert!(set_cookie.contains(&format!("Path=/agents/{RECORD}/owner")));
    assert!(set_cookie.contains("Max-Age=604800"));
    assert!(
        set_cookie.contains("Secure")
            && set_cookie.contains("HttpOnly")
            && set_cookie.contains("SameSite=Strict")
    );
    assert!(!set_cookie.to_ascii_lowercase().contains("domain="));
    assert_eq!(login_body["scope"][0], "owner_wishes");
    assert!(login_body["expires_at"].as_i64().is_some());
    assert!(login_body.get("owner_session").is_none());
    assert!(!login_body
        .to_string()
        .contains(cookie.split('=').nth(1).unwrap()));
    let snapshot = std::fs::read_to_string(root.join("state.json")).unwrap();
    assert!(
        !snapshot.contains(cookie.split('=').nth(1).unwrap()),
        "snapshot must persist only a session hash"
    );

    // Cookie restores on a fresh tab/reload without a wallet extension and is bound to this record.
    let restored = request(
        port,
        "GET",
        &format!("/agents/{RECORD}/owner/browser/session"),
        None,
        Some(&cookie),
        None,
        None,
    );
    assert_eq!(restored.status, 200, "{:?}", restored.body);
    assert_eq!(restored.body["wallet"], wallet);
    assert_eq!(restored.body["expires_at"], login_body["expires_at"]);
    assert!(restored.body.get("owner_session").is_none());
    let wrong_record = request(
        port,
        "GET",
        &format!("/agents/{OTHER_RECORD}/owner/browser/session"),
        None,
        Some(&cookie),
        None,
        None,
    );
    assert_eq!(wrong_record.status, 401);
    assert!(wrong_record
        .header("set-cookie")
        .unwrap()
        .contains("Max-Age=0"));
    let tampered = request(
        port,
        "GET",
        &format!("/agents/{RECORD}/owner/browser/session"),
        None,
        Some("__Secure-alashi-owner=not-hex"),
        None,
        None,
    );
    assert_eq!(tampered.status, 401);
    assert_eq!(tampered.body["error"], "owner_cookie_invalid");
    assert!(tampered.header("set-cookie").unwrap().contains("Max-Age=0"));

    // Create an isolated active game as the registered harness identity; no LLM or chain.
    let created = request(
        port,
        "POST",
        "/game/new",
        Some(&json!({"entry_fee":1,"phase_duration":120,"lobby_duration":120,"grace_s":0})),
        None,
        None,
        None,
    );
    assert_eq!(created.status, 200, "{:?}", created.body);
    let game_id = created.body["game_id"].as_u64().unwrap();
    let strategy_hash = arena::api::agent_id_of("cookie-fixture", "base strategy only");
    let joined = request(
        port,
        "POST",
        &format!("/game/{game_id}/join"),
        Some(&json!({
            "agent_record_id":RECORD,"recovery_secret":recovery_secret,"name":"CookieAgent",
            "model":"cookie-fixture","strategy_hash":strategy_hash
        })),
        None,
        None,
        None,
    );
    assert_eq!(joined.status, 200, "{:?}", joined.body);

    // CSRF and credential ambiguity fail before charging quota; cookie is owner-only, not a runner token.
    let wish_path = format!("/agents/{RECORD}/owner/wishes");
    let wish_body = json!({"game_id":game_id,"client_wish_id":"wish-1","text":"Please remember this private wish."});
    let bad_origin = request(
        port,
        "POST",
        &wish_path,
        Some(&wish_body),
        Some(&cookie),
        None,
        Some("https://evil.example"),
    );
    assert_eq!(bad_origin.status, 403);
    let ambiguous = request(
        port,
        "POST",
        &wish_path,
        Some(&wish_body),
        Some(&cookie),
        Some(&"a".repeat(64)),
        Some(ORIGIN),
    );
    assert_eq!(ambiguous.status, 400);
    assert_eq!(ambiguous.body["error"], "ambiguous_owner_credentials");
    let no_wishleak_game_token = request(
        port,
        "POST",
        &format!("/game/{game_id}/owner/wishes/claim"),
        Some(&json!({"token":cookie.split('=').nth(1).unwrap(),"after":0,"limit":10})),
        None,
        None,
        None,
    );
    assert_eq!(no_wishleak_game_token.status, 401);

    let accepted = request(
        port,
        "POST",
        &wish_path,
        Some(&wish_body),
        Some(&cookie),
        None,
        Some(ORIGIN),
    );
    assert_eq!(accepted.status, 200, "{:?}", accepted.body);
    assert_eq!(accepted.body["remaining"], 2);
    let retry = request(
        port,
        "POST",
        &wish_path,
        Some(&wish_body),
        Some(&cookie),
        None,
        Some(ORIGIN),
    );
    assert_eq!(retry.status, 200, "{:?}", retry.body);
    assert_eq!(retry.body["wish_id"], accepted.body["wish_id"]);
    assert_eq!(retry.body["remaining"], 2);
    for i in 2..=3 {
        let r = request(
            port,
            "POST",
            &wish_path,
            Some(
                &json!({"game_id":game_id,"client_wish_id":format!("wish-{i}"),"text":format!("private wish {i}")}),
            ),
            Some(&cookie),
            None,
            Some(ORIGIN),
        );
        assert_eq!(r.status, 200, "{:?}", r.body);
    }
    let exhausted = request(
        port,
        "POST",
        &wish_path,
        Some(&json!({"game_id":game_id,"client_wish_id":"wish-4","text":"fourth"})),
        Some(&cookie),
        None,
        Some(ORIGIN),
    );
    assert_eq!(exhausted.status, 429);
    assert_eq!(exhausted.body["error"], "wish_quota_exhausted");
    let wishes = request(
        port,
        "GET",
        &format!("{wish_path}?after=0&limit=10"),
        None,
        Some(&cookie),
        None,
        None,
    );
    assert_eq!(wishes.status, 200, "{:?}", wishes.body);
    assert_eq!(wishes.body["wishes"].as_array().unwrap().len(), 3);
    assert_eq!(wishes.body["wishes"][0]["status"], "received");
    assert!(wishes
        .body
        .to_string()
        .contains("Please remember this private wish."));

    // Persisted cookie and wish ledger survive a server restart/reload of snapshot.
    let restarted_dir = root.join("restarted");
    std::fs::create_dir_all(&restarted_dir).unwrap();
    std::fs::copy(root.join("state.json"), restarted_dir.join("state.json")).unwrap();
    std::fs::copy(root.join("seq"), restarted_dir.join("seq")).ok();
    let restarted =
        new_state_with_files(restarted_dir.join("state.json"), restarted_dir.join("seq"));
    load_snapshot(&restarted).unwrap();
    let restart_server = serve_on(restarted, "127.0.0.1:0", 60).unwrap();
    let after_restart = request(
        restart_server.port(),
        "GET",
        &format!("/agents/{RECORD}/owner/browser/session"),
        None,
        Some(&cookie),
        None,
        None,
    );
    assert_eq!(after_restart.status, 200, "{:?}", after_restart.body);
    let wishes_after_restart = request(
        restart_server.port(),
        "GET",
        &format!("{wish_path}?after=0&limit=10"),
        None,
        Some(&cookie),
        None,
        None,
    );
    assert_eq!(wishes_after_restart.status, 200);
    assert_eq!(
        wishes_after_restart.body["wishes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );

    // A persisted but expired session clears the browser cookie and returns no private state.
    let expired_dir = root.join("expired");
    std::fs::create_dir_all(&expired_dir).unwrap();
    let mut expired_snapshot: Value =
        serde_json::from_slice(&std::fs::read(root.join("state.json")).unwrap()).unwrap();
    let sessions = expired_snapshot["owner_auth"]["sessions"]
        .as_object_mut()
        .unwrap();
    for session in sessions.values_mut() {
        session["expires_at"] = json!(1);
    }
    std::fs::write(
        expired_dir.join("state.json"),
        serde_json::to_vec(&expired_snapshot).unwrap(),
    )
    .unwrap();
    std::fs::copy(root.join("seq"), expired_dir.join("seq")).ok();
    let expired_state =
        new_state_with_files(expired_dir.join("state.json"), expired_dir.join("seq"));
    load_snapshot(&expired_state).unwrap();
    let expired_server = serve_on(expired_state, "127.0.0.1:0", 60).unwrap();
    let expired = request(
        expired_server.port(),
        "GET",
        &format!("/agents/{RECORD}/owner/browser/session"),
        None,
        Some(&cookie),
        None,
        None,
    );
    assert_eq!(expired.status, 401);
    assert_eq!(expired.body["error"], "owner_session_expired");
    assert!(expired.header("set-cookie").unwrap().contains("Max-Age=0"));

    // Explicit sign-out revokes server state and clears the cookie, without resetting game quota.
    let logout = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/browser/logout"),
        Some(&json!({})),
        Some(&cookie),
        None,
        Some(ORIGIN),
    );
    assert_eq!(logout.status, 200, "{:?}", logout.body);
    let cleared = logout.header("set-cookie").expect("logout clears cookie");
    assert!(
        cleared.contains(&format!("Path=/agents/{RECORD}/owner")) && cleared.contains("Max-Age=0")
    );
    let revoked = request(
        port,
        "GET",
        &format!("/agents/{RECORD}/owner/browser/session"),
        None,
        Some(&cookie),
        None,
        None,
    );
    assert_eq!(revoked.status, 401);
    assert!(revoked.header("set-cookie").unwrap().contains("Max-Age=0"));

    let (cookie2, _, _) = browser_login(port, &key);
    let quota_survives_logout = request(
        port,
        "GET",
        &format!("{wish_path}?after=0&limit=10"),
        None,
        Some(&cookie2),
        None,
        None,
    );
    assert_eq!(quota_survives_logout.status, 200);
    assert_eq!(
        quota_survives_logout.body["wishes"]
            .as_array()
            .unwrap()
            .len(),
        3
    );
    let still_exhausted = request(
        port,
        "POST",
        &wish_path,
        Some(
            &json!({"game_id":game_id,"client_wish_id":"wish-after-logout","text":"not refunded"}),
        ),
        Some(&cookie2),
        None,
        Some(ORIGIN),
    );
    assert_eq!(still_exhausted.status, 429);

    // A failed disk write must keep the cookie/session retryable; only a persisted revoke clears it.
    let write_blocker = root.join("state.json.tmp");
    std::fs::create_dir(&write_blocker).unwrap();
    let failed_logout = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/browser/logout"),
        Some(&json!({})),
        Some(&cookie2),
        None,
        Some(ORIGIN),
    );
    assert_eq!(failed_logout.status, 503, "{:?}", failed_logout.body);
    assert_eq!(failed_logout.body["error"], "storage_failed");
    assert!(!failed_logout
        .header("set-cookie")
        .unwrap_or("")
        .contains("Max-Age=0"));
    let still_valid = request(
        port,
        "GET",
        &format!("/agents/{RECORD}/owner/browser/session"),
        None,
        Some(&cookie2),
        None,
        None,
    );
    assert_eq!(still_valid.status, 200, "{:?}", still_valid.body);
    std::fs::remove_dir(&write_blocker).unwrap();
    let retried_logout = request(
        port,
        "POST",
        &format!("/agents/{RECORD}/owner/browser/logout"),
        Some(&json!({})),
        Some(&cookie2),
        None,
        Some(ORIGIN),
    );
    assert_eq!(retried_logout.status, 200, "{:?}", retried_logout.body);
    assert!(retried_logout
        .header("set-cookie")
        .unwrap()
        .contains("Max-Age=0"));
    let persisted = new_state_with_files(root.join("state.json"), root.join("seq"));
    load_snapshot(&persisted).unwrap();
    let persisted_server = serve_on(persisted, "127.0.0.1:0", 60).unwrap();
    let after_persisted_logout = request(
        persisted_server.port(),
        "GET",
        &format!("/agents/{RECORD}/owner/browser/session"),
        None,
        Some(&cookie2),
        None,
        None,
    );
    assert_eq!(after_persisted_logout.status, 401);
}
