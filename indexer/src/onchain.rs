use crate::{aggregate, agent_json, leaderboard_json, load_registry, load_state, save_state, IndexerState};
use alashi_rules::state::{Faction, Game};
use anchor_lang::prelude::Pubkey;
use anchor_lang::AccountDeserialize;
use solana_rpc_client::rpc_client::RpcClient;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;

pub fn account_disc(name: &str) -> [u8; 8] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(name.as_bytes());
    let d = h.finalize();
    let mut out = [0u8; 8];
    out.copy_from_slice(&d[..8]);
    out
}

pub fn scan(rpc_url: &str) -> (usize, crate::Aggregated) {
    let rpc = RpcClient::new(rpc_url);
    let prog = alashi::id();
    let accounts = rpc
        .get_program_accounts(&prog)
        .expect("get_program_accounts");
    let game_disc = account_disc("account:Game");
    let faction_disc = account_disc("account:Faction");

    println!("fetched {} accounts", accounts.len());
    let mut games: Vec<Game> = vec![];
    let mut factions: BTreeMap<Pubkey, Vec<Faction>> = BTreeMap::new();
    let mut game_ids: BTreeMap<Pubkey, Pubkey> = BTreeMap::new();

    fn to_pk(a: &solana_address::Address) -> Pubkey {
        Pubkey::new_from_array(a.to_bytes())
    }
    for (addr, acc) in accounts {
        let pk = to_pk(&addr);
        let mut data: &[u8] = &acc.data;
        if acc.data.len() >= 8 && acc.data[..8] == game_disc {
            if let Err(e) = Game::try_deserialize(&mut data) {
                if games.len() == 0 && factions.is_empty() {
                    eprintln!("game parse err: {e:?} len={}", acc.data.len());
                }
            }
            data = &acc.data[..];
            if let Ok(g) = Game::try_deserialize(&mut data) {
                if g.phase == alashi_rules::state::Phase::Finished && g.settled {
                    games.push(g);
                }
            }
        } else if acc.data.len() >= 8 && acc.data[..8] == faction_disc {
            if let Ok(f) = Faction::try_deserialize(&mut data) {
                let gpk = f.game;
                factions.entry(gpk).or_default().push(f);
            }
        }
    }

    let mut known: BTreeMap<Pubkey, bool> = BTreeMap::new();
    let mut state = load_state(crate::STATE_FILE);
    let mut fresh: Vec<Game> = vec![];
    for g in &games {
        let key = crate::game_key(g);
        let idstr = key.to_string();
        if !state.processed_games.contains(&idstr) {
            fresh.push(g.clone());
            known.insert(key, true);
        }
    }
    for (k, _) in known {
        state.processed_games.insert(k.to_string());
    }
    let fresh_count = fresh.len();
    save_state(&state, crate::STATE_FILE);

    println!("parsed {} games, {} game-keys with factions", games.len(), factions.len());
    let registry = load_registry(crate::REGISTRY_FILE);
    let agg = aggregate(&fresh, &factions, &registry);
    persist_agg(&agg, fresh_count);
    (fresh_count, agg)
}

pub fn full_agg() -> crate::Aggregated {
    std::fs::read_to_string("../data/aggregates.json")
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn persist_agg(agg: &crate::Aggregated, fresh: usize) {
    let mut combined = full_agg();
    combined.parties_indexed += agg.parties_indexed;
    for (id, st) in agg.stats.iter() {
        let dst = combined.stats.entry(id.clone()).or_default();
        dst.agent_id = st.agent_id.clone();
        dst.matches += st.matches;
        dst.rank_sum += st.rank_sum;
        dst.total_cash += st.total_cash;
        for i in 0..4 {
            dst.rank_counts[i] += st.rank_counts[i];
        }
    }
    let _ = std::fs::create_dir_all(crate::DATA_DIR);
    let _ = std::fs::write(
        "../data/aggregates.json",
        serde_json::to_vec_pretty(&combined).unwrap(),
    );
    println!("indexed {fresh} new settled parties, total {}", combined.parties_indexed);
}

pub fn serve(port: u16) -> ! {
    let listener = TcpListener::bind(("127.0.0.1", port)).expect("bind");
    println!("leaderboard API on http://127.0.0.1:{port}/leaderboard");
    loop {
        let (mut stream, _) = listener.accept().expect("accept");
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            if reader.read_line(&mut line).is_err() {
                return;
            }
            let path = line
                .split_whitespace()
                .nth(1)
                .unwrap_or("/")
                .to_string();
            let agg = full_agg();
            let (code, body) = if path == "/leaderboard" {
                ("200 OK", leaderboard_json(&agg))
            } else if let Some(id) = path.strip_prefix("/agent/") {
                match agent_json(&agg, id) {
                    Some(j) => ("200 OK", j),
                    None => ("404 Not Found", "{\"error\":\"unknown agent\"}".into()),
                }
            } else {
                ("404 Not Found", "{\"error\":\"not found\"}".into())
            };
            let _ = stream.write_all(
                format!(
                    "HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            );
        });
    }
}

