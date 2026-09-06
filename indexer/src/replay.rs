//! Version 2 exports recorded events without inventing reconstructed states.
//! The historical event stream omits epoch/configuration changes and entropy.
//! It is insufficient to verify a complete rules replay. Direct program/rules
//! replay-equivalence tests remain separate from this archival export.
use crate::events::ParsedEvent;
use serde_json::json;
use std::collections::BTreeMap;

pub struct ReplayOutcome {
    pub record: serde_json::Value,
}

/// Kept as the Rust entry point for existing callers; output is an event archive.
pub fn replay(events: &[ParsedEvent], registry: &[(String, String)]) -> Option<ReplayOutcome> {
    let (game, game_id, entry_fee) = events.iter().find_map(|event| match event {
        ParsedEvent::GameInitialized { game, game_id, entry_fee, .. } => Some((game, *game_id, *entry_fee)),
        _ => None,
    })?;
    if events.iter().any(|event| event.game() != game) { return None; }
    let wallet_to_agent: BTreeMap<_, _> = registry.iter().cloned().collect();
    let ranks: Vec<_> = events.iter().filter_map(|event| match event {
        ParsedEvent::Payout { wallet, rank, amount, .. } => Some(json!({
            "rank": *rank as u16 + 1,
            "wallet": wallet,
            "agent_id": wallet_to_agent.get(wallet).unwrap_or(wallet),
            "payout": amount,
        })),
        _ => None,
    }).collect();
    let settlement = events.iter().rev().find_map(|event| match event {
        ParsedEvent::Settled { pot, rake, paid, .. } => Some(json!({"pot":pot,"rake":rake,"paid":paid})),
        _ => None,
    })?;
    let names: Vec<_> = events.iter().filter_map(|event| match event {
        ParsedEvent::FactionJoined { name, .. } => Some(name),
        _ => None,
    }).collect();
    Some(ReplayOutcome { record: json!({
        "schema_version": 2,
        "kind": "event_export",
        "replay_verified": false,
        "replay_limitation": "Events omit configuration and entropy required to reconstruct all game states. Payouts are recorded events, not independently recomputed outcomes.",
        "game": game,
        "game_id": game_id,
        "entry_fee": entry_fee,
        "names": names,
        "events": events,
        "ranks": ranks,
        "settled": true,
        "settlement": settlement,
    }) })
}
