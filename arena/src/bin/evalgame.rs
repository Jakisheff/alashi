//! Isolated JSON-lines bridge for tools/compare_agents.py. Never connects to arenad.
use alashi_rules::{constants::*, state::VoteChoice};
use arena::{
    runner::{observation_json, play_game, GameConfig},
    strategies::*,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, Write};
use std::time::Instant;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    seed: u64,
    seat: usize,
    opponents: Vec<String>,
    builtin: Option<String>,
    epoch: u8,
    vote_weight_mode: u8,
}

fn emit(value: Value) {
    println!("{}", value);
    io::stdout().flush().expect("bridge stdout");
}
fn read() -> Value {
    let mut line = String::new();
    assert!(
        io::stdin().read_line(&mut line).expect("bridge stdin") > 0,
        "bridge closed"
    );
    serde_json::from_str(&line).expect("bridge JSON")
}
fn observation(o: &Obs) -> Value {
    observation_json(o)
}
fn number<T: TryFrom<u64>>(v: &Value, key: &str) -> Result<T, String> {
    v[key]
        .as_u64()
        .and_then(|n| n.try_into().ok())
        .ok_or_else(|| format!("invalid {key}"))
}
fn market(v: &Value) -> Result<MarketAction, String> {
    Ok(match v["action"].as_str() {
        Some("sell") => MarketAction::Sell(number(v, "units")?),
        Some("buy") => MarketAction::Buy(number(v, "units")?),
        Some("sell_credit") => MarketAction::SellCredit(number(v, "units")?),
        Some("pass") => MarketAction::Pass,
        _ => return Err("unknown market action".into()),
    })
}
fn action(v: &Value) -> Result<ActionAction, String> {
    Ok(match v["action"].as_str() {
        Some("produce") => ActionAction::Produce,
        Some("donkey") => ActionAction::Donkey,
        Some("shuttle") => ActionAction::Shuttle,
        Some("bribe") => ActionAction::Bribe {
            to: number(v, "to")?,
            amount: number(v, "amount")?,
        },
        Some("roof") => ActionAction::Roof {
            to: number(v, "to")?,
            tariff: number(v, "tariff")?,
        },
        Some("bid") => ActionAction::Bid(number(v, "amount")?),
        Some("pass") => ActionAction::Pass,
        _ => return Err("unknown action".into()),
    })
}
fn law(v: &Value) -> Result<LawAction, String> {
    Ok(match v["action"].as_str() {
        Some("vote_yes") => LawAction::Vote(VoteChoice::Yes),
        Some("vote_no") => LawAction::Vote(VoteChoice::No),
        Some("vote_abstain") => LawAction::Vote(VoteChoice::Abstain),
        Some("veto") => LawAction::Veto,
        Some("pass") => LawAction::Pass,
        _ => return Err("unknown law action".into()),
    })
}
struct Subject {
    inner: Option<Box<dyn Strategy>>,
}
impl Subject {
    fn choose<T: std::fmt::Debug>(
        &mut self,
        o: &Obs,
        phase: &str,
        builtin: impl FnOnce(&mut dyn Strategy, &Obs) -> T,
        parse: impl FnOnce(&Value) -> Result<T, String>,
        fallback: T,
    ) -> T {
        let start = Instant::now();
        let obs = observation(o);
        let (result, error) = if let Some(inner) = self.inner.as_mut() {
            (builtin(inner.as_mut(), o), None)
        } else {
            emit(json!({"type":"request","phase":phase,"observation":obs}));
            match parse(&read()) {
                Ok(a) => (a, None),
                Err(e) => (fallback, Some(e)),
            }
        };
        emit(json!({"type":"decision","phase":phase,"observation":obs,
            "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,
            "decision":format!("{:?}",result),"parse_error":error}));
        result
    }
}
impl Strategy for Subject {
    fn name(&self) -> &'static str {
        "subject"
    }
    fn market(&mut self, o: &Obs) -> MarketAction {
        self.choose(o, "market", |s, o| s.market(o), market, MarketAction::Pass)
    }
    fn action(&mut self, o: &Obs) -> ActionAction {
        self.choose(o, "action", |s, o| s.action(o), action, ActionAction::Pass)
    }
    fn law(&mut self, o: &Obs) -> LawAction {
        self.choose(o, "law", |s, o| s.law(o), law, LawAction::Pass)
    }
    fn veto_after_vote(&mut self, o: &Obs) -> LawAction {
        self.choose(o, "law", |s, o| s.veto_after_vote(o), law, LawAction::Pass)
    }
}
fn main() {
    let c: Config = serde_json::from_value(read()).expect("invalid game config");
    let n = c.opponents.len() + 1;
    assert!((MIN_FACTIONS as usize..=MAX_FACTIONS as usize).contains(&n));
    assert!(c.seat < n && c.epoch <= 1 && c.vote_weight_mode <= 1);
    let mut strategies: Vec<Box<dyn Strategy>> = c
        .opponents
        .iter()
        .enumerate()
        .map(|(i, name)| {
            // Opponent identity retains its random stream when seats rotate.
            by_name(name, c.seed.wrapping_add(i as u64 + 1)).expect("unknown opponent")
        })
        .collect();
    let inner = c
        .builtin
        .as_ref()
        .map(|name| by_name(name, c.seed).expect("unknown subject"));
    strategies.insert(0, Box::new(Subject { inner }));
    strategies.rotate_right(c.seat);
    let cfg = GameConfig {
        epoch: c.epoch,
        vote_weight_mode: c.vote_weight_mode,
        ..GameConfig::default()
    };
    let game = play_game(0, c.seed, &mut strategies, &cfg);
    emit(json!({"type":"result","game":game}));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_and_out_of_range_actions_are_rejected() {
        assert!(market(&json!({"action":"sell","units":65536})).is_err());
        assert!(action(&json!({"action":"bribe","to":1,"amount":-1})).is_err());
        assert!(law(&json!({"action":"produce"})).is_err());
        assert_eq!(
            market(&json!({"action":"sell","units":3})).unwrap(),
            MarketAction::Sell(3)
        );
    }
}
