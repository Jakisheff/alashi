use indexer::onchain;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("scan");
    match cmd {
        "serve" => {
            let port: u16 = args
                .get(2)
                .and_then(|p| p.parse().ok())
                .unwrap_or(8081);
            onchain::serve(port);
        }
        _ => {
            let rpc = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| "http://127.0.0.1:8899".to_string());
            let (fresh, _agg) = onchain::scan(&rpc);
            println!("done, {fresh} new parties");
        }
    }
}
