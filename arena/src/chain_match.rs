//! Wallet-proved admission to the operator's separate devnet match host.
use super::{owner_auth, AppState};
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

fn err(code: &str) -> Value {
    json!({"ok":false,"error":code})
}

pub fn assign(state: &AppState, record: &str, origin: &str, bearer: &str, body: &Value) -> Value {
    if !owner_auth::origin_ok(origin) {
        return err("owner_origin_forbidden");
    }
    if !body.as_object().is_some_and(|o| o.is_empty()) {
        return err("bad_match_request");
    }
    let owner = match owner_auth::owner_session_for(state, record, bearer) {
        Ok(owner) => owner,
        Err(code) => return err(code),
    };
    let port = std::env::var("ALASHI_MATCH_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8098);
    forward(&owner.wallet, port, false).unwrap_or_else(err)
}

pub fn fund(wallet: &str) -> Value {
    let port = std::env::var("ALASHI_MATCH_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(8098);
    forward(wallet, port, true).unwrap_or_else(err)
}

fn forward(wallet: &str, port: u16, funding: bool) -> Result<Value, &'static str> {
    let address = SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|_| "match_host_unavailable")?;
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| "match_host_unavailable")?;
    stream
        .set_write_timeout(Some(Duration::from_secs(3)))
        .map_err(|_| "match_host_unavailable")?;
    // Only the authenticated public wallet crosses this loopback boundary.
    let body = json!({"wallet":wallet}).to_string();
    let path = if funding { "/fund" } else { "/match" };
    write!(stream,"POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).map_err(|_| "match_host_unavailable")?;
    let mut response = String::new();
    stream
        .take(8193)
        .read_to_string(&mut response)
        .map_err(|_| "match_host_unavailable")?;
    if response.len() > 8192 {
        return Err("match_host_unavailable");
    }
    let (head, body) = response
        .split_once("\r\n\r\n")
        .ok_or("match_host_unavailable")?;
    if !(head.starts_with("HTTP/1.1 200 ") || head.starts_with("HTTP/1.1 503 ")) {
        return Err("match_host_unavailable");
    }
    let value: Value = serde_json::from_str(body).map_err(|_| "match_host_unavailable")?;
    if value["ok"] == true {
        if value["network"] != "devnet"
            || !(value["status"] == "preparing"
                || value["status"] == if funding { "ready" } else { "assigned" })
            || (funding && (value["wallet"] != wallet || value["target_lamports"] != "100000000"))
        {
            return Err("match_host_unavailable");
        }
        Ok(value)
    } else {
        Err(match value["error"].as_str() {
            Some("match_capacity") => "match_capacity",
            Some("host_funds_low") => "host_funds_low",
            Some("storage_failed") => "storage_failed",
            Some("funding_capacity") => "funding_capacity",
            Some("funding_failed") => "funding_failed",
            Some("funding_rpc_unavailable") => "funding_rpc_unavailable",
            _ => "match_host_unavailable",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn missing_or_wrong_wallet_proof_never_reaches_host() {
        let state = super::super::new_state();
        assert_eq!(
            assign(
                &state,
                &"a".repeat(64),
                "https://alashi.network",
                &"b".repeat(64),
                &json!({})
            )["error"],
            "owner_session_invalid"
        );
        assert_eq!(
            assign(
                &state,
                &"a".repeat(64),
                "https://other.example",
                &"b".repeat(64),
                &json!({})
            )["error"],
            "owner_origin_forbidden"
        );
        assert_eq!(
            assign(
                &state,
                &"a".repeat(64),
                "https://alashi.network",
                &"b".repeat(64),
                &json!({"wallet":"somebody_else"})
            )["error"],
            "bad_match_request"
        );
    }

    #[test]
    fn funding_proxy_forwards_public_wallet_and_checks_the_recipient() {
        for wrong_wallet in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let port = listener.local_addr().unwrap().port();
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let request = crate::http::read_request(&stream).unwrap();
                assert_eq!(request.path, "/fund");
                assert!(request.authorization.is_none() && request.cookie.is_none());
                assert_eq!(
                    serde_json::from_slice::<Value>(&request.body).unwrap(),
                    json!({"wallet":"public-wallet"})
                );
                crate::http::respond(
                    &mut stream,
                    "200 OK",
                    &json!({"ok":true,
                    "network":"devnet","status":"preparing","target_lamports":"100000000",
                    "wallet":if wrong_wallet {"other-wallet"} else {"public-wallet"}})
                    .to_string(),
                );
            });
            let response = forward("public-wallet", port, true);
            if wrong_wallet {
                assert_eq!(response.unwrap_err(), "match_host_unavailable");
            } else {
                assert_eq!(response.unwrap()["status"], "preparing");
            }
            worker.join().unwrap();
        }
    }

    #[test]
    fn proxy_forwards_only_wallet_and_preserves_assignment() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = crate::http::read_request(&stream).unwrap();
            assert_eq!(request.path, "/match");
            assert!(request.authorization.is_none());
            assert_eq!(
                serde_json::from_slice::<Value>(&request.body).unwrap(),
                json!({"wallet":"public-wallet"})
            );
            crate::http::respond(
                &mut stream,
                "200 OK",
                &json!({"ok":true,"status":"preparing","network":"devnet","game_pda":"same-game"})
                    .to_string(),
            );
        });
        assert_eq!(
            forward("public-wallet", port, false).unwrap()["game_pda"],
            "same-game"
        );
        worker.join().unwrap();
    }
}
