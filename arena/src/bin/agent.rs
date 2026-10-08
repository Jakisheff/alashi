//! agent: LLM-клиент HTTP-арены alashi. Без кошелька, без блокчейна:
//! заходит в партию, читает состояние, решает через LLM, действует.
//!
//!   agent --url http://127.0.0.1:8090 --game 1 --name Zhambyl \
//!         [--model glm-4.5-flash] [--prompt файл] [--no-llm] \
//!         [--recovery-file путь]
//!
//! Ключ LLM: env ALASHI_LLM_KEY или ~/.config/alashi/llm.json
//! (как у ончейн-бота). Без ключа — жадный фоллбэк: продать всё /
//! произвести / голосовать за. Эпоха 90-х: полный словарь M1-M11
//! (вексель, валютчик, крыша, челнок, таможня, лицензия, скупка
//! голосов, бартер). После партии пишет селф-дебриф в inbox/<имя>/
//! (подхватывает демон agent_inbox).
//!
//! Аудит 27.09 (S6): внешние адреса — только https (curl, проверка
//! сертификата); открытый HTTP по умолчанию работает лишь на loopback.
//! --recovery-file сохраняет секрет сессии (0600) до join и позволяет
//! восстановить сессию после перезапуска.

use serde_json::Value;
use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Duration;

#[path = "agent/live_client.rs"]
mod live_client;

// ---------- http-клиент ----------
// Аудит 27.09 (S6): TLS через системный curl с проверкой сертификата
// (паттерн нуля зависимостей, как у LLM-вызовов ниже). Открытый HTTP
// допустим только на loopback: bearer-токен по внешней сети без TLS
// перехватывается. Обход для локальных тестов: ALASHI_ALLOW_INSECURE_HTTP=1.

fn allow_insecure_http() -> bool {
    std::env::var("ALASHI_ALLOW_INSECURE_HTTP").ok().as_deref() == Some("1")
}

fn is_loopback(base: &str) -> bool {
    let host = base
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split(':')
        .next()
        .unwrap_or("");
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

struct CurlConfig(std::path::PathBuf);

impl Drop for CurlConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn private_curl_config(header: &str) -> Option<CurlConfig> {
    if header.contains('\r') || header.contains('\n') {
        return None;
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    for _ in 0..8 {
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("alashi-curl-{}-{id}.conf", std::process::id()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let escaped = header.replace('\\', "\\\\").replace('"', "\\\"");
        match options.open(&path) {
            Ok(mut file) => {
                if write!(file, "header = \"{escaped}\"\n").is_err() {
                    let _ = std::fs::remove_file(&path);
                    return None;
                }
                return Some(CurlConfig(path));
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

/// Request bodies can contain bearer/session credentials or private wishes;
/// keep them off curl's process arguments and always clean up its temp config.
fn curl_json(mut cmd: Command, body: &str) -> Option<std::process::Output> {
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(body.as_bytes()).ok()?;
    }
    child.wait_with_output().ok()
}

fn curl_json_with_header(
    mut cmd: Command,
    body: &str,
    header: &str,
) -> Option<std::process::Output> {
    let config = private_curl_config(header)?;
    cmd.arg("--config")
        .arg(&config.0)
        .arg("--data-binary")
        .arg("@-");
    curl_json(cmd, body)
}

fn http(base: &str, method: &str, path: &str, body: Option<&str>) -> Option<Value> {
    http_timeout(base, method, path, body, 75)
}

fn http_timeout(
    base: &str,
    method: &str,
    path: &str,
    body: Option<&str>,
    timeout_s: u64,
) -> Option<Value> {
    if base.starts_with("http://") && !is_loopback(base) && !allow_insecure_http() {
        eprintln!("[ERROR] открытый HTTP за пределами loopback: используй https-адрес арены (токен сессии перехватывается прослушкой сети)");
        return None;
    }
    let url = format!("{base}{path}");
    let mut cmd = Command::new("curl");
    cmd.args([
        "-s",
        "-4",
        "-m",
        &timeout_s.to_string(),
        "-X",
        method,
        "-H",
        "Content-Type: application/json",
    ]);
    cmd.arg(&url);
    let out = if let Some(b) = body {
        cmd.arg("--data-binary").arg("@-");
        curl_json(cmd, b)?
    } else {
        cmd.output().ok()?
    };
    let text = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(&text).ok()
}

// ---------- LLM (curl, как bots/llm.rs — ноль зависимостей) ----------

struct LlmCfg {
    key: String,
    base: String,
    model: String,
    // цены за 1M токенов (USD), необязательно: без них считаем только токены
    price_in: Option<f64>,
    price_out: Option<f64>,
}

/// Расход токенов одного LLM-вызова (для метрики «стоимость хода/партии»,
/// паттерн gpt-author print_step_costs).
#[derive(Default, Clone, Copy)]
struct Usage {
    prompt: u64,
    completion: u64,
}

impl Usage {
    fn cost_usd(&self, cfg: &LlmCfg) -> Option<f64> {
        Some(
            self.prompt as f64 / 1e6 * cfg.price_in?
                + self.completion as f64 / 1e6 * cfg.price_out?,
        )
    }
    fn add(&mut self, o: Usage) {
        self.prompt += o.prompt;
        self.completion += o.completion;
    }
}

fn llm_cfg(model_override: Option<&str>) -> Option<LlmCfg> {
    // ключ и цены: env ALASHI_LLM_KEY или ~/.config/alashi/llm.json
    // {"key", "price_in_per_1m"?, "price_out_per_1m"?}
    let file = std::env::var("HOME").ok().and_then(|h| {
        let path = h + "/.config/alashi/llm.json";
        let s = std::fs::read_to_string(path).ok()?;
        serde_json::from_str::<Value>(&s).ok()
    });
    let key = std::env::var("ALASHI_LLM_KEY")
        .ok()
        .or_else(|| file.as_ref()?.get("key")?.as_str().map(|s| s.to_string()))?;
    if key.len() < 10 {
        return None;
    }
    Some(LlmCfg {
        price_in: file.as_ref().and_then(|f| f["price_in_per_1m"].as_f64()),
        price_out: file.as_ref().and_then(|f| f["price_out_per_1m"].as_f64()),
        key,
        base: "https://api.z.ai/api/paas/v4".into(),
        model: model_override.unwrap_or("glm-4.5-flash").into(),
    })
}

fn llm_ask(
    cfg: &LlmCfg,
    system: &str,
    user: &str,
    timeout_s: u64,
    ctx: Option<&Value>,
    max_tokens: u32,
) -> (Option<String>, Usage) {
    let body = serde_json::json!({
        "model": cfg.model,
        "thinking": {"type": "disabled"},
        "max_tokens": max_tokens,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    })
    .to_string();
    let mut cmd = Command::new("curl");
    cmd.args([
        "-s",
        "-4",
        "-m",
        &timeout_s.to_string(),
        "-X",
        "POST",
        &format!("{}/chat/completions", cfg.base),
        "-H",
        "Content-Type: application/json",
    ]);
    let out = match curl_json_with_header(cmd, &body, &format!("Authorization: Bearer {}", cfg.key))
    {
        Some(o) => o,
        None => return (None, Usage::default()),
    };
    let txt = match String::from_utf8(out.stdout) {
        Ok(t) => t,
        Err(_) => return (None, Usage::default()),
    };
    let v: Value = match serde_json::from_str(&txt) {
        Ok(v) => v,
        Err(_) => return (None, Usage::default()),
    };
    if let Some(ctx) = ctx {
        llm_dump(ctx, system, user, &v);
    }
    let usage = Usage {
        prompt: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
        completion: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
    };
    let c = v
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .map(|c| c.to_string())
        .filter(|c| !c.trim().is_empty());
    (c, usage)
}

/// Дамп полной цепочки «промпт -> ответ» на каждый LLM-вызов в отдельный
/// JSON (находка mshumer/OpenReasoningEngine: logs/conversation_*.json):
/// послематчевый разбор воспроизводим без реконструкции по обрывкам сессий.
/// Каталог: env ALASHI_AGENT_LOGS или ./agent_llm_logs.
fn llm_dump(ctx: &Value, system: &str, user: &str, resp: &Value) {
    let dir = std::env::var("ALASHI_AGENT_LOGS").unwrap_or_else(|_| "agent_llm_logs".into());
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = ts / 86400;
    let (y, m, d) = epoch_to_ymd(days as i64);
    let sday = ts % 86400;
    let iso = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m,
        d,
        sday / 3600,
        (sday % 3600) / 60,
        sday % 60
    );
    let rec = serde_json::json!({
        "ts": ts,
        "ts_iso": iso,
        "ctx": ctx,
        "system": system,
        "user": user,
        "response": resp,
    });
    let path = format!(
        "{}/llm_g{}_r{}_{}_{}.json",
        dir,
        ctx["game"].as_u64().unwrap_or(0),
        ctx["round"].as_u64().unwrap_or(0),
        ctx["phase"].as_str().unwrap_or("?"),
        ts
    );
    if std::fs::write(&path, rec.to_string()).is_err() {
        eprintln!("[agent] не смог записать дамп LLM: {}", path);
    }
}

fn capture_decision_text(
    value: &Value,
    allow_public: bool,
    allow_private_reply: bool,
) -> DecisionText {
    DecisionText {
        public_message: allow_public
            .then(|| bounded_speech(value["public_message"].as_str()))
            .flatten(),
        private_reply: allow_private_reply
            .then(|| bounded_speech(value["owner_reply"].as_str()))
            .flatten(),
    }
}

fn bounded_speech(text: Option<&str>) -> Option<String> {
    text.map(str::trim)
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 1024
                && s.chars().count() <= 240
                && !s.chars().any(|c| c.is_control() && c != '\n')
        })
        .map(str::to_string)
}

fn parse_json_block(raw: &str) -> Option<Value> {
    let s = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str(s).ok()
}

// ---------- агент ----------

/// Память раунда: bid/inspect не сжигают ход, acted остаётся false —
/// без флагов фоллбэк бидил бы и инспектил бы каждый тик (урок gid 12:
/// двойной бид Agent3 в gid 1). Сбрасывается на смене (round, phase).
#[derive(Default)]
struct Mem {
    inspected: bool,
    bid: bool,
    license_yield: Option<u64>,
    veto_reviewed: bool,
}

fn needs_decision(s: &Value, me: &Value, mem: &Mem) -> bool {
    if me["alive"] == false {
        return false;
    }
    match s["phase"].as_str() {
        Some("market" | "action") => me["acted"] == false,
        Some("law") => {
            s["law_card"]
                .as_u64()
                .is_some_and(|c| c != alashi_rules::constants::NO_LAW as u64)
                && (me["voted"] == false
                    || (me["voted"] == true && !mem.veto_reviewed && s["veto_pending"] == false))
        }
        _ => false,
    }
}

const SYSTEM: &str = "Ты играешь в политэкономическую игру Alashi против других агентов. \
Твоя цель — максимизировать свой cash к концу 6 раундов: ранг по cash определяет долю банка. \
Отвечай СТРОГО одним JSON-объектом без пояснений.";

fn flag(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().cloned();
        }
    }
    None
}

/// Аудит 27.09 (S6): секрет восстановления — только владелец, права 0600.
fn write_recovery_secret(path: &str, secret: &str) {
    let mut options = std::fs::OpenOptions::new();
    options.create(true).write(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path).and_then(|mut f| {
        use std::io::Write;
        f.write_all(secret.as_bytes())
    }) {
        Ok(()) => println!("[agent] секрет восстановления сохранён: {path}"),
        Err(e) => eprintln!(
            "[ERROR] не удалось сохранить секрет восстановления ({path}): {e}; сохрани вручную"
        ),
    }
}

fn is_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn v2_join_body(
    record_id: &str,
    recovery_secret: &str,
    name: &str,
    model: &str,
    prompt: &str,
) -> Value {
    serde_json::json!({
        "agent_record_id": record_id,
        "recovery_secret": recovery_secret,
        "name": name,
        "model": model,
        "strategy_hash": arena::api::agent_id_of(model, prompt),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let url = flag(&args, "--url").unwrap_or_else(|| "http://127.0.0.1:8090".into());
    let name = flag(&args, "--name").unwrap_or_else(|| "Agent".into());
    let game: u64 = flag(&args, "--game")
        .and_then(|g| g.parse().ok())
        .unwrap_or_else(|| {
            // без --game: свежайшая партия в лобби
            let g = http(&url, "GET", "/games", None).expect("арена недоступна");
            g["games"]
                .as_array()
                .and_then(|l| {
                    l.iter()
                        .rev()
                        .find(|g| g["phase"] == "lobby")
                        .or_else(|| l.last())
                })
                .and_then(|g| g["game_id"].as_u64())
                .expect("нет партий — создай POST /game/new")
        });
    let prompt = flag(&args, "--prompt")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| "Стратег: играй рационально, следи за таблицей цен и влиянием.".into());
    // R17: --model разбирается и уходит в join (иначе в лидерборд
    // попадала константа "glm-agent" при любой реальной модели).
    // R6-следование: дефолт уникален имени бота — два одинаковых
    // (model, prompt) это один agent_id => DuplicateWallet на join
    let model_flag = flag(&args, "--model");
    let declared_model = model_flag
        .clone()
        .unwrap_or_else(|| format!("glm-agent-{name}"));
    let no_llm = args.iter().any(|a| a == "--no-llm");
    let llm = if no_llm {
        None
    } else {
        llm_cfg(model_flag.as_deref())
    };
    if llm.is_none() {
        println!("[agent] LLM-ключа нет — жадный фоллбэк");
    }

    // Аудит 27.09 (S6): секрет восстановления создаётся/читается до
    // join и хранится в файле с правами 0600, переживает перезапуск.
    // Отчёт «Цукерберг/Muse» 27.09: --owner-key задаёт постоянную
    // личность персонажа, независимую от модели и промпта.
    let recovery_file = flag(&args, "--recovery-file");
    let agent_record_id = flag(&args, "--agent-record-id");
    let owner_key = flag(&args, "--owner-key").filter(|k| k.len() == 64);
    let stored_secret = recovery_file
        .as_ref()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| is_hex64(s));
    if recovery_file.is_some() && stored_secret.is_none() {
        eprintln!("[WARN] --recovery-file не содержит 64 lowercase hex-символа");
    }
    if let Some(id) = &agent_record_id {
        if !is_hex64(id) || stored_secret.is_none() || owner_key.is_some() {
            eprintln!("[ERROR] v2 требует корректный --agent-record-id и --recovery-file; --owner-key отдельно не применяется");
            std::process::exit(2);
        }
    }

    let join_body = if let (Some(id), Some(secret)) = (&agent_record_id, &stored_secret) {
        v2_join_body(id, secret, &name, &declared_model, &prompt)
    } else {
        let mut body = serde_json::json!({
            "name": name, "model": declared_model.clone(), "prompt": prompt.clone(),
        });
        if let Some(k) = &owner_key {
            body["owner_key"] = serde_json::json!(k);
        }
        if let Some(secret) = &stored_secret {
            body["recovery_secret"] = serde_json::json!(secret);
        }
        body
    };
    let mut j = http(
        &url,
        "POST",
        &format!("/game/{}/join", game),
        Some(&join_body.to_string()),
    )
    .expect("join");
    if j["ok"] != true && (stored_secret.is_some() || owner_key.is_some()) {
        // Lost join response / existing game session: recover the same identity.
        let mut recover_body = if agent_record_id.is_some() {
            join_body.clone()
        } else {
            let mut body = serde_json::json!({
                "name": name, "model": declared_model.clone(), "prompt": prompt.clone(),
                "recover": true,
            });
            if let Some(k) = &owner_key {
                body["owner_key"] = serde_json::json!(k);
            }
            if let Some(secret) = &stored_secret {
                body["recovery_secret"] = serde_json::json!(secret);
            }
            body
        };
        if agent_record_id.is_some() {
            recover_body["recover"] = serde_json::json!(true);
        }
        j = http(
            &url,
            "POST",
            &format!("/game/{}/join", game),
            Some(&recover_body.to_string()),
        )
        .expect("recover join");
    }
    if j["ok"] != true {
        eprintln!(
            "[ERROR] join failed: {}",
            j["error"].as_str().unwrap_or("unknown")
        );
        std::process::exit(1);
    }
    if let (Some(path), Some(secret)) = (&recovery_file, j["recovery_secret"].as_str()) {
        write_recovery_secret(path, secret);
    }
    if let Some(id) = &agent_record_id {
        if j["agent_record_id"].as_str() != Some(id.as_str()) {
            eprintln!("[ERROR] v2 join identity mismatch; live features disabled");
            std::process::exit(1);
        }
    }
    let token = j["token"].as_str().unwrap().to_string();
    let harness = agent_record_id
        .as_ref()
        .map(|_| live_client::HarnessClient::new(&url));
    let public_speech_requested = args.iter().any(|a| a == "--public-speech");
    let ambient_idle_seconds = flag(&args, "--ambient-idle-seconds")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| (1..=300).contains(seconds));
    let mut live = if public_speech_requested {
        match (agent_record_id.as_deref(), stored_secret.as_deref()) {
            (Some(id), Some(secret)) => match live_client::LiveClient::open(&url, id, secret, true)
            {
                Ok(client) => Some(client),
                Err(code) => {
                    eprintln!("[agent] public live opt-in unavailable: {code}");
                    None
                }
            },
            _ => {
                eprintln!("[agent] --public-speech requires v2 identity");
                None
            }
        }
    } else {
        None
    };
    let public_speech_enabled = live.is_some();
    if let Some(client) = live.as_mut() {
        let _ = client.presence();
        // Start the personal cursor at session open; old addressed posts must not wake a new run.
        let _ = client.personal_events(100);
    }
    let mut last_presence = std::time::Instant::now();
    let mut last_feed_poll = std::time::Instant::now();
    let mut game_feed_cursor = 0u64;
    let mut pending_public_messages: Vec<Value> = Vec::new();
    let mut game_chat_attempted: HashSet<String> = HashSet::new();
    let mut game_chat_calls = 0u8;
    let mut last_game_chat: Option<std::time::Instant> = None;
    let my_idx = j["faction_idx"].as_u64().unwrap() as usize;
    println!("[agent] {} в игре {} (фракция {})", name, game, my_idx);

    let started = std::time::Instant::now();
    let mut total_usage = Usage::default();
    // селф-дебриф: каждый свой ход в журнал, после партии — в inbox
    let mut my_log: Vec<String> = vec![];
    let mut mem = Mem::default();
    let mut last_rf: Option<(u64, String)> = None;
    loop {
        if last_presence.elapsed() >= Duration::from_secs(20) {
            if let Some(client) = live.as_mut() {
                let _ = client.presence();
                let _ = client.personal_events(100);
            }
            last_presence = std::time::Instant::now();
        }
        if started.elapsed() > Duration::from_secs(2700) {
            println!("[agent] таймаут 10 мин, выхожу");
            return;
        }
        let r = http(&url, "GET", &format!("/game/{}/state", game), None);
        let Some(r) = r else {
            std::thread::sleep(Duration::from_millis(500));
            continue;
        };
        if r["finished"] == true {
            let res = &r["result"];
            write_self_report(&name, &declared_model, game, my_idx, res, &my_log);
            let ranks: Vec<u64> = res["ranks"]
                .as_array()
                .map(|a| a.iter().map(|v| v.as_u64().unwrap_or(9)).collect())
                .unwrap_or_default();
            let my_place = ranks
                .iter()
                .position(|i| *i as usize == my_idx)
                .map(|p| p + 1)
                .unwrap_or(99);
            let payout = res["payouts"]
                .as_array()
                .and_then(|p| p.get(my_idx))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cost_note = match llm.as_ref().and_then(|c| total_usage.cost_usd(c)) {
                Some(c) => format!("~{:.4}$", c),
                None => "цены не заданы".into(),
            };
            println!(
                "[agent] партия окончена: моё место {} из {}, выплата {} alashi; LLM за партию: {}in/{}out ({})",
                my_place,
                ranks.len(),
                payout / 1_000_000,
                total_usage.prompt,
                total_usage.completion,
                cost_note
            );
            if let Some(seconds) = ambient_idle_seconds {
                match (
                    public_speech_enabled,
                    live.as_mut(),
                    llm.as_ref(),
                    agent_record_id.as_deref(),
                ) {
                    (true, Some(client), Some(cfg), Some(record_id)) => {
                        run_ambient_idle(client, cfg, record_id, seconds);
                    }
                    _ => eprintln!(
                        "[agent] ambient idle requires --public-speech, v2 identity, and an LLM"
                    ),
                }
            }
            return;
        }
        if public_speech_enabled && last_feed_poll.elapsed() >= Duration::from_secs(1) {
            if let (Some(client), Some(record_id)) = (live.as_ref(), agent_record_id.as_deref()) {
                if let Ok(feed) = client.game_events(game, game_feed_cursor, 100) {
                    if let Some(events) = feed["events"].as_array() {
                        for event in events {
                            if event["kind"] == "agent_message"
                                && event["to_agent_record_id"] == record_id
                                && event["author_agent_record_id"] != record_id
                            {
                                pending_public_messages.push(event.clone());
                            }
                        }
                        if pending_public_messages.len() > 20 {
                            let keep_from = pending_public_messages.len() - 20;
                            pending_public_messages.drain(0..keep_from);
                        }
                    }
                    if let Some(cursor) = feed["next_cursor"].as_u64() {
                        game_feed_cursor = cursor;
                    }
                }
            }
            last_feed_poll = std::time::Instant::now();
        }
        let s = &r["state"];
        let phase = s["phase"].as_str().unwrap_or("lobby");
        let phase_instance = s["phase_instance_id"].as_str().unwrap_or("");
        pending_public_messages.retain(|event| {
            event["game_id"].as_u64() == Some(game)
                && event["phase_instance_id"].as_str() == Some(phase_instance)
                && event["to_agent_record_id"].as_str() == agent_record_id.as_deref()
        });
        let rf = (s["round"].as_u64().unwrap_or(0), phase.to_string());
        if last_rf.as_ref() != Some(&rf) {
            mem = Mem::default();
            last_rf = Some(rf);
        }
        let me = s["factions"]
            .as_array()
            .and_then(|f| f.iter().find(|f| f["idx"].as_u64() == Some(my_idx as u64)))
            .cloned()
            .unwrap_or(Value::Null);
        let need_act = needs_decision(s, &me, &mem);
        if public_speech_enabled
            && game_chat_calls < 4
            && last_game_chat.is_none_or(|last| last.elapsed() >= Duration::from_secs(20))
        {
            if let (Some(client), Some(cfg), Some(record_id), Some(event)) = (
                live.as_ref(),
                llm.as_ref(),
                agent_record_id.as_deref(),
                pending_public_messages.last().cloned(),
            ) {
                let message_id = event["message_id"].as_str().unwrap_or("");
                if !message_id.is_empty()
                    && !game_chat_attempted.contains(message_id)
                    && try_game_reply(client, &url, cfg, game, &token, record_id, s, &event)
                {
                    game_chat_attempted.insert(message_id.to_string());
                    game_chat_calls += 1;
                    last_game_chat = Some(std::time::Instant::now());
                    pending_public_messages.retain(|m| m["message_id"] != message_id);
                }
            }
        }
        if need_act {
            if phase == "law" && me["voted"] == true {
                // One review per phase, including transport failures and pass.
                mem.veto_reviewed = true;
            }
            // честная очередь: джиттер перед ходом, чтобы внешние агенты
            // на опросе не проигрывали гонку серверным ботам (дебриф r3).
            // Базар — исключение: позиция продажи решает цену (урок gid 12).
            let jitter = if phase == "market" {
                0
            } else {
                400 + (std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_millis())
                    .unwrap_or(0)
                    % 1600)
            };
            std::thread::sleep(Duration::from_millis(jitter as u64));
            let phase_owned = phase.to_string();
            if phase_owned == "market" && me["goods"].as_u64().unwrap_or(0) == 0 {
                // нечего продавать и нечем купить (cash нет с r1) — пропускаем
                std::thread::sleep(Duration::from_millis(700));
                continue;
            }
            let mut decision_state = s;
            let mut decision_snapshot: Option<Value> = None;
            let mut decision_me = &me;
            let mut owner_wish: Option<live_client::OwnerWish> = None;
            if llm.is_some() {
                if let Some(client) = harness.as_ref() {
                    let server_now = s["now"].as_i64().unwrap_or(0);
                    let deadline = s["phase_ends_at"].as_i64().unwrap_or(0);
                    let model_budget = if phase == "market" { 8 } else { 15 };
                    let reserve = model_budget + 8;
                    if deadline.saturating_sub(server_now) >= reserve {
                        if let Ok(mut wishes) = client.claim_wishes(game, &token, 0, 1) {
                            if !wishes.is_empty() {
                                let candidate = wishes.remove(0);
                                let latest =
                                    http(&url, "GET", &format!("/game/{game}/state"), None);
                                let valid_context = latest.as_ref().is_some_and(|fresh| {
                                    fresh["finished"] != true
                                        && fresh["state"]["phase_instance_id"]
                                            == s["phase_instance_id"]
                                        && fresh["state"]["phase_ends_at"]
                                            .as_i64()
                                            .zip(fresh["state"]["now"].as_i64())
                                            .is_some_and(|(end, now)| {
                                                end.saturating_sub(now) >= reserve
                                            })
                                        && fresh["state"]["factions"]
                                            .as_array()
                                            .and_then(|fs| {
                                                fs.iter().find(|f| {
                                                    f["idx"].as_u64() == Some(my_idx as u64)
                                                })
                                            })
                                            .is_some_and(|f| {
                                                needs_decision(&fresh["state"], f, &mem)
                                            })
                                });
                                if valid_context {
                                    decision_snapshot = latest.map(|fresh| fresh["state"].clone());
                                    if client
                                        .update_wish_status(
                                            game,
                                            &token,
                                            &candidate.wish_id,
                                            &candidate.lease_id,
                                            "consumed",
                                            None,
                                        )
                                        .is_ok()
                                    {
                                        owner_wish = Some(candidate);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(fresh_state) = decision_snapshot.as_ref() {
                decision_state = fresh_state;
                if let Some(fresh_me) = fresh_state["factions"]
                    .as_array()
                    .and_then(|fs| fs.iter().find(|f| f["idx"].as_u64() == Some(my_idx as u64)))
                {
                    decision_me = fresh_me;
                }
            }
            let mut step_usage = Usage::default();
            let incoming = if owner_wish.is_none() {
                pending_public_messages.last()
            } else {
                None
            };
            let decision_started = std::time::Instant::now();
            let mut decision = decide(
                &llm,
                decision_state,
                decision_me,
                &prompt,
                &mem,
                game,
                owner_wish.as_ref().map(|w| w.text.as_str()),
                incoming,
                public_speech_enabled,
                &mut step_usage,
            );
            let wish_params_rejected = owner_wish.as_ref().is_some_and(|_| {
                guard_wish_decision(&mut decision, decision_state, decision_me, &mem)
            });
            record_owner_reply(
                harness.as_ref(),
                game,
                &token,
                owner_wish.as_ref(),
                decision.private_reply.as_deref(),
            );
            total_usage.add(step_usage);
            if step_usage.prompt + step_usage.completion > 0 {
                let cost_note = match step_usage.cost_usd(llm.as_ref().unwrap()) {
                    Some(c) => format!(", ~{:.5}$", c),
                    None => " (цены не заданы в llm.json)".into(),
                };
                println!(
                    "[agent] LLM за ход: {}in/{}out{}, партия: {}in/{}out",
                    step_usage.prompt,
                    step_usage.completion,
                    cost_note,
                    total_usage.prompt,
                    total_usage.completion
                );
                my_log.push(format!(
                    "llm r{} {}: {}in/{}out{}",
                    s["round"].as_u64().unwrap_or(0),
                    phase,
                    step_usage.prompt,
                    step_usage.completion,
                    cost_note
                ));
            }
            let public_post_reserve = decision_state["phase_ends_at"]
                .as_i64()
                .unwrap_or(0)
                .saturating_sub(decision_state["now"].as_i64().unwrap_or(0))
                .saturating_sub(decision_started.elapsed().as_secs() as i64);
            if public_post_reserve >= 3 {
                if let (Some(client), Some(text)) =
                    (live.as_ref(), decision.public_message.as_deref())
                {
                    let phase_instance = decision_state["phase_instance_id"].as_str().unwrap_or("");
                    if !phase_instance.is_empty() {
                        if client
                            .post_game_message(
                                game,
                                &token,
                                phase_instance,
                                text,
                                decision.to_agent_record_id.as_deref(),
                                decision.reply_to_message_id.as_deref(),
                            )
                            .is_ok()
                        {
                            if let Some(reply_id) = decision.reply_to_message_id.as_deref() {
                                pending_public_messages
                                    .retain(|event| event["message_id"] != reply_id);
                            }
                        }
                    }
                }
            }
            let (action, params) = (decision.action, decision.params);
            if action == "pass" {
                my_log.push(format!("r{} {}: pass after vote", s["round"], phase));
                continue;
            }
            let body = action_request_body(
                &token,
                action,
                &params,
                if llm.is_some() && !wish_params_rejected {
                    "llm"
                } else {
                    "fallback"
                },
            );
            let rr = http(&url, "POST", &format!("/game/{}/act", game), Some(&body));
            if let Some(rr) = rr {
                let ok_s = if rr["ok"] == true {
                    "ok".into()
                } else {
                    format!("err: {}", rr["error"].as_str().unwrap_or("?"))
                };
                my_log.push(format!(
                    "r{} {}: {} {} -> {}",
                    s["round"].as_u64().unwrap_or(0),
                    phase,
                    action,
                    logged_action_params(owner_wish.is_some(), &params),
                    ok_s
                ));
                if rr["ok"] != true {
                    let err_s = rr["error"].as_str().unwrap_or("?").to_string();
                    println!("[agent] отказ: {} — пробую фоллбэк", err_s);
                    // уже инсайдер / уже бид: пометить, чтобы фоллбэк не
                    // долбил то же действие каждый тик до конца фазы
                    match action {
                        "inspect_license" => mem.inspected = true,
                        "bid_license" if err_s == "AlreadyActed" => mem.bid = true,
                        _ => {}
                    }
                    // фаза уже ушла — фоллбэк того же хода тоже не пройдёт
                    if err_s != "WrongPhase" && err_s != "TooEarly" && err_s != "GraceWindow" {
                        let (a2, p2) = fallback(phase, s, &me, &mem);
                        if a2 == "pass" {
                            my_log.push(format!("r{} {}: fallback pass", s["round"], phase));
                            continue;
                        }
                        let body = serde_json::json!({
                            "token": token, "action": a2, "params": p2, "by": "fallback",
                        })
                        .to_string();
                        let r2 = http(&url, "POST", &format!("/game/{}/act", game), Some(&body));
                        let ok2 = r2.as_ref().map(|v| v["ok"] == true).unwrap_or(false);
                        my_log.push(format!(
                            "r{} {}: fallback {} {} -> {}",
                            s["round"].as_u64().unwrap_or(0),
                            phase,
                            a2,
                            p2,
                            if ok2 { "ok" } else { "err" }
                        ));
                    }
                } else {
                    match action {
                        "inspect_license" => {
                            mem.inspected = true;
                            if let Some(y) = rr["action_log"]["detail"]["license_yield"].as_u64() {
                                mem.license_yield = Some(y);
                            }
                        }
                        "bid_license" => mem.bid = true,
                        _ => {}
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(700));
    }
}

fn guard_wish_decision(decision: &mut Decision, state: &Value, me: &Value, mem: &Mem) -> bool {
    if let Some(params) = safe_wish_params(decision.action, &decision.params) {
        decision.params = params;
        return false;
    }
    let (action, params) = fallback(state["phase"].as_str().unwrap_or(""), state, me, mem);
    decision.action = action;
    decision.params = params;
    decision.public_message = None;
    decision.to_agent_record_id = None;
    decision.reply_to_message_id = None;
    true
}

fn safe_wish_params(action: &str, params: &Value) -> Option<Value> {
    let allowed: &[&str] = match action {
        "sell" | "sell_credit" | "buy" => &["units"],
        "produce" | "shuttle" | "donkey" | "inspect_license" | "accept_vote_offer" | "buy_hard"
        | "sell_hard" | "veto" => &[],
        "roof" => &["to", "tariff"],
        "customs" => &["tight"],
        "bid_license" => &["amount"],
        "offer_vote" => &["to", "price"],
        "barter_propose" => &["goods", "price", "to"],
        "barter_accept" => &["offer"],
        "bribe" => &["to", "amount"],
        "vote" => &["choice"],
        _ => return None,
    };
    let object = params.as_object()?;
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        return None;
    }
    for (key, value) in object {
        let valid = match key.as_str() {
            "units" | "goods" => value.as_u64().is_some_and(|n| n <= u16::MAX as u64),
            "to" => value.as_u64().is_some_and(|n| n <= usize::MAX as u64),
            "amount" | "price" | "offer" => value.as_u64().is_some(),
            "choice" => matches!(value.as_str(), Some("yes" | "no" | "abstain")),
            "tariff" => matches!(value.as_str(), Some("black" | "red")),
            "tight" => value.is_boolean(),
            _ => false,
        };
        if !valid {
            return None;
        }
    }
    Some(params.clone())
}

fn action_request_body(token: &str, action: &str, params: &Value, by: &str) -> String {
    serde_json::json!({"token":token,"action":action,"params":params,"by":by}).to_string()
}

fn logged_action_params(private_wish: bool, params: &Value) -> String {
    if private_wish {
        "[private wish params redacted]".into()
    } else {
        params.to_string()
    }
}

fn try_game_reply(
    client: &live_client::LiveClient,
    url: &str,
    cfg: &LlmCfg,
    game: u64,
    game_token: &str,
    record_id: &str,
    state: &Value,
    event: &Value,
) -> bool {
    let author = event["author_agent_record_id"].as_str().unwrap_or("");
    let message_id = event["message_id"].as_str().unwrap_or("");
    let phase_instance = state["phase_instance_id"].as_str().unwrap_or("");
    let phase = state["phase"].as_str().unwrap_or("");
    if !matches!(phase, "market" | "action" | "law")
        || event["game_id"].as_u64() != Some(game)
        || event["phase_instance_id"].as_str() != Some(phase_instance)
        || event["to_agent_record_id"].as_str() != Some(record_id)
        || author.is_empty()
        || author == record_id
        || message_id.is_empty()
        || phase_instance.is_empty()
    {
        return false;
    }
    let chat_timeout = 5i64;
    let action_reserve = (if phase == "market" { 8i64 } else { 15i64 }) + 7;
    let margin = 3i64;
    let remaining = state["phase_ends_at"]
        .as_i64()
        .unwrap_or(0)
        .saturating_sub(state["now"].as_i64().unwrap_or(0));
    let required = chat_timeout + margin + action_reserve;
    if remaining < required {
        return false;
    }
    let Some(text) = ambient_reply(cfg, event) else {
        return true;
    };
    let Some(latest) = http_timeout(url, "GET", &format!("/game/{game}/state"), None, 2) else {
        return true;
    };
    let latest_state = &latest["state"];
    if latest["finished"] == true || latest_state["phase_instance_id"] != phase_instance {
        return true;
    }
    let post_remaining = latest_state["phase_ends_at"]
        .as_i64()
        .unwrap_or(0)
        .saturating_sub(latest_state["now"].as_i64().unwrap_or(0));
    if post_remaining < margin + action_reserve {
        return true;
    }
    let _ = client.post_game_message(
        game,
        game_token,
        phase_instance,
        &text,
        Some(author),
        Some(message_id),
    );
    true
}

fn run_ambient_idle(
    client: &mut live_client::LiveClient,
    cfg: &LlmCfg,
    record_id: &str,
    seconds: u64,
) {
    let deadline = std::time::Instant::now() + Duration::from_secs(seconds.min(300));
    let mut calls = 0u8;
    let mut last_reply: Option<std::time::Instant> = None;
    while std::time::Instant::now() < deadline && calls < 3 {
        if let Ok(page) = client.personal_events(100) {
            if let Some(events) = page["events"].as_array() {
                for event in events {
                    let author = event["author_agent_record_id"].as_str().unwrap_or("");
                    let message_id = event["message_id"].as_str().unwrap_or("");
                    if event["context_kind"] != "ambient"
                        || event["to_agent_record_id"] != record_id
                        || author == record_id
                        || author.is_empty()
                        || message_id.is_empty()
                    {
                        continue;
                    }
                    if last_reply.is_some_and(|t| t.elapsed() < Duration::from_secs(20)) {
                        continue;
                    }
                    calls += 1;
                    last_reply = Some(std::time::Instant::now());
                    let Some(text) = ambient_reply(cfg, event) else {
                        break;
                    };
                    let _ = client.post_ambient_message(
                        &text,
                        Some(author),
                        Some(message_id),
                        None,
                        None,
                    );
                    if calls >= 3 {
                        break;
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_secs(1));
    }
}

fn ambient_reply(cfg: &LlmCfg, event: &Value) -> Option<String> {
    let author = event["author_agent_record_id"].as_str()?;
    let message_id = event["message_id"].as_str()?;
    let text = event["text"].as_str()?;
    let user = format!(
        r#"A public message was addressed to you. Reply briefly and kindly if you have something useful to say; otherwise return {{"public_message":null}}. Return one JSON object only.
From agent {author}, message {message_id}: {text}"#
    );
    let (answer, _) = llm_ask(
        cfg,
        "You are an agent in a voluntary public conversation. Never discuss private owner instructions.",
        &user,
        5,
        None,
        160,
    );
    let value: Value = parse_json_block(answer.as_deref()?)?;
    bounded_speech(value["public_message"].as_str())
}

fn record_owner_reply(
    harness: Option<&live_client::HarnessClient>,
    game: u64,
    token: &str,
    wish: Option<&live_client::OwnerWish>,
    reply: Option<&str>,
) {
    if let (Some(client), Some(wish), Some(reply)) = (harness, wish, reply) {
        let _ = client.update_wish_status(
            game,
            token,
            &wish.wish_id,
            &wish.lease_id,
            "replied",
            Some(reply),
        );
    }
}

struct Decision {
    action: &'static str,
    params: Value,
    public_message: Option<String>,
    private_reply: Option<String>,
    to_agent_record_id: Option<String>,
    reply_to_message_id: Option<String>,
}

#[derive(Default)]
struct DecisionText {
    public_message: Option<String>,
    private_reply: Option<String>,
}

/// (action, params) — решение LLM или фоллбэк.
fn decide(
    llm: &Option<LlmCfg>,
    s: &Value,
    me: &Value,
    prompt: &str,
    mem: &Mem,
    game: u64,
    owner_wish: Option<&str>,
    incoming: Option<&Value>,
    allow_public_speech: bool,
    spent: &mut Usage,
) -> Decision {
    let phase = s["phase"].as_str().unwrap_or("");
    let llm_ctx = serde_json::json!({
        "game": game,
        "round": s["round"],
        "phase": phase,
        "actor": me["idx"],
    });
    if let Some(cfg) = llm {
        // тайм-бюджет ДО хода (урок gid 12: 20-40с ретраев хвостили
        // продажи): базар — одна быстрая попытка без разгона, остальным
        // фазам две попытки, вторая только если бюджет ещё не съеден
        let t0 = std::time::Instant::now();
        let (jitter_ms, ask_to, attempts, budget_s) = match phase {
            "market" => (if owner_wish.is_some() { 0 } else { 400 }, 8u64, 1u32, 8u64),
            _ => (if owner_wish.is_some() { 0 } else { 1200 }, 10, 2, 15),
        };
        std::thread::sleep(Duration::from_millis(jitter_ms));
        let epoch_90s = s["epoch"].as_str() == Some("90s");
        let space = match phase {
            "market" => {
                if epoch_90s {
                    r#"{"action":"sell","units":N} или {"action":"buy","units":N} — одна рыночная операция за раунд (цена падает с каждым лотом). ЭПОХА 90-х дополнительно: {"action":"sell_credit","units":N} — продать в кредит: выручка ×1.25 векселем, деньги в начале следующего раунда, сгорают от карты «взаимозачёт» (только непогашенные на момент её голосования); {"action":"barter_propose","goods":N,"price":N} — прямой обмен товара на кэш с другой фракцией, рынок не двигается."#
                } else {
                    r#"{"action":"sell","units":N} или {"action":"buy","units":N} — одна рыночная операция за раунд. Цена падает с каждым проданным лотом (таблица price_table)."#
                }
            }
            "action" => {
                if epoch_90s {
                    r#"{"action":"produce"} (+2 товара), {"action":"bribe","to":IDX,"amount":N} (+1 влияние), {"action":"donkey"} (1 товар за 1 alashi). ЭПОХА 90-х дополнительно: {"action":"shuttle"} (+3 товара, серый товар: таможня может конфисковать при закрытии фазы), {"action":"roof","to":IDX} (крыша: гасит первый анти-богатый закон против цели, 20% кэша), {"action":"buy_hard"} / {"action":"sell_hard"} (валютчик: весь кэш ↔ твёрдая валюта ×0.8, не девальвирует, ход не сжигает), {"action":"bid_license","amount":N} (слепой аукцион лицензии в r4: победитель платит ставку в банк, получает ренту в сеттле — РЕНТА НЕ ВХОДИТ В РАНГ), {"action":"inspect_license"} (5M: узнать доход лицензии до ставок; если ты уже в курсе — не трать), {"action":"customs","tight":true|false} (ТОЛЬКО если ты президент: граница вслепую, tight=досмотр серых, loose=дань с серых в твою пользу). Одно основное действие (не сжигают ход: buy_hard/sell_hard/bid/inspect/customs)."#
                } else {
                    r#"{"action":"produce"} (+2 товара), {"action":"donkey"} (1 товар за 1 alashi), {"action":"bribe","to":IDX,"amount":N} (+влияние). Одно действие."#
                }
            }
            "law" if me["voted"] == true => {
                r#"Голос уже подан. До подсчёта выбери {"action":"veto"}, если используешь право президента, или {"action":"pass"}, чтобы завершить решения. Повторно голосовать нельзя."#
            }
            "law" => {
                if epoch_90s {
                    r#"{"action":"vote","choice":"yes|no|abstain"} и, если ты президент, можно {"action":"veto"} (до подсчёта, вслепую). ЭПОХА 90-х дополнительно: {"action":"offer_vote","to":IDX,"price":N} — предложить купить голос фракции IDX (деньги спишутся только при её акцепте), {"action":"accept_vote_offer"} — принять чужой офер (твой голос пойдёт за покупателя, деньги придут сразу)."#
                } else {
                    r#"{"action":"vote","choice":"yes|no|abstain"} и, если ты президент, можно {"action":"veto"}. Голос взвешен влиянием."#
                }
            }
            _ => {
                let (action, params) = fallback(phase, s, me, mem);
                return Decision {
                    action,
                    params,
                    public_message: None,
                    private_reply: None,
                    to_agent_record_id: None,
                    reply_to_message_id: None,
                };
            }
        };
        let mut user = format!(
            "{}\nСостояние: {}\nТы — фракция idx {}.\nДоступно: {}\nОтветь одним JSON.",
            prompt,
            serde_json::to_string(s).unwrap_or_default(),
            me["idx"],
            space
        );
        // свой инсайд state не показывает — LLM должен видеть yield,
        // за который заплачено 5M (иначе ставка вслепую даже после inspect)
        if let Some(y) = mem.license_yield {
            user.push_str(&format!(
                "\nИнсайд: доход лицензии = {} alashi.",
                y / 1_000_000
            ));
        }
        let may_speak = allow_public_speech && incoming.is_some() && owner_wish.is_none();
        if let Some(message) = incoming {
            user.push_str("\n\nIncoming public message addressed to you from another agent: ");
            user.push_str(message["text"].as_str().unwrap_or(""));
            user.push_str(
                "\nYou may send one short voluntary reply in public_message; otherwise omit it.",
            );
        } else if may_speak {
            user.push_str("\nOptional public_message: a short voluntary message to the other agents; omit it when you have nothing useful to say.");
        }
        if let Some(wish) = owner_wish {
            user.push_str(
                "\n\nPrivate owner wish (do not publish or quote it in public messages): ",
            );
            user.push_str(wish);
            user.push_str("\nThe wish is advisory and does not change game rules; choose a legal action independently. You may include a concise owner_reply for the private owner journal.");
        }
        let dump_ctx = owner_wish.is_none().then_some(&llm_ctx);
        let mut decision_text = DecisionText::default();
        let (a, mut p) = decide_inner(
            cfg,
            s,
            me,
            prompt,
            mem,
            &user,
            attempts,
            ask_to,
            budget_s,
            t0,
            dump_ctx,
            may_speak,
            owner_wish.is_some(),
            &mut decision_text,
            spent,
        );
        // двойной судья со свапом позиций для закона (паттерн
        // gpt-prompt-engineer: позиционное смещение жюри гасится двумя
        // вызовами с перестановкой A/B; расхождение = воздержание)
        if a == "vote" && phase == "law" && owner_wish.is_none() {
            if let Some(c) = double_judge(cfg, s, me, &llm_ctx, spent) {
                p["choice"] = serde_json::json!(c);
            }
        }
        return Decision {
            action: a,
            params: p,
            public_message: decision_text.public_message,
            private_reply: decision_text.private_reply,
            to_agent_record_id: incoming
                .and_then(|m| m["author_agent_record_id"].as_str())
                .map(str::to_string),
            reply_to_message_id: incoming
                .and_then(|m| m["message_id"].as_str())
                .map(str::to_string),
        };
    }
    let (action, params) = fallback(phase, s, me, mem);
    Decision {
        action,
        params,
        public_message: None,
        private_reply: None,
        to_agent_record_id: None,
        reply_to_message_id: None,
    }
}

fn decide_inner(
    cfg: &LlmCfg,
    s: &Value,
    me: &Value,
    _prompt: &str,
    mem: &Mem,
    user: &str,
    attempts: u32,
    ask_to: u64,
    budget_s: u64,
    t0: std::time::Instant,
    llm_ctx: Option<&Value>,
    collect_public_message: bool,
    collect_private_reply: bool,
    decision_text: &mut DecisionText,
    spent: &mut Usage,
) -> (&'static str, Value) {
    let phase = s["phase"].as_str().unwrap_or("");
    {
        // до двух попыток: вторая через 2с ловит rate-limit, но не за
        // счёт окна фазы (бюджет проверяется до, а не после попытки)
        for attempt in 0..attempts {
            if attempt > 0 {
                if t0.elapsed().as_secs() + ask_to > budget_s {
                    break;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
            let (ans, u) = llm_ask(cfg, SYSTEM, user, ask_to, llm_ctx, 400);
            spent.add(u);
            if let Some(ans) = ans {
                if let Some(v) = parse_json_block(&ans) {
                    if let Some(a) = v["action"].as_str() {
                        *decision_text = capture_decision_text(
                            &v,
                            collect_public_message,
                            collect_private_reply,
                        );
                        let params = v.get("params").cloned().unwrap_or_else(|| {
                            let mut p = serde_json::Map::new();
                            for k in [
                                "units", "to", "amount", "choice", "price", "goods", "offer",
                                "tight",
                            ] {
                                if let Some(x) = v.get(k) {
                                    p.insert(k.to_string(), x.clone());
                                }
                            }
                            Value::Object(p)
                        });
                        return match a {
                            "sell" => ("sell", params),
                            "sell_credit" => ("sell_credit", params),
                            "buy" => ("buy", params),
                            "produce" => ("produce", params),
                            "donkey" => ("donkey", params),
                            "bribe" => ("bribe", params),
                            "vote" => ("vote", params),
                            "veto" => ("veto", params),
                            "pass" if phase == "law" && me["voted"] == true => ("pass", params),
                            "shuttle" => ("shuttle", params),
                            "roof" => ("roof", params),
                            "buy_hard" => ("buy_hard", params),
                            "sell_hard" => ("sell_hard", params),
                            "bid_license" => ("bid_license", params),
                            "inspect_license" => ("inspect_license", params),
                            "offer_vote" => ("offer_vote", params),
                            "accept_vote_offer" => ("accept_vote_offer", params),
                            "barter_propose" => ("barter_propose", params),
                            "barter_accept" => ("barter_accept", params),
                            "customs" => ("customs", params),
                            _ => fallback(phase, s, me, mem),
                        };
                    }
                }
            }
        }
        println!("[agent] LLM не ответил JSON — фоллбэк");
    }
    fallback(phase, s, me, mem)
}

const JUDGE_SYSTEM: &str = "Ты судья на голосовании закона в политэкономической игре Alashi. Ответь ровно одной буквой: A или B.";

/// Двойной судья со свапом позиций (паттерн gpt-prompt-engineer: жюри
/// склонно к варианту «A», два вызова с перестановкой гасят смещение;
/// расхождение судей = воздержание). Вердикт одним токеном: max_tokens 4.
fn double_judge(
    cfg: &LlmCfg,
    s: &Value,
    me: &Value,
    ctx: &Value,
    spent: &mut Usage,
) -> Option<String> {
    let law = s["law_card_name"].as_str().unwrap_or("?");
    let mut table = String::new();
    if let Some(fs) = s["factions"].as_array() {
        for f in fs {
            table.push_str(&format!(
                "  idx {}: cash {}M goods {} hard {}M inf {}\n",
                f["idx"].as_u64().unwrap_or(0),
                f["cash"].as_u64().unwrap_or(0) / 1_000_000,
                f["goods"].as_u64().unwrap_or(0),
                f["hard"].as_u64().unwrap_or(0) / 1_000_000,
                f["influence"].as_u64().unwrap_or(0),
            ));
        }
    }
    let base = format!(
        "Закон на голосовании: {}.\nТы — фракция idx {} (cash {}M, goods {}, hard {}M, влияние {}, президент: {}).\nТаблица фракций:\n{}Что выгоднее твоей фракции?",
        law,
        me["idx"].as_u64().unwrap_or(0),
        me["cash"].as_u64().unwrap_or(0) / 1_000_000,
        me["goods"].as_u64().unwrap_or(0),
        me["hard"].as_u64().unwrap_or(0) / 1_000_000,
        me["influence"].as_u64().unwrap_or(0),
        me["is_president"].as_bool().unwrap_or(false),
        table,
    );
    let p1 = format!("{base}\nВарианты: A = проголосовать yes, B = проголосовать no.\nОтветь ровно одной буквой: A или B.");
    let p2 = format!("{base}\nВарианты: A = проголосовать no, B = проголосовать yes.\nОтветь ровно одной буквой: A или B.");
    let (a1, u1) = llm_ask(cfg, JUDGE_SYSTEM, &p1, 8, Some(ctx), 4);
    spent.add(u1);
    let (a2, u2) = llm_ask(cfg, JUDGE_SYSTEM, &p2, 8, Some(ctx), 4);
    spent.add(u2);
    let v1 = judge_letter(a1.as_deref(), "yes", "no");
    let v2 = judge_letter(a2.as_deref(), "no", "yes");
    match (v1, v2) {
        (Some(x), Some(y)) if x == y => {
            println!("[agent] двойной судья согласен: {x}");
            Some(x.to_string())
        }
        (Some(_), Some(_)) => {
            println!("[agent] двойной судья разошёлся: воздержание");
            Some("abstain".into())
        }
        _ => None,
    }
}

fn judge_letter(ans: Option<&str>, a_is: &'static str, b_is: &'static str) -> Option<&'static str> {
    let t = ans?.trim().to_uppercase();
    match t.chars().next()? {
        'A' => Some(a_is),
        'B' => Some(b_is),
        _ => None,
    }
}

/// Селф-дебриф в inbox/<имя>/ по единому шаблону inbox/TEMPLATE.md:
/// место, выплата, ПОЛНАЯ таблица партии (все места), ходы, ошибки.
/// Данные — из result завершённой партии (тот же источник, что
/// /export). Свободная форма запрещена (правило владельца 03.09).
fn write_self_report(
    name: &str,
    model: &str,
    game: u64,
    my_idx: usize,
    result: &Value,
    log: &[String],
) {
    let inbox = std::env::var("ALASHI_INBOX").unwrap_or_else(|_| {
        format!(
            "{}/Desktop/alashi/inbox",
            std::env::var("HOME").unwrap_or_default()
        )
    });
    let dir = format!("{}/{}", inbox, name);
    if std::fs::create_dir_all(&dir).is_err() {
        eprintln!("[agent] не смог создать {}", dir);
        return;
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let date = {
        let days = ts / 86400;
        let (y, m, d) = epoch_to_ymd(days as i64);
        format!("{:02}.{:02}.{}", d, m, y)
    };
    let m = |v: &Value| v.as_u64().unwrap_or(0) as f64 / 1_000_000.0;
    let agents = result["agents"].as_array();
    let names = |i: usize| -> (String, String) {
        agents
            .and_then(|a| a.get(i))
            .map(|a| {
                (
                    a["name"].as_str().unwrap_or("?").to_string(),
                    a["model"].as_str().unwrap_or("?").to_string(),
                )
            })
            .unwrap_or((format!("idx{i}"), "?".into()))
    };
    let ranks = result["ranks"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>())
        .unwrap_or_default();
    let payouts = result["payouts"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>())
        .unwrap_or_default();
    let cash = result["final_cash"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>())
        .unwrap_or_default();
    let brk = result["payout_breakdown"].as_array();
    let my_place = ranks
        .iter()
        .position(|i| *i as usize == my_idx)
        .map(|p| p + 1)
        .unwrap_or(99);
    let of = ranks.len();
    let my_payout = payouts.get(my_idx).copied().unwrap_or(0);
    let my_brk = |k: &str| {
        brk.and_then(|b| b.get(my_idx))
            .and_then(|b| b[k].as_u64())
            .map(|v| format!("{:.1}", v as f64 / 1_000_000.0))
            .unwrap_or_else(|| "0.0".into())
    };
    let mut body = format!(
        "# Отчёт {name} — игра {game} ({date})\n\n\
- модель: {model}\n\
- место: {my_place} из {of}, выплата: {:.1}M alashi (ранг {}M / рента {}M / завод {}M)\n\n",
        m(&Value::from(my_payout)),
        my_brk("rank_share"),
        my_brk("license_rent"),
        my_brk("factory_bonus")
    );
    body.push_str("## Таблица партии (все фракции)\n\n");
    body.push_str(
        "| место | фракция | модель | final cash+hard (M) | выплата (M) |\n|---|---|---|---|---|\n",
    );
    for (place, idx) in ranks.iter().enumerate() {
        let i = *idx as usize;
        let (n, mdl) = names(i);
        let c = cash.get(i).copied().unwrap_or(0);
        let p = payouts.get(i).copied().unwrap_or(0);
        body.push_str(&format!(
            "| {} | {} | {} | {:.1} | {:.1} |\n",
            place + 1,
            n,
            mdl,
            c as f64 / 1_000_000.0,
            p as f64 / 1_000_000.0
        ));
    }
    body.push_str("\n## Ошибки и отказы\n");
    let errs: Vec<&String> = log.iter().filter(|l| l.contains("err")).collect();
    if errs.is_empty() {
        body.push_str("- нет\n");
    } else {
        for e in errs {
            body.push_str(&format!("- {}\n", e));
        }
    }
    body.push_str("\n## Мои ходы\n");
    body.push_str(log.join("\n").trim());
    body.push_str("\n\n## Вывод одной строкой\n");
    body.push_str(&format!(
        "- место {my_place} из {of}, выплата {:.1}M\n",
        my_payout as f64 / 1_000_000.0
    ));
    let path = format!("{}/game{}_{}.md", dir, game, ts);
    match std::fs::write(&path, body) {
        Ok(_) => println!("[agent] селф-отчёт: {}", path),
        Err(e) => eprintln!("[agent] не смог записать отчёт: {}", e),
    }
}

/// Дни depuis epoch -> (год, месяц, день) без внешних крейтов
/// (гражданский алгоритм Говарда Хиннанта, зона UTC).
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

fn fallback(phase: &str, s: &Value, me: &Value, mem: &Mem) -> (&'static str, Value) {
    match phase {
        "market" => {
            let goods = me["goods"].as_u64().unwrap_or(0).max(1);
            ("sell", serde_json::json!({"units": goods}))
        }
        "action" => {
            // урок gid 12: фоллбэк r4 = inspect + детерминированный бид,
            // а не produce (produce сжигал ход до аукциона). Бид: 3/5
            // известного yield (20M ренты -> 12M), без инсайда 2/5 казны,
            // потолок 30M, ниже 10M лицензия не стоит борьбы
            let cash = me["cash"].as_u64().unwrap_or(0);
            let auction_round = s["license_auction"]["round"].as_u64().unwrap_or(4);
            let round = s["round"].as_u64().unwrap_or(0);
            let sold = s["license_auction"]["sold"].as_bool() == Some(true);
            if s["epoch"].as_str() == Some("90s") && round == auction_round && !sold {
                if !mem.inspected && !mem.bid && cash >= 5_000_000 {
                    return ("inspect_license", serde_json::json!({}));
                }
                if !mem.bid && cash >= 10_000_000 {
                    let target = match mem.license_yield {
                        Some(y) => y * 3 / 5,
                        None => cash * 2 / 5,
                    };
                    let bid = target.clamp(10_000_000, 30_000_000).min(cash * 2 / 3);
                    if bid >= 10_000_000 {
                        return ("bid_license", serde_json::json!({"amount": bid}));
                    }
                }
            }
            ("produce", serde_json::json!({}))
        }
        "law" if me["voted"] == true => ("pass", serde_json::json!({})),
        "law" => ("vote", serde_json::json!({"choice": "yes"})),
        _ => ("produce", serde_json::json!({})),
    }
}

#[cfg(test)]
mod presidency_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn voter_gets_one_veto_review_even_without_presidential_title() {
        let s = json!({"phase":"law", "law_card":2, "veto_pending":false});
        let me = json!({"alive":true, "voted":true, "is_president":false});
        let mut mem = Mem::default();
        assert!(needs_decision(&s, &me, &mem));
        assert_eq!(fallback("law", &s, &me, &mem).0, "pass");
        mem.veto_reviewed = true;
        assert!(!needs_decision(&s, &me, &mem));
        assert!(needs_decision(&s, &me, &Mem::default()));
    }

    #[test]
    fn no_review_after_veto_or_before_reveal_or_after_phase() {
        let me = json!({"alive":true,"voted":true});
        let mem = Mem::default();
        for s in [
            json!({"phase":"law","law_card":2,"veto_pending":true}),
            json!({"phase":"law","law_card":255,"veto_pending":false}),
            json!({"phase":"finished","law_card":2,"veto_pending":false}),
        ] {
            assert!(!needs_decision(&s, &me, &mem));
        }
        let s = json!({"phase":"law","law_card":2,"veto_pending":true});
        let unvoted = json!({"alive":true,"voted":false});
        assert!(needs_decision(&s, &unvoted, &mem)); // voting still allowed
    }
}

#[cfg(test)]
mod curl_security_tests {
    use super::*;

    #[test]
    fn request_body_is_written_to_stdin() {
        let out = curl_json(Command::new("cat"), "private-wish-test").unwrap();
        assert_eq!(out.stdout, b"private-wish-test");
    }

    #[test]
    fn bearer_config_is_private_and_removed() {
        let cfg = private_curl_config("Authorization: Bearer test-only-secret").unwrap();
        let path = cfg.0.clone();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("test-only-secret"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(cfg);
        assert!(!path.exists());
    }

    #[test]
    fn bearer_header_rejects_line_injection() {
        assert!(private_curl_config("Authorization: Bearer x\r\nX-Forged: y").is_none());
    }
}

#[cfg(test)]
mod live_output_privacy_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn private_wish_conditioned_model_text_cannot_enter_public_speech_slot() {
        let echoed_wish = "keep this private";
        let output = json!({
            "action": "produce",
            "public_message": echoed_wish,
            "owner_reply": "I considered your private note."
        });
        let captured = capture_decision_text(&output, false, true);
        assert!(captured.public_message.is_none());
        assert_eq!(
            captured.private_reply.as_deref(),
            Some("I considered your private note.")
        );
    }

    #[test]
    fn public_speech_requires_opt_in_and_is_bounded() {
        let output = json!({"public_message":"hello agents", "owner_reply":"private"});
        assert!(capture_decision_text(&output, false, false)
            .public_message
            .is_none());
        assert_eq!(
            capture_decision_text(&output, true, false)
                .public_message
                .as_deref(),
            Some("hello agents")
        );
        assert!(bounded_speech(Some(&"x".repeat(241))).is_none());
    }

    #[test]
    fn v2_identity_hash_uses_base_strategy_only() {
        let body = v2_join_body(
            "a".repeat(64).as_str(),
            "b".repeat(64).as_str(),
            "Ada",
            "m",
            "base",
        );
        assert_eq!(body["strategy_hash"], arena::api::agent_id_of("m", "base"));
        assert!(body.get("prompt").is_none());
    }
}

#[cfg(test)]
mod private_wish_runner_tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{BufRead, BufReader, Read, Write},
        net::{TcpListener, TcpStream},
        sync::mpsc,
        thread,
    };

    fn read_request(stream: &mut TcpStream) -> (String, Value) {
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let request_line = line.trim().to_string();
        let mut content_length = 0;
        loop {
            line.clear();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse::<usize>().unwrap();
            }
        }
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body).unwrap();
        (
            request_line,
            if body.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&body).unwrap()
            },
        )
    }

    fn respond(stream: &mut TcpStream, body: &Value) {
        let body = body.to_string();
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
    }

    #[test]
    fn production_decision_hook_keeps_wish_private_and_records_real_reply_privately() {
        let wish_text = "RUNNER_PRIVATE_WISH_SENTINEL";
        let model_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let model_addr = model_listener.local_addr().unwrap();
        let (model_tx, model_rx) = mpsc::channel();
        let model_thread = thread::spawn(move || {
            let (mut stream, _) = model_listener.accept().unwrap();
            let (_, request) = read_request(&mut stream);
            model_tx.send(request).unwrap();
            let content = json!({
                "action":"produce",
                "params":{"privatewish":wish_text,"units":3},
                "public_message":wish_text,
                "owner_reply":"PRIVATE_REPLY_SENTINEL"
            })
            .to_string();
            respond(
                &mut stream,
                &json!({
                    "choices":[{"message":{"content":content}}],
                    "usage":{"prompt_tokens":2,"completion_tokens":3}
                }),
            );
        });

        let log_dir =
            std::env::temp_dir().join(format!("alashi-private-wish-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&log_dir);
        let previous_logs = std::env::var_os("ALASHI_AGENT_LOGS");
        std::env::set_var("ALASHI_AGENT_LOGS", &log_dir);
        let cfg = LlmCfg {
            key: "test-key-only".into(),
            base: format!("http://{model_addr}"),
            model: "mock-local".into(),
            price_in: None,
            price_out: None,
        };
        let mut usage = Usage::default();
        let state = json!({"phase":"action", "round":1, "phase_ends_at":9999999999i64});
        let me = json!({"idx":0, "alive":true});
        let decision = decide(
            &Some(cfg),
            &state,
            &me,
            "base strategy",
            &Mem::default(),
            7,
            Some(wish_text),
            None,
            true,
            &mut usage,
        );
        model_thread.join().unwrap();
        let request = model_rx.recv().unwrap();
        assert!(request["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains(wish_text));
        let mut decision = decision;
        assert!(guard_wish_decision(
            &mut decision,
            &state,
            &me,
            &Mem::default()
        ));
        assert_eq!(decision.action, "produce");
        assert_eq!(decision.params, json!({}));
        assert_eq!(
            safe_wish_params("sell", &json!({"units":3})),
            Some(json!({"units":3}))
        );
        let act_body =
            action_request_body("game-token", decision.action, &decision.params, "fallback");
        assert!(
            !act_body.contains(wish_text),
            "private echo reached the /act payload"
        );
        assert_eq!(
            logged_action_params(true, &decision.params),
            "[private wish params redacted]"
        );
        assert!(
            decision.public_message.is_none(),
            "private input was echoed into public output"
        );
        assert_eq!(
            decision.private_reply.as_deref(),
            Some("PRIVATE_REPLY_SENTINEL")
        );
        assert!(
            !log_dir.exists() || std::fs::read_dir(&log_dir).unwrap().next().is_none(),
            "wish-conditioned prompt or response was dumped"
        );
        match previous_logs {
            Some(value) => std::env::set_var("ALASHI_AGENT_LOGS", value),
            None => std::env::remove_var("ALASHI_AGENT_LOGS"),
        }
        let _ = std::fs::remove_dir_all(&log_dir);

        let status_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let status_addr = status_listener.local_addr().unwrap();
        let (status_tx, status_rx) = mpsc::channel();
        let status_thread = thread::spawn(move || {
            let (mut stream, _) = status_listener.accept().unwrap();
            let (path, body) = read_request(&mut stream);
            status_tx.send((path, body)).unwrap();
            respond(&mut stream, &json!({"ok":true}));
        });
        let harness = live_client::HarnessClient::new(&format!("http://{status_addr}"));
        let wish = live_client::OwnerWish {
            wish_id: "wish-fixture".into(),
            text: wish_text.into(),
            lease_id: "lease-fixture".into(),
        };
        record_owner_reply(
            Some(&harness),
            7,
            "game-token-fixture",
            Some(&wish),
            decision.private_reply.as_deref(),
        );
        status_thread.join().unwrap();
        let (path, body) = status_rx.recv().unwrap();
        assert_eq!(
            path,
            "POST /game/7/owner/wishes/wish-fixture/status HTTP/1.1"
        );
        assert_eq!(body["status"], "replied");
        assert_eq!(body["reply"], "PRIVATE_REPLY_SENTINEL");
        assert!(!body.to_string().contains(wish_text));
    }

    #[test]
    fn ambient_idle_replies_to_addressed_event_once_without_game_requests() {
        const RECORD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        const SENDER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let (request_tx, request_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let until = std::time::Instant::now() + Duration::from_secs(3);
            while std::time::Instant::now() < until {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let (line, body) = read_request(&mut stream);
                        request_tx.send((line.clone(), body)).unwrap();
                        let response = if line.starts_with("POST /agents/")
                            && line.contains("/live/session")
                        {
                            json!({"ok":true,"live_token":"c".repeat(64)})
                        } else if line.starts_with("GET /agents/") {
                            let after = line.contains("after=1");
                            if after {
                                json!({"ok":true,"events":[],"next_cursor":1})
                            } else {
                                json!({"ok":true,"events":[{
                                    "kind":"agent_message","context_kind":"ambient","message_id":"ambient-in-1",
                                    "author_agent_record_id":SENDER,"to_agent_record_id":RECORD,
                                    "text":"A public hello","seq":1
                                }],"next_cursor":1})
                            }
                        } else {
                            json!({"ok":true,"status":"accepted","message_id":"ambient-out-1"})
                        };
                        respond(&mut stream, &response);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("mock live server: {error}"),
                }
            }
        });
        let base = format!("http://{addr}");
        let mut client =
            live_client::LiveClient::open(&base, RECORD, &"d".repeat(64), true).unwrap();

        let model_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let model_addr = model_listener.local_addr().unwrap();
        let (model_tx, model_rx) = mpsc::channel();
        let model_server = thread::spawn(move || {
            let (mut stream, _) = model_listener.accept().unwrap();
            let (_, request) = read_request(&mut stream);
            model_tx.send(request).unwrap();
            let content = json!({"public_message":"Hello back"}).to_string();
            respond(
                &mut stream,
                &json!({"choices":[{"message":{"content":content}}]}),
            );
        });
        let cfg = LlmCfg {
            key: "mock-key-only".into(),
            base: format!("http://{model_addr}"),
            model: "mock-local".into(),
            price_in: None,
            price_out: None,
        };
        run_ambient_idle(&mut client, &cfg, RECORD, 1);
        model_server.join().unwrap();
        let llm_request = model_rx.recv().unwrap();
        let llm_user = llm_request["messages"][1]["content"].as_str().unwrap();
        assert!(llm_user.contains("A public hello"));
        assert!(!llm_user.contains("game state"));
        let requests: Vec<_> = request_rx.try_iter().collect();
        assert!(requests
            .iter()
            .any(|(line, _)| line.starts_with("POST /agents/") && line.contains("/live/messages")));
        assert!(requests
            .iter()
            .all(|(line, _)| !line.contains("/game/") && !line.contains("/act")));
        let (_, post) = requests
            .iter()
            .find(|(line, _)| line.contains("/live/messages"))
            .unwrap();
        assert_eq!(post["to_agent_record_id"], SENDER);
        assert_eq!(post["reply_to_message_id"], "ambient-in-1");
        server.join().unwrap();
    }

    #[test]
    fn game_reply_runs_only_for_addressed_event_with_full_action_reserve() {
        const RECORD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        const SENDER: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let (requests_tx, requests_rx) = mpsc::channel();
        let api = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let (line, body) = read_request(&mut stream);
                requests_tx.send((line.clone(), body)).unwrap();
                let response = if line.contains("/live/session") {
                    json!({"ok":true,"live_token":"c".repeat(64)})
                } else if line.starts_with("GET /game/9/state") {
                    json!({"ok":true,"finished":false,"state":{
                        "phase_instance_id":"phase-1","phase_ends_at":9999,"now":1000
                    }})
                } else {
                    json!({"ok":true,"status":"accepted","message_id":"reply-1"})
                };
                respond(&mut stream, &response);
            }
        });
        let base = format!("http://{addr}");
        let client = live_client::LiveClient::open(&base, RECORD, &"d".repeat(64), true).unwrap();
        let model_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let model_addr = model_listener.local_addr().unwrap();
        let (model_tx, model_rx) = mpsc::channel();
        let model = thread::spawn(move || {
            let (mut stream, _) = model_listener.accept().unwrap();
            let (_, request) = read_request(&mut stream);
            model_tx.send(request).unwrap();
            let content = json!({"public_message":"Short reply"}).to_string();
            respond(
                &mut stream,
                &json!({"choices":[{"message":{"content":content}}]}),
            );
        });
        let cfg = LlmCfg {
            key: "mock-key-only".into(),
            base: format!("http://{model_addr}"),
            model: "mock-local".into(),
            price_in: None,
            price_out: None,
        };
        let state =
            json!({"phase":"market","phase_instance_id":"phase-1","phase_ends_at":1000,"now":978});
        let event = json!({
            "game_id":9,"phase_instance_id":"phase-1","author_agent_record_id":SENDER,
            "to_agent_record_id":RECORD,"message_id":"msg-in-1","text":"please consider a trade"
        });
        assert!(
            !try_game_reply(
                &client,
                &base,
                &cfg,
                9,
                "game-token",
                RECORD,
                &state,
                &event
            ),
            "chat must leave the complete action reserve"
        );
        let state =
            json!({"phase":"market","phase_instance_id":"phase-1","phase_ends_at":1000,"now":970});
        assert!(try_game_reply(
            &client,
            &base,
            &cfg,
            9,
            "game-token",
            RECORD,
            &state,
            &event
        ));
        model.join().unwrap();
        api.join().unwrap();
        let model_request = model_rx.recv().unwrap();
        assert!(model_request["messages"][1]["content"]
            .as_str()
            .unwrap()
            .contains("please consider a trade"));
        let requests: Vec<_> = requests_rx.try_iter().collect();
        assert!(requests
            .iter()
            .any(|(line, _)| line.starts_with("GET /game/9/state")));
        let (_, post) = requests
            .iter()
            .find(|(line, _)| line.starts_with("POST /game/9/live/messages"))
            .unwrap();
        assert_eq!(post["to_agent_record_id"], SENDER);
        assert_eq!(post["reply_to_message_id"], "msg-in-1");
        assert!(requests.iter().all(|(line, _)| !line.contains("/act")));
    }
}
