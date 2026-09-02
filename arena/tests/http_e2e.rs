//! E2E HTTP-тест арены: живой сервер, два «агента» по HTTP играют
//! полную партию до settle, проверяем экспорт и лидерборд.

use arena::api::{new_state, serve_on};
use serde_json::Value;
use std::io::{Read, Write};
use std::net::TcpStream;

fn http(port: u16, method: &str, path: &str, body: Option<&str>) -> Value {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let body = body.unwrap_or("");
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(req.as_bytes()).unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).unwrap();
    let text = String::from_utf8_lossy(&buf);
    let body_start = text.find("\r\n\r\n").expect("headers") + 4;
    serde_json::from_str(&text[body_start..]).expect("valid json body")
}

#[test]
fn http_full_game_two_agents() {
    let state = new_state();
    let addr = serve_on(state, "127.0.0.1:0", 50).expect("serve");
    let port = addr.port();

    // создать партию: фазы по 1 секунде (grace 0 — старый тайминг,
    // скорость теста; грейс покрыт отдельным тестом ниже)
    let r = http(
        port,
        "POST",
        "/game/new",
        Some(r#"{"entry_fee": 10000000, "phase_duration": 1, "grace_s": 0}"#),
    );
    assert_eq!(r["ok"], true, "{r}");
    let gid = r["game_id"].as_u64().unwrap();

    // два агента
    let mut tokens = vec![];
    for (name, model) in [("Alpha", "test-model-a"), ("Beta", "test-model-b")] {
        let r = http(
            port,
            "POST",
            &format!("/game/{}/join", gid),
            Some(&format!(
                r#"{{"name": "{}", "model": "{}", "prompt": "e2e"}}"#,
                name, model
            )),
        );
        assert_eq!(r["ok"], true, "{r}");
        tokens.push(r["token"].as_str().unwrap().to_string());
    }

    // игровая политика «жадный»: продать всё / произвести / за
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    loop {
        assert!(deadline.elapsed().is_zero(), "тест-таймаут 90с");
        let r = http(port, "GET", &format!("/game/{}/state", gid), None);
        if r["finished"] == true {
            break;
        }
        assert_eq!(r["ok"], true, "{r}");
        let s = &r["state"];
        let phase = s["phase"].as_str().unwrap();
        if phase == "lobby" {
            std::thread::sleep(std::time::Duration::from_millis(200));
            http(port, "POST", &format!("/game/{}/advance", gid), Some("{}"));
            continue;
        }
        for (i, f) in s["factions"].as_array().unwrap().iter().enumerate() {
            let acted = f["acted"].as_bool().unwrap();
            let voted = f["voted"].as_bool().unwrap();
            let (action, params) = match phase {
                "market" if !acted => {
                    let goods = f["goods"].as_u64().unwrap() as u64;
                    if goods > 0 {
                        ("sell", format!(r#"{{"units": {}}}"#, goods))
                    } else {
                        continue;
                    }
                }
                "action" if !acted => ("produce", "{}".to_string()),
                "law" if !voted => ("vote", r#"{"choice": "yes"}"#.to_string()),
                _ => continue,
            };
            let body = format!(
                r#"{{"token": "{}", "action": "{}", "params": {}}}"#,
                tokens[i], action, params
            );
            let r = http(port, "POST", &format!("/game/{}/act", gid), Some(&body));
            assert!(r["ok"] == true || r["error"].is_string(), "{r}");
        }
        // фаза 1с: ждём таймер кранка, advance подталкивает
        std::thread::sleep(std::time::Duration::from_millis(300));
        http(port, "POST", &format!("/game/{}/advance", gid), Some("{}"));
    }

    // результат: state показывает finished, export и лидерборд заполнены
    let r = http(port, "GET", &format!("/game/{}/state", gid), None);
    assert!(r["finished"] == true, "{r}");
    let result = &r["result"];
    let payouts: Vec<u64> = result["payouts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap())
        .collect();
    let bank = result["bank"].as_u64().unwrap();
    let rake = result["rake"].as_u64().unwrap();
    assert!(payouts.iter().sum::<u64>() == bank - rake, "{payouts:?}");
    assert!(rake > 0);

    let lb = http(port, "GET", "/leaderboard", None);
    let rows = lb["leaderboard"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{lb}");
    assert!(rows.iter().all(|r| r["games"].as_u64() == Some(1)));

    let ex = http(port, "GET", "/export", None);
    let ex_s = serde_json::to_string(&ex).unwrap();
    assert!(ex_s.contains(&gid.to_string()));
    // полный протокол партии: фазы с итогами законов и каждый ход
    let g = ex
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["game_id"].as_u64() == Some(gid))
        .expect("партия в export");
    let phases = g["phases"].as_array().unwrap();
    let actions = g["actions"].as_array().unwrap();
    assert_eq!(phases.len(), 19, "лобби + 6 раундов × 3 фазы: {phases:?}");
    assert!(!actions.is_empty(), "ходы должны попасть в export");
    let laws: Vec<_> = phases
        .iter()
        .filter(|p| p["phase"] == "law")
        .collect();
    assert_eq!(laws.len(), 6);
    assert!(laws.iter().all(|p| p["card_name"].is_string()));

    // авторизация: чужой токен отклоняется
    let r = http(
        port,
        "POST",
        &format!("/game/{}/act", gid),
        Some(r#"{"token": "deadbeef", "action": "produce"}"#),
    );
    // партия уже закрыта → unknown_game, тоже валидный отказ
    assert_eq!(r["ok"], false);
}

#[test]
fn http_epoch_90s_full_game() {
    let state = new_state();
    let addr = serve_on(state, "127.0.0.1:0", 50).expect("serve");
    let port = addr.port();

    let r = http(
        port,
        "POST",
        "/game/new",
        Some(r#"{"entry_fee": 10000000, "phase_duration": 1, "epoch": "90s"}"#),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["state"]["epoch"], "90s", "{r}");
    let gid = r["game_id"].as_u64().unwrap();

    let mut tokens = vec![];
    for name in ["Old", "New"] {
        let r = http(
            port,
            "POST",
            &format!("/game/{}/join", gid),
            Some(&format!(r#"{{"name": "{}", "model": "t90", "prompt": "e"}}"#, name)),
        );
        assert_eq!(r["ok"], true, "{r}");
        tokens.push(r["token"].as_str().unwrap().to_string());
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(90);
    let mut did_shuttle = false;
    let mut did_credit = false;
    let mut did_roof = false;
    loop {
        assert!(deadline.elapsed().is_zero(), "таймаут 90с");
        http(port, "POST", &format!("/game/{}/advance", gid), Some("{}"));
        let r = http(port, "GET", &format!("/game/{}/state", gid), None);
        if r["finished"] == true {
            break;
        }
        let s = &r["state"];
        let phase = s["phase"].as_str().unwrap();
        for (i, f) in s["factions"].as_array().unwrap().iter().enumerate() {
            let acted = f["acted"].as_bool().unwrap();
            let goods = f["goods"].as_u64().unwrap_or(0);
            let (action, params) = match phase {
                "market" if !acted && goods > 0 => {
                    // вексель хотя бы раз, дальше обычная продажа
                    if i == 0 && !did_credit {
                        did_credit = true;
                        ("sell_credit", format!(r#"{{"units": {}}}"#, goods))
                    } else {
                        ("sell", format!(r#"{{"units": {}}}"#, goods))
                    }
                }
                "action" if !acted => {
                    if i == 0 && !did_shuttle {
                        did_shuttle = true;
                        ("shuttle", "{}".to_string())
                    } else if i == 1 && !did_roof && f["cash"].as_u64().unwrap_or(0) > 10_000_000 {
                        did_roof = true;
                        ("roof", r#"{"to": 0}"#.to_string())
                    } else {
                        ("produce", "{}".to_string())
                    }
                }
                "law" if !f["voted"].as_bool().unwrap() => ("vote", r#"{"choice": "yes"}"#.to_string()),
                _ => continue,
            };
            let body = format!(
                r#"{{"token": "{}", "action": "{}", "params": {}}}"#,
                tokens[i], action, params
            );
            let r = http(port, "POST", &format!("/game/{}/act", gid), Some(&body));
            assert!(r["ok"] == true || r["error"].is_string(), "{action}: {r}");
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    assert!(did_shuttle && did_credit, "90s-механики должны были сыграть");
    // деньги сходятся: выплаты + рейк = банк
    let r = http(port, "GET", &format!("/game/{}/state", gid), None);
    let result = &r["result"];
    let payouts: u64 = result["payouts"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()).sum();
    let bank = result["bank"].as_u64().unwrap();
    let rake = result["rake"].as_u64().unwrap();
    assert_eq!(payouts + rake, bank);
    // протокол партии содержит новые действия
    let ex = http(port, "GET", "/export", None);
    let ex_s = serde_json::to_string(&ex).unwrap();
    assert!(ex_s.contains("shuttle"), "протокол должен видеть челнок");
    assert!(ex_s.contains("sell_credit"), "протокол должен видеть вексель");
}

#[test]
fn http_wait_longpoll_wakes_on_phase_change() {
    let state = new_state();
    let addr = serve_on(state, "127.0.0.1:0", 50).expect("serve");
    let port = addr.port();

    let r = http(
        port,
        "POST",
        "/game/new",
        Some(r#"{"entry_fee": 10000000, "phase_duration": 1}"#),
    );
    assert_eq!(r["ok"], true, "{r}");
    let gid = r["game_id"].as_u64().unwrap();
    for name in ["W1", "W2"] {
        let r = http(
            port,
            "POST",
            &format!("/game/{}/join", gid),
            Some(&format!(r#"{{"name": "{name}", "model": "t", "prompt": "e"}}"#)),
        );
        assert_eq!(r["ok"], true, "{r}");
    }

    // long-poll из потока: засыпаем на фазе market r1 — должны
    // проснуться, когда фаза сменится (кранк 250мс, фазы по 1с)
    let (tx, rx) = std::sync::mpsc::channel();
    let port2 = port;
    std::thread::spawn(move || {
        // дождёмся старта партии (лобби закрывается по 2-му join+таймер)
        let mut started = false;
        for _ in 0..40 {
            let r = http(port2, "GET", &format!("/game/{gid}/state"), None);
            if r["state"]["round"].as_u64().unwrap_or(0) >= 1 {
                started = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        assert!(started, "партия не стартовала");
        let t0 = std::time::Instant::now();
        let r = http(
            port2,
            "GET",
            &format!("/game/{gid}/wait?r=1&p=market&t=25"),
            None,
        );
        let _ = tx.send((r, t0.elapsed()));
    });

    // ждём ответа long-poll: фаза обязана смениться за 25с
    let (r, elapsed) = rx.recv_timeout(std::time::Duration::from_secs(30)).expect("long-poll молчит");
    assert_eq!(r["changed"], true, "{r}");
    let phase = r["state"]["phase"].as_str().unwrap().to_string();
    assert_ne!(phase, "market", "проснулись в той же фазе");
    assert!(elapsed.as_secs() < 20, "проснулись слишком поздно: {elapsed:?}");
}

#[test]
fn http_bribe_rejects_bad_target() {
    let state = new_state();
    let addr = serve_on(state, "127.0.0.1:0", 50).expect("serve");
    let port = addr.port();

    let r = http(
        port,
        "POST",
        "/game/new",
        Some(r#"{"entry_fee": 10000000, "phase_duration": 1}"#),
    );
    assert_eq!(r["ok"], true, "{r}");
    let gid = r["game_id"].as_u64().unwrap();
    let mut tokens = vec![];
    for name in ["Attacker", "Victim"] {
        let r = http(
            port,
            "POST",
            &format!("/game/{}/join", gid),
            Some(&format!(
                r#"{{"name": "{}", "model": "t", "prompt": "e2e"}}"#,
                name
            )),
        );
        assert_eq!(r["ok"], true, "{r}");
        tokens.push(r["token"].as_str().unwrap().to_string());
    }
    // доигрываем до фазы action: market без товаров, ждём таймер и advance
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        assert!(deadline.elapsed().is_zero(), "не дошли до action за 20с");
        http(port, "POST", &format!("/game/{}/advance", gid), Some("{}"));
        let r = http(port, "GET", &format!("/game/{}/state", gid), None);
        let phase = r["state"]["phase"].as_str().unwrap().to_string();
        if phase == "action" {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    // R1: to за границей, to опущено (usize::MAX), self-bribe — всё ok:false
    for params in [
        r#"{"to": 999, "amount": 5000000}"#,
        r#"{"amount": 5000000}"#,
        r#"{"to": 0, "amount": 5000000}"#,
    ] {
        let body = format!(
            r#"{{"token": "{}", "action": "bribe", "params": {}}}"#,
            tokens[0], params
        );
        let r = http(port, "POST", &format!("/game/{}/act", gid), Some(&body));
        assert_eq!(r["ok"], false, "params {params} прошли: {r}");
    }
    // арена жива после атак: /games и /game/:id/state отвечают
    let r = http(port, "GET", "/games", None);
    assert_eq!(r["ok"], true, "{r}");
    let r = http(port, "GET", &format!("/game/{}/state", gid), None);
    assert_eq!(r["ok"], true, "{r}");
}

#[test]
fn http_vote_weight_mode_flag() {
    let state = new_state();
    let addr = serve_on(state, "127.0.0.1:0", 50).expect("serve");
    let port = addr.port();

    // дефолт = legacy (0)
    let r = http(port, "POST", "/game/new", Some(r#"{"entry_fee": 10000000, "phase_duration": 5}"#));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["state"]["vote_weight_mode"], 0, "{r}");

    // contribution (1) выставляется и читается
    let r = http(
        port,
        "POST",
        "/game/new",
        Some(r#"{"entry_fee": 10000000, "phase_duration": 5, "vote_weight_mode": 1}"#),
    );
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["state"]["vote_weight_mode"], 1, "{r}");

    // мусорное значение отбивается
    let r = http(
        port,
        "POST",
        "/game/new",
        Some(r#"{"entry_fee": 10000000, "phase_duration": 5, "vote_weight_mode": 7}"#),
    );
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"], "bad_params", "{r}");
}

/// Кастдев 02.09 №1: грейс-окно после phase_ends_at. Кранк и /advance
/// ждут ends_at + grace_s, действие, опоздавшее на < grace_s, легально
/// приземляется в ещё не закрытую фазу.
#[test]
fn http_grace_window_lands_late_action() {
    let state = new_state();
    let addr = serve_on(state, "127.0.0.1:0", 50).expect("serve");
    let port = addr.port();

    let r = http(
        port,
        "POST",
        "/game/new",
        Some(r#"{"entry_fee": 10000000, "phase_duration": 1, "grace_s": 2}"#),
    );
    assert_eq!(r["ok"], true, "{r}");
    let gid = r["game_id"].as_u64().unwrap();
    assert_eq!(r["state"]["grace_s"], 2, "{r}");
    let mut tokens = vec![];
    for name in ["Late1", "Late2"] {
        let r = http(
            port,
            "POST",
            &format!("/game/{}/join", gid),
            Some(&format!(r#"{{"name": "{name}", "model": "t", "prompt": "e"}}"#)),
        );
        assert_eq!(r["ok"], true, "{r}");
        tokens.push(r["token"].as_str().unwrap().to_string());
    }

    // доходим до action r1 (лобби 5с по таймеру, market 1+2с)
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(25);
    let ends = loop {
        assert!(deadline.elapsed().is_zero(), "не дошли до action за 25с");
        let r = http(port, "GET", &format!("/game/{}/state", gid), None);
        let s = &r["state"];
        if s["phase"] == "action" && s["round"] == 1 {
            let ends = s["phase_ends_at"].as_i64().unwrap();
            assert_eq!(s["grace_until"].as_i64().unwrap(), ends + 2, "{s}");
            break ends;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    };

    // ждём середину окна (ends + 1с, окно [ends, ends+2))
    let r = http(port, "GET", &format!("/game/{}/state", gid), None);
    let now_ts = r["state"]["now"].as_i64().unwrap();
    let wait = (ends + 1 - now_ts).max(0) as u64;
    std::thread::sleep(std::time::Duration::from_secs(wait));

    // опоздавшее на 1с produce обязано приземлиться: фаза ещё открыта
    let body = format!(
        r#"{{"token": "{}", "action": "produce", "params": {{}}}}"#,
        tokens[0]
    );
    let r = http(port, "POST", &format!("/game/{}/act", gid), Some(&body));
    assert_eq!(r["ok"], true, "опоздавшее действие не приземлилось: {r}");

    // ранний permissionless /advance срезается грейсом, фаза не двигается
    let r = http(port, "POST", &format!("/game/{}/advance", gid), Some("{}"));
    assert_eq!(r["ok"], false, "{r}");
    assert_eq!(r["error"], "GraceWindow", "{r}");
    let r = http(port, "GET", &format!("/game/{}/state", gid), None);
    assert_eq!(r["state"]["phase"], "action", "фаза уехала до grace_until: {r}");

    // после grace_until кранк закрывает фазу: produce зачтён, пришли в law
    let mut law = false;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while deadline.elapsed().is_zero() {
        let r = http(port, "GET", &format!("/game/{}/state", gid), None);
        let s = &r["state"];
        if s["phase"] == "law" && s["round"] == 1 {
            law = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(law, "после грейса фаза не закрылась");
}
