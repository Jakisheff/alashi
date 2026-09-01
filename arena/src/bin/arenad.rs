//! arenad: HTTP-арена alashi. Порт и тик кранка через аргументы/env:
//!   arenad [--port 8090] [--tick-ms 250] [--bind 127.0.0.1]

use arena::api::{new_state, serve};

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
    let port = flag(&args, "--port")
        .or_else(|| std::env::var("PORT").ok())
        .and_then(|v| v.parse().ok())
        .unwrap_or(8090);
    let tick_ms: u64 = flag(&args, "--tick-ms")
        .and_then(|v| v.parse().ok())
        .unwrap_or(250);
    let bind = flag(&args, "--bind").unwrap_or_else(|| "127.0.0.1".into());
    let addr = format!("{}:{}", bind, port);
    let state = new_state();
    if let Err(e) = serve(state, &addr, tick_ms) {
        eprintln!("[ERROR] не подняться на {}: {}", addr, e);
        std::process::exit(1);
    }
}
