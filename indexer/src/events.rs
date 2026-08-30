use alashi::events::*;
use anchor_lang::prelude::Pubkey;
use base64::Engine;
use sha2::{Digest, Sha256};

pub fn event_disc(name: &str) -> [u8; 8] {
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    let d = h.finalize();
    let mut out = [0u8; 8];
    out.copy_from_slice(&d[..8]);
    out
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ParsedEvent {
    GameInitialized { game: String, game_id: u64, entry_fee: u64, phase_duration: i64 },
    FactionJoined { game: String, faction: String, name: String, count: u8 },
    Produced { game: String, faction: String, goods: u16 },
    Sold { game: String, faction: String, units: u16, revenue: u64 },
    GoodsBought { game: String, faction: String, units: u16, cost: u64 },
    BribeGiven { game: String, from: String, to: String, amount: u64, influence_gained: u16 },
    DonkeyBought { game: String, faction: String, price: u64 },
    VoteCast { game: String, faction: String, choice: u8 },
    VetoCast { game: String, president: String },
    LawDrawn { game: String, round: u8, card: u8 },
    LawResult { game: String, round: u8, yes: u32, no: u32, passed: bool },
    LawVetoed { game: String, round: u8, card: u8 },
    PhaseAdvanced { game: String, round: u8, phase: u8 },
    Payout { game: String, wallet: String, rank: u8, amount: u64 },
    Settled { game: String, pot: u64, rake: u64, paid: u64 },
    LawCommitted { game: String, randomness: String, commit_slot: u64 },
}


pub fn parse_log_line(line: &str) -> Option<ParsedEvent> {
    let payload_b64 = line.strip_prefix("Program data: ")?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(payload_b64.trim())
        .ok()?;
    if raw.len() < 8 {
        return None;
    }
    let mut disc = [0u8; 8];
    disc.copy_from_slice(&raw[..8]);
    let body = &raw[8..];
    use anchor_lang::AnchorDeserialize;
    macro_rules! dec {
        ($t:ident, $b:expr) => {
            $t::try_from_slice($b).ok().map(|e| e)
        };
    }
    let _ = disc;
    if raw.starts_with(&event_disc("event:GameInitialized")) {
        dec!(GameInitialized, body).map(|e| ParsedEvent::GameInitialized {
            game: e.game.to_string(),
            game_id: e.game_id,
            entry_fee: e.entry_fee,
            phase_duration: e.phase_duration,
        })
    } else if raw.starts_with(&event_disc("event:FactionJoined")) {
        dec!(FactionJoined, body).map(|e| ParsedEvent::FactionJoined {
            game: e.game.to_string(),
            faction: e.faction.to_string(),
            name: e.name,
            count: e.count,
        })
    } else if raw.starts_with(&event_disc("event:Produced")) {
        dec!(Produced, body).map(|e| ParsedEvent::Produced {
            game: e.game.to_string(),
            faction: e.faction.to_string(),
            goods: e.goods,
        })
    } else if raw.starts_with(&event_disc("event:Sold")) {
        dec!(Sold, body).map(|e| ParsedEvent::Sold {
            game: e.game.to_string(),
            faction: e.faction.to_string(),
            units: e.units,
            revenue: e.revenue,
        })
    } else if raw.starts_with(&event_disc("event:GoodsBought")) {
        dec!(GoodsBought, body).map(|e| ParsedEvent::GoodsBought {
            game: e.game.to_string(),
            faction: e.faction.to_string(),
            units: e.units,
            cost: e.cost,
        })
    } else if raw.starts_with(&event_disc("event:BribeGiven")) {
        dec!(BribeGiven, body).map(|e| ParsedEvent::BribeGiven {
            game: e.game.to_string(),
            from: e.from.to_string(),
            to: e.to.to_string(),
            amount: e.amount,
            influence_gained: e.influence_gained,
        })
    } else if raw.starts_with(&event_disc("event:DonkeyBought")) {
        dec!(DonkeyBought, body).map(|e| ParsedEvent::DonkeyBought {
            game: e.game.to_string(),
            faction: e.faction.to_string(),
            price: e.price,
        })
    } else if raw.starts_with(&event_disc("event:VoteCast")) {
        dec!(VoteCast, body).map(|e| {
            let choice = match e.choice {
                alashi_rules::state::VoteChoice::Yes => 0u8,
                alashi_rules::state::VoteChoice::No => 1,
                alashi_rules::state::VoteChoice::Abstain => 2,
            };
            ParsedEvent::VoteCast {
                game: e.game.to_string(),
                faction: e.faction.to_string(),
                choice,
            }
        })
    } else if raw.starts_with(&event_disc("event:VetoCast")) {
        dec!(VetoCast, body).map(|e| ParsedEvent::VetoCast {
            game: e.game.to_string(),
            president: e.president.to_string(),
        })
    } else if raw.starts_with(&event_disc("event:LawDrawn")) {
        dec!(LawDrawn, body).map(|e| ParsedEvent::LawDrawn {
            game: e.game.to_string(),
            round: e.round,
            card: e.card,
        })
    } else if raw.starts_with(&event_disc("event:LawResult")) {
        dec!(LawResult, body).map(|e| ParsedEvent::LawResult {
            game: e.game.to_string(),
            round: e.round,
            yes: e.yes,
            no: e.no,
            passed: e.passed,
        })
    } else if raw.starts_with(&event_disc("event:LawVetoed")) {
        dec!(LawVetoed, body).map(|e| ParsedEvent::LawVetoed {
            game: e.game.to_string(),
            round: e.round,
            card: e.card,
        })
    } else if raw.starts_with(&event_disc("event:PhaseAdvanced")) {
        dec!(PhaseAdvanced, body).map(|e| ParsedEvent::PhaseAdvanced {
            game: e.game.to_string(),
            round: e.round,
            phase: e.phase as u8,
        })
    } else if raw.starts_with(&event_disc("event:Payout")) {
        dec!(Payout, body).map(|e| ParsedEvent::Payout {
            game: e.game.to_string(),
            wallet: e.wallet.to_string(),
            rank: e.rank,
            amount: e.amount,
        })
    } else if raw.starts_with(&event_disc("event:Settled")) {
        dec!(Settled, body).map(|e| ParsedEvent::Settled {
            game: e.game.to_string(),
            pot: e.pot,
            rake: e.rake,
            paid: e.paid,
        })
    } else {
        None
    }
}
