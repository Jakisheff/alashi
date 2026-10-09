//! Private owner guidance for an actual, wallet-bound on-chain devnet faction.
//! Deliberately separate from the HTTP simulator's `wishes` game-id namespace.
use super::owner_auth::{origin_ok, owner_session_for};
use super::{
    hex32, now, random_hex, recovery_hash, save_snapshot_locked, secret_matches, sha256_hex,
    AppState,
};
use alashi_rules::anchor_lang::prelude::Pubkey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::str::FromStr;
use std::time::Duration;

const MAX_TEXT: usize = 512;
const MAX_WISHES: u8 = 3;
const LEASE_SECONDS: i64 = 30;
const RUNNER_SECONDS: i64 = 300;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainBinding {
    pub record_id: String,
    pub game_pda: String,
    pub faction_pda: String,
    pub faction_wallet: String,
    pub runner_token_hash: String,
    pub last_seen_at: i64,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChainStatus {
    Received,
    Consumed,
    Deferred,
    Declined,
    Expired,
    Unconfirmed,
    Confirmed,
}
impl ChainStatus {
    fn name(&self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::Consumed => "consumed",
            Self::Deferred => "deferred",
            Self::Declined => "declined",
            Self::Expired => "expired",
            Self::Unconfirmed => "unconfirmed",
            Self::Confirmed => "confirmed",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainWish {
    pub wish_id: String,
    pub seq: u64,
    pub status_seq: u64,
    pub record_id: String,
    pub game_pda: String,
    pub client_wish_id: String,
    pub request_hash: String,
    pub intent: String,
    pub text: String,
    pub status: ChainStatus,
    pub accepted_at: i64,
    pub admission_remaining: u8,
    pub status_at: i64,
    pub lease_id: Option<String>,
    pub lease_expires_at: Option<i64>,
    pub consumptions: u8,
    pub consumed_after_slot: Option<u64>,
    pub signature: Option<String>,
    pub slot: Option<u64>,
    pub event_id: Option<String>,
    pub reply: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ChainWishState {
    pub next_seq: u64,
    pub bindings: HashMap<String, ChainBinding>,
    pub wishes: HashMap<String, ChainWish>,
    pub idempotency: HashMap<String, String>,
    pub accepted: HashMap<String, u8>,
}

fn err(code: &str) -> Value {
    json!({"ok":false,"error":code})
}
fn key(record: &str, game: &str) -> String {
    format!("{record}:{game}")
}
fn idem(record: &str, game: &str, client: &str) -> String {
    format!("{record}:{game}:{client}")
}
fn token_hash(token: &str) -> String {
    sha256_hex(&format!("chain-runner-v1:{token}"))
}
fn canonical_pda(pda: &str) -> bool {
    Pubkey::from_str(pda)
        .ok()
        .is_some_and(|p| p.to_string() == pda)
}
fn client_id_ok(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 96
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn intent_ok(v: &str) -> bool {
    matches!(
        v,
        "produce" | "sell_one" | "buy_one" | "vote_yes" | "vote_no"
    )
}
fn intent_phase(intent: &str, phase: &str) -> bool {
    matches!(
        (intent, phase),
        ("produce", "Action")
            | ("sell_one" | "buy_one", "Market")
            | ("vote_yes" | "vote_no", "Law")
    )
}
fn text_ok(v: &str) -> bool {
    !v.trim().is_empty() && v.len() <= MAX_TEXT && !v.chars().any(char::is_control)
}
fn binding_live(b: &ChainBinding, at: i64) -> bool {
    at.saturating_sub(b.last_seen_at) <= RUNNER_SECONDS
}

/// Owner handoff requires an already verified exact binding. A live runner
/// must be recent; a returning owner may also pair to that same faction after
/// a confirmed settled Game, without reviving a runner or signing anything.
pub(super) fn owner_handoff_binding(
    state: &AppState,
    record: &str,
    game: &str,
    faction: &str,
    wallet: &str,
) -> bool {
    let bound = {
        let ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
        ledger.bindings.get(&key(record, game)).and_then(|binding| {
            (binding.record_id == record
                && binding.game_pda == game
                && binding.faction_pda == faction
                && binding.faction_wallet == wallet)
                .then(|| binding_live(binding, now()))
        })
    };
    match bound {
        Some(true) => true,
        Some(false) => public_chain(game).ok().is_some_and(|view| {
            view["game"]["phase"] == "Finished"
                && view["game"]["settled"] == true
                && view["factions"].as_array().is_some_and(|rows| rows.iter().any(|row| {
                    row["pda"] == faction && row["wallet"] == wallet
                }))
        }),
        None => false,
    }
}

/// One fixed loopback projection. No arbitrary host, URL, or upstream token.
fn public_chain(game: &str) -> Result<Value, &'static str> {
    if !canonical_pda(game) {
        return Err("bad_game_pda");
    }
    let port = std::env::var("ALASHI_CHAIN_API_PORT")
        .ok()
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(8097);
    let mut conn = TcpStream::connect(("127.0.0.1", port)).map_err(|_| "chain_api_unavailable")?;
    conn.set_read_timeout(Some(Duration::from_secs(100)))
        .map_err(|_| "chain_api_unavailable")?;
    let path = format!("/chain/devnet/games/{game}");
    conn.write_all(
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n").as_bytes(),
    )
    .map_err(|_| "chain_api_unavailable")?;
    let mut bytes = Vec::new();
    conn.take(2_000_001)
        .read_to_end(&mut bytes)
        .map_err(|_| "chain_api_unavailable")?;
    if bytes.len() > 2_000_000 {
        return Err("chain_api_unavailable");
    }
    let header_end = bytes
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or("chain_api_unavailable")?;
    if !bytes.starts_with(b"HTTP/1.1 200 OK\r\n") {
        return Err("chain_api_unavailable");
    }
    let value: Value =
        serde_json::from_slice(&bytes[header_end + 4..]).map_err(|_| "chain_api_unavailable")?;
    if value["ok"] != true
        || value["game_pda"] != game
        || value["cluster"] != "devnet"
        || value["program_id"] != "3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC"
    {
        return Err("chain_api_unavailable");
    }
    Ok(value)
}
fn active(view: &Value) -> bool {
    matches!(
        view["game"]["phase"].as_str(),
        Some("Lobby" | "Market" | "Action" | "Law")
    ) && view["game"]["settled"] == false
}
fn binding_for<'a>(
    state: &'a ChainWishState,
    record: &str,
    game: &str,
) -> Option<&'a ChainBinding> {
    state.bindings.get(&key(record, game))
}
fn authorize_runner<'a>(
    state: &'a ChainWishState,
    game: &str,
    raw: &str,
) -> Option<&'a ChainBinding> {
    if !hex32(raw) {
        return None;
    }
    let hash = token_hash(raw);
    state
        .bindings
        .values()
        .find(|b| b.game_pda == game && secret_matches(&b.runner_token_hash, &hash))
}
pub fn validate(s: &ChainWishState) -> bool {
    if s.wishes.len() != s.idempotency.len() || s.bindings.len() > 1000 || s.wishes.len() > 3000 {
        return false;
    }
    let mut counts = HashMap::<String, u8>::new();
    let mut sequence = HashSet::new();
    let mut status_sequence = HashSet::new();
    for (id, w) in &s.wishes {
        if id != &w.wish_id
            || !hex32(id)
            || !hex32(&w.record_id)
            || !canonical_pda(&w.game_pda)
            || !intent_ok(&w.intent)
            || !text_ok(&w.text)
            || !client_id_ok(&w.client_wish_id)
            || w.seq == 0
            || !sequence.insert(w.seq)
            || w.status_seq < w.seq
            || !status_sequence.insert(w.status_seq)
            || w.status_seq > s.next_seq
            || w.consumptions > 1
            || (w.consumptions == 0) != w.consumed_after_slot.is_none()
            || w.lease_id.is_some() != w.lease_expires_at.is_some()
            || (w.status == ChainStatus::Confirmed)
                != (w.signature.is_some()
                    && w.slot.is_some()
                    && w.event_id.is_some()
                    && w.reply.is_some())
            || w.admission_remaining >= MAX_WISHES
            || s.idempotency
                .get(&idem(&w.record_id, &w.game_pda, &w.client_wish_id))
                != Some(id)
        {
            return false;
        }
        let count = counts.entry(key(&w.record_id, &w.game_pda)).or_default();
        *count = match count.checked_add(1) {
            Some(v) if v <= MAX_WISHES => v,
            _ => return false,
        };
    }
    if counts != s.accepted {
        return false;
    }
    let mut binding_factions = HashSet::new();
    s.bindings.iter().all(|(k, b)| {
        k == &key(&b.record_id, &b.game_pda)
            && hex32(&b.record_id)
            && hex32(&b.runner_token_hash)
            && canonical_pda(&b.game_pda)
            && canonical_pda(&b.faction_pda)
            && canonical_pda(&b.faction_wallet)
            && binding_factions.insert((b.game_pda.clone(), b.faction_pda.clone()))
    })
}

/// Called only from the loopback runner route after the faction exists on chain.
pub fn bind(state: &AppState, game: &str, body: &Value) -> Value {
    if !body.as_object().is_some_and(|o| {
        o.len() == 3
            && o.contains_key("agent_record_id")
            && o.contains_key("recovery_secret")
            && o.contains_key("faction_pda")
    }) {
        return err("bad_binding");
    }
    let (Some(record), Some(secret), Some(faction)) = (
        body["agent_record_id"].as_str(),
        body["recovery_secret"].as_str(),
        body["faction_pda"].as_str(),
    ) else {
        return err("bad_binding");
    };
    if !hex32(record) || !hex32(secret) || !canonical_pda(game) || !canonical_pda(faction) {
        return err("bad_binding");
    }
    let Some(secret_hash) = recovery_hash(secret) else {
        return err("bad_binding");
    };
    let wallet = {
        let registrations = state
            .registrations
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(r) = registrations.get(record) else {
            return err("binding_forbidden");
        };
        if !secret_matches(&r.recovery_hash, &secret_hash)
            || r.receipt.as_ref().is_none_or(|p| {
                p.mode != "agent_lifecycle_v2" || p.network != "devnet" || p.wallet != r.wallet
            })
        {
            return err("binding_forbidden");
        }
        r.wallet.clone()
    };
    bind_verified(state, game, record, faction, &wallet, None)
}

/// Public runner binding uses a short owner bearer minted only after the
/// registered wallet signs the existing owner challenge. It never receives a
/// recovery secret, browser cookie, or arbitrary Game authority.
pub fn bind_wallet(
    state: &AppState,
    game: &str,
    origin: &str,
    bearer: &str,
    body: &Value,
) -> Value {
    if !origin_ok(origin) {
        return err("owner_origin_forbidden");
    }
    if !body.as_object().is_some_and(|o| {
        o.len() == 2 && o.contains_key("agent_record_id") && o.contains_key("faction_pda")
    }) {
        return err("bad_binding");
    }
    let (Some(record), Some(faction)) = (
        body["agent_record_id"].as_str(),
        body["faction_pda"].as_str(),
    ) else {
        return err("bad_binding");
    };
    if !hex32(record) || !canonical_pda(game) || !canonical_pda(faction) {
        return err("bad_binding");
    }
    let owner = match owner_session_for(state, record, bearer) {
        Ok(owner) => owner,
        Err(code) => return err(code),
    };
    bind_verified(state, game, record, faction, &owner.wallet, Some(bearer))
}

fn bind_verified(state: &AppState, game: &str, record: &str, faction: &str, wallet: &str, bearer: Option<&str>) -> Value {
    let view = match public_chain(game) {
        Ok(v) => v,
        Err(code) => return err(code),
    };
    let matched_faction = view["factions"].as_array().and_then(|arr| {
        arr.iter()
            .find(|f| f["pda"] == faction && f["wallet"] == wallet)
    });
    if matched_faction.is_none() {
        return err("binding_forbidden");
    }
    let token = match random_hex() {
        Ok(v) => v,
        Err(_) => return err("entropy_unavailable"),
    };
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if bearer.is_some_and(|raw| match owner_session_for(state, record, raw) {
        Ok(owner) => owner.wallet != wallet,
        Err(_) => true,
    }) {
        return err("owner_session_invalid");
    }
    let mut ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
    let before = ledger.clone();
    // A restarted runner may need a new capability to report its already
    // submitted receipt after the game ends. Never create a new terminal bind.
    let receipt_only_rebind = !active(&view)
        && binding_for(&ledger, record, game)
            .is_some_and(|b| b.faction_pda == faction && b.faction_wallet == wallet)
        && ledger.wishes.values().any(|w| {
            w.record_id == record
                && w.game_pda == game
                && matches!(w.status, ChainStatus::Consumed | ChainStatus::Unconfirmed)
        });
    if !(active(&view) && matched_faction.is_some_and(|f| f["alive"] == true)
        || receipt_only_rebind)
    {
        return err("binding_forbidden");
    }
    // A faction may have only one record and one runner authority in a game.
    if ledger
        .bindings
        .values()
        .any(|b| b.game_pda == game && b.faction_pda == faction && b.record_id != record)
    {
        return err("binding_conflict");
    }
    ledger.bindings.insert(
        key(record, game),
        ChainBinding {
            record_id: record.to_string(),
            game_pda: game.to_string(),
            faction_pda: faction.to_string(),
            faction_wallet: wallet.to_string(),
            runner_token_hash: token_hash(&token),
            last_seen_at: at,
        },
    );
    drop(ledger);
    if save_snapshot_locked(state).is_err() {
        *state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    json!({"ok":true,"runner_token":token,"record_id":record,"game_pda":game,"faction_pda":faction,"faction_wallet":wallet})
}

pub fn submit(state: &AppState, record: &str, origin: &str, session: &str, body: &Value) -> Value {
    if !origin_ok(origin) {
        return err("owner_origin_forbidden");
    }
    let owner = match owner_session_for(state, record, session) {
        Ok(v) => v,
        Err(_) => return err("owner_session_invalid"),
    };
    if !body.as_object().is_some_and(|o| {
        o.len() == 4
            && o.contains_key("game_pda")
            && o.contains_key("client_wish_id")
            && o.contains_key("intent")
            && o.contains_key("text")
    }) {
        return err("bad_chain_wish");
    }
    let (Some(game), Some(client), Some(intent), Some(text)) = (
        body["game_pda"].as_str(),
        body["client_wish_id"].as_str(),
        body["intent"].as_str(),
        body["text"].as_str(),
    ) else {
        return err("bad_chain_wish");
    };
    if !canonical_pda(game) || !client_id_ok(client) || !intent_ok(intent) || !text_ok(text) {
        return err("bad_chain_wish");
    }
    let hash = sha256_hex(&format!(
        "chain-wish-v1\n{record}\n{game}\n{client}\n{intent}\n{text}"
    ));
    // Lost-response retries return the durable admission even after a game ends.
    {
        let ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(w) = ledger
            .idempotency
            .get(&idem(record, game, client))
            .and_then(|id| ledger.wishes.get(id))
        {
            return if w.request_hash == hash {
                admission(
                    w,
                    MAX_WISHES - *ledger.accepted.get(&key(record, game)).unwrap_or(&0),
                )
            } else {
                err("idempotency_conflict")
            };
        }
    }
    let view = match public_chain(game) {
        Ok(v) => v,
        Err(code) => return err(code),
    };
    if !active(&view) {
        return err("no_active_chain_game");
    }
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if owner_session_for(state, record, session).is_err() {
        return err("owner_session_invalid");
    }
    let mut ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(w) = ledger
        .idempotency
        .get(&idem(record, game, client))
        .and_then(|id| ledger.wishes.get(id))
    {
        return if w.request_hash == hash {
            admission(
                w,
                MAX_WISHES - *ledger.accepted.get(&key(record, game)).unwrap_or(&0),
            )
        } else {
            err("idempotency_conflict")
        };
    }
    let Some(binding) = binding_for(&ledger, record, game) else {
        return err("no_chain_binding");
    };
    if binding.faction_wallet != owner.wallet
        || !binding_live(binding, at)
        || !view["factions"].as_array().is_some_and(|arr| {
            arr.iter().any(|f| {
                f["pda"] == binding.faction_pda
                    && f["wallet"] == binding.faction_wallet
                    && f["alive"] == true
            })
        })
    {
        return err("binding_forbidden");
    }
    let used = *ledger.accepted.get(&key(record, game)).unwrap_or(&0);
    if used >= MAX_WISHES {
        return err("wish_quota_exhausted");
    }
    let id = match random_hex() {
        Ok(v) => v,
        Err(_) => return err("entropy_unavailable"),
    };
    let before = ledger.clone();
    ledger.next_seq += 1;
    let w = ChainWish {
        wish_id: id.clone(),
        seq: ledger.next_seq,
        status_seq: ledger.next_seq,
        record_id: record.to_string(),
        game_pda: game.to_string(),
        client_wish_id: client.to_string(),
        request_hash: hash,
        intent: intent.to_string(),
        text: text.to_string(),
        status: ChainStatus::Received,
        accepted_at: at,
        admission_remaining: MAX_WISHES - used - 1,
        status_at: at,
        lease_id: None,
        lease_expires_at: None,
        consumptions: 0,
        consumed_after_slot: None,
        signature: None,
        slot: None,
        event_id: None,
        reply: None,
    };
    ledger.wishes.insert(id.clone(), w.clone());
    ledger.idempotency.insert(idem(record, game, client), id);
    ledger.accepted.insert(key(record, game), used + 1);
    drop(ledger);
    if save_snapshot_locked(state).is_err() {
        *state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    admission(&w, MAX_WISHES - used - 1)
}
fn admission(w: &ChainWish, remaining: u8) -> Value {
    json!({"ok":true,"wish_id":w.wish_id,"game_pda":w.game_pda,"status":"received",
    "accepted_at":w.accepted_at,"admission_remaining":w.admission_remaining,"remaining":remaining})
}

fn expire_pending(ledger: &mut ChainWishState, record: &str, game: &str, at: i64) {
    let pending = ledger
        .wishes
        .values()
        .filter(|w| {
            w.record_id == record
                && w.game_pda == game
                && matches!(w.status, ChainStatus::Received | ChainStatus::Deferred)
        })
        .map(|w| w.wish_id.clone())
        .collect::<Vec<_>>();
    for id in pending {
        ledger.next_seq += 1;
        let seq = ledger.next_seq;
        let w = ledger.wishes.get_mut(&id).unwrap();
        w.status = ChainStatus::Expired;
        w.status_at = at;
        w.status_seq = seq;
        w.lease_id = None;
        w.lease_expires_at = None;
    }
}

/// A runner normally makes a final non-signing claim. Owner reads also
/// reconcile terminal games after a runner exits unexpectedly. An unavailable
/// projection leaves the last durable status untouched and is labelled below.
fn reconcile_owner_terminal(state: &AppState, record: &str, game: &str) -> bool {
    let pending = state
        .chain_wishes
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .wishes
        .values()
        .any(|w| {
            w.record_id == record
                && w.game_pda == game
                && matches!(w.status, ChainStatus::Received | ChainStatus::Deferred)
        });
    if !pending {
        return true;
    }
    let view = match public_chain(game) {
        Ok(v) => v,
        Err(_) => return false,
    };
    if active(&view) {
        return true;
    }
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
    let before = ledger.clone();
    expire_pending(&mut ledger, record, game, now());
    drop(ledger);
    if save_snapshot_locked(state).is_err() {
        *state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return false;
    }
    true
}

pub fn owner_after(
    state: &AppState,
    record: &str,
    session: &str,
    game: &str,
    after: u64,
    limit: usize,
) -> Value {
    let owner = match owner_session_for(state, record, session) {
        Ok(v) => v,
        Err(_) => return err("owner_session_invalid"),
    };
    if !canonical_pda(game) || (1..=100).contains(&limit) == false {
        return err("bad_cursor");
    }
    let chain_observation_available = reconcile_owner_terminal(state, record, game);
    let ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
    let binding = binding_for(&ledger, record, game).filter(|b| b.faction_wallet == owner.wallet);
    let mut rows = ledger
        .wishes
        .values()
        .filter(|w| w.record_id == record && w.game_pda == game && w.status_seq > after)
        .collect::<Vec<_>>();
    rows.sort_by_key(|w| w.status_seq);
    rows.truncate(limit);
    let next = rows.last().map(|w| w.status_seq).unwrap_or(after);
    let last = ledger
        .wishes
        .values()
        .filter(|w| w.record_id == record && w.game_pda == game)
        .map(|w| w.status_seq)
        .max()
        .unwrap_or(0);
    json!({"ok":true,"game_pda":game,"binding":binding.map(|b|json!({"faction_pda":b.faction_pda,
        "faction_wallet":b.faction_wallet,"active":binding_live(b,now())})),
        "wishes":rows.iter().map(|w|json!({"wish_id":w.wish_id,"seq":w.seq,"status_seq":w.status_seq,
            "intent":w.intent,"text":w.text,"status":w.status.name(),"accepted_at":w.accepted_at,
            "status_at":w.status_at,"reply":w.reply,"signature":w.signature,"slot":w.slot,"event_id":w.event_id})).collect::<Vec<_>>(),
        "remaining":MAX_WISHES.saturating_sub(*ledger.accepted.get(&key(record,game)).unwrap_or(&0)),
        "next_cursor":next,"last_seq":last,"chain_observation_available":chain_observation_available})
}

pub fn claim(state: &AppState, game: &str, body: &Value) -> Value {
    if !body.as_object().is_some_and(|o| {
        o.len() == 3
            && o.contains_key("runner_token")
            && o.contains_key("after")
            && o.contains_key("limit")
    }) {
        return err("bad_chain_claim");
    }
    let Some(raw) = body["runner_token"].as_str() else {
        return err("runner_token_invalid");
    };
    let (Some(after), Some(1)) = (body["after"].as_u64(), body["limit"].as_u64()) else {
        return err("bad_chain_claim");
    };
    {
        let ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
        if authorize_runner(&ledger, game, raw).is_none() {
            return err("runner_token_invalid");
        }
    }
    let view = match public_chain(game) {
        Ok(v) => v,
        Err(code) => return err(code),
    };
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
    let Some(binding) = authorize_runner(&ledger, game, raw).cloned() else {
        return err("runner_token_invalid");
    };
    if active(&view)
        && !view["factions"].as_array().is_some_and(|rows| {
            rows.iter().any(|f| {
                f["pda"] == binding.faction_pda
                    && f["wallet"] == binding.faction_wallet
                    && f["alive"] == true
            })
        })
    {
        return err("binding_forbidden");
    }
    let before = ledger.clone();
    let k = key(&binding.record_id, game);
    ledger.bindings.get_mut(&k).unwrap().last_seen_at = at;
    if !active(&view) {
        expire_pending(&mut ledger, &binding.record_id, game, at);
        let last = ledger
            .wishes
            .values()
            .filter(|w| w.record_id == binding.record_id && w.game_pda == game)
            .map(|w| w.seq)
            .max()
            .unwrap_or(after);
        drop(ledger);
        if save_snapshot_locked(state).is_err() {
            *state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner()) = before;
            return err("storage_failed");
        };
        return json!({"ok":true,"wishes":[],"next_cursor":after,"last_seq":last});
    }
    let mut ids = ledger
        .wishes
        .values()
        .filter(|w| {
            w.record_id == binding.record_id
                && w.game_pda == game
                && w.seq > after
                && matches!(w.status, ChainStatus::Received | ChainStatus::Deferred)
                && intent_phase(&w.intent, view["game"]["phase"].as_str().unwrap_or(""))
                && w.lease_expires_at.is_none_or(|expiry| expiry <= at)
        })
        .map(|w| w.wish_id.clone())
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| {
        let w = &ledger.wishes[id];
        (w.status == ChainStatus::Deferred, w.seq)
    });
    let row = if let Some(id) = ids.first() {
        let lease = match random_hex() {
            Ok(v) => v,
            Err(_) => return err("entropy_unavailable"),
        };
        let w = ledger.wishes.get_mut(id).unwrap();
        w.lease_id = Some(lease.clone());
        w.lease_expires_at = Some(at + LEASE_SECONDS);
        Some(
            json!({"wish_id":w.wish_id,"seq":w.seq,"intent":w.intent,"text":w.text,"status":w.status.name(),
            "lease_id":lease,"lease_expires_at":at+LEASE_SECONDS}),
        )
    } else {
        None
    };
    let last = ledger
        .wishes
        .values()
        .filter(|w| w.record_id == binding.record_id && w.game_pda == game)
        .map(|w| w.seq)
        .max()
        .unwrap_or(0);
    drop(ledger);
    if save_snapshot_locked(state).is_err() {
        *state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    json!({"ok":true,"wishes":row.into_iter().collect::<Vec<_>>(),"next_cursor":after,"last_seq":last})
}

pub fn status(state: &AppState, game: &str, id: &str, body: &Value) -> Value {
    if !body.as_object().is_some_and(|o| {
        o.keys().all(|k| {
            matches!(
                k.as_str(),
                "runner_token" | "lease_id" | "status" | "signature" | "slot" | "event_id"
            )
        }) && o.contains_key("runner_token")
            && o.contains_key("status")
    }) {
        return err("bad_chain_status");
    }
    let (Some(raw), Some(target)) = (body["runner_token"].as_str(), body["status"].as_str()) else {
        return err("bad_chain_status");
    };
    if !matches!(
        target,
        "consumed" | "deferred" | "declined" | "expired" | "unconfirmed" | "confirmed"
    ) {
        return err("bad_chain_status");
    }
    {
        let ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
        let Some(binding) = authorize_runner(&ledger, game, raw) else {
            return err("runner_token_invalid");
        };
        let Some(w) = ledger
            .wishes
            .get(id)
            .filter(|w| w.record_id == binding.record_id && w.game_pda == game)
        else {
            return err("wish_forbidden");
        };
        if w.status.name() == target && matches!(target, "consumed" | "unconfirmed" | "confirmed") {
            if target == "confirmed"
                && (body["signature"].as_str() != w.signature.as_deref()
                    || body["slot"].as_u64() != w.slot)
            {
                return err("receipt_mismatch");
            }
            return status_response(w);
        }
    }
    let view = if matches!(target, "confirmed" | "consumed") {
        match public_chain(game) {
            Ok(v) => Some(v),
            Err(code) => return err(code),
        }
    } else {
        None
    };
    let at = now();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
    let Some(binding) = authorize_runner(&ledger, game, raw).cloned() else {
        return err("runner_token_invalid");
    };
    let Some(w) = ledger.wishes.get(id) else {
        return err("unknown_wish");
    };
    if w.record_id != binding.record_id || w.game_pda != game {
        return err("wish_forbidden");
    }
    let leased = w.lease_id.as_deref() == body["lease_id"].as_str()
        && w.lease_expires_at.is_some_and(|expiry| at < expiry);
    let legal = match (&w.status, target) {
        (
            ChainStatus::Received | ChainStatus::Deferred,
            "consumed" | "deferred" | "declined" | "expired",
        ) => leased && (target != "consumed" || w.consumptions == 0),
        (ChainStatus::Consumed, "confirmed" | "unconfirmed" | "declined" | "expired") => true,
        (ChainStatus::Unconfirmed, "confirmed") => true,
        _ => false,
    };
    if !legal {
        return err("wish_status_conflict");
    }
    let consumed_baseline = if target == "consumed" {
        let v = view.as_ref().unwrap();
        let slot = v["snapshot_slot"]
            .as_u64()
            .unwrap_or(0)
            .max(v["journal_through_slot"].as_u64().unwrap_or(0));
        if slot == 0 {
            return err("chain_api_unavailable");
        }
        Some(slot)
    } else {
        None
    };
    let receipt = if target == "confirmed" {
        let (Some(sig), Some(slot)) = (body["signature"].as_str(), body["slot"].as_u64()) else {
            return err("bad_chain_receipt");
        };
        if !w.consumed_after_slot.is_some_and(|before| slot > before) {
            return err("receipt_mismatch");
        }
        let Some(rows) = view.as_ref().unwrap()["events"].as_array() else {
            return err("receipt_pending");
        };
        let matches = rows
            .iter()
            .filter(|e| {
                e["signature"] == sig
                    && e["slot"] == slot
                    && e["game"] == game
                    && event_for_intent(e, &w.intent, &binding.faction_pda)
            })
            .collect::<Vec<_>>();
        if matches.is_empty() {
            return err("receipt_pending");
        }
        if matches.len() != 1 {
            return err("receipt_mismatch");
        }
        let Some(event_id) = matches[0]["id"].as_str() else {
            return err("receipt_mismatch");
        };
        if body["event_id"].as_str().is_some_and(|id| id != event_id) {
            return err("receipt_mismatch");
        }
        Some((sig.to_string(), slot, event_id.to_string()))
    } else {
        None
    };
    let before = ledger.clone();
    ledger.next_seq += 1;
    let status_seq = ledger.next_seq;
    let w = ledger.wishes.get_mut(id).unwrap();
    w.status = match target {
        "consumed" => ChainStatus::Consumed,
        "deferred" => ChainStatus::Deferred,
        "declined" => ChainStatus::Declined,
        "expired" => ChainStatus::Expired,
        "unconfirmed" => ChainStatus::Unconfirmed,
        "confirmed" => ChainStatus::Confirmed,
        _ => unreachable!(),
    };
    w.status_at = at;
    w.status_seq = status_seq;
    w.lease_id = None;
    w.lease_expires_at = None;
    if target == "consumed" {
        w.consumptions += 1
    }
    if let Some(baseline) = consumed_baseline {
        w.consumed_after_slot = Some(baseline);
    }
    if let Some((sig, slot, event_id)) = receipt {
        w.signature = Some(sig);
        w.slot = Some(slot);
        w.event_id = Some(event_id);
        w.reply = Some("Запрошенное действие подтверждено в devnet".into());
    }
    let result = status_response(w);
    drop(ledger);
    if save_snapshot_locked(state).is_err() {
        *state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner()) = before;
        return err("storage_failed");
    }
    result
}
fn status_response(w: &ChainWish) -> Value {
    json!({"ok":true,"wish_id":w.wish_id,"status":w.status.name(),"status_seq":w.status_seq,
        "signature":w.signature,"slot":w.slot,"event_id":w.event_id,
        "consumed_after_slot":w.consumed_after_slot})
}
fn event_for_intent(e: &Value, intent: &str, faction: &str) -> bool {
    if e["faction"] != faction {
        return false;
    }
    match intent {
        "produce" => e["type"] == "produced",
        "sell_one" => e["type"] == "sold" && e["units"] == 1,
        "buy_one" => e["type"] == "goods_bought" && e["units"] == 1,
        "vote_yes" => e["type"] == "vote_cast" && e["choice"] == 0,
        "vote_no" => e["type"] == "vote_cast" && e["choice"] == 1,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intent_requires_matching_confirmed_event() {
        let faction = "faction";
        assert!(event_for_intent(
            &json!({"faction":faction,"type":"sold","units":1}),
            "sell_one",
            faction
        ));
        assert!(!event_for_intent(
            &json!({"faction":faction,"type":"produced"}),
            "sell_one",
            faction
        ));
        assert!(!event_for_intent(
            &json!({"faction":"other","type":"sold","units":1}),
            "sell_one",
            faction
        ));
    }
}
