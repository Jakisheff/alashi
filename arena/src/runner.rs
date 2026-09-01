//! Прогонщик off-chain партий: одна партия = чистые правила + стратегии.
//! Времени реального нет: виртуальные часы двигаются фазами, энтропия
//! детерминирована seed → серия воспроизводима побайтово.

use crate::strategies::{ActionAction, LawAction, MarketAction, Obs, Strategy};
use alashi_rules::anchor_lang::prelude::Pubkey;
use alashi_rules::constants::*;
use alashi_rules::logic::compute_settlement;
use alashi_rules::sim::Simulator;
use alashi_rules::state::{Phase, VoteChoice};
use alashi_rules::FactionSnapshot;
use serde::Serialize;

fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

#[derive(Serialize, Clone)]
pub struct ActionLog {
    pub phase: &'static str,
    pub actor: usize,
    pub action: String,
    pub detail: serde_json::Value,
    pub ok: bool,
    pub err: Option<String>,
    pub cash_after: Option<u64>,
    pub goods_after: Option<u16>,
}

#[derive(Serialize, Clone)]
pub struct PhaseLog {
    pub round: u8,
    pub phase: &'static str,
    pub actions: Vec<ActionLog>,
    pub law_card: Option<u8>,
    pub law_passed: Option<bool>,
    pub vetoed: Option<bool>,
    pub yes_influence: Option<u32>,
    pub no_influence: Option<u32>,
}

#[derive(Serialize)]
pub struct GameRecord {
    pub game_id: u64,
    pub seed: u64,
    pub entry_fee: u64,
    pub vote_weight_mode: u8,
    pub epoch: u8,
    pub n_factions: usize,
    pub strategies: Vec<&'static str>,
    pub phases: Vec<PhaseLog>,
    pub final_cash: Vec<u64>,
    pub final_goods: Vec<u16>,
    pub final_influence: Vec<u16>,
    /// Индексы фракций по убыванию выплаты (ранг 0 = богатейший).
    pub ranks: Vec<usize>,
    pub payouts: Vec<u64>,
    pub rake: u64,
    pub bank: u64,
}

pub struct GameConfig {
    pub entry_fee: u64,
    pub phase_duration: i64,
    pub vote_weight_mode: u8,
    pub epoch: u8,
}

impl Default for GameConfig {
    fn default() -> Self {
        GameConfig {
            entry_fee: 10 * PESO,
            phase_duration: 10,
            vote_weight_mode: alashi_rules::constants::VOTE_WEIGHT_LEGACY,
            epoch: alashi_rules::constants::EPOCH_CLASSIC,
        }
    }
}

fn err_str(e: alashi_rules::error::GameError) -> String {
    format!("{:?}", e)
}

/// Наблюдение без аллокаций-утечек: Vec живут в замыкании.
fn with_obs<R>(
    sim: &Simulator,
    wallets: &[Pubkey],
    my_idx: usize,
    f: impl FnOnce(&Obs) -> R,
) -> R {
    let g = &sim.game;
    let cash: Vec<u64> = sim.factions.iter().map(|x| x.cash).collect();
    let goods: Vec<u16> = sim.factions.iter().map(|x| x.goods).collect();
    let influence: Vec<u16> = sim.factions.iter().map(|x| x.influence).collect();
    let alive: Vec<bool> = sim.factions.iter().map(|x| x.alive).collect();
    let obs = Obs {
        round: g.round,
        my_idx,
        n_factions: sim.factions.len(),
        cash: &cash,
        goods: &goods,
        influence: &influence,
        alive: &alive,
        sold_counter: g.sold_this_round,
        active_tax_bps: g.active_tax_bps,
        active_price_shift: g.active_price_shift,
        active_boom: g.active_boom,
        law_card: g.law_card,
        president: g.president,
        my_wallet: wallets[my_idx],
    };
    f(&obs)
}

pub fn play_game(
    game_id: u64,
    seed: u64,
    strategies: &mut [Box<dyn Strategy>],
    cfg: &GameConfig,
) -> GameRecord {
    let n = strategies.len();
    assert!(n >= MIN_FACTIONS as usize && n <= MAX_FACTIONS as usize);
    let mut sim = Simulator::new(game_id, cfg.entry_fee, cfg.phase_duration, ENTROPY_SLOTHASH);
    sim.game.vote_weight_mode = cfg.vote_weight_mode;
    sim.game.epoch = cfg.epoch;
    // Кошельки детерминированы из seed: уникальны и воспроизводимы.
    let wallets: Vec<Pubkey> = (0..n)
        .map(|i| {
            let h = splitmix64(seed ^ splitmix64(i as u64 + 1));
            let mut a = [0u8; 32];
            a[..8].copy_from_slice(&h.to_le_bytes());
            Pubkey::new_from_array(a)
        })
        .collect();
    for (i, s) in strategies.iter().enumerate() {
        sim.join(wallets[i], &format!("{}-{}", s.name(), i))
            .expect("join в лобби не падает");
    }

    let mut now: i64 = 1_000_000;
    let mut phases: Vec<PhaseLog> = Vec::new();
    // Лобби: состав полный.
    sim.advance(now, splitmix64(seed ^ 0xABCD))
        .expect("старт из лобби");

    while sim.game.phase != Phase::Finished {
        let phase = sim.game.phase;
        let round = sim.game.round;
        let phase_name: &'static str = match phase {
            Phase::Market => "market",
            Phase::Action => "action",
            Phase::Law => "law",
            _ => unreachable!(),
        };
        let mut actions: Vec<ActionLog> = Vec::new();
        // Ротация первого хода: честнее на падающей цене.
        let order: Vec<usize> = (0..n).map(|k| (round as usize + k) % n).collect();
        for &i in &order {
            match phase {
                Phase::Market => {
                    let act = with_obs(&sim, &wallets, i, |o| strategies[i].market(o));
                    actions.push(apply_market(&mut sim, i, &act));
                }
                Phase::Action => {
                    let act = with_obs(&sim, &wallets, i, |o| strategies[i].action(o));
                    actions.push(apply_action(&mut sim, i, &act));
                }
                Phase::Law => {
                    let act = with_obs(&sim, &wallets, i, |o| strategies[i].law(o));
                    actions.push(apply_law(&mut sim, i, &act, &wallets[i]));
                }
                _ => unreachable!(),
            }
        }
        let res = sim
            .advance(
                now + cfg.phase_duration,
                splitmix64(seed ^ ((round as u64) << 8) ^ (phase as u64)),
            )
            .expect("advance внутри партии");
        phases.push(PhaseLog {
            round,
            phase: phase_name,
            actions,
            law_card: res.law_card_drawn,
            law_passed: if phase == Phase::Law {
                Some(sim.game.last_law_passed)
            } else {
                None
            },
            vetoed: if phase == Phase::Law {
                Some(res.vetoed)
            } else {
                None
            },
            yes_influence: if phase == Phase::Law {
                Some(sim.game.yes_influence)
            } else {
                None
            },
            no_influence: if phase == Phase::Law {
                Some(sim.game.no_influence)
            } else {
                None
            },
        });
        now += cfg.phase_duration;
    }

    // Settle: банк = взносы, резерва нет (нет рента), рейк по правилам.
    let (ranks, payouts, rake, bank) = settle(&sim, cfg.entry_fee);
    let rake = rake;

    GameRecord {
        game_id,
        seed,
        entry_fee: cfg.entry_fee,
        vote_weight_mode: cfg.vote_weight_mode,
        epoch: cfg.epoch,
        n_factions: n,
        strategies: strategies.iter().map(|s| s.name()).collect(),
        phases,
        final_cash: sim.factions.iter().map(|f| f.cash).collect(),
        final_goods: sim.factions.iter().map(|f| f.goods).collect(),
        final_influence: sim.factions.iter().map(|f| f.influence).collect(),
        ranks,
        payouts,
        rake,
        bank,
    }
}

/// Settle по правилам для off-chain партии: банк = взносы, резерв 0.
pub fn settle(sim: &Simulator, entry_fee: u64) -> (Vec<usize>, Vec<u64>, u64, u64) {
    let n = sim.factions.len();
    let bank = entry_fee * n as u64;
    let snaps: Vec<FactionSnapshot> = sim
        .factions
        .iter()
        .map(|f| FactionSnapshot {
            wallet: f.wallet,
            cash: f.cash,
            influence: f.influence,
            alive: f.alive,
        })
        .collect();
    let plan = compute_settlement(&snaps, bank, 0, DEFAULT_RAKE_BPS, &PAYOUT_SHARES)
        .expect("settle валиден");
    let mut payouts = vec![0u64; n];
    let mut ranks: Vec<usize> = Vec::new();
    for p in &plan.payouts {
        payouts[p.faction_index] = p.amount;
        ranks.push(p.faction_index);
    }
    // Фракции вне долей (5-е место) идут после по cash.
    let mut rest: Vec<usize> = (0..n).filter(|i| !ranks.contains(i)).collect();
    rest.sort_by(|&a, &b| sim.factions[b].cash.cmp(&sim.factions[a].cash));
    ranks.extend(rest);
    // SPEC_EPOCH_90S M5 «завод»: фракция с макс влиянием получает 5%
    // банка из рейка (при равенстве влияния — лучший ранг по cash).
    let mut rake = plan.rake;
    if sim.game.epoch == EPOCH_90S && n > 0 {
        let bonus = bank * FACTORY_NUM / FACTORY_DEN;
        if bonus > 0 && rake >= bonus {
            let mut best = ranks.first().copied().unwrap_or(0);
            let mut best_key = (0u16, 0u64);
            for &i in &ranks {
                let f = &sim.factions[i];
                let key = (f.influence, f.cash);
                if key > best_key {
                    best_key = key;
                    best = i;
                }
            }
            payouts[best] += bonus;
            rake -= bonus;
        }
    }
    (ranks, payouts, rake, bank)
}

pub fn apply_market(sim: &mut Simulator, i: usize, act: &MarketAction) -> ActionLog {
    let phase = "market";
    match act {
        MarketAction::Sell(units) => match sim.sell(i, *units) {
            Ok(gross) => {
                let f = &sim.factions[i];
                ActionLog {
                    phase,
                    actor: i,
                    action: "sell".into(),
                    detail: serde_json::json!({
                        "units": units,
                        "revenue": gross, // net после налога
                        "tax_bps": sim.game.active_tax_bps,
                        "counter_after": sim.game.sold_this_round,
                        "shift": sim.game.active_price_shift,
                        "boom": sim.game.active_boom,
                    }),
                    ok: true,
                    err: None,
                    cash_after: Some(f.cash),
                    goods_after: Some(f.goods),
                }
            }
            Err(e) => fail(phase, i, "sell", serde_json::json!({"units": units}), e),
        },
        MarketAction::SellCredit(units) => match sim.sell_credit(i, *units) {
            Ok(gross) => {
                let f = &sim.factions[i];
                ActionLog {
                    phase,
                    actor: i,
                    action: "sell_credit".into(),
                    detail: serde_json::json!({
                        "units": units,
                        "promissory": gross,
                        "tax_bps": sim.game.active_tax_bps,
                        "counter_after": sim.game.sold_this_round,
                        "shift": sim.game.active_price_shift,
                        "boom": sim.game.active_boom,
                    }),
                    ok: true,
                    err: None,
                    cash_after: Some(f.cash),
                    goods_after: Some(f.goods),
                }
            }
            Err(e) => fail(phase, i, "sell_credit", serde_json::json!({"units": units}), e),
        },
        MarketAction::Buy(units) => match sim.buy(i, *units) {
            Ok(cost) => {
                let f = &sim.factions[i];
                ActionLog {
                    phase,
                    actor: i,
                    action: "buy".into(),
                    detail: serde_json::json!({
                        "units": units,
                        "cost": cost,
                        "counter_after": sim.game.sold_this_round,
                        "shift": sim.game.active_price_shift,
                        "boom": sim.game.active_boom,
                    }),
                    ok: true,
                    err: None,
                    cash_after: Some(f.cash),
                    goods_after: Some(f.goods),
                }
            }
            Err(e) => fail(phase, i, "buy", serde_json::json!({"units": units}), e),
        },
        MarketAction::Pass => {
            let f = &sim.factions[i];
            ActionLog {
                phase,
                actor: i,
                action: "pass".into(),
                detail: serde_json::json!({}),
                ok: true,
                err: None,
                cash_after: Some(f.cash),
                goods_after: Some(f.goods),
            }
        }
    }
}

pub fn apply_action(sim: &mut Simulator, i: usize, act: &ActionAction) -> ActionLog {
    let phase = "action";
    match act {
        ActionAction::Produce => match sim.produce(i) {
            Ok(_) => ok(phase, i, "produce", serde_json::json!({}), &sim.factions[i]),
            Err(e) => fail(phase, i, "produce", serde_json::json!({}), e),
        },
        ActionAction::Donkey => match sim.donkey(i) {
            Ok(_) => ok(phase, i, "donkey", serde_json::json!({}), &sim.factions[i]),
            Err(e) => fail(phase, i, "donkey", serde_json::json!({}), e),
        },
        ActionAction::Bribe { to, amount } => match sim.bribe(i, *to, *amount) {
            Ok(infl) => {
                let f = &sim.factions[i];
                ActionLog {
                    phase,
                    actor: i,
                    action: "bribe".into(),
                    detail: serde_json::json!({"to": to, "amount": amount, "influence_after": infl}),
                    ok: true,
                    err: None,
                    cash_after: Some(f.cash),
                    goods_after: Some(f.goods),
                }
            }
            Err(e) => fail(phase, i, "bribe", serde_json::json!({"to": to, "amount": amount}), e),
        },
        ActionAction::Pass => ok(phase, i, "pass", serde_json::json!({}), &sim.factions[i]),
        ActionAction::Shuttle => match sim.shuttle(i) {
            Ok(goods) => ok(
                phase,
                i,
                "shuttle",
                serde_json::json!({"goods_after": goods, "grey": sim.factions[i].grey_goods}),
                &sim.factions[i],
            ),
            Err(e) => fail(phase, i, "shuttle", serde_json::json!({}), e),
        },
        ActionAction::Roof { to } => match sim.roof(i, *to) {
            Ok(roof_to) => {
                let f = &sim.factions[i];
                ActionLog {
                    phase,
                    actor: i,
                    action: "roof".into(),
                    detail: serde_json::json!({"to": roof_to, "price": f.cash * ROOF_NUM / ROOF_DEN}),
                    ok: true,
                    err: None,
                    cash_after: Some(f.cash),
                    goods_after: Some(f.goods),
                }
            }
            Err(e) => fail(phase, i, "roof", serde_json::json!({"to": to}), e),
        },
    }
}

pub fn apply_law(sim: &mut Simulator, i: usize, act: &LawAction, wallet: &Pubkey) -> ActionLog {
    let phase = "law";
    match act {
        LawAction::Vote(choice) => {
            let label = match choice {
                VoteChoice::Yes => "vote_yes",
                VoteChoice::No => "vote_no",
                VoteChoice::Abstain => "vote_abstain",
            };
            match sim.vote(i, *choice) {
                Ok(_) => ActionLog {
                    phase,
                    actor: i,
                    action: label.into(),
                    detail: serde_json::json!({"card": sim.game.law_card}),
                    ok: true,
                    err: None,
                    cash_after: None,
                    goods_after: None,
                },
                Err(e) => fail(phase, i, label, serde_json::json!({}), e),
            }
        }
        LawAction::Veto => match sim.veto(i) {
            Ok(_) => ActionLog {
                phase,
                actor: i,
                action: "veto".into(),
                detail: serde_json::json!({"president": wallet.to_string()}),
                ok: true,
                err: None,
                cash_after: None,
                goods_after: None,
            },
            Err(e) => fail(phase, i, "veto", serde_json::json!({}), e),
        },
        LawAction::Pass => ActionLog {
            phase,
            actor: i,
            action: "pass".into(),
            detail: serde_json::json!({}),
            ok: true,
            err: None,
            cash_after: None,
            goods_after: None,
        },
    }
}

fn ok(
    phase: &'static str,
    i: usize,
    action: &str,
    detail: serde_json::Value,
    f: &alashi_rules::state::Faction,
) -> ActionLog {
    ActionLog {
        phase,
        actor: i,
        action: action.into(),
        detail,
        ok: true,
        err: None,
        cash_after: Some(f.cash),
        goods_after: Some(f.goods),
    }
}

fn fail(
    phase: &'static str,
    i: usize,
    action: &str,
    detail: serde_json::Value,
    e: alashi_rules::error::GameError,
) -> ActionLog {
    ActionLog {
        phase,
        actor: i,
        action: action.into(),
        detail,
        ok: false,
        err: Some(err_str(e)),
        cash_after: None,
        goods_after: None,
    }
}

/// Серия игр: одна строка JSONL на партию.
pub fn run_series(
    n_games: u64,
    master_seed: u64,
    mix: &[&str],
    cfg: &GameConfig,
) -> Result<Vec<String>, String> {
    if mix.len() < MIN_FACTIONS as usize || mix.len() > MAX_FACTIONS as usize {
        return Err(format!(
            "микс фракций должен быть от 2 до {}, дали {}",
            MAX_FACTIONS,
            mix.len()
        ));
    }
    let mut out = Vec::with_capacity(n_games as usize);
    for g in 0..n_games {
        let seed = splitmix64(master_seed ^ splitmix64(g + 1));
        // R9 (REVIEW_EXTERNAL): ротация посадки — порядок хода (round+k)%n
        // спутан со стратегией, если микс зафиксирован во всех партиях;
        // теперь назначение стратегий по слотам крутится каждую партию.
        let rotated: Vec<&str> = (0..mix.len())
            .map(|i| mix[(i + g as usize) % mix.len()])
            .collect();
        let mut strategies: Vec<Box<dyn Strategy>> = rotated
            .iter()
            .enumerate()
            .map(|(i, name)| {
                crate::strategies::by_name(name, splitmix64(seed ^ splitmix64(i as u64 + 7)))
                    .ok_or_else(|| format!("неизвестная стратегия: {}", name))
            })
            .collect::<Result<_, _>>()?;
        let rec = play_game(g, seed, &mut strategies, cfg);
        out.push(serde_json::to_string(&rec).map_err(|e| e.to_string())?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strategies(mix: &[&str], seed: u64) -> Vec<Box<dyn Strategy>> {
        mix.iter()
            .enumerate()
            .map(|(i, n)| crate::strategies::by_name(n, seed + i as u64).unwrap())
            .collect()
    }

    #[test]
    fn game_completes_and_money_conserves() {
        let mut strs = strategies(&["greedy", "random", "tactical"], 42);
        let rec = play_game(1, 42, &mut strs, &GameConfig::default());
        assert_eq!(rec.phases.len(), 6 * 3, "6 раундов × 3 фазы");
        assert_eq!(rec.ranks.len(), 3);
        let pot = rec.bank - rec.rake;
        let paid: u64 = rec.payouts.iter().sum();
        assert_eq!(paid, pot, "выплаты = банк - рейк");
        assert_eq!(rec.rake, rec.bank * DEFAULT_RAKE_BPS as u64 / 10_000);
    }

    #[test]
    fn series_reproducible_and_parseable() {
        let cfg = GameConfig::default();
        let a = run_series(3, 777, &["greedy", "random", "tactical"], &cfg).unwrap();
        let b = run_series(3, 777, &["greedy", "random", "tactical"], &cfg).unwrap();
        assert_eq!(a, b, "одинаковый seed = одинаковые партии");
        for line in &a {
            let v: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(v["ranks"].as_array().unwrap().len() >= 2);
        }
    }

    #[test]
    fn bad_mix_rejected() {
        assert!(run_series(1, 1, &["greedy"], &GameConfig::default()).is_err());
        assert!(run_series(1, 1, &["greedy", "nope"], &GameConfig::default()).is_err());
    }

    #[test]
    fn m5_90s_epoch_money_conserves_with_factory() {
        // SPEC_EPOCH_90S: полная партия в эпохе 90-х, деньги сходятся,
        // завод (5% из рейка) уходит фракции с макс влиянием.
        let mut strs = strategies(&["greedy", "random", "tactical"], 42);
        let cfg = GameConfig {
            epoch: alashi_rules::constants::EPOCH_90S,
            ..GameConfig::default()
        };
        let rec = play_game(1, 42, &mut strs, &cfg);
        let paid: u64 = rec.payouts.iter().sum();
        assert_eq!(paid + rec.rake, rec.bank, "выплаты + рейк = банк");
        assert!(rec.rake <= rec.bank * DEFAULT_RAKE_BPS as u64 / 10_000,
            "завод не увеличивает рейк, а забирает из него");
    }
}
