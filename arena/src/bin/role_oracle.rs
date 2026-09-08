//! Local fixture oracle for the behavioral role-binding probe. No arena or RPC.
use alashi_rules::{actions, anchor_lang::prelude::Pubkey, constants::NO_LAW, state::*};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, BufRead};

#[derive(Deserialize)]
struct Row {
    id: u8,
    name: String,
    influence: u16,
}

#[derive(Deserialize)]
struct Snapshot {
    phase: String,
    president_id: u8,
    law_revealed: bool,
    veto_pending: bool,
    factions: Vec<Row>,
}

fn wallet(id: u8) -> Pubkey {
    Pubkey::new_from_array([id + 1; 32])
}

fn evaluate(s: Snapshot) -> Result<Value, String> {
    let phase = match s.phase.as_str() {
        "law" => Phase::Law,
        "market" => Phase::Market,
        _ => return Err("unsupported fixture phase".into()),
    };
    if s.factions.len() != 3
        || !(0..3).all(|id| s.factions.iter().filter(|f| f.id == id).count() == 1)
        || s.president_id > 2
    {
        return Err("fixture requires unique IDs 0, 1, 2 and an existing president".into());
    }
    let mut eligible = vec![];
    let mut outcomes = vec![];
    for row in s.factions {
        // Rebuild state for each attempt: one candidate must not consume another's veto.
        let mut game = Game::default();
        game.phase = phase;
        game.president = wallet(s.president_id);
        game.law_card = if s.law_revealed { 0 } else { NO_LAW };
        game.veto_pending = s.veto_pending;
        let mut faction = Faction::default();
        faction.wallet = wallet(row.id);
        faction.name = row.name;
        faction.influence = row.influence;
        faction.alive = true;
        let result = actions::veto(&mut game, &mut faction);
        if result.is_ok() {
            eligible.push(row.id);
        }
        outcomes.push(json!({"id":row.id, "error":result.err().map(|e| format!("{e:?}"))}));
    }
    eligible.sort();
    Ok(json!({"president_id":s.president_id, "veto_eligible_ids":eligible, "outcomes":outcomes}))
}

fn main() {
    for line in io::stdin().lock().lines() {
        let result = line
            .map_err(|e| e.to_string())
            .and_then(|line| serde_json::from_str::<Snapshot>(&line).map_err(|e| e.to_string()))
            .and_then(evaluate);
        match result {
            Ok(answer) => println!("{answer}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }
}
