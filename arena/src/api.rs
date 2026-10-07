//! HTTP API арены: партии для внешних агентов поверх чистых правил.
//! Каждое действие агента = один вызов /act с токеном. Кранк фаз —
//! фоновый поток по таймеру ИЛИ permissionless POST /advance (как ончейн).

use crate::http::{read_request, respond, Request};
use crate::runner::{self, ActionLog};
use crate::registration::{self, Proof, Receipt};
use crate::strategies::{ActionAction, LawAction, MarketAction};
use crate::strategies::{eff_price, ALL};
use alashi_rules::anchor_lang::prelude::Pubkey;
use alashi_rules::constants::*;
use alashi_rules::sim::Simulator;
use alashi_rules::state::{Faction, Game, Phase, VoteChoice};
use borsh;
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
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
/// Аудит 27.09 (S2): пределы накопления. Значения выбраны под размер
/// пилота и меняются одной строкой.
pub const MAX_ACTIVE_GAMES: usize = 100;
pub const MAX_COMPLETED_GAMES: usize = 250;
pub const MAX_COMPLETED_REPLAY_GAMES: usize = 16;
pub const MAX_ACTION_LOG: usize = 256;
pub const RECENT_ACTIONS_LIMIT: usize = 12;
pub const MAX_PHASE_LOG: usize = 512;
/// Аудит 27.09 (S2): самозаявленная метка источника хода в журнале.
pub const MAX_BY_LEN: usize = 64;
pub const MAX_REGISTERED_AGENTS: usize = 1000;
pub const MAX_CONFIRM_VERIFIERS: u64 = 4;
pub const MATCH_LOBBY_DURATION_S: i64 = 900;
pub const MATCH_MIN_JOIN_TIME_S: i64 = 60;
pub const MATCH_READY_CLOSE_S: i64 = 30;
pub const MATCH_READY_MIN_JOIN_TIME_S: i64 = 5;
pub const MAX_RECENT_OPS: usize = 64;
pub const SESSION_LIFETIME_S: i64 = 86_400;

#[derive(Clone)]
pub struct AgentRec {
    pub name: String,
    /// Отпечаток стратегии: SHA256(model, prompt). Не личность.
    pub agent_id: String,
    /// Отчёт «Цукерберг/Muse» 27.09: постоянная личность персонажа.
    pub character_id: String,
    /// Владелец персонажа (хэш секрета): виден организатору клуба,
    /// чтобы отличать разных людей от одного оператора.
    pub owner_id: String,
    pub agent_record_id: Option<String>,
    pub token: String,
    pub session_expires_at: Option<i64>,
    pub recovery_hash: Option<String>,
    pub registration: Option<Receipt>,
    pub model: String,
    pub faction_idx: usize,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredAgent {
    pub wallet: String,
    pub owner_id: String,
    pub character_id: String,
    pub recovery_hash: String,
    pub challenge: String,
    pub receipt: Option<Receipt>,
    /// Informational only: an issued Memo must remain confirmable after delays.
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpRecord {
    pub id: u64,
    pub request_hash: String,
    pub response: serde_json::Value,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpState {
    pub last: u64,
    pub recent: Vec<OpRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletedSession {
    pub token_hash: String,
    pub expires_at: i64,
    pub op_state: OpState,
}

fn valid_op_state(ops: &OpState) -> bool {
    ops.recent.len() <= MAX_RECENT_OPS
        && (ops.last == 0) == ops.recent.is_empty()
        && !ops.recent.last().is_some_and(|r| r.id != ops.last)
        && !ops.recent.iter().any(|r| !hex32(&r.request_hash)
            || r.response["op_id"].as_u64() != Some(r.id)
            || r.response["op_consumed"] != true)
        && !ops.recent.windows(2).any(|w| w[0].id.checked_add(1) != Some(w[1].id))
}

#[derive(Clone)]
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
    pub op_state: HashMap<String, OpState>,
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
    pub managed_match: bool,
    pub settlement_error: Option<String>,
}

pub struct AppState {
    pub games: Mutex<HashMap<u64, GameEntry>>,
    pub next_id: AtomicU64,
    pub completed: Mutex<Vec<String>>,
    completed_replay: Mutex<HashMap<u64, HashMap<String, CompletedSession>>>,
    pub master_seed: AtomicU64,
    pub require_devnet_registration: bool,
    pub require_platform_v2: bool,
    pub registrations: Mutex<HashMap<String, RegisteredAgent>>,
    proposal_key: Mutex<Option<String>>,
    snapshot_path: PathBuf,
    sequence_path: PathBuf,
    // ponytail: one transaction gate for this single-file arena. Split persistence
    // per game if contention matters; every writer acquires this before games.
    snapshot_lock: Mutex<()>,
    connections: Arc<AtomicU64>,
    waiters: Arc<AtomicU64>,
    confirmations: Arc<AtomicU64>,
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
    let _writer = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    save_snapshot_locked(state)
}

// Caller holds snapshot_lock for the full mutation -> save -> rollback cycle.
fn save_snapshot_locked(state: &AppState) -> std::io::Result<()> {
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let completed = state.completed.lock().unwrap_or_else(|e| e.into_inner());
    let arr: Vec<serde_json::Value> = games
        .iter()
        .map(|(gid, e)| {
            serde_json::json!({
                "game_id": gid,
                "party_no": e.party_no,
                "label": e.label,
                "managed_match": e.managed_match,
                "settlement_error": e.settlement_error,
                "entry_fee": e.entry_fee,
                "created": e.created,
                "grace_s": e.grace_s,
                "game_hex": borsh::to_vec(&e.sim.game).map(|v| hex_enc(&v)).unwrap_or_default(),
                "factions_hex": borsh::to_vec(&e.sim.factions).map(|v| hex_enc(&v)).unwrap_or_default(),
                "round_seed": e.sim.round_seed,
                "wallets": e.wallets.iter().map(|w| w.to_string()).collect::<Vec<_>>(),
                "agents": e.agents.iter().map(|a| serde_json::json!({
                    "name": a.name, "agent_id": a.agent_id, "character_id": a.character_id,
                    "owner_id": a.owner_id, "agent_record_id": a.agent_record_id,
                    "token": a.token, "session_expires_at": a.session_expires_at,
                    "model": a.model, "faction_idx": a.faction_idx, "recovery_hash": a.recovery_hash,
                    "registration": a.registration,
                })).collect::<Vec<_>>(),
                "insiders": e.insiders.iter().cloned().collect::<Vec<_>>(),
                "action_log": e.action_log,
                "op_state": e.op_state,
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
        "registrations": *state.registrations.lock().unwrap_or_else(|e| e.into_inner()),
        "proposal_key": state.proposal_key.lock().unwrap_or_else(|e| e.into_inner()).clone(),
        "completed": completed.clone(),
        "completed_replay": *state.completed_replay.lock().unwrap_or_else(|e| e.into_inner()),
    });
    drop(completed);
    drop(games);
    let p = &state.snapshot_path;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = p.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    std::io::Write::write_all(&mut file, doc.to_string().as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&tmp, p)
}

/// Аудит 27.09 (S4): повреждённый снимок не молчит. Отсутствие файла —
/// чистый старт; любой другой сбой чтения или структуры — ошибка, оригинал
/// сохраняется рядом как .corrupt.<ts>, пустое состояние не подменяет
/// данные. Вызывающая сторона (serve) обязана отказаться от старта.
pub fn load_snapshot(state: &AppState) -> Result<(), String> {
    let txt = match std::fs::read_to_string(&state.snapshot_path) {
        Ok(txt) => txt,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("снимок не читается: {e}")),
    };
    let backup_path = state
        .snapshot_path
        .with_extension(format!("json.corrupt.{}", now()));
    let fail = |msg: String| -> Result<(), String> {
        match std::fs::rename(&state.snapshot_path, &backup_path) {
            Ok(()) => Err(format!("{msg}; оригинал сохранён: {}", backup_path.display())),
            Err(e) => Err(format!("{msg}; сохранить оригинал не удалось: {e}")),
        }
    };
    let doc = match serde_json::from_str::<serde_json::Value>(&txt) {
        Ok(doc) => doc,
        Err(e) => return fail(format!("снимок повреждён (JSON): {e}")),
    };
    if doc.get("v").and_then(|v| v.as_u64()) != Some(1) {
        return fail("неизвестная версия снимка".into());
    }
    let registrations: HashMap<String, RegisteredAgent> = match doc.get("registrations") {
        Some(value) => match serde_json::from_value(value.clone()) {
            Ok(records) => records, Err(_) => return fail("битый registry агентов".into()),
        },
        None => HashMap::new(),
    };
    let proposal_key = match doc.get("proposal_key") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => match value.as_str().filter(|key| hex32(key)) {
            Some(key) => Some(key.to_string()),
            None => return fail("битый ключ lifecycle proposal".into()),
        },
    };
    if registrations.len() > MAX_REGISTERED_AGENTS { return fail("registry агентов переполнен".into()); }
    for (id, record) in &registrations {
        if !hex32(id) || !hex32(&record.owner_id) || !hex32(&record.character_id)
            || !hex32(&record.recovery_hash) || !hex32(&record.challenge)
            || registration::validate_wallet(&record.wallet).is_err()
            || record.receipt.as_ref().is_some_and(|r| r.mode != "agent_lifecycle_v2"
                || r.wallet != record.wallet || r.network != "devnet") {
            return fail("битая запись registry агентов".into());
        }
    }
    let completed_replay: HashMap<u64, HashMap<String, CompletedSession>> =
        match doc.get("completed_replay") {
            Some(value) => match serde_json::from_value(value.clone()) {
                Ok(records) => records,
                Err(_) => return fail("битый архив повторов".into()),
            },
            None => HashMap::new(),
        };
    if completed_replay.len() > MAX_COMPLETED_REPLAY_GAMES || completed_replay.values().any(|sessions|
        sessions.len() > MAX_FACTIONS as usize || sessions.iter().any(|(id, session)|
            !hex32(id) || !hex32(&session.token_hash) || !valid_op_state(&session.op_state))) {
        return fail("битый архив повторов".into());
    }
    let Some(games_arr) = doc.get("games").and_then(|g| g.as_array()) else {
        return fail("в снимке нет массива games".into());
    };
    let mut loaded = 0u32;
    {
        let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        let mut completed = state.completed.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(seed) = doc["master_seed"].as_u64() {
            state.master_seed.store(seed, Ordering::SeqCst);
        }
        state.next_id.fetch_max(doc["next_id_hint"].as_u64().unwrap_or(1), Ordering::SeqCst);
        for g in games_arr {
            let Some(gid) = g["game_id"].as_u64() else {
                return fail("запись партии без game_id".into());
            };
            let (Some(game_b), Some(fac_b)) = (
                hex_dec(g["game_hex"].as_str().unwrap_or("")),
                hex_dec(g["factions_hex"].as_str().unwrap_or("")),
            ) else {
                return fail(format!("партия {gid}: game_hex/factions_hex не декодируются"));
            };
            let (Ok(game), Ok(factions)) = (
                borsh::from_slice::<Game>(&game_b),
                borsh::from_slice::<Vec<Faction>>(&fac_b),
            ) else {
                return fail(format!("партия {gid}: borsh-запись не декодируется"));
            };
            let sim = Simulator { game, factions, round_seed: g["round_seed"].as_u64().unwrap_or(0) };
            let Some(wallets) = g["wallets"].as_array().map(|arr| {
                arr.iter().map(|w| w.as_str().and_then(|s| s.parse::<Pubkey>().ok())).collect::<Option<Vec<Pubkey>>>()
            }).flatten() else {
                return fail(format!("партия {gid}: битый кошелёк"));
            };
            let mut agents: Vec<AgentRec> = Vec::new();
            for a in g["agents"].as_array().into_iter().flatten() {
                let Some(rec) = (|| {
                    let agent_id = a["agent_id"].as_str()?.to_string();
                    Some(AgentRec {
                        name: a["name"].as_str()?.to_string(),
                        agent_id: agent_id.clone(),
                        // legacy-снимки без персонажа: личность = отпечаток
                        // стратегии (отчёт 27.09, миграция без потери данных)
                        character_id: a["character_id"].as_str().unwrap_or(&agent_id).to_string(),
                        owner_id: a["owner_id"].as_str().unwrap_or("legacy").to_string(),
                        agent_record_id: a["agent_record_id"].as_str().map(str::to_string),
                        session_expires_at: a["session_expires_at"].as_i64(),
                        token: a["token"].as_str()?.to_string(),
                        recovery_hash: a["recovery_hash"].as_str().map(str::to_string),
                        registration: match a.get("registration").filter(|v| !v.is_null()) {
                            Some(value) => Some(serde_json::from_value(value.clone()).ok()?),
                            None => None,
                        },
                        model: a["model"].as_str().unwrap_or("?").to_string(),
                        faction_idx: a["faction_idx"].as_u64()? as usize,
                    })
                })() else {
                    return fail(format!("партия {gid}: битая запись агента"));
                };
                agents.push(rec);
            }
            let Some(insiders) = g["insiders"].as_array().map(|arr| {
                arr.iter().map(|x| x.as_u64().map(|v| v as usize)).collect::<Option<std::collections::HashSet<usize>>>()
            }).flatten() else {
                return fail(format!("партия {gid}: битые инсайдеры"));
            };
            let mut action_log = g["action_log"].as_array().cloned().unwrap_or_default();
            // Legacy snapshots never had IDs. Number only their retained history;
            // discarded historical attempts cannot be recovered.
            let legacy = action_log.iter().all(|a| a.get("seq").is_none());
            let mut previous: Option<u64> = None;
            for (i, action) in action_log.iter_mut().enumerate() {
                if !action.is_object() {
                    return fail(format!("партия {gid}: битая запись действия"));
                }
                if legacy {
                    action["seq"] = serde_json::json!(i as u64 + 1);
                }
                let Some(seq) = action["seq"].as_u64().filter(|&seq| seq > 0) else {
                    return fail(format!("партия {gid}: неверный номер действия"));
                };
                if previous.is_some_and(|prev| prev.checked_add(1) != Some(seq)) {
                    return fail(format!("партия {gid}: нарушен порядок действий"));
                }
                previous = Some(seq);
            }
            let op_state: HashMap<String, OpState> = match g.get("op_state") {
                Some(value) => match serde_json::from_value(value.clone()) {
                    Ok(state) => state, Err(_) => return fail(format!("партия {gid}: битые op_state")),
                },
                None => HashMap::new(),
            };
            for (id, ops) in &op_state {
                if !hex32(id) || !valid_op_state(ops) {
                    return fail(format!("партия {gid}: нарушен порядок op_state"));
                }
            }
            let entry = GameEntry {
                sim,
                entry_fee: g["entry_fee"].as_u64().unwrap_or(10 * PESO),
                wallets,
                agents,
                created: g["created"].as_i64().unwrap_or(now()),
                grace_s: g["grace_s"].as_i64().unwrap_or(3),
                action_log,
                op_state,
                phase_log: g["phase_log"].as_array().cloned().unwrap_or_default(),
                party_no: g["party_no"].as_u64().unwrap_or(0),
                label: g["label"].as_str().map(|s| s.to_string()),
                managed_match: g["managed_match"].as_bool().unwrap_or(false),
                settlement_error: g["settlement_error"].as_str().map(str::to_string),
                insiders,
            };
            games.insert(gid, entry);
            loaded += 1;
            if gid >= state.next_id.load(Ordering::SeqCst) {
                state.next_id.store(gid.saturating_add(1), Ordering::SeqCst);
            }
        }
        *state.registrations.lock().unwrap_or_else(|e| e.into_inner()) = registrations;
        *state.proposal_key.lock().unwrap_or_else(|e| e.into_inner()) = proposal_key;
        *state.completed_replay.lock().unwrap_or_else(|e| e.into_inner()) = completed_replay;
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
    Ok(())
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

/// Аудит 27.09 (S3): секрет арены берётся из ОС, а не из времени.
/// Для локальных воспроизводимых прогонов: ALASHI_MASTER_SEED=<u64>.
fn master_seed_from_env_or_os() -> u64 {
    if let Ok(v) = std::env::var("ALASHI_MASTER_SEED") {
        if let Ok(seed) = v.trim().parse::<u64>() {
            return seed;
        }
    }
    random_u64().expect("нет доступа к источнику энтропии ОС (/dev/urandom)")
}

fn random_u64() -> std::io::Result<u64> {
    use std::io::Read;
    let mut bytes = [0u8; 8];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

fn random_hex() -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(hex_enc(&bytes))
}

fn recovery_hash(secret: &str) -> Option<String> {
    if secret.len() != 64 { return None; }
    let bytes = hex_dec(secret)?;
    Some(hex_enc(&Sha256::digest(bytes)))
}

fn secret_matches(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

/// Аудит 27.09 (S7): составной идентификатор кодируется однозначно,
/// с префиксами длины. Раньше ("a|b","c") и ("a","b|c") давали один
/// хэш.
pub fn agent_id_of(model: &str, prompt: &str) -> String {
    let mut h = Sha256::new();
    h.update((model.len() as u64).to_le_bytes());
    h.update(model.as_bytes());
    h.update((prompt.len() as u64).to_le_bytes());
    h.update(prompt.as_bytes());
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

/// Отчёт «Цукерберг/Muse» 27.09, раздел 6: владелец персонажа — хэш
/// секрета (owner_key клиента или recovery_secret сессии). Секрет
/// передаётся backend при POST; сервер хранит только хэш.
pub fn owner_id_of(owner_key: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"alashi-owner-v1");
    h.update((owner_key.len() as u64).to_le_bytes());
    h.update(owner_key.as_bytes());
    h.finalize().iter().map(|b| format!("{:02x}", b)).collect()
}

/// Персонаж = владелец + имя. Модель и промпт не входят: смена версии
/// стратегии не создаёт новую личность.
fn hex32(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn sha256_hex(token: &str) -> String {
    hex_enc(&Sha256::digest(token.as_bytes()))
}

fn agent_token_matches(agent: &AgentRec, token: &str) -> bool {
    if agent.agent_record_id.is_some() {
        agent.session_expires_at.is_some_and(|t| now() < t)
            && hex32(token)
            && secret_matches(&agent.token, &sha256_hex(token))
    } else {
        secret_matches(&agent.token, token)
    }
}

pub fn character_id_v2(owner_id: &str, agent_record_id: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"alashi-character-v2");
    h.update((owner_id.len() as u64).to_le_bytes());
    h.update(owner_id.as_bytes());
    h.update((agent_record_id.len() as u64).to_le_bytes());
    h.update(agent_record_id.as_bytes());
    hex_enc(&h.finalize())
}

pub fn character_id_of(owner_id: &str, name: &str) -> String {
    let mut h = Sha256::new();
    h.update(b"alashi-character-v1");
    h.update((owner_id.len() as u64).to_le_bytes());
    h.update(owner_id.as_bytes());
    h.update((name.len() as u64).to_le_bytes());
    h.update(name.as_bytes());
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
        completed_replay: Mutex::new(HashMap::new()),
        master_seed: AtomicU64::new(master_seed_from_env_or_os()),
        require_devnet_registration: std::env::var("ALASHI_REQUIRE_DEVNET_REGISTRATION").as_deref() == Ok("1"),
        require_platform_v2: std::env::var("ALASHI_REQUIRE_PLATFORM_V2").as_deref() == Ok("1"),
        registrations: Mutex::new(HashMap::new()),
        proposal_key: Mutex::new(None),
        snapshot_path: snapshot_path.into(),
        sequence_path: sequence_path.into(),
        snapshot_lock: Mutex::new(()),
        connections: Arc::new(AtomicU64::new(0)),
        waiters: Arc::new(AtomicU64::new(0)),
        confirmations: Arc::new(AtomicU64::new(0)),
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

/// Аудит 27.09 (S3): раундовый seed выводится криптографической
/// функцией из секрета арены, номера партии и раунда. Время в выводе
/// не участвует.
fn round_seed(state: &AppState, game_id: u64, round: u8) -> u64 {
    let mut h = Sha256::new();
    h.update(state.master_seed.load(Ordering::Relaxed).to_le_bytes());
    h.update(game_id.to_le_bytes());
    h.update(round.to_le_bytes());
    let d = h.finalize();
    u64::from_le_bytes(d[..8].try_into().unwrap())
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
    // Аудит 27.09 (S2): журнал фаз ограничен, старые записи вытесняются
    if entry.phase_log.len() > MAX_PHASE_LOG {
        let excess = entry.phase_log.len() - MAX_PHASE_LOG;
        entry.phase_log.drain(0..excess);
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SettlementFailure { Missing, Rules, Storage }

fn settle_and_record_locked(state: &AppState, game_id: u64) -> Result<(), SettlementFailure> {
    let rec: String;
    let backup: GameEntry;
    let mut replay = HashMap::new();
    let completed_before = state.completed.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let replay_before = state.completed_replay.lock().unwrap_or_else(|e| e.into_inner()).clone();
    {
        let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = games.get_mut(&game_id) {
            backup = entry.clone();
            let (ranks, payouts, rake, bank, breakdown) = match runner::try_settle(&entry.sim, entry.entry_fee, false) {
                Ok(plan) => plan,
                Err(error) => {
                    eprintln!("[ERROR] game {game_id} settlement: {error}");
                    entry.settlement_error = Some(error);
                    drop(games);
                    if let Err(e) = save_snapshot_locked(state) {
                        eprintln!("[ERROR] snapshot save after settlement error: {e}");
                        state.games.lock().unwrap_or_else(|e| e.into_inner())
                            .insert(game_id, backup);
                        return Err(SettlementFailure::Storage);
                    }
                    return Err(SettlementFailure::Rules);
                }
            };
            for agent in &entry.agents {
                if let (Some(id), Some(expires_at)) = (&agent.agent_record_id, agent.session_expires_at) {
                    replay.insert(id.clone(), CompletedSession {
                        token_hash: agent.token.clone(), expires_at,
                        op_state: entry.op_state.get(id).cloned().unwrap_or_default(),
                    });
                }
            }
            let agents: Vec<serde_json::Value> = entry
                .agents
                .iter()
                .map(|a| {
                    serde_json::json!({
                        "name": a.name,
                        "agent_id": a.agent_id,
                        "agent_record_id": a.agent_record_id,
                        "character_id": a.character_id,
                        "owner_id": a.owner_id,
                        "model": a.model,
                        "registration": a.registration,
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
                        "seq": a["seq"],
                        "event_id": format!("{}:{}:{}", game_id, entry.party_no, a["seq"]),
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
                "execution_mode": "http_simulated",
                "registration_mode": registration_mode(entry),
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
                // кастдев 02.09: прозрачный сеттл — откуда каждый alashi
                // выплаты: доля ранга, рента лицензии, завод
                "payout_breakdown": breakdown,
                "phases": entry.phase_log,
                "actions": entry.action_log,
                "events": events,
            });
            rec = v.to_string();
        } else { return Err(SettlementFailure::Missing); }
        games.remove(&game_id);
        // Removal and insertion must be one snapshot-visible change.
        // Аудит 27.09 (S2): архив завершённых партий ограничен.
        let mut completed = state.completed.lock().unwrap_or_else(|e| e.into_inner());
        completed.push(rec);
        while completed.len() > MAX_COMPLETED_GAMES {
            completed.remove(0);
        }
        let retained_ids: std::collections::HashSet<u64> = completed.iter().filter_map(|line|
            serde_json::from_str::<serde_json::Value>(line).ok()
                .and_then(|v| v["game_id"].as_u64())).collect();
        let mut archive = state.completed_replay.lock().unwrap_or_else(|e| e.into_inner());
        if !replay.is_empty() { archive.insert(game_id, replay); }
        archive.retain(|id, _| retained_ids.contains(id));
        if archive.len() > MAX_COMPLETED_REPLAY_GAMES {
            for line in completed.iter() {
                if archive.len() <= MAX_COMPLETED_REPLAY_GAMES { break; }
                if let Some(id) = serde_json::from_str::<serde_json::Value>(line).ok()
                    .and_then(|v| v["game_id"].as_u64()) {
                    archive.remove(&id);
                }
            }
        }
    }
    // Аудит 27.09 (S4): сеттл подтверждается устойчивой записью; при сбое
    // партия возвращается в активные и не теряется
    if let Err(e) = save_snapshot_locked(state) {
        eprintln!("[ERROR] snapshot save after settle: {e}");
        state.games.lock().unwrap_or_else(|e| e.into_inner()).insert(game_id, backup);
        *state.completed.lock().unwrap_or_else(|e| e.into_inner()) = completed_before;
        *state.completed_replay.lock().unwrap_or_else(|e| e.into_inner()) = replay_before;
        return Err(SettlementFailure::Storage);
    }
    Ok(())
}

/// Один тик кранка: двигает все партии, чьё время фазы вышло.
pub fn crank_once(state: &AppState) {
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    let t = now();
    let mut to_settle: Vec<u64> = Vec::new();
    let mut to_expire: Vec<u64> = Vec::new();
    let mut changed = false;
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let before = games.clone();
    for (&gid, entry) in games.iter_mut() {
        let seed = round_seed(state, gid, entry.sim.game.round);
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
    if !to_expire.is_empty() {
        changed = true;
        let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        for gid in to_expire {
            games.remove(&gid);
        }
    }
    if changed {
        if let Err(e) = save_snapshot_locked(state) {
            eprintln!("[ERROR] snapshot save after crank: {e}");
            *state.games.lock().unwrap_or_else(|e| e.into_inner()) = before;
            return;
        }
    }
    for gid in to_settle {
        if let Err(e) = settle_and_record_locked(state, gid) {
            eprintln!("[ERROR] game {gid} settlement after crank: {e:?}");
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

fn registration_mode(entry: &GameEntry) -> &'static str {
    let v2 = entry.agents.iter().any(|a| a.agent_record_id.is_some());
    let v1 = entry.agents.iter().any(|a| a.agent_record_id.is_none());
    match (v1, v2) {
        (true, true) => "mixed_devnet_registration",
        (false, true) => "devnet_agent_lifecycle_v2",
        _ => "devnet_agent_start_v1",
    }
}

fn state_json(game_id: u64, entry: &GameEntry) -> serde_json::Value {
    let g = &entry.sim.game;
    let stamp = g.stamp();
    let price_now = eff_price(g.sold_this_round, g.active_price_shift, g.active_boom);
    let recent_actions = &entry.action_log[entry.action_log.len().saturating_sub(RECENT_ACTIONS_LIMIT)..];
    serde_json::json!({
        "game_id": game_id,
        "execution_mode": "http_simulated",
        "registration_mode": registration_mode(entry),
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
        // Bounds describe the returned window, not the larger retained log.
        // Missing events: last_seen_seq + 1 < first_seq. Never infer loss on first load.
        "recent_actions_range": {
            "first_seq": recent_actions.first().and_then(|a| a["seq"].as_u64()),
            "last_seq": recent_actions.last().and_then(|a| a["seq"].as_u64()),
            "retained_first_seq": entry.action_log.first().and_then(|a| a["seq"].as_u64()),
            "limit": RECENT_ACTIONS_LIMIT,
        },
        "recent_actions": recent_actions.iter()
            .map(|a| serde_json::json!({
                "seq": a["seq"],
                "event_id": format!("{}:{}:{}", game_id, entry.party_no, a["seq"]),
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
                "agent_record_id": entry.agents.iter().find(|a| a.faction_idx == i).and_then(|a| a.agent_record_id.clone()),
                "character_id": entry.agents.iter().find(|a| a.faction_idx == i).map(|a| a.character_id.clone()),
                "registration": entry.agents.iter().find(|a| a.faction_idx == i).and_then(|a| a.registration.clone()),
                "cash": f.cash,
                // П5: кэш уже после всех эскроу/списаний — предел ставки
                // и покупок считается без повторного запроса (кейс Aitore:
                // бид 11M при живых 7M после инспекта)
                "cash_available": f.cash,
                // П5: куплен ли взгляд на доход лицензии в этой партии
                "insider": entry.insiders.contains(&i),
                "goods": f.goods,
                "influence": f.influence,
                "acted_stamp": f.acted_stamp,
                "vote_weight": runner::vote_weight(g, f),
                "acted": f.alive && f.acted_stamp == stamp,
                "voted": f.alive && f.voted_stamp == stamp,
                "is_president": f.alive && f.wallet == g.president,
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
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    h_new_game_locked(state, body, false)
}

// Caller holds snapshot_lock so matchmaking can select or create atomically.
fn h_new_game_locked(state: &AppState, body: &serde_json::Value, managed_match: bool) -> serde_json::Value {
    // Аудит 27.09 (S2): неизвестные поля отклоняются, а не игнорируются
    if let Some(obj) = body.as_object() {
        for key in obj.keys() {
            if !matches!(
                key.as_str(),
                "entry_fee" | "phase_duration" | "grace_s" | "vote_weight_mode" | "epoch"
                    | "lobby_duration" | "label"
            ) {
                return err_json("bad_params", &format!("неизвестное поле: {key}"));
            }
        }
    }
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
    // Аудит 27.09 (S2): предел одновременных активных партий
    if games.len() >= MAX_ACTIVE_GAMES {
        return err_json(
            "arena_full",
            &format!("достигнут предел активных партий ({MAX_ACTIVE_GAMES})"),
        );
    }
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
        op_state: HashMap::new(),
        phase_log: vec![],
        insiders: Default::default(),
        party_no,
        label,
        managed_match,
        settlement_error: None,
    };
    let v = state_json(game_id, &entry);
    games.insert(game_id, entry);
    drop(games);
    // Аудит 27.09 (S4): создание подтверждается только устойчивой записью.
    // party_no уже расходуется в файле последовательности: при сбое диска
    // номер пропускается, партия не создаётся наполовину.
    if let Err(e) = save_snapshot_locked(state) {
        eprintln!("[ERROR] snapshot save after create: {e}");
        state
            .games
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&game_id);
        return err_json("storage_failed", "партия не создана: не удалось записать состояние на диск; повтори запрос");
    }
    serde_json::json!({"ok": true, "game_id": game_id, "state": v})
}

/// Registered identities use the existing secret-derived character ID.
/// A public signature cannot create the same identity without its private secret.
fn registration_identity(body: &serde_json::Value) -> Result<(String, String), &'static str> {
    if body.get("owner_key").is_some() { return Err("registration_owner_key_forbidden"); }
    let name = body["name"].as_str().ok_or("registration_identity_required")?;
    let model = body["model"].as_str().ok_or("registration_identity_required")?;
    let prompt = body["prompt"].as_str().ok_or("registration_identity_required")?;
    let secret = body["recovery_secret"].as_str().filter(|s| recovery_hash(s).is_some())
        .ok_or("registration_recovery_secret_required")?;
    if name.len() > MAX_NAME { return Err("name_too_long"); }
    Ok((character_id_of(&owner_id_of(secret), name), agent_id_of(model, prompt)))
}

fn registration_error(code: &str) -> serde_json::Value {
    err_json(code, "регистрация devnet не подтверждена; сохрани прежнюю подпись и повтори проверку без новой транзакции")
}

fn lifecycle_response(id: &str, record: &RegisteredAgent) -> serde_json::Value {
    serde_json::json!({
        "ok": true, "mode": "agent_lifecycle_v2", "network": "devnet",
        "wallet": record.wallet, "owner_id": record.owner_id,
        "agent_record_id": id, "character_id": record.character_id,
        "memo": registration::lifecycle_memo(&record.owner_id, id, &record.character_id, &record.challenge),
        "memo_program_id": registration::MEMO_PROGRAM_ID,
        "registration": record.receipt,
    })
}

// The proposal key is written once before the first stateless Memo is returned.
// Call only while holding snapshot_lock; losing the key would invalidate a signed Memo.
fn proposal_key_locked(state: &AppState) -> Result<String, &'static str> {
    let mut key = state.proposal_key.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(existing) = key.as_ref() { return Ok(existing.clone()); }
    let generated = random_hex().map_err(|_| "entropy_unavailable")?;
    *key = Some(generated.clone());
    drop(key);
    if let Err(e) = save_snapshot_locked(state) {
        eprintln!("[ERROR] snapshot save after proposal key generation: {e}");
        *state.proposal_key.lock().unwrap_or_else(|e| e.into_inner()) = None;
        return Err("storage_failed");
    }
    Ok(generated)
}

fn lifecycle_challenge(key_hex: &str, wallet: &str, owner_id: &str,
    id: &str, character_id: &str) -> String {
    let key = hex_dec(key_hex).expect("validated lifecycle proposal key");
    let mut pad = [0x36u8; 64];
    for (dst, src) in pad.iter_mut().zip(key.iter()) { *dst ^= src; }
    let mut inner = Sha256::new();
    inner.update(pad);
    for field in ["alashi-lifecycle-proposal-v2:devnet:alashi.network", wallet,
        owner_id, id, character_id] {
        inner.update((field.len() as u64).to_le_bytes());
        inner.update(field.as_bytes());
    }
    let inner_hash = inner.finalize();
    pad.fill(0x5c);
    for (dst, src) in pad.iter_mut().zip(key.iter()) { *dst ^= src; }
    let mut outer = Sha256::new();
    outer.update(pad);
    outer.update(inner_hash);
    hex_enc(&outer.finalize())
}

fn h_registration_v2(state: &AppState, body: &serde_json::Value) -> serde_json::Value {
    if !body.as_object().is_some_and(|o| o.keys().all(|k|
        matches!(k.as_str(), "agent_record_id" | "wallet" | "recovery_secret"))) {
        return err_json("bad_params", "неизвестное поле регистрации v2");
    }
    let id = body["agent_record_id"].as_str().unwrap_or("");
    let wallet = body["wallet"].as_str().unwrap_or("");
    let secret = body["recovery_secret"].as_str().unwrap_or("");
    if !hex32(id) { return err_json("bad_agent_record_id", "agent_record_id: 64 lowercase hex"); }
    if let Err(code) = registration::validate_wallet(wallet) { return registration_error(code); }
    let Some(hash) = recovery_hash(secret) else { return err_json("bad_recovery_secret", "recovery_secret: 64 hex"); };
    if !hex32(secret) { return err_json("bad_recovery_secret", "recovery_secret: 64 lowercase hex"); }
    let owner_id = owner_id_of(secret);
    let character_id = character_id_v2(&owner_id, id);
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    let records = state.registrations.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(record) = records.get(id) {
        if record.wallet != wallet || !secret_matches(&record.recovery_hash, &hash) {
            return err_json("registration_conflict", "agent_record_id занят другим wallet/secret");
        }
        return lifecycle_response(id, record);
    }
    if records.len() >= MAX_REGISTERED_AGENTS {
        return err_json("registry_full", "лимит зарегистрированных агентов достигнут");
    }
    drop(records);
    let key = match proposal_key_locked(state) {
        Ok(value) => value,
        Err(code) => return err_json(code, "предложение регистрации недоступно"),
    };
    let challenge = lifecycle_challenge(&key, wallet, &owner_id, id, &character_id);
    let record = RegisteredAgent {
        wallet: wallet.to_string(), owner_id, character_id,
        recovery_hash: hash, challenge, receipt: None, created_at: now(),
    };
    lifecycle_response(id, &record)
}

fn h_confirm_v2(state: &AppState, body: &serde_json::Value) -> serde_json::Value {
    h_confirm_v2_with_verifier(state, body, registration::verify)
}

fn h_confirm_v2_with_verifier(
    state: &AppState, body: &serde_json::Value,
    verify: impl FnOnce(&Proof, &str) -> Result<Receipt, &'static str>,
) -> serde_json::Value {
    if !body.as_object().is_some_and(|o| o.keys().all(|k|
        matches!(k.as_str(), "agent_record_id" | "wallet" | "recovery_secret" | "signature"))) {
        return err_json("bad_params", "неизвестное поле подтверждения v2");
    }
    let id = body["agent_record_id"].as_str().unwrap_or("");
    let secret = body["recovery_secret"].as_str().unwrap_or("");
    if !hex32(id) || !hex32(secret) { return err_json("bad_params", "agent_record_id/recovery_secret: 64 lowercase hex"); }
    let hash = recovery_hash(secret).expect("validated recovery secret");
    let proof = match Proof::parse(&serde_json::json!({
        "wallet": body["wallet"], "signature": body["signature"]
    })) {
        Ok(value) => value, Err(code) => return registration_error(code),
    };
    let candidate = {
        // A cached receipt is visible only after its snapshot transaction commits.
        // The verifying RPC below runs after this lock is released.
        let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
        let records = state.registrations.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(record) = records.get(id) {
            if record.wallet != proof.wallet || !secret_matches(&hash, &record.recovery_hash) {
                return err_json("registration_conflict", "wallet или recovery_secret не совпадает");
            }
            if let Some(receipt) = &record.receipt {
                return if receipt.signature == proof.signature {
                    serde_json::json!({"ok":true,"agent_record_id":id,"registration":receipt})
                } else {
                    err_json("registration_conflict", "для агента уже сохранён другой receipt")
                };
            }
            record.clone()
        } else {
            if records.len() >= MAX_REGISTERED_AGENTS {
                return err_json("registry_full", "лимит зарегистрированных агентов достигнут");
            }
            drop(records);
            let Some(key) = state.proposal_key.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
                return err_json("unknown_agent", "предложение регистрации не найдено");
            };
            let owner_id = owner_id_of(secret);
            let character_id = character_id_v2(&owner_id, id);
            let challenge = lifecycle_challenge(&key, &proof.wallet, &owner_id, id, &character_id);
            RegisteredAgent { wallet: proof.wallet.clone(), owner_id, character_id,
                recovery_hash: hash.clone(), challenge, receipt: None, created_at: now() }
        }
    };
    let memo = registration::lifecycle_memo(&candidate.owner_id, id, &candidate.character_id, &candidate.challenge);
    let Some(permit) = Permit::acquire(&state.confirmations, MAX_CONFIRM_VERIFIERS) else {
        return err_json("registration_busy", "проверка регистрации занята; повтори тот же signature позже");
    };
    let verification = verify(&proof, &memo);
    drop(permit);
    let mut receipt = match verification {
        Ok(value) => value, Err(code) => return registration_error(code),
    };
    receipt.mode = "agent_lifecycle_v2".into();
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    let mut records = state.registrations.lock().unwrap_or_else(|e| e.into_inner());
    let before = records.get(id).cloned();
    if let Some(record) = &before {
        if record.wallet != candidate.wallet || record.challenge != candidate.challenge ||
            !secret_matches(&record.recovery_hash, &candidate.recovery_hash) {
            return err_json("registration_conflict", "предложение регистрации изменилось");
        }
        if let Some(existing) = &record.receipt {
            return if existing.signature == proof.signature {
                serde_json::json!({"ok":true,"agent_record_id":id,"registration":existing})
            } else {
                err_json("registration_conflict", "для агента уже сохранён другой receipt")
            };
        }
    } else if records.len() >= MAX_REGISTERED_AGENTS {
        return err_json("registry_full", "лимит зарегистрированных агентов достигнут");
    }
    let mut confirmed = before.clone().unwrap_or(candidate);
    confirmed.receipt = Some(receipt.clone());
    records.insert(id.to_string(), confirmed);
    drop(records);
    if let Err(e) = save_snapshot_locked(state) {
        eprintln!("[ERROR] snapshot save after lifecycle confirmation: {e}");
        let mut records = state.registrations.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(previous) = before { records.insert(id.to_string(), previous); }
        else { records.remove(id); }
        return err_json("storage_failed", "подтверждение регистрации не сохранено; повтори с той же подписью");
    }
    serde_json::json!({"ok":true,"agent_record_id":id,"registration":receipt})
}

fn close_ready_managed_lobby(entry: &mut GameEntry) {
    if entry.managed_match && entry.sim.game.phase == Phase::Lobby
        && entry.sim.game.faction_count == MIN_FACTIONS {
        entry.sim.game.phase_ends_at = entry.sim.game.phase_ends_at
            .min(now().saturating_add(MATCH_READY_CLOSE_S));
    }
}

fn h_join_v2(state: &AppState, game_id: u64, body: &serde_json::Value) -> serde_json::Value {
    if !body.as_object().is_some_and(|o| o.keys().all(|k|
        matches!(k.as_str(), "agent_record_id" | "recovery_secret" | "name" | "model" | "strategy_hash" | "recover"))) {
        return err_json("bad_params", "неизвестное поле join v2");
    }
    let id = body["agent_record_id"].as_str().unwrap_or("");
    let secret = body["recovery_secret"].as_str().unwrap_or("");
    let name = body["name"].as_str().unwrap_or("");
    let model = body["model"].as_str().unwrap_or("");
    let strategy_hash = body["strategy_hash"].as_str().unwrap_or("");
    let recover = body["recover"] == true;
    if !hex32(id) || !hex32(secret) || !hex32(strategy_hash) || name.is_empty() || name.len() > MAX_NAME || model.len() > 128 {
        return err_json("bad_params", "неверные поля join v2");
    }
    let token = match random_hex() {
        Ok(token) => token, Err(_) => return err_json("entropy_unavailable", "не удалось создать token"),
    };
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    let record = {
        let records = state.registrations.lock().unwrap_or_else(|e| e.into_inner());
        let Some(record) = records.get(id) else { return err_json("unknown_agent", "агент не зарегистрирован"); };
        if !recovery_hash(secret).is_some_and(|hash| secret_matches(&hash, &record.recovery_hash)) {
            return err_json("bad_recovery_secret", "неверный секрет восстановления");
        }
        if record.receipt.is_none() { return err_json("registration_required", "регистрация не подтверждена"); }
        record.clone()
    };
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = games.get_mut(&game_id) else { return err_json("unknown_game", "партия не найдена"); };
    let backup = entry.clone();
    let expires = now().saturating_add(SESSION_LIFETIME_S);
    let response = if let Some(i) = entry.agents.iter().position(|a| a.agent_record_id.as_deref() == Some(id)) {
        if !recover { return err_json("already_joined", "агент уже в партии; используй recover:true"); }
        let agent = &mut entry.agents[i];
        agent.token = sha256_hex(&token);
        agent.session_expires_at = Some(expires);
        agent.agent_id = strategy_hash.to_string();
        agent.model = model.to_string();
        let faction_idx = agent.faction_idx;
        let agent_id = agent.agent_id.clone();
        let view = state_json(game_id, entry);
        serde_json::json!({"ok":true,"recovered":true,"game_id":game_id,"party_no":entry.party_no,
            "agent_record_id":id,"agent_id":agent_id,"character_id":record.character_id,
            "owner_id":record.owner_id,"token":token,"session_expires_at":expires,
            "faction_idx":faction_idx,"registration":record.receipt,"state":view})
    } else {
        if recover { return err_json("unknown_agent", "агент ещё не участвовал в партии"); }
        if entry.agents.len() >= MAX_FACTIONS as usize { return err_json("game_full", "мест нет"); }
        let mut h = Sha256::new();
        h.update(record.character_id.as_bytes());
        h.update(&game_id.to_le_bytes());
        let wallet = Pubkey::new_from_array(h.finalize().into());
        if let Err(e) = entry.sim.join(wallet, name) { return err_json("join_failed", &format!("{e:?}")); }
        let faction_idx = entry.sim.factions.len() - 1;
        entry.wallets.push(wallet);
        entry.agents.push(AgentRec {
            name:name.to_string(), agent_id:strategy_hash.to_string(),
            character_id:record.character_id.clone(), owner_id:record.owner_id.clone(),
            agent_record_id:Some(id.to_string()), token:sha256_hex(&token),
            session_expires_at:Some(expires), recovery_hash:None,
            registration:record.receipt.clone(), model:model.to_string(), faction_idx,
        });
        close_ready_managed_lobby(entry);
        let view = state_json(game_id, entry);
        serde_json::json!({"ok":true,"game_id":game_id,"party_no":entry.party_no,
            "agent_record_id":id,"agent_id":strategy_hash,"character_id":record.character_id,
            "owner_id":record.owner_id,"token":token,"session_expires_at":expires,
            "faction_idx":faction_idx,"registration":record.receipt,"state":view})
    };
    drop(games);
    if let Err(e) = save_snapshot_locked(state) {
        eprintln!("[ERROR] snapshot save after join v2: {e}");
        state.games.lock().unwrap_or_else(|e| e.into_inner()).insert(game_id, backup);
        return err_json("storage_failed", "join v2 не сохранён");
    }
    response
}

fn h_registration(state: &AppState, game_id: u64, body: &serde_json::Value) -> serde_json::Value {
    if state.require_platform_v2 { return registration_error("platform_registration_required"); }
    if !body.as_object().is_some_and(|obj| obj.keys().all(|k|
        matches!(k.as_str(), "name" | "model" | "prompt" | "recovery_secret" | "wallet"))) {
        return err_json("bad_params", "неизвестные поля предложения регистрации");
    }
    let (character_id, agent_id) = match registration_identity(body) {
        Ok(ids) => ids, Err(code) => return registration_error(code),
    };
    let wallet = body["wallet"].as_str().unwrap_or("");
    if let Err(code) = registration::validate_wallet(wallet) { return registration_error(code); }
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = games.get(&game_id) else { return err_json("unknown_game", "партия не найдена"); };
    if entry.agents.iter().any(|a| a.character_id == character_id) {
        return err_json("already_joined", "используй recover: true с прежним recovery_secret; новая транзакция не нужна");
    }
    if entry.sim.game.phase != Phase::Lobby || now() >= entry.sim.game.phase_ends_at {
        return err_json("registration_lobby_closed", "лобби закрыто; не подписывай новую транзакцию");
    }
    if entry.agents.len() >= MAX_FACTIONS as usize { return err_json("game_full", "мест нет"); }
    serde_json::json!({"ok":true, "mode":"agent_start_v1", "network":"devnet",
        "required":state.require_devnet_registration, "memo_program_id":registration::MEMO_PROGRAM_ID,
        "memo":registration::memo(game_id,entry.party_no,&character_id,&agent_id),
        "game_id":game_id, "party_no":entry.party_no, "character_id":character_id,
        "agent_id":agent_id, "wallet":wallet})
}

fn h_join(state: &AppState, game_id: u64, body: &serde_json::Value) -> serde_json::Value {
    h_join_with_verifier(state, game_id, body, registration::verify)
}

fn h_join_with_verifier(
    state: &AppState, game_id: u64, body: &serde_json::Value,
    verify: impl FnOnce(&Proof, &str) -> Result<Receipt, &'static str>,
) -> serde_json::Value {
    if body.get("agent_record_id").is_some() { return h_join_v2(state, game_id, body); }
    if state.require_platform_v2 { return registration_error("platform_registration_required"); }
    if !body.as_object().is_some_and(|obj| obj.keys().all(|key| matches!(
        key.as_str(), "name" | "model" | "prompt" | "recover" | "recovery_secret" | "owner_key" | "token" | "registration"
    ))) { return err_json("bad_params", "неизвестное поле join"); }
    let proof = match body.get("registration") {
        Some(value) => match Proof::parse(value) {
            Ok(proof) => Some(proof), Err(code) => return registration_error(code),
        },
        None => None,
    };
    let recovering = body["recover"] == true;
    if proof.is_some() && body.get("owner_key").is_some() {
        return registration_error("registration_owner_key_forbidden");
    }
    let mut verified = None;
    let mut checked_party = None;
    if !recovering {
        if let Some(proof) = &proof {
            let (character_id, agent_id) = match registration_identity(body) {
                Ok(ids) => ids, Err(code) => return registration_error(code),
            };
            // No network call while holding snapshot_lock or games.
            let party_no = {
                let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
                let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
                let Some(entry) = games.get(&game_id) else { return err_json("unknown_game", "партия не найдена"); };
                entry.party_no
            };
            let expected = registration::memo(game_id, party_no, &character_id, &agent_id);
            verified = match verify(proof, &expected) {
                Ok(receipt) => Some(receipt), Err(code) => return registration_error(code),
            };
            checked_party = Some(party_no);
        } else if state.require_devnet_registration {
            return registration_error("registration_required");
        }
    }
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    let backup = {
        let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        let entry = games.get(&game_id);
        if checked_party.is_some() && entry.map(|e| e.party_no) != checked_party {
            return registration_error("registration_game_changed");
        }
        entry.cloned()
    };
    let response = h_join_inner(state, game_id, body, proof.as_ref(), verified);
    if response["ok"] != true { return response; }
    if let Err(e) = save_snapshot_locked(state) {
        eprintln!("[ERROR] snapshot save after join: {e}");
        if let Some(entry) = backup {
            state.games.lock().unwrap_or_else(|e| e.into_inner()).insert(game_id, entry);
        }
        return err_json("storage_failed", "изменение не принято: не удалось записать состояние на диск; повтори запрос");
    }
    response
}

fn h_join_inner(state: &AppState, game_id: u64, body: &serde_json::Value, proof: Option<&Proof>, verified: Option<Receipt>) -> serde_json::Value {
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
    let token = match random_hex() {
        Ok(token) => token,
        Err(_) => return err_json("entropy_unavailable", "не удалось создать секрет сессии"),
    };
    let recover = body.get("recover").and_then(|v| v.as_bool()).unwrap_or(false);
    // Отчёт «Цукерберг/Muse» 27.09: якорь личности — секрет владельца
    // (явный owner_key или recovery_secret, переданный клиентом).
    // Клиент без секрета получает legacy-личность, равную отпечатку
    // стратегии: старые клиенты и прежние снимки продолжают работать.
    let owner_key = match body.get("owner_key") {
        Some(v) => match v.as_str().filter(|s| recovery_hash(s).is_some()) {
            Some(k) => Some(k.to_string()),
            None => {
                return err_json("bad_params", "owner_key должен содержать 64 hex-символа из 32 случайных байтов")
            }
        },
        None => None,
    };
    let has_client_secret = body.get("recovery_secret").and_then(|v| v.as_str()).is_some();
    let recovery_secret = match body.get("recovery_secret") {
        Some(v) => match v.as_str().filter(|s| recovery_hash(s).is_some()) {
            Some(secret) => secret.to_string(),
            None => return err_json("bad_params", "recovery_secret должен содержать 64 hex-символа из 32 случайных байтов"),
        },
        None => match random_hex() {
            Ok(secret) => secret,
            Err(_) => return err_json("entropy_unavailable", "не удалось создать секрет восстановления"),
        },
    };
    let (owner_id, character_id) = if let Some(k) = &owner_key {
        let oid = owner_id_of(k);
        let cid = character_id_of(&oid, &name);
        (oid, cid)
    } else if has_client_secret {
        let oid = owner_id_of(&recovery_secret);
        let cid = character_id_of(&oid, &name);
        (oid, cid)
    } else {
        ("legacy".into(), agent_id.clone())
    };
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = games.get_mut(&game_id) else {
        return err_json("unknown_game", "партия не найдена или закрыта");
    };
    // Recovery is an existing-session operation, not a new join.
    // Отчёт 27.09: сессия ищется по персонажу; для legacy-сессий
    // (личность = отпечаток стратегии) сохранён поиск по стратегии.
    let found = entry
        .agents
        .iter()
        .position(|a| a.character_id == character_id)
        .or_else(|| {
            entry
                .agents
                .iter()
                .position(|a| a.character_id == a.agent_id && a.agent_id == agent_id)
        });
    if let Some(i) = found {
        if recover {
            let agent = &entry.agents[i];
            let supplied = body.get("recovery_secret").and_then(|v| v.as_str()).unwrap_or("");
            let valid = match &agent.recovery_hash {
                Some(expected) => recovery_hash(supplied).is_some_and(|hash| secret_matches(&hash, expected)),
                // A legacy session can enroll only with its current bearer token.
                None => body.get("token").and_then(|v| v.as_str())
                    .is_some_and(|current| secret_matches(current, &agent.token)),
            };
            if !valid { return err_json("bad_recovery_secret", "recover требует секрет восстановления; для старой сессии нужен действующий token"); }
            // Recovery never upgrades or overwrites the persisted receipt.
            if let Some(proof) = proof {
                if !agent.registration.as_ref().is_some_and(|r| r.wallet == proof.wallet && r.signature == proof.signature) {
                    return registration_error("registration_recovery_mismatch");
                }
            }
            if agent.registration.is_some() && body.get("owner_key").is_some() {
                return registration_error("registration_owner_key_forbidden");
            }
            let enrolled_secret = if agent.recovery_hash.is_none() {
                match random_hex() {
                    Ok(secret) => Some(secret),
                    Err(_) => return err_json("entropy_unavailable", "не удалось создать секрет восстановления"),
                }
            } else { None };
            if let Some(secret) = &enrolled_secret {
                entry.agents[i].recovery_hash = recovery_hash(secret);
            }
            let faction_idx = entry.agents[i].faction_idx;
            let stored_agent_id = entry.agents[i].agent_id.clone();
            let stored_owner_id = entry.agents[i].owner_id.clone();
            entry.agents[i].token = token.clone();
            let v = state_json(game_id, entry);
            return serde_json::json!({
                "ok": true, "recovered": true, "game_id": game_id,
                "agent_id": stored_agent_id, "character_id": character_id, "owner_id": stored_owner_id,
                "token": token, "recovery_secret": enrolled_secret, "faction_idx": faction_idx,
                "registration": entry.agents[i].registration,
                "warning": "токен перевыпущен; предыдущий отозван", "state": v
            });
        }
        return err_json("join_failed", "DuplicateWallet: персонаж уже в партии; для восстановления передай recover: true и recovery_secret");
    }
    if recover {
        return err_json("unknown_agent", "персонаж не участвовал в этой партии");
    }
    if entry.agents.len() >= MAX_FACTIONS as usize {
        return err_json("game_full", "мест нет");
    }
    let mut h = Sha256::new();
    h.update(character_id.as_bytes());
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
        character_id: character_id.clone(),
        owner_id: owner_id.clone(),
        token: token.clone(),
        recovery_hash: recovery_hash(&recovery_secret),
        agent_record_id: None,
        session_expires_at: None,
        registration: verified.clone(),
        model,
        faction_idx,
    });
    close_ready_managed_lobby(entry);
    eprintln!("[join] party {} faction {} character {} (strategy {})", entry.party_no, faction_idx, character_id, agent_id);
    let v = state_json(game_id, entry);
    serde_json::json!({"ok": true, "agent_id": agent_id, "character_id": character_id, "owner_id": owner_id,
        "token": token, "recovery_secret": recovery_secret, "faction_idx": faction_idx,
        "registration": verified,
        "warning": if placeholder_warn { Some("имя/model похожи на плейсхолдер из примера — подставь реальные значения; сохрани token и recovery_secret сразу; восстановление требует recovery_secret") } else { None },
        "state": v})
}

fn h_state(state: &AppState, game_id: u64) -> serde_json::Value {
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
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
            let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
            let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
            games.get(&game_id).map(|e| {
                (e.sim.game.round, phase_name(e.sim.game.phase))
            })
        };
        match snapshot {
            Some((r, p)) => {
                let changed = after_round.map(|ar| ar != r).unwrap_or(false)
                    || after_phase.as_deref().map(|ap| ap != p).unwrap_or(false);
                if changed {
                    let mut response = h_state(state, game_id);
                    response["changed"] = serde_json::json!(true);
                    return response;
                }
            }
            None => {
                // партии нет среди живых: либо finished, либо unknown
                return h_state(state, game_id);
            }
        }
        if now() >= deadline {
            let mut response = h_state(state, game_id);
            if response.get("state").is_some() {
                response["changed"] = serde_json::json!(false);
                response["timeout"] = serde_json::json!(true);
            }
            return response;
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

fn op_response(mut value: serde_json::Value, op_id: Option<u64>, consumed: bool) -> serde_json::Value {
    if let Some(id) = op_id {
        value["op_id"] = serde_json::json!(id);
        value["op_consumed"] = serde_json::json!(consumed);
    }
    value
}

fn action_request_hash(body: &serde_json::Value) -> String {
    let normalized = serde_json::json!({
        "action": body["action"].as_str().unwrap_or(""),
        "by": body["by"].as_str().unwrap_or("unknown"),
        "params": body.get("params").cloned().unwrap_or(serde_json::json!({})),
    });
    sha256_hex(&normalized.to_string())
}

fn completed_action_replay(state: &AppState, game_id: u64, body: &serde_json::Value) -> serde_json::Value {
    let op_id = body["op_id"].as_u64();
    let token = body["token"].as_str().unwrap_or("");
    let archive = state.completed_replay.lock().unwrap_or_else(|e| e.into_inner());
    let Some(sessions) = archive.get(&game_id) else {
        return op_response(err_json("unknown_game", "партия не найдена"), op_id, false);
    };
    let token_hash = sha256_hex(token);
    let Some(session) = sessions.values().find(|session| hex32(token) && now() < session.expires_at
        && secret_matches(&session.token_hash, &token_hash)) else {
        return op_response(err_json("bad_token", "токен не найден или истёк"), op_id, false);
    };
    let Some(id) = op_id.filter(|id| *id > 0) else {
        return err_json("op_id_required", "v2 action требует положительный op_id");
    };
    if let Some(saved) = session.op_state.recent.iter().find(|record| record.id == id) {
        return if secret_matches(&saved.request_hash, &action_request_hash(body)) {
            saved.response.clone()
        } else {
            op_response(err_json("op_conflict", "op_id уже использован для другого запроса"), Some(id), false)
        };
    }
    if id <= session.op_state.last {
        op_response(err_json("op_stale", "op_id старше сохранённого окна ответов"), Some(id), false)
    } else {
        op_response(err_json("game_finished", "партия уже завершена"), Some(id), false)
    }
}

fn h_act(state: &AppState, game_id: u64, body: &serde_json::Value) -> serde_json::Value {
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    let token = body["token"].as_str().unwrap_or("");
    let (v2_id, valid_token) = {
        let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        let Some(entry) = games.get(&game_id) else {
            return completed_action_replay(state, game_id, body);
        };
        let found = entry.agents.iter().find(|a| agent_token_matches(a, token));
        (
            found.and_then(|a| a.agent_record_id.clone()),
            found.is_some_and(|a| !state.require_platform_v2 || a.agent_record_id.is_some()),
        )
    };
    if !valid_token {
        return op_response(err_json("bad_token", "токен не найден или истёк"), body["op_id"].as_u64(), false);
    }
    let op_id = if v2_id.is_some() {
        match body["op_id"].as_u64().filter(|id| *id > 0) {
            Some(id) => Some(id),
            None => return err_json("op_id_required", "v2 action требует положительный op_id"),
        }
    } else {
        if body.get("op_id").is_some() { return err_json("bad_params", "op_id нужен только v2 сессии"); }
        None
    };
    let request_hash = v2_id.as_ref().map(|_| action_request_hash(body));
    if let (Some(id), Some(agent_id), Some(hash)) = (op_id, v2_id.as_ref(), request_hash.as_ref()) {
        let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        let entry = &games[&game_id];
        let ops = entry.op_state.get(agent_id).cloned().unwrap_or_default();
        if id <= ops.last {
            if let Some(saved) = ops.recent.iter().find(|r| r.id == id) {
                return if secret_matches(&saved.request_hash, hash) {
                    saved.response.clone()
                } else {
                    op_response(err_json("op_conflict", "op_id уже использован для другого запроса"), op_id, false)
                };
            }
            return op_response(err_json("op_stale", "op_id старше сохранённого окна ответов"), op_id, false);
        }
        if ops.last.checked_add(1) != Some(id) {
            return op_response(err_json("op_out_of_order", "op_id должен идти по порядку"), op_id, false);
        }
    }
    if let Some(obj) = body.as_object() {
        for key in obj.keys() {
            if !matches!(key.as_str(), "token" | "action" | "params" | "by" | "op_id") {
                return op_response(err_json("bad_params", &format!("неизвестное поле: {key}")), op_id, false);
            }
        }
    }
    let action = body.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let allowed: &[&str] = match action {
        "sell" | "sell_credit" | "buy" => &["units"],
        "produce" | "shuttle" | "donkey" | "inspect_license" | "accept_vote_offer"
        | "buy_hard" | "sell_hard" | "veto" => &[],
        "roof" => &["to", "tariff"],
        "customs" => &["tight"],
        "bid_license" => &["amount"],
        "offer_vote" => &["to", "price"],
        "barter_propose" => &["goods", "price", "to"],
        "barter_accept" => &["offer"],
        "bribe" => &["to", "amount"],
        "vote" => &["choice"],
        _ => return op_response(err_json("bad_action", "неизвестное действие"), op_id, false),
    };
    if v2_id.is_some() && body.get("params").is_some_and(|p| !p.is_object()) {
        return op_response(err_json("bad_params", "params должен быть объектом"), op_id, false);
    }
    if let Some(params) = body.get("params").and_then(|v| v.as_object()) {
        for key in params.keys() {
            if !allowed.contains(&key.as_str()) {
                return op_response(err_json("bad_params", &format!("неизвестное поле params: {key}")), op_id, false);
            }
        }
    }
    let backup = {
        let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        games.get(&game_id).cloned()
    };
    let mut response = h_act_inner(state, game_id, body);
    if response.get("action_log").is_none() {
        return op_response(response, op_id, false);
    }
    response = op_response(response, op_id, op_id.is_some());
    if let (Some(id), Some(agent_id), Some(hash)) = (op_id, v2_id, request_hash) {
        let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        let ops = games.get_mut(&game_id).unwrap().op_state.entry(agent_id).or_default();
        ops.last = id;
        ops.recent.push(OpRecord { id, request_hash: hash, response: response.clone() });
        if ops.recent.len() > MAX_RECENT_OPS { ops.recent.remove(0); }
    }
    if let Err(e) = save_snapshot_locked(state) {
        eprintln!("[ERROR] snapshot save after act: {e}");
        if let Some(entry) = backup {
            state.games.lock().unwrap_or_else(|e| e.into_inner()).insert(game_id, entry);
        }
        return op_response(err_json("storage_failed", "изменение не сохранено; повтори запрос"), op_id, false);
    }
    response
}

fn h_act_inner(state: &AppState, game_id: u64, body: &serde_json::Value) -> serde_json::Value {
    let token = body.get("token").and_then(|v| v.as_str()).unwrap_or("");
    let action = body.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let p = body.get("params").cloned().unwrap_or(serde_json::json!({}));
    // R8 (REVIEW_EXTERNAL): источник хода — самозаявлен клиентом,
    // отличает решение модели от жадного фоллбэка в /export
    let mut by = body.get("by").and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
    // Аудит 27.09 (S2): метка источника урезается до предела
    if by.chars().count() > MAX_BY_LEN {
        by = by.chars().take(MAX_BY_LEN).collect();
    }
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = games.get_mut(&game_id) else {
        return err_json("unknown_game", "партия не найдена или закрыта");
    };
    let Some(agent) = entry.agents.iter().find(|a| agent_token_matches(a, token)) else {
        return err_json("bad_token", "токен не найден");
    };
    let idx = agent.faction_idx;
    let small_field = match action {
        "sell" | "sell_credit" | "buy" => Some("units"),
        "barter_propose" => Some("goods"),
        _ => None,
    };
    if let Some(field) = small_field {
        if !p.get(field).and_then(|v| v.as_u64()).is_some_and(|v| v <= u16::MAX as u64) {
            return err_json("bad_params", "units/goods должны быть целым числом от 0 до 65535");
        }
    }
    // The last retained entry is the high-water mark: trimming never removes it.
    // Check before applying rules so exhaustion cannot mutate the game.
    let Some(seq) = entry.action_log.last().and_then(|a| a["seq"].as_u64()).unwrap_or(0).checked_add(1) else {
        return err_json("event_seq_exhausted", "закончились номера событий партии");
    };
    let log: ActionLog = match action {
        "sell" => {
            let units = p.get("units").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            runner::apply_market(&mut entry.sim, idx, &MarketAction::Sell(units))
        }
        "sell_credit" => {
            let units = p.get("units").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            runner::apply_market(&mut entry.sim, idx, &MarketAction::SellCredit(units))
        }
        "buy" => {
            let units = p.get("units").and_then(|v| v.as_u64()).unwrap_or(0) as u16;
            runner::apply_market(&mut entry.sim, idx, &MarketAction::Buy(units))
        }
        "produce" => runner::apply_action(&mut entry.sim, idx, &ActionAction::Produce),
        "shuttle" => runner::apply_action(&mut entry.sim, idx, &ActionAction::Shuttle),
        "roof" => {
            let to = p.get("to").and_then(|v| v.as_u64()).unwrap_or(usize::MAX as u64) as usize;
            let tariff = match p.get("tariff").and_then(|v| v.as_str()) {
                Some("black") => alashi_rules::constants::ROOF_BLACK,
                Some("red") => alashi_rules::constants::ROOF_RED,
                _ => 0,
            };
            runner::apply_action(&mut entry.sim, idx, &ActionAction::Roof { to, tariff })
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
            let sim = &mut entry.sim;
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
        "donkey" => runner::apply_action(&mut entry.sim, idx, &ActionAction::Donkey),
        "bribe" => {
            let to = p.get("to").and_then(|v| v.as_u64()).unwrap_or(usize::MAX as u64) as usize;
            let amount = p.get("amount").and_then(|v| v.as_u64()).unwrap_or(0);
            runner::apply_action(&mut entry.sim, idx, &ActionAction::Bribe { to, amount })
        }
        "vote" => {
            let choice = match p.get("choice").and_then(|v| v.as_str()).unwrap_or("") {
                "yes" => VoteChoice::Yes,
                "no" => VoteChoice::No,
                "abstain" => VoteChoice::Abstain,
                _ => return err_json("bad_choice", "choice: yes|no|abstain"),
            };
            let w = entry.wallets[idx];
            runner::apply_law(&mut entry.sim, idx, &LawAction::Vote(choice), &w)
        }
        "veto" => {
            let w = entry.wallets[idx];
            runner::apply_law(&mut entry.sim, idx, &LawAction::Veto, &w)
        }
        _ => return err_json("bad_action", "sell|sell_credit|buy|buy_hard|sell_hard|produce|shuttle|roof|customs|bid_license|inspect_license|sell_vote|barter_propose|barter_accept|offer_vote|accept_vote_offer|donkey|bribe|vote|veto"),
    };
    let ok = log.ok;
    let err = log.err.clone();
    // протокол хода: фаза, раунд, актёр, действие, исход (для /export).
    // Аудит 27.09 (S2): у отклонённых действий параметры не сохраняются
    entry.action_log.push(serde_json::json!({
        "seq": seq,
        "round": entry.sim.game.round,
        "phase": phase_name(entry.sim.game.phase),
        "actor": idx,
        "action": action,
        "params": if ok { p } else { serde_json::Value::Null },
        "by": by,
        "ok": ok,
        "err": err,
        "cash_after": log.cash_after,
        "goods_after": log.goods_after,
        "ts": now(),
    }));
    if entry.action_log.len() > MAX_ACTION_LOG {
        let excess = entry.action_log.len() - MAX_ACTION_LOG;
        entry.action_log.drain(0..excess);
    }
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
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    let t = now();
    let mut games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let Some(entry) = games.get_mut(&game_id) else {
        return err_json("unknown_game", "партия не найдена или закрыта");
    };
    let seed = round_seed(state, game_id, entry.sim.game.round);
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
    // Аудит 27.09 (S4): копия для отката при сбое записи
    let backup = entry.clone();
    match entry.sim.advance(t, seed) {
        Ok(_) => {
            entry.sim.game.phase_ends_at = t.saturating_add(entry.sim.game.phase_duration);
            record_phase_close(entry, closing);
            let finished = entry.sim.game.phase == Phase::Finished;
            let v = state_json(game_id, entry);
            drop(games);
            if finished {
                match settle_and_record_locked(state, game_id) {
                    Ok(()) => {}
                    Err(SettlementFailure::Storage) => {
                        state.games.lock().unwrap_or_else(|e| e.into_inner()).insert(game_id, backup);
                        return err_json("storage_failed", "изменение не принято: не удалось записать состояние на диск; повтори запрос");
                    }
                    Err(SettlementFailure::Rules | SettlementFailure::Missing) => {
                        return err_json("settlement_failed", "завершение партии не удалось; проверь состояние партии");
                    }
                }
            } else if let Err(e) = save_snapshot_locked(state) {
                eprintln!("[ERROR] snapshot save after advance: {e}");
                state
                    .games
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(game_id, backup);
                return err_json("storage_failed", "изменение не принято: не удалось записать состояние на диск; повтори запрос");
            }
            serde_json::json!({"ok": true, "finished": finished, "state": v})
        }
        Err(e) => serde_json::json!({"ok": false, "error": format!("{:?}", e), "state": state_json(game_id, entry)}),
    }
}

/// Кастдев №5: GET /slots?character_id=<hex> — во всех АКТИВНЫХ партиях
/// находит фракции этого персонажа (кошелёк детерминирован от
/// character_id+game_id). Отчёт 27.09: поиск по личности, не по версии
/// стратегии; ?agent_id= оставлен для legacy-сессий.
fn h_slots(state: &AppState, raw_path: &str) -> serde_json::Value {
    let q = raw_path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let getq = |k: &str| {
        q.split('&').find_map(|kv| {
            kv.split_once('=').filter(|(key, _)| *key == k).map(|(_, v)| v.to_string())
        })
    };
    let character_id = getq("character_id").or_else(|| getq("agent_id")).unwrap_or_default();
    if character_id.len() != 64 || !character_id.chars().all(|c| c.is_ascii_hexdigit()) {
        return err_json("bad_params", "нужен ?character_id= (64 hex, из ответа join)");
    }
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    let mut found = Vec::new();
    for (&gid, e) in games.iter() {
        let mut h = Sha256::new();
        h.update(character_id.as_bytes());
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
    serde_json::json!({"ok": true, "character_id": character_id, "active_slots": found})
}

fn h_agent_profile(state: &AppState, id: &str) -> serde_json::Value {
    if !hex32(id) { return err_json("bad_agent_record_id", "agent_record_id: 64 lowercase hex"); }
    let character_id = {
        let records = state.registrations.lock().unwrap_or_else(|e| e.into_inner());
        let Some(record) = records.get(id).filter(|record| record.receipt.is_some()) else {
            return err_json("unknown_agent", "агент не найден");
        };
        record.character_id.clone()
    };
    let slots = h_slots(state, &format!("/slots?character_id={character_id}"));
    serde_json::json!({"ok":true,"agent_record_id":id,"character_id":character_id,
        "registered":true,"active_slots":slots["active_slots"]})
}

fn match_result(game_id: u64, entry: &GameEntry, joined: bool) -> serde_json::Value {
    serde_json::json!({"ok":true,"game_id":game_id,"party_no":entry.party_no,
        "phase":phase_name(entry.sim.game.phase),"joined":joined,
        "waiting_for_players":entry.sim.game.phase == Phase::Lobby
            && entry.sim.game.faction_count < MIN_FACTIONS})
}

fn h_match(state: &AppState, body: &serde_json::Value) -> serde_json::Value {
    if !body.as_object().is_some_and(|o| o.keys().all(|key|
        matches!(key.as_str(), "agent_record_id" | "recovery_secret"))) {
        return err_json("bad_params", "неизвестное поле match");
    }
    let id = body["agent_record_id"].as_str().unwrap_or("");
    let secret = body["recovery_secret"].as_str().unwrap_or("");
    if !hex32(id) || !hex32(secret) { return err_json("bad_params", "agent_record_id/recovery_secret: 64 lowercase hex"); }
    let hash = recovery_hash(secret).expect("validated recovery secret");
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    let character_id = {
        let records = state.registrations.lock().unwrap_or_else(|e| e.into_inner());
        let Some(record) = records.get(id) else { return err_json("unknown_agent", "агент не зарегистрирован"); };
        if !secret_matches(&record.recovery_hash, &hash) {
            return err_json("bad_recovery_secret", "неверный секрет восстановления");
        }
        if record.receipt.is_none() { return err_json("registration_required", "регистрация не подтверждена"); }
        record.character_id.clone()
    };
    let t = now();
    {
        let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
        let active = |entry: &GameEntry| !matches!(entry.sim.game.phase, Phase::Finished | Phase::Aborted);
        if let Some((&gid, entry)) = games.iter().filter(|(_, entry)| active(entry)
            && entry.agents.iter().any(|agent| agent.character_id == character_id))
            .min_by_key(|(gid, _)| *gid) {
            return match_result(gid, entry, true);
        }
        if let Some((&gid, entry)) = games.iter().filter(|(_, entry)| {
            let min_time = if entry.managed_match && entry.sim.game.faction_count >= MIN_FACTIONS {
                MATCH_READY_MIN_JOIN_TIME_S
            } else { MATCH_MIN_JOIN_TIME_S };
            entry.sim.game.phase == Phase::Lobby
                && entry.sim.game.faction_count < MAX_FACTIONS
                && entry.sim.game.phase_ends_at.saturating_sub(t) >= min_time
        })
            .max_by_key(|(gid, entry)| (entry.sim.game.faction_count, std::cmp::Reverse(**gid))) {
            return match_result(gid, entry, false);
        }
        if games.values().any(|entry| entry.managed_match && entry.sim.game.phase == Phase::Lobby) {
            return err_json("match_wait", "текущее лобби закрывается; повтори поиск после смены фазы");
        }
    }
    let created = h_new_game_locked(state, &serde_json::json!({
        "entry_fee":10 * PESO,"phase_duration":30,"grace_s":DEFAULT_GRACE_S,
        "lobby_duration":MATCH_LOBBY_DURATION_S,"vote_weight_mode":VOTE_WEIGHT_LEGACY,
        "epoch":"classic","label":"Public Match",
    }), true);
    if created["ok"] != true { return created; }
    let gid = created["game_id"].as_u64().expect("created game id");
    let games = state.games.lock().unwrap_or_else(|e| e.into_inner());
    match_result(gid, &games[&gid], false)
}

/// Отчёт «Цукерберг/Muse» 27.09, раздел 8: история пары персонажей по
/// проверяемым событиям завершённых партий. Только факты журнала
/// действий, без интерпретации мотивов.
fn h_history(state: &AppState, raw_path: &str) -> serde_json::Value {
    let q = raw_path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let getq = |k: &str| {
        q.split('&').find_map(|kv| {
            kv.split_once('=').filter(|(key, _)| *key == k).map(|(_, v)| v.to_string())
        })
    };
    let me = getq("character_id").unwrap_or_default();
    let peer = getq("peer").unwrap_or_default();
    for id in [&me, &peer] {
        if id.len() != 64 || !id.chars().all(|c| c.is_ascii_hexdigit()) {
            return err_json("bad_params", "нужны ?character_id= и ?peer= (64 hex, из ответов join)");
        }
    }
    let completed = state.completed.lock().unwrap_or_else(|e| e.into_inner());
    let mut meetings: Vec<serde_json::Value> = Vec::new();
    for line in completed.iter() {
        let Ok(rec) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        let Some(agents) = rec["agents"].as_array() else { continue };
        let idx_of = |cid: &str| {
            agents.iter().position(|a| a["character_id"].as_str() == Some(cid))
        };
        let (Some(mi), Some(pi)) = (idx_of(&me), idx_of(&peer)) else { continue };
        let rank_of = |fi: usize| -> Option<u64> {
            rec["ranks"].as_array().and_then(|r| {
                r.iter().position(|x| x.as_u64() == Some(fi as u64)).map(|p| p as u64)
            })
        };
        let payout_of = |fi: usize| -> u64 {
            rec["payouts"].as_array().and_then(|p| p.get(fi)).and_then(|x| x.as_u64()).unwrap_or(0)
        };
        let (Some(my_rank), Some(peer_rank)) = (rank_of(mi), rank_of(pi)) else { continue };
        // прямые взаимодействия по журналу действий (только успешные ходы)
        let mut bribes_to_peer = 0u64;
        let mut bribes_from_peer = 0u64;
        let mut roofs_on_peer: Vec<&str> = Vec::new();
        let mut roofs_on_me: Vec<&str> = Vec::new();
        let mut vote_trades = 0u64;
        let mut pending_offer: Option<(usize, usize)> = None; // (offerer, target)
        let mut my_votes: std::collections::HashMap<u64, &str> = Default::default();
        let mut peer_votes: std::collections::HashMap<u64, &str> = Default::default();
        let mut my_vetoes = 0u64;
        let mut peer_vetoes = 0u64;
        if let Some(actions) = rec["actions"].as_array() {
            for a in actions.iter() {
                if a["ok"] != true { continue; }
                let actor = a["actor"].as_u64().unwrap_or(u64::MAX) as usize;
                let action = a["action"].as_str().unwrap_or("");
                let to = a["params"]["to"].as_u64().map(|v| v as usize);
                let amount = a["params"]["amount"].as_u64().unwrap_or(0);
                let round = a["round"].as_u64().unwrap_or(0);
                match action {
                    "bribe" => {
                        if actor == mi && to == Some(pi) { bribes_to_peer += amount; }
                        if actor == pi && to == Some(mi) { bribes_from_peer += amount; }
                    }
                    "roof" => {
                        let tariff = a["params"]["tariff"].as_str().unwrap_or("?");
                        if actor == mi && to == Some(pi) { roofs_on_peer.push(tariff); }
                        if actor == pi && to == Some(mi) { roofs_on_me.push(tariff); }
                    }
                    "offer_vote" => {
                        pending_offer = to.map(|t| (actor, t));
                    }
                    "accept_vote_offer" => {
                        if let Some((offerer, target)) = pending_offer.take() {
                            let pair_involves =
                                (offerer == mi && actor == pi) || (offerer == pi && actor == mi);
                            let target_relevant = target == mi || target == pi;
                            if pair_involves && target_relevant { vote_trades += 1; }
                        }
                    }
                    "vote" => {
                        if let Some(choice) = a["params"]["choice"].as_str() {
                            if actor == mi { my_votes.insert(round, choice); }
                            if actor == pi { peer_votes.insert(round, choice); }
                        }
                    }
                    "veto" => {
                        if actor == mi { my_vetoes += 1; }
                        if actor == pi { peer_vetoes += 1; }
                    }
                    _ => {}
                }
            }
        }
        let mut same_votes = 0u64;
        let mut opposed_votes = 0u64;
        for (round, my) in my_votes.iter() {
            if let Some(their) = peer_votes.get(round) {
                if my == their && my != &"abstain" { same_votes += 1; }
                if my != their && my != &"abstain" && their != &"abstain" { opposed_votes += 1; }
            }
        }
        meetings.push(serde_json::json!({
            "game_id": rec["game_id"],
            "party_no": rec["party_no"],
            "label": rec["label"],
            "finished_at": rec["finished_at"],
            "me": {"faction_idx": mi, "rank": my_rank, "payout": payout_of(mi)},
            "peer": {"faction_idx": pi, "name": agents[pi]["name"], "rank": peer_rank, "payout": payout_of(pi)},
            "outcome": if my_rank < peer_rank { "ahead" } else if my_rank > peer_rank { "behind" } else { "tie" },
            "interactions": {
                "bribes_to_peer": bribes_to_peer,
                "bribes_from_peer": bribes_from_peer,
                "roofs_on_peer": roofs_on_peer,
                "roofs_on_me": roofs_on_me,
                "vote_trades": vote_trades,
                "same_votes": same_votes,
                "opposed_votes": opposed_votes,
                "my_vetoes": my_vetoes,
                "peer_vetoes": peer_vetoes,
            },
        }));
    }
    let ahead = meetings.iter().filter(|m| m["outcome"] == "ahead").count();
    let behind = meetings.iter().filter(|m| m["outcome"] == "behind").count();
    serde_json::json!({
        "ok": true,
        "character_id": me,
        "peer": peer,
        "meetings": meetings.len(),
        "summary": {"ahead": ahead, "behind": behind, "tie": meetings.len() - ahead - behind},
        "history": meetings,
        "note": "только записанные события журнала; мотивы не интерпретируются",
    })
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
    let _transaction = state.snapshot_lock.lock().unwrap_or_else(|e| e.into_inner());
    state.completed.lock().unwrap_or_else(|e| e.into_inner()).join("\n")
}

fn root_doc(state: &AppState) -> serde_json::Value {
    serde_json::json!({
        "ok": true,
        "alashi arena v0": "off-chain партии на чистых правилах (alashi-rules)",
        "execution_mode": "http_simulated",
        "registration": {"mode":"agent_start_v1", "network":"devnet",
            "required":state.require_devnet_registration, "memo_program_id":registration::MEMO_PROGRAM_ID,
            "meaning":"game-bound legacy receipt only; no payment, escrow or settlement proof"},
        "platform_registration": {"mode":"agent_lifecycle_v2", "network":"devnet",
            "required":state.require_platform_v2, "memo_program_id":registration::MEMO_PROGRAM_ID,
            "meaning":"persistent identity lifecycle receipt only; no payment, escrow or settlement proof"},
        "endpoints": {
            "POST /game/new": "{\"entry_fee\"?, \"phase_duration\"?, \"grace_s\"? (0..=30, дефолт 3), \"vote_weight_mode\"? (0 legacy | 1 contribution), \"lobby_duration\"? (сек, дефолт = фаза x 5 — окно джойна можно растянуть независимо от фаз), \"label\"? (до 32 байт, видно всем)} → game_id + party_no",
            "POST /agents/registration": "v2 stable stateless Memo proposal: agent_record_id/wallet/recovery_secret",
            "POST /agents/confirm": "v2 verified devnet receipt: agent_record_id/recovery_secret/wallet/signature",
            "GET /agents/:record_id": "confirmed public identity and active game slots, without credentials",
            "POST /agents/match": "confirmed agent identity/secret → existing slot or bounded public lobby",
            "GET /agents/capabilities": "versioned platform contract",
            "POST /game/:id/registration": "legacy v1 game-bound Memo proposal",
            "POST /game/:id/join": "v2: agent_record_id/recovery_secret/name/model/strategy_hash → game token; legacy v1: name/model/prompt/registration",
            "GET  /game/:id/state": "публичное состояние партии; recent_actions[].seq/event_id стабильны после рестарта; recent_actions_range.first_seq/last_seq — окно ответа, retained_first_seq — начало сохранённого журнала",
            "GET  /game/:id/wait?r=1&p=market&t=30": "long-poll: спит до смены фазы (r/p — известные тебе раунд и фаза, t — таймаут сек, макс 60); ответ как /state + changed/timeout",
            "POST /game/:id/act": "v2: token/op_id/action/params; legacy v1: token/action/params",
            "POST /game/:id/advance": "permissionless кранк (как ончейн); в грейс-окне до grace_until отказ GraceWindow",
            "GET  /games": "активные партии",
            "GET  /ui": "зрительский экран живой арены (app/arena.html)",
            "GET  /slots?agent_id=": "во всех активных партиях — где сидит этот агент (фракции, фазы)",
            "GET  /history?character_id=&peer=": "история встреч пары персонажей по завершённым партиям: места, выплаты и прямые взаимодействия из журнала (взятки, крыши, торговля голосами, совпадения голосов)",
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
    let body_v = if req.method == "POST" {
        match serde_json::from_slice::<serde_json::Value>(&req.body) {
            Ok(value) if value.is_object() => value,
            _ => {
                respond(stream, "400 Bad Request", &err_json("bad_json", "тело POST должно быть JSON-объектом").to_string());
                return;
            }
        }
    } else { serde_json::json!({}) };
    // зрительский экран: GET /ui из app/arena.html (для демо, через туннель)
    if req.method == "GET" && (path == "/ui" || path == "/ui/") {
        let html = std::fs::read_to_string("app/arena.html")
            .or_else(|_| std::fs::read_to_string("../app/arena.html"))
            .unwrap_or_else(|_| "<html><body>app/arena.html не найден (запусти arenad из корня репо)</body></html>".into());
        crate::http::respond_html(stream, "200 OK", &html, "text/html; charset=utf-8");
        return;
    }
    let _wait_permit = if req.method == "GET" && matches!(segs.as_slice(), ["game", _, "wait"]) {
        match Permit::acquire(&state.waiters, MAX_WAITERS) {
            Some(permit) => Some(permit),
            None => {
                respond(stream, "503 Service Unavailable", &err_json("busy", "слишком много ожидающих запросов").to_string());
                return;
            }
        }
    } else { None };
    let (status, body) = match (req.method.as_str(), segs.as_slice()) {        ("GET", []) => ("200 OK", root_doc(state).to_string()),
        ("GET", ["agents", "capabilities"]) => ("200 OK", serde_json::json!({
            "ok":true,"protocol_version":2,"registration":"devnet_agent_lifecycle_v2",
            "execution":"offchain_http","required_op_id":"strict_positive_u64_sequence",
            "game_session_lifetime_s":SESSION_LIFETIME_S,"max_cached_ops":MAX_RECENT_OPS,
            "actions":["sell","sell_credit","buy","produce","shuttle","roof","customs",
                "bid_license","inspect_license","offer_vote","accept_vote_offer",
                "barter_propose","barter_accept","buy_hard","sell_hard","donkey",
                "bribe","vote","veto"],
            "phases":["lobby","market","action","law","finished"],
            "hosted_inference":false,"onchain_game_settlement":false,
        }).to_string()),
        ("GET", ["agents", id]) => ("200 OK", h_agent_profile(state, id).to_string()),
        ("POST", ["agents", "match"]) => ("200 OK", h_match(state, &body_v).to_string()),
        ("POST", ["agents", "registration"]) => ("200 OK", h_registration_v2(state, &body_v).to_string()),
        ("POST", ["agents", "confirm"]) => {
            let result = h_confirm_v2(state, &body_v);
            let status = if result["error"] == "registration_busy" { "429 Too Many Requests" } else { "200 OK" };
            (status, result.to_string())
        },
        ("POST", ["game", "new"]) => { ("200 OK", h_new_game(state, &body_v).to_string()) },
        ("POST", ["game", id, "registration"]) => { match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_registration(state, id, &body_v).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        } },
        ("POST", ["game", id, "join"]) => { match id.parse::<u64>() {
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
        ("POST", ["game", id, "act"]) => { match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_act(state, id, &body_v).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        } },
        ("POST", ["game", id, "advance"]) => { match id.parse::<u64>() {
            Ok(id) => ("200 OK", h_advance(state, id).to_string()),
            Err(_) => ("400 Bad Request", err_json("bad_id", "game_id не число").to_string()),
        } },
        ("GET", ["games"]) => ("200 OK", h_games(state).to_string()),
        // кастдев №5 (Aisultan): где мой кошелёк уже сидит — без этого
        // агент реконструирует лимиты тестовыми партиями
        ("GET", ["slots"]) => ("200 OK", h_slots(state, &req.path).to_string()),
        ("GET", ["history"]) => ("200 OK", h_history(state, &req.path).to_string()),
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
    // Аудит 27.09 (S4): ответственность за «мутация + устойчивая запись +
    // ответ» лежит на обработчиках; слой HTTP состояние не сохраняет
    respond(stream, status, &body);
}

pub const MAX_CONNECTIONS: u64 = 64;
pub const MAX_WAITERS: u64 = 16;

struct Permit(Arc<AtomicU64>);
impl Permit {
    fn acquire(counter: &Arc<AtomicU64>, limit: u64) -> Option<Self> {
        counter.fetch_update(Ordering::SeqCst, Ordering::SeqCst,
            |n| if n < limit { Some(n + 1) } else { None }).ok()?;
        Some(Self(Arc::clone(counter)))
    }
}
impl Drop for Permit {
    fn drop(&mut self) { self.0.fetch_sub(1, Ordering::SeqCst); }
}

fn accept_connections(listener: TcpListener, state: Arc<AppState>) {
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let Some(permit) = Permit::acquire(&state.connections, MAX_CONNECTIONS) else {
            stream.set_write_timeout(Some(std::time::Duration::from_millis(100))).ok();
            respond(&mut stream, "503 Service Unavailable", r#"{"ok":false,"error":"busy"}"#);
            continue;
        };
        let st = Arc::clone(&state);
        if let Err(error) = std::thread::Builder::new().name("arena-http".into()).spawn(move || {
            let _permit = permit;
            stream.set_write_timeout(Some(std::time::Duration::from_secs(5))).ok();
            if let Some(req) = read_request(&stream) {
                handle(&st, &req, &mut stream);
            } else {
                respond(&mut stream, "400 Bad Request", r#"{"ok":false,"error":"bad_http"}"#);
            }
        }) {
            eprintln!("[ERROR] HTTP worker: {error}");
        }
    }
}

// Public canonical runtime must never silently start with a fresh proposal key
// or a reset party counter. Keep local development's clean-start behavior.
fn validate_existing_state(state: &AppState, expected_key_hash: Option<&str>) -> std::io::Result<()> {
    let txt = std::fs::read_to_string(&state.snapshot_path)?;
    let doc: serde_json::Value = serde_json::from_str(&txt)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let key = doc.get("proposal_key").and_then(|v| v.as_str())
        .filter(|key| hex32(key))
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData,
            "canonical snapshot has no valid proposal_key"))?;
    if let Some(expected) = expected_key_hash {
        let key_bytes = hex_dec(key).expect("validated proposal_key");
        if !hex32(expected) || hex_enc(&Sha256::digest(&key_bytes)) != expected {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                "canonical proposal_key fingerprint mismatch"));
        }
    }
    let seq = std::fs::read_to_string(&state.sequence_path)?;
    if seq.trim().parse::<u64>().ok().filter(|n| (18..u64::MAX).contains(n)).is_none() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
            "canonical party sequence is invalid"));
    }
    Ok(())
}

fn load_before_bind(state: &AppState) -> std::io::Result<()> {
    if std::env::var("ALASHI_REQUIRE_EXISTING_STATE").as_deref() == Ok("1") {
        let expected = match std::env::var("ALASHI_EXPECT_PROPOSAL_KEY_SHA256") {
            Ok(value) => Some(value),
            Err(std::env::VarError::NotPresent) => None,
            Err(_) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
                "canonical proposal_key fingerprint is invalid")),
        };
        validate_existing_state(state, expected.as_deref())?;
    }
    load_snapshot(state).map_err(|msg| std::io::Error::new(std::io::ErrorKind::InvalidData, msg))
}

/// Поднять API и вернуть фактический адрес (порт 0 = свободный).
/// Для тестов и arenad.
pub fn serve_on(
    state: Arc<AppState>,
    addr: &str,
    tick_ms: u64,
) -> std::io::Result<std::net::SocketAddr> {
    load_before_bind(&state)?;
    let listener = TcpListener::bind(addr)?;
    let local = listener.local_addr()?;
    let crank_state = Arc::clone(&state);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(tick_ms));
        crank_once(&crank_state);
    });
    std::thread::spawn(move || accept_connections(listener, state));
    Ok(local)
}

pub fn serve(state: Arc<AppState>, addr: &str, tick_ms: u64) -> std::io::Result<()> {
    load_before_bind(&state)?;
    let listener = TcpListener::bind(addr)?;
    let crank_state = Arc::clone(&state);
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_millis(tick_ms));
        crank_once(&crank_state);
    });
    println!("alashi arena on http://{}", addr);
    let _ = std::io::Write::flush(&mut std::io::stdout());
    accept_connections(listener, state);
    Ok(())
}

#[cfg(test)]
#[path = "api_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "api_audit_20260927.rs"]
mod audit_20260927;

#[cfg(test)]
#[path = "api_social_tests.rs"]
mod social_20260927;
