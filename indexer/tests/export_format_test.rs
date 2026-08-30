use indexer::events::ParsedEvent;
use indexer::replay;

fn ev_seq() -> Vec<ParsedEvent> {
    use anchor_lang::prelude::Pubkey;
    let ga = Pubkey::new_from_array([1; 32]).to_string();
    let fa = Pubkey::new_from_array([2; 32]).to_string();
    let fb = Pubkey::new_from_array([3; 32]).to_string();
    vec![
        ParsedEvent::GameInitialized { game: ga.clone(), game_id: 7, entry_fee: 100_000_000, phase_duration: 0 },
        ParsedEvent::FactionJoined { game: ga.clone(), faction: fa.clone(), name: "A".into(), count: 1 },
        ParsedEvent::FactionJoined { game: ga.clone(), faction: fb.clone(), name: "B".into(), count: 2 },
        ParsedEvent::PhaseAdvanced { game: ga.clone(), round: 1, phase: 1 },
        ParsedEvent::PhaseAdvanced { game: ga.clone(), round: 1, phase: 2 },
        ParsedEvent::Produced { game: ga.clone(), faction: fa.clone(), goods: 2 },
        ParsedEvent::Produced { game: ga.clone(), faction: fb.clone(), goods: 2 },
        ParsedEvent::PhaseAdvanced { game: ga.clone(), round: 1, phase: 3 },
        ParsedEvent::LawDrawn { game: ga.clone(), round: 1, card: 0 },
        ParsedEvent::VoteCast { game: ga.clone(), faction: fa.clone(), choice: 0 },
        ParsedEvent::PhaseAdvanced { game: ga.clone(), round: 2, phase: 1 },
        ParsedEvent::Sold { game: ga.clone(), faction: fa.clone(), units: 2, revenue: 22_000_000 },
        ParsedEvent::PhaseAdvanced { game: ga.clone(), round: 2, phase: 2 },
        ParsedEvent::Produced { game: ga.clone(), faction: fa.clone(), goods: 2 },
        ParsedEvent::Produced { game: ga.clone(), faction: fb.clone(), goods: 2 },
        ParsedEvent::PhaseAdvanced { game: ga.clone(), round: 2, phase: 3 },
        ParsedEvent::LawDrawn { game: ga.clone(), round: 2, card: 1 },
        ParsedEvent::VoteCast { game: ga.clone(), faction: fa.clone(), choice: 0 },
        ParsedEvent::PhaseAdvanced { game: ga.clone(), round: 3, phase: 1 },
        ParsedEvent::Payout { game: ga.clone(), wallet: fa.clone(), rank: 1, amount: 60_000_000 },
        ParsedEvent::Payout { game: ga.clone(), wallet: fb.clone(), rank: 2, amount: 36_000_000 },
        ParsedEvent::Settled { game: ga.clone(), pot: 100_000_000, rake: 4_000_000, paid: 96_000_000 },
    ]
}

#[test]
fn jsonl_roundtrip_and_ranks() {
    let reg = vec![(anchor_lang::prelude::Pubkey::new_from_array([2; 32]).to_string(), "agent-x".into())];
    let out = replay::replay(&ev_seq(), &reg).expect("replay");
    let line = serde_json::to_string(&out.record).unwrap();
    let back: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert!(back["steps"].as_array().unwrap().len() >= 10);
    assert_eq!(back["game_id"], 7);
    let ranks = back["ranks"].as_array().unwrap();
    assert_eq!(ranks.len(), 2);
    assert_eq!(ranks[0]["rank"], 1);
    assert_eq!(ranks[0]["agent_id"], "agent-x");
    let winner_cash = ranks[0]["final_cash"].as_u64().unwrap();
    assert!(winner_cash >= ranks[1]["final_cash"].as_u64().unwrap());
}

#[test]
fn observations_present_every_step() {
    let out = replay::replay(&ev_seq(), &[]).expect("replay");
    for step in out.record["steps"].as_array().unwrap() {
        assert!(step["obs_before"]["phase"].is_number());
        assert!(step["obs_after"]["factions"].is_array());
        assert!(step["action"]["type"].is_string());
    }
}
