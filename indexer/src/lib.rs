pub mod onchain;

use alashi_rules::state::{Faction, Game, Phase};
use anchor_lang::prelude::Pubkey;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const DATA_DIR: &str = "../data";
pub const REGISTRY_FILE: &str = "../data/registry.json";
pub const STATE_FILE: &str = "../data/indexer_state.json";
pub const EXPORTS_DIR: &str = "../data/exports";

pub struct AgentEntry {
    pub wallet: Pubkey,
    pub agent_id: String,
    pub model: String,
    pub prompt: String,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct AgentStats {
    pub agent_id: String,
    pub matches: u64,
    pub rank_sum: u64,
    pub rank_counts: [u64; 4],
    pub total_cash: u64,
}

impl AgentStats {
    pub fn avg_rank(&self) -> f64 {
        if self.matches == 0 {
            return 0.0;
        }
        self.rank_sum as f64 / self.matches as f64
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct Aggregated {
    pub stats: BTreeMap<String, AgentStats>,
    pub parties_indexed: u64,
}

pub fn agent_id_for(model: &str, prompt: &str) -> String {
    let mut h = Sha256::new();
    h.update(model.as_bytes());
    h.update(b"|");
    h.update(prompt.as_bytes());
    hex(&h.finalize())
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn load_registry(path: &str) -> Vec<AgentEntry> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return vec![];
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return vec![];
    };
    v.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|e| {
                    Some(AgentEntry {
                        wallet: Pubkey::new_from_array(
                            bs58_decode(e.get("wallet")?.as_str()?),
                        ),
                        agent_id: e.get("agent_id")?.as_str()?.to_string(),
                        model: e
                            .get("model")
                            .and_then(|m| m.as_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        prompt: e
                            .get("prompt")
                            .and_then(|p| p.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn bs58_decode(s: &str) -> [u8; 32] {
    let alphabet: Vec<char> =
        "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz".chars().collect();
    let mut num = vec![0u8];
    for c in s.chars() {
        let val = alphabet.iter().position(|a| *a == c).unwrap_or(0) as u32;
        let mut carry = val;
        for digit in num.iter_mut().rev() {
            let x = *digit as u32 * 58 + carry;
            *digit = (x & 0xff) as u8;
            carry = x >> 8;
        }
        while carry > 0 {
            num.insert(0, (carry & 0xff) as u8);
            carry >>= 8;
        }
    }
    let leading_zeros = s.chars().take_while(|c| *c == '1').count();
    let mut out = [0u8; 32];
    let bytes: Vec<u8> = std::iter::repeat(0)
        .take(leading_zeros)
        .chain(num.into_iter().skip_while(|b| *b == 0))
        .collect();
    let n = bytes.len().min(32);
    out[32 - n..].copy_from_slice(&bytes[32 - n..]);
    out
}

pub fn rank_factions(factions: &[Faction]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..factions.len()).collect();
    order.sort_by(|&a, &b| {
        let ca = factions[a].cash;
        let cb = factions[b].cash;
        if ca != cb {
            cb.cmp(&ca)
        } else {
            factions[a].wallet.cmp(&factions[b].wallet)
        }
    });
    order
}

pub fn aggregate(
    games: &[Game],
    factions_of_game: &BTreeMap<Pubkey, Vec<Faction>>,
    registry: &[AgentEntry],
) -> Aggregated {
    let by_wallet: BTreeMap<Pubkey, &AgentEntry> = registry
        .iter()
        .map(|e| (e.wallet, e))
        .collect();
    let mut agg = Aggregated::default();
    for game in games {
        if game.phase != Phase::Finished || !game.settled {
            continue;
        }
        let Some(factions) = factions_of_game.get(&game_key(game)) else {
            continue;
        };
        if factions.is_empty() {
            continue;
        }
        agg.parties_indexed += 1;
        let order = rank_factions(factions);
        for (rank_pos, &fi) in order.iter().enumerate() {
            if rank_pos >= 4 {
                break;
            }
            let wallet = factions[fi].wallet;
            let id = by_wallet
                .get(&wallet)
                .map(|e| e.agent_id.clone())
                .unwrap_or_else(|| wallet.to_string());
            let st = agg.stats.entry(id.clone()).or_default();
            st.agent_id = id;
            st.matches += 1;
            st.rank_sum += (rank_pos + 1) as u64;
            st.rank_counts[rank_pos] += 1;
            st.total_cash += factions[fi].cash;
        }
    }
    agg
}

pub fn game_key(game: &Game) -> Pubkey {
    let seed = game.game_id.to_le_bytes();
    Pubkey::find_program_address(
        &[alashi::constants::GAME_SEED, seed.as_ref()],
        &alashi::id(),
    )
    .0
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct IndexerState {
    pub processed_games: BTreeSet<String>,
}

pub fn load_state(path: &str) -> IndexerState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_state(state: &IndexerState, path: &str) {
    if let Some(dir) = Path::new(path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, serde_json::to_string_pretty(state).unwrap());
}

pub fn leaderboard_json(agg: &Aggregated) -> String {
    let rows: Vec<serde_json::Value> = agg
        .stats
        .values()
        .map(|s| {
            serde_json::json!({
                "agent_id": s.agent_id,
                "matches": s.matches,
                "avg_rank": s.avg_rank(),
                "rank_counts": s.rank_counts,
                "total_cash": s.total_cash,
            })
        })
        .collect();
    serde_json::to_string_pretty(&serde_json::json!({
        "parties_indexed": agg.parties_indexed,
        "agents": rows,
    }))
    .unwrap()
}

pub fn agent_json(agg: &Aggregated, agent_id: &str) -> Option<String> {
    agg.stats
        .get(agent_id)
        .map(|s| {
            serde_json::to_string_pretty(&serde_json::json!({
                "agent_id": s.agent_id,
                "matches": s.matches,
                "avg_rank": s.avg_rank(),
                "rank_counts": s.rank_counts,
                "total_cash": s.total_cash,
            }))
            .unwrap()
        })
}
