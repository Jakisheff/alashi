use indexer::chain_api::ChainApi;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

const MAX_CONNECTIONS: usize = 8;

struct ConnectionPermit(Arc<AtomicUsize>);
impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

fn response(first: &str, api: &ChainApi) -> (&'static str, Value) {
    if let Some(path) = first
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
    }
}

fn handle(mut stream: TcpStream, api: &ChainApi) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let mut first = String::new();
    let Ok(n) = BufReader::new(&stream).take(2048).read_line(&mut first) else {
        return Ok(());
    };
    if n == 0 || n == 2048 || !first.ends_with('\n') {
        return Ok(());
    }
    let (status, body) = response(&first, api);
    let bytes = body.to_string();
    let head=format!("HTTP/1.1 {status}\r\nContent-Type: application/json; charset=utf-8\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",bytes.len());
    stream
        .write_all(head.as_bytes())
        .and_then(|_| stream.write_all(bytes.as_bytes()))
}

fn main() -> std::io::Result<()> {
    let port: u16 = std::env::var("ALASHI_CHAIN_API_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8097);
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let api = Arc::new(ChainApi::new());
    let active = Arc::new(AtomicUsize::new(0));
    for incoming in listener.incoming() {
        let Ok(mut stream) = incoming else { continue };
        if active
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                (n < MAX_CONNECTIONS).then_some(n + 1)
            })
            .is_err()
        {
            let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
            let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            continue;
        }
        let permit = ConnectionPermit(Arc::clone(&active));
        let api = Arc::clone(&api);
        let _ = std::thread::Builder::new()
            .name("chain-api".into())
            .spawn(move || {
                let _permit = permit;
                let _ = handle(stream, &api);
            });
    }
    Ok(())
}
