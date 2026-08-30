use indexer::events::parse_log_line;
use indexer::onchain;
use indexer::replay;
use indexer::AgentEntry;
use solana_rpc_client::rpc_client::RpcClient;

fn collect_events(rpc: &RpcClient) -> Vec<indexer::events::ParsedEvent> {
    let stream_path = "../data/events_stream.jsonl";
    if std::path::Path::new(stream_path).exists() {
        let mut events = vec![];
        for line in std::fs::read_to_string(stream_path).unwrap_or_default().lines() {
            if let Some(e) = parse_log_line(line) {
                events.push(e);
            }
        }
        return events;
    }
    collect_events_rpc(rpc)
}

fn collect_events_rpc(rpc: &RpcClient) -> Vec<indexer::events::ParsedEvent> {
    let prog = alashi::id();
    let sigs = rpc
        .get_signatures_for_address(&prog)
        .unwrap_or_default();
    let mut events = vec![];
    let mut ok_tx = 0usize;
    let mut no_meta = 0usize;
    for st in sigs.iter().rev() {
        let sig: solana_signature::Signature = match st.signature.parse() {
            Ok(s) => s,
            Err(_) => continue,
        };
        let tx = match rpc.get_transaction(
            &sig,
            solana_transaction_status_client_types::UiTransactionEncoding::Json,
        ) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let meta = match tx.transaction.meta {
            Some(m) => m,
            None => {
                no_meta += 1;
                continue;
            }
        };
        ok_tx += 1;
        let logs: Option<Vec<String>> = meta.log_messages.into();
        if let Some(logs) = logs {
            for line in &logs {
                if let Some(e) = parse_log_line(line) {
                    events.push(e);
                }
            }
        }
    }
    eprintln!("tx ok={ok_tx} no_meta={no_meta} events={}", events.len());
    events
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rpc_url = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "http://127.0.0.1:8899".to_string());
    let out_dir = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| "../data/exports".to_string());

    let rpc = RpcClient::new_with_commitment(rpc_url, solana_commitment_config::CommitmentConfig::confirmed());
    let events = collect_events(&rpc);
    eprintln!(
        "init={} payout={}",
        events.iter().any(|e| matches!(e, indexer::events::ParsedEvent::GameInitialized { .. })),
        events.iter().any(|e| matches!(e, indexer::events::ParsedEvent::Payout { .. })),
    );
    println!("collected {} events", events.len());

    let registry: Vec<(String, String)> = indexer::load_registry(indexer::REGISTRY_FILE)
        .into_iter()
        .map(|e: AgentEntry| (e.wallet.to_string(), e.agent_id))
        .collect();

    let mut by_game: std::collections::BTreeMap<String, Vec<indexer::events::ParsedEvent>> =
        Default::default();
    let mut current: Option<String> = None;
    for e in events {
        let gpk = match &e {
            indexer::events::ParsedEvent::GameInitialized { game, .. } => Some(game.clone()),
            indexer::events::ParsedEvent::FactionJoined { game, .. } => Some(game.clone()),
            indexer::events::ParsedEvent::Sold { game, .. } => Some(game.clone()),
            indexer::events::ParsedEvent::Produced { game, .. } => Some(game.clone()),
            indexer::events::ParsedEvent::PhaseAdvanced { game, .. } => Some(game.clone()),
            indexer::events::ParsedEvent::Payout { game, .. } => Some(game.clone()),
            indexer::events::ParsedEvent::Settled { game, .. } => Some(game.clone()),
            _ => current.clone(),
        };
        if let Some(g) = gpk {
            current = Some(g.clone());
            by_game.entry(g).or_default().push(e);
        }
    }
    let mut total_events = 0;
    for (_, evs) in by_game.iter_mut() {
        total_events += evs.len();
    }
    let _ = total_events;

    std::fs::create_dir_all(&out_dir).unwrap();
    let path = format!("{out_dir}/parties.jsonl");
    let mut out = String::new();
    let mut count = 0;
    for (gid, evs) in by_game.iter() {
        let Some(rec) = replay::replay(evs, &registry) else {
            continue;
        };
        out.push_str(&serde_json::to_string(&rec.record).unwrap());
        out.push('\n');
        count += 1;
        let _ = gid;
    }
    std::fs::write(&path, out).unwrap();
    std::fs::write(
        format!("{out_dir}/LICENSE"),
        "MIT License\n\nCopyright (c) 2026 Alashi\n\nPermission is hereby granted, free of charge, to any person obtaining a copy\nof this software and associated documentation files (the \"Software\"), to deal\nin the Software without restriction, including without limitation the rights\nto use, copy, modify, merge, publish, distribute, sublicense, and/or sell\ncopies of the Software, and to permit persons to whom the Software is\nfurnished to do so, subject to the following conditions:\n\nThe above copyright notice and this permission notice shall be included in all\ncopies or substantial portions of the Software.\n\nTHE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR\nIMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,\nFITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE\nAUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER\nLIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,\nOUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE\nSOFTWARE.\n",
    )
    .unwrap();
    println!("exported {count} parties to {path} (+ LICENSE)");
    let _ = onchain::full_agg();
}
