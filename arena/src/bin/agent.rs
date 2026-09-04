//! agent: LLM-клиент HTTP-арены alashi. Без кошелька, без блокчейна:
//! заходит в партию, читает состояние, решает через LLM, действует.
//!
//!   agent --url http://127.0.0.1:8090 --game 1 --name Zhambyl \
//!         [--model glm-4.5-flash] [--prompt файл] [--no-llm]
//!
//! Ключ LLM: env ALASHI_LLM_KEY или ~/.config/alashi/llm.json
//! (как у ончейн-бота). Без ключа — жадный фоллбэк: продать всё /
//! произвести / голосовать за. Эпоха 90-х: полный словарь M1-M11
//! (вексель, валютчик, крыша, челнок, таможня, лицензия, скупка
//! голосов, бартер). После партии пишет селф-дебриф в inbox/<имя>/
//! (подхватывает демон agent_inbox).

use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

// ---------- http-клиент (как в e2e-тесте) ----------

fn http(base: &str, method: &str, path: &str, body: Option<&str>) -> Option<Value> {
    let https = base.starts_with("https://");
    if https {
        // R17: клиент без TLS — по https подключится к 443 и упадёт
        // молча; говорим явно и отказываемся.
        eprintln!("[ERROR] клиент без TLS: используй http-адрес арены (для туннеля: локальный порт хоста)");
        return None;
    }
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

fn llm_cfg(model_override: Option<&str>) -> Option<LlmCfg> {
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
        model: model_override.unwrap_or("glm-4.5-flash").into(),
    })
}

fn llm_ask(cfg: &LlmCfg, system: &str, user: &str, timeout_s: u64) -> Option<String> {
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
            "-s", "-4", "-m", &timeout_s.to_string(), "-X", "POST",
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

/// Память раунда: bid/inspect не сжигают ход, acted остаётся false —
/// без флагов фоллбэк бидил бы и инспектил бы каждый тик (урок gid 12:
/// двойной бид Agent3 в gid 1). Сбрасывается на смене (round, phase).
#[derive(Default)]
struct Mem {
    inspected: bool,
    bid: bool,
    license_yield: Option<u64>,
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
    // R17: --model разбирается и уходит в join (иначе в лидерборд
    // попадала константа "glm-agent" при любой реальной модели).
    // R6-следование: дефолт уникален имени бота — два одинаковых
    // (model, prompt) это один agent_id => DuplicateWallet на join
    let model_flag = flag(&args, "--model");
    let declared_model = model_flag.clone().unwrap_or_else(|| format!("glm-agent-{name}"));
    let no_llm = args.iter().any(|a| a == "--no-llm");
    let llm = if no_llm { None } else { llm_cfg(model_flag.as_deref()) };
    if llm.is_none() {
        println!("[agent] LLM-ключа нет — жадный фоллбэк");
    }

    let j = http(
        &url,
        "POST",
        &format!("/game/{}/join", game),
        Some(&serde_json::json!({"name": name, "model": declared_model, "prompt": prompt}).to_string()),
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
    // селф-дебриф: каждый свой ход в журнал, после партии — в inbox
    let mut my_log: Vec<String> = vec![];
    let mut mem = Mem::default();
    let mut last_rf: Option<(u64, String)> = None;
    loop {
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
            let ranks: Vec<u64> = res["ranks"].as_array().map(|a| a.iter().map(|v| v.as_u64().unwrap_or(9)).collect()).unwrap_or_default();
            let my_place = ranks.iter().position(|i| *i as usize == my_idx).map(|p| p + 1).unwrap_or(99);
            let payout = res["payouts"].as_array().and_then(|p| p.get(my_idx)).and_then(|v| v.as_u64()).unwrap_or(0);
            println!(
                "[agent] партия окончена: моё место {} из {}, выплата {} песо",
                my_place,
                ranks.len(),
                payout / 1_000_000
            );
            return;
        }
        let s = &r["state"];
        let phase = s["phase"].as_str().unwrap_or("lobby");
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
        let need_act = match phase {
            "market" | "action" => me["acted"].as_bool() == Some(false),
            "law" => me["voted"].as_bool() == Some(false),
            _ => false,
        };
        if need_act {
            // честная очередь: джиттер перед ходом, чтобы внешние агенты
            // на опросе не проигрывали гонку серверным ботам (дебриф r3).
            // Базар — исключение: позиция продажи решает цену (урок gid 12).
            let jitter = if phase == "market" { 0 } else {
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
            let decision = decide(&llm, s, &me, &prompt, &mem);
            let (action, params) = decision;
            let body = serde_json::json!({
                "token": token, "action": action, "params": params,
                "by": if llm.is_some() { "llm" } else { "fallback" },
            }).to_string();
            let rr = http(&url, "POST", &format!("/game/{}/act", game), Some(&body));
            if let Some(rr) = rr {
                let ok_s = if rr["ok"] == true { "ok".into() } else { format!("err: {}", rr["error"].as_str().unwrap_or("?")) };
                my_log.push(format!(
                    "r{} {}: {} {} -> {}",
                    s["round"].as_u64().unwrap_or(0),
                    phase,
                    action,
                    params,
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
                        let body = serde_json::json!({
                            "token": token, "action": a2, "params": p2, "by": "fallback",
                        }).to_string();
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

/// (action, params) — решение LLM или фоллбэк.
fn decide(
    llm: &Option<LlmCfg>,
    s: &Value,
    me: &Value,
    prompt: &str,
    mem: &Mem,
) -> (&'static str, Value) {
    let phase = s["phase"].as_str().unwrap_or("");
    if let Some(cfg) = llm {
        // тайм-бюджет ДО хода (урок gid 12: 20-40с ретраев хвостили
        // продажи): базар — одна быстрая попытка без разгона, остальным
        // фазам две попытки, вторая только если бюджет ещё не съеден
        let t0 = std::time::Instant::now();
        let (jitter_ms, ask_to, attempts, budget_s) = match phase {
            "market" => (400, 8u64, 1u32, 8u64),
            _ => (1200, 10, 2, 15),
        };
        std::thread::sleep(Duration::from_millis(jitter_ms));
        let epoch_90s = s["epoch"].as_str() == Some("90s");
        let space = match phase {
            "market" => if epoch_90s {
                r#"{"action":"sell","units":N} или {"action":"buy","units":N} — одна рыночная операция за раунд (цена падает с каждым лотом). ЭПОХА 90-х дополнительно: {"action":"sell_credit","units":N} — продать в кредит: выручка ×1.25 векселем, деньги в начале следующего раунда, сгорают от карты «взаимозачёт» (только непогашенные на момент её голосования); {"action":"barter_propose","goods":N,"price":N} — прямой обмен товара на кэш с другой фракцией, рынок не двигается."#
            } else {
                r#"{"action":"sell","units":N} или {"action":"buy","units":N} — одна рыночная операция за раунд. Цена падает с каждым проданным лотом (таблица price_table)."#
            },
            "action" => if epoch_90s {
                r#"{"action":"produce"} (+2 товара), {"action":"bribe","to":IDX,"amount":N} (+1 влияние), {"action":"donkey"} (1 товар за 1 песо). ЭПОХА 90-х дополнительно: {"action":"shuttle"} (+3 товара, серый товар: таможня может конфисковать при закрытии фазы), {"action":"roof","to":IDX} (крыша: гасит первый анти-богатый закон против цели, 20% кэша), {"action":"buy_hard"} / {"action":"sell_hard"} (валютчик: весь кэш ↔ твёрдая валюта ×0.8, не девальвирует, ход не сжигает), {"action":"bid_license","amount":N} (слепой аукцион лицензии в r4: победитель платит ставку в банк, получает ренту в сеттле — РЕНТА НЕ ВХОДИТ В РАНГ), {"action":"inspect_license"} (5M: узнать доход лицензии до ставок; если ты уже в курсе — не трать), {"action":"customs","tight":true|false} (ТОЛЬКО если ты президент: граница вслепую, tight=досмотр серых, loose=дань с серых в твою пользу). Одно основное действие (не сжигают ход: buy_hard/sell_hard/bid/inspect/customs)."#
            } else {
                r#"{"action":"produce"} (+2 товара), {"action":"donkey"} (1 товар за 1 песо), {"action":"bribe","to":IDX,"amount":N} (+влияние). Одно действие."#
            },
            "law" => if epoch_90s {
                r#"{"action":"vote","choice":"yes|no|abstain"} и, если ты президент, можно {"action":"veto"} (до подсчёта, вслепую). ЭПОХА 90-х дополнительно: {"action":"offer_vote","to":IDX,"price":N} — предложить купить голос фракции IDX (деньги спишутся только при её акцепте), {"action":"accept_vote_offer"} — принять чужой офер (твой голос пойдёт за покупателя, деньги придут сразу)."#
            } else {
                r#"{"action":"vote","choice":"yes|no|abstain"} и, если ты президент, можно {"action":"veto"}. Голос взвешен влиянием."#
            },
            _ => return fallback(phase, s, me, mem),
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
            user.push_str(&format!("\nИнсайд: доход лицензии = {} песо.", y / 1_000_000));
        }
        // до двух попыток: вторая через 2с ловит rate-limit, но не за
        // счёт окна фазы (бюджет проверяется до, а не после попытки)
        for attempt in 0..attempts {
            if attempt > 0 {
                if t0.elapsed().as_secs() + ask_to > budget_s {
                    break;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
            if let Some(ans) = llm_ask(cfg, SYSTEM, &user, ask_to) {
                if let Some(v) = parse_json_block(&ans) {
                    if let Some(a) = v["action"].as_str() {
                        let params = v.get("params").cloned().unwrap_or_else(|| {
                            let mut p = serde_json::Map::new();
                            for k in ["units", "to", "amount", "choice", "price", "goods", "offer", "tight"] {
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
    let inbox = std::env::var("ALASHI_INBOX")
        .unwrap_or_else(|_| format!("{}/Desktop/alashi/inbox", std::env::var("HOME").unwrap_or_default()));
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
    let ranks = result["ranks"].as_array().map(|a| a.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>()).unwrap_or_default();
    let payouts = result["payouts"].as_array().map(|a| a.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>()).unwrap_or_default();
    let cash = result["final_cash"].as_array().map(|a| a.iter().filter_map(|v| v.as_u64()).collect::<Vec<_>>()).unwrap_or_default();
    let brk = result["payout_breakdown"].as_array();
    let my_place = ranks.iter().position(|i| *i as usize == my_idx).map(|p| p + 1).unwrap_or(99);
    let of = ranks.len();
    let my_payout = payouts.get(my_idx).copied().unwrap_or(0);
    let my_brk = |k: &str| brk
        .and_then(|b| b.get(my_idx))
        .and_then(|b| b[k].as_u64())
        .map(|v| format!("{:.1}", v as f64 / 1_000_000.0))
        .unwrap_or_else(|| "0.0".into());
    let mut body = format!(
        "# Отчёт {name} — игра {game} ({date})\n\n\
- модель: {model}\n\
- место: {my_place} из {of}, выплата: {:.1}M песо (ранг {}M / рента {}M / завод {}M)\n\n",
        m(&Value::from(my_payout)),
        my_brk("rank_share"),
        my_brk("license_rent"),
        my_brk("factory_bonus")
    );
    body.push_str("## Таблица партии (все фракции)\n\n");
    body.push_str("| место | фракция | модель | final cash+hard (M) | выплата (M) |\n|---|---|---|---|---|\n");
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
        "law" => ("vote", serde_json::json!({"choice": "yes"})),
        _ => ("produce", serde_json::json!({})),
    }
}
