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
        ParsedEvent::Payout { game: ga.clone(), wallet: fa.clone(), rank: 0, amount: 60_000_000 },
        ParsedEvent::Payout { game: ga.clone(), wallet: fb.clone(), rank: 1, amount: 36_000_000 },
        ParsedEvent::Settled { game: ga.clone(), pot: 100_000_000, rake: 4_000_000, paid: 96_000_000 },
    ]
}

#[test]
fn recorded_payouts_and_events_survive_roundtrip_without_synthetic_cash() {
    let reg = vec![(anchor_lang::prelude::Pubkey::new_from_array([2; 32]).to_string(), "agent-x".into())];
    let out = replay::replay(&ev_seq(), &reg).expect("export");
    let back: serde_json::Value = serde_json::from_str(&out.record.to_string()).unwrap();
    assert_eq!(back["schema_version"], 2);
    assert_eq!(back["replay_verified"], false);
    assert_eq!(back["game_id"], 7);
    assert_eq!(back["ranks"][0]["agent_id"], "agent-x");
    assert_eq!(back["ranks"][0]["payout"], 60_000_000);
    assert!(back["ranks"][0].get("final_cash").is_none());
    assert!(back.get("steps").is_none());
    assert_eq!(back["events"], serde_json::to_value(ev_seq()).unwrap());
}

#[test]
fn epoch_events_are_preserved_without_claiming_to_apply_them() {
    let mut events = ev_seq();
    let game = events[0].game().to_string();
    let event = ParsedEvent::Exchanged { game, faction:"faction".into(), to_hard:true, got:17 };
    events.insert(7, event.clone());
    let record = replay::replay(&events, &[]).unwrap().record;
    assert_eq!(record["events"][7], serde_json::to_value(event).unwrap());
    assert_eq!(record["replay_verified"], false);
}

#[test]
fn incomplete_or_mixed_game_logs_are_not_exported_as_settled_matches() {
    let mut events = ev_seq();
    events.pop();
    assert!(replay::replay(&events, &[]).is_none());
    let mut events = ev_seq();
    events.remove(0);
    assert!(replay::replay(&events, &[]).is_none());
    let mut events = ev_seq();
    events.push(ParsedEvent::Exchanged { game:"another-game".into(), faction:"faction".into(), to_hard:true, got:17 });
    assert!(replay::replay(&events, &[]).is_none());
}
