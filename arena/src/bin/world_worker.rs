//! Isolated, paused JSONL world for an explicitly opted-in 2..100 agent experiment.
//! No server, wallet, network or real funds. All actions, transitions and settlement
//! use the canonical Rust rules. The optional classic-only winner policy supplies
//! one share to the canonical settlement primitive; ordinary admission and payouts
//! elsewhere are unchanged. The bounded menu is an experiment interface, not
//! a claim that every possible Alashi strategy is exposed. Wall time never advances
//! the game while a provider is queued. See tools/agent-bootstrap/world-run.mjs.
#![recursion_limit = "512"]
use alashi_rules::{anchor_lang::prelude::Pubkey, constants::*, logic::{compute_settlement, market_price, FactionSnapshot},
    sim::Simulator, state::{Phase, VoteChoice}};
use arena::{runner::{apply_action, apply_law, apply_market, try_settle, vote_weight, ActionLog, PayoutLine},
    strategies::{ActionAction, LawAction, MarketAction}};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::{self, BufRead, Write};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    actor: usize,
    action: Option<String>,
    params: Value,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum PayoutPolicy {
    #[default]
    CanonicalTop4,
    WinnerTakesAll,
}

impl PayoutPolicy {
    fn name(self) -> &'static str {
        match self { Self::CanonicalTop4 => "canonical_top4", Self::WinnerTakesAll => "winner_takes_all" }
    }
    fn shares(self) -> &'static [u64] {
        match self { Self::CanonicalTop4 => &PAYOUT_SHARES, Self::WinnerTakesAll => &[100] }
    }
    fn objective(self) -> &'static str {
        match self {
            Self::CanonicalTop4 => "Maximize final cash (cash plus hard currency in epoch1); goods do not count in final rank. Top four share the simulated bank after rake.",
            Self::WinnerTakesAll => "Compete against every other player to finish rank 1 by maximizing your own final cash. Exactly one winner receives the simulated bank after rake; every other player receives zero. Goods and influence do not count in final rank. Equal cash is broken by ascending wallet byte order, giving exactly one winner. Game cash and the fixed payout bank are separate; increasing game cash does not mint payout money.",
        }
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Init { experimental: bool, game_id: u64, seed: u64, agents: u8,
        #[serde(default)] epoch: u8, #[serde(default = "default_fee")] entry_fee: u64,
        #[serde(default)] payout_policy: PayoutPolicy },
    Snapshot,
    Apply { expected_step: u64, expected_state_hash: String, decisions: Vec<Decision> },
}

fn default_fee() -> u64 { 10 * PESO }
fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}
fn phase_name(p: Phase) -> &'static str {
    match p { Phase::Lobby => "lobby", Phase::Market => "market", Phase::Action => "action",
        Phase::Law => "law", Phase::Finished => "finished", Phase::Aborted => "aborted" }
}
fn vote_name(v: VoteChoice) -> &'static str {
    match v { VoteChoice::Yes => "yes", VoteChoice::No => "no", VoteChoice::Abstain => "abstain" }
}
fn law_name(card: u8) -> &'static str {
    match card { LAW_STATUS_QUO => "status_quo", LAW_TAX_10 => "tax_10_percent",
        LAW_TAX_20 => "tax_20_percent", LAW_SUBSIDY_PRODUCE => "production_subsidy_one_good",
        LAW_SUBSIDY_POOR => "poorest_gains_influence", LAW_SUBSIDY_RICH => "richest_gains_influence",
        LAW_EMBARGO => "next_round_price_minus_two", LAW_BOOM => "next_round_price_plus_two",
        LAW_AMNESTY => "promissory_amnesty", _ => "none" }
}

#[derive(Clone)]
struct World {
    sim: Simulator,
    seed: u64,
    step: u64,
    now: i64,
    post_vote: bool,
    payout_policy: PayoutPolicy,
    // Only the immediately preceding complete apply is retryable. It cannot
    // be executed twice, even if a response was lost after committing it.
    last_apply: Option<(Value, Value)>,
}

impl World {
    fn new(experimental: bool, game_id: u64, seed: u64, agents: u8, epoch: u8,
        entry_fee: u64) -> Result<Self, String> {
        if !experimental { return Err("explicit experimental:true is required".into()); }
        if !(MIN_FACTIONS..=100).contains(&agents) { return Err("agents must be 2..100".into()); }
        if epoch != EPOCH_CLASSIC && epoch != EPOCH_90S { return Err("epoch must be 0 or 1".into()); }
        if entry_fee == 0 || entry_fee > 1_000_000 * PESO { return Err("entry_fee out of bounds".into()); }
        let mut sim = Simulator::new(game_id, entry_fee, 1, ENTROPY_SLOTHASH);
        sim.game.epoch = epoch;
        for actor in 0..agents {
            let mut bytes = [0u8; 32];
            bytes[..8].copy_from_slice(&splitmix64(seed ^ splitmix64(actor as u64 + 1)).to_le_bytes());
            // This disjoint actor suffix guarantees uniqueness for all seeds.
            bytes[8] = actor;
            bytes[31] = 1; // Never alias the canonical "no president/holder" sentinel.
            sim.join_for_experiment(Pubkey::new_from_array(bytes), &format!("agent-{actor:03}"), agents)
                .map_err(|e| e.to_string())?;
        }
        let now = 1_000_000;
        // An expired logical Lobby deadline starts 100 actors without changing
        // standard MAX_FACTIONS or any serialized on-chain account definition.
        sim.advance(now, splitmix64(seed ^ 0xABCD)).map_err(|e| e.to_string())?;
        Ok(Self { sim, seed, step: 0, now, post_vote: false,
            payout_policy: PayoutPolicy::CanonicalTop4, last_apply: None })
    }

    fn new_with_policy(experimental: bool, game_id: u64, seed: u64, agents: u8,
        epoch: u8, entry_fee: u64, payout_policy: PayoutPolicy) -> Result<Self, String> {
        // Epoch 1 has separate license/factory recipients. Silently redirecting
        // those payments would change more than the requested classic prize rule.
        if payout_policy == PayoutPolicy::WinnerTakesAll && epoch != EPOCH_CLASSIC {
            return Err("winner_takes_all requires epoch 0".into());
        }
        let mut world = Self::new(experimental, game_id, seed, agents, epoch, entry_fee)?;
        world.payout_policy = payout_policy;
        Ok(world)
    }

    fn stage(&self) -> &'static str {
        if self.sim.game.phase == Phase::Finished { "finished" }
        else if self.post_vote { "post_vote" } else { "primary" }
    }

    fn state_hash(&self) -> String {
        let mut h = Sha256::new();
        h.update(b"alashi-world-v1\0");
        // Keep historical/default state hashes byte-for-byte stable while
        // binding the opt-in prize contract to every winner-mode request.
        if self.payout_policy == PayoutPolicy::WinnerTakesAll {
            h.update(b"payout_policy=winner_takes_all\0");
        }
        h.update(self.seed.to_le_bytes());
        h.update(self.step.to_le_bytes());
        h.update(self.now.to_le_bytes());
        h.update([self.post_vote as u8]);
        h.update(self.sim.round_seed.to_le_bytes());
        let (game, factions) = self.sim.state_bytes();
        for bytes in std::iter::once(game).chain(factions) {
            h.update((bytes.len() as u64).to_le_bytes());
            h.update(bytes);
        }
        format!("{:x}", h.finalize())
    }

    fn president(&self) -> Option<usize> {
        self.sim.factions.iter().position(|f| f.alive && f.wallet == self.sim.game.president)
    }

    fn remaining_market_phases(&self) -> u8 {
        match self.sim.game.phase {
            Phase::Market => ROUNDS.saturating_sub(self.sim.game.round).saturating_add(1),
            Phase::Action | Phase::Law => ROUNDS.saturating_sub(self.sim.game.round),
            _ => 0,
        }
    }

    fn settlement(&self) -> Result<(Vec<usize>, Vec<u64>, u64, u64, Vec<PayoutLine>), String> {
        // Reuse canonical ranking, checked bank computation and legacy epoch
        // settlement. No state mutation or standalone ranking algorithm here.
        let legacy = try_settle(&self.sim, self.sim.game.entry_fee, false)?;
        if self.payout_policy == PayoutPolicy::CanonicalTop4 { return Ok(legacy); }
        if self.sim.game.epoch != EPOCH_CLASSIC { return Err("winner_takes_all requires epoch 0".into()); }
        let (order, _, _, bank, _) = legacy;
        let factions: Vec<_> = self.sim.factions.iter().map(FactionSnapshot::from).collect();
        let plan = compute_settlement(&factions, bank, 0, self.sim.game.rake_bps,
            self.payout_policy.shares()).map_err(|e| e.to_string())?;
        let mut payouts = vec![0; factions.len()];
        let mut breakdown: Vec<_> = (0..factions.len()).map(|_| PayoutLine {
            rank_share: 0, license_rent: 0, factory_bonus: 0 }).collect();
        for line in plan.payouts {
            payouts[line.faction_index] = line.amount;
            breakdown[line.faction_index].rank_share = line.amount;
        }
        Ok((order, payouts, plan.rake, bank, breakdown))
    }

    fn required_actors(&self) -> Vec<usize> {
        if self.sim.game.phase == Phase::Finished { return vec![]; }
        if self.post_vote { return self.president().into_iter().collect(); }
        let mut actors: Vec<_> = self.sim.factions.iter().enumerate()
            .filter_map(|(i, f)| f.alive.then_some(i)).collect();
        if !actors.is_empty() {
            let n = actors.len();
            // A seed-dependent start, then rotating priority. The queue completion
            // order never decides the execution order of economic actions.
            let start = (splitmix64(self.seed) % n as u64 + self.step % n as u64) % n as u64;
            actors.rotate_left(start as usize);
        }
        actors
    }

    /// Wealth/influence-blind candidate visibility, separate from action legality.
    /// Every live actor uses the same nonzero offsets in one seeded live ring:
    /// each recipient therefore appears once per slot and k times per full cohort.
    /// With an unchanged 100-player cohort, six rounds use 18 distinct offsets,
    /// hence 18 distinct peers per actor. A changed live set rebuilds the ring;
    /// repeated peers across that change are possible. Balancing candidates does
    /// not promise balanced legal bribe menus or actual transfers (cash matters).
    fn interaction_peers(&self, actor: usize) -> Vec<usize> {
        let mut ring: Vec<_> = self.sim.factions.iter().enumerate()
            .filter_map(|(i, f)| f.alive.then_some(i)).collect();
        ring.sort_unstable_by_key(|&i| (splitmix64(self.seed ^ 0x70656572736c6976 ^ i as u64), i));
        let Some(position) = ring.iter().position(|&i| i == actor) else { return vec![]; };
        let n = ring.len();
        if n < 2 { return vec![]; }
        let k = 3.min(n - 1);
        let mut offsets: Vec<_> = (1..n).collect();
        offsets.sort_unstable_by_key(|&offset|
            (splitmix64(self.seed ^ 0x706565726f666673 ^ offset as u64), offset));
        let start = self.sim.game.round.saturating_sub(1) as usize * k;
        (0..k).map(|slot| ring[(position + offsets[(start + slot) % (n - 1)]) % n]).collect()
    }

    fn allowed_actions(&self, actor: usize) -> Vec<Value> {
        let f = &self.sim.factions[actor];
        let mut candidates = vec![json!({"action":"pass","params":{}})];
        let mut add = |action: &str, params: Value| {
            let v = json!({"action":action,"params":params});
            if !candidates.contains(&v) { candidates.push(v); }
        };
        match self.sim.game.phase {
            Phase::Market => {
                add("sell", json!({"units":1}));
                add("sell", json!({"units":f.goods}));
                add("buy", json!({"units":1}));
                add("buy", json!({"units":2}));
                if self.sim.game.epoch == EPOCH_90S {
                    add("sell_credit", json!({"units":1}));
                    add("sell_credit", json!({"units":f.goods}));
                }
            },
            Phase::Action => {
                add("produce", json!({}));
                add("donkey", json!({}));
                if self.sim.game.epoch == EPOCH_90S { add("shuttle", json!({})); }
                let peers = self.interaction_peers(actor);
                for &to in &peers {
                    add("bribe", json!({"to":to,"amount":BRIBE_PRICE}));
                }
                if self.sim.game.epoch == EPOCH_90S {
                    if let Some(&to) = peers.first() {
                        add("roof", json!({"to":to,"tariff":ROOF_BLACK}));
                        add("roof", json!({"to":to,"tariff":ROOF_RED}));
                    }
                    add("bid", json!({"amount":PESO}));
                }
            },
            Phase::Law => {
                if !self.post_vote {
                    for action in ["vote_yes", "vote_no", "vote_abstain"] { add(action, json!({})); }
                }
                add("veto", json!({}));
            },
            _ => {},
        }
        candidates.into_iter().filter(|v| {
            let d = Decision { actor, action: v["action"].as_str().map(str::to_owned),
                params: v["params"].clone(), error: None };
            let mut sim = self.sim.clone();
            execute(&mut sim, self.post_vote, &d).map(|r| r.ok).unwrap_or(false)
        }).collect()
    }

    fn snapshot(&self) -> Result<Value, String> {
        let g = &self.sim.game;
        let required = self.required_actors();
        let settlement = if self.payout_policy == PayoutPolicy::WinnerTakesAll || g.phase == Phase::Finished {
            Some(self.settlement()?)
        } else { None };
        let mut ranks = vec![0; self.sim.factions.len()];
        let mut leader_score = 0;
        if let Some((order, _, _, _, _)) = &settlement {
            for (rank, &actor) in order.iter().enumerate() { ranks[actor] = rank + 1; }
            if let Some(&actor) = order.first() { leader_score = self.sim.factions[actor].cash; }
        }
        let public_state: Vec<_> = self.sim.factions.iter().enumerate().map(|(actor, f)| json!({
            "actor":actor,"name":f.name,"cash":f.cash,"hard":f.hard,"goods":f.goods,
            "influence":f.influence,"vote_weight":vote_weight(g,f),"alive":f.alive,
            "vote":vote_name(f.vote),"voted":f.voted_stamp==g.stamp(),
            "is_president":f.wallet==g.president})).collect();
        let agents: Vec<_> = required.iter().enumerate().map(|(position, &actor)| {
            let mut agent = json!({"actor":actor,
                "name":self.sim.factions[actor].name,"observation":public_state[actor],
                "interaction_peers":self.interaction_peers(actor),
                "allowed_actions":self.allowed_actions(actor)});
            if self.payout_policy == PayoutPolicy::WinnerTakesAll {
                let score = self.sim.factions[actor].cash;
                agent["competition"] = json!({"rank":ranks[actor],"score":score,
                    "leader_score":leader_score,"gap_to_leader":leader_score - score,
                    "execution_position":position + 1});
            }
            agent
        }).collect();
        let (yes, no) = alashi_rules::logic::tally_votes(&self.sim.factions.iter()
            .filter(|f| f.alive && f.voted_stamp == g.stamp())
            .map(|f| (vote_weight(g, f), f.vote)).collect::<Vec<_>>());
        let result = if g.phase == Phase::Finished {
            let (order, payouts, rake, bank, breakdown) = settlement.as_ref().ok_or("missing settlement")?;
            let leaderboard: Vec<_> = order.iter().enumerate().map(|(rank, &actor)| json!({
                "rank":rank+1,"actor":actor,"name":self.sim.factions[actor].name,
                "cash":self.sim.factions[actor].cash,"hard":self.sim.factions[actor].hard,
                "goods":self.sim.factions[actor].goods,"influence":self.sim.factions[actor].influence,
                "payout":payouts[actor]})).collect();
            json!({"leaderboard":leaderboard,"order":order,"payouts":payouts,
                "rake":rake,"bank":bank,"payout_breakdown":breakdown,"settlement_mode":"simulated",
                "winner_actor":order.first().copied().ok_or("empty ranking")?,
                "payout_policy":self.payout_policy.name()})
        } else { Value::Null };
        let mut snapshot = json!({"ok":true,"type":"snapshot","protocol":"alashi-world-v1",
            "step":self.step,"game_id":g.game_id,"round":g.round,
            "phase":phase_name(g.phase),"stage":self.stage(),"state_hash":self.state_hash(),
            "required_actors":required,"agents":agents,"result":result,
            "world":{"experimental":true,"agents":self.sim.factions.len(),"epoch":g.epoch,
                "seed":self.seed,"rounds":ROUNDS,"money":"simulated","unit":"micro_peso","peso":PESO,
                "entry_fee":g.entry_fee,"rake_bps":g.rake_bps,"payout_shares":self.payout_policy.shares(),
                "payout_policy":self.payout_policy.name(),
                "vote_weight_mode":g.vote_weight_mode,"logical_time":self.now,"deadline":g.phase_ends_at,
                "objective":self.payout_policy.objective(),
                "action_rules":{"produce_goods":PRODUCE_YIELD + g.active_subsidy_goods as u16,
                    "donkey_cost":DONKEY_PRICE * PESO,"donkey_goods":1,
                    "bribe":"Transfer amount to target; gain one influence per 5000000. Highest influence becomes president; ties use wallet order.",
                    "market":"Sell lowers the shared price; buy raises it. A trade uses the price of each unit in sequence, and sales pay the active tax.",
                    "law":"Votes are weighted by influence. Yes must exceed no. President may veto after the votes; a veto blocks the law.",
                    "pass":"Leave this decision unused; no action is invented on provider failure."},
                "turn_order":required,"execution_policy":"common_snapshot_seeded_rotating_order",
                "interaction_policy":"balanced_cyclic_peers_v1",
                "menu_scope":"bounded_subset_of_canonical_actions","market_price":market_price(g.sold_this_round,g.active_price_shift,g.active_boom),
                "sold_this_round":g.sold_this_round,"active_tax_bps":g.active_tax_bps,
                "active_subsidy_goods":g.active_subsidy_goods,"active_price_shift":g.active_price_shift,
                "active_boom":g.active_boom,"law_card":g.law_card,"law_name":law_name(g.law_card),
                "veto_pending":g.veto_pending,"president":self.president(),
                "current_yes_weight":yes,"current_no_weight":no,"laws_passed":g.laws_passed,
                "last_law_passed":g.last_law_passed,"public_state":public_state}});
        if self.payout_policy == PayoutPolicy::WinnerTakesAll {
            snapshot["world"]["remaining_market_phases_including_current"] = json!(self.remaining_market_phases());
            snapshot["world"]["action_rules"]["produce_cash_cost"] = json!(0);
            snapshot["world"]["market_unit_prices_by_sold_count"] = json!((0..PRICE_TABLE.len())
                .map(|i| market_price(i as u16, g.active_price_shift, g.active_boom)).collect::<Vec<_>>());
            snapshot["world"]["market_price_curve_note"] = json!("Current modifiers only: index is cumulative sold count, and the last entry applies to all higher counts. Sales pay active_tax_bps. Future laws can change this curve; it is not a forecast or a promise of future execution order.");
        }
        Ok(snapshot)
    }

    fn apply(&mut self, raw: &Value, expected_step: u64, expected_hash: &str,
        decisions: &[Decision]) -> Result<Value, String> {
        if let Some((previous, response)) = &self.last_apply {
            if previous == raw { return Ok(response.clone()); }
        }
        if expected_step != self.step { return Err("stale or conflicting expected_step".into()); }
        if expected_hash != self.state_hash() { return Err("expected_state_hash mismatch".into()); }
        if self.sim.game.phase == Phase::Finished { return Err("world already finished".into()); }
        let required = self.required_actors();
        if decisions.len() != required.len() { return Err("decisions must cover every required actor".into()); }
        let mut by_actor: Vec<Option<&Decision>> = vec![None; self.sim.factions.len()];
        for d in decisions {
            if !required.contains(&d.actor) { return Err("unexpected actor".into()); }
            if by_actor[d.actor].replace(d).is_some() { return Err("duplicate actor".into()); }
            if !d.params.is_object() { return Err("params must be an object".into()); }
            if d.action.is_none() && d.error.as_deref().unwrap_or("").trim().is_empty() {
                return Err("null action requires a provider/parser failure reason".into());
            }
            if d.error.as_ref().map_or(false, |e| e.len() > 500) { return Err("error too long".into()); }
        }
        if required.iter().any(|&i| by_actor[i].is_none()) { return Err("missing actor".into()); }
        // Whole-phase atomicity: failures in advancing or serializing do not leave
        // a partially applied phase behind. Invalid individual decisions are receipts.
        let mut next = self.clone();
        next.last_apply = None;
        let before_hash = self.state_hash();
        let mut receipts = vec![];
        for actor in required {
            let d = by_actor[actor].expect("validated complete batch");
            let mut trial = next.sim.clone();
            let (accepted, skipped, error, canonical) = if d.action.is_none() {
                (false, true, d.error.clone(), Value::Null)
            } else {
                match execute(&mut trial, next.post_vote, d) {
                    Ok(log) => {
                        let accepted = log.ok;
                        let error = log.err.clone();
                        if accepted { next.sim = trial; }
                        (accepted, false, error, serde_json::to_value(log).map_err(|e| e.to_string())?)
                    },
                    Err(e) => (false, false, Some(e), Value::Null),
                }
            };
            receipts.push(json!({"actor":actor,"action":d.action,"params":d.params,
                "accepted":accepted,"skipped":skipped,"error":error,"canonical":canonical,
                "cash_after":next.sim.factions[actor].cash,"goods_after":next.sim.factions[actor].goods}));
        }
        if next.sim.game.phase == Phase::Law && !next.post_vote
            && !next.sim.game.veto_pending && next.president().is_some() {
            // The incumbent sees the votes before deciding whether to veto.
            next.post_vote = true;
        } else {
            let g = &next.sim.game;
            let seed = splitmix64(next.seed ^ ((g.round as u64) << 8) ^ g.phase as u64);
            next.now = g.phase_ends_at.max(next.now);
            next.sim.advance(next.now, seed).map_err(|e| format!("advance: {e}"))?;
            next.post_vote = false;
        }
        next.step += 1;
        let response = json!({"ok":true,"type":"applied","step":self.step,
            "state_hash_before":before_hash,"state_hash_after":next.state_hash(),
            "receipts":receipts,"snapshot":next.snapshot()?});
        next.last_apply = Some((raw.clone(), response.clone()));
        *self = next;
        Ok(response)
    }
}

fn exact_params(params: &Value, keys: &[&str]) -> Result<(), String> {
    let map = params.as_object().ok_or("params must be an object")?;
    if map.len() != keys.len() || keys.iter().any(|k| !map.contains_key(*k)) {
        return Err(format!("expected params: {}", keys.join(",")));
    }
    Ok(())
}
fn number<T: TryFrom<u64>>(v: &Value, key: &str) -> Result<T, String> {
    v[key].as_u64().and_then(|n| n.try_into().ok()).ok_or_else(|| format!("invalid {key}"))
}

fn execute(sim: &mut Simulator, post_vote: bool, d: &Decision) -> Result<ActionLog, String> {
    if d.actor >= sim.factions.len() { return Err("actor out of bounds".into()); }
    let action = d.action.as_deref().ok_or("missing action")?;
    let params = &d.params;
    let keys: &[&str] = match action {
        "sell" | "buy" | "sell_credit" => &["units"],
        "bribe" => &["to", "amount"], "roof" => &["to", "tariff"], "bid" => &["amount"],
        "pass" | "produce" | "donkey" | "shuttle" | "vote_yes" | "vote_no" | "vote_abstain" | "veto" => &[],
        _ => return Err("unknown action".into()),
    };
    exact_params(params, keys)?;
    if let Some(to) = params.get("to") {
        if to.as_u64().map_or(true, |i| i >= sim.factions.len() as u64) { return Err("target out of bounds".into()); }
    }
    if post_vote && action != "pass" && action != "veto" { return Err("post_vote permits only president pass or veto".into()); }
    Ok(match sim.game.phase {
        Phase::Market => apply_market(sim, d.actor, &match action {
            "sell" => MarketAction::Sell(number(params, "units")?),
            "sell_credit" => MarketAction::SellCredit(number(params, "units")?),
            "buy" => MarketAction::Buy(number(params, "units")?),
            "pass" => MarketAction::Pass, _ => return Err("wrong phase".into()),
        }),
        Phase::Action => apply_action(sim, d.actor, &match action {
            "produce" => ActionAction::Produce, "donkey" => ActionAction::Donkey,
            "shuttle" => ActionAction::Shuttle, "pass" => ActionAction::Pass,
            "bribe" => ActionAction::Bribe { to: number(params,"to")?, amount: number(params,"amount")? },
            "roof" => ActionAction::Roof { to: number(params,"to")?, tariff: number(params,"tariff")? },
            "bid" => ActionAction::Bid(number(params,"amount")?), _ => return Err("wrong phase".into()),
        }),
        Phase::Law => {
            let wallet = sim.factions[d.actor].wallet;
            apply_law(sim, d.actor, &match action {
                "vote_yes" => LawAction::Vote(VoteChoice::Yes), "vote_no" => LawAction::Vote(VoteChoice::No),
                "vote_abstain" => LawAction::Vote(VoteChoice::Abstain),
                "veto" => LawAction::Veto, "pass" => LawAction::Pass,
                _ => return Err("wrong phase".into()),
            }, &wallet)
        },
        _ => return Err("phase is not playable".into()),
    })
}

#[derive(Default)]
struct Worker { world: Option<World> }
impl Worker {
    fn handle(&mut self, raw: Value) -> Value {
        let result = (|| -> Result<Value, String> {
            let request: Request = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
            match request {
                Request::Init { experimental, game_id, seed, agents, epoch, entry_fee, payout_policy } => {
                    if self.world.is_some() { return Err("already initialized".into()); }
                    let world = World::new_with_policy(experimental, game_id, seed, agents, epoch, entry_fee, payout_policy)?;
                    let snapshot = world.snapshot()?;
                    self.world = Some(world);
                    Ok(snapshot)
                },
                Request::Snapshot => self.world.as_ref().ok_or("not initialized")?.snapshot(),
                Request::Apply { expected_step, expected_state_hash, decisions } =>
                    self.world.as_mut().ok_or("not initialized")?.apply(&raw, expected_step, &expected_state_hash, &decisions),
            }
        })();
        result.unwrap_or_else(|error| json!({"ok":false,"type":"error","error":error}))
    }
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    let mut worker = Worker::default();
    for line in stdin.lock().lines() {
        let response = match line {
            Ok(line) if line.len() <= 1_000_000 => match serde_json::from_str(&line) {
                Ok(raw) => worker.handle(raw),
                Err(e) => json!({"ok":false,"type":"error","error":format!("invalid JSON: {e}")}),
            },
            Ok(_) => json!({"ok":false,"type":"error","error":"request too large"}),
            Err(_) => break,
        };
        if writeln!(stdout, "{response}").and_then(|_| stdout.flush()).is_err() { break; }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker(agents: u8) -> Worker {
        let mut worker = Worker::default();
        assert_eq!(worker.handle(json!({"op":"init","experimental":true,"game_id":1,
            "seed":42,"agents":agents}))["ok"], true);
        worker
    }
    fn batch(w: &Worker, action: &str) -> Value {
        let world = w.world.as_ref().unwrap();
        json!({"op":"apply","expected_step":world.step,"expected_state_hash":world.state_hash(),
            "decisions":world.required_actors().iter().map(|actor| json!({"actor":actor,
                "action":action,"params":{}})).collect::<Vec<_>>()})
    }

    #[test]
    fn hundred_share_six_rounds_and_canonical_settlement_replays() {
        let mut a = worker(100);
        let mut b = worker(100);
        let mut calls = 0;
        let mut completed_laws = 0;
        while a.world.as_ref().unwrap().sim.game.phase != Phase::Finished {
            let world = a.world.as_ref().unwrap();
            let action = match world.sim.game.phase {
                Phase::Action => "produce", Phase::Law if !world.post_vote => "vote_yes", _ => "pass" };
            let request = batch(&a, action);
            calls += request["decisions"].as_array().unwrap().len();
            if world.post_vote { completed_laws += 1; }
            let aa = a.handle(request.clone());
            let bb = b.handle(request);
            assert_eq!(aa["ok"], true, "{aa}");
            assert_eq!(aa, bb);
            assert!(aa["receipts"].as_array().unwrap().iter().all(|r| r["accepted"] == true));
        }
        assert_eq!(calls, 1806);
        assert_eq!(completed_laws, 6);
        let world = a.world.as_ref().unwrap();
        assert_eq!(world.sim.game.round, 6);
        assert_eq!(world.sim.factions.len(), 100);
        let (order, payouts, rake, bank, _) = try_settle(&world.sim, default_fee(), false).unwrap();
        let mut unique = order.clone(); unique.sort_unstable(); unique.dedup();
        assert_eq!(unique, (0..100).collect::<Vec<_>>());
        assert_eq!(payouts.iter().sum::<u64>() + rake, bank);
        assert_eq!(rake, bank * DEFAULT_RAKE_BPS as u64 / 10_000);
        assert_eq!(payouts.iter().filter(|&&p| p > 0).count(), 4);
        assert_eq!(a.handle(json!({"op":"snapshot"}))["result"]["leaderboard"].as_array().unwrap().len(), 100);
    }

    #[test]
    fn malformed_batches_never_mutate_and_exact_retry_is_idempotent() {
        let mut w = worker(100);
        let valid = batch(&w, "pass");
        let hash = w.world.as_ref().unwrap().state_hash();
        for kind in 0..4 {
            let mut bad = valid.clone();
            match kind {
                0 => { bad["decisions"][0]["actor"] = json!(100); },
                1 => { bad["decisions"][0]["actor"] = bad["decisions"][1]["actor"].clone(); },
                2 => { bad["decisions"].as_array_mut().unwrap().pop(); },
                _ => { bad["expected_state_hash"] = json!("wrong"); },
            }
            assert_eq!(w.handle(bad)["ok"], false);
            assert_eq!(w.world.as_ref().unwrap().state_hash(), hash);
        }
        let response = w.handle(valid.clone());
        assert_eq!(response["ok"], true);
        assert_eq!(w.handle(valid.clone()), response);
        let mut conflict = valid; conflict["decisions"][0]["action"] = json!("buy");
        let hash_after = w.world.as_ref().unwrap().state_hash();
        assert_eq!(w.handle(conflict)["ok"], false);
        assert_eq!(w.world.as_ref().unwrap().state_hash(), hash_after);
    }

    #[test]
    fn failures_are_receipts_and_president_has_exclusive_post_vote() {
        let mut w = worker(2);
        let mut request = batch(&w, "pass");
        request["decisions"][0]["action"] = Value::Null;
        request["decisions"][0]["error"] = json!("provider_timeout");
        request["decisions"][1]["action"] = json!("invented_action");
        let response = w.handle(request);
        assert_eq!(response["ok"], true);
        let receipts = response["receipts"].as_array().unwrap();
        assert_eq!(receipts.iter().filter(|r| r["skipped"] == true).count(), 1);
        assert!(receipts.iter().all(|r| r["accepted"] == false));
        assert!(receipts.iter().all(|r| r["cash_after"] == 0 && r["goods_after"] == 0));
        let request = batch(&w, "produce"); assert_eq!(w.handle(request)["ok"], true);
        let request = batch(&w, "vote_yes"); assert_eq!(w.handle(request)["ok"], true);
        let world = w.world.as_ref().unwrap();
        assert!(world.post_vote);
        let president = world.president().unwrap();
        assert_eq!(world.required_actors(), vec![president]);
        let mut invalid = batch(&w, "veto");
        invalid["decisions"][0]["actor"] = json!(1-president);
        let before = world.state_hash();
        assert_eq!(w.handle(invalid)["ok"], false);
        assert_eq!(w.world.as_ref().unwrap().state_hash(), before);
        let request = batch(&w, "veto");
        let response = w.handle(request);
        assert_eq!(response["receipts"][0]["accepted"], true);
        assert_eq!(response["snapshot"]["round"], 2);
        assert_eq!(response["snapshot"]["world"]["last_law_passed"], false);
    }

    #[test]
    fn offered_actions_are_bounded_and_valid_at_opening_snapshot() {
        let w = worker(100);
        let world = w.world.as_ref().unwrap();
        for actor in world.required_actors() {
            let allowed = world.allowed_actions(actor);
            assert!(!allowed.is_empty() && allowed.len() <= 15);
            for option in allowed {
                let d = Decision { actor, action: option["action"].as_str().map(str::to_owned),
                    params: option["params"].clone(), error: None };
                assert!(execute(&mut world.sim.clone(), false, &d).unwrap().ok);
            }
        }
    }

    fn assert_peer_exposure_is_balanced(world: &World) {
        let n = world.sim.factions.len();
        let live: Vec<_> = (0..n).filter(|&i| world.sim.factions[i].alive).collect();
        let k = 3.min(live.len().saturating_sub(1));
        let mut exposure = vec![0; n];
        let mut slots = vec![vec![0; n]; k];
        for actor in 0..n {
            let peers = world.interaction_peers(actor);
            assert_eq!(peers, world.interaction_peers(actor));
            if !world.sim.factions[actor].alive { assert!(peers.is_empty()); continue; }
            assert_eq!(peers.len(), k);
            let mut unique = peers.clone(); unique.sort_unstable(); unique.dedup();
            assert_eq!(unique.len(), k);
            for (slot, peer) in peers.into_iter().enumerate() {
                assert_ne!(peer, actor);
                assert!(world.sim.factions[peer].alive);
                exposure[peer] += 1;
                slots[slot][peer] += 1;
            }
        }
        for actor in 0..n {
            assert_eq!(exposure[actor], if world.sim.factions[actor].alive { k } else { 0 });
            for slot in &slots {
                assert_eq!(slot[actor], if world.sim.factions[actor].alive { 1 } else { 0 });
            }
        }
        assert!(world.interaction_peers(n).is_empty());
    }

    #[test]
    fn cyclic_peers_balance_every_recipient_and_slot_in_small_and_hundred_worlds() {
        for seed in [0, 42, u64::MAX] {
            for n in [2, 3, 4, 6, 19, 100] {
                let mut world = World::new(true, 1, seed, n, EPOCH_CLASSIC, default_fee()).unwrap();
                for round in 1..=ROUNDS {
                    world.sim.game.round = round;
                    assert_peer_exposure_is_balanced(&world);
                }
                for actor in (0..n as usize).step_by(3) { world.sim.factions[actor].alive = false; }
                for round in 1..=ROUNDS {
                    world.sim.game.round = round;
                    assert_peer_exposure_is_balanced(&world);
                }
                for f in &mut world.sim.factions { f.alive = false; }
                assert_peer_exposure_is_balanced(&world);
                world.sim.factions[0].alive = true;
                assert_peer_exposure_is_balanced(&world);
            }
        }
    }

    #[test]
    fn hundred_actors_receive_eighteen_distinct_peers_over_six_rounds() {
        for seed in [0, 42, 20261109, u64::MAX] {
            let mut world = World::new(true, 1, seed, 100, EPOCH_CLASSIC, default_fee()).unwrap();
            let mut seen = vec![std::collections::BTreeSet::new(); 100];
            for round in 1..=ROUNDS {
                world.sim.game.round = round;
                for (actor, peers) in seen.iter_mut().enumerate() {
                    for peer in world.interaction_peers(actor) { assert!(peers.insert(peer)); }
                }
            }
            assert!(seen.iter().all(|peers| peers.len() == 18));
        }
    }

    #[test]
    fn peers_ignore_wealth_influence_phase_and_execution_order() {
        let mut world = World::new(true, 1, 20261109, 100, EPOCH_CLASSIC, default_fee()).unwrap();
        for round in 1..=ROUNDS {
            world.sim.game.round = round;
            let expected: Vec<_> = (0..100).map(|actor| world.interaction_peers(actor)).collect();
            for (actor, f) in world.sim.factions.iter_mut().enumerate() {
                f.cash = (100 - actor as u64) * 13 * PESO;
                f.influence = (actor * 7 % MAX_INFLUENCE as usize) as u16;
            }
            for phase in [Phase::Market, Phase::Action, Phase::Law] {
                world.sim.game.phase = phase;
                world.step += 1;
                for (actor, peers) in expected.iter().enumerate() {
                    assert_eq!(&world.interaction_peers(actor), peers);
                }
            }
        }
    }

    #[test]
    fn snapshot_and_targeted_menus_share_the_same_ordered_peers_in_both_epochs() {
        for epoch in [EPOCH_CLASSIC, EPOCH_90S] {
            let mut world = World::new(true, 1, 20261109, 100, epoch, default_fee()).unwrap();
            world.sim.game.phase = Phase::Action;
            for f in &mut world.sim.factions { f.cash = 100 * PESO; }
            let snapshot = world.snapshot().unwrap();
            assert_eq!(snapshot["world"]["interaction_policy"], "balanced_cyclic_peers_v1");
            for agent in snapshot["agents"].as_array().unwrap() {
                let actor = agent["actor"].as_u64().unwrap() as usize;
                let peers = world.interaction_peers(actor);
                assert_eq!(agent["interaction_peers"], json!(peers));
                let menu = agent["allowed_actions"].as_array().unwrap();
                let bribes: Vec<_> = menu.iter().filter(|a| a["action"] == "bribe")
                    .map(|a| a["params"]["to"].as_u64().unwrap() as usize).collect();
                assert_eq!(bribes, peers);
                for action in menu.iter().filter(|a| a["params"].get("to").is_some()) {
                    assert!(peers.contains(&(action["params"]["to"].as_u64().unwrap() as usize)));
                }
                let roofs: Vec<_> = menu.iter().filter(|a| a["action"] == "roof").collect();
                if epoch == EPOCH_90S {
                    assert_eq!(roofs.len(), 2);
                    assert!(roofs.iter().all(|a| a["params"]["to"] == peers[0]));
                } else { assert!(roofs.is_empty()); }
            }
            // Insufficient cash removes legal bribes, not potential visibility.
            world.sim.factions[0].cash = 0;
            let snapshot = world.snapshot().unwrap();
            let agent = snapshot["agents"].as_array().unwrap().iter().find(|a| a["actor"] == 0).unwrap();
            assert_eq!(agent["interaction_peers"], json!(world.interaction_peers(0)));
            assert!(agent["allowed_actions"].as_array().unwrap().iter().all(|a| a["action"] != "bribe"));
        }
    }

    fn productive_batch(w: &Worker) -> Value {
        let world = w.world.as_ref().unwrap();
        let mut request = batch(w, "pass");
        for decision in request["decisions"].as_array_mut().unwrap() {
            let actor = decision["actor"].as_u64().unwrap() as usize;
            match world.sim.game.phase {
                Phase::Market if world.sim.factions[actor].goods > 0 => {
                    decision["action"] = json!("sell");
                    decision["params"] = json!({"units":world.sim.factions[actor].goods});
                },
                Phase::Action if world.sim.game.round < ROUNDS => decision["action"] = json!("produce"),
                Phase::Law if !world.post_vote => decision["action"] = json!("vote_yes"),
                _ => {},
            }
        }
        request
    }

    #[test]
    fn legacy_default_and_explicit_policy_preserve_both_epochs_at_one_hundred() {
        for epoch in [EPOCH_CLASSIC, EPOCH_90S] {
            let mut omitted = Worker::default();
            let mut explicit = Worker::default();
            let init = json!({"op":"init","experimental":true,"game_id":20261109,
                "seed":20261109,"agents":100,"epoch":epoch});
            let mut named_init = init.clone(); named_init["payout_policy"] = json!("canonical_top4");
            let initial = omitted.handle(init);
            assert_eq!(initial, explicit.handle(named_init));
            assert_eq!(initial["world"]["payout_policy"], "canonical_top4");
            assert_eq!(initial["world"]["payout_shares"], json!(PAYOUT_SHARES));
            assert!(initial["world"].get("remaining_market_phases_including_current").is_none());
            assert!(initial["world"]["action_rules"].get("produce_cash_cost").is_none());
            assert!(initial["world"].get("market_unit_prices_by_sold_count").is_none());
            assert!(initial["agents"].as_array().unwrap().iter().all(|a| a.get("competition").is_none()));
            if epoch == EPOCH_CLASSIC {
                // Recorded initial hash of the frozen second 100-agent game.
                assert_eq!(initial["state_hash"], "6769fd7d761ae0f9fca772ee83784c8848dc71772399a7f806a84cbc95c4af7b");
            }
            while omitted.world.as_ref().unwrap().sim.game.phase != Phase::Finished {
                let request = productive_batch(&omitted);
                let response = omitted.handle(request.clone());
                assert_eq!(response["ok"], true, "{response}");
                assert_eq!(response, explicit.handle(request));
            }
            let world = omitted.world.as_ref().unwrap();
            let (order, payouts, rake, bank, breakdown) = try_settle(&world.sim, default_fee(), false).unwrap();
            let result = world.snapshot().unwrap()["result"].clone();
            assert_eq!(result["order"], json!(order));
            assert_eq!(result["payouts"], json!(payouts));
            assert_eq!(result["rake"], rake);
            assert_eq!(result["bank"], bank);
            assert_eq!(result["payout_breakdown"], serde_json::to_value(breakdown).unwrap());
            assert_eq!(result["winner_actor"], order[0]);
            assert_eq!(result["payout_policy"], "canonical_top4");
        }
    }

    #[test]
    fn winner_policy_preserves_play_but_awards_the_classic_bank_to_exactly_one() {
        let mut legacy = worker(100);
        let mut winner = Worker::default();
        assert_eq!(winner.handle(json!({"op":"init","experimental":true,"game_id":1,
            "seed":42,"agents":100,"payout_policy":"winner_takes_all"}))["ok"], true);
        let mut batches = 0;
        while legacy.world.as_ref().unwrap().sim.game.phase != Phase::Finished {
            let legacy_request = productive_batch(&legacy);
            let winner_request = productive_batch(&winner);
            assert_eq!(legacy_request["decisions"], winner_request["decisions"]);
            let a = legacy.handle(legacy_request);
            let b = winner.handle(winner_request);
            assert_eq!(a["ok"], true, "{a}"); assert_eq!(b["ok"], true, "{b}");
            assert_eq!(a["receipts"], b["receipts"]);
            assert_eq!(legacy.world.as_ref().unwrap().sim.state_bytes(), winner.world.as_ref().unwrap().sim.state_bytes());
            batches += 1;
        }
        assert_eq!(batches, 24);
        let final_state = winner.world.as_ref().unwrap().snapshot().unwrap();
        let result = &final_state["result"];
        let payouts: Vec<u64> = serde_json::from_value(result["payouts"].clone()).unwrap();
        let actor = result["winner_actor"].as_u64().unwrap() as usize;
        assert_eq!(result["bank"], 1_000 * PESO);
        assert_eq!(result["rake"], 50 * PESO);
        assert_eq!(payouts.iter().filter(|&&p| p > 0).count(), 1);
        assert_eq!(payouts[actor], 950 * PESO);
        assert_eq!(payouts.iter().sum::<u64>() + 50 * PESO, 1_000 * PESO);
        assert_eq!(result["order"][0], actor);
        assert_eq!(result["leaderboard"][0]["actor"], actor);
        assert_eq!(result["leaderboard"][0]["payout"], payouts[actor]);
        assert_eq!(result["payout_breakdown"][actor]["rank_share"], payouts[actor]);
        assert_eq!(final_state["world"]["payout_shares"], json!([100]));
        assert_eq!(result["payout_policy"], "winner_takes_all");
        assert_eq!(final_state["world"]["remaining_market_phases_including_current"], 0);
    }

    #[test]
    fn winner_ties_use_canonical_wallet_order_without_goods_or_influence() {
        let mut world = World::new_with_policy(true, 1, 20261109, 100, EPOCH_CLASSIC,
            default_fee(), PayoutPolicy::WinnerTakesAll).unwrap();
        world.sim.game.phase = Phase::Finished;
        world.sim.game.round = ROUNDS;
        for (actor, f) in world.sim.factions.iter_mut().enumerate() {
            f.cash = 42 * PESO;
            f.goods = actor as u16;
            f.influence = (100 - actor) as u16;
        }
        let expected = world.sim.factions.iter().enumerate().min_by_key(|(_, f)| f.wallet).unwrap().0;
        let result = world.snapshot().unwrap()["result"].clone();
        assert_eq!(result["winner_actor"], expected);
        assert_eq!(result["order"][0], expected);
        let payouts: Vec<u64> = serde_json::from_value(result["payouts"].clone()).unwrap();
        assert_eq!(payouts.iter().filter(|&&p| p > 0).count(), 1);
        assert_eq!(payouts[expected], 950 * PESO);
        let (canonical_order, _, _, _, _) = try_settle(&world.sim, default_fee(), false).unwrap();
        assert_eq!(result["order"], json!(canonical_order));
    }

    #[test]
    fn policy_is_explicit_bounded_and_bound_to_the_state_hash() {
        let init = json!({"op":"init","experimental":true,"game_id":1,"seed":42,"agents":100});
        for bad_policy in [json!("winner"), json!(""), Value::Null, json!(1)] {
            let mut w = Worker::default();
            let mut bad = init.clone(); bad["payout_policy"] = bad_policy;
            assert_eq!(w.handle(bad)["ok"], false);
            assert!(w.world.is_none());
        }
        let mut bad = init.clone();
        bad["payout_policy"] = json!("winner_takes_all"); bad["epoch"] = json!(EPOCH_90S);
        let mut w = Worker::default();
        assert_eq!(w.handle(bad)["ok"], false);
        assert!(w.world.is_none());
        let mut legacy = Worker::default();
        let legacy_snapshot = legacy.handle(init.clone());
        let mut winner_init = init; winner_init["payout_policy"] = json!("winner_takes_all");
        let winner_snapshot = w.handle(winner_init);
        assert_eq!(winner_snapshot["ok"], true);
        assert_ne!(legacy_snapshot["state_hash"], winner_snapshot["state_hash"]);
        assert_eq!(legacy.world.as_ref().unwrap().sim.state_bytes(), w.world.as_ref().unwrap().sim.state_bytes());
        let wrong_policy_request = batch(&legacy, "pass");
        assert_eq!(w.handle(wrong_policy_request)["ok"], false);
        assert_eq!(w.handle(json!({"op":"snapshot"})), winner_snapshot);
    }

    #[test]
    fn winner_observations_expose_canonical_standings_and_inclusive_market_horizon() {
        let mut world = World::new_with_policy(true, 1, 42, 100, EPOCH_CLASSIC,
            default_fee(), PayoutPolicy::WinnerTakesAll).unwrap();
        world.sim.factions[0].cash = 10 * PESO;
        world.sim.factions[1].cash = 30 * PESO;
        world.sim.factions[2].cash = 20 * PESO;
        for round in 1..=ROUNDS {
            for phase in [Phase::Market, Phase::Action, Phase::Law] {
                world.sim.game.round = round;
                world.sim.game.phase = phase;
                let snapshot = world.snapshot().unwrap();
                let expected_horizon = ROUNDS - round + u8::from(phase == Phase::Market);
                assert_eq!(snapshot["world"]["remaining_market_phases_including_current"], expected_horizon);
                assert_eq!(snapshot["world"]["action_rules"]["produce_cash_cost"], 0);
                let (order, _, _, _, _) = try_settle(&world.sim, default_fee(), false).unwrap();
                for (position, agent) in snapshot["agents"].as_array().unwrap().iter().enumerate() {
                    let actor = agent["actor"].as_u64().unwrap() as usize;
                    assert_eq!(agent["competition"]["rank"], order.iter().position(|&i| i == actor).unwrap() + 1);
                    assert_eq!(agent["competition"]["score"], world.sim.factions[actor].cash);
                    assert_eq!(agent["competition"]["leader_score"], 30 * PESO);
                    assert_eq!(agent["competition"]["gap_to_leader"], 30 * PESO - world.sim.factions[actor].cash);
                    assert_eq!(agent["competition"]["execution_position"], position + 1);
                    assert_eq!(snapshot["required_actors"][position], actor);
                }
            }
        }
        world.post_vote = true;
        world.sim.game.president = world.sim.factions[0].wallet;
        let snapshot = world.snapshot().unwrap();
        assert_eq!(snapshot["required_actors"], json!([0]));
        assert_eq!(snapshot["agents"][0]["competition"]["execution_position"], 1);
        assert_eq!(snapshot["agents"][0]["competition"]["rank"], 3);
    }

    #[test]
    fn winner_market_curve_uses_current_canonical_modifiers_and_exposes_tax() {
        let mut world = World::new_with_policy(true, 1, 42, 100, EPOCH_CLASSIC,
            default_fee(), PayoutPolicy::WinnerTakesAll).unwrap();
        world.sim.game.active_tax_bps = 1_000;
        world.sim.game.sold_this_round = 30;
        for (shift, boom) in [(0, 0), (-2, 0), (0, 2), (-2, 2)] {
            world.sim.game.active_price_shift = shift;
            world.sim.game.active_boom = boom;
            let snapshot = world.snapshot().unwrap();
            let prices: Vec<u64> = serde_json::from_value(snapshot["world"]["market_unit_prices_by_sold_count"].clone()).unwrap();
            assert_eq!(prices.len(), PRICE_TABLE.len());
            for (sold, &price) in prices.iter().enumerate() {
                assert_eq!(price, market_price(sold as u16, shift, boom));
            }
            assert_eq!(*prices.last().unwrap(), market_price(30, shift, boom));
            assert_eq!(snapshot["world"]["market_price"], *prices.last().unwrap());
            assert_eq!(snapshot["world"]["active_tax_bps"], 1_000);
            assert!(snapshot["world"]["market_price_curve_note"].as_str().unwrap().contains("Current modifiers only"));
        }
    }
}
