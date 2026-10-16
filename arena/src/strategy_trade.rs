//! Trading evidence shared by strategy runs and the existing game APIs.
use super::{chain_wishes, now, phase_name, state_json, AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Participant {
    pub wallet: String,
    pub actor: String,
    pub cash: String,
    pub goods: u16,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Context {
    pub mode: String,
    pub game: String,
    pub round: u8,
    pub phase: String,
    pub phase_ends_at: i64,
    pub captured_at: i64,
    pub after: u64,
    pub participants: Vec<Participant>,
    pub observation: Value,
}
impl Context {
    pub fn valid(&self) -> bool {
        matches!(self.mode.as_str(), "http" | "solana")
            && !self.game.is_empty()
            && self.game.len() <= 64
            && (1..=6).contains(&self.round)
            && matches!(self.phase.as_str(), "market" | "action")
            && self.captured_at > 0
            && self.phase_ends_at > self.captured_at
            && self.participants.len() == 2
            && self
                .participants
                .iter()
                .map(|p| &p.wallet)
                .collect::<BTreeSet<_>>()
                .len()
                == 2
            && self
                .participants
                .iter()
                .map(|p| &p.actor)
                .collect::<BTreeSet<_>>()
                .len()
                == 2
            && self.participants.iter().all(|p| {
                !p.wallet.is_empty()
                    && p.wallet.len() <= 64
                    && !p.actor.is_empty()
                    && p.actor.len() <= 64
                    && p.cash.parse::<u64>().is_ok()
            })
            && self.participants[0].cash == self.participants[1].cash
            && self.participants[0].goods == self.participants[1].goods
            && self.observation.is_object()
            && self.observation.to_string().len() <= 32000
    }
}
pub fn capture(state: &AppState, request: &Value) -> Result<Context, &'static str> {
    if request
        .as_object()
        .is_none_or(|o| o.len() != 2 || !o.contains_key("mode") || !o.contains_key("game"))
    {
        return Err("bad_trading_request");
    }
    let mode = request["mode"].as_str().unwrap_or("");
    let game = request["game"].as_str().ok_or("bad_trading_request")?;
    let context = if mode == "http" {
        let id: u64 = game.parse().map_err(|_| "bad_trading_request")?;
        if id.to_string() != game {
            return Err("bad_trading_request");
        }
        let _tx = state
            .snapshot_lock
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        let entry = games.get(&id).ok_or("unknown_game")?;
        if entry.agents.len() != 2 || entry.sim.factions.len() != 2 {
            return Err("two_registered_owners_required");
        }
        let registrations = state
            .registrations
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut participants = Vec::new();
        for agent in &entry.agents {
            let record = agent
                .agent_record_id
                .as_ref()
                .and_then(|id| registrations.get(id))
                .ok_or("two_registered_owners_required")?;
            let faction = &entry.sim.factions[agent.faction_idx];
            if !faction.alive || faction.acted_stamp == entry.sim.game.stamp() {
                return Err("trading_window_unavailable");
            }
            participants.push(Participant {
                wallet: record.wallet.clone(),
                actor: agent.faction_idx.to_string(),
                cash: faction.cash.to_string(),
                goods: faction.goods,
            });
        }
        let after = state
            .live
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .next_seq;
        Context {
            mode: mode.into(),
            game: game.into(),
            round: entry.sim.game.round,
            phase: phase_name(entry.sim.game.phase).into(),
            phase_ends_at: entry.sim.game.phase_ends_at,
            captured_at: now(),
            after,
            participants,
            observation: state_json(id, entry),
        }
    } else if mode == "solana" {
        capture_chain(game, &chain_wishes::public_chain(game)?)?
    } else {
        return Err("bad_trading_request");
    };
    if !context.valid() {
        return Err("equal_active_trading_conditions_required");
    }
    if context.phase_ends_at - now() < 10 {
        return Err("trading_window_unavailable");
    }
    Ok(context)
}
pub fn capture_chain(game: &str, view: &Value) -> Result<Context, &'static str> {
    if view["ok"] != true
        || view["cluster"] != "devnet"
        || view["game_pda"] != game
        || view["program_id"] != "3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC"
        || view["commitment"] != "confirmed"
        || view["game"]["settled"] != false
        || view["market"].as_object().is_none()
    {
        return Err("trading_snapshot_unavailable");
    }
    let rows = view["factions"]
        .as_array()
        .ok_or("trading_snapshot_unavailable")?;
    if rows.len() != 2 {
        return Err("two_registered_owners_required");
    }
    let mut participants = Vec::new();
    for f in rows {
        if f["alive"] != true || f["acted"] != false {
            return Err("trading_window_unavailable");
        }
        participants.push(Participant {
            wallet: f["wallet"]
                .as_str()
                .ok_or("trading_snapshot_unavailable")?
                .into(),
            actor: f["pda"]
                .as_str()
                .ok_or("trading_snapshot_unavailable")?
                .into(),
            cash: f["cash"]
                .as_str()
                .ok_or("trading_snapshot_unavailable")?
                .into(),
            goods: u16::try_from(f["goods"].as_u64().ok_or("trading_snapshot_unavailable")?)
                .map_err(|_| "trading_snapshot_unavailable")?,
        });
    }
    Ok(Context {
        mode: "solana".into(),
        game: game.into(),
        round: u8::try_from(
            view["game"]["round"]
                .as_u64()
                .ok_or("trading_snapshot_unavailable")?,
        )
        .map_err(|_| "trading_snapshot_unavailable")?,
        phase: view["game"]["phase"]
            .as_str()
            .unwrap_or("")
            .to_ascii_lowercase(),
        phase_ends_at: view["game"]["phase_ends_at"]
            .as_i64()
            .ok_or("trading_snapshot_unavailable")?,
        captured_at: now(),
        after: view["snapshot_slot"]
            .as_u64()
            .ok_or("trading_snapshot_unavailable")?,
        participants,
        observation: json!({"game":view["game"],"factions":view["factions"],"market":view["market"]}),
    })
}
pub fn checks(context: &Context, artifact: &Value) -> Value {
    let action = artifact["action"].as_str().unwrap_or("");
    let schema = artifact
        .as_object()
        .is_some_and(|o| o.len() == 2 && o.contains_key("action") && o.contains_key("params"));
    let allowed = if context.phase == "market" {
        matches!(action, "sell" | "buy")
    } else {
        action == "produce"
    };
    let params = artifact["params"].as_object().is_some_and(|p| {
        if action == "produce" {
            p.is_empty()
        } else {
            p.len() == 1
                && p.get("units")
                    .and_then(Value::as_u64)
                    .is_some_and(|n| (1..=65535).contains(&n))
        }
    });
    json!({"schema":schema,"phase_action":allowed,"params":params,"passed":schema&&allowed&&params,"limit":"decision_format_only_until_a_matching_game_receipt_is_confirmed"})
}
pub fn http_receipt(
    context: &Context,
    wallet: &str,
    decision: &Value,
    event: &Value,
) -> Result<Value, &'static str> {
    let actor = context
        .participants
        .iter()
        .find(|p| p.wallet == wallet)
        .ok_or("trading_owner_forbidden")?;
    let action = &event["action_ref"];
    if context.mode != "http"
        || event["game_id"].as_u64().map(|v| v.to_string()).as_deref()
            != Some(context.game.as_str())
        || event["kind"] != "game_action"
        || event["finality"] != "final"
        || event["visibility"] != "public"
        || event["seq"].as_u64().is_none_or(|seq| seq <= context.after)
        || event["round"] != context.round
        || event["phase"] != context.phase
        || action["actor"].as_u64().map(|v| v.to_string()).as_deref() != Some(actor.actor.as_str())
        || action["action"] != decision["action"]
        || action["params"] != decision["params"]
    {
        return Err("trade_receipt_mismatch");
    }
    let change = action["result"]["changes"]
        .as_array()
        .and_then(|rows| rows.iter().find(|r| r["actor"] == action["actor"]))
        .ok_or("trade_receipt_mismatch")?;
    let cash_before = change["cash_before"]
        .as_u64()
        .ok_or("trade_receipt_mismatch")?;
    let cash_after = change["cash_after"]
        .as_u64()
        .ok_or("trade_receipt_mismatch")?;
    let goods_before = change["goods_before"]
        .as_u64()
        .ok_or("trade_receipt_mismatch")?;
    let goods_after = change["goods_after"]
        .as_u64()
        .ok_or("trade_receipt_mismatch")?;
    Ok(
        json!({"mode":"http","game":context.game,"event_id":event["event_id"],"actor":actor.actor,"action":decision["action"],"params":decision["params"],"cash_delta":(cash_after as i128-cash_before as i128).to_string(),"goods_delta":goods_after as i128-goods_before as i128,"confirmed_at":event["created_at_epoch"],"source":"server_accepted_rule_action","event":event}),
    )
}
pub fn chain_receipt(
    context: &Context,
    wallet: &str,
    decision: &Value,
    view: &Value,
    signature: &str,
) -> Result<Value, &'static str> {
    let participant = context
        .participants
        .iter()
        .find(|p| p.wallet == wallet)
        .ok_or("trading_owner_forbidden")?;
    if context.mode != "solana"
        || view["ok"] != true
        || view["cluster"] != "devnet"
        || view["game_pda"] != context.game
        || view["program_id"] != "3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC"
        || view["commitment"] != "confirmed"
        || view["history_complete"] != true
    {
        return Err("trade_receipt_pending");
    }
    if !view["factions"].as_array().is_some_and(|fs| {
        fs.iter()
            .any(|f| f["pda"] == participant.actor && f["wallet"] == wallet)
    }) {
        return Err("trade_receipt_mismatch");
    }
    let events = view["events"].as_array().ok_or("trade_receipt_pending")?;
    let expected = match decision["action"].as_str() {
        Some("sell") => "sold",
        Some("buy") => "goods_bought",
        Some("produce") => "produced",
        _ => return Err("trade_receipt_mismatch"),
    };
    let mut round = 0;
    let mut phase = String::new();
    let mut found = None;
    for event in events {
        if event["type"] == "phase_advanced" {
            round = event["round"].as_u64().unwrap_or(0);
            phase = match event["phase"].as_u64() {
                Some(1) => "market",
                Some(2) => "action",
                _ => "other",
            }
            .into();
        }
        if event["signature"] == signature
            && event["type"] == expected
            && event["faction"] == participant.actor
        {
            if found.is_some()
                || round != u64::from(context.round)
                || phase != context.phase
                || event["game"] != context.game
                || event["slot"].as_u64().is_none_or(|s| s <= context.after)
                || event["block_time"]
                    .as_i64()
                    .is_none_or(|t| t < context.captured_at || t > context.phase_ends_at)
            {
                return Err("trade_receipt_mismatch");
            }
            found = Some(event);
        }
    }
    let e = found.ok_or("trade_receipt_pending")?;
    if e["log_index"]
        .as_u64()
        .is_none_or(|index| e["id"] != format!("{signature}:{index}"))
        || e["slot"].as_u64().is_none_or(|slot| {
            view["journal_through_slot"]
                .as_u64()
                .is_none_or(|through| slot > through)
                || view["snapshot_slot"]
                    .as_u64()
                    .is_none_or(|head| slot > head)
        })
    {
        return Err("trade_receipt_pending");
    }
    let (cash, goods) = if expected == "produced" {
        (
            0i128,
            i128::from(e["goods"].as_u64().ok_or("trade_receipt_mismatch")?)
                - i128::from(participant.goods),
        )
    } else {
        if e["units"] != decision["params"]["units"] {
            return Err("trade_receipt_mismatch");
        }
        let n = i128::from(e["units"].as_u64().ok_or("trade_receipt_mismatch")?);
        let amount = e[if expected == "sold" {
            "revenue"
        } else {
            "cost"
        }]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or("trade_receipt_mismatch")?;
        if expected == "sold" {
            (i128::from(amount), -n)
        } else {
            (-i128::from(amount), n)
        }
    };
    Ok(
        json!({"mode":"solana","game":context.game,"event_id":e["id"],"actor":participant.actor,"action":decision["action"],"params":decision["params"],"cash_delta":cash.to_string(),"goods_delta":goods,"confirmed_at":e["block_time"],"source":"confirmed_devnet_event","signature":signature,"slot":e["slot"],"event":e}),
    )
}
pub fn valid_receipt(context: &Context, wallet: &str, decision: &Value, receipt: &Value) -> bool {
    let event = &receipt["event"];
    let expected = if context.mode == "http" {
        http_receipt(context, wallet, decision, event)
    } else {
        let view = json!({"ok":true,"cluster":"devnet","game_pda":context.game,"program_id":"3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC","commitment":"confirmed","history_complete":true,"journal_through_slot":event["slot"],"snapshot_slot":event["slot"],"factions":context.participants.iter().map(|p|json!({"pda":p.actor,"wallet":p.wallet})).collect::<Vec<_>>(),"events":[{"type":"phase_advanced","round":context.round,"phase":if context.phase=="market"{1}else{2}},event]});
        chain_receipt(
            context,
            wallet,
            decision,
            &view,
            receipt["signature"].as_str().unwrap_or(""),
        )
    };
    expected.is_ok_and(|expected| &expected == receipt)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recorded_devnet_sale_requires_matching_owner_context_and_canonical_receipt() {
        let view: Value = serde_json::from_str(include_str!(
            "../../experiments/alashi-watch/trading-cycle/devnet-snapshot.json"
        ))
        .unwrap();
        let sold = view["events"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["type"] == "sold")
            .unwrap();
        let participants = view["factions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| Participant {
                wallet: f["wallet"].as_str().unwrap().into(),
                actor: f["pda"].as_str().unwrap().into(),
                cash: "0".into(),
                goods: 2,
            })
            .collect::<Vec<_>>();
        let owner = participants
            .iter()
            .find(|p| p.actor == sold["faction"])
            .unwrap()
            .wallet
            .clone();
        let mut c = Context {
            mode: "solana".into(),
            game: view["game_pda"].as_str().unwrap().into(),
            round: 2,
            phase: "market".into(),
            phase_ends_at: sold["block_time"].as_i64().unwrap() + 60,
            captured_at: sold["block_time"].as_i64().unwrap() - 1,
            after: sold["slot"].as_u64().unwrap() - 1,
            participants,
            observation: json!({}),
        };
        let decision = json!({"action":"sell","params":{"units":2}});
        let signature = sold["signature"].as_str().unwrap();
        let receipt = chain_receipt(&c, &owner, &decision, &view, signature).unwrap();
        assert_eq!(receipt["cash_delta"], "22000000");
        assert_eq!(receipt["goods_delta"], -2);
        assert!(valid_receipt(&c, &owner, &decision, &receipt));
        let mut corrupt = receipt.clone();
        corrupt["cash_delta"] = json!("23000000");
        assert!(!valid_receipt(&c, &owner, &decision, &corrupt));
        assert!(chain_receipt(&c, "another-owner", &decision, &view, signature).is_err());
        assert!(chain_receipt(
            &c,
            &owner,
            &json!({"action":"sell","params":{"units":3}}),
            &view,
            signature
        )
        .is_err());
        let mut pending = view.clone();
        pending["commitment"] = json!("processed");
        assert!(chain_receipt(&c, &owner, &decision, &pending, signature).is_err());
        pending = view.clone();
        pending["history_complete"] = json!(false);
        assert!(chain_receipt(&c, &owner, &decision, &pending, signature).is_err());
        c.round = 3;
        assert!(chain_receipt(&c, &owner, &decision, &view, signature).is_err());
        c.round = 2;
        c.after += 1;
        assert!(chain_receipt(&c, &owner, &decision, &view, signature).is_err());
    }
}
