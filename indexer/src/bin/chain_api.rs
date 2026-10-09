use indexer::chain_api::ChainApi;
use serde_json::json;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;

fn main() -> std::io::Result<()> {
    let port: u16 = std::env::var("ALASHI_CHAIN_API_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8097);
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let api = ChainApi::new();
    for incoming in listener.incoming() {
        let Ok(mut stream) = incoming else { continue };
        stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
        let mut first = String::new();
        let Ok(n) = BufReader::new(&stream).take(2048).read_line(&mut first) else {
            continue;
        };
        if n == 0 || n == 2048 || !first.ends_with('\n') {
            continue;
        }
        let (status, body) = if let Some(path) = first
            .trim_end_matches(['\r', '\n'])
            .strip_prefix("GET ")
            .and_then(|s| s.split_once(" HTTP/").map(|v| v.0))
        {
            if let Some(pda) = path
                .strip_prefix("/chain/devnet/games/")
                .filter(|s| !s.contains(['?', '/', '%']))
            {
                match api.get(pda) {
                    Ok(v) => ("200 OK", v),
                    Err(e) => (e.status(), json!({"ok":false,"error":e.code()})),
                }
            } else {
                ("404 Not Found", json!({"ok":false,"error":"not_found"}))
            }
        } else {
            (
                "405 Method Not Allowed",
                json!({"ok":false,"error":"method_not_allowed"}),
            )
        };
        let bytes = body.to_string();
        let head=format!("HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len());
        let _ = stream
            .write_all(head.as_bytes())
            .and_then(|_| stream.write_all(bytes.as_bytes()));
    }
    Ok(())
}
