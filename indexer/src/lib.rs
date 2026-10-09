pub mod events;
pub mod chain_api;
pub mod onchain;
pub mod replay;

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

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentStats {
    pub agent_id: String,
    pub matches: u64,
    pub rank_sum: u64,
    pub rank_counts: Vec<u64>,
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

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct Aggregated {
    pub stats: BTreeMap<String, AgentStats>,
    pub parties_indexed: u64,
}

/// Аудит 27.09 (S7): кодирование как в arena::api::agent_id_of —
/// с префиксами длины, без неоднозначного разделителя.
pub fn agent_id_for(model: &str, prompt: &str) -> String {
    let mut h = Sha256::new();
    h.update((model.len() as u64).to_le_bytes());
    h.update(model.as_bytes());
    h.update((prompt.len() as u64).to_le_bytes());
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
                        wallet: e.get("wallet")?.as_str()?.parse::<Pubkey>().ok()?,
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

pub fn rank_factions(factions: &[Faction]) -> Vec<usize> {
    rank_factions_for_epoch(factions, alashi_rules::constants::EPOCH_CLASSIC)
}

pub fn rank_factions_for_epoch(factions: &[Faction], epoch: u8) -> Vec<usize> {
    let wealth = |f: &Faction| f.cash as u128 + if epoch == alashi_rules::constants::EPOCH_90S { f.hard as u128 } else { 0 };
    let mut order: Vec<usize> = (0..factions.len()).collect();
    order.sort_by(|&a, &b| wealth(&factions[b]).cmp(&wealth(&factions[a]))
        .then_with(|| factions[a].wallet.cmp(&factions[b].wallet)));
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
        let order = rank_factions_for_epoch(factions, game.epoch);
        for (rank_pos, &fi) in order.iter().enumerate() {
            let wallet = factions[fi].wallet;
            let id = by_wallet
                .get(&wallet)
                .map(|e| e.agent_id.clone())
                .unwrap_or_else(|| wallet.to_string());
            let st = agg.stats.entry(id.clone()).or_default();
            st.agent_id = id;
            st.matches += 1;
            st.rank_sum += (rank_pos + 1) as u64;
            st.rank_counts.resize(alashi_rules::constants::MAX_FACTIONS as usize, 0);
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
    /// Аудит 27.09 (S5): агрегаты живут в одном файле с checkpoint и
    /// пишутся одной атомарной операцией. Раньше checkpoint сохранялся
    /// до агрегатов, и сбой между ними навсегда пропускал партию.
    #[serde(default)]
    pub aggregates: Aggregated,
    /// Подпись обработанной партии (хэш сериализации Game): один и тот
    /// же PDA может быть создан заново после закрытия — при другом
    /// состоянии партия индексируется повторно, а не пропускается.
    #[serde(default)]
    pub processed_sigs: BTreeMap<String, String>,
}

pub fn load_state(path: &str) -> IndexerState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

/// Аудит 27.09 (S5): запись состояния атомарна (tmp + rename) и
/// возвращает ошибку вызывающему: потерянная запись не считается успехом.
pub fn save_state(state: &IndexerState, path: &str) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(dir) = Path::new(path).parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = format!("{path}.tmp");
    let mut f = std::fs::File::create(&tmp)?;
    f.write_all(serde_json::to_string_pretty(state).unwrap().as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

/// Подпись итогового состояния партии для распознавания повторного
/// использования адреса игры (аудит 27.09, S5).
pub fn game_signature(game: &Game) -> String {
    use anchor_lang::AccountSerialize;
    let mut buf = Vec::new();
    let _ = game.try_serialize(&mut buf);
    let mut h = Sha256::new();
    h.update(&buf);
    hex(&h.finalize())
}

/// Слияние приращения агрегатов (бывшая логика persist_agg, теперь без
/// собственной записи на диск).
pub fn merge_aggregates(combined: &mut Aggregated, add: &Aggregated) {
    combined.parties_indexed += add.parties_indexed;
    for (id, st) in add.stats.iter() {
        let dst = combined.stats.entry(id.clone()).or_default();
        dst.agent_id = st.agent_id.clone();
        dst.matches += st.matches;
        dst.rank_sum += st.rank_sum;
        dst.total_cash += st.total_cash;
        dst.rank_counts.resize(alashi_rules::constants::MAX_FACTIONS as usize, 0);
        for (i, count) in st.rank_counts.iter().enumerate() {
            dst.rank_counts[i] += count;
        }
    }
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
