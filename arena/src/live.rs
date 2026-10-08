use super::{
    agent_token_matches, err_json, hex32, iso_utc, now, random_hex, recovery_hash,
    save_snapshot_locked, secret_matches, sha256_hex, AppState, GameEntry,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

const SESSION_S: i64 = 86_400;
const PRESENCE_S: i64 = 40;
const RETAIN_S: i64 = 7 * 86_400;
const LEDGER_S: i64 = 30 * 86_400;
const MAX_TEXT_BYTES: usize = 1024;
const MAX_TEXT_CHARS: usize = 240;
const MAX_LIMIT: usize = 100;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LiveState {
    pub next_seq: u64,
    pub events: Vec<Value>,
    pub sessions: HashMap<String, LiveSession>,
    pub memberships: HashMap<String, Vec<u64>>,
    pub receipts: HashMap<String, MessageReceipt>,
    pub pruned_room: HashMap<String, u64>,
    pub pruned_personal: HashMap<String, u64>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LiveSession {
    token_hash: String,
    expires_at: i64,
    lease_until: i64,
    ambient_replies_enabled: bool,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct MessageReceipt {
    request_hash: String,
    response: Value,
    created_at: i64,
}

impl LiveState {
    fn append(&mut self, mut event: Value, ts: i64) -> Value {
        self.next_seq += 1;
        let seq = self.next_seq;
        event["seq"] = json!(seq);
        event["event_id"] = json!(format!("live-{seq}"));
        event["server_created_at"] = json!(iso_utc(ts.max(0) as u64));
        self.events.push(event.clone());
        event
    }

    fn prune(&mut self, ts: i64, active_games: &HashSet<u64>) {
        self.receipts.retain(|_, r| r.created_at > ts - LEDGER_S);
        self.events.retain(|event| {
            let age = event["created_at_epoch"].as_i64().unwrap_or(ts);
            let active = event["game_id"]
                .as_u64()
                .is_some_and(|id| active_games.contains(&id));
            if age > ts - RETAIN_S || active {
                return true;
            }
            let seq = event["seq"].as_u64().unwrap_or(0);
            if let Some(room) = event["room_id"].as_str() {
                self.pruned_room.insert(room.to_string(), seq);
            }
            if let Some(to) = event["to_agent_record_id"].as_str() {
                self.pruned_personal.insert(to.to_string(), seq);
            }
            if let Some(from) = event["author_agent_record_id"].as_str() {
                self.pruned_personal.insert(from.to_string(), seq);
            }
            false
        });
    }
}

fn response_error(code: &str) -> Value {
    err_json(code, code)
}
fn active_games(state: &AppState) -> HashSet<u64> {
    state
        .games
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .keys()
        .copied()
        .collect()
}
fn valid_token(state: &AppState, record_id: &str, token: &str) -> bool {
    if !hex32(token) {
        return false;
    }
    let live = state.live.lock().unwrap_or_else(|e| e.into_inner());
    live.sessions
        .get(record_id)
        .is_some_and(|s| now() < s.expires_at && secret_matches(&s.token_hash, &sha256_hex(token)))
}

pub(super) fn session(state: &AppState, record_id: &str, body: &Value) -> Value {
    let secret = body["recovery_secret"].as_str().unwrap_or("");
    if !hex32(record_id)
        || !hex32(secret)
        || body
            .get("ambient_replies_enabled")
            .is_some_and(|v| !v.is_boolean())
    {
        return response_error("bad_credentials");
    }
    let Some(secret_hash) = recovery_hash(secret) else {
        return response_error("bad_credentials");
    };
    let token = match random_hex() {
        Ok(v) => v,
        Err(_) => return response_error("entropy_unavailable"),
    };
    let _gate = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let records = state
        .registrations
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let good = records
        .get(record_id)
        .is_some_and(|r| r.receipt.is_some() && secret_matches(&r.recovery_hash, &secret_hash));
    drop(records);
    if !good {
        return response_error("bad_credentials");
    }
    let t = now();
    let mut live = state.live.lock().unwrap_or_else(|e| e.into_inner());
    let backup = live.clone();
    live.sessions.insert(
        record_id.to_string(),
        LiveSession {
            token_hash: sha256_hex(&token),
            expires_at: t + SESSION_S,
            lease_until: 0,
            ambient_replies_enabled: body["ambient_replies_enabled"] == true,
        },
    );
    drop(live);
    if save_snapshot_locked(state).is_err() {
        *state.live.lock().unwrap_or_else(|e| e.into_inner()) = backup;
        return response_error("storage_failed");
    }
    json!({"ok":true,"live_token":token,"stream_id":record_id,"scope":["presence","ambient_write"],"expires_at":t+SESSION_S,"server_now":t})
}

pub(super) fn revoke(state: &AppState, record_id: &str, body: &Value) -> Value {
    let secret = body["recovery_secret"].as_str().unwrap_or("");
    if !hex32(record_id) || !hex32(secret) {
        return response_error("bad_credentials");
    }
    let Some(hash) = recovery_hash(secret) else {
        return response_error("bad_credentials");
    };
    let _gate = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if !state
        .registrations
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(record_id)
        .is_some_and(|r| r.receipt.is_some() && secret_matches(&r.recovery_hash, &hash))
    {
        return response_error("bad_credentials");
    }
    let mut live = state.live.lock().unwrap_or_else(|e| e.into_inner());
    let backup = live.clone();
    live.sessions.remove(record_id);
    drop(live);
    if save_snapshot_locked(state).is_err() {
        *state.live.lock().unwrap_or_else(|e| e.into_inner()) = backup;
        return response_error("storage_failed");
    }
    json!({"ok":true,"server_now":now()})
}

pub(super) fn presence(state: &AppState, record_id: &str, body: &Value) -> Value {
    let token = body["live_token"].as_str().unwrap_or("");
    let instance = body["client_instance_id"].as_str().unwrap_or("");
    if instance.len() > 64
        || instance.is_empty()
        || !instance.is_ascii()
        || !valid_token(state, record_id, token)
    {
        return response_error("bad_token");
    }
    let _gate = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if !valid_token(state, record_id, token) {
        return response_error("bad_token");
    }
    let t = now();
    let mut live = state.live.lock().unwrap_or_else(|e| e.into_inner());
    let backup = live.clone();
    let session = live.sessions.get_mut(record_id).expect("validated token");
    session.lease_until = t + PRESENCE_S;
    drop(live);
    if save_snapshot_locked(state).is_err() {
        *state.live.lock().unwrap_or_else(|e| e.into_inner()) = backup;
        return response_error("storage_failed");
    }
    json!({"ok":true,"server_now":t,"lease_until":t+PRESENCE_S})
}

pub(super) fn joined(state: &AppState, record_id: &str, game_id: u64) {
    let mut live = state.live.lock().unwrap_or_else(|e| e.into_inner());
    let membership = live.memberships.entry(record_id.to_string()).or_default();
    if !membership.contains(&game_id) {
        membership.push(game_id);
    }
}

pub(super) fn phase_instance(game_id: u64, game: &GameEntry) -> String {
    format!(
        "g{game_id}-r{}-{}-{}",
        game.sim.game.round,
        super::phase_name(game.sim.game.phase),
        game.sim.game.phase_ends_at
    )
}

pub(super) fn append_action(state: &AppState, game_id: u64, entry: &GameEntry) {
    let Some(action) = entry.action_log.last() else {
        return;
    };
    if action["ok"] != true {
        return;
    }
    let t = now();
    let event = json!({"room_id":format!("game:{game_id}"),"game_id":game_id,"kind":"game_action",
        "round":action["round"],"phase":action["phase"],"phase_instance_id":phase_instance(game_id,entry),
        "branch_id":"MAIN","finality":"final","visibility":"public",
        "action_ref":{"seq":action["seq"],"actor":action["actor"],"action":action["action"]},"created_at_epoch":t});
    state
        .live
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .append(event, t);
}

pub(super) fn append_final(state: &AppState, game_id: u64, result: &Value) {
    let t = now();
    let event = json!({"room_id":format!("game:{game_id}"),"game_id":game_id,"kind":"final_result",
        "party_no":result["party_no"],"ranks":result["ranks"],"payouts":result["payouts"],
        "branch_id":"MAIN","finality":"final","visibility":"public","created_at_epoch":t});
    state
        .live
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .append(event, t);
}

pub(super) fn append_phase(state: &AppState, game_id: u64, entry: &GameEntry) {
    let t = now();
    let mut event = json!({"room_id":format!("game:{game_id}"),"game_id":game_id,"kind":"phase_changed",
        "round":entry.sim.game.round,"phase":super::phase_name(entry.sim.game.phase),"phase_instance_id":phase_instance(game_id,entry),
        "phase_ends_at":entry.sim.game.phase_ends_at,"grace_until":entry.sim.game.phase_ends_at.saturating_add(entry.grace_s),
        "branch_id":"MAIN","finality":"final","visibility":"public","created_at_epoch":t});
    if let Some(closed) = entry.phase_log.last().filter(|v| v["phase"] == "law") {
        event["law_result"] = json!({"round":closed["round"],"card":closed["card"],
            "passed":closed["passed"],"yes":closed["yes"],"no":closed["no"]});
    }
    state
        .live
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .append(event, t);
}

fn message(
    state: &AppState,
    room: &str,
    game_id: Option<u64>,
    host: Option<&str>,
    body: &Value,
) -> Value {
    let permitted = if game_id.is_some() {
        [
            "token",
            "client_message_id",
            "phase_instance_id",
            "text",
            "to_agent_record_id",
            "reply_to_message_id",
        ]
        .as_slice()
    } else {
        [
            "live_token",
            "client_message_id",
            "text",
            "to_agent_record_id",
            "reply_to_message_id",
            "about_game_id",
            "about_round",
        ]
        .as_slice()
    };
    if !body
        .as_object()
        .is_some_and(|o| o.keys().all(|k| permitted.contains(&k.as_str())))
    {
        return response_error("bad_params");
    }
    let text = body["text"].as_str().unwrap_or("").trim();
    let client_id = body["client_message_id"].as_str().unwrap_or("");
    if text.is_empty()
        || text.len() > MAX_TEXT_BYTES
        || text.chars().count() > MAX_TEXT_CHARS
        || text.chars().any(|c| c.is_control() && c != '\n')
        || client_id.is_empty()
        || client_id.len() > 64
        || !client_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return response_error("bad_message");
    }
    let recipient = body["to_agent_record_id"].as_str();
    if recipient.is_some_and(|id| !hex32(id)) {
        return response_error("bad_recipient");
    }
    let reply = body["reply_to_message_id"].as_str();
    let _gate = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let (author, context) = if let Some(gid) = game_id {
        let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = games.get(&gid) else {
            return response_error("unknown_game");
        };
        let token = body["token"].as_str().unwrap_or("");
        let Some(agent) = entry
            .agents
            .iter()
            .find(|a| agent_token_matches(a, token) && a.agent_record_id.is_some())
        else {
            return response_error("bad_token");
        };
        let expected = phase_instance(gid, entry);
        (
            agent.agent_record_id.clone().unwrap(),
            Some((
                expected,
                entry.sim.game.phase_ends_at,
                entry.sim.game.round,
                super::phase_name(entry.sim.game.phase).to_string(),
            )),
        )
    } else {
        let token = body["live_token"].as_str().unwrap_or("");
        let live = state.live.lock().unwrap_or_else(|e| e.into_inner());
        let Some((id, _)) = live.sessions.iter().find(|(_, s)| {
            hex32(token)
                && now() < s.expires_at
                && secret_matches(&s.token_hash, &sha256_hex(token))
        }) else {
            return response_error("bad_token");
        };
        (id.clone(), None)
    };
    let key = format!("{author}:{client_id}");
    let request_hash = sha256_hex(&json!({"room":room,"text":text,"to":recipient,"reply":reply,"phase":body["phase_instance_id"],"about_game_id":body["about_game_id"],"about_round":body["about_round"]}).to_string());
    let t = now();
    let active = active_games(state);
    let mut live = state.live.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(saved) = live.receipts.get(&key) {
        return if secret_matches(&saved.request_hash, &request_hash) {
            saved.response.clone()
        } else {
            response_error("message_conflict")
        };
    }
    if let Some((expected, deadline, _, _)) = &context {
        if body["phase_instance_id"].as_str() != Some(expected) || t >= *deadline {
            return response_error("stale_context");
        }
    } else if body.get("phase_instance_id").is_some() {
        return response_error("bad_context");
    }
    if let Some(host_id) = host {
        if author != host_id {
            if !live
                .sessions
                .get(host_id)
                .is_some_and(|s| s.ambient_replies_enabled && now() < s.expires_at)
            {
                return response_error("replies_disabled");
            }
            let matches_reply = reply.is_some_and(|id| {
                live.events.iter().any(|e| {
                    e["room_id"] == room
                        && e["message_id"] == id
                        && e["author_agent_record_id"] == host_id
                })
            });
            if !matches_reply {
                return response_error("reply_required");
            }
        }
    }
    if let Some(reply_id) = reply {
        if !live.events.iter().any(|e| {
            e["room_id"] == room && e["message_id"] == reply_id && e["kind"] == "agent_message"
        }) {
            return response_error("bad_reply");
        }
    }
    if let Some(to) = recipient {
        if !state
            .registrations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(to)
            .is_some_and(|r| r.receipt.is_some())
        {
            return response_error("bad_recipient");
        }
        if let Some(gid) = game_id {
            if !state
                .games
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&gid)
                .is_some_and(|e| {
                    e.agents
                        .iter()
                        .any(|a| a.agent_record_id.as_deref() == Some(to))
                })
            {
                return response_error("bad_recipient");
            }
        }
    }
    let today = t / 86_400;
    let mine: Vec<_> = live
        .events
        .iter()
        .filter(|e| e["kind"] == "agent_message" && e["author_agent_record_id"] == author)
        .collect();
    if mine
        .last()
        .is_some_and(|e| t - e["created_at_epoch"].as_i64().unwrap_or(0) < 5)
    {
        return response_error("rate_limited");
    }
    if let Some(gid) = game_id {
        if mine
            .iter()
            .filter(|e| e["game_id"].as_u64() == Some(gid))
            .count()
            >= 24
        {
            return response_error("budget_exhausted");
        }
        if let Some((expected, _, _, _)) = &context {
            if mine
                .iter()
                .filter(|e| {
                    e["game_id"].as_u64() == Some(gid)
                        && e["phase_instance_id"].as_str() == Some(expected)
                })
                .count()
                >= 4
            {
                return response_error("budget_exhausted");
            }
        }
    } else if mine
        .iter()
        .filter(|e| {
            e["room_id"]
                .as_str()
                .is_some_and(|s| s.starts_with("agent:"))
                && e["created_at_epoch"].as_i64().unwrap_or(0) / 86_400 == today
        })
        .count()
        >= 12
    {
        return response_error("budget_exhausted");
    }
    let backup = live.clone();
    live.prune(t, &active);
    let next = live.next_seq + 1;
    let mut event = json!({"room_id":room,"kind":"agent_message","visibility":"public","author_agent_record_id":author,
        "to_agent_record_id":recipient,"reply_to_message_id":reply,"message_id":format!("m{next}"),"text":text,"created_at_epoch":t});
    if let Some(gid) = game_id {
        let (phase_id, _, round, phase) = context.unwrap();
        event["game_id"] = json!(gid);
        event["round"] = json!(round);
        event["phase"] = json!(phase);
        event["phase_instance_id"] = json!(phase_id);
        event["branch_id"] = json!("MAIN");
        event["finality"] = json!("final");
    } else {
        event["context_kind"] = json!("ambient");
        if let Some(id) = body["about_game_id"].as_u64() {
            event["about_game_id"] = json!(id);
        }
        if let Some(round) = body["about_round"].as_u64() {
            event["about_round"] = json!(round);
        }
    }
    let event = live.append(event, t);
    let receipt = json!({"ok":true,"status":"accepted","client_message_id":client_id,
        "message_id":event["message_id"],"event_id":event["event_id"],"seq":event["seq"],
        "server_created_at":event["server_created_at"],"server_now":t});
    live.receipts.insert(
        key,
        MessageReceipt {
            request_hash,
            response: receipt.clone(),
            created_at: t,
        },
    );
    drop(live);
    if save_snapshot_locked(state).is_err() {
        *state.live.lock().unwrap_or_else(|e| e.into_inner()) = backup;
        return response_error("storage_failed");
    }
    receipt
}

pub(super) fn game_message(state: &AppState, game_id: u64, body: &Value) -> Value {
    message(state, &format!("game:{game_id}"), Some(game_id), None, body)
}
pub(super) fn ambient_message(state: &AppState, host: &str, body: &Value) -> Value {
    if !hex32(host) {
        return response_error("bad_id");
    }
    message(state, &format!("agent:{host}"), None, Some(host), body)
}

fn query(raw: &str, key: &str, default: u64) -> Result<u64, &'static str> {
    let Some((_, args)) = raw.split_once('?') else {
        return Ok(default);
    };
    let mut seen = HashSet::new();
    let mut result = default;
    for pair in args.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            return Err("bad_cursor");
        };
        if !matches!(k, "after" | "limit") || !seen.insert(k) {
            return Err("bad_cursor");
        }
        if k == key {
            result = v.parse().map_err(|_| "bad_cursor")?;
        }
    }
    Ok(result)
}

fn events(state: &AppState, room: Option<&str>, personal: Option<&str>, raw: &str) -> Value {
    let after = match query(raw, "after", 0) {
        Ok(v) => v,
        Err(e) => return response_error(e),
    };
    let limit = match query(raw, "limit", 100) {
        Ok(v) if v > 0 && v <= MAX_LIMIT as u64 => v as usize,
        _ => return response_error("bad_limit"),
    };
    let live = state.live.lock().unwrap_or_else(|e| e.into_inner());
    if after > live.next_seq {
        return response_error("cursor_ahead");
    }
    let mut rooms = HashSet::new();
    if let Some(r) = room {
        rooms.insert(r.to_string());
    }
    if let Some(id) = personal {
        rooms.insert(format!("agent:{id}"));
        for gid in live.memberships.get(id).into_iter().flatten() {
            rooms.insert(format!("game:{gid}"));
        }
    }
    let mut pruned = rooms
        .iter()
        .filter_map(|r| live.pruned_room.get(r).copied())
        .max()
        .unwrap_or(0);
    if let Some(id) = personal {
        pruned = pruned.max(live.pruned_personal.get(id).copied().unwrap_or(0));
    }
    if after > 0 && after < pruned {
        return json!({"ok":false,"error":"cursor_expired","pruned_through_seq":pruned,"last_seq":live.next_seq,"server_now":now()});
    }
    let relevant = |e: &&Value| {
        rooms.contains(e["room_id"].as_str().unwrap_or(""))
            || personal.is_some_and(|id| {
                e["kind"] == "agent_message"
                    && (e["to_agent_record_id"] == id || e["author_agent_record_id"] == id)
            })
    };
    // ponytail: scan retained journal for the pilot; per-room indexes if polling throughput demands it.
    let mut page: Vec<Value> = live
        .events
        .iter()
        .filter(|e| e["seq"].as_u64().unwrap_or(0) > after)
        .filter(relevant)
        .take(limit + 1)
        .cloned()
        .collect();
    let has_more = page.len() > limit;
    page.truncate(limit);
    let next_cursor = if has_more {
        page.last().and_then(|e| e["seq"].as_u64()).unwrap_or(after)
    } else {
        live.next_seq
    };
    let mut result = json!({"ok":true,"events":page,"next_cursor":next_cursor,"oldest_available_seq":live.events.first().and_then(|e|e["seq"].as_u64()).unwrap_or(live.next_seq+1),
        "pruned_through_seq":pruned,"last_seq":live.next_seq,"server_now":now(),"has_more":has_more,"history_truncated":after==0 && pruned>0});
    if let Some(id) = personal {
        result["stream_id"] = json!(id);
        result["presence"] = json!(if live
            .sessions
            .get(id)
            .is_some_and(|s| now() < s.lease_until && now() < s.expires_at)
        {
            "connected"
        } else {
            "offline"
        });
    }
    result
}
pub(super) fn game_events(state: &AppState, game_id: u64, raw: &str) -> Value {
    events(state, Some(&format!("game:{game_id}")), None, raw)
}
pub(super) fn personal_events(state: &AppState, id: &str, raw: &str) -> Value {
    if !hex32(id) {
        return response_error("bad_id");
    }
    events(state, None, Some(id), raw)
}

pub(super) fn validate(live: &LiveState) -> bool {
    live.events.windows(2).all(|w| {
        w[0]["seq"]
            .as_u64()
            .is_some_and(|a| w[1]["seq"].as_u64().is_some_and(|b| a < b))
    }) && live
        .events
        .last()
        .is_none_or(|e| e["seq"].as_u64() == Some(live.next_seq))
        && live
            .sessions
            .iter()
            .all(|(id, s)| hex32(id) && hex32(&s.token_hash))
}

pub(super) fn clear_loaded_presence(live: &mut LiveState) {
    for session in live.sessions.values_mut() {
        session.lease_until = 0;
    }
}
