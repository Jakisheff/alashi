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
    // R18 (REVIEW_EXTERNAL): VRF-ветвь раньше терялась — прерванная
    // партия была неотличима от незавершённой
    VrfRetry { game: String, round: u8, attempt: u8 },
    GameAborted { game: String, round: u8 },
    // SPEC_EPOCH_90S: события M-действий
    SoldCreditEv { game: String, faction: String, units: u16, promissory: u64 },
    ShuttledEv { game: String, faction: String, goods: u16, grey: u16 },
    RoofBought { game: String, from: String, to: String, tariff: u8, price: u64 },
    CustomsSet { game: String, president: String, tight: bool },
    LicenseBid { game: String, faction: String, amount: u64, total_bid: u64 },
    LicenseInsight { game: String, faction: String, yield_amount: u64 },
    Exchanged { game: String, faction: String, to_hard: bool, got: u64 },
    VoteOffered { game: String, seller: String, buyer: String, price: u64 },
    VoteSold { game: String, buyer: String, seller: String, price: u64 },
    BarterProposed { game: String, from: String, offer: u64, goods: u16, price: u64 },
    BarterAccepted { game: String, by: String, from: String, offer: u64 },
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
    } else if raw.starts_with(&event_disc("event:LawCommitted")) {
        dec!(LawCommitted, body).map(|e| ParsedEvent::LawCommitted {
            game: e.game.to_string(),
            randomness: e.randomness.to_string(),
            commit_slot: e.commit_slot,
        })
    } else if raw.starts_with(&event_disc("event:VrfRetry")) {
        dec!(VrfRetry, body).map(|e| ParsedEvent::VrfRetry {
            game: e.game.to_string(),
            round: e.round,
            attempt: e.attempt,
        })
    } else if raw.starts_with(&event_disc("event:GameAbortedEvent")) {
        dec!(GameAbortedEvent, body).map(|e| ParsedEvent::GameAborted {
            game: e.game.to_string(),
            round: e.round,
        })
    } else if raw.starts_with(&event_disc("event:SoldCredit")) {
        dec!(SoldCredit, body).map(|e| ParsedEvent::SoldCreditEv {
            game: e.game.to_string(),
            faction: e.faction.to_string(),
            units: e.units,
            promissory: e.promissory,
        })
    } else if raw.starts_with(&event_disc("event:Shuttled")) {
        dec!(Shuttled, body).map(|e| ParsedEvent::ShuttledEv {
            game: e.game.to_string(),
            faction: e.faction.to_string(),
            goods: e.goods,
            grey: e.grey,
        })
    } else if raw.starts_with(&event_disc("event:RoofBought")) {
        dec!(RoofBought, body).map(|e| ParsedEvent::RoofBought {
            game: e.game.to_string(),
            from: e.from.to_string(),
            to: e.to.to_string(),
            tariff: e.tariff,
            price: e.price,
        })
    } else if raw.starts_with(&event_disc("event:CustomsSet")) {
        dec!(CustomsSet, body).map(|e| ParsedEvent::CustomsSet {
            game: e.game.to_string(),
            president: e.president.to_string(),
            tight: e.tight,
        })
    } else if raw.starts_with(&event_disc("event:LicenseBid")) {
        dec!(LicenseBid, body).map(|e| ParsedEvent::LicenseBid {
            game: e.game.to_string(),
            faction: e.faction.to_string(),
            amount: e.amount,
            total_bid: e.total_bid,
        })
    } else if raw.starts_with(&event_disc("event:LicenseInsight")) {
        dec!(LicenseInsight, body).map(|e| ParsedEvent::LicenseInsight {
            game: e.game.to_string(),
            faction: e.faction.to_string(),
            yield_amount: e.yield_amount,
        })
    } else if raw.starts_with(&event_disc("event:Exchanged")) {
        dec!(Exchanged, body).map(|e| ParsedEvent::Exchanged {
            game: e.game.to_string(),
            faction: e.faction.to_string(),
            to_hard: e.to_hard,
            got: e.got,
        })
    } else if raw.starts_with(&event_disc("event:VoteOffered")) {
        dec!(VoteOffered, body).map(|e| ParsedEvent::VoteOffered {
            game: e.game.to_string(),
            seller: e.seller.to_string(),
            buyer: e.buyer.to_string(),
            price: e.price,
        })
    } else if raw.starts_with(&event_disc("event:VoteSold")) {
        dec!(VoteSold, body).map(|e| ParsedEvent::VoteSold {
            game: e.game.to_string(),
            buyer: e.buyer.to_string(),
            seller: e.seller.to_string(),
            price: e.price,
        })
    } else if raw.starts_with(&event_disc("event:BarterProposed")) {
        dec!(BarterProposed, body).map(|e| ParsedEvent::BarterProposed {
            game: e.game.to_string(),
            from: e.from.to_string(),
            offer: e.offer,
            goods: e.goods,
            price: e.price,
        })
    } else if raw.starts_with(&event_disc("event:BarterAccepted")) {
        dec!(BarterAccepted, body).map(|e| ParsedEvent::BarterAccepted {
            game: e.game.to_string(),
            by: e.by.to_string(),
            from: e.from.to_string(),
            offer: e.offer,
        })
    } else {
        None
    }
}
