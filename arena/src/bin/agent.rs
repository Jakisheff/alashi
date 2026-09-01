//! agent: LLM-клиент HTTP-арены alashi. Без кошелька, без блокчейна:
//! заходит в партию, читает состояние, решает через LLM, действует.
//!
//!   agent --url http://127.0.0.1:8090 --game 1 --name Zhambyl \
//!         [--model glm-4.5-flash] [--prompt файл] [--no-llm]
//!
//! Ключ LLM: env ALASHI_LLM_KEY или ~/.config/alashi/llm.json
//! (как у ончейн-бота). Без ключа — жадный фоллбэк: продать всё /
//! произвести / голосовать за.

use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

// ---------- http-клиент (как в e2e-тесте) ----------

fn http(base: &str, method: &str, path: &str, body: Option<&str>) -> Option<Value> {
    let url = base.trim_start_matches("http://");
    let mut stream = TcpStream::connect(url).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .ok();
    let body = body.unwrap_or("");
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {url}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).ok()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    let at = text.find("\r\n\r\n")? + 4;
    serde_json::from_str(&text[at..]).ok()
}

// ---------- LLM (curl, как bots/llm.rs — ноль зависимостей) ----------

struct LlmCfg {
    key: String,
    base: String,
    model: String,
}

fn llm_cfg() -> Option<LlmCfg> {
    let key = std::env::var("ALASHI_LLM_KEY").ok().or_else(|| {
        let path = std::env::var("HOME").ok()? + "/.config/alashi/llm.json";
        let s = std::fs::read_to_string(path).ok()?;
        let k = serde_json::from_str::<Value>(&s).ok()?.get("key")?.as_str()?.to_string();
        Some(k)
    })?;
    if key.len() < 10 {
        return None;
    }
    Some(LlmCfg {
        key,
        base: "https://api.z.ai/api/paas/v4".into(),
        model: "glm-4.5-flash".into(),
    })
}

fn llm_ask(cfg: &LlmCfg, system: &str, user: &str) -> Option<String> {
    let body = serde_json::json!({
        "model": cfg.model,
        "thinking": {"type": "disabled"},
        "max_tokens": 400,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ]
    })
    .to_string();
    let out = std::process::Command::new("curl")
        .args([
            "-s", "-4", "-m", "14", "-X", "POST",
            &format!("{}/chat/completions", cfg.base),
            "-H", &format!("Authorization: Bearer {}", cfg.key),
            "-H", "Content-Type: application/json",
            "-d", &body,
        ])
        .output()
        .ok()?;
    let txt = String::from_utf8(out.stdout).ok()?;
    let v: Value = serde_json::from_str(&txt).ok()?;
    let c = v.get("choices")?.get(0)?.get("message")?.get("content")?.as_str()?.to_string();
    if c.trim().is_empty() { None } else { Some(c) }
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let url = flag(&args, "--url").unwrap_or_else(|| "http://127.0.0.1:8090".into());
    let name = flag(&args, "--name").unwrap_or_else(|| "Agent".into());
    let game: u64 = flag(&args, "--game").and_then(|g| g.parse().ok()).unwrap_or_else(|| {
        // без --game: свежайшая партия в лобби
        let g = http(&url, "GET", "/games", None).expect("арена недоступна");
        g["games"]
            .as_array()
            .and_then(|l| l.iter().rev().find(|g| g["phase"] == "lobby").or_else(|| l.last()))
            .and_then(|g| g["game_id"].as_u64())
            .expect("нет партий — создай POST /game/new")
    });
    let prompt = flag(&args, "--prompt")
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| "Стратег: играй рационально, следи за таблицей цен и влиянием.".into());
    let no_llm = args.iter().any(|a| a == "--no-llm");
    let llm = if no_llm { None } else { llm_cfg() };
    if llm.is_none() {
        println!("[agent] LLM-ключа нет — жадный фоллбэк");
    }

    let j = http(
        &url,
        "POST",
        &format!("/game/{}/join", game),
        Some(&serde_json::json!({"name": name, "model": "glm-agent", "prompt": prompt}).to_string()),
    )
    .expect("join");
    if j["ok"] != true {
        eprintln!("[ERROR] join: {}", j);
        std::process::exit(1);
    }
    let token = j["token"].as_str().unwrap().to_string();
    let my_idx = j["faction_idx"].as_u64().unwrap() as usize;
    println!("[agent] {} в игре {} (фракция {})", name, game, my_idx);

    let started = std::time::Instant::now();
    loop {
        if started.elapsed() > Duration::from_secs(600) {
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
            let ranks: Vec<u64> = res["ranks"].as_array().map(|a| a.iter().map(|v| v.as_u64().unwrap_or(9)).collect()).unwrap_or_default();
            let my_place = ranks.iter().position(|i| *i as usize == my_idx).map(|p| p + 1).unwrap_or(99);
            println!(
                "[agent] партия окончена: моё место {} из {}, выплата {} песо",
                my_place,
                ranks.len(),
                res["payouts"].as_array().and_then(|p| p.get(my_idx)).and_then(|v| v.as_u64()).unwrap_or(0) / 1_000_000
            );
            return;
        }
        let s = &r["state"];
        let phase = s["phase"].as_str().unwrap_or("lobby");
        let me = s["factions"]
            .as_array()
            .and_then(|f| f.iter().find(|f| f["idx"].as_u64() == Some(my_idx as u64)))
            .cloned()
            .unwrap_or(Value::Null);
        let need_act = match phase {
            "market" | "action" => me["acted"].as_bool() == Some(false),
            "law" => me["voted"].as_bool() == Some(false),
            _ => false,
        };
        if need_act {
            let phase_owned = phase.to_string();
            if phase_owned == "market" && me["goods"].as_u64().unwrap_or(0) == 0 {
                // нечего продавать и нечем купить (cash нет с r1) — пропускаем
                std::thread::sleep(Duration::from_millis(700));
                continue;
            }
            let decision = decide(&llm, s, &me, &prompt);
            let (action, params) = decision;
            let body = serde_json::json!({"token": token, "action": action, "params": params}).to_string();
            let rr = http(&url, "POST", &format!("/game/{}/act", game), Some(&body));
            if let Some(rr) = rr {
                if rr["ok"] != true {
                    println!("[agent] отказ: {} — пробую фоллбэк", rr["error"].as_str().unwrap_or("?"));
                    let (a2, p2) = fallback(phase, &me);
                    let body = serde_json::json!({"token": token, "action": a2, "params": p2}).to_string();
                    let _ = http(&url, "POST", &format!("/game/{}/act", game), Some(&body));
                }
            }
        }
        std::thread::sleep(Duration::from_millis(700));
    }
}

/// (action, params) — решение LLM или фоллбэк.
fn decide(llm: &Option<LlmCfg>, s: &Value, me: &Value, prompt: &str) -> (&'static str, Value) {
    let phase = s["phase"].as_str().unwrap_or("");
    if let Some(cfg) = llm {
        // разгона: три агента в одной фазе не должны бить API одновременно
        let jitter = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_millis() % 2000)
            .unwrap_or(0);
        std::thread::sleep(Duration::from_millis(u64::from(jitter)));
        let space = match phase {
            "market" => r#"{"action":"sell","units":N} или {"action":"buy","units":N} — одна рыночная операция за раунд. Цена падает с каждым проданным лотом (таблица price_table)."#,
            "action" => r#"{"action":"produce"} (+2 товара), {"action":"donkey"} (1 товар за 1 песо), {"action":"bribe","to":IDX,"amount":N} (+влияние). Одно действие."#,
            "law" => r#"{"action":"vote","choice":"yes|no|abstain"} и, если ты президент, можно {"action":"veto"}. Голос взвешен влиянием."#,
            _ => return fallback(phase, me),
        };
        let user = format!(
            "{}\nСостояние: {}\nТы — фракция idx {}.\nДоступно: {}\nОтветь одним JSON.",
            prompt,
            serde_json::to_string(s).unwrap_or_default(),
            me["idx"],
            space
        );
        // до двух попыток: вторая через 3с ловит rate-limit
        for attempt in 0..2 {
            if attempt > 0 {
                std::thread::sleep(Duration::from_secs(3));
            }
            if let Some(ans) = llm_ask(cfg, SYSTEM, &user) {
                if let Some(v) = parse_json_block(&ans) {
                    if let Some(a) = v["action"].as_str() {
                        let params = v.get("params").cloned().unwrap_or_else(|| {
                            let mut p = serde_json::Map::new();
                            for k in ["units", "to", "amount", "choice"] {
                                if let Some(x) = v.get(k) {
                                    p.insert(k.to_string(), x.clone());
                                }
                            }
                            Value::Object(p)
                        });
                        return match a {
                            "sell" => ("sell", params),
                            "buy" => ("buy", params),
                            "produce" => ("produce", params),
                            "donkey" => ("donkey", params),
                            "bribe" => ("bribe", params),
                            "vote" => ("vote", params),
                            "veto" => ("veto", params),
                            _ => fallback(phase, me),
                        };
                    }
                }
            }
        }
        println!("[agent] LLM не ответил JSON — фоллбэк");
    }
    fallback(phase, me)
}

fn fallback(phase: &str, me: &Value) -> (&'static str, Value) {
    match phase {
        "market" => {
            let goods = me["goods"].as_u64().unwrap_or(0).max(1);
            ("sell", serde_json::json!({"units": goods}))
        }
        "action" => ("produce", serde_json::json!({})),
        "law" => ("vote", serde_json::json!({"choice": "yes"})),
        _ => ("produce", serde_json::json!({})),
    }
}
