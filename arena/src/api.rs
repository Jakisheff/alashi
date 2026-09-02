//! HTTP API арены: партии для внешних агентов поверх чистых правил.
//! Каждое действие агента = один вызов /act с токеном. Кранк фаз —
//! фоновый поток по таймеру ИЛИ permissionless POST /advance (как ончейн).

use crate::http::{read_request, respond, Request};
use crate::runner::{self, ActionLog};
use crate::strategies::{ActionAction, LawAction, MarketAction};
use crate::strategies::{eff_price, ALL};
use alashi_rules::anchor_lang::prelude::Pubkey;
use alashi_rules::constants::*;
use alashi_rules::error::GameError;
use alashi_rules::sim::Simulator;
use alashi_rules::state::{Phase, VoteChoice};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Дефолт грейс-окна после фазы (кастдев 02.09, №1 голосования агентов).
pub const DEFAULT_GRACE_S: i64 = 3;
/// Верхняя граница grace_s при создании партии.
pub const MAX_GRACE_S: i64 = 30;

pub struct AgentRec {
    pub name: String,
    pub agent_id: String,
    pub token: String,
    pub model: String,
    pub faction_idx: usize,
}

pub struct GameEntry {
    pub sim: Simulator,
    pub entry_fee: u64,
    pub wallets: Vec<Pubkey>,
    pub agents: Vec<AgentRec>,
    pub created: i64,
    /// Грейс-окно (сек) после phase_ends_at: кранк и /advance ждут
    /// ends_at + grace_s, опоздавшие на <= grace_s действия прошлой
    /// фазы успевают легально приземлиться (кастдев 02.09, запрос №1).
    /// 0 = старое поведение (для A/B).
    pub grace_s: i64,
    /// Полный протокол партии для /export: каждый ход с фазой и раундом.
    pub action_log: Vec<serde_json::Value>,
    /// Итоги закрытых фаз (закон: карта, да/нет, прошёл/вето).
    pub phase_log: Vec<serde_json::Value>,
}

pub struct AppState {
    pub games: Mutex<HashMap<u64, GameEntry>>,
    pub next_id: AtomicU64,
    pub completed: Mutex<Vec<String>>,
    pub master_seed: AtomicU64,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn splitmix64(x: u64) -> u64 {
    let mut z = x.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

fn random_hex() -> String {
    use std::io::Read;
    let mut b = [0u8; 16];
    let ok = std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut b))
        .is_ok();
    if ok {
        return b.iter().map(|x| format!("{:02x}", x)).collect();
    }
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!("{:032x}", splitmix64(t ^ 0xDEAD))
}

pub fn agent_id_of(model: &str, prompt: &str) -> String {
    let mut h = Sha256::new();
    h.update(model.as_bytes());
    h.update(b"|");
    h.update(prompt.as_bytes());
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn new_state() -> Arc<AppState> {
    Arc::new(AppState {
        games: Mutex::new(HashMap::new()),
        next_id: AtomicU64::new(1),
        completed: Mutex::new(Vec::new()),
        master_seed: AtomicU64::new(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(1),
        ),
    })
}

// ---------- служебное ----------

fn game_seed(state: &AppState, game_id: u64) -> u64 {
    splitmix64(state.master_seed.load(Ordering::Relaxed) ^ splitmix64(game_id))
}

/// Фиксирует итог закрытой фазы в протокол: для law — карту, вес да/нет,
/// итог и вето (веса обновляются самим advance при подсчёте).
fn record_phase_close(entry: &mut GameEntry, closing: (Phase, u8, u8)) {
    let (phase, round, card) = closing;
    let g = &entry.sim.game;
    let rec = match phase {
        Phase::Law => serde_json::json!({
            "round": round,
            "phase": "law",
            "card": card,
            "card_name": law_name(card),
            "yes": g.yes_influence,
            "no": g.no_influence,
            "passed": g.last_law_passed,
            "veto_pending": g.veto_pending,
            "laws_passed_total": g.laws_passed,
        }),
        _ => serde_json::json!({
            "round": round,
            "phase": phase_name(phase),
        }),
    };
    entry.phase_log.push(rec);
}

fn settle_and_record(state: &AppState, game_id: u64) {
    let mut rec = None;
    {
        let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = games.get_mut(&game_id) {
            let (ranks, payouts, rake, bank, breakdown) = runner::settle(&entry.sim, entry.entry_fee, false);
            let agents: Vec<serde_json::Value> = entry
                .agents
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "name": a.name,
                        "agent_id": a.agent_id,
                        "model": a.model,
                    })
                })
                .collect();
            let v = serde_json::json!({
                "game_id": game_id,
                "entry_fee": entry.entry_fee,
                "vote_weight_mode": entry.sim.game.vote_weight_mode,
                "epoch": entry.sim.game.epoch,
                "n_factions": entry.sim.factions.len(),
                "finished_at": now(),
                "agents": agents,
                "ranks": ranks,
                "payouts": payouts,
                "rake": rake,
                "bank": bank,
                // кастдев 02.09: final_cash обязан включать твёрдую
                // валюту, иначе агенты неверно читают свой ранг
                "final_cash": entry.sim.factions.iter().map(|f| f.cash + f.hard).collect::<Vec<_>>(),
                "final_hard": entry.sim.factions.iter().map(|f| f.hard).collect::<Vec<_>>(),
                "final_promissory": entry.sim.factions.iter().map(|f| f.promissory).collect::<Vec<_>>(),
                "final_goods": entry.sim.factions.iter().map(|f| f.goods).collect::<Vec<_>>(),
                "final_influence": entry.sim.factions.iter().map(|f| f.influence).collect::<Vec<_>>(),
                // кастдев 02.09: прозрачный сеттл — откуда каждый песо
                // выплаты: доля ранга, рента лицензии, завод
                "payout_breakdown": breakdown,
                "phases": entry.phase_log,
                "actions": entry.action_log,
            });
            rec = Some(v.to_string());
        }
        games.remove(&game_id);
    }
    if let Some(r) = rec {
        state.completed.lock().unwrap_or_else(|e| e.into_inner()).push(r);
    }
}

/// Один тик кранка: двигает все партии, чьё время фазы вышло.
pub fn crank_once(state: &AppState) {
    let t = now();
    let mut to_settle: Vec<u64> = Vec::new();
    let mut to_expire: Vec<u64> = Vec::new();
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    for (&gid, entry) in games.iter_mut() {
        let seed = splitmix64(game_seed(state, gid) ^ (entry.sim.game.round as u64));
        match entry.sim.game.phase {
            Phase::Lobby => {
                let full = entry.sim.game.faction_count >= MAX_FACTIONS;
                let late = t >= entry.sim.game.phase_ends_at;
                if full || late {
                    if entry.sim.game.faction_count >= MIN_FACTIONS {
                        let closing =
                            (entry.sim.game.phase, entry.sim.game.round, entry.sim.game.law_card);
                        if entry.sim.advance(t, seed).is_ok() {
                            entry.sim.game.phase_ends_at = t + entry.sim.game.phase_duration;
                            record_phase_close(entry, closing);
                        }
                    } else if late {
                        to_expire.push(gid);
                    }
                }
            }
            Phase::Finished => to_settle.push(gid),
            Phase::Aborted => to_expire.push(gid),
            _ => {
                // грейс-окно: опоздавшие действия прошлой фазы ещё приняты,
                // кранк ждёт ends_at + grace_s (лобби выше — без грейса)
                if t >= entry.sim.game.phase_ends_at + entry.grace_s {
                    let closing =
                        (entry.sim.game.phase, entry.sim.game.round, entry.sim.game.law_card);
                    if entry.sim.advance(t, seed).is_ok() {
                        entry.sim.game.phase_ends_at = t + entry.sim.game.phase_duration;
                        record_phase_close(entry, closing);
                    }
                    if entry.sim.game.phase == Phase::Finished {
                        to_settle.push(gid);
                    }
                }
            }
        }
    }
    drop(games);
    for gid in to_settle {
        settle_and_record(state, gid);
    }
    if !to_expire.is_empty() {
        let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        for gid in to_expire {
            games.remove(&gid);
        }
    }
}

// ---------- JSON состояния ----------

fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Lobby => "lobby",
        Phase::Market => "market",
        Phase::Action => "action",
        Phase::Law => "law",
        Phase::Finished => "finished",
        Phase::Aborted => "aborted",
    }
}

fn state_json(game_id: u64, entry: &GameEntry) -> serde_json::Value {
    let g = &entry.sim.game;
    let stamp = g.stamp();
    let price_now = eff_price(g.sold_this_round, g.active_price_shift, g.active_boom);
    serde_json::json!({
        "game_id": game_id,
        "phase": phase_name(g.phase),
        "round": g.round,
        "entry_fee": entry.entry_fee,
        "phase_ends_at": g.phase_ends_at,
        // грейс-окно: реальный дедлайн приёма действий = grace_until
        "grace_s": entry.grace_s,
        "grace_until": g.phase_ends_at + entry.grace_s,
        "now": now(),
        "law_card": if g.phase == Phase::Law { Some(g.law_card) } else { None },
        "law_card_name": if g.phase == Phase::Law { Some(law_name(g.law_card)) } else { None },
        "sold_counter": g.sold_this_round,
        "price_now": price_now,
        "price_table": PRICE_TABLE,
        "tax_bps": g.active_tax_bps,
        "price_shift": g.active_price_shift,
        "boom": g.active_boom,
        "laws_passed": g.laws_passed,
        "last_law_passed": g.last_law_passed,
        "vote_weight_mode": g.vote_weight_mode,
        "epoch": if g.epoch == alashi_rules::constants::EPOCH_90S { "90s" } else { "classic" },
        "amnesty_used": g.amnesty_used,
        // M8: решение президента скрыто до закрытия фазы, виден факт
        "customs_decided": g.customs_decided,
        // M9: аукцион публичен, доходность скрыта (инсайд через /act)
        "license_auction": if g.epoch == alashi_rules::constants::EPOCH_90S {
            let mut v = serde_json::Map::new();
            v.insert("round".into(), json_num(alashi_rules::constants::AUCTION_ROUND));
            v.insert("sold".into(), serde_json::json!(g.license_sold));
            if g.license_sold {
                let holder_idx = entry.sim.factions.iter().position(|x| x.wallet == g.license_holder);
                v.insert("holder".into(), serde_json::json!(holder_idx));
                // кастдев v2 (Agent3, правка №3): рента видна после
                // аукциона — доход держателя уже факт его актива
                v.insert("yield".into(), json_num(g.license_yield));
            }
            v.insert("pot".into(), json_num(g.prize_pot));
            serde_json::Value::Object(v)
        } else { serde_json::Value::Null },
        // M11: публичные бартерные оферы (адресные to скрыты);
        // from/to показаны индексами фракций (кошельки остаются в Game)
        "barter_offers": entry.sim.game.barter_offers.iter().map(|o| {
            let from_idx = entry.sim.factions.iter().position(|f| f.wallet == o.from);
            serde_json::json!({
                "offer": o.id, "from": from_idx, "goods": o.goods, "price": o.price,
            })
        }).collect::<Vec<_>>(),
        "veto_pending": g.veto_pending,
        "president_idx": entry.sim.factions.iter().position(|f| f.wallet == g.president),
        "yes_influence": g.yes_influence,
        "no_influence": g.no_influence,
        // кастдев v2 (голосование фич): живой лог ходов — чужие
        // действия текущей фазы, слепота к ним стоила агентам ~50M
        "recent_actions": entry.action_log.iter().rev().take(12).rev()
            .map(|a| serde_json::json!({
                "round": a.get("round"),
                "phase": a.get("phase"),
                "actor": a.get("actor"),
                "action": a.get("action"),
                "ok": a.get("ok"),
                "ts": a.get("ts"),
            })).collect::<Vec<_>>(),
        "factions": entry.sim.factions.iter().enumerate().map(|(i, f)| {
            serde_json::json!({
                "idx": i,
                "name": f.name,
                "agent_id": entry.agents.iter().find(|a| a.faction_idx == i).map(|a| a.agent_id.clone()),
                "cash": f.cash,
                "goods": f.goods,
                "influence": f.influence,
                "acted": f.alive && f.acted_stamp == stamp,
                "voted": f.alive && f.voted_stamp == stamp,
                "is_president": f.is_president,
                "alive": f.alive,
                "grey_goods": f.grey_goods,
                "promissory": f.promissory,
                "hard": f.hard,
                "roof_to": if f.roof_armed {
                    serde_json::json!(entry.sim.factions.iter().position(|x| x.wallet == f.roof_to))
                } else { serde_json::Value::Null },
            })
        }).collect::<Vec<_>>(),
    })
}

pub fn law_name(card: u8) -> &'static str {
    match card {
        LAW_STATUS_QUO => "status_quo",
        LAW_TAX_10 => "tax_10",
        LAW_TAX_20 => "tax_20",
        LAW_SUBSIDY_PRODUCE => "subsidy_produce",
        LAW_SUBSIDY_POOR => "subsidy_poor",
        LAW_SUBSIDY_RICH => "subsidy_rich",
        LAW_EMBARGO => "embargo",
        LAW_BOOM => "boom",
        8 => "vzaimozachet",
        _ => "no_law",
    }
}

// ---------- обработчики ----------

fn err_json(code: &str, msg: &str) -> serde_json::Value {
    serde_json::json!({"ok": false, "error": code, "message": msg})
}

/// Заготовка неудачного сервисного лога (для M7-M11 действий вне runner).
fn fail_like(e: alashi_rules::error::GameError, idx: usize, action: &str) -> ActionLog {
    let mut l = runner::empty_log();
    l.phase = "money".into();
    l.actor = idx;
    l.action = action.into();
    l.err = Some(format!("{:?}", e));
    l
}

fn json_num(v: impl Into<u64>) -> serde_json::Value {
    serde_json::json!(v.into())
}

fn h_new_game(state: &AppState, body: &serde_json::Value) -> serde_json::Value {
    let entry_fee = body
        .get("entry_fee")
        .and_then(|v| v.as_u64())
        .unwrap_or(10 * PESO);
    let phase_duration = body
        .get("phase_duration")
        .and_then(|v| v.as_i64())
        .unwrap_or(30);
    let grace_s = body
        .get("grace_s")
        .and_then(|v| v.as_i64())
        .unwrap_or(DEFAULT_GRACE_S);
    let vote_weight_mode = body
        .get("vote_weight_mode")
        .and_then(|v| v.as_u64())
        .unwrap_or(alashi_rules::constants::VOTE_WEIGHT_LEGACY as u64)
        as u8;
    let epoch = match body.get("epoch").and_then(|v| v.as_str()) {
        Some("90s") => alashi_rules::constants::EPOCH_90S,
        Some("classic") | None => alashi_rules::constants::EPOCH_CLASSIC,
        _ => return err_json("bad_params", "epoch: classic | 90s"),
    };
    if entry_fee == 0 || phase_duration < 1 {
        return err_json("bad_params", "entry_fee > 0, phase_duration >= 1");
    }
    if !(0..=MAX_GRACE_S).contains(&grace_s) {
        return err_json("bad_params", "grace_s: 0..=30");
    }
    if vote_weight_mode > alashi_rules::constants::VOTE_WEIGHT_CONTRIB {
        return err_json("bad_params", "vote_weight_mode: 0 legacy, 1 contribution");
    }
    let game_id = state.next_id.fetch_add(1, Ordering::SeqCst);
    let entropy = alashi_rules::constants::ENTROPY_SLOTHASH;
    let mut sim = Simulator::new(game_id, entry_fee, phase_duration, entropy);
    sim.game.vote_weight_mode = vote_weight_mode;
    sim.game.epoch = epoch;
    sim.game.phase_ends_at = now() + phase_duration * LOBBY_MULT;
    let entry = GameEntry {
        sim,
        entry_fee,
        wallets: vec![],
        agents: vec![],
        created: now(),
        grace_s,
        action_log: vec![],
        phase_log: vec![],
    };
    let v = state_json(game_id, &entry);
    state.games.lock().unwrap_or_else(|e| e.into_inner()).insert(game_id, entry);
    serde_json::json!({"ok": true, "game_id": game_id, "state": v})
}

fn h_join(state: &AppState, game_id: u64, body: &serde_json::Value) -> serde_json::Value {
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("Agent")
        .to_string();
    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let prompt = body
        .get("prompt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if name.len() > MAX_NAME {
        return err_json("name_too_long", "имя до 16 байт");
    }
    let agent_id = agent_id_of(&model, &prompt);
    let token = random_hex();
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = games.get_mut(&game_id) else {
        return err_json("unknown_game", "партия не найдена или закрыта");
    };
    if entry.agents.len() >= MAX_FACTIONS as usize {
        return err_json("game_full", "мест нет");
    }
    // Кошелёк детерминирован из agent_id + game_id.
    let mut h = Sha256::new();
    h.update(agent_id.as_bytes());
    h.update(&game_id.to_le_bytes());
    let d: [u8; 32] = h.finalize().into();
    let wallet = Pubkey::new_from_array(d);
    if let Err(e) = entry.sim.join(wallet, &name) {
        return err_json("join_failed", &format!("{:?}", e));
    }
    let faction_idx = entry.sim.factions.len() - 1;
    entry.wallets.push(wallet);
    entry.agents.push(AgentRec {
        name,
        agent_id: agent_id.clone(),
        token: token.clone(),
        model,
        faction_idx,
    });
    let v = state_json(game_id, entry);
    serde_json::json!({"ok": true, "agent_id": agent_id, "token": token, "faction_idx": faction_idx, "state": v})
}

fn h_state(state: &AppState, game_id: u64) -> serde_json::Value {
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    match games.get(&game_id) {
        Some(e) => serde_json::json!({"ok": true, "state": state_json(game_id, e)}),
        None => {
            let completed = state.completed.lock().unwrap_or_else(|e| e.into_inner());
            let rec = completed
                .iter()
                .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                .find(|r| r["game_id"].as_u64() == Some(game_id));
            match rec {
                Some(r) => serde_json::json!({"ok": true, "finished": true, "result": r}),
                None => err_json("unknown_game", "партия не найдена"),
            }
        }
    }
}

/// Кастдев 02.09: long-pool ожидание смены фазы. Спим до тех пор, пока
/// (round, phase) партии не станет отличаться от переданного, партия
/// не завершится или не выйдет таймаут. Ответ = как /state плюс флаги
/// changed/timeout. Параметры: r (раунд, число), p (фаза, строка),
/// t (таймаут в секундах, дефолт 30, максимум 60).
fn h_wait(state: &AppState, game_id: u64, raw_path: &str) -> serde_json::Value {
    let q = raw_path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let getq = |k: &str| {
        q.split('&').find_map(|kv| {
            let (kk, vv) = kv.split_once('=')?;
            (kk == k).then(|| vv.to_string())
        })
    };
    let after_round: Option<u8> = getq("r").and_then(|v| v.parse().ok());
    let after_phase = getq("p");
    let timeout_s: u64 = getq("t").and_then(|v| v.parse().ok()).unwrap_or(30).min(60);
    let deadline = now() + timeout_s as i64;

    loop {
        // короткий лок: читаем и отпускаем, кранк не блокируется
        let snapshot = {
            let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
            games.get(&game_id).map(|e| {
                (e.sim.game.round, phase_name(e.sim.game.phase), state_json(game_id, e))
            })
        };
        match snapshot {
            Some((r, p, st)) => {
                let changed = after_round.map(|ar| ar != r).unwrap_or(false)
                    || after_phase.as_deref().map(|ap| ap != p).unwrap_or(false);
                if changed {
                    return serde_json::json!({"ok": true, "changed": true, "state": st});
                }
            }
            None => {
                // партии нет среди живых: либо finished, либо unknown
                return h_state(state, game_id);
            }
        }
        if now() >= deadline {
            let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(e) = games.get(&game_id) {
                return serde_json::json!({
                    "ok": true, "changed": false, "timeout": true,
                    "state": state_json(game_id, e),
                });
            }
            return h_state(state, game_id);
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}


fn log_to_json(l: &ActionLog) -> serde_json::Value {
    serde_json::json!({
        "actor": l.actor,
        "action": l.action,
        "detail": l.detail,
        "ok": l.ok,
        "err": l.err,
        "cash_after": l.cash_after,
        "goods_after": l.goods_after,
    })
}

fn h_act(state: &AppState, game_id: u64, body: &serde_json::Value) -> serde_json::Value {
    let token = body.get("token").and_then(|v| v.as_str()).unwrap_or("");
    let action = body.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let p = body.get("params").cloned().unwrap_or(serde_json::json!({}));
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = games.get_mut(&game_id) else {
        return err_json("unknown_game", "партия не найдена или закрыта");
    };
    let Some(agent) = entry.agents.iter().find(|a| a.token == token) else {
        return err_json("bad_token", "токен не найден");
    };
    let idx = agent.faction_idx;
    let log: ActionLog = match action {
        "sell" => {
            let units = p.get("units").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            runner::apply_market(entry.sim_mut(), idx, &MarketAction::Sell(units))
        }
        "sell_credit" => {
            let units = p.get("units").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            runner::apply_market(entry.sim_mut(), idx, &MarketAction::SellCredit(units))
        }
        "buy" => {
            let units = p.get("units").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            runner::apply_market(entry.sim_mut(), idx, &MarketAction::Buy(units))
        }
        "produce" => runner::apply_action(entry.sim_mut(), idx, &ActionAction::Produce),
        "shuttle" => runner::apply_action(entry.sim_mut(), idx, &ActionAction::Shuttle),
        "roof" => {
            let to = p.get("to").and_then(|v| v.as_u64()).unwrap_or(usize::MAX as u64) as usize;
            let tariff = match p.get("tariff").and_then(|v| v.as_str()) {
                Some("black") => alashi_rules::constants::ROOF_BLACK,
                Some("red") => alashi_rules::constants::ROOF_RED,
                _ => 0,
            };
            runner::apply_action(entry.sim_mut(), idx, &ActionAction::Roof { to, tariff })
        }
        "customs" => {
            // M8: президент выбирает границу вслепую (до чужих ходов)
            let tight = p.get("tight").and_then(|v| v.as_bool()).unwrap_or(true);
            match entry.sim.set_customs(idx, tight) {
                Ok(_) => {
                    let mut l = runner::empty_log();
                    l.phase = "action".into();
                    l.actor = idx;
                    l.action = "customs".into();
                    l.ok = true;
                    l.detail = serde_json::json!({"decided": true});
                    l
                }
                Err(e) => {
                    let mut l = runner::empty_log();
                    l.phase = "action".into();
                    l.actor = idx;
                    l.action = "customs".into();
                    l.err = Some(format!("{:?}", e));
                    l
                }
            }
        }
        "bid_license" => {
            let amount = p.get("amount").and_then(|v| v.as_u64()).unwrap_or(0);
            match entry.sim.bid_license(idx, amount) {
                Ok(total) => {
                    let f = &entry.sim.factions[idx];
                    ActionLog {
                        phase: "action",
                        actor: idx,
                        action: "bid_license".into(),
                        detail: serde_json::json!({"amount": amount, "total_bid": total}),
                        ok: true,
                        err: None,
                        cash_after: Some(f.cash),
                        goods_after: Some(f.goods),
                    }
                }
                Err(e) => fail_like(e, idx, "bid_license"),
            }
        }
        "inspect_license" => {
            // M9: ответ содержит закрытый доход — только для инсайдера
            match entry.sim.inspect_license(idx) {
                Ok(y) => {
                    let f = &entry.sim.factions[idx];
                    ActionLog {
                        phase: "action",
                        actor: idx,
                        action: "inspect_license".into(),
                        detail: serde_json::json!({"license_yield": y}),
                        ok: true,
                        err: None,
                        cash_after: Some(f.cash),
                        goods_after: Some(f.goods),
                    }
                }
                Err(e) => fail_like(e, idx, "inspect_license"),
            }
        }
        "offer_vote" => {
            // M10 (фикс): офер продажи голоса, деньги не списываются
            let buyer = p.get("to").and_then(|v| v.as_u64()).unwrap_or(usize::MAX as u64) as usize;
            let price = p.get("price").and_then(|v| v.as_u64()).unwrap_or(0);
            match entry.sim.offer_vote(idx, buyer, price) {
                Ok(_) => {
                    let mut l = runner::empty_log();
                    l.phase = "law".into();
                    l.actor = idx;
                    l.action = "offer_vote".into();
                    l.ok = true;
                    l.detail = serde_json::json!({"to": buyer, "price": price, "pending_accept": true});
                    l
                }
                Err(e) => fail_like(e, idx, "offer_vote"),
            }
        }
        "accept_vote_offer" => {
            match entry.sim.accept_vote_offer(idx) {
                Ok(price) => {
                    let f = &entry.sim.factions[idx];
                    ActionLog {
                        phase: "law",
                        actor: idx,
                        action: "accept_vote_offer".into(),
                        detail: serde_json::json!({"paid": price}),
                        ok: true,
                        err: None,
                        cash_after: Some(f.cash),
                        goods_after: Some(f.goods),
                    }
                }
                Err(e) => fail_like(e, idx, "accept_vote_offer"),
            }
        }
        "barter_propose" => {
            let goods = p.get("goods").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            let price = p.get("price").and_then(|v| v.as_u64()).unwrap_or(0);
            let to = p.get("to").and_then(|v| v.as_u64()).map(|t| t as usize);
            match entry.sim.barter_propose(idx, to, goods, price) {
                Ok(id) => ActionLog {
                    phase: "market",
                    actor: idx,
                    action: "barter_propose".into(),
                    detail: serde_json::json!({"offer": id, "goods": goods, "price": price}),
                    ok: true,
                    err: None,
                    cash_after: Some(entry.sim.factions[idx].cash),
                    goods_after: Some(entry.sim.factions[idx].goods),
                },
                Err(e) => fail_like(e, idx, "barter_propose"),
            }
        }
        "barter_accept" => {
            let offer = p.get("offer").and_then(|v| v.as_u64()).unwrap_or(0);
            match entry.sim.barter_accept(idx, offer) {
                Ok(_) => ActionLog {
                    phase: "market",
                    actor: idx,
                    action: "barter_accept".into(),
                    detail: serde_json::json!({"offer": offer}),
                    ok: true,
                    err: None,
                    cash_after: Some(entry.sim.factions[idx].cash),
                    goods_after: Some(entry.sim.factions[idx].goods),
                },
                Err(e) => fail_like(e, idx, "barter_accept"),
            }
        }
        "buy_hard" | "sell_hard" => {
            // SPEC_EPOCH_90S M6: валютчик, сервисная операция без сжигания хода
            let to_hard = action == "buy_hard";
            let sim = entry.sim_mut();
            match sim.exchange(idx, to_hard) {
                Ok(amount) => {
                    let f = &sim.factions[idx];
                    ActionLog {
                        phase: "money",
                        actor: idx,
                        action: action.into(),
                        detail: serde_json::json!({
                            "amount": amount,
                            "hard_after": f.hard,
                            "cash_after": f.cash,
                        }),
                        ok: true,
                        err: None,
                        cash_after: Some(f.cash),
                        goods_after: Some(f.goods),
                    }
                }
                Err(e) => {
                    let mut l = runner::empty_log();
                    l.phase = "money".into();
                    l.actor = idx;
                    l.action = action.into();
                    l.err = Some(format!("{:?}", e));
                    l
                }
            }
        }
        "donkey" => runner::apply_action(entry.sim_mut(), idx, &ActionAction::Donkey),
        "bribe" => {
            let to = p.get("to").and_then(|v| v.as_u64()).unwrap_or(usize::MAX as u64) as usize;
            let amount = p.get("amount").and_then(|v| v.as_u64()).unwrap_or(0);
            runner::apply_action(entry.sim_mut(), idx, &ActionAction::Bribe { to, amount })
        }
        "vote" => {
            let choice = match p.get("choice").and_then(|v| v.as_str()).unwrap_or("") {
                "yes" => VoteChoice::Yes,
                "no" => VoteChoice::No,
                "abstain" => VoteChoice::Abstain,
                _ => return err_json("bad_choice", "choice: yes|no|abstain"),
            };
            let w = entry.wallets[idx];
            runner::apply_law(entry.sim_mut(), idx, &LawAction::Vote(choice), &w)
        }
        "veto" => {
            let w = entry.wallets[idx];
            runner::apply_law(entry.sim_mut(), idx, &LawAction::Veto, &w)
        }
        _ => return err_json("bad_action", "sell|sell_credit|buy|buy_hard|sell_hard|produce|shuttle|roof|customs|bid_license|inspect_license|sell_vote|barter_propose|barter_accept|offer_vote|accept_vote_offer|donkey|bribe|vote|veto"),
    };
    let ok = log.ok;
    let err = log.err.clone();
    // протокол хода: фаза, раунд, актёр, действие, исход (для /export)
    entry.action_log.push(serde_json::json!({
        "round": entry.sim.game.round,
        "phase": phase_name(entry.sim.game.phase),
        "actor": idx,
        "action": action,
        "params": p,
        "ok": ok,
        "err": err,
        "cash_after": log.cash_after,
        "goods_after": log.goods_after,
        "ts": now(),
    }));
    let v = state_json(game_id, entry);
    serde_json::json!({"ok": ok, "error": err, "action_log": log_to_json(&log), "state": v})
}

fn h_advance(state: &AppState, game_id: u64) -> serde_json::Value {
    let t = now();
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = games.get_mut(&game_id) else {
        return err_json("unknown_game", "партия не найдена или закрыта");
    };
    let seed = splitmix64(game_seed(state, game_id) ^ (entry.sim.game.round as u64));
    // грейс-окно (только игровые фазы): ранний permissionless-кранк
    // отменил бы окно для опоздавших действий — отказ до ends_at+grace
    if matches!(
        entry.sim.game.phase,
        Phase::Market | Phase::Action | Phase::Law
    ) && t < entry.sim.game.phase_ends_at + entry.grace_s
    {
        let until = entry.sim.game.phase_ends_at + entry.grace_s;
        return serde_json::json!({
            "ok": false,
            "error": "GraceWindow",
            "grace_until": until,
            "state": state_json(game_id, entry),
        });
    }
    let closing = (entry.sim.game.phase, entry.sim.game.round, entry.sim.game.law_card);
    match entry.sim.advance(t, seed) {
        Ok(_) => {
            entry.sim.game.phase_ends_at = t + entry.sim.game.phase_duration;
            record_phase_close(entry, closing);
            let finished = entry.sim.game.phase == Phase::Finished;
            let v = state_json(game_id, entry);
            drop(games);
            if finished {
                settle_and_record(state, game_id);
            }
            serde_json::json!({"ok": true, "finished": finished, "state": v})
        }
        Err(e) => serde_json::json!({"ok": false, "error": format!("{:?}", e), "state": state_json(game_id, entry)}),
    }
}

fn h_games(state: &AppState) -> serde_json::Value {
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let list: Vec<serde_json::Value> = games
        .iter()
        .map(|(&gid, e)| {
            serde_json::json!({
                "game_id": gid,
                "phase": phase_name(e.sim.game.phase),
                "round": e.sim.game.round,
                "factions": e.sim.game.faction_count,
                "names": e.sim.factions.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
                "ends_in": e.sim.game.phase_ends_at - now(),
            })
        })
        .collect();
    serde_json::json!({"ok": true, "games": list})
}

fn h_leaderboard(state: &AppState) -> serde_json::Value {
    let completed = state.completed.lock().unwrap_or_else(|e| e.into_inner());
    let mut rows: HashMap<String, serde_json::Value> = HashMap::new();
    for line in completed.iter() {
        let Ok(r) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let ranks = r["ranks"].as_array().cloned().unwrap_or_default();
        let payouts = r["payouts"].as_array().cloned().unwrap_or_default();
        let agents = r["agents"].as_array().cloned().unwrap_or_default();
        for (place, fi) in ranks.iter().enumerate() {
            let fi = fi.as_u64().unwrap_or(0) as usize;
            let Some(a) = agents.get(fi) else { continue };
            let key = a["agent_id"].as_str().unwrap_or("?").to_string();
            let e = rows
                .entry(key.clone())
                .or_insert_with(|| serde_json::json!({"agent_id": key, "name": a["name"], "model": a["model"], "games": 0, "wins": 0, "rank_sum": 0, "payout": 0}));
            e["games"] = json_add(&e["games"], 1);
            e["rank_sum"] = json_add(&e["rank_sum"], place as u64);
            e["payout"] = json_add(&e["payout"], payouts.get(fi).and_then(|x| x.as_u64()).unwrap_or(0));
            if place == 0 {
                e["wins"] = json_add(&e["wins"], 1);
            }
        }
    }
    let mut out: Vec<serde_json::Value> = rows.into_values().collect();
    out.sort_by_key(|e| {
        std::cmp::Reverse((e["payout"].as_f64().unwrap_or(0.0) * 1e6) as u64)
    });
    for e in out.iter_mut() {
        let g = e["games"].as_u64().unwrap_or(1).max(1);
        e["avg_rank"] = serde_json::json!(e["rank_sum"].as_f64().unwrap_or(0.0) / g as f64);
    }
    serde_json::json!({"ok": true, "leaderboard": out})
}

fn json_add(v: &serde_json::Value, add: u64) -> serde_json::Value {
    serde_json::json!(v.as_u64().unwrap_or(0) + add)
}

fn h_export(state: &AppState) -> String {
    state.completed.lock().unwrap_or_else(|e| e.into_inner()).join("\n")
}

fn root_doc() -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "alashi arena v0": "off-chain партии на чистых правилах (alashi-rules)",
        "endpoints": {
            "POST /game/new": "{\"entry_fee\"?, \"phase_duration\"?, \"grace_s\"? (0..=30, дефолт 3), \"vote_weight_mode\"? (0 legacy | 1 contribution)} → game_id",
            "POST /game/:id/join": "{\"name\", \"model\", \"prompt\"} → agent_id + token",
            "GET  /game/:id/state": "публичное состояние партии",
            "GET  /game/:id/wait?r=1&p=market&t=30": "long-poll: спит до смены фазы (r/p — известные тебе раунд и фаза, t — таймаут сек, макс 60); ответ как /state + changed/timeout",
            "POST /game/:id/act": "{\"token\", \"action\": sell|buy|produce|donkey|bribe|vote|veto, \"params\"}",
            "POST /game/:id/advance": "permissionless кранк (как ончейн); в грейс-окне до grace_until отказ GraceWindow",
            "GET  /games": "активные партии",
            "GET  /leaderboard": "рейтинг агентов по завершённым партиям",
            "GET  /export": "завершённые партии JSONL",
        },
        "strategies_for_selfplay": ALL,
    })
}

pub fn handle(state: &AppState, req: &Request, stream: &mut TcpStream) {
    let path = req.path.split('?').next().unwrap_or("").to_string();
    let segs: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let body_v: serde_json::Value = serde_json::from_slice(&req.body).unwrap_or(serde_json::json!({}));
    let (status, body) = match (req.method.as_str(), segs.as_slice()) {
        ("GET", []) => ("200 OK", root_doc().to_string()),
        ("POST", ["game", "new"]) => ("200 OK", h_new_game(state, &body_v).to_string()),
        ("POST", ["game", id, "join"]) => match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_join(state, id, &body_v).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        },
        ("GET", ["game", id, "state"]) => match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_state(state, id).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        },
        // кастдев 02.09: long-poll — просыпаемся на смене фазы, а не
        // молотим state. GET /game/:id/wait?r=1&p=market&t=30
        ("GET", ["game", id, "wait"]) => match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_wait(state, id, &req.path).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        },
        ("POST", ["game", id, "act"]) => match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_act(state, id, &body_v).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        },
        ("POST", ["game", id, "advance"]) => match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_advance(state, id).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        },
        ("GET", ["games"]) => ("200 OK", h_games(state).to_string()),
        ("GET", ["leaderboard"]) => ("200 OK", h_leaderboard(state).to_string()),
        ("GET", ["export"]) => {
            let l = h_export(state);
            let body = if l.is_empty() {
                "[]".to_string()
            } else {
                format!("[{}]", l.split('\n').map(|x| x.to_string()).collect::<Vec<_>>().join(","))
            };
            ("200 OK", body)
        }
        _ => (
            "404 Not Found",
            err_json("not_found", "см. GET / для списка эндпоинтов").to_string(),
        ),
    };
    respond(stream, status, &body);
}

/// Поднять API и вернуть фактический адрес (порт 0 = свободный).
/// Для тестов и arenad.
pub fn serve_on(
    state: Arc<AppState>,
    addr: &str,
    tick_ms: u64,
) -> std::io::Result<std::net::SocketAddr> {
    let listener = TcpListener::bind(addr)?;
    let local = listener.local_addr()?;
    let crank_state = Arc::clone(&state);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(tick_ms));
        crank_once(&crank_state);
    });
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let st = Arc::clone(&state);
            std::thread::spawn(move || {
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                    .ok();
                if let Some(req) = read_request(&stream) {
                    handle(&st, &req, &mut stream);
                }
            });
        }
    });
    Ok(local)
}

pub fn serve(state: Arc<AppState>, addr: &str, tick_ms: u64) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    let crank_state = Arc::clone(&state);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(tick_ms));
        crank_once(&crank_state);
    });
    println!("alashi arena on http://{}", addr);
    let _ = std::io::Write::flush(&mut std::io::stdout());
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let st = Arc::clone(&state);
        std::thread::spawn(move || {
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .ok();
            if let Some(req) = read_request(&stream) {
                handle(&st, &req, &mut stream);
            }
        });
    }
    Ok(())
}

// reimplement sim_mut: Simulator field доступен напрямую
impl GameEntry {
    fn sim_mut(&mut self) -> &mut Simulator {
        &mut self.sim
    }
}

// silence unused warnings for GameError import (используется в типах ошибок)
#[allow(dead_code)]
fn _unused(_e: GameError) {}
