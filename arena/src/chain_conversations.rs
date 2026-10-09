//! Public, bounded barter journal for confirmed devnet events and explicit
//! bound-runner rule decisions. It is separate from private owner wishes.
use super::{
    chain_wishes, iso_utc, now, owner_auth, random_hex, save_snapshot_locked, sha256_hex, AppState,
};
use alashi_rules::anchor_lang::prelude::Pubkey;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

const MAX_GAMES: usize = 1000;
const MAX_ENTRIES: usize = 2000;
const MAX_IDEMPOTENCY: usize = 2000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub event_type: String,
    pub signature: String,
    pub slot: String,
    pub event_id: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationEntry {
    pub entry_id: String,
    pub seq: String,
    pub game_pda: String,
    pub kind: String,
    pub source: String,
    pub created_at: String,
    pub round: u8,
    pub author_faction_pda: String,
    pub proposer_faction_pda: String,
    pub counterparty_faction_pda: Option<String>,
    pub offer_id: String,
    pub goods: Option<u16>,
    pub price: Option<String>,
    pub in_reply_to: Option<String>,
    pub rule_code: Option<String>,
    pub receipt: Option<Receipt>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GameJournal {
    pub next_seq: u64,
    pub recording_started_at: Option<String>,
    pub entries: Vec<ConversationEntry>,
    /// record:client_entry_id -> (entry_id, request_hash); never public.
    pub idempotency: HashMap<String, (String, String)>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ConversationState {
    pub games: HashMap<String, GameJournal>,
}

fn pda_ok(raw: &str) -> bool {
    Pubkey::from_str(raw)
        .ok()
        .is_some_and(|p| p.to_string() == raw)
}
fn signature_ok(raw: &str) -> bool {
    (80..=100).contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".contains(&b))
}
fn decimal_u64(raw: &str) -> Option<u64> {
    let value = raw.parse::<u64>().ok()?;
    (value.to_string() == raw).then_some(value)
}
fn client_id_ok(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 96
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn error(code: &str) -> Value {
    json!({"ok":false,"error":code})
}

pub fn validate(state: &ConversationState) -> bool {
    if state.games.len() > MAX_GAMES {
        return false;
    }
    let mut total = 0;
    let mut idem_total = 0;
    for (game, journal) in &state.games {
        if !pda_ok(game) {
            return false;
        }
        total += journal.entries.len();
        idem_total += journal.idempotency.len();
        if total > MAX_ENTRIES || idem_total > MAX_IDEMPOTENCY {
            return false;
        }
        let mut previous = 0;
        let mut ids = HashSet::new();
        let mut events = HashSet::new();
        let mut prior = HashMap::<&str, &ConversationEntry>::new();
        for row in &journal.entries {
            let Some(seq) = decimal_u64(&row.seq) else {
                return false;
            };
            if seq <= previous
                || seq > journal.next_seq
                || row.game_pda != *game
                || row.round > 6
                || !pda_ok(&row.author_faction_pda)
                || !pda_ok(&row.proposer_faction_pda)
                || row
                    .counterparty_faction_pda
                    .as_deref()
                    .is_some_and(|p| !pda_ok(p))
                || decimal_u64(&row.offer_id).is_none()
                || !ids.insert(row.entry_id.as_str())
            {
                return false;
            }
            match row.kind.as_str() {
                "offer_confirmed"
                    if row.source == "onchain_event"
                        && row
                            .receipt
                            .as_ref()
                            .is_some_and(|r| r.event_type == "barter_proposed")
                        && row.author_faction_pda == row.proposer_faction_pda
                        && row.counterparty_faction_pda.is_none()
                        && row.goods.is_some()
                        && row.price.as_deref().and_then(decimal_u64).is_some()
                        && row.in_reply_to.is_none()
                        && row.rule_code.is_none() => {}
                "accepted_confirmed"
                    if row.source == "onchain_event"
                        && row
                            .receipt
                            .as_ref()
                            .is_some_and(|r| r.event_type == "barter_accepted")
                        && row.counterparty_faction_pda.as_deref()
                            == Some(row.author_faction_pda.as_str())
                        && row.in_reply_to.is_some()
                        && row.goods.is_none()
                        && row.price.is_none()
                        && row.rule_code.is_none() => {}
                "declined_rule"
                    if row.source == "runner_reported"
                        && row.receipt.is_none()
                        && row.counterparty_faction_pda.as_deref()
                            == Some(row.author_faction_pda.as_str())
                        && row.in_reply_to.is_some()
                        && row.goods.is_none()
                        && row.price.is_none()
                        && matches!(
                            row.rule_code.as_deref(),
                            Some(
                                "insufficient_goods"
                                    | "insufficient_cash"
                                    | "outside_policy"
                                    | "expired_offer"
                            )
                        ) => {}
                _ => return false,
            }
            if let Some(reply) = row.in_reply_to.as_deref() {
                let Some(offer) = prior.get(reply) else {
                    return false;
                };
                if offer.kind != "offer_confirmed"
                    || offer.offer_id != row.offer_id
                    || offer.proposer_faction_pda != row.proposer_faction_pda
                    || row.author_faction_pda == row.proposer_faction_pda
                {
                    return false;
                }
            }
            if let Some(receipt) = &row.receipt {
                if decimal_u64(&receipt.slot).is_none()
                    || !signature_ok(&receipt.signature)
                    || receipt
                        .event_id
                        .strip_prefix(&format!("{}:", receipt.signature))
                        .and_then(decimal_u64)
                        .is_none()
                    || !events.insert(receipt.event_id.as_str())
                {
                    return false;
                }
            }
            prior.insert(&row.entry_id, row);
            previous = seq;
        }
        if previous != journal.next_seq && !journal.entries.is_empty() {
            return false;
        }
        if journal
            .idempotency
            .values()
            .any(|(id, hash)| !ids.contains(id.as_str()) || hash.len() != 64)
        {
            return false;
        }
    }
    true
}

pub fn public_after(state: &AppState, game: &str, after: u64, limit: usize) -> Value {
    if !pda_ok(game) {
        return error("bad_game_pda");
    }
    if !(1..=100).contains(&limit) {
        return error("bad_limit");
    }
    let ledger = state
        .chain_conversations
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let Some(journal) = ledger.games.get(game) else {
        return json!({"ok":true,"schema":"alashi.chain_conversations.v1","game_pda":game,
            "entries":[],"next_cursor":after.to_string(),"latest_seq":"0","has_more":false,
            "history_complete":false,"recording_started_at":null});
    };
    let rows = journal
        .entries
        .iter()
        .filter(|e| decimal_u64(&e.seq).is_some_and(|s| s > after))
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let cursor = rows
        .last()
        .and_then(|e| decimal_u64(&e.seq))
        .unwrap_or(after);
    json!({"ok":true,"schema":"alashi.chain_conversations.v1","game_pda":game,
        "entries":rows,"next_cursor":cursor.to_string(),"latest_seq":journal.next_seq.to_string(),
        "has_more":journal.next_seq > cursor,"history_complete":false,
        "recording_started_at":journal.recording_started_at})
}

fn event_round(events: &[Value], id: &str) -> Option<u8> {
    let mut round = 0u8;
    for row in events {
        if row["type"] == "phase_advanced" {
            round = row["round"].as_u64().and_then(|r| u8::try_from(r).ok())?;
        }
        if row["id"] == id {
            return (round <= 6).then_some(round);
        }
    }
    None
}
fn chain_event<'a>(
    view: &'a Value,
    kind: &str,
    game: &str,
    offer: &str,
    signature: &str,
    slot: u64,
) -> Option<&'a Value> {
    let matches = view["events"]
        .as_array()?
        .iter()
        .filter(|e| {
            e["type"] == kind
                && e["game"] == game
                && e["offer"] == offer
                && e["signature"] == signature
                && e["slot"] == slot
        })
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0])
}
fn receipt_from(event: &Value, event_type: &str) -> Option<Receipt> {
    Some(Receipt {
        event_type: event_type.into(),
        signature: event["signature"].as_str()?.into(),
        slot: event["slot"].as_u64()?.to_string(),
        event_id: event["id"].as_str()?.into(),
    })
}
fn offer_row(game: &str, event: &Value, round: u8, at: &str) -> Option<ConversationEntry> {
    let from = event["from"].as_str()?;
    let offer = event["offer"].as_str()?;
    let goods = u16::try_from(event["goods"].as_u64()?).ok()?;
    let price = event["price"].as_str()?;
    if !pda_ok(from) || goods == 0 || decimal_u64(offer).is_none() || decimal_u64(price).is_none() {
        return None;
    }
    Some(ConversationEntry {
        entry_id: random_hex().ok()?,
        seq: String::new(),
        game_pda: game.into(),
        kind: "offer_confirmed".into(),
        source: "onchain_event".into(),
        created_at: at.into(),
        round,
        author_faction_pda: from.into(),
        proposer_faction_pda: from.into(),
        counterparty_faction_pda: None,
        offer_id: offer.into(),
        goods: Some(goods),
        price: Some(price.into()),
        in_reply_to: None,
        rule_code: None,
        receipt: Some(receipt_from(event, "barter_proposed")?),
    })
}
fn append(journal: &mut GameJournal, mut row: ConversationEntry) -> ConversationEntry {
    journal.next_seq += 1;
    row.seq = journal.next_seq.to_string();
    journal.entries.push(row.clone());
    row
}

pub fn runner_report(state: &AppState, game: &str, origin: &str, body: &Value) -> Value {
    if !owner_auth::origin_ok(origin) {
        return error("owner_origin_forbidden");
    }
    if !pda_ok(game) {
        return error("bad_game_pda");
    }
    let Some(obj) = body.as_object() else {
        return error("bad_conversation_report");
    };
    if !obj.keys().all(|k| {
        matches!(
            k.as_str(),
            "runner_token"
                | "client_entry_id"
                | "kind"
                | "offer_id"
                | "proposer_faction_pda"
                | "signature"
                | "slot"
                | "goods"
                | "price"
                | "in_reply_to"
                | "rule_code"
        )
    }) {
        return error("bad_conversation_report");
    }
    let (Some(token), Some(client_id), Some(kind), Some(offer), Some(proposer)) = (
        body["runner_token"].as_str(),
        body["client_entry_id"].as_str(),
        body["kind"].as_str(),
        body["offer_id"].as_str(),
        body["proposer_faction_pda"].as_str(),
    ) else {
        return error("bad_conversation_report");
    };
    if !client_id_ok(client_id)
        || decimal_u64(offer).is_none()
        || !pda_ok(proposer)
        || !matches!(
            kind,
            "offer_confirmed" | "accepted_confirmed" | "declined_rule"
        )
    {
        return error("bad_conversation_report");
    }
    let binding = {
        let ledger = state.chain_wishes.lock().unwrap_or_else(|e| e.into_inner());
        chain_wishes::authorize_runner(&ledger, game, token).cloned()
    };
    let Some(binding) = binding else {
        return error("runner_token_invalid");
    };
    let idem_key = format!("{}:{}", binding.record_id, client_id);
    let mut fingerprint = body.clone();
    fingerprint.as_object_mut().unwrap().remove("runner_token");
    let request_hash = sha256_hex(&fingerprint.to_string());
    {
        let ledger = state
            .chain_conversations
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(journal) = ledger.games.get(game) {
            if let Some((id, hash)) = journal.idempotency.get(&idem_key) {
                return if hash == &request_hash {
                    journal
                        .entries
                        .iter()
                        .find(|e| &e.entry_id == id)
                        .map_or_else(|| error("storage_failed"), |e| json!({"ok":true,"entry":e}))
                } else {
                    error("idempotency_conflict")
                };
            }
        }
    }
    let at = now();
    let created = iso_utc(at as u64);
    let mut candidate = Vec::<ConversationEntry>::new();
    let mut matched_offer_event: Option<String> = None;
    if kind == "declined_rule" {
        let view = match chain_wishes::public_chain(game) {
            Ok(v) => v,
            Err(code) => return error(code),
        };
        if view["game"]["epoch"] != 1
            || view["game"]["phase"] != "Market"
            || view["game"]["settled"] != false
            || !view["factions"].as_array().is_some_and(|rows| {
                rows.iter().any(|f| {
                    f["pda"] == binding.faction_pda && f["wallet"] == binding.faction_wallet
                })
            })
        {
            return error("binding_forbidden");
        }
        let (Some(reply), Some(rule)) = (body["in_reply_to"].as_str(), body["rule_code"].as_str())
        else {
            return error("bad_conversation_report");
        };
        if !matches!(
            rule,
            "insufficient_goods" | "insufficient_cash" | "outside_policy" | "expired_offer"
        ) || obj.contains_key("signature")
            || obj.contains_key("slot")
            || obj.contains_key("goods")
            || obj.contains_key("price")
        {
            return error("bad_conversation_report");
        }
        let ledger = state
            .chain_conversations
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(offer_row) = ledger.games.get(game).and_then(|j| {
            j.entries
                .iter()
                .find(|e| e.entry_id == reply && e.kind == "offer_confirmed")
        }) else {
            return error("unknown_offer");
        };
        if offer_row.offer_id != offer
            || offer_row.proposer_faction_pda != proposer
            || offer_row.proposer_faction_pda == binding.faction_pda
            || ledger.games.get(game).is_some_and(|j| {
                j.entries.iter().any(|e| {
                    e.in_reply_to.as_deref() == Some(reply)
                        && e.counterparty_faction_pda.as_deref()
                            == Some(binding.faction_pda.as_str())
                })
            })
        {
            return error("conversation_conflict");
        }
        let Ok(entry_id) = random_hex() else {
            return error("storage_failed");
        };
        candidate.push(ConversationEntry {
            entry_id,
            seq: String::new(),
            game_pda: game.into(),
            kind: kind.into(),
            source: "runner_reported".into(),
            created_at: created,
            round: offer_row.round,
            author_faction_pda: binding.faction_pda.clone(),
            proposer_faction_pda: proposer.into(),
            counterparty_faction_pda: Some(binding.faction_pda.clone()),
            offer_id: offer.into(),
            goods: None,
            price: None,
            in_reply_to: Some(reply.into()),
            rule_code: Some(rule.into()),
            receipt: None,
        });
    } else {
        if obj.contains_key("in_reply_to") || obj.contains_key("rule_code") {
            return error("bad_conversation_report");
        }
        let (Some(sig), Some(slot_raw)) = (body["signature"].as_str(), body["slot"].as_str())
        else {
            return error("bad_conversation_report");
        };
        let Some(slot) = decimal_u64(slot_raw) else {
            return error("bad_conversation_report");
        };
        if !signature_ok(sig) {
            return error("bad_conversation_report");
        }
        let view = match chain_wishes::public_chain(game) {
            Ok(v) => v,
            Err(code) => return error(code),
        };
        if view["game"]["epoch"] != 1 {
            return error("wrong_epoch");
        }
        if view["history_complete"] != true {
            return error("receipt_pending");
        }
        let Some(events) = view["events"].as_array() else {
            return error("chain_api_unavailable");
        };
        let factions = view["factions"].as_array();
        if !factions.is_some_and(|rows| {
            rows.iter()
                .any(|f| f["pda"] == binding.faction_pda && f["wallet"] == binding.faction_wallet)
                && rows.iter().any(|f| f["pda"] == proposer)
        }) {
            return error("binding_forbidden");
        }
        if kind == "offer_confirmed" {
            if proposer != binding.faction_pda
                || obj.contains_key("goods") != obj.contains_key("price")
            {
                return error("binding_forbidden");
            }
            let Some(event) = chain_event(&view, "barter_proposed", game, offer, sig, slot) else {
                return error("receipt_pending");
            };
            if event["from"] != proposer
                || obj.get("goods").is_some_and(|v| v != &event["goods"])
                || obj.get("price").is_some_and(|v| v != &event["price"])
            {
                return error("receipt_mismatch");
            }
            let Some(round) = event["id"].as_str().and_then(|id| event_round(events, id)) else {
                return error("receipt_pending");
            };
            let Some(row) = offer_row(game, event, round, &created) else {
                return error("receipt_mismatch");
            };
            candidate.push(row);
        } else {
            if proposer == binding.faction_pda
                || obj.contains_key("goods")
                || obj.contains_key("price")
            {
                return error("binding_forbidden");
            }
            let Some(accepted) = chain_event(&view, "barter_accepted", game, offer, sig, slot)
            else {
                return error("receipt_pending");
            };
            if accepted["by"] != binding.faction_pda || accepted["from"] != proposer {
                return error("receipt_mismatch");
            }
            let Some(accepted_pos) = events.iter().position(|e| e["id"] == accepted["id"]) else {
                return error("receipt_pending");
            };
            let offers = events[..accepted_pos]
                .iter()
                .filter(|e| {
                    e["type"] == "barter_proposed"
                        && e["game"] == game
                        && e["offer"] == offer
                        && e["from"] == proposer
                })
                .collect::<Vec<_>>();
            if offers.len() != 1 {
                return error("receipt_pending");
            }
            let offer_event = offers[0];
            let Some(offer_event_id) = offer_event["id"].as_str() else {
                return error("receipt_mismatch");
            };
            let Some(offer_round) = event_round(events, offer_event_id) else {
                return error("receipt_pending");
            };
            let Some(accept_round) = accepted["id"]
                .as_str()
                .and_then(|id| event_round(events, id))
            else {
                return error("receipt_pending");
            };
            let Some(row) = offer_row(game, offer_event, offer_round, &created) else {
                return error("receipt_mismatch");
            };
            matched_offer_event = Some(offer_event_id.into());
            candidate.push(row);
            let Some(accepted_receipt) = receipt_from(accepted, "barter_accepted") else {
                return error("receipt_mismatch");
            };
            let Ok(entry_id) = random_hex() else {
                return error("storage_failed");
            };
            candidate.push(ConversationEntry {
                entry_id,
                seq: String::new(),
                game_pda: game.into(),
                kind: kind.into(),
                source: "onchain_event".into(),
                created_at: created,
                round: accept_round,
                author_faction_pda: binding.faction_pda.clone(),
                proposer_faction_pda: proposer.into(),
                counterparty_faction_pda: Some(binding.faction_pda.clone()),
                offer_id: offer.into(),
                goods: None,
                price: None,
                in_reply_to: None,
                rule_code: None,
                receipt: Some(accepted_receipt),
            });
        }
    }
    let _tx = state
        .snapshot_lock
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let mut ledger = state
        .chain_conversations
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let before = ledger.clone();
    let total = ledger
        .games
        .values()
        .map(|j| j.entries.len())
        .sum::<usize>();
    if total + candidate.len() > MAX_ENTRIES
        || ledger.games.len() >= MAX_GAMES && !ledger.games.contains_key(game)
    {
        return error("journal_full");
    }
    let journal = ledger.games.entry(game.into()).or_default();
    if let Some((id, hash)) = journal.idempotency.get(&idem_key) {
        return if hash == &request_hash {
            journal
                .entries
                .iter()
                .find(|e| &e.entry_id == id)
                .map_or_else(|| error("storage_failed"), |e| json!({"ok":true,"entry":e}))
        } else {
            error("idempotency_conflict")
        };
    }
    if kind == "declined_rule" {
        let reply = candidate[0].in_reply_to.as_deref().unwrap();
        if journal.entries.iter().any(|e| {
            e.in_reply_to.as_deref() == Some(reply)
                && e.counterparty_faction_pda.as_deref() == Some(binding.faction_pda.as_str())
        }) {
            return error("conversation_conflict");
        }
    }
    if let Some(offer_event_id) = matched_offer_event {
        if let Some(existing) = journal.entries.iter().find(|e| {
            e.receipt
                .as_ref()
                .is_some_and(|r| r.event_id == offer_event_id)
        }) {
            candidate[1].in_reply_to = Some(existing.entry_id.clone());
            candidate.remove(0);
        } else {
            candidate[1].in_reply_to = Some(candidate[0].entry_id.clone());
        }
    }
    if candidate
        .iter()
        .filter_map(|e| e.receipt.as_ref())
        .any(|r| {
            journal.entries.iter().any(|e| {
                e.receipt
                    .as_ref()
                    .is_some_and(|old| old.event_id == r.event_id)
            })
        })
    {
        return error("conversation_conflict");
    }
    if journal.recording_started_at.is_none() {
        journal.recording_started_at = Some(iso_utc(at as u64));
    }
    let mut result = None;
    for row in candidate {
        result = Some(append(journal, row));
    }
    let row = result.unwrap();
    journal
        .idempotency
        .insert(idem_key, (row.entry_id.clone(), request_hash));
    drop(ledger);
    if save_snapshot_locked(state).is_err() {
        *state
            .chain_conversations
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = before;
        return error("storage_failed");
    }
    json!({"ok":true,"entry":row})
}
