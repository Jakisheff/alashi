use super::owner_auth::{origin_ok, owner_session_for};
use crate::api::{
    active_game_for_record, active_game_ids_for_record, game_record_for_token, now, random_hex,
    save_snapshot_locked, sha256_hex, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

const WISHES_PER_GAME: u8 = 3;
const DEFAULT_WISH_MAX_BYTES: usize = 512;
const MAX_WISH_BYTES: usize = 4096;
const LEASE_S: i64 = 30;
const MAX_PAGE: usize = 100;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WishStatus {
    Received,
    Consumed,
    Replied,
    Deferred,
    Declined,
    Expired,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WishLease {
    pub lease_id: String,
    pub expires_at: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wish {
    pub wish_id: String,
    pub seq: u64,
    pub status_seq: u64,
    pub agent_record_id: String,
    pub game_id: u64,
    pub client_wish_id: String,
    pub request_hash: String,
    pub text: String,
    pub status: WishStatus,
    pub admission_remaining: u8,
    pub consumptions: u8,
    pub accepted_at: i64,
    pub status_at: i64,
    pub lease: Option<WishLease>,
    pub reply: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WishState {
    pub next_seq: u64,
    pub wishes: HashMap<String, Wish>,
    pub idempotency: HashMap<String, String>,
    pub accepted_per_game: HashMap<String, u8>,
}

fn err(code: &'static str) -> Value {
    json!({"ok":false,"error":code})
}
fn game_key(record: &str, game: u64) -> String {
    format!("{record}:{game}")
}
fn idem_key(record: &str, game: u64, client: &str) -> String {
    format!("{record}:{game}:{client}")
}
fn max_wish_bytes() -> usize {
    std::env::var("ALASHI_WISH_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|v| (1..=MAX_WISH_BYTES).contains(v))
        .unwrap_or(DEFAULT_WISH_MAX_BYTES)
}
fn status_name(status: &WishStatus) -> &'static str {
    match status {
        WishStatus::Received => "received",
        WishStatus::Consumed => "consumed",
        WishStatus::Replied => "replied",
        WishStatus::Deferred => "deferred",
        WishStatus::Declined => "declined",
        WishStatus::Expired => "expired",
    }
}
fn valid_client_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn valid_text(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_wish_bytes()
        && !value.chars().any(|c| c.is_control())
}
/// `admission_remaining` is immutable evidence for this admission; `remaining`
/// is the current authoritative game allowance and can change after retries.
fn admission_receipt(wish: &Wish, remaining: u8) -> Value {
    json!({"ok":true,"wish_id":wish.wish_id,"seq":wish.seq,"status":"received",
        "accepted_at":wish.accepted_at,"admission_remaining":wish.admission_remaining,
        "remaining":remaining})
}
fn wish_json(wish: &Wish) -> Value {
    json!({"wish_id":wish.wish_id,"seq":wish.seq,"status_seq":wish.status_seq,"game_id":wish.game_id,"text":wish.text,
        "status":status_name(&wish.status),"accepted_at":wish.accepted_at,"status_at":wish.status_at,
        "reply":wish.reply})
}
fn expire_finished(state: &AppState, wishes: &mut WishState, at: i64) -> bool {
    let mut changed = false;
    let ids: Vec<String> = wishes
        .wishes
        .iter()
        .filter_map(|(id, wish)| {
            if matches!(
                wish.status,
                WishStatus::Received | WishStatus::Deferred | WishStatus::Consumed
            ) && !active_game_for_record(state, &wish.agent_record_id, wish.game_id)
            {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect();
    for id in ids {
        wishes.next_seq = wishes.next_seq.saturating_add(1);
        let wish = wishes.wishes.get_mut(&id).expect("selected");
        wish.status = WishStatus::Expired;
        wish.status_at = at;
        wish.status_seq = wishes.next_seq;
        wish.lease = None;
        changed = true;
    }
    changed
}

fn submit_fields(body: &Value) -> Result<(u64, &str, &str), &'static str> {
    if !body.as_object().is_some_and(|o| {
        o.len() == 3
            && o.contains_key("game_id")
            && o.contains_key("client_wish_id")
            && o.contains_key("text")
    }) {
        return Err("bad_wish");
    }
    let game_id = body["game_id"]
        .as_u64()
        .filter(|v| *v > 0)
        .ok_or("bad_wish")?;
    let client_id = body["client_wish_id"].as_str().ok_or("bad_wish")?;
    let text = body["text"].as_str().ok_or("bad_wish")?;
    if !valid_client_id(client_id) || !valid_text(text) {
        return Err("bad_wish");
    }
    Ok((game_id, client_id, text))
}

/// Owner admission is durable and consumes exactly one of the three per-game
/// wishes. A later runner outcome never refunds an accepted wish.
pub fn h_submit_wish(
    state: &AppState,
    record: &str,
    origin: &str,
    raw_session: &str,
    body: &Value,
) -> Value {
    if !origin_ok(origin) {
        return err("owner_origin_forbidden");
    }
    if owner_session_for(state, record, raw_session).is_err() {
        return err("owner_session_invalid");
    }
    let (game_id, client_id, text) = match submit_fields(body) {
        Ok(v) => v,
        Err(code) => return err(code),
    };
    let request_hash = sha256_hex(&format!(
        "wish-v1\n{record}\n{game_id}\n{client_id}\n{text}"
    ));
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if owner_session_for(state, record, raw_session).is_err() {
        return err("owner_session_invalid");
    }
    if !active_game_for_record(state, record, game_id) {
        return err("no_active_game");
    }
    let mut wishes = state.wishes.lock().unwrap_or_else(|e| e.into_inner());
    let before = wishes.clone();
    let ikey = idem_key(record, game_id, client_id);
    if let Some(existing) = wishes
        .idempotency
        .get(&ikey)
        .and_then(|id| wishes.wishes.get(id))
    {
        if existing.request_hash != request_hash {
            return err("idempotency_conflict");
        }
        let remaining = WISHES_PER_GAME.saturating_sub(
            *wishes
                .accepted_per_game
                .get(&game_key(record, game_id))
                .unwrap_or(&0),
        );
        return admission_receipt(existing, remaining);
    }
    // Only a new admission needs an active game. A lost-response retry must
    // retain its durable receipt even after that game has ended.
    if !active_game_for_record(state, record, game_id) {
        return err("no_active_game");
    }
    let gkey = game_key(record, game_id);
    let used = *wishes.accepted_per_game.get(&gkey).unwrap_or(&0);
    if used >= WISHES_PER_GAME {
        return err("wish_quota_exhausted");
    }
    let wish_id = match random_hex() {
        Ok(v) => v,
        Err(_) => return err("entropy_unavailable"),
    };
    wishes.next_seq = wishes.next_seq.saturating_add(1);
    let wish = Wish {
        wish_id: wish_id.clone(),
        seq: wishes.next_seq,
        agent_record_id: record.to_string(),
        game_id,
        client_wish_id: client_id.to_string(),
        request_hash,
        text: text.to_string(),
        status: WishStatus::Received,
        status_seq: wishes.next_seq,
        admission_remaining: WISHES_PER_GAME - used - 1,
        consumptions: 0,
        accepted_at: at,
        status_at: at,
        lease: None,
        reply: None,
    };
    wishes.wishes.insert(wish_id.clone(), wish.clone());
    wishes.idempotency.insert(ikey, wish_id);
    wishes.accepted_per_game.insert(gkey, used + 1);
    drop(wishes);
    if save_snapshot_locked(state).is_err() {
        *state.wishes.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    admission_receipt(&wish, WISHES_PER_GAME - used - 1)
}

pub fn h_owner_wishes_after(
    state: &AppState,
    record: &str,
    raw_session: &str,
    after: u64,
    limit: usize,
) -> Value {
    if owner_session_for(state, record, raw_session).is_err() {
        return err("owner_session_invalid");
    }
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if owner_session_for(state, record, raw_session).is_err() {
        return err("owner_session_invalid");
    }
    let mut wishes = state.wishes.lock().unwrap_or_else(|e| e.into_inner());
    let before = wishes.clone();
    let changed = expire_finished(state, &mut wishes, at);
    let mut rows: Vec<&Wish> = wishes
        .wishes
        .values()
        .filter(|w| w.agent_record_id == record && w.status_seq > after)
        .collect();
    rows.sort_by_key(|w| w.status_seq);
    rows.truncate(limit.clamp(1, MAX_PAGE));
    let next = rows.last().map(|w| w.status_seq).unwrap_or(after);
    let last = wishes
        .wishes
        .values()
        .filter(|w| w.agent_record_id == record)
        .map(|w| w.status_seq)
        .max()
        .unwrap_or(0);
    let remaining_by_game = wishes
        .accepted_per_game
        .iter()
        .filter_map(|(key, used)| {
            key.strip_prefix(&format!("{record}:")).map(|game| {
                (
                    game.to_string(),
                    json!(WISHES_PER_GAME.saturating_sub(*used)),
                )
            })
        })
        .collect::<serde_json::Map<String, Value>>();
    let mut remaining_by_game = remaining_by_game;
    for game_id in active_game_ids_for_record(state, record) {
        remaining_by_game
            .entry(game_id.to_string())
            .or_insert_with(|| json!(WISHES_PER_GAME));
    }
    let response = json!({"ok":true,"wishes":rows.into_iter().map(wish_json).collect::<Vec<_>>(),
        "next_cursor":next,"last_seq":last,"remaining_by_game":remaining_by_game});
    drop(wishes);
    if changed && save_snapshot_locked(state).is_err() {
        *state.wishes.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    response
}

/// A claim lease prevents concurrent harness calls. It is not consumption: the
/// runner marks consumed only immediately before it injects text into a model call.
pub fn h_harness_claim_wishes(
    state: &AppState,
    game_id: u64,
    raw_game_token: &str,
    after: u64,
    limit: usize,
) -> Value {
    let Some(record) = game_record_for_token(state, raw_game_token, game_id) else {
        return err("game_token_invalid");
    };
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if game_record_for_token(state, raw_game_token, game_id).as_deref() != Some(record.as_str()) {
        return err("game_token_invalid");
    }
    let mut wishes = state.wishes.lock().unwrap_or_else(|e| e.into_inner());
    let before = wishes.clone();
    let mut changed = expire_finished(state, &mut wishes, at);
    let mut ids: Vec<String> = wishes
        .wishes
        .values()
        .filter(|w| {
            w.agent_record_id == record
                && w.game_id == game_id
                && w.seq > after
                && matches!(w.status, WishStatus::Received | WishStatus::Deferred)
                && w.lease.as_ref().is_none_or(|lease| lease.expires_at <= at)
        })
        .map(|w| w.wish_id.clone())
        .collect();
    ids.sort_by_key(|id| wishes.wishes[id].seq);
    ids.truncate(limit.clamp(1, MAX_PAGE));
    let leases: Result<Vec<(String, String)>, Value> = ids
        .iter()
        .map(|id| {
            random_hex()
                .map(|lease| (id.clone(), lease))
                .map_err(|_| err("entropy_unavailable"))
        })
        .collect();
    let leases = match leases {
        Ok(v) => v,
        Err(error) => return error,
    };
    let mut rows = Vec::new();
    for (id, lease_id) in leases {
        let wish = wishes.wishes.get_mut(&id).expect("selected");
        let lease = WishLease {
            lease_id: lease_id.clone(),
            expires_at: at.saturating_add(LEASE_S),
        };
        wish.lease = Some(lease.clone());
        changed = true;
        rows.push(
            json!({"wish_id":wish.wish_id,"seq":wish.seq,"text":wish.text,"status":"received",
            "lease_id":lease_id,"lease_expires_at":lease.expires_at}),
        );
    }
    // A lease is reversible. Advancing this cursor before the runner consumes
    // it would strand the wish after crash or lease expiry.
    let next = after;
    let last = wishes
        .wishes
        .values()
        .filter(|w| w.agent_record_id == record && w.game_id == game_id)
        .map(|w| w.seq)
        .max()
        .unwrap_or(0);
    drop(wishes);
    if changed && save_snapshot_locked(state).is_err() {
        *state.wishes.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    json!({"ok":true,"wishes":rows,"next_cursor":next,"last_seq":last})
}

fn update_fields(body: &Value) -> Result<(&str, Option<&str>, Option<&str>), &'static str> {
    let Some(map) = body.as_object() else {
        return Err("bad_wish_status");
    };
    if !(1..=3).contains(&map.len())
        || !map.contains_key("status")
        || map
            .keys()
            .any(|key| !matches!(key.as_str(), "status" | "lease_id" | "reply"))
    {
        return Err("bad_wish_status");
    }
    let status = body["status"].as_str().ok_or("bad_wish_status")?;
    let lease = body.get("lease_id").and_then(Value::as_str);
    let reply = body.get("reply").and_then(Value::as_str);
    if reply.is_some_and(|v| !valid_text(v)) {
        return Err("bad_wish_status");
    }
    Ok((status, lease, reply))
}

pub fn h_harness_update_wish(
    state: &AppState,
    game_id: u64,
    raw_game_token: &str,
    wish_id: &str,
    body: &Value,
) -> Value {
    let Some(record) = game_record_for_token(state, raw_game_token, game_id) else {
        return err("game_token_invalid");
    };
    let (target, lease_id, reply) = match update_fields(body) {
        Ok(v) => v,
        Err(code) => return err(code),
    };
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if game_record_for_token(state, raw_game_token, game_id).as_deref() != Some(record.as_str()) {
        return err("game_token_invalid");
    }
    let mut wishes = state.wishes.lock().unwrap_or_else(|e| e.into_inner());
    let before_expiry = wishes.clone();
    if expire_finished(state, &mut wishes, at) {
        drop(wishes);
        if save_snapshot_locked(state).is_err() {
            *state.wishes.lock().unwrap_or_else(|e| e.into_inner()) = before_expiry;
            return err("storage_failed");
        }
        wishes = state.wishes.lock().unwrap_or_else(|e| e.into_inner());
    }
    let before = wishes.clone();
    let Some(wish) = wishes.wishes.get(wish_id) else {
        return err("unknown_wish");
    };
    if wish.agent_record_id != record || wish.game_id != game_id {
        return err("wish_forbidden");
    }
    let legal = match (&wish.status, target) {
        (
            WishStatus::Received | WishStatus::Deferred,
            "consumed" | "deferred" | "declined" | "expired",
        ) => {
            wish.lease
                .as_ref()
                .is_some_and(|l| lease_id == Some(l.lease_id.as_str()) && at < l.expires_at)
                && (target != "consumed" || wish.consumptions == 0)
        }
        // Once text was injected into a decision, it must never be replayed.
        // A private reply may still arrive later, or the terminal outcome can
        // record that no reply will arrive.
        (WishStatus::Consumed, "replied" | "declined" | "expired") => true,
        _ => false,
    };
    if !legal {
        return err("wish_status_conflict");
    }
    if target == "replied" && reply.is_none() {
        return err("reply_required");
    }
    let status_seq = wishes.next_seq.saturating_add(1);
    wishes.next_seq = status_seq;
    let wish = wishes.wishes.get_mut(wish_id).expect("checked");
    wish.status = match target {
        "consumed" => WishStatus::Consumed,
        "replied" => WishStatus::Replied,
        "deferred" => WishStatus::Deferred,
        "declined" => WishStatus::Declined,
        "expired" => WishStatus::Expired,
        _ => return err("bad_wish_status"),
    };
    wish.status_at = at;
    wish.status_seq = status_seq;
    if target == "consumed" {
        wish.consumptions = wish.consumptions.saturating_add(1);
    }
    wish.lease = None;
    if target == "replied" {
        wish.reply = reply.map(str::to_string);
    }
    let response = json!({"ok":true,"wish_id":wish.wish_id,"seq":wish.seq,"status_seq":wish.status_seq,
        "status":status_name(&wish.status),"status_at":at});
    drop(wishes);
    if save_snapshot_locked(state).is_err() {
        *state.wishes.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_limits_are_not_a_hidden_140_rule() {
        assert!(valid_text(&"x".repeat(DEFAULT_WISH_MAX_BYTES)));
        assert!(!valid_text(&"x".repeat(DEFAULT_WISH_MAX_BYTES + 1)));
        assert!(!valid_text("a\u{0000}b"));
    }
    #[test]
    fn client_id_is_bounded() {
        assert!(valid_client_id("abc-123_X"));
        assert!(!valid_client_id("x y"));
        assert!(!valid_client_id(&"x".repeat(97)));
    }
    #[test]
    fn status_only_decline_is_valid() {
        assert_eq!(
            update_fields(&json!({"status":"declined"})).unwrap().0,
            "declined"
        );
    }
    #[test]
    fn retry_receipt_keeps_admission_evidence_but_reports_current_allowance() {
        let wish = Wish {
            wish_id: "a".repeat(64),
            seq: 1,
            status_seq: 1,
            agent_record_id: "b".repeat(64),
            game_id: 7,
            client_wish_id: "one".into(),
            request_hash: "c".repeat(64),
            text: "hold goods".into(),
            status: WishStatus::Received,
            admission_remaining: 2,
            consumptions: 0,
            accepted_at: 9,
            status_at: 9,
            lease: None,
            reply: None,
        };
        let receipt = admission_receipt(&wish, 0);
        assert_eq!(receipt["admission_remaining"], 2);
        assert_eq!(receipt["remaining"], 0);
        assert_eq!(receipt["status"], "received");
    }
}
