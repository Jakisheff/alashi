use indexer::chain_api::ChainApi;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

const MAX_CONNECTIONS: usize = 8;
const MAX_REJECTIONS: usize = 2;

struct ConnectionPermit(Arc<AtomicUsize>);
impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}
fn acquire(active: &Arc<AtomicUsize>, limit: usize) -> Option<ConnectionPermit> {
    active
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
            (n < limit).then_some(n + 1)
        })
        .ok()?;
    Some(ConnectionPermit(Arc::clone(active)))
}

fn complete_header(request: &[u8]) -> bool {
    request.windows(4).any(|part| part == b"\r\n\r\n")
        || request.windows(2).any(|part| part == b"\n\n")
}

fn reject_overload(mut stream: TcpStream) -> std::io::Result<()> {
    // Read a bounded complete header before responding, otherwise HTTP clients
    // can observe a reset while they are still sending their request.
    let deadline = Instant::now() + Duration::from_millis(250);
    let mut request = [0; 4096];
    let mut used = 0;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() || used == request.len() {
            return Ok(());
        }
        stream.set_read_timeout(Some(remaining))?;
        let n = stream.read(&mut request[used..])?;
        if n == 0 {
            return Ok(());
        }
        used += n;
        if complete_header(&request[..used]) {
            break;
        }
    }
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    stream.write_all(
        b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
    )
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
    let rejecting = Arc::new(AtomicUsize::new(0));
    for incoming in listener.incoming() {
        let Ok(stream) = incoming else { continue };
        let Some(permit) = acquire(&active, MAX_CONNECTIONS) else {
            if let Some(permit) = acquire(&rejecting, MAX_REJECTIONS) {
                let _ = std::thread::Builder::new()
                    .name("chain-reject".into())
                    .spawn(move || {
                        let _permit = permit;
                        let _ = reject_overload(stream);
                    });
            }
            continue;
        };
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
