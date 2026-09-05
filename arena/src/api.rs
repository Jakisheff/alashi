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
use alashi_rules::state::{Faction, Game, Phase, VoteChoice};
use borsh;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Дефолт грейс-окна после фазы (кастдев 02.09, №1 голосования агентов).
pub const DEFAULT_GRACE_S: i64 = 3;
/// Верхняя граница grace_s при создании партии.
pub const MAX_GRACE_S: i64 = 30;
pub const MAX_ENTRY_FEE: u64 = 1_000_000 * PESO;
pub const MAX_PHASE_DURATION: i64 = 86_400;
pub const MAX_LOBBY_DURATION: i64 = 7 * 86_400;

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
    /// П5 (ТРИЗ, 05.09): кто купил инспект лицензии в этой партии
    /// (производное поле insider в /state; правила не трогаем).
    pub insiders: std::collections::HashSet<usize>,
    /// Канонический номер партии, виден ВСЕМ (state/join/games/export).
    /// game_id и party_no сохраняются при рестарте (кастдев 05.09: «номер игры должен знать
    /// каждый агент, а не только оператор»).
    pub party_no: u64,
    /// Человекочитаемая метка партии (необязательная, из POST /game/new).
    pub label: Option<String>,
    pub settlement_error: Option<String>,
}

pub struct AppState {
    pub games: Mutex<HashMap<u64, GameEntry>>,
    pub next_id: AtomicU64,
    pub completed: Mutex<Vec<String>>,
    pub master_seed: AtomicU64,
    snapshot_path: PathBuf,
    sequence_path: PathBuf,
}

// ---------- П7 (ТРИЗ, 05.09): сериализация состояния ----------
// Рестарт арены/деплой не убивает живую партию (кейс №20): снимок
// всех GameEntry пишется атомарно при каждом изменении и грузится
// на старте. Канон кодирования — borsh (AnchorSerialize), тот же,
// что ончейн: нулевой дрейф форматов.

fn state_file() -> std::path::PathBuf {
    std::path::PathBuf::from(
        std::env::var("ALASHI_STATE_FILE").unwrap_or_else(|_| "data/arena_state.json".into()),
    )
}

fn hex_enc(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}
fn hex_dec(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 || !s.is_ascii() {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok())
        .collect()
}

pub fn save_snapshot(state: &AppState) -> std::io::Result<()> {
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let completed = state.completed.lock().unwrap_or_else(|e| e.into_inner());
    let arr: Vec<serde_json::Value> = games
        .iter()
        .map(|(gid, e)| {
            serde_json::json!({
                "game_id": gid,
                "party_no": e.party_no,
                "label": e.label,
                "settlement_error": e.settlement_error,
                "entry_fee": e.entry_fee,
                "created": e.created,
                "grace_s": e.grace_s,
                "game_hex": borsh::to_vec(&e.sim.game).map(|v| hex_enc(&v)).unwrap_or_default(),
                "factions_hex": borsh::to_vec(&e.sim.factions).map(|v| hex_enc(&v)).unwrap_or_default(),
                "round_seed": e.sim.round_seed,
                "wallets": e.wallets.iter().map(|w| w.to_string()).collect::<Vec<_>>(),
                "agents": e.agents.iter().map(|a| serde_json::json!({
                    "name": a.name, "agent_id": a.agent_id, "token": a.token,
                    "model": a.model, "faction_idx": a.faction_idx,
                })).collect::<Vec<_>>(),
                "insiders": e.insiders.iter().cloned().collect::<Vec<_>>(),
                "action_log": e.action_log,
                "phase_log": e.phase_log,
            })
        })
        .collect();
    let doc = serde_json::json!({
        "v": 1,
        "saved_at": now(),
        "next_id_hint": state.next_id.load(Ordering::SeqCst),
        "master_seed": state.master_seed.load(Ordering::SeqCst),
        "games": arr,
        "completed": completed.clone(),
    });
    let p = &state.snapshot_path;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, doc.to_string())?;
    std::fs::rename(&tmp, p)
}

fn persist_snapshot(state: &AppState) {
    if let Err(e) = save_snapshot(state) {
        eprintln!("[ERROR] snapshot save: {e}");
    }
}

pub fn load_snapshot(state: &AppState) {
    
    let Ok(txt) = std::fs::read_to_string(&state.snapshot_path) else { return };
    let Ok(doc) = serde_json::from_str::<serde_json::Value>(&txt) else {
        eprintln!("[STATE] снимок не читается, старт пустой");
        return;
    };
    let mut loaded = 0u32;
    {
        let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        let mut completed = state.completed.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(seed) = doc["master_seed"].as_u64() {
            state.master_seed.store(seed, Ordering::SeqCst);
        }
        state.next_id.fetch_max(doc["next_id_hint"].as_u64().unwrap_or(1), Ordering::SeqCst);
        for g in doc["games"].as_array().into_iter().flatten() {
            let (Some(game_b), Some(fac_b)) = (
                hex_dec(g["game_hex"].as_str().unwrap_or("")),
                hex_dec(g["factions_hex"].as_str().unwrap_or("")),
            ) else { continue };
            let (Ok(game), Ok(factions)) = (
                borsh::from_slice::<Game>(&game_b),
                borsh::from_slice::<Vec<Faction>>(&fac_b),
            ) else { continue };
            let sim = Simulator { game, factions, round_seed: g["round_seed"].as_u64().unwrap_or(0) };
            let wallets: Vec<Pubkey> = g["wallets"].as_array().into_iter().flatten()
                .filter_map(|w| w.as_str().and_then(|s| s.parse::<Pubkey>().ok())).collect();
            let agents: Vec<AgentRec> = g["agents"].as_array().into_iter().flatten().filter_map(|a| {
                Some(AgentRec {
                    name: a["name"].as_str()?.to_string(),
                    agent_id: a["agent_id"].as_str()?.to_string(),
                    token: a["token"].as_str()?.to_string(),
                    model: a["model"].as_str().unwrap_or("?").to_string(),
                    faction_idx: a["faction_idx"].as_u64()? as usize,
                })
            }).collect();
            let insiders: std::collections::HashSet<usize> =
                g["insiders"].as_array().into_iter().flatten()
                    .filter_map(|x| x.as_u64().map(|v| v as usize)).collect();
            let gid = g["game_id"].as_u64().unwrap_or(0);
            let entry = GameEntry {
                sim,
                entry_fee: g["entry_fee"].as_u64().unwrap_or(10 * PESO),
                wallets,
                agents,
                created: g["created"].as_i64().unwrap_or(now()),
                grace_s: g["grace_s"].as_i64().unwrap_or(3),
                action_log: g["action_log"].as_array().cloned().unwrap_or_default(),
                phase_log: g["phase_log"].as_array().cloned().unwrap_or_default(),
                party_no: g["party_no"].as_u64().unwrap_or(0),
                label: g["label"].as_str().map(|s| s.to_string()),
                settlement_error: g["settlement_error"].as_str().map(str::to_string),
                insiders,
            };
            games.insert(gid, entry);
            loaded += 1;
            if gid >= state.next_id.load(Ordering::SeqCst) {
                state.next_id.store(gid.saturating_add(1), Ordering::SeqCst);
            }
        }
        if let Some(arr) = doc["completed"].as_array() {
            for c in arr {
                if let Some(s) = c.as_str() {
                    if let Ok(rec) = serde_json::from_str::<serde_json::Value>(s) {
                        if let Some(gid) = rec["game_id"].as_u64() {
                            state.next_id.fetch_max(gid.saturating_add(1), Ordering::SeqCst);
                        }
                    }
                    if !completed.iter().any(|existing| existing == s) {
                        completed.push(s.to_string());
                    }
                }
            }
        }
    }
    if loaded > 0 {
        eprintln!("[STATE] восстановлено партий: {}", loaded);
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Персистентный счётчик партий: файл хранит ПОСЛЕДНИЙ использованный
/// номер. Файла нет — считаем 18 (канон живых партий на 05.09).
/// Путь: $ALASHI_SEQ_FILE или data/arena_party_no.txt от CWD arenad.
fn next_party_no(path: &std::path::Path) -> std::io::Result<u64> {
    let last: u64 = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(18);
    let next = last.saturating_add(1);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, format!("{}", next))?;
    Ok(next)
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
    new_state_with_files(
        state_file(),
        std::env::var("ALASHI_SEQ_FILE").unwrap_or_else(|_| "data/arena_party_no.txt".into()),
    )
}

pub fn new_state_with_files(snapshot_path: impl Into<PathBuf>, sequence_path: impl Into<PathBuf>) -> Arc<AppState> {
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
        snapshot_path: snapshot_path.into(),
        sequence_path: sequence_path.into(),
    })
}

// ---------- служебное ----------

/// Дни epoch -> (год, месяц, день), гражданский алгоритм Хиннанта (UTC).
fn epoch_to_ymd(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// unix-секунды -> ISO 8601 UTC без внешних крейтов.
fn iso_utc(ts: u64) -> String {
    let (y, m, d) = epoch_to_ymd((ts / 86400) as i64);
    let s = ts % 86400;
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        s / 3600,
        (s % 3600) / 60,
        s % 60
    )
}

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
            let (ranks, payouts, rake, bank, breakdown) = match runner::try_settle(&entry.sim, entry.entry_fee, false) {
                Ok(plan) => plan,
                Err(error) => {
                    eprintln!("[ERROR] game {game_id} settlement: {error}");
                    entry.settlement_error = Some(error);
                    drop(games);
                    persist_snapshot(state);
                    return;
                }
            };
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
            // NDJSON-совместимый поток типизированных событий (паттерн
            // mshumer/autonomous-researcher ::EVENT::{json}): каждый ход
            // как машиночитаемое событие с ISO-временем рядом с
            // человеческим actions[]; фазы и сеттл — типами PHASE/SETTLE
            let mut events: Vec<serde_json::Value> = entry
                .action_log
                .iter()
                .map(|a| {
                    let ts = a["ts"].as_u64().unwrap_or(0);
                    serde_json::json!({
                        "type": "MOVE",
                        "ts": ts,
                        "ts_iso": iso_utc(ts),
                        "round": a["round"],
                        "phase": a["phase"],
                        "actor": a["actor"],
                        "action": a["action"],
                        "params": a["params"],
                        "by": a["by"],
                        "ok": a["ok"],
                        "err": a["err"],
                    })
                })
                .collect();
            for p in entry.phase_log.iter() {
                let mut e = p.clone();
                e["type"] = serde_json::json!("PHASE");
                events.push(e);
            }
            events.push(serde_json::json!({
                "type": "SETTLE",
                "ts": now(),
                "ts_iso": iso_utc(now() as u64),
                "ranks": ranks,
                "bank": bank,
            }));
            let v = serde_json::json!({
                "game_id": game_id,
                "party_no": entry.party_no,
                "label": entry.label,
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
                "events": events,
            });
            rec = Some(v.to_string());
        }
        games.remove(&game_id);
        if let Some(r) = rec {
            // Removal and insertion must be one snapshot-visible change.
            state.completed.lock().unwrap_or_else(|e| e.into_inner()).push(r);
        }
    }
    persist_snapshot(state);
}

/// Один тик кранка: двигает все партии, чьё время фазы вышло.
pub fn crank_once(state: &AppState) {
    let t = now();
    let mut to_settle: Vec<u64> = Vec::new();
    let mut to_expire: Vec<u64> = Vec::new();
    let mut changed = false;
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
                            changed = true;
                            entry.sim.game.phase_ends_at = t.saturating_add(entry.sim.game.phase_duration);
                            record_phase_close(entry, closing);
                        }
                    } else if late {
                        to_expire.push(gid);
                    }
                }
            }
            Phase::Finished => {
                if entry.settlement_error.is_none() {
                    to_settle.push(gid);
                }
            }
            Phase::Aborted => to_expire.push(gid),
            _ => {
                // грейс-окно: опоздавшие действия прошлой фазы ещё приняты,
                // кранк ждёт ends_at + grace_s (лобби выше — без грейса)
                if t >= entry.sim.game.phase_ends_at.saturating_add(entry.grace_s) {
                    let closing =
                        (entry.sim.game.phase, entry.sim.game.round, entry.sim.game.law_card);
                    if entry.sim.advance(t, seed).is_ok() {
                        changed = true;
                        entry.sim.game.phase_ends_at = t.saturating_add(entry.sim.game.phase_duration);
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
        changed = true;
        let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        for gid in to_expire {
            games.remove(&gid);
        }
    }
    if changed {
        persist_snapshot(state);
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
        // канон партии: одинаков до и после рестарта арены
        "party_no": entry.party_no,
        "settlement_error": entry.settlement_error,
        "label": entry.label,
        "phase": phase_name(g.phase),
        "round": g.round,
        "entry_fee": entry.entry_fee,
        "phase_ends_at": g.phase_ends_at,
        // грейс-окно: реальный дедлайн приёма действий = grace_until
        "grace_s": entry.grace_s,
        "grace_until": g.phase_ends_at.saturating_add(entry.grace_s),
        "now": now(),
        "law_card": if g.phase == Phase::Law { Some(g.law_card) } else { None },
        "law_card_name": if g.phase == Phase::Law { Some(law_name(g.law_card)) } else { None },
        "sold_counter": g.sold_this_round,
        "price_now": price_now,
        // П5 (ТРИЗ, 05.09): производные для расчётов агента — цена
        // СЛЕДУЮЩЕГО юнита и дедлайн окна ставок лицензии (r4 action)
        "price_next": eff_price(g.sold_this_round.saturating_add(1), g.active_price_shift, g.active_boom),
        "bids_close_at": if g.epoch == alashi_rules::constants::EPOCH_90S
            && g.round == alashi_rules::constants::AUCTION_ROUND
            && g.phase == Phase::Action && !g.license_sold
        { serde_json::json!(g.phase_ends_at.saturating_add(entry.grace_s)) } else { serde_json::Value::Null },
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
                "by": a.get("by"),
                "ok": a.get("ok"),
                "ts": a.get("ts"),
            })).collect::<Vec<_>>(),
        "factions": entry.sim.factions.iter().enumerate().map(|(i, f)| {
            serde_json::json!({
                "idx": i,
                "name": f.name,
                "agent_id": entry.agents.iter().find(|a| a.faction_idx == i).map(|a| a.agent_id.clone()),
                "cash": f.cash,
                // П5: кэш уже после всех эскроу/списаний — предел ставки
                // и покупок считается без повторного запроса (кейс Aitore:
                // бид 11M при живых 7M после инспекта)
                "cash_available": f.cash,
                // П5: куплен ли взгляд на доход лицензии в этой партии
                "insider": entry.insiders.contains(&i),
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
        .unwrap_or(alashi_rules::constants::VOTE_WEIGHT_LEGACY as u64);
    let epoch = match body.get("epoch").and_then(|v| v.as_str()) {
        Some("90s") => alashi_rules::constants::EPOCH_90S,
        Some("classic") | None => alashi_rules::constants::EPOCH_CLASSIC,
        _ => return err_json("bad_params", "epoch: classic | 90s"),
    };
    if !(1..=MAX_ENTRY_FEE).contains(&entry_fee) || !(1..=MAX_PHASE_DURATION).contains(&phase_duration) {
        return err_json("bad_params", "entry_fee: 1..=1000000000000; phase_duration: 1..=86400");
    }
    if !(0..=MAX_GRACE_S).contains(&grace_s) {
        return err_json("bad_params", "grace_s: 0..=30");
    }
    if vote_weight_mode > alashi_rules::constants::VOTE_WEIGHT_CONTRIB as u64 {
        return err_json("bad_params", "vote_weight_mode: 0 legacy, 1 contribution");
    }
    let lobby_duration = body.get("lobby_duration").and_then(|v| v.as_i64())
        .unwrap_or(phase_duration * LOBBY_MULT);
    if !(1..=MAX_LOBBY_DURATION).contains(&lobby_duration) {
        return err_json("bad_params", "lobby_duration: 1..=604800");
    }
    // Serialize ID/sequence allocation with insertion for simultaneous creators.
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let game_id = match state.next_id.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |id| id.checked_add(1)) {
        Ok(id) => id,
        Err(_) => return err_json("id_exhausted", "закончились идентификаторы партий"),
    };
    let entropy = alashi_rules::constants::ENTROPY_SLOTHASH;
    let mut sim = Simulator::new(game_id, entry_fee, phase_duration, entropy);
    sim.game.vote_weight_mode = vote_weight_mode as u8;
    sim.game.epoch = epoch;
    // кастдев №5/ночь 05.09: окно джойна внешних рвётся - длина лобби
    // отвязана от длины фазы; отдельно задаётся lobby_duration (сек),
    // дефолт прежний phase_duration x LOBBY_MULT. Пола нет: тесты
    // и быстрые смоуки используют короткие фазы.
    sim.game.phase_ends_at = now() + lobby_duration;
    let party_no = match next_party_no(&state.sequence_path) {
        Ok(number) => number,
        Err(e) => {
            eprintln!("[ERROR] party sequence: {e}");
            return err_json("sequence_failed", "не удалось сохранить номер партии");
        }
    };
    let label = body
        .get("label")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty() && s.len() <= 32)
        .map(|s| s.to_string());
    let entry = GameEntry {
        sim,
        entry_fee,
        wallets: vec![],
        agents: vec![],
        created: now(),
        grace_s,
        action_log: vec![],
        phase_log: vec![],
        insiders: Default::default(),
        party_no,
        label,
        settlement_error: None,
    };
    let v = state_json(game_id, &entry);
    games.insert(game_id, entry);
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
    // кастдев №5 (Aisultan): плейсхолдеры из примеров джойна принимались
    // буквально («имя», «модель», «test») — предупреждаем в ответе
    let placeholder_warn = matches!(name.as_str(), "имя" | "модель" | "test" | "Agent")
        || matches!(model.as_str(), "модель" | "name" | "test");
    let agent_id = agent_id_of(&model, &prompt);
    let token = random_hex();
    let recover = body.get("recover").and_then(|v| v.as_bool()).unwrap_or(false);
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = games.get_mut(&game_id) else {
        return err_json("unknown_game", "партия не найдена или закрыта");
    };
    // Recovery is an existing-session operation, not a new join.
    if let Some(i) = entry.agents.iter().position(|a| a.agent_id == agent_id) {
        if recover {
            let faction_idx = entry.agents[i].faction_idx;
            entry.agents[i].token = token.clone();
            let v = state_json(game_id, entry);
            return serde_json::json!({
                "ok": true, "recovered": true, "game_id": game_id,
                "agent_id": agent_id, "token": token, "faction_idx": faction_idx,
                "warning": "токен перевыпущен; предыдущий отозван", "state": v
            });
        }
        return err_json("join_failed", "DuplicateWallet: агент уже в партии; для восстановления передай recover: true");
    }
    if recover {
        return err_json("unknown_agent", "агент не участвовал в этой партии");
    }
    if entry.agents.len() >= MAX_FACTIONS as usize {
        return err_json("game_full", "мест нет");
    }
    let mut h = Sha256::new();
    h.update(agent_id.as_bytes());
    h.update(&game_id.to_le_bytes());
    let wallet = Pubkey::new_from_array(h.finalize().into());
    if let Err(e) = entry.sim.join(wallet, &name) {
        return err_json("join_failed", &format!("{:?}", e));
    }
    let faction_idx = entry.sim.factions.len() - 1;
    entry.wallets.push(wallet);
    entry.agents.push(AgentRec {
        name: name.clone(),
        agent_id: agent_id.clone(),
        token: token.clone(),
        model,
        faction_idx,
    });
    // П2 (ТРИЗ): токен в лог оператора при каждом join — страховка
    // от утери ответа сессией (ночной кейс 05.09)
    eprintln!(
        "[join] party {} faction {} «{}» agent {}.. model {} token {}",
        entry.party_no, faction_idx, name, &agent_id[..8.min(agent_id.len())], body.get("model").and_then(|v| v.as_str()).unwrap_or("?"), token
    );
    let v = state_json(game_id, entry);
    serde_json::json!({"ok": true, "agent_id": agent_id, "token": token, "faction_idx": faction_idx,
        "warning": if placeholder_warn { Some("имя/model похожи на плейсхолдер из примера — подставь реальные значения; токен сохранить сразу; восстановление через join с recover: true") } else { None },
        "state": v})
}

fn h_state(state: &AppState, game_id: u64) -> serde_json::Value {
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    match games.get(&game_id) {
        Some(e) => serde_json::json!({"ok": true, "state": state_json(game_id, e)}),
        None => {
            let completed = state.completed.lock().unwrap_or_else(|e| e.into_inner());
            let rec = completed
                .iter()
                .rev()
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
    // R8 (REVIEW_EXTERNAL): источник хода — самозаявлен клиентом,
    // отличает решение модели от жадного фоллбэка в /export
    let by = body.get("by").and_then(|v| v.as_str()).unwrap_or("unknown");
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
                    // П5: фиксируем инсайдерство для поля /state
                    entry.insiders.insert(idx);
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
        "by": by,
        "ok": ok,
        "err": err,
        "cash_after": log.cash_after,
        "goods_after": log.goods_after,
        "ts": now(),
    }));
    let v = state_json(game_id, entry);
    // кастдев №5: повтор при потерянном ответе выглядел как «за меня
    // играл фоллбэк» — прямо говорим, что это почти наверняка свой повтор
    let hint = match err.as_deref() {
        Some("AlreadyActed") | Some("AlreadyVoted") => Some(
            "ход/голос в этой фазе уже принят — почти наверняка твой же повторный запрос, чей ответ потерялся в туннеле. Серверных фоллбэков нет: ходы делает только владелец токена",
        ),
        _ => None,
    };
    serde_json::json!({"ok": ok, "error": err, "hint": hint, "action_log": log_to_json(&log), "state": v})
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
    ) && t < entry.sim.game.phase_ends_at.saturating_add(entry.grace_s)
    {
        let until = entry.sim.game.phase_ends_at.saturating_add(entry.grace_s);
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
            entry.sim.game.phase_ends_at = t.saturating_add(entry.sim.game.phase_duration);
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

/// Кастдев №5: GET /slots?agent_id=<hex> — во всех АКТИВНЫХ партиях
/// находит фракции этого агента (кошелёк детерминирован от
/// agent_id+game_id). Отвечает на «где я уже сижу» без тест-джойнов.
fn h_slots(state: &AppState, raw_path: &str) -> serde_json::Value {
    let q = raw_path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let agent_id = q
        .split('&')
        .find_map(|kv| kv.split_once('=').filter(|(k, _)| *k == "agent_id").map(|(_, v)| v.to_string()))
        .unwrap_or_default();
    if agent_id.len() != 64 || !agent_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return err_json("bad_params", "нужен ?agent_id= (64 hex, из ответа join)");
    }
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let mut found = Vec::new();
    for (&gid, e) in games.iter() {
        let mut h = Sha256::new();
        h.update(agent_id.as_bytes());
        h.update(&gid.to_le_bytes());
        let d: [u8; 32] = h.finalize().into();
        let wallet = Pubkey::new_from_array(d);
        if let Some(i) = e.wallets.iter().position(|w| *w == wallet) {
            found.push(serde_json::json!({
                "game_id": gid,
                "party_no": e.party_no,
                "label": e.label,
                "phase": phase_name(e.sim.game.phase),
                "round": e.sim.game.round,
                "faction_idx": i,
                "faction_name": e.agents.get(i).map(|a| a.name.clone()),
            }));
        }
    }
    serde_json::json!({"ok": true, "agent_id": agent_id, "active_slots": found})
}

fn h_games(state: &AppState) -> serde_json::Value {
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let list: Vec<serde_json::Value> = games
        .iter()
        .map(|(&gid, e)| {
            serde_json::json!({
                "game_id": gid,
                "party_no": e.party_no,
                "label": e.label,
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
    // Plackett-Luce по партиям в порядке завершения (канон: исследование
    // владельца «Оценка LLM в ончейн-играх», приоритет 1 — вместо парного
    // Elo; реализация rating.rs свёрена с openskill.py тест-векторами)
    let mut rt: std::collections::HashMap<String, crate::rating::Rating> =
        std::collections::HashMap::new();
    for line in completed.iter() {
        let Ok(r) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let ranks = r["ranks"].as_array().cloned().unwrap_or_default();
        let payouts = r["payouts"].as_array().cloned().unwrap_or_default();
        let agents = r["agents"].as_array().cloned().unwrap_or_default();
        let mut party_players: Vec<(String, u64)> = Vec::new();
        for (place, fi) in ranks.iter().enumerate() {
            let fi = fi.as_u64().unwrap_or(0) as usize;
            let Some(a) = agents.get(fi) else { continue };
            let key = a["agent_id"].as_str().unwrap_or("?").to_string();
            if !rt.contains_key(&key) {
                rt.insert(key.clone(), crate::rating::Rating::new());
            }
            party_players.push((key.clone(), place as u64));
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
        crate::rating::rate_party(&party_players, &mut rt);
    }
    let mut out: Vec<serde_json::Value> = rows.into_values().collect();
    for e in out.iter_mut() {
        let g = e["games"].as_u64().unwrap_or(1).max(1);
        e["avg_rank"] = serde_json::json!(e["rank_sum"].as_f64().unwrap_or(0.0) / g as f64);
        if let Some(r) = e["agent_id"].as_str().and_then(|k| rt.get(k)) {
            e["plackett_luce_mu"] = serde_json::json!((r.mu * 1e6).round() / 1e6);
            e["plackett_luce_sigma"] = serde_json::json!((r.sigma * 1e6).round() / 1e6);
            e["plackett_luce_ordinal"] = serde_json::json!((r.ordinal() * 1e6).round() / 1e6);
        }
    }
    out.sort_by(|a, b| {
        b["plackett_luce_ordinal"].as_f64().unwrap_or(0.0)
            .total_cmp(&a["plackett_luce_ordinal"].as_f64().unwrap_or(0.0))
            .then_with(|| a["agent_id"].as_str().cmp(&b["agent_id"].as_str()))
    });
    serde_json::json!({
        "ok": true,
        "rating_model": "plackett-luce (weng-lin, openskill-совместимо)",
        "leaderboard": out,
    })
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
            "POST /game/new": "{\"entry_fee\"?, \"phase_duration\"?, \"grace_s\"? (0..=30, дефолт 3), \"vote_weight_mode\"? (0 legacy | 1 contribution), \"lobby_duration\"? (сек, дефолт = фаза x 5 — окно джойна можно растянуть независимо от фаз), \"label\"? (до 32 байт, видно всем)} → game_id + party_no",
            "POST /game/:id/join": "{\"name\", \"model\", \"prompt\"} → agent_id + token",
            "GET  /game/:id/state": "публичное состояние партии",
            "GET  /game/:id/wait?r=1&p=market&t=30": "long-poll: спит до смены фазы (r/p — известные тебе раунд и фаза, t — таймаут сек, макс 60); ответ как /state + changed/timeout",
            "POST /game/:id/act": "{\"token\", \"action\": sell|buy|produce|donkey|bribe|vote|veto, \"params\"}",
            "POST /game/:id/advance": "permissionless кранк (как ончейн); в грейс-окне до grace_until отказ GraceWindow",
            "GET  /games": "активные партии",
            "GET  /ui": "зрительский экран живой арены (app/arena.html)",
            "GET  /slots?agent_id=": "во всех активных партиях — где сидит этот агент (фракции, фазы)",
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
    // зрительский экран: GET /ui из app/arena.html (для демо, через туннель)
    if req.method == "GET" && (path == "/ui" || path == "/ui/") {
        let html = std::fs::read_to_string("app/arena.html")
            .or_else(|_| std::fs::read_to_string("../app/arena.html"))
            .unwrap_or_else(|_| "<html><body>app/arena.html не найден (запусти arenad из корня репо)</body></html>".into());
        crate::http::respond_html(stream, "200 OK", &html, "text/html; charset=utf-8");
        return;
    }
    let mut is_post_mut = false;
    let (status, body) = match (req.method.as_str(), segs.as_slice()) {
        ("GET", []) => ("200 OK", root_doc().to_string()),
        ("POST", ["game", "new"]) => { is_post_mut = true; ("200 OK", h_new_game(state, &body_v).to_string()) },
        ("POST", ["game", id, "join"]) => { is_post_mut = true; match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_join(state, id, &body_v).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        } },
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
        ("POST", ["game", id, "act"]) => { is_post_mut = true; match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_act(state, id, &body_v).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        } },
        ("POST", ["game", id, "advance"]) => { is_post_mut = true; match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_advance(state, id).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        } },
        ("GET", ["games"]) => ("200 OK", h_games(state).to_string()),
        // кастдев №5 (Aisultan): где мой кошелёк уже сидит — без этого
        // агент реконструирует лимиты тестовыми партиями
        ("GET", ["slots"]) => ("200 OK", h_slots(state, &req.path).to_string()),
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
    // П7: любой POST меняет состояние - снимок на диск
    if is_post_mut {
        if let Err(e) = save_snapshot(state) {
            eprintln!("[ERROR] snapshot save after POST: {e}");
            respond(stream, "503 Service Unavailable", &err_json(
                "snapshot_failed", "изменение в памяти принято, но запись на диск не удалась; проверь state перед повтором",
            ).to_string());
            return;
        }
    }
    respond(stream, status, &body);
}

/// Поднять API и вернуть фактический адрес (порт 0 = свободный).
/// Для тестов и arenad.
pub fn serve_on(
    state: Arc<AppState>,
    addr: &str,
    tick_ms: u64,
) -> std::io::Result<std::net::SocketAddr> {
    load_snapshot(&state);
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
    load_snapshot(&state);
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

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;
