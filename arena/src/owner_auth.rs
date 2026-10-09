use crate::api::{
    chain_wishes, hex32, now, random_hex, save_snapshot_locked, sha256_hex, AppState,
};
use alashi_rules::anchor_lang::prelude::Pubkey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solana_signature::Signature;
use std::{collections::HashMap, str::FromStr};

const CHALLENGE_TTL_S: i64 = 300;
const SESSION_TTL_S: i64 = 1800;
const BROWSER_SESSION_TTL_S: i64 = 7 * 24 * 60 * 60;
const HANDOFF_TTL_S: i64 = 300;
const PAIRING_TTL_S: i64 = 1800;
const BROWSER_COOKIE_NAME: &str = "__Secure-alashi-owner";
const PAIRING_COOKIE_NAME: &str = "__Secure-alashi-pair";
const CHALLENGE_WINDOW_S: i64 = 300;
const MAX_CHALLENGES_PER_WINDOW: usize = 5;
const DEFAULT_OWNER_ORIGIN: &str = "https://alashi.network";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerChallenge {
    pub challenge_id: String,
    pub agent_record_id: String,
    pub wallet: String,
    pub origin: String,
    pub nonce: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub used_at: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerSession {
    pub agent_record_id: String,
    pub wallet: String,
    pub session_hash: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerHandoff {
    pub agent_record_id: String,
    pub wallet: String,
    pub game_pda: String,
    pub faction_pda: String,
    pub code_hash: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub used_at: Option<i64>,
    #[serde(default)]
    pub pairing_grant_hash: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrowserPairing {
    pub grant_hash: String,
    pub browser_hash: String,
    #[serde(default)]
    pub expected_record_id: Option<String>,
    pub issued_at: i64,
    pub expires_at: i64,
    pub agent_record_id: Option<String>,
    pub wallet: Option<String>,
    pub game_pda: Option<String>,
    pub faction_pda: Option<String>,
    pub completed_at: Option<i64>,
    pub redeemed_at: Option<i64>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OwnerAuthState {
    pub challenges: HashMap<String, OwnerChallenge>,
    pub sessions: HashMap<String, OwnerSession>,
    pub challenge_attempts: HashMap<String, Vec<i64>>,
    pub handoffs: HashMap<String, OwnerHandoff>,
    pub pairings: HashMap<String, BrowserPairing>,
}

fn err(code: &'static str) -> Value {
    json!({"ok": false, "error": code})
}

fn configured_origin() -> String {
    std::env::var("ALASHI_OWNER_ORIGIN")
        .ok()
        .filter(|v| v.starts_with("https://") && v.len() <= 200)
        .unwrap_or_else(|| DEFAULT_OWNER_ORIGIN.to_string())
}
pub(super) fn origin_ok(origin: &str) -> bool {
    origin == configured_origin()
}

/// These exact UTF-8 bytes are the wallet signature boundary, not JSON.
pub fn challenge_message(c: &OwnerChallenge) -> String {
    format!(
        "alashi-owner-auth-v1\norigin:{}\nagent_record_id:{}\nwallet:{}\nnonce:{}\nissued_at:{}\nexpires_at:{}\n",
        c.origin, c.agent_record_id, c.wallet, c.nonce, c.issued_at, c.expires_at,
    )
}
fn session_hash(raw: &str) -> String {
    sha256_hex(&format!("alashi-owner-session-v1:{raw}"))
}
fn handoff_hash(raw: &str) -> String {
    sha256_hex(&format!("alashi-owner-handoff-v1:{raw}"))
}
fn pairing_grant_hash(raw: &str) -> String {
    sha256_hex(&format!("alashi-browser-pair-grant-v1:{raw}"))
}
fn pairing_browser_hash(raw: &str) -> String {
    sha256_hex(&format!("alashi-browser-pair-cookie-v1:{raw}"))
}
fn pairing_cookie(raw: &str) -> String {
    format!("{PAIRING_COOKIE_NAME}={raw}; Path=/owner/pairing; Max-Age={PAIRING_TTL_S}; Secure; HttpOnly; SameSite=Strict")
}
pub fn pairing_browser_token(cookie_header: Option<&str>) -> Result<String, &'static str> {
    let mut found = None;
    for part in cookie_header.unwrap_or("").split(';') {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        if name != PAIRING_COOKIE_NAME {
            continue;
        }
        if found.is_some() || !hex32(value) {
            return Err("pairing_cookie_invalid");
        }
        found = Some(value.to_string());
    }
    found.ok_or("pairing_cookie_invalid")
}

/// Browser starts the flow before an agent/record is known. Only the matching
/// HttpOnly pairing cookie can later exchange a completed grant for an owner
/// cookie; the grant in the copied prompt cannot redeem by itself.
pub fn h_start_browser_pairing(
    state: &AppState,
    origin: &str,
    cookie: Option<&str>,
    body: &Value,
) -> (Value, Option<String>) {
    if !origin_ok(origin) {
        return (err("owner_origin_forbidden"), None);
    }
    if !body.as_object().is_some_and(|obj| obj.is_empty()
        || (obj.len() == 1 && obj.get("expected_record_id")
            .and_then(Value::as_str).is_some_and(hex32))) {
        return (err("bad_pairing"), None);
    }
    let expected_record_id = body["expected_record_id"].as_str().map(str::to_string);
    let grant = match random_hex() {
        Ok(v) => v,
        Err(_) => return (err("entropy_unavailable"), None),
    };
    let browser = match random_hex() {
        Ok(v) => v,
        Err(_) => return (err("entropy_unavailable"), None),
    };
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
    let before = auth.clone();
    auth.pairings.retain(|_, pair| pair.expires_at > at);
    if let Ok(old) = pairing_browser_token(cookie) {
        let old_hash = pairing_browser_hash(&old);
        auth.pairings
            .retain(|_, pair| pair.browser_hash != old_hash);
    }
    if auth.pairings.len() >= 1000 {
        return (err("pairing_busy"), None);
    }
    let hash = pairing_grant_hash(&grant);
    auth.pairings.insert(
        hash.clone(),
        BrowserPairing {
            grant_hash: hash,
            browser_hash: pairing_browser_hash(&browser),
            expected_record_id,
            issued_at: at,
            expires_at: at.saturating_add(PAIRING_TTL_S),
            agent_record_id: None,
            wallet: None,
            game_pda: None,
            faction_pda: None,
            completed_at: None,
            redeemed_at: None,
        },
    );
    drop(auth);
    if save_snapshot_locked(state).is_err() {
        *state.owner_auth.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return (err("storage_failed"), None);
    }
    (
        json!({"ok":true,"pairing_grant":grant,"expires_at":at.saturating_add(PAIRING_TTL_S)}),
        Some(pairing_cookie(&browser)),
    )
}

/// Agent completion requires the existing registered-wallet owner bearer and
/// a live exact chain faction binding. It also mints a separate one-use link
/// for the case where the original browser cannot stay open.
pub fn h_complete_browser_pairing(
    state: &AppState,
    record_id: &str,
    origin: &str,
    bearer: &str,
    body: &Value,
) -> Value {
    if !origin_ok(origin) {
        return err("owner_origin_forbidden");
    }
    if !body.as_object().is_some_and(|obj| {
        obj.len() == 3
            && obj.contains_key("pairing_grant")
            && obj.contains_key("game_pda")
            && obj.contains_key("faction_pda")
    }) {
        return err("bad_pairing");
    }
    let (Some(grant), Some(game), Some(faction)) = (
        body["pairing_grant"].as_str(),
        body["game_pda"].as_str(),
        body["faction_pda"].as_str(),
    ) else {
        return err("bad_pairing");
    };
    if !hex32(grant) || !canonical_pda(game) || !canonical_pda(faction) {
        return err("bad_pairing");
    }
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let owner = match owner_session_for(state, record_id, bearer) {
        Ok(owner) => owner,
        Err(code) => return err(code),
    };
    if !chain_wishes::owner_handoff_binding(state, record_id, game, faction, &owner.wallet) {
        return err("binding_forbidden");
    }
    let code = match random_hex() {
        Ok(code) => code,
        Err(_) => return err("entropy_unavailable"),
    };
    let grant_hash = pairing_grant_hash(grant);
    let mut auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
    let before = auth.clone();
    auth.handoffs.retain(|_, old| old.expires_at > at && old.used_at.is_none());
    if auth.handoffs.len() >= 1000 {
        *auth = before;
        return err("owner_handoff_busy");
    }
    let Some(pair) = auth.pairings.get_mut(&grant_hash) else {
        return err("pairing_invalid");
    };
    if pair.completed_at.is_some() || pair.redeemed_at.is_some() || at >= pair.expires_at {
        return err("pairing_invalid");
    }
    if pair.expected_record_id.as_deref().is_some_and(|expected| expected != record_id) {
        return err("pairing_identity_mismatch");
    }
    pair.agent_record_id = Some(record_id.to_string());
    pair.wallet = Some(owner.wallet.clone());
    pair.game_pda = Some(game.to_string());
    pair.faction_pda = Some(faction.to_string());
    pair.completed_at = Some(at);
    let code_hash = handoff_hash(&code);
    auth.handoffs.insert(
        code_hash.clone(),
        OwnerHandoff {
            agent_record_id: record_id.to_string(),
            wallet: owner.wallet,
            game_pda: game.to_string(),
            faction_pda: faction.to_string(),
            code_hash,
            issued_at: at,
            expires_at: at.saturating_add(HANDOFF_TTL_S),
            used_at: None,
            pairing_grant_hash: Some(grant_hash),
        },
    );
    drop(auth);
    if save_snapshot_locked(state).is_err() {
        *state.owner_auth.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    let owner_url = format!(
        "{}/devnet?game={game}&player={faction}#owner={record_id}.{code}",
        configured_origin()
    );
    json!({"ok":true,"owner_url":owner_url,"agent_record_id":record_id,
        "game_pda":game,"faction_pda":faction,"expires_at":at.saturating_add(HANDOFF_TTL_S)})
}

pub fn h_poll_browser_pairing(
    state: &AppState,
    origin: &str,
    cookie: Option<&str>,
    body: &Value,
) -> (Value, Option<String>) {
    if !origin_ok(origin) {
        return (err("owner_origin_forbidden"), None);
    }
    if !body.as_object().is_some_and(|obj| obj.is_empty()) {
        return (err("bad_pairing"), None);
    }
    let browser = match pairing_browser_token(cookie) {
        Ok(browser) => browser,
        Err(code) => return (err(code), None),
    };
    let browser_hash = pairing_browser_hash(&browser);
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let pair = {
        let auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
        let Some(pair) = auth
            .pairings
            .values()
            .find(|p| p.browser_hash == browser_hash)
        else {
            return (err("pairing_invalid"), None);
        };
        pair.clone()
    };
    if at >= pair.expires_at {
        return (err("pairing_invalid"), None);
    }
    if pair.completed_at.is_none() {
        return (
            json!({"ok":true,"status":"waiting","expires_at":pair.expires_at}),
            None,
        );
    }
    let (Some(record), Some(wallet), Some(game), Some(faction)) = (
        pair.agent_record_id.as_deref(),
        pair.wallet.as_deref(),
        pair.game_pda.as_deref(),
        pair.faction_pda.as_deref(),
    ) else {
        return (err("pairing_invalid"), None);
    };
    if owner_wallet(state, record).ok().as_deref() != Some(wallet)
        || !chain_wishes::owner_handoff_binding(state, record, game, faction, wallet)
    {
        return (err("binding_forbidden"), None);
    }
    // Deterministic from the random HttpOnly browser nonce: a lost Set-Cookie
    // response can be retried by this browser, without persisting any raw
    // owner token. A later explicit logout revokes this exact session.
    let raw_session = sha256_hex(&format!(
        "alashi-browser-pair-session-v1:{browser}:{}:{record}:{game}",
        pair.grant_hash
    ));
    let mut auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
    let before = auth.clone();
    let Some(stored) = auth.pairings.get_mut(&pair.grant_hash) else {
        return (err("pairing_invalid"), None);
    };
    let hash = session_hash(&raw_session);
    let already_redeemed = stored.redeemed_at.is_some();
    if at >= stored.expires_at {
        return (err("pairing_invalid"), None);
    }
    if !already_redeemed {
        stored.redeemed_at = Some(at);
    }
    if already_redeemed {
        if !auth.sessions.get(&hash).is_some_and(|session| {
            session.revoked_at.is_none()
                && session.agent_record_id == record
                && session.wallet == wallet
                && at < session.expires_at
        }) {
            return (err("pairing_invalid"), None);
        }
    } else {
        for link in auth.handoffs.values_mut() {
            if link.pairing_grant_hash.as_deref() == Some(&pair.grant_hash) {
                link.used_at = Some(at);
            }
        }
        auth.sessions.insert(
            hash.clone(),
            OwnerSession {
                agent_record_id: record.to_string(),
                wallet: wallet.to_string(),
                session_hash: hash,
                issued_at: at,
                expires_at: at.saturating_add(BROWSER_SESSION_TTL_S),
                revoked_at: None,
            },
        );
    }
    drop(auth);
    if !already_redeemed && save_snapshot_locked(state).is_err() {
        *state.owner_auth.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return (err("storage_failed"), None);
    }
    let expires_at = if already_redeemed {
        state
            .owner_auth
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .sessions
            .get(&session_hash(&raw_session))
            .map(|s| s.expires_at)
            .unwrap_or(at)
    } else {
        at.saturating_add(BROWSER_SESSION_TTL_S)
    };
    (
        json!({"ok":true,"status":"paired","agent_record_id":record,
        "game_pda":game,"faction_pda":faction,"wallet":wallet,
        "expires_at":expires_at}),
        Some(browser_cookie(record, &raw_session, BROWSER_SESSION_TTL_S)),
    )
}
fn canonical_pda(raw: &str) -> bool {
    Pubkey::from_str(raw)
        .ok()
        .is_some_and(|key| key.to_string() == raw)
}

/// A wallet-proved local runner may return one short-lived owner link for its
/// exact registered record and joined faction. The raw code is never saved.
pub fn h_issue_owner_handoff(
    state: &AppState,
    record_id: &str,
    origin: &str,
    bearer: &str,
    body: &Value,
) -> Value {
    if !origin_ok(origin) {
        return err("owner_origin_forbidden");
    }
    if !body.as_object().is_some_and(|obj| {
        obj.len() == 2 && obj.contains_key("game_pda") && obj.contains_key("faction_pda")
    }) {
        return err("bad_owner_handoff");
    }
    let (Some(game), Some(faction)) = (body["game_pda"].as_str(), body["faction_pda"].as_str())
    else {
        return err("bad_owner_handoff");
    };
    if !canonical_pda(game) || !canonical_pda(faction) {
        return err("bad_owner_handoff");
    }
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let owner = match owner_session_for(state, record_id, bearer) {
        Ok(owner) => owner,
        Err(code) => return err(code),
    };
    if !chain_wishes::owner_handoff_binding(state, record_id, game, faction, &owner.wallet) {
        return err("binding_forbidden");
    }
    let code = match random_hex() {
        Ok(code) => code,
        Err(_) => return err("entropy_unavailable"),
    };
    let at = now();
    let hash = handoff_hash(&code);
    let handoff = OwnerHandoff {
        agent_record_id: record_id.to_string(),
        wallet: owner.wallet,
        game_pda: game.to_string(),
        faction_pda: faction.to_string(),
        code_hash: hash.clone(),
        issued_at: at,
        expires_at: at.saturating_add(HANDOFF_TTL_S),
        used_at: None,
        pairing_grant_hash: None,
    };
    let mut auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
    let before = auth.clone();
    auth.handoffs.retain(|_, old| {
        old.expires_at > at
            && !(old.agent_record_id == record_id && old.game_pda == game && old.used_at.is_none())
    });
    if auth.handoffs.len() >= 1000 {
        return err("owner_handoff_busy");
    }
    auth.handoffs.insert(hash, handoff);
    drop(auth);
    if save_snapshot_locked(state).is_err() {
        *state.owner_auth.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    let owner_url = format!(
        "{}/devnet?game={game}&player={faction}#owner={record_id}.{code}",
        configured_origin()
    );
    json!({"ok":true,"owner_url":owner_url,"expires_at":at.saturating_add(HANDOFF_TTL_S),
        "game_pda":game,"faction_pda":faction,"agent_record_id":record_id})
}

/// The browser sends the fragment code in a POST body, then removes it from
/// history. Successful redemption atomically consumes it and mints the same
/// record-scoped Secure HttpOnly cookie used by wallet browser login.
pub fn h_redeem_owner_handoff(
    state: &AppState,
    record_id: &str,
    origin: &str,
    body: &Value,
) -> (Value, Option<String>) {
    if !origin_ok(origin) {
        return (err("owner_origin_forbidden"), None);
    }
    if !body.as_object().is_some_and(|obj| {
        obj.len() == 2 && obj.contains_key("game_pda") && obj.contains_key("code")
    }) {
        return (err("bad_owner_handoff"), None);
    }
    let (Some(game), Some(code)) = (body["game_pda"].as_str(), body["code"].as_str()) else {
        return (err("bad_owner_handoff"), None);
    };
    if !canonical_pda(game) || !hex32(code) {
        return (err("bad_owner_handoff"), None);
    }
    let hash = handoff_hash(code);
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let wallet = match owner_wallet(state, record_id) {
        Ok(wallet) => wallet,
        Err(code) => return (err(code), None),
    };
    let mut auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
    let Some(grant) = auth.handoffs.get(&hash).cloned() else {
        return (err("owner_handoff_invalid"), None);
    };
    if grant.agent_record_id != record_id
        || grant.game_pda != game
        || grant.wallet != wallet
        || grant.used_at.is_some()
        || at >= grant.expires_at
    {
        return (err("owner_handoff_invalid"), None);
    }
    if grant.pairing_grant_hash.as_ref().is_some_and(|pair_hash| {
        auth.pairings
            .get(pair_hash)
            .is_none_or(|pair| pair.redeemed_at.is_some())
    }) {
        return (err("owner_handoff_invalid"), None);
    }
    drop(auth);
    if !chain_wishes::owner_handoff_binding(state, record_id, game, &grant.faction_pda, &wallet) {
        return (err("binding_forbidden"), None);
    }
    let raw_session = match random_hex() {
        Ok(raw) => raw,
        Err(_) => return (err("entropy_unavailable"), None),
    };
    auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
    let before = auth.clone();
    let Some(stored) = auth.handoffs.get_mut(&hash) else {
        return (err("owner_handoff_invalid"), None);
    };
    if stored.used_at.is_some() || at >= stored.expires_at {
        return (err("owner_handoff_invalid"), None);
    }
    stored.used_at = Some(at);
    if let Some(pair_hash) = &grant.pairing_grant_hash {
        if let Some(pair) = auth.pairings.get_mut(pair_hash) {
            pair.redeemed_at = Some(at);
        }
    }
    let session_hash = session_hash(&raw_session);
    auth.sessions.insert(
        session_hash.clone(),
        OwnerSession {
            agent_record_id: record_id.to_string(),
            wallet: wallet.clone(),
            session_hash,
            issued_at: at,
            expires_at: at.saturating_add(BROWSER_SESSION_TTL_S),
            revoked_at: None,
        },
    );
    drop(auth);
    if save_snapshot_locked(state).is_err() {
        *state.owner_auth.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return (err("storage_failed"), None);
    }
    (
        json!({"ok":true,"agent_record_id":record_id,"game_pda":game,
        "faction_pda":grant.faction_pda,"wallet":wallet,
        "expires_at":at.saturating_add(BROWSER_SESSION_TTL_S)}),
        Some(browser_cookie(
            record_id,
            &raw_session,
            BROWSER_SESSION_TTL_S,
        )),
    )
}

fn owner_wallet(state: &AppState, record_id: &str) -> Result<String, &'static str> {
    if !hex32(record_id) {
        return Err("bad_agent_record_id");
    }
    let records = state
        .registrations
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let record = records.get(record_id).ok_or("unknown_agent")?;
    if record.receipt.is_none() {
        return Err("registration_required");
    }
    Ok(record.wallet.clone())
}
fn prune_attempts(auth: &mut OwnerAuthState, record_id: &str, at: i64) -> usize {
    let attempts = auth
        .challenge_attempts
        .entry(record_id.to_string())
        .or_default();
    attempts.retain(|created| *created > at.saturating_sub(CHALLENGE_WINDOW_S));
    attempts.len()
}

pub fn h_begin_owner_challenge(state: &AppState, record_id: &str, origin: &str) -> Value {
    if !origin_ok(origin) {
        return err("owner_origin_forbidden");
    }
    let issued_at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // Keep the issued challenge tied to the registration visible in the same
    // persistence transaction that stores it.
    let wallet = match owner_wallet(state, record_id) {
        Ok(v) => v,
        Err(code) => return err(code),
    };
    let mut auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
    if prune_attempts(&mut auth, record_id, issued_at) >= MAX_CHALLENGES_PER_WINDOW {
        return err("owner_challenge_rate_limited");
    }
    let challenge_id = match random_hex() {
        Ok(v) => v,
        Err(_) => return err("entropy_unavailable"),
    };
    let nonce = match random_hex() {
        Ok(v) => v,
        Err(_) => return err("entropy_unavailable"),
    };
    let c = OwnerChallenge {
        challenge_id: challenge_id.clone(),
        agent_record_id: record_id.to_string(),
        wallet: wallet.clone(),
        origin: origin.to_string(),
        nonce,
        issued_at,
        expires_at: issued_at.saturating_add(CHALLENGE_TTL_S),
        used_at: None,
    };
    let before = auth.clone();
    auth.challenges
        .retain(|_, v| v.expires_at > issued_at && v.agent_record_id != record_id);
    auth.challenges.insert(challenge_id.clone(), c.clone());
    auth.challenge_attempts
        .entry(record_id.to_string())
        .or_default()
        .push(issued_at);
    drop(auth);
    if save_snapshot_locked(state).is_err() {
        *state.owner_auth.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    json!({"ok":true,"challenge_id":challenge_id,"wallet":wallet,"message":challenge_message(&c),
        "issued_at":c.issued_at,"expires_at":c.expires_at})
}

fn parse_finish(body: &Value) -> Result<(&str, &str), &'static str> {
    if !body.as_object().is_some_and(|o| {
        o.len() == 2 && o.contains_key("challenge_id") && o.contains_key("signature")
    }) {
        return Err("bad_owner_challenge_proof");
    }
    let id = body["challenge_id"]
        .as_str()
        .ok_or("bad_owner_challenge_proof")?;
    let signature = body["signature"]
        .as_str()
        .ok_or("bad_owner_challenge_proof")?;
    if !hex32(id) || !(64..=88).contains(&signature.len()) {
        return Err("bad_owner_challenge_proof");
    }
    Ok((id, signature))
}
fn verify_wallet_signature(wallet: &str, message: &str, encoded: &str) -> bool {
    let Ok(key) = Pubkey::from_str(wallet) else {
        return false;
    };
    let Ok(signature) = Signature::from_str(encoded) else {
        return false;
    };
    signature.verify(key.as_ref(), message.as_bytes())
}

/// Ed25519 verification occurs before taking the persistence transaction gate.
fn finish_owner_challenge(
    state: &AppState,
    record_id: &str,
    origin: &str,
    body: &Value,
    ttl_s: i64,
) -> Value {
    if !origin_ok(origin) {
        return err("owner_origin_forbidden");
    }
    let (challenge_id, signature) = match parse_finish(body) {
        Ok(v) => v,
        Err(code) => return err(code),
    };
    let candidate = {
        let auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
        let Some(c) = auth.challenges.get(challenge_id) else {
            return err("owner_challenge_unknown");
        };
        if c.agent_record_id != record_id || c.origin != origin || c.used_at.is_some() {
            return err("owner_challenge_invalid");
        }
        if now() >= c.expires_at {
            return err("owner_challenge_expired");
        }
        c.clone()
    };
    if !verify_wallet_signature(&candidate.wallet, &challenge_message(&candidate), signature) {
        return err("owner_signature_invalid");
    }
    let raw_session = match random_hex() {
        Ok(v) => v,
        Err(_) => return err("entropy_unavailable"),
    };
    let issued_at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let wallet = match owner_wallet(state, record_id) {
        Ok(v) => v,
        Err(code) => return err(code),
    };
    let mut auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
    let before = auth.clone();
    let Some(c) = auth.challenges.get_mut(challenge_id) else {
        return err("owner_challenge_unknown");
    };
    if c.agent_record_id != record_id
        || c.wallet != wallet
        || c.origin != origin
        || c.used_at.is_some()
    {
        return err("owner_challenge_invalid");
    }
    if issued_at >= c.expires_at {
        return err("owner_challenge_expired");
    }
    c.used_at = Some(issued_at);
    auth.sessions
        .retain(|_, session| session.revoked_at.is_none() && session.expires_at > issued_at);
    let hash = session_hash(&raw_session);
    let session = OwnerSession {
        agent_record_id: record_id.to_string(),
        wallet,
        session_hash: hash.clone(),
        issued_at,
        expires_at: issued_at.saturating_add(ttl_s),
        revoked_at: None,
    };
    auth.sessions.insert(hash, session.clone());
    drop(auth);
    if save_snapshot_locked(state).is_err() {
        *state.owner_auth.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    json!({"ok":true,"owner_session":raw_session,"expires_at":session.expires_at,"scope":["owner_wishes"]})
}

pub fn h_finish_owner_challenge(
    state: &AppState,
    record_id: &str,
    origin: &str,
    body: &Value,
) -> Value {
    finish_owner_challenge(state, record_id, origin, body, SESSION_TTL_S)
}

pub fn h_finish_owner_browser_challenge(
    state: &AppState,
    record_id: &str,
    origin: &str,
    body: &Value,
) -> (Value, Option<String>) {
    let mut result = finish_owner_challenge(state, record_id, origin, body, BROWSER_SESSION_TTL_S);
    let raw = result
        .as_object_mut()
        .and_then(|o| o.remove("owner_session"));
    let cookie = raw.and_then(|v| {
        v.as_str()
            .map(|s| browser_cookie(record_id, s, BROWSER_SESSION_TTL_S))
    });
    (result, cookie)
}

pub fn record_id_ok(record_id: &str) -> bool {
    hex32(record_id)
}

pub fn browser_cookie(record_id: &str, raw: &str, max_age: i64) -> String {
    debug_assert!(hex32(record_id));
    debug_assert!(raw.is_empty() || hex32(raw));
    format!("{BROWSER_COOKIE_NAME}={raw}; Path=/agents/{record_id}/owner; Max-Age={max_age}; Secure; HttpOnly; SameSite=Strict")
}

pub fn has_browser_cookie(cookie_header: Option<&str>) -> bool {
    cookie_header.unwrap_or("").split(';').any(|part| {
        part.trim()
            .split_once('=')
            .is_some_and(|(name, _)| name == BROWSER_COOKIE_NAME)
    })
}

/// Parse only our record-scoped cookie; all other site cookies are ignored.
pub fn browser_token(cookie_header: Option<&str>) -> Result<Option<String>, &'static str> {
    let mut found = None;
    for part in cookie_header.unwrap_or("").split(';') {
        let Some((name, value)) = part.trim().split_once('=') else {
            continue;
        };
        if name != BROWSER_COOKIE_NAME {
            continue;
        }
        if found.is_some() || !hex32(value) {
            return Err("owner_cookie_invalid");
        }
        found = Some(value.to_string());
    }
    Ok(found)
}

pub fn h_restore_owner_browser_session(state: &AppState, record_id: &str, raw: &str) -> Value {
    match owner_session_for(state, record_id, raw) {
        Ok(session) => json!({"ok":true,"wallet":session.wallet,"expires_at":session.expires_at}),
        Err(code) => err(code),
    }
}

pub fn owner_session_for(
    state: &AppState,
    record_id: &str,
    raw_session: &str,
) -> Result<OwnerSession, &'static str> {
    if !hex32(record_id) || !hex32(raw_session) {
        return Err("owner_session_invalid");
    }
    let hash = session_hash(raw_session);
    let session = {
        let auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
        let session = auth.sessions.get(&hash).ok_or("owner_session_invalid")?;
        if session.agent_record_id != record_id
            || session.revoked_at.is_some()
            || now() >= session.expires_at
        {
            return Err("owner_session_expired");
        }
        session.clone()
    };
    // The browser session is bound to the currently confirmed registration, not
    // merely to the public record id it was originally issued for.
    let registrations = state
        .registrations
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if !registrations
        .get(record_id)
        .is_some_and(|entry| entry.receipt.is_some() && entry.wallet == session.wallet)
    {
        return Err("owner_session_invalid");
    }
    Ok(session)
}

pub fn h_revoke_owner_session(
    state: &AppState,
    record_id: &str,
    origin: &str,
    raw_session: &str,
) -> Value {
    if !origin_ok(origin) {
        return err("owner_origin_forbidden");
    }
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let hash = session_hash(raw_session);
    let mut auth = state.owner_auth.lock().unwrap_or_else(|e| e.into_inner());
    let Some(session) = auth.sessions.get(&hash) else {
        return err("owner_session_invalid");
    };
    if session.agent_record_id != record_id
        || session.revoked_at.is_some()
        || at >= session.expires_at
    {
        return err("owner_session_expired");
    }
    let before = auth.clone();
    auth.sessions.get_mut(&hash).expect("checked").revoked_at = Some(at);
    drop(auth);
    if save_snapshot_locked(state).is_err() {
        *state.owner_auth.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    json!({"ok":true,"revoked_at":at})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_cookie_is_record_scoped_and_unambiguous() {
        let id = "a".repeat(64);
        let token = "b".repeat(64);
        let header = browser_cookie(&id, &token, BROWSER_SESSION_TTL_S);
        assert!(header.contains(&format!("Path=/agents/{id}/owner")));
        assert!(header.contains("Max-Age=604800; Secure; HttpOnly; SameSite=Strict"));
        assert!(!header.contains("Domain="));
        assert_eq!(
            browser_token(Some(&format!("other=1; {BROWSER_COOKIE_NAME}={token}"))).unwrap(),
            Some(token.clone())
        );
        assert_eq!(
            browser_token(Some(&format!(
                "{BROWSER_COOKIE_NAME}={token}; {BROWSER_COOKIE_NAME}={token}"
            ))),
            Err("owner_cookie_invalid")
        );
        assert_eq!(
            browser_token(Some(&format!("{BROWSER_COOKIE_NAME}=bad"))),
            Err("owner_cookie_invalid")
        );
    }

    #[test]
    fn canonical_message_binds_origin() {
        let c = OwnerChallenge {
            challenge_id: "a".repeat(64),
            agent_record_id: "b".repeat(64),
            wallet: "wallet".into(),
            origin: DEFAULT_OWNER_ORIGIN.into(),
            nonce: "c".repeat(64),
            issued_at: 10,
            expires_at: 20,
            used_at: None,
        };
        assert_eq!(challenge_message(&c), format!(
            "alashi-owner-auth-v1\norigin:https://alashi.network\nagent_record_id:{}\nwallet:wallet\nnonce:{}\nissued_at:10\nexpires_at:20\n",
            "b".repeat(64), "c".repeat(64)));
        assert!(origin_ok(DEFAULT_OWNER_ORIGIN));
        assert!(!origin_ok("https://alashi.network.attacker.example"));
    }
}
