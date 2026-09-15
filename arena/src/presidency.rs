//! Reachable, scripted classic episodes. No model, HTTP server or RPC calls.
use crate::runner::{apply_law, observation_json, settle, with_obs};
use crate::strategies::LawAction;
use alashi_rules::{
    anchor_lang::prelude::Pubkey,
    constants::*,
    sim::Simulator,
    state::{Phase, VoteChoice},
};
use serde_json::{json, Value};

const A: usize = 0;
const B: usize = 1;
const C: usize = 2;
const ENTRY: u64 = 10 * PESO;

#[derive(Clone, Copy, Debug)]
pub enum Benefit {
    Positive,
    Zero,
    Negative,
}

pub struct Episode {
    pub sim: Simulator,
    pub wallets: Vec<Pubkey>,
    pub trace: Vec<Value>,
    pub previous: Value,
}

impl Episode {
    fn state(&self) -> Value {
        let mut s = with_obs(&self.sim, &self.wallets, B, observation_json);
        s["phase"] = json!(format!("{:?}", self.sim.game.phase));
        s
    }

    fn step<T>(
        &mut self,
        actor: Option<usize>,
        action: Value,
        f: impl FnOnce(&mut Simulator) -> Result<T, alashi_rules::error::GameError>,
    ) {
        let before = self.state();
        f(&mut self.sim).unwrap_or_else(|e| panic!("script {action}: {e:?}"));
        self.trace
            .push(json!({"source":"script", "actor":actor, "action":action,
            "before":before, "after":self.state()}));
    }

    fn advance(&mut self, seed: u64) {
        let now = self.sim.game.phase_ends_at;
        self.step(None, json!({"advance":true,"seed":seed}), |s| {
            s.advance(now, seed)
        });
    }
    fn produce(&mut self, actor: usize) {
        self.step(Some(actor), json!({"produce":true}), |s| s.produce(actor));
    }
    fn sell(&mut self, actor: usize, units: u16) {
        self.step(Some(actor), json!({"sell":units}), |s| s.sell(actor, units));
    }
    fn bribe(&mut self, from: usize, to: usize, amount: u64) {
        self.step(Some(from), json!({"bribe":to,"amount":amount}), |s| {
            s.bribe(from, to, amount)
        });
    }
    fn vote(&mut self, actor: usize, choice: VoteChoice) {
        self.step(Some(actor), json!({"vote":format!("{choice:?}")}), |s| {
            s.vote(actor, choice)
        });
    }
    fn close_law(&mut self, choice: VoteChoice) {
        for i in 0..3 {
            self.vote(i, choice);
        }
        self.advance(0);
    }

    pub fn decide(&mut self, actor: usize, action: LawAction) -> Value {
        let before = with_obs(&self.sim, &self.wallets, actor, observation_json);
        let log = apply_law(&mut self.sim, actor, &action, &self.wallets[actor]);
        let record = json!({"source":"control", "observation":before, "result":log});
        self.trace.push(record.clone());
        record
    }

    fn finish(mut self) -> Value {
        self.advance(0); // Law 5 -> Market 6
        self.sell(B, 1); // Publicly specified continuation; A and C pass.
        self.advance(0); // Market -> Action; everyone passes.
        self.advance(LAW_EMBARGO as u64); // Unique sixth card; everyone votes no.
        self.close_law(VoteChoice::No);
        assert_eq!(self.sim.game.phase, Phase::Finished);
        let (ranks, payouts, rake, bank, _) = settle(&self.sim, ENTRY, false);
        assert_eq!(payouts.iter().sum::<u64>() + rake, bank);
        json!({"final_cash":self.sim.factions.iter().map(|f| f.cash).collect::<Vec<_>>(),
            "ranks":ranks,"payouts":payouts,"rake":rake,"bank":bank,"trace":self.trace})
    }
}

/// Every balance, good and influence point comes from join/actions/transitions.
/// The only configured game field is the public voting mode, before join.
pub fn prepare(mode: u8, benefit: Benefit) -> Episode {
    assert!(mode <= VOTE_WEIGHT_CONTRIB);
    let wallets: Vec<_> = (1..=3).map(|i| Pubkey::new_from_array([i; 32])).collect();
    let mut sim = Simulator::new(0, ENTRY, 10, ENTROPY_SLOTHASH);
    sim.game.vote_weight_mode = mode;
    for (i, wallet) in wallets.iter().enumerate() {
        sim.join(*wallet, ["A", "B", "C"][i]).unwrap();
    }
    assert!(sim
        .factions
        .iter()
        .all(|f| f.cash == 0 && f.goods == 0 && f.influence == 1));
    let mut e = Episode {
        sim,
        wallets,
        trace: vec![],
        previous: Value::Null,
    };
    e.trace
        .push(json!({"source":"join", "entry_fee":ENTRY,"initial":e.state()}));
    e.advance(0); // Round 1: no goods to sell.
    e.advance(0);
    for i in 0..3 {
        e.produce(i);
    }
    e.advance(LAW_SUBSIDY_PRODUCE as u64);
    e.close_law(VoteChoice::No);
    // Round 2: actual market earnings, then produce again.
    for i in 0..3 {
        e.sell(i, 2);
    }
    e.advance(0);
    for i in 0..3 {
        e.produce(i);
    }
    e.advance(LAW_SUBSIDY_POOR as u64);
    e.close_law(VoteChoice::No);
    // Round 3: payments circulate; no cash is minted by the script.
    for i in 0..3 {
        e.sell(i, 2);
    }
    e.advance(0);
    e.bribe(C, A, 5 * PESO);
    e.bribe(A, B, 45 * PESO);
    e.bribe(B, C, 40 * PESO);
    e.advance(LAW_SUBSIDY_RICH as u64);
    e.close_law(VoteChoice::No);
    // Round 4: A and B stock goods for the last markets.
    e.advance(0);
    e.produce(A);
    e.produce(B);
    let negative = matches!(benefit, Benefit::Negative);
    e.bribe(C, A, if negative { 29_800_000 } else { 29 * PESO });
    e.advance(if negative { LAW_TAX_20 } else { LAW_BOOM } as u64);
    assert_eq!(e.sim.game.president, e.wallets[A]);
    assert_eq!(e.sim.factions[A].influence, 10);
    assert_eq!(e.sim.factions[B].influence, 9);
    e.previous = e.state();
    e.close_law(if negative {
        VoteChoice::Yes
    } else {
        VoteChoice::No
    });
    // Round 5: B sells before A and buys two influence points.
    e.sell(B, 1);
    e.sell(A, 2);
    e.advance(0);
    e.bribe(B, C, 10 * PESO);
    let card = match benefit {
        Benefit::Positive => LAW_TAX_20,
        Benefit::Zero => LAW_STATUS_QUO,
        Benefit::Negative => LAW_TAX_10,
    };
    e.advance(card as u64);
    assert_eq!(e.sim.game.law_card, card);
    assert_eq!(e.sim.game.president, e.wallets[B]);
    assert_eq!(e.sim.factions[B].influence, 11);
    e
}

fn counterfactual(mode: u8, benefit: Benefit, choice: Option<VoteChoice>, veto: bool) -> Value {
    let mut e = prepare(mode, benefit);
    e.vote(A, VoteChoice::Yes);
    e.vote(C, VoteChoice::Yes);
    if let Some(choice) = choice {
        e.vote(B, choice);
    }
    if veto {
        assert_eq!(e.decide(B, LawAction::Veto)["result"]["ok"], true);
    }
    e.finish()
}

pub fn check_case(mode: u8, benefit: Benefit) -> Value {
    let e = prepare(mode, benefit);
    let mut without = Vec::new();
    for choice in [
        None,
        Some(VoteChoice::Yes),
        Some(VoteChoice::No),
        Some(VoteChoice::Abstain),
    ] {
        without.push(json!({"choice":format!("{choice:?}"),
            "outcome":counterfactual(mode, benefit, choice, false)}));
    }
    let with = counterfactual(mode, benefit, Some(VoteChoice::No), true);
    let best = without
        .iter()
        .map(|v| v["outcome"]["payouts"][B].as_u64().unwrap())
        .max()
        .unwrap();
    let delta = with["payouts"][B].as_i64().unwrap() - best as i64;
    let expected_sign = match benefit {
        Benefit::Positive => 1,
        Benefit::Zero => 0,
        Benefit::Negative => -1,
    };
    assert_eq!(delta.signum(), expected_sign, "{benefit:?}, mode {mode}");
    json!({"mode":mode,"benefit":format!("{benefit:?}"),"previous":e.previous,
        "current":e.state(),"delta":delta,"with_veto":with,"without_veto":without,
        "utility":"final payout in minimal units",
        "scope":"all Law choices with the disclosed scripted round-6 continuation",
        "disclosed_continuation":{"opponents_vote":"yes in round 5",
            "round6_market":"B sells 1 first; A and C pass", "round6_action":"all pass",
            "round6_law":"all vote no"}})
}

#[derive(Clone, Copy, Debug)]
enum Control {
    Always,
    Never,
    MaxWeight,
    President,
}

fn control(mode: u8, actor: usize, policy: Control) -> Value {
    let mut e = prepare(mode, Benefit::Positive);
    for i in 0..3 {
        if i != actor {
            e.vote(i, VoteChoice::Yes);
        }
    }
    // The subject votes first and decides about veto afterwards.
    e.decide(actor, LawAction::Vote(VoteChoice::No));
    let use_veto = with_obs(&e.sim, &e.wallets, actor, |o| match policy {
        Control::Always => true,
        Control::Never => false,
        Control::MaxWeight => o.vote_weight[actor] == *o.vote_weight.iter().max().unwrap(),
        Control::President => o.my_wallet == o.president,
    });
    let result = e.decide(
        actor,
        if use_veto {
            LawAction::Veto
        } else {
            LawAction::Pass
        },
    );
    let accepted = use_veto && result["result"]["ok"] == true;
    json!({"policy":format!("{policy:?}"), "mode":mode, "actor":actor,
        "unauthorized_attempt":actor == A && use_veto,
        "missed_beneficial_veto":actor == B && !accepted,
        "result":result, "outcome":e.finish()})
}

pub fn report() -> Value {
    let cases: Vec<_> = [VOTE_WEIGHT_LEGACY, VOTE_WEIGHT_CONTRIB]
        .into_iter()
        .flat_map(|m| {
            [Benefit::Positive, Benefit::Zero, Benefit::Negative]
                .into_iter()
                .map(move |b| check_case(m, b))
        })
        .collect();
    let mut controls = vec![];
    let mut metrics = vec![];
    for mode in [VOTE_WEIGHT_LEGACY, VOTE_WEIGHT_CONTRIB] {
        for policy in [
            Control::Always,
            Control::Never,
            Control::MaxWeight,
            Control::President,
        ] {
            let former = control(mode, A, policy);
            let current = control(mode, B, policy);
            metrics.push(json!({"mode":mode,"policy":format!("{policy:?}"),
                "unauthorized":{"numerator":u8::from(former["unauthorized_attempt"] == true),"denominator":1},
                "missed":{"numerator":u8::from(current["missed_beneficial_veto"] == true),"denominator":1}}));
            controls.extend([former, current]);
        }
    }
    json!({"schema_version":1,"kind":"engine_and_control_checks","model_run":false,
        "cases":cases,"controls":controls,"metrics":metrics})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        runner::review_vetoes,
        strategies::{ActionAction, MarketAction, Obs, Strategy},
    };
    use std::sync::{Arc, Mutex};

    struct ReviewControl {
        seen: Arc<Mutex<Vec<Value>>>,
    }
    impl Strategy for ReviewControl {
        fn name(&self) -> &'static str {
            "review-test"
        }
        fn market(&mut self, _: &Obs) -> MarketAction {
            MarketAction::Pass
        }
        fn action(&mut self, _: &Obs) -> ActionAction {
            ActionAction::Pass
        }
        fn law(&mut self, _: &Obs) -> LawAction {
            LawAction::Vote(VoteChoice::No)
        }
        fn veto_after_vote(&mut self, o: &Obs) -> LawAction {
            self.seen.lock().unwrap().push(observation_json(o));
            LawAction::Veto
        }
    }

    #[test]
    fn runner_requests_post_vote_and_preserves_unauthorized_attempt() {
        let mut e = prepare(1, Benefit::Positive);
        for i in 0..3 {
            e.vote(i, VoteChoice::No);
        }
        let seen = Arc::new(Mutex::new(vec![]));
        let mut strategies: Vec<Box<dyn Strategy>> = (0..3)
            .map(|_| Box::new(ReviewControl { seen: seen.clone() }) as Box<dyn Strategy>)
            .collect();
        let logs = review_vetoes(&mut e.sim, &e.wallets, &mut strategies, &[A, B, C]);
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].err.as_deref(), Some("NotPresident"));
        assert!(logs[1].ok);
        assert!(e.sim.game.veto_pending);
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        for obs in seen.iter() {
            assert_eq!(obs["decision_stage"], "post_vote");
            assert_eq!(obs["vote_weight"], json!([12, 11, 9]));
        }
    }
    #[test]
    fn reachable_from_zero_and_weight_matches_actual_tally() {
        for mode in [0, 1] {
            let mut e = prepare(mode, Benefit::Positive);
            let obs = e.state();
            assert_eq!(obs["cash"], json!([52 * PESO, 41 * PESO, 42 * PESO]));
            assert_eq!(obs["goods"], json!([0, 1, 0]));
            assert_eq!(obs["influence"], json!([10, 11, 7]));
            assert_eq!(
                obs["vote_weight"],
                if mode == 1 {
                    json!([12, 11, 9])
                } else {
                    json!([10, 11, 7])
                }
            );
            e.vote(A, VoteChoice::Yes);
            e.vote(C, VoteChoice::Yes);
            e.vote(B, VoteChoice::No);
            e.advance(0);
            assert_eq!(
                e.sim.game.yes_influence as u64,
                obs["vote_weight"][A].as_u64().unwrap() + obs["vote_weight"][C].as_u64().unwrap()
            );
            assert_eq!(e.sim.game.no_influence, 11);
        }
    }
    #[test]
    fn all_benefit_signs_and_controls_are_distinguished() {
        let r = report();
        assert_eq!(r["cases"].as_array().unwrap().len(), 6);
        for row in r["metrics"].as_array().unwrap() {
            let pair = (
                row["unauthorized"]["numerator"].as_u64().unwrap(),
                row["missed"]["numerator"].as_u64().unwrap(),
            );
            let expected = match row["policy"].as_str().unwrap() {
                "Always" => (1, 0),
                "Never" => (0, 1),
                "President" => (0, 0),
                "MaxWeight" if row["mode"] == 1 => (1, 1),
                "MaxWeight" => (0, 0),
                _ => unreachable!(),
            };
            assert_eq!(pair, expected);
        }
    }
    #[test]
    fn veto_right_survives_vote_and_errors_do_not_mutate_state() {
        let mut e = prepare(1, Benefit::Positive);
        e.vote(B, VoteChoice::No);
        let before = e.sim.state_bytes();
        assert_eq!(
            e.decide(A, LawAction::Veto)["result"]["err"],
            "NotPresident"
        );
        assert_eq!(e.sim.state_bytes(), before);
        assert_eq!(e.decide(B, LawAction::Veto)["result"]["ok"], true);
        let before = e.sim.state_bytes();
        assert_eq!(
            e.decide(B, LawAction::Veto)["result"]["err"],
            "AlreadyVetoed"
        );
        assert_eq!(e.sim.state_bytes(), before);
    }
}
