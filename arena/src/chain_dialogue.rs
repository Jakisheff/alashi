//! Off-chain public runner speech. No transaction receipt or confirmed-text claim.
use super::{
    chain_wishes, iso_utc, now, owner_auth, random_hex, save_snapshot_locked, sha256_hex, AppState,
};
use alashi_rules::anchor_lang::prelude::Pubkey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;
const MAX_GAMES: usize = 1000;
const MAX_ROWS: usize = 2000;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogueEntry {
    pub message_id: String,
    pub seq: String,
    pub game_pda: String,
    pub author_faction_pda: String,
    pub to_faction_pda: String,
    pub text: String,
    pub reply_to_message_id: Option<String>,
    pub related_offer_entry_id: Option<String>,
    pub round: u8,
    pub phase: String,
    pub phase_ends_at: i64,
    pub created_at: String,
    pub observed_chain_event_id: Option<String>,
    pub source: String,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameJournal {
    pub next_seq: u64,
    pub recording_started_at: Option<String>,
    pub entries: Vec<DialogueEntry>,
    pub idempotency: HashMap<String, (String, String)>,
    pub accepted_at: HashMap<String, i64>,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DialogueState {
    pub games: HashMap<String, GameJournal>,
}
fn pda(s: &str) -> bool {
    Pubkey::from_str(s).ok().is_some_and(|p| p.to_string() == s)
}
fn hex(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn client(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
fn decimal(s: &str) -> Option<u64> {
    let n = s.parse::<u64>().ok()?;
    (n.to_string() == s).then_some(n)
}
fn text(s: &str) -> bool {
    !s.trim().is_empty() && s.chars().count() <= 240 && !s.chars().any(char::is_control)
}
fn phase(s: &str) -> bool {
    matches!(s, "Market" | "Action" | "Law")
}
fn error(s: &str) -> Value {
    json!({"ok":false,"error":s})
}
fn anchor(s: &str) -> bool {
    let Some((signature, index)) = s.rsplit_once(':') else {
        return false;
    };
    signature.parse::<solana_signature::Signature>().is_ok() && decimal(index).is_some()
}
pub fn validate(state: &DialogueState) -> bool {
    if state.games.len() > MAX_GAMES {
        return false;
    }
    let mut total = 0;
    for (game, journal) in &state.games {
        total += journal.entries.len();
        if !pda(game)
            || total > MAX_ROWS
            || journal.idempotency.len() != journal.entries.len()
            || journal.accepted_at.len() != journal.entries.len()
        {
            return false;
        }
        let mut ids = HashSet::new();
        let mut prior = HashMap::<&str, &DialogueEntry>::new();
        let mut counts = HashMap::<(&str, u8, &str, i64), usize>::new();
        let mut authors = HashMap::<&str, (usize, i64)>::new();
        for (i, row) in journal.entries.iter().enumerate() {
            let Some(at) = journal.accepted_at.get(&row.message_id).copied() else {
                return false;
            };
            if decimal(&row.seq) != Some(i as u64 + 1)
                || !hex(&row.message_id)
                || !ids.insert(row.message_id.as_str())
                || row.game_pda != *game
                || !pda(&row.author_faction_pda)
                || !pda(&row.to_faction_pda)
                || row.author_faction_pda == row.to_faction_pda
                || !text(&row.text)
                || !(1..=6).contains(&row.round)
                || !phase(&row.phase)
                || at < 0
                || row.phase_ends_at <= at
                || row.created_at != iso_utc(at as u64)
                || row.source != "runner_reported"
                || row
                    .observed_chain_event_id
                    .as_deref()
                    .is_some_and(|s| !anchor(s))
                || row
                    .related_offer_entry_id
                    .as_deref()
                    .is_some_and(|s| !hex(s))
            {
                return false;
            }
            if let Some(id) = row.reply_to_message_id.as_deref() {
                let Some(parent) = prior.get(id) else {
                    return false;
                };
                if parent.author_faction_pda != row.to_faction_pda
                    || parent.to_faction_pda != row.author_faction_pda
                {
                    return false;
                }
            }
            let count = counts
                .entry((
                    &row.author_faction_pda,
                    row.round,
                    &row.phase,
                    row.phase_ends_at,
                ))
                .or_default();
            *count += 1;
            if *count > 4 {
                return false;
            }
            let author = authors.entry(&row.author_faction_pda).or_insert((0, -5));
            if at.saturating_sub(author.1) < 5 {
                return false;
            }
            author.0 += 1;
            author.1 = at;
            if author.0 > 24 {
                return false;
            }
            prior.insert(&row.message_id, row);
        }
        if journal.next_seq != journal.entries.len() as u64
            || journal.recording_started_at.as_deref()
                != journal.entries.first().map(|r| r.created_at.as_str())
        {
            return false;
        }
        let mut referenced = HashSet::new();
        for (key, (id, hash)) in &journal.idempotency {
            let Some((record, client_id)) = key.split_once(':') else {
                return false;
            };
            if !hex(record)
                || !client(client_id)
                || !ids.contains(id.as_str())
                || !referenced.insert(id)
                || !hex(hash)
            {
                return false;
            }
        }
    }
    true
}
fn retry(j: &GameJournal, key: &str, hash: &str) -> Option<Value> {
    j.idempotency.get(key).map(|(id, old)| {
        if old != hash {
            error("idempotency_conflict")
        } else {
            j.entries.iter().find(|e| &e.message_id == id).map_or_else(
                || error("storage_failed"),
                |e| json!({"ok":true,"message":e}),
            )
        }
    })
}
pub fn public_after(state: &AppState, game: &str, after: u64, limit: usize) -> Value {
    if !pda(game) {
        return error("bad_game_pda");
    }
    if !(1..=100).contains(&limit) {
        return error("bad_limit");
    }
    let _committed = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let ledger = state
        .chain_dialogue
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(j) = ledger.games.get(game) else {
        if after > 0 {
            return error("cursor_ahead");
        }
        return json!({"ok":true,"schema":"alashi.chain_dialogue.v1","game_pda":game,"entries":[],"next_cursor":after.to_string(),"latest_seq":"0","has_more":false,"history_complete":false,"recording_started_at":null});
    };
    if after > j.next_seq {
        return error("cursor_ahead");
    }
    let rows: Vec<_> = j
        .entries
        .iter()
        .filter(|r| decimal(&r.seq).is_some_and(|n| n > after))
        .take(limit)
        .collect();
    let cursor = rows.last().and_then(|r| decimal(&r.seq)).unwrap_or(after);
    json!({"ok":true,"schema":"alashi.chain_dialogue.v1","game_pda":game,"entries":rows,"next_cursor":cursor.to_string(),"latest_seq":j.next_seq.to_string(),"has_more":cursor<j.next_seq,"history_complete":false,"recording_started_at":j.recording_started_at})
}
pub fn runner_report(state: &AppState, game: &str, origin: &str, body: &Value) -> Value {
    if !owner_auth::origin_ok(origin) {
        return error("owner_origin_forbidden");
    }
    if !pda(game) {
        return error("bad_game_pda");
    }
    let Some(obj) = body.as_object() else {
        return error("bad_dialogue_report");
    };
    let required = [
        "runner_token",
        "client_message_id",
        "expected_round",
        "expected_phase",
        "expected_phase_ends_at",
        "to_faction_pda",
        "text",
    ];
    if required.iter().any(|k| !obj.contains_key(*k))
        || obj.keys().any(|k| {
            !required.contains(&k.as_str())
                && !["reply_to_message_id", "related_offer_entry_id"].contains(&k.as_str())
        })
    {
        return error("bad_dialogue_report");
    }
    let (
        Some(token),
        Some(id),
        Some(to),
        Some(words),
        Some(round),
        Some(expected_phase),
        Some(deadline),
    ) = (
        body["runner_token"].as_str(),
        body["client_message_id"].as_str(),
        body["to_faction_pda"].as_str(),
        body["text"].as_str(),
        body["expected_round"].as_u64(),
        body["expected_phase"].as_str(),
        body["expected_phase_ends_at"].as_i64(),
    )
    else {
        return error("bad_dialogue_report");
    };
    if !client(id)
        || !pda(to)
        || !text(words)
        || !(1..=6).contains(&round)
        || !phase(expected_phase)
        || deadline <= 0
    {
        return error("bad_dialogue_report");
    }
    for key in ["reply_to_message_id", "related_offer_entry_id"] {
        if obj.get(key).is_some_and(|v| !v.as_str().is_some_and(hex)) {
            return error("bad_dialogue_report");
        }
    }
    let binding = {
        let l = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
        chain_wishes::authorize_runner(&l, game, token).cloned()
    };
    let Some(binding) = binding else {
        return error("runner_token_invalid");
    };
    if to == binding.faction_pda {
        return error("binding_forbidden");
    }
    let key = format!("{}:{id}", binding.record_id);
    let mut fingerprint = body.clone();
    fingerprint.as_object_mut().unwrap().remove("runner_token");
    let hash = sha256_hex(&fingerprint.to_string());
    {
        let _committed = state
            .snapshot_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let l = state
            .chain_dialogue
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(v) = l.games.get(game).and_then(|j| retry(j, &key, &hash)) {
            return v;
        }
    }
    let view = match chain_wishes::public_chain(game) {
        Ok(v) => v,
        Err(c) => return error(c),
    };
    if view["game"]["epoch"] != 1 || view["game"]["settled"] != false {
        return error("binding_forbidden");
    }
    if view["game"]["round"].as_u64() != Some(round)
        || view["game"]["phase"].as_str() != Some(expected_phase)
        || view["game"]["phase_ends_at"].as_i64() != Some(deadline)
        || now() >= deadline
    {
        return error("stale_context");
    }
    if !view["factions"].as_array().is_some_and(|fs| {
        fs.iter().any(|f| {
            f["pda"] == binding.faction_pda
                && f["wallet"] == binding.faction_wallet
                && f["alive"] == true
        }) && fs.iter().any(|f| f["pda"] == to && f["alive"] == true)
    }) {
        return error("binding_forbidden");
    }
    let observed = view["events"]
        .as_array()
        .and_then(|events| events.last())
        .and_then(|e| e["id"].as_str())
        .map(String::from);
    if observed.as_deref().is_some_and(|s| !anchor(s)) {
        return error("chain_api_unavailable");
    }
    let reply = body["reply_to_message_id"].as_str();
    let related = body["related_offer_entry_id"].as_str();
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // Bind again after upstream I/O, before retaining any text.
    {
        let l = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
        if !chain_wishes::authorize_runner(&l, game, token).is_some_and(|b| {
            b.record_id == binding.record_id
                && b.faction_pda == binding.faction_pda
                && b.faction_wallet == binding.faction_wallet
        }) {
            return error("runner_token_invalid");
        }
    }
    let mut ledger = state
        .chain_dialogue
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(v) = ledger.games.get(game).and_then(|j| retry(j, &key, &hash)) {
        return v;
    }
    let at = now();
    if at >= deadline {
        return error("stale_context");
    }
    if let Some(offer_id) = related {
        let c = state
            .chain_conversations
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !c.games.get(game).is_some_and(|j| {
            j.entries
                .iter()
                .any(|e| e.entry_id == offer_id && e.kind == "offer_confirmed")
        }) {
            return error("unknown_offer");
        }
    }
    if let Some(parent) = reply {
        if !ledger.games.get(game).is_some_and(|j| {
            j.entries.iter().any(|e| {
                e.message_id == parent
                    && e.author_faction_pda == to
                    && e.to_faction_pda == binding.faction_pda
            })
        }) {
            return error("unknown_reply");
        }
    }
    if ledger
        .games
        .values()
        .map(|j| j.entries.len())
        .sum::<usize>()
        >= MAX_ROWS
        || (!ledger.games.contains_key(game) && ledger.games.len() >= MAX_GAMES)
    {
        return error("journal_full");
    }
    if let Some(j) = ledger.games.get(game) {
        let own: Vec<_> = j
            .entries
            .iter()
            .filter(|e| e.author_faction_pda == binding.faction_pda)
            .collect();
        if own.len() >= 24
            || own
                .iter()
                .filter(|e| {
                    e.round == round as u8
                        && e.phase == expected_phase
                        && e.phase_ends_at == deadline
                })
                .count()
                >= 4
        {
            return error("dialogue_quota_exhausted");
        }
        if own
            .last()
            .and_then(|e| j.accepted_at.get(&e.message_id))
            .is_some_and(|last| at.saturating_sub(*last) < 5)
        {
            return error("dialogue_rate_limited");
        }
    }
    let Ok(message_id) = random_hex() else {
        return error("storage_failed");
    };
    let before = ledger.clone();
    let j = ledger.games.entry(game.into()).or_default();
    let Some(seq) = j.next_seq.checked_add(1) else {
        return error("journal_full");
    };
    let created_at = iso_utc(at as u64);
    let row = DialogueEntry {
        message_id: message_id.clone(),
        seq: seq.to_string(),
        game_pda: game.into(),
        author_faction_pda: binding.faction_pda,
        to_faction_pda: to.into(),
        text: words.into(),
        reply_to_message_id: reply.map(String::from),
        related_offer_entry_id: related.map(String::from),
        round: round as u8,
        phase: expected_phase.into(),
        phase_ends_at: deadline,
        created_at: created_at.clone(),
        observed_chain_event_id: observed,
        source: "runner_reported".into(),
    };
    j.next_seq = seq;
    if j.recording_started_at.is_none() {
        j.recording_started_at = Some(created_at);
    }
    j.entries.push(row.clone());
    j.accepted_at.insert(message_id.clone(), at);
    j.idempotency.insert(key, (message_id, hash));
    drop(ledger);
    if save_snapshot_locked(state).is_err() {
        *state
            .chain_dialogue
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = before;
        return error("storage_failed");
    }
    json!({"ok":true,"message":row})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_empty_snapshot_and_text_bounds() {
        assert!(validate(&DialogueState::default()));
        assert!(serde_json::from_value::<DialogueState>(json!({"games":{},"private":1})).is_err());
        assert!(serde_json::from_value::<DialogueState>(json!({})).is_err());
        assert!(text(&"é".repeat(240)));
        assert!(!text(&"é".repeat(241)));
        assert!(!text("hello\nsecret"));
    }
    #[test]
    fn retry_does_not_depend_on_current_phase() {
        let mut j = GameJournal::default();
        j.idempotency
            .insert("record:client".into(), ("missing".into(), "hash".into()));
        assert_eq!(
            retry(&j, "record:client", "changed").unwrap()["error"],
            "idempotency_conflict"
        );
        assert_eq!(
            retry(&j, "record:client", "hash").unwrap()["error"],
            "storage_failed"
        );
    }
    fn sample(rows: usize) -> DialogueState {
        let game = Pubkey::new_from_array([3; 32]).to_string();
        let mut j = GameJournal::default();
        for i in 0..rows {
            let id = format!("{:064x}", i + 1);
            let at = 100 + i as i64 * 5;
            let r = DialogueEntry {
                message_id: id.clone(),
                seq: (i + 1).to_string(),
                game_pda: game.clone(),
                author_faction_pda: Pubkey::new_from_array([1; 32]).to_string(),
                to_faction_pda: Pubkey::new_from_array([2; 32]).to_string(),
                text: "Public words".into(),
                reply_to_message_id: None,
                related_offer_entry_id: None,
                round: (i / 4 + 1) as u8,
                phase: "Market".into(),
                phase_ends_at: 1000,
                created_at: iso_utc(at as u64),
                observed_chain_event_id: None,
                source: "runner_reported".into(),
            };
            if j.recording_started_at.is_none() {
                j.recording_started_at = Some(r.created_at.clone());
            }
            j.accepted_at.insert(id.clone(), at);
            j.idempotency
                .insert(format!("{}:m{i}", "a".repeat(64)), (id, "b".repeat(64)));
            j.entries.push(r);
        }
        j.next_seq = rows as u64;
        DialogueState {
            games: HashMap::from([(game, j)]),
        }
    }
    #[test]
    fn snapshot_roundtrip_and_semantic_limits() {
        let valid = sample(24);
        assert!(validate(&valid));
        let restored: DialogueState =
            serde_json::from_value(serde_json::to_value(&valid).unwrap()).unwrap();
        assert!(validate(&restored));
        assert!(!validate(&sample(25)));
        let mut five = sample(5);
        five.games.values_mut().next().unwrap().entries[4].round = 1;
        assert!(!validate(&five));
        let mut malformed = sample(1);
        malformed.games.values_mut().next().unwrap().entries[0].seq = "0".into();
        assert!(!validate(&malformed));
        let mut empty = DialogueState::default();
        empty.games.insert(
            Pubkey::new_from_array([3; 32]).to_string(),
            GameJournal {
                next_seq: 1,
                ..Default::default()
            },
        );
        assert!(!validate(&empty));
    }
    #[test]
    fn public_read_waits_for_snapshot_commit_and_never_observes_rollback() {
        use std::sync::mpsc;
        use std::time::Duration;
        // Constructor creates no state files; this test does not save or start a server.
        let dir =
            std::env::temp_dir().join(format!("alashi-dialogue-gate-{}", random_hex().unwrap()));
        let state = super::super::new_state_with_files(dir.join("state.json"), dir.join("seq.txt"));
        let game = Pubkey::new_from_array([3; 32]).to_string();
        assert_eq!(public_after(&state, &game, 1, 100)["error"], "cursor_ahead");
        let guard = state.snapshot_lock.lock().unwrap();
        *state.chain_dialogue.lock().unwrap() = sample(1);
        let reader = state.clone();
        let requested_game = game.clone();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            done_tx
                .send(public_after(&reader, &requested_game, 0, 100))
                .unwrap();
        });
        ready_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(done_rx.recv_timeout(Duration::from_millis(100)).is_err());
        *state.chain_dialogue.lock().unwrap() = DialogueState::default();
        drop(guard);
        let committed = done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(committed["entries"], json!([]));
        thread.join().unwrap();
    }
}
