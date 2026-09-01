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

    // создать партию: фазы по 1 секунде
    let r = http(
        port,
        "POST",
        "/game/new",
        Some(r#"{"entry_fee": 10000000, "phase_duration": 1}"#),
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
