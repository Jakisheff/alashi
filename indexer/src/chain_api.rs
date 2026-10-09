//! Bounded, read-only projection of confirmed Alashi devnet accounts and events.
//! This is an account snapshot plus an event journal, never a reconstructed game.
use crate::events::{parse_log_line, ParsedEvent};
use alashi_rules::state::{Faction, Game, Phase, VoteChoice};
use anchor_lang::{prelude::Pubkey, AccountDeserialize};
use base64::Engine;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const RPC: &str = "https://api.devnet.solana.com";
const DEVNET_GENESIS: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
const MAX_SIGNATURES: usize = 300;
const MAX_EVENTS: usize = 2000;

#[derive(Debug)]
pub enum Error {
    BadGame,
    NotFound,
    Upstream,
    RateLimited,
    Incomplete,
}
impl Error {
    pub fn status(&self) -> &'static str {
        match self {
            Self::BadGame => "400 Bad Request",
            Self::NotFound => "404 Not Found",
            Self::RateLimited => "503 Service Unavailable",
            Self::Upstream | Self::Incomplete => "503 Service Unavailable",
        }
    }
    pub fn code(&self) -> &'static str {
        match self {
            Self::BadGame => "bad_game_pda",
            Self::NotFound => "unknown_game",
            Self::RateLimited => "upstream_rate_limited",
            Self::Upstream => "upstream_unavailable",
            Self::Incomplete => "incomplete_snapshot",
        }
    }
}

#[derive(Clone)]
struct Cached {
    value: Value,
    at: Instant,
    terminal: bool,
    seen_signatures: HashSet<String>,
}

pub struct ChainApi {
    http: reqwest::blocking::Client,
    rpc_url: String,
    // The single gate coalesces same-PDA work and bounds aggregate public RPC.
    gate: Mutex<Instant>,
    cache: Mutex<HashMap<String, Cached>>,
    genesis_ok: Mutex<bool>,
    archive_dir: Option<PathBuf>,
}

impl ChainApi {
    pub fn new() -> Self {
        Self {
            http: reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .expect("http"),
            rpc_url: RPC.to_owned(),
            gate: Mutex::new(Instant::now() - Duration::from_secs(1)),
            cache: Mutex::new(HashMap::new()),
            genesis_ok: Mutex::new(false),
            archive_dir: std::env::var_os("ALASHI_CHAIN_ARCHIVE_DIR").map(PathBuf::from),
        }
    }
    fn rpc(&self, method: &str, params: Value) -> Result<Value, Error> {
        let mut gate = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        let left = Duration::from_secs(1).saturating_sub(gate.elapsed());
        if !left.is_zero() {
            std::thread::sleep(left);
        }
        *gate = Instant::now();
        drop(gate);
        let resp = self
            .http
            .post(&self.rpc_url)
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .map_err(|_| Error::Upstream)?;
        if resp.status().as_u16() == 429 {
            return Err(Error::RateLimited);
        }
        if !resp.status().is_success() {
            return Err(Error::Upstream);
        }
        let v: Value = resp.json().map_err(|_| Error::Upstream)?;
        if v["error"].is_object() {
            return Err(if v["error"]["code"].as_i64() == Some(429) {
                Error::RateLimited
            } else {
                Error::Upstream
            });
        }
        v.get("result").cloned().ok_or(Error::Upstream)
    }
    fn genesis(&self) -> Result<(), Error> {
        let mut checked = self.genesis_ok.lock().unwrap_or_else(|e| e.into_inner());
        if !*checked {
            if self.rpc("getGenesisHash", json!([]))?.as_str() != Some(DEVNET_GENESIS) {
                return Err(Error::Upstream);
            }
            *checked = true;
        }
        Ok(())
    }
    pub fn get(&self, pda: &str) -> Result<Value, Error> {
        let key = Pubkey::from_str(pda).map_err(|_| Error::BadGame)?;
        if key.to_string() != pda {
            return Err(Error::BadGame);
        }
        let previous = self
            .cache
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(pda)
            .cloned();
        if let Some(c) = &previous {
            if c.terminal || c.at.elapsed() < Duration::from_secs(8) {
                return Ok(c.value.clone());
            }
        }
        if let Some(value) = self.load_archive(pda) {
            return Ok(value);
        }
        // One publisher at a time. A bounded negative cache is deliberately
        // omitted: unknown PDAs cost a single account read, not a scan.
        let (value, seen_signatures) = self.fetch(&key, previous.as_ref())?;
        let terminal = value["game"]["settled"] == true && value["history_complete"] == true;
        if terminal {
            self.save_archive(pda, &value);
        }
        self.cache.lock().unwrap_or_else(|e| e.into_inner()).insert(
            pda.to_owned(),
            Cached {
                value: value.clone(),
                at: Instant::now(),
                terminal,
                seen_signatures,
            },
        );
        // A single-thread server caps aggregate work; this cap bounds positive
        // memory even if many different valid games are requested.
        let mut cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() > 32 {
            if let Some(old) = cache
                .iter()
                .filter(|(k, _)| k.as_str() != pda)
                .min_by_key(|(_, v)| v.at)
                .map(|(k, _)| k.clone())
            {
                cache.remove(&old);
            }
        }
        Ok(value)
    }
    fn load_archive(&self, pda: &str) -> Option<Value> {
        let path = self.archive_dir.as_ref()?.join(format!("{pda}.json"));
        let bytes = std::fs::read(path).ok()?;
        if bytes.len() > 2_000_000 {
            return None;
        }
        let value: Value = serde_json::from_slice(&bytes).ok()?;
        if value["ok"] == true
            && value["game_pda"] == pda
            && value["cluster"] == "devnet"
            && value["program_id"] == alashi::id().to_string()
            && value["game"]["settled"] == true
            && value["history_complete"] == true
            && value["events"]
                .as_array()
                .is_some_and(|a| a.len() <= MAX_EVENTS)
        {
            Some(value)
        } else {
            None
        }
    }
    fn save_archive(&self, pda: &str, value: &Value) {
        let Some(dir) = &self.archive_dir else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let path = dir.join(format!("{pda}.json"));
        let tmp = dir.join(format!(".{pda}.tmp"));
        let Ok(bytes) = serde_json::to_vec(value) else {
            return;
        };
        #[cfg(unix)]
        use std::os::unix::fs::OpenOptionsExt;
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        opts.mode(0o600);
        if let Ok(mut f) = opts.open(&tmp) {
            use std::io::Write;
            if f.write_all(&bytes).is_ok() && f.sync_all().is_ok() {
                let _ = std::fs::rename(tmp, path);
            }
        }
    }
    fn fetch(
        &self,
        key: &Pubkey,
        previous: Option<&Cached>,
    ) -> Result<(Value, HashSet<String>), Error> {
        self.genesis()?;
        let key_s = key.to_string();
        let game_result = self.rpc(
            "getAccountInfo",
            json!([key_s,{"encoding":"base64","commitment":"confirmed"}]),
        )?;
        let game_account = game_result
            .get("value")
            .filter(|v| !v.is_null())
            .ok_or(Error::NotFound)?;
        let game: Game = decode_account(game_account, "Game")?;
        if crate::game_key(&game) != *key {
            return Err(Error::BadGame);
        }
        let game_slot = game_result["context"]["slot"]
            .as_u64()
            .ok_or(Error::Upstream)?;
        let discovered = self.rpc(
            "getProgramAccounts",
            json!([alashi::id().to_string(),
            {"encoding":"base64","commitment":"confirmed","withContext":true,
             "filters":[{"memcmp":{"offset":8,"bytes":key_s}}]}]),
        )?;
        let rows = discovered["value"].as_array().ok_or(Error::Upstream)?;
        if rows.len() > 6 {
            return Err(Error::Incomplete);
        }
        let mut faction_keys = Vec::new();
        let mut wallets = HashSet::new();
        for row in rows {
            let pda = row["pubkey"]
                .as_str()
                .and_then(|s| Pubkey::from_str(s).ok())
                .ok_or(Error::Upstream)?;
            let faction: Faction = decode_account(&row["account"], "Faction")?;
            if faction.game != *key || faction_pda(key, &faction.wallet) != pda {
                return Err(Error::BadGame);
            }
            wallets.insert(faction.wallet.to_string());
            faction_keys.push(pda);
        }
        faction_keys.sort();
        if faction_keys.len() != game.faction_count as usize {
            return Err(Error::Incomplete);
        }
        let discovery_slot = discovered["context"]["slot"]
            .as_u64()
            .ok_or(Error::Upstream)?;
        let faction_ids: HashSet<String> = faction_keys.iter().map(ToString::to_string).collect();
        if wallets.len() != faction_keys.len() {
            return Err(Error::Incomplete);
        }
        let mut journal = self.journal(key, &faction_ids, &wallets, previous)?;
        // Read one coherent account snapshot after the journal pass. Historical
        // backfill can take many seconds; a pre-backfill phase would be stale.
        let addresses = std::iter::once(key_s.clone())
            .chain(faction_keys.iter().map(ToString::to_string))
            .collect::<Vec<_>>();
        let snapshot = self.rpc(
            "getMultipleAccounts",
            json!([addresses,
            {"encoding":"base64","commitment":"confirmed",
             "minContextSlot":game_slot.max(discovery_slot).max(journal.through_slot)}]),
        )?;
        let snapshot_slot = snapshot["context"]["slot"]
            .as_u64()
            .ok_or(Error::Upstream)?;
        let accounts = snapshot["value"].as_array().ok_or(Error::Upstream)?;
        if accounts.len() != faction_keys.len() + 1 {
            return Err(Error::Incomplete);
        }
        let game: Game = decode_account(&accounts[0], "Game")?;
        if crate::game_key(&game) != *key || game.faction_count as usize != faction_keys.len() {
            return Err(Error::Incomplete);
        }
        let mut factions = Vec::new();
        for (account, pda) in accounts.iter().skip(1).zip(&faction_keys) {
            let f: Faction = decode_account(account, "Faction")?;
            if f.game != *key || faction_pda(key, &f.wallet) != *pda {
                return Err(Error::BadGame);
            }
            factions.push((pda.to_string(), f));
        }
        if factions
            .iter()
            .any(|(_, f)| !wallets.contains(&f.wallet.to_string()))
        {
            return Err(Error::Incomplete);
        }
        let head = self.rpc(
            "getSignaturesForAddress",
            json!([key_s,
            {"limit":1,"commitment":"confirmed"}]),
        )?;
        if head.as_array().is_none_or(|rows| {
            rows.first().is_some_and(|row| {
                row["signature"]
                    .as_str()
                    .is_none_or(|sig| !journal.seen_signatures.contains(sig))
            })
        }) {
            journal.complete = false;
        }
        let winner = if game.settled && journal.complete {
            let matches = journal
                .events
                .iter()
                .filter(|e| e["type"] == "payout" && e["rank"] == 0)
                .filter_map(|e| e["wallet"].as_str())
                .collect::<Vec<_>>();
            if matches.len() == 1 {
                factions
                    .iter()
                    .find(|(_, f)| f.wallet.to_string() == matches[0])
                    .map(|(p, _)| p.clone())
            } else {
                None
            }
        } else {
            None
        };
        let faction_json = factions.iter().map(|(p,f)|json!({
            "pda":p,"wallet":f.wallet.to_string(),"name":f.name,"cash":f.cash.to_string(),
            "hard":f.hard.to_string(),"goods":f.goods,"influence":f.influence,
            "vote":match f.vote { VoteChoice::Yes=>"Yes",VoteChoice::No=>"No",VoteChoice::Abstain=>"Abstain" },
            "alive":f.alive
        })).collect::<Vec<_>>();
        let value = json!({"ok":true,"cluster":"devnet","program_id":alashi::id().to_string(),
            "game_pda":key_s,"commitment":"confirmed","snapshot_slot":snapshot_slot,
            "journal_through_slot":journal.through_slot,"history_complete":journal.complete,
            "game":{"id":game.game_id.to_string(),"phase":phase_name(game.phase),"round":game.round,
                "phase_ends_at":game.phase_ends_at,"settled":game.settled,"epoch":game.epoch},
            "factions":faction_json,"events":journal.events,"winner_faction_pda":winner,
            "fetched_at":chrono::Utc::now().to_rfc3339()});
        Ok((value, journal.seen_signatures))
    }
    fn journal(
        &self,
        key: &Pubkey,
        faction_ids: &HashSet<String>,
        wallets: &HashSet<String>,
        previous: Option<&Cached>,
    ) -> Result<Journal, Error> {
        let base = previous.filter(|p| p.value["history_complete"] == true && !p.terminal);
        let mut before: Option<String> = None;
        let mut sigs = Vec::<(String, u64, Option<i64>)>::new();
        let mut seen_signatures = HashSet::new();
        let mut examined = 0usize;
        let mut complete = true;
        let mut found_init = base.is_some();
        let mut reached_base = false;
        while examined < MAX_SIGNATURES {
            let take = (MAX_SIGNATURES - examined).min(100);
            let mut config = json!({"limit":take,"commitment":"confirmed"});
            if let Some(ref cursor) = before {
                config["before"] = json!(cursor);
            }
            let page = self.rpc("getSignaturesForAddress", json!([key.to_string(), config]))?;
            let Some(rows) = page.as_array() else {
                return Err(Error::Upstream);
            };
            if rows.is_empty() {
                break;
            }
            for row in rows {
                examined += 1;
                let sig = row["signature"]
                    .as_str()
                    .ok_or(Error::Upstream)?
                    .to_string();
                if base.is_some_and(|p| p.seen_signatures.contains(&sig)) {
                    reached_base = true;
                    break;
                }
                before = Some(sig.clone());
                seen_signatures.insert(sig.clone());
                if !row["err"].is_null() {
                    continue;
                }
                sigs.push((
                    sig,
                    row["slot"].as_u64().ok_or(Error::Upstream)?,
                    row["blockTime"].as_i64(),
                ));
            }
            if reached_base || rows.len() < take {
                break;
            }
        }
        if base.is_some() && !reached_base {
            // A previously complete journal no longer overlaps the current
            // signature window; reconstruct from genesis rather than splice.
            return self.journal(key, faction_ids, wallets, None);
        }
        if let Some(p) = base {
            let previous_slot = p.value["journal_through_slot"].as_u64().unwrap_or(0);
            if sigs.iter().any(|(_, slot, _)| *slot <= previous_slot) {
                // A new transaction in the same slot needs block ordering
                // against previously cached transactions too.
                return self.journal(key, faction_ids, wallets, None);
            }
            seen_signatures.extend(p.seen_signatures.iter().cloned());
        }
        if examined == MAX_SIGNATURES && !reached_base {
            complete = false;
        }
        // RPC returns newest first. For equal slots, getBlock provides the
        // canonical transaction order; no signature-lexical tie breaker.
        let mut duplicate_slots = HashMap::<u64, usize>::new();
        for (_, slot, _) in &sigs {
            *duplicate_slots.entry(*slot).or_default() += 1;
        }
        let mut positions = HashMap::<(u64, String), usize>::new();
        for (slot, count) in duplicate_slots {
            if count > 1 {
                let block = self.rpc(
                    "getBlock",
                    json!([slot,{"encoding":"json","transactionDetails":"signatures",
                "rewards":false,"commitment":"confirmed"}]),
                );
                match block {
                    Ok(v) => {
                        if let Some(rows) = v["signatures"].as_array() {
                            for (i, s) in rows.iter().enumerate() {
                                if let Some(sig) = s.as_str() {
                                    positions.insert((slot, sig.to_string()), i);
                                }
                            }
                        } else {
                            complete = false;
                        }
                        if sigs.iter().any(|(sig, s, _)| {
                            *s == slot && !positions.contains_key(&(slot, sig.clone()))
                        }) {
                            complete = false;
                        }
                    }
                    Err(_) => complete = false,
                }
            }
        }
        sigs.sort_by(|a, b| {
            a.1.cmp(&b.1).then_with(|| {
                positions
                    .get(&(a.1, a.0.clone()))
                    .cmp(&positions.get(&(b.1, b.0.clone())))
            })
        });
        let mut events = base
            .and_then(|p| p.value["events"].as_array().cloned())
            .unwrap_or_default();
        let mut through_slot = base
            .and_then(|p| p.value["journal_through_slot"].as_u64())
            .unwrap_or(0);
        for (sig, slot, block_time) in sigs {
            let tx = match self.rpc(
                "getTransaction",
                json!([sig,{"encoding":"json","maxSupportedTransactionVersion":0,
                "commitment":"confirmed"}]),
            ) {
                Ok(v) if !v.is_null() => v,
                _ => {
                    complete = false;
                    continue;
                }
            };
            if !tx["meta"]["err"].is_null() {
                continue;
            }
            let Some(logs) = tx["meta"]["logMessages"].as_array() else {
                complete = false;
                continue;
            };
            through_slot = through_slot.max(slot);
            for (log_index, event) in filtered_events(logs, key) {
                if !event_associated(&event, faction_ids, wallets) {
                    complete = false;
                    continue;
                }
                if matches!(event, ParsedEvent::GameInitialized { .. }) {
                    found_init = true;
                }
                if events.len() == MAX_EVENTS {
                    complete = false;
                    break;
                }
                let mut row = serde_json::to_value(event).map_err(|_| Error::Upstream)?;
                stringify_u64(&mut row);
                let event_index = events
                    .iter()
                    .rev()
                    .take_while(|e: &&Value| e["signature"] == sig)
                    .count();
                let obj = row.as_object_mut().ok_or(Error::Upstream)?;
                obj.insert("id".into(), json!(format!("{sig}:{log_index}")));
                obj.insert("signature".into(), json!(sig));
                obj.insert("slot".into(), json!(slot));
                obj.insert("log_index".into(), json!(log_index));
                obj.insert("event_index".into(), json!(event_index));
                obj.insert("block_time".into(), json!(block_time));
                events.push(row);
            }
        }
        if !found_init {
            complete = false;
        }
        Ok(Journal {
            events,
            through_slot,
            complete,
            seen_signatures,
        })
    }
}

struct Journal {
    events: Vec<Value>,
    through_slot: u64,
    complete: bool,
    seen_signatures: HashSet<String>,
}
fn phase_name(p: Phase) -> &'static str {
    match p {
        Phase::Lobby => "Lobby",
        Phase::Market => "Market",
        Phase::Action => "Action",
        Phase::Law => "Law",
        Phase::Finished => "Finished",
        Phase::Aborted => "Aborted",
    }
}
fn faction_pda(game: &Pubkey, wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            alashi::constants::FACTION_SEED,
            game.as_ref(),
            wallet.as_ref(),
        ],
        &alashi::id(),
    )
    .0
}
fn decode_account<T: AccountDeserialize>(v: &Value, name: &str) -> Result<T, Error> {
    if v["owner"].as_str() != Some(&alashi::id().to_string()) {
        return Err(Error::BadGame);
    }
    let b64 = v["data"][0].as_str().ok_or(Error::Upstream)?;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| Error::Upstream)?;
    if raw.len() < 8 || raw[..8] != crate::onchain::account_disc(&format!("account:{name}")) {
        return Err(Error::BadGame);
    }
    T::try_deserialize(&mut raw.as_slice()).map_err(|_| Error::BadGame)
}
/// Only data emitted by the exact target program's current invocation frame.
pub fn filtered_events(logs: &[Value], game: &Pubkey) -> Vec<(usize, ParsedEvent)> {
    let target = alashi::id().to_string();
    let mut stack = Vec::<String>::new();
    let mut rows = Vec::new();
    for (i, v) in logs.iter().enumerate() {
        let Some(line) = v.as_str() else { continue };
        if let Some((program, _)) = line
            .strip_prefix("Program ")
            .and_then(|s| s.split_once(" invoke ["))
        {
            stack.push(program.to_string());
        } else if let Some(program) = line
            .strip_prefix("Program ")
            .and_then(|s| s.strip_suffix(" success"))
        {
            if stack.last().is_some_and(|p| p == program) {
                stack.pop();
            } else {
                stack.clear();
            }
        } else if let Some((program, _)) = line
            .strip_prefix("Program ")
            .and_then(|s| s.split_once(" failed:"))
        {
            if stack.last().is_some_and(|p| p == program) {
                stack.pop();
            } else {
                stack.clear();
            }
        } else if stack.last().is_some_and(|p| p == &target) {
            if let Some(ev) = parse_log_line(line).filter(|e| e.game() == game.to_string()) {
                rows.push((i, ev));
            }
        }
    }
    rows
}
fn event_associated(
    e: &ParsedEvent,
    factions: &HashSet<String>,
    wallets: &HashSet<String>,
) -> bool {
    use ParsedEvent::*;
    match e {
        FactionJoined { faction, .. }
        | Produced { faction, .. }
        | Sold { faction, .. }
        | GoodsBought { faction, .. }
        | DonkeyBought { faction, .. }
        | VoteCast { faction, .. }
        | SoldCreditEv { faction, .. }
        | ShuttledEv { faction, .. }
        | LicenseBid { faction, .. }
        | LicenseInsight { faction, .. }
        | Exchanged { faction, .. } => factions.contains(faction),
        BribeGiven { from, to, .. }
        | RoofBought { from, to, .. }
        | VoteOffered {
            seller: from,
            buyer: to,
            ..
        }
        | VoteSold {
            buyer: from,
            seller: to,
            ..
        }
        | BarterAccepted {
            by: from, from: to, ..
        } => factions.contains(from) && factions.contains(to),
        BarterProposed { from, .. } => factions.contains(from),
        VetoCast { president, .. }
        | Payout {
            wallet: president, ..
        } => wallets.contains(president),
        CustomsSet { president, .. } => factions.contains(president),
        _ => true,
    }
}
fn stringify_u64(v: &mut Value) {
    if let Some(obj) = v.as_object_mut() {
        for key in [
            "game_id",
            "entry_fee",
            "revenue",
            "cost",
            "amount",
            "price",
            "pot",
            "rake",
            "paid",
            "promissory",
            "got",
            "offer",
            "total_bid",
            "yield_amount",
            "commit_slot",
        ] {
            if let Some(value) = obj.get_mut(key) {
                if let Some(n) = value.as_u64() {
                    *value = json!(n.to_string());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    #[test]
    fn foreign_stack_and_wrong_game_are_not_events() {
        let game = Pubkey::new_unique();
        let foreign = Pubkey::new_unique();
        use anchor_lang::AnchorSerialize;
        let mut raw = Vec::new();
        alashi::events::Produced {
            game,
            faction: Pubkey::new_unique(),
            goods: 2,
        }
        .serialize(&mut raw)
        .unwrap();
        let line = format!(
            "Program data: {}",
            base64::engine::general_purpose::STANDARD
                .encode([crate::events::event_disc("event:Produced").to_vec(), raw].concat())
        );
        let logs = vec![
            json!(format!("Program {foreign} invoke [1]")),
            json!(line.clone()),
            json!(format!("Program {foreign} success")),
            json!(format!("Program {} invoke [1]", alashi::id())),
            json!(format!("Program {foreign} invoke [2]")),
            json!(line.clone()),
            json!(format!("Program {foreign} success")),
            json!(line),
            json!(format!("Program {} success", alashi::id())),
        ];
        assert_eq!(filtered_events(&logs, &game).len(), 1);
        assert!(filtered_events(&logs, &Pubkey::new_unique()).is_empty());
        assert!(!event_associated(
            &ParsedEvent::Produced {
                game: game.to_string(),
                faction: Pubkey::new_unique().to_string(),
                goods: 1,
            },
            &HashSet::new(),
            &HashSet::new()
        ));
        assert!(!event_associated(
            &ParsedEvent::Payout {
                game: game.to_string(),
                wallet: Pubkey::new_unique().to_string(),
                rank: 0,
                amount: 1,
            },
            &HashSet::new(),
            &HashSet::new()
        ));
    }

    #[test]
    fn complete_cached_journal_reads_only_new_signatures() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut methods = Vec::new();
            for incoming in listener.incoming().take(2) {
                let mut stream = incoming.unwrap();
                let mut header = Vec::new();
                let mut byte = [0];
                while !header.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                }
                let head = String::from_utf8(header).unwrap();
                let size: usize = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|n| n.parse().ok())
                    })
                    .unwrap();
                let mut data = vec![0; size];
                stream.read_exact(&mut data).unwrap();
                let req: Value = serde_json::from_slice(&data).unwrap();
                let method = req["method"].as_str().unwrap().to_string();
                let result = match method.as_str() {
                    "getSignaturesForAddress" => json!([
                        {"signature":"new","slot":11,"blockTime":10,"err":null},
                        {"signature":"old","slot":10,"blockTime":9,"err":null}]),
                    "getTransaction" => {
                        assert_eq!(req["params"][0], "new");
                        json!({"meta":{"err":null,"logMessages":[]}})
                    }
                    _ => panic!("unexpected RPC method"),
                };
                methods.push(method);
                let body = json!({"jsonrpc":"2.0","id":1,"result":result}).to_string();
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                stream.write_all(body.as_bytes()).unwrap();
            }
            methods
        });
        let mut api = ChainApi::new();
        api.rpc_url = url;
        let prior = Cached {
            value: json!({"history_complete":true,"events":[{"id":"old:1","type":"game_initialized"}],
                "journal_through_slot":10}),
            at: Instant::now(),
            terminal: false,
            seen_signatures: HashSet::from(["old".to_string()]),
        };
        let journal = api
            .journal(
                &Pubkey::new_unique(),
                &HashSet::new(),
                &HashSet::new(),
                Some(&prior),
            )
            .unwrap();
        assert!(journal.complete);
        assert_eq!(journal.through_slot, 11);
        assert_eq!(journal.events.len(), 1);
        assert_eq!(journal.seen_signatures.len(), 2);
        assert_eq!(
            server.join().unwrap(),
            vec!["getSignaturesForAddress", "getTransaction"]
        );
    }

    #[test]
    fn same_slot_transactions_follow_block_position_not_signature_order() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut transaction_order = Vec::new();
            for incoming in listener.incoming().take(4) {
                let mut stream = incoming.unwrap();
                let mut header = Vec::new();
                let mut byte = [0];
                while !header.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    header.push(byte[0]);
                }
                let head = String::from_utf8(header).unwrap();
                let size: usize = head
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|n| n.parse().ok())
                    })
                    .unwrap();
                let mut data = vec![0; size];
                stream.read_exact(&mut data).unwrap();
                let req: Value = serde_json::from_slice(&data).unwrap();
                let response = match req["method"].as_str().unwrap() {
                    "getSignaturesForAddress" => json!([
                        {"signature":"z-newest","slot":10,"blockTime":10,"err":null},
                        {"signature":"a-oldest","slot":10,"blockTime":10,"err":null}]),
                    "getBlock" => json!({"signatures":["a-oldest","z-newest"]}),
                    "getTransaction" => {
                        transaction_order.push(req["params"][0].as_str().unwrap().to_string());
                        json!({"meta":{"err":null,"logMessages":[]}})
                    }
                    method => panic!("unexpected method {method}"),
                };
                let body = json!({"jsonrpc":"2.0","id":1,"result":response}).to_string();
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                stream.write_all(header.as_bytes()).unwrap();
                stream.write_all(body.as_bytes()).unwrap();
            }
            transaction_order
        });
        let mut api = ChainApi::new();
        api.rpc_url = url;
        let journal = api
            .journal(
                &Pubkey::new_unique(),
                &HashSet::new(),
                &HashSet::new(),
                None,
            )
            .unwrap();
        assert!(
            !journal.complete,
            "no GameInitialized event means incomplete history"
        );
        assert_eq!(server.join().unwrap(), vec!["a-oldest", "z-newest"]);
    }
}
