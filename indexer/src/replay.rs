use crate::events::ParsedEvent;
use alashi_rules::sim::Simulator;
use alashi_rules::state::{Phase, VoteChoice};
use serde_json::json;
use std::collections::BTreeMap;

pub struct ReplayOutcome {
    pub record: serde_json::Value,
}

fn observation(sim: &Simulator, wallet_to_idx: &BTreeMap<String, usize>) -> serde_json::Value {
    json!({
        "round": sim.game.round,
        "phase": sim.game.phase as u8,
        "sold_this_round": sim.game.sold_this_round,
        "law_card": sim.game.law_card,
        "president": sim.game.president.to_string(),
        "yes_influence": sim.game.yes_influence,
        "no_influence": sim.game.no_influence,
        "active_tax_bps": sim.game.active_tax_bps,
        "factions": sim.factions.iter().map(|f| json!({
            "faction_pda": f.wallet.to_string(),
            "name": f.name,
            "cash": f.cash,
            "goods": f.goods,
            "influence": f.influence,
        })).collect::<Vec<_>>(),
        "seat_of": wallet_to_idx,
    })
}

fn final_cash_for(
    wallet: &str,
    game_pk: &str,
    final_cash: &BTreeMap<String, u64>,
) -> u64 {
    let Ok(wpk) = wallet.parse::<anchor_lang::prelude::Pubkey>() else {
        return 0;
    };
    let Ok(gpk) = game_pk.parse::<anchor_lang::prelude::Pubkey>() else {
        return 0;
    };
    let pda = anchor_lang::prelude::Pubkey::find_program_address(
        &[alashi::constants::FACTION_SEED, gpk.as_ref(), wpk.as_ref()],
        &alashi::id(),
    )
    .0
    .to_string();
    final_cash.get(&pda).copied().unwrap_or(0)
}

pub fn replay(events: &[ParsedEvent], registry: &[(String, String)]) -> Option<ReplayOutcome> {
    let (game_id, entry_fee, phase_duration) = events.iter().find_map(|e| match e {
        ParsedEvent::GameInitialized { game_id, entry_fee, phase_duration, .. } =>
            Some((*game_id, *entry_fee, *phase_duration)),
        _ => None,
    })
    .unwrap_or((0, 0, 1));
    let game_pk = events
        .iter()
        .find_map(|e| match e {
            ParsedEvent::GameInitialized { game, .. } => Some(game.clone()),
            ParsedEvent::FactionJoined { game, .. } => Some(game.clone()),
            ParsedEvent::Sold { game, .. } => Some(game.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let mut sim = Simulator::new(game_id, entry_fee, phase_duration, 0);
    sim.game.law_card = 255;
    let mut pda_to_idx: BTreeMap<String, usize> = BTreeMap::new();
    let wallet_to_agent: BTreeMap<String, String> = registry.iter().cloned().collect();

    let mut steps: Vec<serde_json::Value> = vec![];
    let mut tick: i64 = 0;

    for e in events {
        let before = observation(&sim, &pda_to_idx);
        let step = sim.game.phase_duration.max(1) as i64 + 1;
        let applied = match e {
            ParsedEvent::GameInitialized { .. } => true,
            ParsedEvent::FactionJoined { faction, name, .. } => {
                let pk: anchor_lang::prelude::Pubkey = faction.parse().unwrap_or_default();
                let _ = sim.join(pk, name);
                if sim.factions.len() == pda_to_idx.len() + 1 {
                    pda_to_idx.insert(faction.clone(), sim.factions.len() - 1);
                }
                true
            }
            ParsedEvent::PhaseAdvanced { .. } => {
                tick += step;
                let now = tick - step;
                let card = events.iter().find_map(|e2| match e2 {
                    ParsedEvent::LawDrawn { round, card, .. }
                        if *round == sim.game.round =>
                    {
                        Some(*card)
                    }
                    _ => None,
                });
                let res = if sim.game.phase == Phase::Action {
                    match card {
                        Some(c) => sim.advance_with_card(now, c),
                        None => sim.advance(now, 0),
                    }
                } else {
                    sim.advance(now, 0)
                };
                res.is_ok()
            }
            ParsedEvent::LawDrawn { card, .. } => {
                if sim.game.phase == Phase::Law && sim.game.law_card == 255 {
                    sim.reveal_for_replay(*card).is_ok()
                } else {
                    true
                }
            }
            ParsedEvent::Produced { faction, .. } => {
                idx_of(&pda_to_idx, faction).map(|i| sim.produce(i).is_ok()).unwrap_or(false)
            }
            ParsedEvent::Sold { faction, units, .. } => {
                idx_of(&pda_to_idx, faction).map(|i| sim.sell(i, *units).is_ok()).unwrap_or(false)
            }
            ParsedEvent::GoodsBought { faction, units, .. } => {
                idx_of(&pda_to_idx, faction).map(|i| sim.buy(i, *units).is_ok()).unwrap_or(false)
            }
            ParsedEvent::DonkeyBought { faction, .. } => {
                idx_of(&pda_to_idx, faction).map(|i| sim.donkey(i).is_ok()).unwrap_or(false)
            }
            ParsedEvent::BribeGiven { from, to, amount, .. } => {
                match (idx_of(&pda_to_idx, from), idx_of(&pda_to_idx, to)) {
                    (Some(f), Some(t)) => sim.bribe(f, t, *amount).is_ok(),
                    _ => false,
                }
            }
            ParsedEvent::VoteCast { faction, choice, .. } => {
                let vc = match choice {
                    0 => VoteChoice::Yes,
                    1 => VoteChoice::No,
                    _ => VoteChoice::Abstain,
                };
                idx_of(&pda_to_idx, faction).map(|i| sim.vote(i, vc).is_ok()).unwrap_or(false)
            }
            ParsedEvent::VetoCast { president, .. } => match sim
                .factions
                .iter()
                .position(|f| {
                    let p: anchor_lang::prelude::Pubkey = president.parse().unwrap_or_default();
                    f.wallet == p
                })
            {
                Some(i) => sim.veto(i).is_ok(),
                None => false,
            },
            _ => true,
        };
        let after = observation(&sim, &pda_to_idx);
        steps.push(json!({
            "action": e,
            "applied": applied,
            "obs_before": before,
            "obs_after": after,
        }));
    }

    let final_cash: BTreeMap<String, u64> = steps
        .last()
        .and_then(|s| s["obs_after"]["factions"].as_array().cloned())
        .map(|arr| {
            arr.iter()
                .filter_map(|f| {
                    Some((
                        f["faction_pda"].as_str()?.to_string(),
                        f["cash"].as_u64().unwrap_or(0),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();

    let ranks: Vec<serde_json::Value> = events
        .iter()
        .filter_map(|e| match e {
            ParsedEvent::Payout { wallet, rank, amount, .. } => Some(json!({
                "rank": rank + 1,
                "wallet": wallet,
                "agent_id": wallet_to_agent.get(wallet).cloned().unwrap_or(wallet.clone()),
                "payout": amount,
                "final_cash": final_cash_for(wallet, &game_pk, &final_cash),
            })),
            _ => None,
        })
        .collect();
    if ranks.is_empty() {
        return None;
    }

    let record = json!({
        "game_id": game_id,
        "entry_fee": entry_fee,
        "names": sim.factions.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
        "final_phase": sim.game.phase as u8,
        "laws_passed": sim.game.laws_passed,
        "steps": steps,
        "ranks": ranks,
        "settled": sim.game.settled,
    });

    Some(ReplayOutcome { record })
}

fn idx_of(map: &BTreeMap<String, usize>, faction: &str) -> Option<usize> {
    map.get(faction).copied()
}
