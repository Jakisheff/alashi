use crate::api::{hex32, now, random_hex, save_snapshot_locked, sha256_hex, AppState};
use alashi_rules::anchor_lang::prelude::Pubkey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solana_signature::Signature;
use std::{collections::HashMap, str::FromStr};

const CHALLENGE_TTL_S: i64 = 300;
const SESSION_TTL_S: i64 = 1800;
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

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OwnerAuthState {
    pub challenges: HashMap<String, OwnerChallenge>,
    pub sessions: HashMap<String, OwnerSession>,
    pub challenge_attempts: HashMap<String, Vec<i64>>,
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
pub fn h_finish_owner_challenge(
    state: &AppState,
    record_id: &str,
    origin: &str,
    body: &Value,
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
    let hash = session_hash(&raw_session);
    let session = OwnerSession {
        agent_record_id: record_id.to_string(),
        wallet,
        session_hash: hash.clone(),
        issued_at,
        expires_at: issued_at.saturating_add(SESSION_TTL_S),
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
