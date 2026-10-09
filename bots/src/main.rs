mod agent_cli;
mod chain_wish_client;
mod llm;
mod session_cli;

use solana_signature::Signature;
use solana_signer::Signer;

use {
    alashi::{accounts, constants, id, instruction, state},
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    solana_commitment_config::CommitmentConfig,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_rpc_client::rpc_client::RpcClient,
    solana_transaction::versioned::VersionedTransaction,
    std::{
        path::Path,
        str::FromStr,
        thread::sleep,
        time::{Duration, SystemTime, UNIX_EPOCH},
    },
};

const ENTRY_FEE: u64 = 50_000_000;
const PHASE_DURATION: i64 = 15;

fn demo_mode() -> bool {
    std::env::var("ALASHI_DEMO").is_ok()
}

fn demo_say(msg: &str) {
    if demo_mode() {
        println!("    [DEMO] {msg}");
        let _ = std::io::Write::flush(&mut std::io::stdout());
    }
}
const KEYS_DIR: &str = "bots/keys";
const PRICE_TABLE: [u64; 16] = [12, 10, 9, 8, 7, 6, 5, 4, 3, 3, 2, 2, 2, 1, 1, 1];

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn chain_now(rpc: &RpcClient) -> i64 {
    match rpc.get_slot() {
        Ok(slot) => match rpc.get_block_time(slot) {
            Ok(t) => t,
            Err(e) => {
                println!("[ERROR] get_block_time({slot}): {e}");
                let _ = std::io::Write::flush(&mut std::io::stdout());
                0
            }
        },
        Err(e) => {
            println!("[ERROR] get_slot: {e}");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            0
        }
    }
}

fn game_pda(game_id: u64) -> Pubkey {
    let seed = game_id.to_le_bytes();
    Pubkey::find_program_address(&[constants::GAME_SEED, seed.as_ref()], &id()).0
}

fn faction_pda(game: &Pubkey, wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[constants::FACTION_SEED, game.as_ref(), wallet.as_ref()],
        &id(),
    )
    .0
}

fn load_or_create(path: &str) -> Keypair {
    if Path::new(path).exists() {
        let bytes: Vec<u8> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let seed: [u8; 32] = bytes[..32]
            .try_into()
            .expect("keypair file must hold 64 bytes");
        Keypair::new_from_array(seed)
    } else {
        let kp = Keypair::new();
        std::fs::create_dir_all(KEYS_DIR).unwrap();
        std::fs::write(
            path,
            serde_json::to_string(&kp.to_bytes().to_vec()).unwrap(),
        )
        .unwrap();
        kp
    }
}

/// Build and sign locally so callers can persist the signature before submission.
fn prepare_ix(
    rpc: &RpcClient,
    signer: &Keypair,
    ix: Instruction,
) -> Result<VersionedTransaction, serde_json::Value> {
    use serde_json::json;
    let bh = rpc.get_latest_blockhash().map_err(|_| {
        json!({
            "status":"not_sent", "error":"blockhash_unavailable"
        })
    })?;
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer])
        .map_err(|_| json!({"status":"not_sent", "error":"signing_failed"}))?;
    Ok(tx)
}

/// Success means a confirmed receipt with meta.err == null, not RPC acceptance.
fn send_ix_confirmed(
    rpc: &RpcClient,
    signer: &Keypair,
    ix: Instruction,
) -> Result<serde_json::Value, serde_json::Value> {
    let tx = prepare_ix(rpc, signer, ix)?;
    submit_confirmed(rpc, &tx)
}

fn submit_confirmed(
    rpc: &RpcClient,
    tx: &VersionedTransaction,
) -> Result<serde_json::Value, serde_json::Value> {
    use serde_json::json;
    // Keep the locally derived signature even when the submit response is lost.
    let sig = tx.signatures[0];
    if let Err(error) = rpc.send_transaction(tx) {
        if let Some(reason) = error.get_transaction_error() {
            return Err(json!({"status":"rejected", "signature":sig.to_string(),
                "error":format!("{reason:?}")}));
        }
        // A transport error is ambiguous. Read the receipt before advising a retry.
        return wait_receipt(rpc, &sig, Duration::from_secs(10));
    }
    wait_receipt(rpc, &sig, Duration::from_secs(30))
}

fn receipt_result(
    sig: &str,
    tx: &serde_json::Value,
) -> Option<Result<serde_json::Value, serde_json::Value>> {
    use serde_json::json;
    let meta = tx.get("meta")?.as_object()?;
    let err = meta.get("err")?;
    let slot = tx.get("slot")?.as_u64()?;
    if !err.is_null() {
        return Some(Err(json!({"status":"failed", "signature":sig,
            "slot":slot, "error":err})));
    }
    Some(Ok(
        json!({"status":"confirmed", "signature":sig, "slot":slot,
        "block_time":tx.get("blockTime"), "log_messages":meta.get("logMessages")}),
    ))
}

fn wait_receipt(
    rpc: &RpcClient,
    sig: &solana_signature::Signature,
    timeout: Duration,
) -> Result<serde_json::Value, serde_json::Value> {
    use serde_json::json;
    use solana_rpc_client_api::request::RpcRequest;
    let started = std::time::Instant::now();
    loop {
        if let Ok(tx) = rpc.send::<serde_json::Value>(
            RpcRequest::GetTransaction,
            json!([sig.to_string(), {"encoding":"json", "commitment":"confirmed",
                "maxSupportedTransactionVersion":0}]),
        ) {
            if let Some(result) = receipt_result(&sig.to_string(), &tx) {
                return result;
            }
        }
        if started.elapsed() >= timeout {
            break;
        }
        sleep(Duration::from_millis(500));
    }
    Err(json!({"status":"unknown", "signature":sig.to_string(),
        "error":"confirmation_unavailable", "retry":"inspect signature and state before resubmitting"}))
}

fn send_ix(rpc: &RpcClient, signer: &Keypair, ix: Instruction) -> bool {
    match send_ix_confirmed(rpc, signer, ix) {
        Ok(receipt) => {
            capture_events(&receipt);
            println!(
                "  confirmed tx https://explorer.solana.com/tx/{}?cluster=devnet",
                receipt["signature"].as_str().unwrap_or_default()
            );
            let _ = std::io::Write::flush(&mut std::io::stdout());
            true
        }
        Err(err) => {
            eprintln!("  tx result: {err}");
            if err["status"] == "unknown" {
                eprintln!("Receipt unknown: host/guest stopped. Inspect signature and chain state before resuming; do not resubmit blindly.");
                std::process::exit(3);
            }
            false
        }
    }
}

fn fetch_game(rpc: &RpcClient, game: &Pubkey) -> Option<state::Game> {
    match rpc.get_account(game) {
        Ok(acc) => {
            let mut d: &[u8] = &acc.data;
            Some(state::Game::try_deserialize(&mut d).ok()?)
        }
        Err(_) => None,
    }
}

fn fetch_faction(rpc: &RpcClient, faction: &Pubkey) -> Option<state::Faction> {
    match rpc.get_account(faction) {
        Ok(acc) => {
            let mut d: &[u8] = &acc.data;
            Some(state::Faction::try_deserialize(&mut d).ok()?)
        }
        Err(_) => None,
    }
}

struct Bot {
    kp: Keypair,
    faction: Pubkey,
    name: &'static str,
    acted: bool,
    voted: bool,
    goods: u16,
    bribed: bool,
    wish_profile: Option<chain_wish_client::Profile>,
    wish: Option<chain_wish_client::ChainWishClient>,
}

fn ensure_funds(rpc: &RpcClient, who: &str, kp: &Keypair) {
    let bal = rpc.get_balance(&kp.pubkey()).unwrap_or(0);
    println!("{who} balance: {bal} lamports");
    if bal < 200_000_000 {
        println!("{who} requesting airdrop 1 SOL...");
        match rpc.request_airdrop(&kp.pubkey(), 1_000_000_000) {
            Ok(sig) => {
                println!("  airdrop tx {sig}");
                sleep(Duration::from_secs(8));
            }
            Err(e) => println!("  airdrop err: {e}"),
        }
    }
}

fn ix_initialize(admin: Pubkey, game: Pubkey, game_id: u64, phase_duration: i64) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Initialize {
            game_id,
            entry_fee: ENTRY_FEE,
            phase_duration,
            entropy_mode: 0,
            epoch: 0,
        }
        .data(),
        accounts::Initialize {
            admin,
            game,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_join(name: &str, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Join {
            name: name.to_string(),
        }
        .data(),
        accounts::Join {
            player,
            game,
            faction,
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_produce(player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Produce {}.data(),
        accounts::Produce {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_sell(units: u16, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Sell { units }.data(),
        accounts::Sell {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_bribe(
    amount: u64,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    target: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Bribe { amount }.data(),
        accounts::Bribe {
            player,
            game,
            faction,
            target,
        }
        .to_account_metas(None),
    )
}

fn ix_vote(
    choice: state::VoteChoice,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Vote { choice }.data(),
        accounts::Vote {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_advance(crank: Pubkey, game: Pubkey, factions: &[Pubkey]) -> Instruction {
    let mut metas = accounts::Advance {
        crank,
        game,
        hashes: solana_sysvar::slot_hashes::ID,
    }
    .to_account_metas(None);
    for f in factions {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(
            *f, false,
        ));
    }
    Instruction::new_with_bytes(id(), &instruction::Advance {}.data(), metas)
}

fn ix_buy(units: u16, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Buy { units }.data(),
        accounts::BuyGoods {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_veto(player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Veto {}.data(),
        accounts::Veto {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_donkey(player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::BuyDonkey {}.data(),
        accounts::BuyDonkey {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_settle(
    crank: Pubkey,
    game: Pubkey,
    factions: &[Pubkey],
    wallets: &[Pubkey],
    admin: Pubkey,
) -> Instruction {
    let mut metas = accounts::Settle { crank, game }.to_account_metas(None);
    for f in factions {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new_readonly(*f, false));
    }
    for w in wallets {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(
            *w, false,
        ));
    }
    metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(
        admin, false,
    ));
    Instruction::new_with_bytes(id(), &instruction::Settle {}.data(), metas)
}

fn rpc_url() -> String {
    std::env::var("ALASHI_RPC").unwrap_or_else(|_| "https://api.devnet.solana.com".to_string())
}

fn wait_account(rpc: &RpcClient, key: &Pubkey, label: &str) -> bool {
    for _ in 0..40 {
        if rpc.get_account(key).is_ok() {
            return true;
        }
        sleep(Duration::from_millis(500));
    }
    println!("[ERROR] account {label} не появился за 20с: {key}");
    false
}

fn llm_decide(
    cfg: &llm::LlmConfig,
    phase: &str,
    round: u8,
    cash: u64,
    goods: u16,
    influence: u16,
    sold: u16,
    price_hint: u64,
    law: &str,
    president_is_me: bool,
) -> Option<serde_json::Value> {
    const TPL: &str = "Доступные действия JSON: \"action\":\"sell\",\"units\":N / \"action\":\"buy\",\"units\":N / \"action\":\"produce\" / \"action\":\"bribe\",\"amount\":N / \"action\":\"donkey\" / \"action\":\"vote\",\"choice\":\"yes|no|abstain\",\"veto\":true|false (объект в фигурных скобках, veto только если ты президент)";
    let user = format!(
        "Фаза: {}. Раунд {}/6. Твоё состояние: {} alashi, {} товаров, влияние {}. На базаре продано {} единиц, следующая цена ~{} alashi. Закон на голосовании: {}. Ты президент: {}. {}",
        phase, round, cash / 1_000_000, goods, influence, sold, price_hint, law, president_is_me, TPL
    );
    let raw = llm::llm_ask(cfg, llm::SYSTEM, &user)?;
    println!("    [llm raw] {}", raw.replace('\n', " "));
    llm::parse_json_block(&raw)
}

/// Preserve the legacy export stream, but only from confirmed successful Alashi invocations.
fn capture_events(receipt: &serde_json::Value) {
    use std::io::Write;
    if let Some(logs) = receipt["log_messages"].as_array() {
        let lines = alashi_event_logs(logs);
        if lines.is_empty() {
            return;
        }
        if let Ok(mut out) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("../data/events_stream.jsonl")
        {
            for line in lines {
                let _ = writeln!(out, "{line}");
            }
        }
    }
}

fn alashi_event_logs(logs: &[serde_json::Value]) -> Vec<&str> {
    let program = id().to_string();
    let mut stack: Vec<&str> = Vec::new();
    let mut result = Vec::new();
    for value in logs {
        let Some(line) = value.as_str() else { continue };
        if let Some(rest) = line.strip_prefix("Program ") {
            if let Some((key, _)) = rest.split_once(" invoke [") {
                stack.push(key);
            } else if rest.ends_with(" success") || rest.contains(" failed:") {
                stack.pop();
            }
        }
        if line.starts_with("Program data: ") && stack.last().copied() == Some(program.as_str()) {
            result.push(line);
        }
    }
    result
}

fn register_agent(wallet: &str, model: &str, prompt: &str) {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(model.as_bytes());
    h.update(b"|");
    h.update(prompt.as_bytes());
    let agent_id: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    let path = "../data/registry.json";
    let mut list: Vec<serde_json::Value> = std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    list.retain(|e| e.get("wallet").and_then(|w| w.as_str()) != Some(wallet));
    list.push(serde_json::json!({
        "wallet": wallet,
        "agent_id": agent_id,
        "model": model,
        "prompt": prompt,
    }));
    let _ = std::fs::create_dir_all("../data");
    let _ = std::fs::write(path, serde_json::to_vec_pretty(&list).unwrap());
}

fn faction_cash(rpc: &RpcClient, faction: &Pubkey) -> u64 {
    fetch_faction(rpc, faction).map(|f| f.cash).unwrap_or(0)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WishRun {
    Pass,
    Applied(chain_wish_client::Intent),
    Block,
}

fn phase_name(phase: state::Phase) -> &'static str {
    match phase {
        state::Phase::Market => "market",
        state::Phase::Action => "action",
        state::Phase::Law => "law",
        _ => "other",
    }
}

fn wait_for_slot_after(rpc: &RpcClient, floor: u64) -> bool {
    for _ in 0..20 {
        if rpc.get_slot().is_ok_and(|slot| slot > floor) {
            return true;
        }
        sleep(Duration::from_millis(500));
    }
    false
}

fn retry_pending_chain_wish(rpc: &RpcClient, client: &mut chain_wish_client::ChainWishClient) {
    if let Some(signature) = client.pending_signature_without_slot().map(str::to_owned) {
        if let Ok(signature) = Signature::from_str(&signature) {
            match wait_receipt(rpc, &signature, Duration::ZERO) {
                Ok(receipt) => {
                    if let Some(slot) = receipt["slot"].as_u64() {
                        let _ = client.record_pending_slot(slot);
                    }
                }
                Err(error) if error["status"] == "failed" => {
                    let _ = client.mark_pending_unconfirmed();
                }
                Err(_) => {}
            }
        }
    }
    if client.has_pending() {
        let _ = client.retry_pending();
    }
}

fn expire_terminal_wishes(rpc: &RpcClient, client: &mut chain_wish_client::ChainWishClient) {
    retry_pending_chain_wish(rpc, client);
    if client.has_pending() {
        return;
    }
    // The server expires queued wishes for a finished game during this claim.
    // Failure is nonfatal: owner reads perform durable reconciliation on exit.
    let _ = client.claim();
}

/// A private wish has no effect until its typed instruction is sent and confirmed.
/// On an ambiguous send, do not run a fallback action in the same decision slot.
fn run_chain_wish(
    rpc: &RpcClient,
    signer: &Keypair,
    faction: Pubkey,
    client: Option<&mut chain_wish_client::ChainWishClient>,
    game: Pubkey,
    current: &state::Game,
    acted: &mut bool,
    voted: &mut bool,
) -> WishRun {
    let Some(client) = client else {
        return WishRun::Pass;
    };
    // A recovered receipt is status-only: do not claim or sign another action
    // until the existing confirmed signature receives a server acknowledgement.
    if client.has_pending() {
        retry_pending_chain_wish(rpc, client);
        return WishRun::Block;
    }
    let wish = match client.claim() {
        Ok(Some(wish)) => wish,
        Ok(None) => return WishRun::Pass,
        Err(_) => return WishRun::Block,
    };
    if matches!(wish.intent, chain_wish_client::Intent::Unsupported) {
        return if client.decline(&wish).is_ok() {
            WishRun::Pass
        } else {
            WishRun::Block
        };
    }
    if !wish.intent.matches_phase(phase_name(current.phase)) {
        return if client.defer(&wish).is_ok() {
            WishRun::Pass
        } else {
            WishRun::Block
        };
    }
    let Some(faction_state) = fetch_faction(rpc, &faction) else {
        return if client.defer(&wish).is_ok() {
            WishRun::Pass
        } else {
            WishRun::Block
        };
    };
    let unavailable = !faction_state.alive
        || faction_state.acted_stamp == current.stamp()
        || faction_state.voted_stamp == current.stamp()
        || matches!(wish.intent, chain_wish_client::Intent::SellOne) && faction_state.goods == 0
        || matches!(wish.intent, chain_wish_client::Intent::BuyOne)
            && alashi::logic::compute_purchase(
                1,
                current.sold_this_round,
                current.active_price_shift,
                current.active_boom,
            )
            .gross
                > faction_state.cash
        || matches!(
            wish.intent,
            chain_wish_client::Intent::VoteYes | chain_wish_client::Intent::VoteNo
        ) && current.law_card == 255;
    if unavailable {
        return if client.decline(&wish).is_ok() {
            WishRun::Pass
        } else {
            WishRun::Block
        };
    }
    let ix = match wish.intent {
        chain_wish_client::Intent::Produce => ix_produce(signer.pubkey(), game, faction),
        chain_wish_client::Intent::SellOne => ix_sell(1, signer.pubkey(), game, faction),
        chain_wish_client::Intent::BuyOne => ix_buy(1, signer.pubkey(), game, faction),
        chain_wish_client::Intent::VoteYes => {
            ix_vote(state::VoteChoice::Yes, signer.pubkey(), game, faction)
        }
        chain_wish_client::Intent::VoteNo => {
            ix_vote(state::VoteChoice::No, signer.pubkey(), game, faction)
        }
        chain_wish_client::Intent::Unsupported => unreachable!(),
    };
    let consumed_after_slot = match client.consume(&wish) {
        Ok(slot) => slot,
        Err(_) => return WishRun::Block,
    };
    if !wait_for_slot_after(rpc, consumed_after_slot) {
        let _ = client.unconfirmed(&wish);
        return WishRun::Block;
    }
    let tx = match prepare_ix(rpc, signer, ix) {
        Ok(tx) => tx,
        Err(_) => {
            let _ = client.unconfirmed(&wish);
            return WishRun::Block;
        }
    };
    let signature = tx.signatures[0].to_string();
    if client.remember_pending(&wish, &signature, None).is_err() {
        return WishRun::Block;
    }
    let receipt = match submit_confirmed(rpc, &tx) {
        Ok(receipt) => receipt,
        Err(error) => {
            if error["status"] == "failed" {
                let _ = client.mark_pending_unconfirmed();
            }
            return WishRun::Block;
        }
    };
    let Some(slot) = receipt["slot"].as_u64() else {
        return WishRun::Block;
    };
    if client.record_pending_slot(slot).is_err() {
        return WishRun::Block;
    }
    retry_pending_chain_wish(rpc, client);
    capture_events(&receipt);
    println!("  confirmed tx https://explorer.solana.com/tx/{signature}?cluster=devnet");
    match wish.intent {
        chain_wish_client::Intent::VoteYes | chain_wish_client::Intent::VoteNo => *voted = true,
        _ => *acted = true,
    }
    WishRun::Applied(wish.intent)
}

fn attach_wish_profiles(
    profiles: Option<chain_wish_client::Profiles>,
    bots: &mut [Bot],
) -> Result<Option<chain_wish_client::Profiles>, &'static str> {
    let Some(profiles) = profiles else {
        return Ok(None);
    };
    let mut matches = 0;
    for bot in bots {
        bot.wish_profile = profiles.matching_profile(&bot.kp.pubkey().to_string())?;
        matches += usize::from(bot.wish_profile.is_some());
    }
    if matches == 0 {
        return Err("chain_wish_profile_has_no_local_signer");
    }
    Ok(Some(profiles))
}

fn bind_wishes(
    profiles: Option<&chain_wish_client::Profiles>,
    bots: &mut [Bot],
    game: &Pubkey,
) -> Result<(), &'static str> {
    let Some(profiles) = profiles else {
        return Ok(());
    };
    for bot in bots {
        if let Some(profile) = bot.wish_profile.take() {
            bot.wish = Some(profiles.bind(profile, &game.to_string(), &bot.faction.to_string())?);
        }
    }
    Ok(())
}

/// Список всех фракций партии прямо из цепи (memcmp по Faction.game, offset 8
/// после дискриминатора). Нужен, когда в партию вступил гость: advance и settle
/// требуют полный набор фракций, а не только ботов хоста.
fn discover_factions(rpc: &RpcClient, game: &Pubkey) -> Vec<Pubkey> {
    use solana_rpc_client_api::{
        config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
        filter::{Memcmp, RpcFilterType},
        response::UiAccountEncoding,
    };
    let cfg = RpcProgramAccountsConfig {
        filters: Some(vec![RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
            8,
            game.as_ref(),
        ))]),
        // Публичный devnet RPC отвергает getProgramAccounts без encoding
        // (INVALID_PARAMS, 05.10.2026); с base64 тот же запрос проходит.
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            ..Default::default()
        },
        with_context: Some(false),
        sort_results: None,
    };
    match rpc.get_program_ui_accounts_with_config(&id(), cfg) {
        Ok(accs) => accs.iter().map(|(k, _)| *k).collect(),
        Err(e) => {
            println!("[ERROR] discover_factions: {e}");
            Vec::new()
        }
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Режим гостя: подключиться к чужой партии (--game <PUBKEY>) и играть
/// только свои ходы. Фазы двигает host: advance и settle гостю запрещены
/// (advance требует все фракции партии, settle требует game.admin).
fn run_join_mode(rpc: &RpcClient, game_str: &str, key_path: &str, name: &str) {
    let game = match game_str.parse::<Pubkey>() {
        Ok(p) => p,
        Err(_) => {
            println!("[ERROR] --game: неверный pubkey: {game_str}");
            return;
        }
    };
    let kp = load_or_create(key_path);
    let name: String = name.chars().take(16).collect();
    let faction = faction_pda(&game, &kp.pubkey());
    let wish_profiles = match chain_wish_client::Profiles::from_env() {
        Ok(profiles) => profiles,
        Err(error) => {
            eprintln!("[ERROR] chain wish opt-in unavailable: {error}");
            return;
        }
    };
    let wish_profile = match wish_profiles.as_ref() {
        Some(profiles) => match profiles.matching_profile(&kp.pubkey().to_string()) {
            Ok(Some(profile)) => Some(profile),
            Ok(None) => {
                eprintln!("[ERROR] chain wish profile has no local signer");
                return;
            }
            Err(error) => {
                eprintln!("[ERROR] chain wish opt-in unavailable: {error}");
                return;
            }
        },
        None => None,
    };
    register_agent(&kp.pubkey().to_string(), "join-v1", "guest-heuristic-v1");
    println!("=== JOIN MODE | rpc: {} ===", rpc_url());
    println!("guest: {} (ключ: {key_path})", kp.pubkey());
    println!("game: {game}");
    println!("faction PDA: {faction}");

    let mut tries = 0u32;
    let g0 = loop {
        if let Some(g) = fetch_game(rpc, &game) {
            break g;
        }
        tries += 1;
        if tries >= 20 {
            println!("[ERROR] game-аккаунт не найден за 10с: {game}");
            return;
        }
        sleep(Duration::from_millis(500));
    };
    println!(
        "game: phase {:?}, round {}, entry_fee {} lamports, factions {}/5, admin {}",
        g0.phase, g0.round, g0.entry_fee, g0.faction_count, g0.admin
    );
    let mut bal = rpc.get_balance(&kp.pubkey()).unwrap_or(0);
    println!("guest balance: {bal} lamports");
    if wish_profiles.is_none() && bal < g0.entry_fee + 20_000_000 {
        println!("мало средств, пробую devnet airdrop 1 SOL ...");
        if let Ok(sig) = rpc.request_airdrop(&kp.pubkey(), 1_000_000_000) {
            println!("  airdrop tx {sig}");
            sleep(Duration::from_secs(8));
        }
        bal = rpc.get_balance(&kp.pubkey()).unwrap_or(0);
        println!("guest balance: {bal} lamports");
    }
    if rpc.get_account(&faction).is_ok() {
        println!("фракция уже существует, повторный join не нужен");
    } else {
        if bal < g0.entry_fee + 20_000_000 {
            println!(
                "[ERROR] нужно >= {} lamports (взнос + комиссии), есть {}",
                g0.entry_fee + 20_000_000,
                bal
            );
            return;
        }
        if !matches!(g0.phase, state::Phase::Lobby) {
            println!(
                "[ERROR] партия уже идёт (phase {:?}); join возможен только в Lobby",
                g0.phase
            );
            return;
        }
        println!("join: плачу взнос {} lamports ...", g0.entry_fee);
        if !send_ix(rpc, &kp, ix_join(&name, kp.pubkey(), game, faction)) {
            println!("[ERROR] join отклонён: партия полная / фаза ушла / средства; см лог tx выше");
            return;
        }
        if !wait_account(rpc, &faction, "faction") {
            return;
        }
    }
    let mut wish_client = match (wish_profiles.as_ref(), wish_profile) {
        (Some(profiles), Some(profile)) => {
            match profiles.bind(profile, &game.to_string(), &faction.to_string()) {
                Ok(client) => Some(client),
                Err(error) => {
                    eprintln!("[ERROR] chain wish binding unavailable before gameplay: {error}");
                    return;
                }
            }
        }
        _ => None,
    };
    println!("в партии. Фазы двигает host, я играю свои ходы.");

    let started = now();
    let mut last_stamp: Option<u16> = None;
    let mut acted = false;
    let mut voted = false;
    let mut hb = 0u32;
    loop {
        hb += 1;
        if now() - started > 15 * 60 {
            println!("[timeout] 15 минут в режиме гостя, выхожу; партия продолжится без меня");
            break;
        }
        let tnow = chain_now(rpc);
        let g = match fetch_game(rpc, &game) {
            Some(g) => g,
            None => {
                sleep(Duration::from_secs(4));
                continue;
            }
        };
        let stamp = g.stamp();
        if last_stamp != Some(stamp) {
            acted = false;
            voted = false;
            last_stamp = Some(stamp);
        }
        let goods = fetch_faction(rpc, &faction).map(|f| f.goods).unwrap_or(0);
        match g.phase {
            state::Phase::Lobby => {
                if hb % 10 == 1 {
                    println!("[lobby r{}] жду старта от host, tnow={tnow}", g.round);
                }
            }
            state::Phase::Market => {
                if !acted {
                    match run_chain_wish(
                        rpc,
                        &kp,
                        faction,
                        wish_client.as_mut(),
                        game,
                        &g,
                        &mut acted,
                        &mut voted,
                    ) {
                        WishRun::Block => acted = true,
                        WishRun::Applied(_) | WishRun::Pass => {}
                    }
                }
                if !acted && goods > 0 {
                    println!("[market r{}] продаю {} товаров", g.round, goods);
                    if send_ix(rpc, &kp, ix_sell(goods, kp.pubkey(), game, faction)) {
                        acted = true;
                    }
                } else if !acted && hb % 10 == 1 {
                    println!("[market r{}] товаров нет, жду", g.round);
                }
            }
            state::Phase::Action => {
                if !acted {
                    match run_chain_wish(
                        rpc,
                        &kp,
                        faction,
                        wish_client.as_mut(),
                        game,
                        &g,
                        &mut acted,
                        &mut voted,
                    ) {
                        WishRun::Block => acted = true,
                        WishRun::Applied(_) | WishRun::Pass => {}
                    }
                }
                if !acted {
                    println!("[action r{}] произвожу (+2 товара)", g.round);
                    if send_ix(rpc, &kp, ix_produce(kp.pubkey(), game, faction)) {
                        acted = true;
                    }
                }
            }
            state::Phase::Law => {
                if !voted {
                    match run_chain_wish(
                        rpc,
                        &kp,
                        faction,
                        wish_client.as_mut(),
                        game,
                        &g,
                        &mut acted,
                        &mut voted,
                    ) {
                        WishRun::Block => voted = true,
                        WishRun::Applied(_) | WishRun::Pass => {}
                    }
                }
                if !voted && g.law_card != 255 {
                    println!("[law r{}] голосую NO", g.round);
                    if send_ix(
                        &rpc,
                        &kp,
                        ix_vote(state::VoteChoice::No, kp.pubkey(), game, faction),
                    ) {
                        voted = true;
                    }
                }
            }
            state::Phase::Aborted => {
                println!("=== ПАРТИЯ ПРЕРВАНА (оракул VRF); возврат взноса сделает host-кранк ===");
                break;
            }
            state::Phase::Finished => {
                println!("=== FINISHED ===");
                if let Some(f) = fetch_faction(rpc, &faction) {
                    println!(
                        "моя фракция {}: cash {} lamports, goods {}, influence {}",
                        f.name, f.cash, f.goods, f.influence
                    );
                }
                println!(
                    "settle выполнит host (permissionless); выплата придёт на кошелёк {}",
                    kp.pubkey()
                );
                println!("game: https://explorer.solana.com/address/{game}?cluster=devnet");
                if let Some(client) = wish_client.as_mut() {
                    expire_terminal_wishes(rpc, client);
                }
                break;
            }
        }
        let _ = std::io::Write::flush(&mut std::io::stdout());
        sleep(Duration::from_secs(2));
    }
}

fn host_timing(args: &[String]) -> Result<(i64, i64), String> {
    let number = |flag: &str, default: i64| -> Result<i64, String> {
        if let Some(i) = args.iter().position(|a| a == flag) {
            args.get(i + 1)
                .and_then(|s| s.parse::<i64>().ok())
                .filter(|n| *n > 0 && *n <= 86400)
                .ok_or_else(|| format!("{flag} requires seconds in 1..86400"))
        } else {
            Ok(default)
        }
    };
    let duration = number("--phase-duration", PHASE_DURATION)?;
    // 5 lobby intervals + 18 phases, plus time for confirmations and settlement.
    let timeout = number("--timeout", (duration * 23 + 120).max(12 * 60))?;
    if timeout < duration * 23 + 30 {
        return Err("--timeout must cover lobby + 18 phases + at least 30 seconds".into());
    }
    Ok((duration, timeout))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("session") {
        std::process::exit(session_cli::run(&args[2..]));
    }
    if args.get(1).map(String::as_str) == Some("agent") {
        std::process::exit(agent_cli::run(&args[2..]));
    }
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("bots [--phase-duration SECONDS] [--timeout SECONDS] [--no-llm]\nbots --game PUBKEY --name NAME --key LOCAL_FILE\nbots agent inspect|join|act --help");
        return;
    }
    let (phase_duration, host_timeout) = match host_timing(&args) {
        Ok(timing) => timing,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let rpc = RpcClient::new_with_timeout_and_commitment(
        rpc_url(),
        Duration::from_secs(10),
        CommitmentConfig::confirmed(),
    );
    if let Some(game_str) = flag_value(&args, "--game") {
        let key_path =
            flag_value(&args, "--key").unwrap_or_else(|| format!("{KEYS_DIR}/join.json"));
        let name = flag_value(&args, "--name").unwrap_or_else(|| "Guest".to_string());
        run_join_mode(&rpc, &game_str, &key_path, &name);
        return;
    }

    let bot1_kp = load_or_create(&format!("{KEYS_DIR}/bot1.json"));
    let bot2_kp = load_or_create(&format!("{KEYS_DIR}/bot2.json"));
    let wish_profiles = match chain_wish_client::Profiles::from_env() {
        Ok(profiles) => profiles,
        Err(error) => {
            eprintln!("[ERROR] chain wish opt-in unavailable: {error}");
            return;
        }
    };
    register_agent(
        &bot1_kp.pubkey().to_string(),
        "greedy-v1",
        "greedy-heuristic-v1",
    );
    let llm_cfg = if args.iter().any(|a| a == "--no-llm") {
        None
    } else {
        llm::llm_config()
    };
    register_agent(
        &bot2_kp.pubkey().to_string(),
        llm_cfg
            .as_ref()
            .map(|c| c.model.clone())
            .unwrap_or_default()
            .as_str(),
        llm::SYSTEM,
    );
    println!("rpc: {}", rpc_url());
    println!("bot1: {}", bot1_kp.pubkey());
    println!("bot2: {}", bot2_kp.pubkey());
    if wish_profiles.is_none() {
        ensure_funds(&rpc, "bot1", &bot1_kp);
        ensure_funds(&rpc, "bot2", &bot2_kp);
    }

    let llm = llm_cfg;
    println!(
        "Botagul brain: {}",
        if llm.is_some() {
            "GLM llm"
        } else {
            "greedy heuristic (нет ключа)"
        }
    );
    if demo_mode() {
        println!("=== РЕЖИМ ДЕМО: фазы 30с, закон 45с, нарратив для рассказчика ===");
    }
    let game_id = now() as u64;
    let game = game_pda(game_id);
    let mut bots = vec![
        Bot {
            faction: faction_pda(&game, &bot1_kp.pubkey()),
            kp: bot1_kp,
            name: "Aibot",
            acted: false,
            voted: false,
            goods: 0,
            bribed: false,
            wish_profile: None,
            wish: None,
        },
        Bot {
            faction: faction_pda(&game, &bot2_kp.pubkey()),
            kp: bot2_kp,
            name: "Botagul",
            acted: false,
            voted: false,
            goods: 0,
            bribed: false,
            wish_profile: None,
            wish: None,
        },
    ];
    let wish_profiles = match attach_wish_profiles(wish_profiles, &mut bots) {
        Ok(profiles) => profiles,
        Err(error) => {
            eprintln!("[ERROR] chain wish opt-in unavailable: {error}");
            return;
        }
    };
    if wish_profiles.is_some() {
        println!("chain wishes: opt-in enabled; pre-funded signer wallets required");
    }
    let faction_keys: Vec<Pubkey> = bots.iter().map(|b| b.faction).collect();
    let mut faction_keys = faction_keys;

    println!("=== ALASHI devnet party ===");
    println!("program: {}", id());
    println!("game {game_id}: https://explorer.solana.com/address/{game}?cluster=devnet");

    if !send_ix(
        &rpc,
        &bots[0].kp,
        ix_initialize(bots[0].kp.pubkey(), game, game_id, phase_duration),
    ) {
        println!("[ERROR] initialize не отправился");
        return;
    }
    wait_account(&rpc, &game, "game");
    send_ix(
        &rpc,
        &bots[0].kp,
        ix_join("Aibot", bots[0].kp.pubkey(), game, bots[0].faction),
    );
    wait_account(&rpc, &bots[0].faction, "faction1");
    send_ix(
        &rpc,
        &bots[1].kp,
        ix_join("Botagul", bots[1].kp.pubkey(), game, bots[1].faction),
    );
    wait_account(&rpc, &bots[1].faction, "faction2");
    if let Err(error) = bind_wishes(wish_profiles.as_ref(), &mut bots, &game) {
        eprintln!("[ERROR] chain wish binding unavailable before gameplay: {error}");
        return;
    }

    let started = now();
    let mut last_stamp: Option<u16> = None;
    let mut hb = 0u32;
    loop {
        hb += 1;
        if now() - started > host_timeout {
            println!("timeout, party unfinished");
            break;
        }
        let tnow = chain_now(&rpc);
        if hb % 5 == 1 {
            println!("[hb] tnow={tnow}");
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        let g = match fetch_game(&rpc, &game) {
            Some(g) => g,
            None => {
                sleep(Duration::from_secs(4));
                continue;
            }
        };
        let stamp = g.stamp();
        if last_stamp != Some(stamp) {
            for b in bots.iter_mut() {
                b.acted = false;
                b.voted = false;
            }
            last_stamp = Some(stamp);
        }
        if g.faction_count as usize != faction_keys.len() {
            let found = discover_factions(&rpc, &game);
            if !found.is_empty() {
                println!(
                    "[factions] в партии {} фракций, я знал {}: обновляю список по цепи",
                    g.faction_count,
                    faction_keys.len()
                );
                faction_keys = found;
            }
        }

        match g.phase {
            state::Phase::Lobby => {
                if tnow + 2 >= g.phase_ends_at {
                    println!("[lobby] advance");
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_advance(bots[0].kp.pubkey(), game, &faction_keys),
                    );
                }
            }
            state::Phase::Market => {
                let mut sold_now = g.sold_this_round;
                for bi in 0..bots.len() {
                    let b = &mut bots[bi];
                    if b.acted {
                        continue;
                    }
                    match run_chain_wish(
                        &rpc,
                        &b.kp,
                        b.faction,
                        b.wish.as_mut(),
                        game,
                        &g,
                        &mut b.acted,
                        &mut b.voted,
                    ) {
                        WishRun::Applied(chain_wish_client::Intent::SellOne) => {
                            sold_now += 1;
                            b.goods = fetch_faction(&rpc, &b.faction)
                                .map(|f| f.goods)
                                .unwrap_or(b.goods);
                            continue;
                        }
                        WishRun::Applied(_) => {
                            b.goods = fetch_faction(&rpc, &b.faction)
                                .map(|f| f.goods)
                                .unwrap_or(b.goods);
                            continue;
                        }
                        WishRun::Block => {
                            b.acted = true;
                            continue;
                        }
                        WishRun::Pass => {}
                    }
                    if bi == 1 && llm.is_some() {
                        let f = fetch_faction(&rpc, &b.faction);
                        let (cash, goods, infl) = f
                            .as_ref()
                            .map(|f| (f.cash, f.goods, f.influence))
                            .unwrap_or((0, 0, 0));
                        let price_hint = 1_000_000u64.saturating_mul(
                            (PRICE_TABLE[(g.sold_this_round as usize).min(15)] as i64
                                + g.active_price_shift as i64
                                + g.active_boom as i64)
                                .max(1) as u64,
                        );
                        let dec = llm_decide(
                            llm.as_ref().unwrap(),
                            "Базар",
                            g.round,
                            cash,
                            goods,
                            infl,
                            sold_now,
                            price_hint / 1_000_000,
                            "-",
                            false,
                        );
                        let act = dec
                            .as_ref()
                            .and_then(|d| d.get("action"))
                            .and_then(|a| a.as_str())
                            .map(|s| s.to_string());
                        let units = dec
                            .as_ref()
                            .and_then(|d| d.get("units"))
                            .and_then(|u| u.as_u64())
                            .unwrap_or(1) as u16;
                        match act.as_deref() {
                            Some("buy") if cash >= units as u64 * price_hint => {
                                println!(
                                    "[market r{}] Botagul (LLM) buys {} goods",
                                    g.round, units
                                );
                                if send_ix(
                                    &rpc,
                                    &b.kp,
                                    ix_buy(units, b.kp.pubkey(), game, b.faction),
                                ) {
                                    b.acted = true;
                                    if let Some(f) = fetch_faction(&rpc, &b.faction) {
                                        b.goods = f.goods;
                                    }
                                    continue;
                                }
                            }
                            Some("sell") if goods > 0 && units <= goods => {
                                println!(
                                    "[market r{}] Botagul (LLM) sells {} goods",
                                    g.round, units
                                );
                                if send_ix(
                                    &rpc,
                                    &b.kp,
                                    ix_sell(units, b.kp.pubkey(), game, b.faction),
                                ) {
                                    b.acted = true;
                                    sold_now += units;
                                    if let Some(f) = fetch_faction(&rpc, &b.faction) {
                                        b.goods = f.goods;
                                    }
                                    continue;
                                }
                            }
                            _ => {}
                        }
                        println!("[market r{}] Botagul (LLM) fallback: greedy sell", g.round);
                        if b.goods == 0 {
                            b.acted = true;
                        }
                    }
                    if bi == 1 && sold_now >= 2 && faction_cash(&rpc, &b.faction) >= 10_000_000 {
                        println!(
                            "[market r{}] {} BUYS 2 cheap goods (sold={})",
                            g.round, b.name, g.sold_this_round
                        );
                        if send_ix(&rpc, &b.kp, ix_buy(2, b.kp.pubkey(), game, b.faction)) {
                            b.acted = true;
                            b.goods += 2;
                        }
                        continue;
                    }
                    if b.goods > 0 {
                        demo_say(&format!("РАССКАЗЧИК: {} продаёт — смотри, цена в таблице падает (продано в раунде: {})", b.name, g.sold_this_round));
                        println!("[market r{}] {} sells {} goods", g.round, b.name, b.goods);
                        if send_ix(
                            &rpc,
                            &b.kp,
                            ix_sell(b.goods, b.kp.pubkey(), game, b.faction),
                        ) {
                            sold_now += b.goods;
                            b.acted = true;
                            b.goods = 0;
                        }
                    }
                }
                if tnow >= g.phase_ends_at {
                    println!("[market r{}] advance", g.round);
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_advance(bots[0].kp.pubkey(), game, &faction_keys),
                    );
                }
            }
            state::Phase::Action => {
                let target_faction = bots[1].faction;
                for (i, b) in bots.iter_mut().enumerate() {
                    if b.acted {
                        continue;
                    }
                    match run_chain_wish(
                        &rpc,
                        &b.kp,
                        b.faction,
                        b.wish.as_mut(),
                        game,
                        &g,
                        &mut b.acted,
                        &mut b.voted,
                    ) {
                        WishRun::Applied(_) => {
                            b.goods = fetch_faction(&rpc, &b.faction)
                                .map(|f| f.goods)
                                .unwrap_or(b.goods);
                            continue;
                        }
                        WishRun::Block => {
                            b.acted = true;
                            continue;
                        }
                        WishRun::Pass => {}
                    }
                    if i == 0 && !b.bribed && faction_cash(&rpc, &b.faction) >= 5_000_000 {
                        println!(
                            "[action r{}] {} bribes Botagul for influence",
                            g.round, b.name
                        );
                        if send_ix(
                            &rpc,
                            &b.kp,
                            ix_bribe(
                                5 * 1_000_000,
                                b.kp.pubkey(),
                                game,
                                b.faction,
                                target_faction,
                            ),
                        ) {
                            b.acted = true;
                            b.bribed = true;
                        }
                    } else if i == 1 && llm.is_some() {
                        let f = fetch_faction(&rpc, &b.faction);
                        let (cash, goods, infl) = f
                            .as_ref()
                            .map(|f| (f.cash, f.goods, f.influence))
                            .unwrap_or((0, 0, 0));
                        let dec = llm_decide(
                            llm.as_ref().unwrap(),
                            "Действие",
                            g.round,
                            cash,
                            goods,
                            infl,
                            g.sold_this_round,
                            0,
                            "-",
                            g.president == b.kp.pubkey(),
                        );
                        let act = dec
                            .as_ref()
                            .and_then(|d| d.get("action"))
                            .and_then(|a| a.as_str())
                            .map(|s| s.to_string());
                        match act.as_deref() {
                            Some("donkey") if cash >= 1_000_000 => {
                                println!("[action r{}] Botagul (LLM) buys a donkey", g.round);
                                if send_ix(&rpc, &b.kp, ix_donkey(b.kp.pubkey(), game, b.faction)) {
                                    b.acted = true;
                                    b.goods += 1;
                                }
                            }
                            Some("bribe") if cash >= 5_000_000 => {
                                println!(
                                    "[action r{}] Botagul (LLM) bribes for influence",
                                    g.round
                                );
                                if send_ix(
                                    &rpc,
                                    &b.kp,
                                    ix_bribe(
                                        5_000_000,
                                        b.kp.pubkey(),
                                        game,
                                        b.faction,
                                        target_faction,
                                    ),
                                ) {
                                    b.acted = true;
                                }
                            }
                            Some("produce") => {
                                println!("[action r{}] Botagul (LLM) produces", g.round);
                                if send_ix(&rpc, &b.kp, ix_produce(b.kp.pubkey(), game, b.faction))
                                {
                                    b.acted = true;
                                    b.goods += 2;
                                }
                            }
                            _ => {}
                        }
                        if !b.acted {
                            println!("[action r{}] Botagul fallback: produce", g.round);
                            if send_ix(&rpc, &b.kp, ix_produce(b.kp.pubkey(), game, b.faction)) {
                                b.acted = true;
                                b.goods += 2;
                            }
                        }
                    } else if i == 1 && g.round == 4 && llm.is_none() {
                        println!("[action r{}] {} buys a donkey", g.round, b.name);
                        if send_ix(&rpc, &b.kp, ix_donkey(b.kp.pubkey(), game, b.faction)) {
                            b.acted = true;
                            b.goods += 1;
                        }
                    } else {
                        println!("[action r{}] {} produces", g.round, b.name);
                        if send_ix(&rpc, &b.kp, ix_produce(b.kp.pubkey(), game, b.faction)) {
                            b.acted = true;
                            if let Some(f) = fetch_faction(&rpc, &b.faction) {
                                b.goods = f.goods;
                            }
                        }
                    }
                }
                if tnow >= g.phase_ends_at {
                    println!("[action r{}] advance", g.round);
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_advance(bots[0].kp.pubkey(), game, &faction_keys),
                    );
                }
            }
            state::Phase::Law => {
                if g.law_card != 255 {
                    demo_say(
                        "РАССКАЗЧИК: теперь ЖЮРИ — подключи кошелёк и проголосуй ПРОТИВ, 45 секунд",
                    );
                }
                for (i, b) in bots.iter_mut().enumerate() {
                    if b.voted {
                        continue;
                    }
                    match run_chain_wish(
                        &rpc,
                        &b.kp,
                        b.faction,
                        b.wish.as_mut(),
                        game,
                        &g,
                        &mut b.acted,
                        &mut b.voted,
                    ) {
                        WishRun::Applied(_) => continue,
                        WishRun::Block => {
                            b.voted = true;
                            continue;
                        }
                        WishRun::Pass => {}
                    }
                    if i == 1 && llm.is_some() {
                        let f = fetch_faction(&rpc, &b.faction);
                        let (cash, goods, infl) = f
                            .as_ref()
                            .map(|f| (f.cash, f.goods, f.influence))
                            .unwrap_or((0, 0, 0));
                        let law_name = match g.law_card {
                            0 => "Статус-кво",
                            1 => "Налог 10%",
                            2 => "Налог 20%",
                            3 => "Субсидия производителям",
                            4 => "Субсидия бедным",
                            5 => "Субсидия богатым",
                            6 => "Эмбарго (цены -2)",
                            7 => "Бум (+2 к цене)",
                            _ => "неизвестен",
                        };
                        let dec = llm_decide(
                            llm.as_ref().unwrap(),
                            "Закон",
                            g.round,
                            cash,
                            goods,
                            infl,
                            g.sold_this_round,
                            0,
                            law_name,
                            g.president == b.kp.pubkey(),
                        );
                        let choice_s = dec
                            .as_ref()
                            .and_then(|d| d.get("choice"))
                            .and_then(|c| c.as_str())
                            .unwrap_or("abstain")
                            .to_string();
                        let choice = match choice_s.as_str() {
                            "yes" => state::VoteChoice::Yes,
                            "no" => state::VoteChoice::No,
                            _ => state::VoteChoice::Abstain,
                        };
                        println!("[law r{}] Botagul (LLM) votes {}", g.round, choice_s);
                        if send_ix(&rpc, &b.kp, ix_vote(choice, b.kp.pubkey(), game, b.faction)) {
                            b.voted = true;
                            let want_veto = dec
                                .as_ref()
                                .and_then(|d| d.get("veto"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            if want_veto && g.president == b.kp.pubkey() && !g.veto_pending {
                                println!("[law r{}] Botagul (LLM, president) vetoes", g.round);
                                send_ix(&rpc, &b.kp, ix_veto(b.kp.pubkey(), game, b.faction));
                            }
                        }
                        continue;
                    }
                    let choice = if i == 0 {
                        state::VoteChoice::Yes
                    } else {
                        state::VoteChoice::No
                    };
                    println!("[law r{}] {} votes {:?}", g.round, b.name, choice);
                    if send_ix(&rpc, &b.kp, ix_vote(choice, b.kp.pubkey(), game, b.faction)) {
                        b.voted = true;
                    }
                }
                if g.round == 3 && g.president == bots[0].kp.pubkey() && !g.veto_pending {
                    demo_say("РАССКАЗЧИК: президент Aibot кладёт СЛЕПОЕ ВЕТО до подсчёта голосов — как на столе бумажками");
                    println!("[law r{}] Aibot (president) vetoes", g.round);
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_veto(bots[0].kp.pubkey(), game, bots[0].faction),
                    );
                }
                if tnow >= g.phase_ends_at {
                    println!("[law r{}] advance", g.round);
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_advance(bots[0].kp.pubkey(), game, &faction_keys),
                    );
                }
            }
            state::Phase::Aborted => {
                println!("=== ПАРТИЯ ПРЕРВАНА (оракул VRF) ===");
                break;
            }
            state::Phase::Finished => {
                println!("=== FINISHED ===");
                println!(
                    "rounds done, laws_passed: {}, last vote {}:{}",
                    g.laws_passed, g.yes_influence, g.no_influence
                );
                for b in bots.iter() {
                    if let Some(f) = fetch_faction(&rpc, &b.faction) {
                        println!(
                            "faction {} ({}): cash {} lamports, goods {}, influence {}",
                            f.name, b.faction, f.cash, f.goods, f.influence
                        );
                    }
                }
                demo_say("РАССКАЗЧИК: settle — банк делится 50/30 по богатству, рейк 5% виден в эксплорере");
                let bank = rpc.get_balance(&game).unwrap_or(0);
                println!("bank before settle: {bank} lamports");
                let fkeys = discover_factions(&rpc, &game);
                let wallets: Vec<Pubkey> = fkeys
                    .iter()
                    .filter_map(|f| fetch_faction(&rpc, f).map(|fa| fa.wallet))
                    .collect();
                if !g.settled && fkeys.len() == wallets.len() && !fkeys.is_empty() {
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_settle(bots[0].kp.pubkey(), game, &fkeys, &wallets, g.admin),
                    );
                    sleep(Duration::from_secs(5));
                    let after = rpc.get_balance(&game).unwrap_or(0);
                    println!("bank after settle: {after} lamports");
                    let rake_wallet = rpc.get_balance(&g.admin).unwrap_or(0);
                    println!(
                        "rake receiver ({}) balance: {} lamports",
                        g.admin, rake_wallet
                    );
                }
                for b in bots.iter_mut() {
                    if let Some(client) = b.wish.as_mut() {
                        expire_terminal_wishes(&rpc, client);
                    }
                }
                println!("game: https://explorer.solana.com/address/{game}?cluster=devnet");
                break;
            }
        }

        let _ = std::io::Write::flush(&mut std::io::stdout());
        sleep(Duration::from_secs(2));
    }
}
