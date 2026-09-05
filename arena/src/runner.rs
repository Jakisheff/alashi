//! Прогонщик off-chain партий: одна партия = чистые правила + стратегии.
//! Времени реального нет: виртуальные часы двигаются фазами, энтропия
//! детерминирована seed → серия воспроизводима побайтово.

use crate::strategies::{ActionAction, LawAction, MarketAction, Obs, Strategy};
use alashi_rules::anchor_lang::prelude::Pubkey;
use alashi_rules::constants::*;
use alashi_rules::logic::compute_settlement_epoch;
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
    /// M9-метрики лицензиара для A/B и датасета: держатель, его ставка,
    /// рента и суммарная выплата держателю.
    pub license: Option<LicenseRec>,
}

#[derive(Serialize, Clone, Copy)]
pub struct LicenseRec {
    pub holder_idx: usize,
    pub bid: u64,
    pub rent: u64,
    pub holder_payout: u64,
    /// место держателя по рангу wealth (0 = богатейший)
    pub holder_rank: u8,
}

pub struct GameConfig {
    pub entry_fee: u64,
    pub phase_duration: i64,
    pub vote_weight_mode: u8,
    pub epoch: u8,
    /// A/B «рента в ранге» (прод = false): рента лицензии в ранг
    /// держателя вместо отдельной выплаты.
    pub rent_in_rank: bool,
    /// П1 (ТРИЗ, 05.09): режим исполнения рынка. Sequential —
    /// прод = мгновенно по прибытию (гонка латентностей);
    /// Lottery — решения против состояния на открытии, порядок
    /// случайный; Batch — все продажи по одной средневзвешенной цене.
    pub market_exec: MarketExec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketExec {
    Sequential,
    Lottery,
    Batch,
}

impl Default for GameConfig {
    fn default() -> Self {
        GameConfig {
            entry_fee: 10 * PESO,
            phase_duration: 10,
            vote_weight_mode: alashi_rules::constants::VOTE_WEIGHT_LEGACY,
            epoch: alashi_rules::constants::EPOCH_CLASSIC,
            rent_in_rank: false,
            market_exec: MarketExec::Sequential,
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
        if phase == Phase::Market && cfg.market_exec != MarketExec::Sequential {
            // П1 A/B: честная очередь. Решения собираются против
            // состояния НА ОТКРЫТИИ фазы (все видят одинаковый рынок),
            // затем исполняются по режиму.
            let mut decisions = Vec::new();
            for &i in &order {
                let act = with_obs(&sim, &wallets, i, |o| strategies[i].market(o));
                decisions.push((i, act));
            }
            let exec_order: Vec<usize> = match cfg.market_exec {
                MarketExec::Lottery => {
                    // случайный порядок продавцов (seed^round — воспроизводимо)
                    let mut idx: Vec<usize> = (0..decisions.len()).collect();
                    let mut rng = splitmix64(seed ^ ((round as u64) << 16) ^ 0x1234);
                    for k in (1..idx.len()).rev() {
                        rng = splitmix64(rng);
                        let j = (rng % (k as u64 + 1)) as usize;
                        idx.swap(k, j);
                    }
                    idx
                }
                _ => (0..decisions.len()).collect(),
            };
            if cfg.market_exec == MarketExec::Batch {
                actions.extend(batch_market(&mut sim, &decisions));
            } else {
                for k in exec_order {
                    let (i, ref act) = decisions[k];
                    actions.push(apply_market(&mut sim, i, act));
                }
            }
        } else {
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
    let (ranks, payouts, rake, bank, breakdown) = settle(&sim, cfg.entry_fee, cfg.rent_in_rank);
    let rake = rake;
    let license = if sim.game.license_sold {
        sim.factions
            .iter()
            .position(|f| f.wallet == sim.game.license_holder)
            .map(|h| LicenseRec {
                holder_idx: h,
                bid: sim.game.prize_pot,
                rent: breakdown[h].license_rent,
                holder_payout: payouts[h],
                holder_rank: ranks.iter().position(|&x| x == h).map(|r| r as u8).unwrap_or(255),
            })
    } else {
        None
    };

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
        license,
    }
}

/// Слагаемые выплаты для прозрачного сеттл-отчёта (кастдев 02.09).
#[derive(Serialize)]
pub struct PayoutLine {
    pub rank_share: u64,
    pub license_rent: u64,
    pub factory_bonus: u64,
}

/// Settle по правилам для off-chain партии: банк = взносы, резерв 0.
/// Вся математика — в rules::compute_settlement_epoch (общая с ончейн),
/// арена передаёт external_rent=true: рента лицензии здесь экзогенный
/// песо-поток (в песо-мире арены источник есть).
pub fn settle(
    sim: &Simulator,
    entry_fee: u64,
    rent_in_rank: bool,
) -> (Vec<usize>, Vec<u64>, u64, u64, Vec<PayoutLine>) {
    try_settle(sim, entry_fee, rent_in_rank).expect("valid simulation settlement")
}

/// The HTTP arena must report invalid persisted games without stopping its crank.
pub fn try_settle(
    sim: &Simulator,
    entry_fee: u64,
    rent_in_rank: bool,
) -> Result<(Vec<usize>, Vec<u64>, u64, u64, Vec<PayoutLine>), String> {
    let n = sim.factions.len();
    // M9: банк партии включает ставки аукциона лицензии
    let bank = entry_fee.checked_mul(n as u64)
        .and_then(|v| v.checked_add(sim.game.prize_pot))
        .ok_or("settlement bank overflow")?;
    bank.checked_add(sim.game.license_yield).ok_or("settlement payout overflow")?;
    for f in &sim.factions {
        let wealth = f.cash.checked_add(f.hard).ok_or("settlement wealth overflow")?;
        if rent_in_rank {
            wealth.checked_add(sim.game.license_yield).ok_or("settlement rank overflow")?;
        }
    }
    let es = compute_settlement_epoch(&sim.game, &sim.factions, bank, 0, true, rent_in_rank)
        .map_err(|e| e.to_string())?;
    let mut payouts = vec![0u64; n];
    let mut breakdown = Vec::with_capacity(n);
    for line in &es.lines {
        payouts[line.idx] = line.total;
        breakdown.push(PayoutLine {
            rank_share: line.rank_share,
            license_rent: line.license_rent,
            factory_bonus: line.factory_bonus,
        });
    }
    Ok((es.order, payouts, es.rake, bank, breakdown))
}

/// Пустой лог-заготовка (для сервисных операций вроде валютчика).
pub fn empty_log() -> ActionLog {
    ActionLog {
        phase: "",
        actor: 0,
        action: "".into(),
        detail: serde_json::json!({}),
        ok: false,
        err: None,
        cash_after: None,
        goods_after: None,
    }
}

/// П1 (ТРИЗ) Batch: все продажи фазы по одной средневзвешенной цене
/// (суммарный gross по таблице от нуля / суммарные юниты). Налог и
/// кредит считаются каждому на его долю. Покупки исполняются после
/// пакета по итоговому счётчику. Решения уже собраны против
/// состояния на открытии фазы.
fn batch_market(sim: &mut Simulator, decisions: &[(usize, MarketAction)]) -> Vec<ActionLog> {
    use crate::strategies::sale_gross;
    let mut logs = Vec::new();
    let shift = sim.game.active_price_shift;
    let boom = sim.game.active_boom;
    let tax_bps = sim.game.active_tax_bps;
    let mut sells: Vec<(usize, u16, bool)> = Vec::new(); // (i, units, credit)
    let mut buys: Vec<(usize, u16)> = Vec::new();
    for &(i, ref act) in decisions {
        match act {
            MarketAction::Sell(u) => sells.push((i, *u, false)),
            MarketAction::SellCredit(u) => sells.push((i, *u, true)),
            MarketAction::Buy(u) => buys.push((i, *u)),
            MarketAction::Pass => {}
        }
    }
    let total_units: u64 = sells.iter().map(|(_, u, _)| *u as u64).sum();
    if total_units > 0 {
        let gross_total = sale_gross(0, total_units as u16, shift, boom);
        let avg = gross_total / total_units;
        let mut distributed = 0u64;
        let n = sells.len();
        for (k, &(i, units, credit)) in sells.iter().enumerate() {
            // валидация: товара хватает (иначе отказ как в правилах)
            if sim.factions[i].goods < units {
                logs.push(fail("market", i, "sell", serde_json::json!({"units": units, "batch": true}), alashi_rules::error::GameError::NotEnoughGoods));
                continue;
            }
            let mut gross = avg * units as u64;
            if k + 1 == n {
                gross = gross_total - distributed; // остаток последнему — сумма сходится
            } else {
                distributed += gross;
            }
            let tax = gross * tax_bps as u64 / 10_000;
            let mut revenue = gross - tax;
            if credit {
                revenue = revenue * alashi_rules::constants::CREDIT_NUM
                    / alashi_rules::constants::CREDIT_DEN;
                sim.factions[i].promissory += revenue;
            } else {
                sim.factions[i].cash += revenue;
            }
            sim.factions[i].goods -= units;
            let f = &sim.factions[i];
            logs.push(ActionLog {
                phase: "market",
                actor: i,
                action: if credit { "sell_credit" } else { "sell" }.into(),
                detail: serde_json::json!({
                    "units": units, "revenue": revenue, "batch": true,
                    "avg_price": avg, "tax_bps": tax_bps,
                }),
                ok: true,
                err: None,
                cash_after: Some(f.cash),
                goods_after: Some(f.goods),
            });
        }
        sim.game.sold_this_round += total_units as u16;
    }
    for (i, u) in buys {
        logs.push(apply_market(sim, i, &MarketAction::Buy(u)));
    }
    logs
}

pub fn apply_market(sim: &mut Simulator, i: usize, act: &MarketAction) -> ActionLog {    let phase = "market";
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
        ActionAction::Roof { to, tariff } => match sim.roof(i, *to, *tariff) {
            Ok(roof_to) => {
                let f = &sim.factions[i];
                ActionLog {
                    phase,
                    actor: i,
                    action: "roof".into(),
                    detail: serde_json::json!({"to": roof_to, "tariff": if *tariff == ROOF_BLACK { "black" } else { "red" }}),
                    ok: true,
                    err: None,
                    cash_after: Some(f.cash),
                    goods_after: Some(f.goods),
                }
            }
            Err(e) => fail(phase, i, "roof", serde_json::json!({"to": to}), e),
        },
        ActionAction::Bid(amount) => match sim.bid_license(i, *amount) {
            Ok(total) => ok(
                phase,
                i,
                "bid_license",
                serde_json::json!({"amount": amount, "total_bid": total}),
                &sim.factions[i],
            ),
            Err(e) => fail(phase, i, "bid_license", serde_json::json!({"amount": amount}), e),
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
        // Рента лицензии — экзогенный поток (не из банка): инвариант
        // банка считается без неё: доли + фактический рейк = банк.
        let mut strs = strategies(&["greedy", "random", "tactical"], 42);
        let cfg = GameConfig {
            epoch: alashi_rules::constants::EPOCH_90S,
            ..GameConfig::default()
        };
        let rec = play_game(1, 42, &mut strs, &cfg);
        let rent: u64 = rec.license.map(|l| l.rent).unwrap_or(0);
        let paid: u64 = rec.payouts.iter().sum();
        assert_eq!(
            paid - rent + rec.rake,
            rec.bank,
            "доли рангов + фактический рейк = банк (рента экзогенна)"
        );
        assert!(rec.rake <= rec.bank * DEFAULT_RAKE_BPS as u64 / 10_000,
            "завод не увеличивает рейк, а забирает из него");
        // метрика лицензиара заполнена, если аукцион состоялся
        if let Some(l) = rec.license {
            assert!(l.rent == 0 || l.holder_payout >= l.rent);
        }
    }
}
