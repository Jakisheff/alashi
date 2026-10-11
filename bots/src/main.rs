mod agent_cli;
mod barter;
mod chain_wish_client;
mod donkey_risk;
mod llm;
mod runner_auth;
mod session_cli;

use solana_signature::Signature;
use solana_signer::Signer;

use {
    alashi::{accounts, constants, id, instruction, state},
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{
            instruction::{AccountMeta, Instruction},
            system_program,
        },
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    solana_commitment_config::CommitmentConfig,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_rpc_client::rpc_client::RpcClient,
    solana_transaction::versioned::VersionedTransaction,
    std::{
        fs::{self, OpenOptions},
        io::Write,
        os::unix::fs::{OpenOptionsExt, PermissionsExt},
        path::{Path, PathBuf},
        str::FromStr,
        thread::sleep,
        time::{Duration, SystemTime, UNIX_EPOCH},
    },
};

const ENTRY_FEE: u64 = 50_000_000;
const PHASE_DURATION: i64 = 15;
const BARTER_DEMO_GOODS: u16 = 1;
const BARTER_DEMO_PRICE: u64 = 3_000_000;
const BARTER_REPORT_ATTEMPTS: usize = 18;
// Epoch 2 must never fall through to the epoch 0/1 donkey buy.
const DONKEY_RISK_FEE: u64 = constants::DONKEY_RISK_FEE;
const DONKEY_RISK_GOODS: u16 = constants::DONKEY_RISK_REWARD;

const SWITCHBOARD_DEVNET_PROGRAM: &str = "Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2";

const SPL_TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const ASSOCIATED_TOKEN_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
const NATIVE_MINT: &str = "So11111111111111111111111111111111111111112";
const LOOKUP_TABLE_PROGRAM: &str = "AddressLookupTab1e1111111111111111111111111";

#[derive(Clone, Debug)]
struct DonkeyOperatorSetup {
    queue: Pubkey,
    state: Pubkey,
    oracles: [Pubkey; constants::ROUNDS as usize],
}

fn parse_donkey_operator_setup(args: &[String]) -> Result<DonkeyOperatorSetup, String> {
    let parse = |flag: &str| -> Result<Pubkey, String> {
        flag_value(args, flag)
            .ok_or_else(|| format!("--epoch2-donkey-risk requires {flag}"))?
            .parse()
            .map_err(|_| format!("{flag} must be a public key"))
    };
    let queue = parse("--donkey-queue")?;
    let state = parse("--donkey-switchboard-state")?;
    let raw = flag_value(args, "--donkey-oracles").ok_or_else(|| {
        "--epoch2-donkey-risk requires --donkey-oracles with six public keys".to_string()
    })?;
    let keys: Result<Vec<_>, _> = raw.split(',').map(Pubkey::from_str).collect();
    let keys =
        keys.map_err(|_| "--donkey-oracles must contain six valid public keys".to_string())?;
    if keys.len() != constants::ROUNDS as usize
        || keys.iter().any(|key| *key == Pubkey::default())
        || keys
            .iter()
            .enumerate()
            .any(|(i, key)| keys[..i].contains(key))
    {
        return Err("--donkey-oracles requires six distinct non-default public keys".into());
    }
    Ok(DonkeyOperatorSetup {
        queue,
        state,
        oracles: keys.try_into().expect("length verified"),
    })
}

fn validate_donkey_operator_setup(
    rpc: &RpcClient,
    setup: &DonkeyOperatorSetup,
) -> Result<(), String> {
    let expected_owner =
        Pubkey::from_str(SWITCHBOARD_DEVNET_PROGRAM).expect("fixed Switchboard devnet id");
    for key in std::iter::once(&setup.queue)
        .chain(std::iter::once(&setup.state))
        .chain(setup.oracles.iter())
    {
        let account = rpc.get_account(key).map_err(|_| {
            "configured Switchboard queue/state/oracle is unavailable before game creation"
                .to_string()
        })?;
        if account.owner != expected_owner {
            return Err("configured Switchboard queue/state/oracle has the wrong owner".into());
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DonkeyRiskReason {
    Pending,
    LegacyEpoch,
    EntropyUnavailable,
    OracleConfigUnavailable,
    CrankUnavailable,
    WrongPhase,
    InactiveFaction,
    ActionAlreadyUsed,
    NoMarketRemaining,
    InsufficientCash,
    GoodsOverflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DonkeyRiskEligibility {
    Eligible,
    Ineligible(DonkeyRiskReason),
}

struct DonkeyRiskInput {
    epoch: u8,
    entropy_mode: u8,
    phase: state::Phase,
    round: u8,
    alive: bool,
    acted: bool,
    cash: u64,
    goods: u16,
    oracle_config_ready: bool,
    crank_ready: bool,
    pending: bool,
}

fn donkey_risk_eligibility(input: DonkeyRiskInput) -> DonkeyRiskEligibility {
    if input.pending {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::Pending);
    }
    if input.epoch != constants::EPOCH_DONKEY_RISK {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::LegacyEpoch);
    }
    if input.entropy_mode != constants::ENTROPY_SWITCHBOARD {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::EntropyUnavailable);
    }
    if !input.oracle_config_ready {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::OracleConfigUnavailable);
    }
    if !input.crank_ready {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::CrankUnavailable);
    }
    if input.phase != state::Phase::Action {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::WrongPhase);
    }
    if !input.alive {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::InactiveFaction);
    }
    if input.acted {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::ActionAlreadyUsed);
    }
    if input.round >= constants::ROUNDS {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::NoMarketRemaining);
    }
    if input.cash < DONKEY_RISK_FEE {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::InsufficientCash);
    }
    if input.goods > u16::MAX - DONKEY_RISK_GOODS {
        return DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::GoodsOverflow);
    }
    DonkeyRiskEligibility::Eligible
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DonkeyRiskSource {
    oracle: Pubkey,
    randomness: Pubkey,
}

fn donkey_oracle_pda(game: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[constants::DONKEY_ORACLE_SEED, game.as_ref()], &id()).0
}

fn donkey_source_pda(game: &Pubkey, round: u8) -> Pubkey {
    Pubkey::find_program_address(&[b"donkey-source", game.as_ref(), &[round]], &id()).0
}

fn donkey_init_accounts(
    game: Pubkey,
    round: u8,
    setup: &DonkeyOperatorSetup,
) -> (Pubkey, Pubkey, Pubkey, Pubkey) {
    let source = donkey_source_pda(&game, round);
    let provider =
        Pubkey::from_str(SWITCHBOARD_DEVNET_PROGRAM).expect("fixed Switchboard devnet id");
    let token = Pubkey::from_str(SPL_TOKEN_PROGRAM).expect("fixed token program");
    let mint = Pubkey::from_str(NATIVE_MINT).expect("fixed native mint");
    let ata = Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM).expect("fixed ata program");
    let escrow =
        Pubkey::find_program_address(&[source.as_ref(), token.as_ref(), mint.as_ref()], &ata).0;
    let lut_signer = Pubkey::find_program_address(&[b"LutSigner", source.as_ref()], &provider).0;
    (source, escrow, lut_signer, setup.queue)
}

/// Reads the immutable, game-bound portion of the epoch-2 source. The program
/// still validates Switchboard data immediately before debiting the fee; the
/// runner never treats an operator flag as proof of a future reveal.
fn donkey_risk_source(
    rpc: &RpcClient,
    game: &Pubkey,
    round: u8,
) -> Result<DonkeyRiskSource, DonkeyRiskReason> {
    if round == 0 || round > constants::ROUNDS {
        return Err(DonkeyRiskReason::NoMarketRemaining);
    }
    let oracle = donkey_oracle_pda(game);
    let account = rpc
        .get_account(&oracle)
        .map_err(|_| DonkeyRiskReason::OracleConfigUnavailable)?;
    if account.owner != id() {
        return Err(DonkeyRiskReason::OracleConfigUnavailable);
    }
    let mut data: &[u8] = &account.data;
    let config = alashi::DonkeyOracleConfig::try_deserialize(&mut data)
        .map_err(|_| DonkeyRiskReason::OracleConfigUnavailable)?;
    let index = (round - 1) as usize;
    let randomness = config.accounts[index];
    let selected_oracle = config.oracles[index];
    if config.game != *game
        || config.queue == Pubkey::default()
        || config.authority != oracle
        || randomness == Pubkey::default()
        || selected_oracle == Pubkey::default()
        || config.committed_rounds_mask & (1 << index) != 0
    {
        return Err(DonkeyRiskReason::OracleConfigUnavailable);
    }
    // This is a cheap source-owner check only. seed/reveal freshness, queue,
    // and authority are checked atomically by AttemptDonkey before mutation.
    let randomness_account = rpc
        .get_account(&randomness)
        .map_err(|_| DonkeyRiskReason::OracleConfigUnavailable)?;
    let devnet_owner = Pubkey::from_str("Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2")
        .expect("fixed Switchboard devnet id");
    if randomness_account.owner != devnet_owner {
        return Err(DonkeyRiskReason::OracleConfigUnavailable);
    }
    Ok(DonkeyRiskSource { oracle, randomness })
}

fn unavailable_epoch2_donkey(
    current: &state::Game,
    faction: &state::Faction,
    pending: bool,
) -> DonkeyRiskEligibility {
    // The frozen account/config read has not landed yet. Until it does, an
    // epoch-2 risk attempt is fail-closed and the caller chooses Produce.
    donkey_risk_eligibility(DonkeyRiskInput {
        epoch: current.epoch,
        entropy_mode: current.entropy_mode,
        phase: current.phase,
        round: current.round,
        alive: faction.alive,
        acted: faction.acted_stamp == current.stamp(),
        cash: faction.cash,
        goods: faction.goods,
        oracle_config_ready: false,
        crank_ready: false,
        pending,
    })
}

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

struct SelfHost {
    owner: runner_auth::OwnerSession,
    agent_file: PathBuf,
    primary_key_path: String,
    opponent_key_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DonkeyPending {
    game: Pubkey,
    faction: Pubkey,
    round: u8,
    signature: String,
    slot: Option<u64>,
}

fn donkey_pending_path(profile: &Path) -> PathBuf {
    profile.with_extension("donkey-pending.json")
}

fn load_donkey_pending(
    path: &Path,
    game: Pubkey,
    faction: Pubkey,
) -> Result<Option<DonkeyPending>, String> {
    let Ok(metadata) = fs::metadata(path) else {
        return Ok(None);
    };
    if metadata.permissions().mode() & 0o077 != 0 {
        return Err("donkey pending state must be mode 0600".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(path).map_err(|_| "cannot read donkey pending state")?)
            .map_err(|_| "invalid donkey pending state")?;
    let parsed = DonkeyPending {
        game: value["game"]
            .as_str()
            .and_then(|v| Pubkey::from_str(v).ok())
            .ok_or("invalid pending game")?,
        faction: value["faction"]
            .as_str()
            .and_then(|v| Pubkey::from_str(v).ok())
            .ok_or("invalid pending faction")?,
        round: value["round"]
            .as_u64()
            .and_then(|v| u8::try_from(v).ok())
            .ok_or("invalid pending round")?,
        signature: value["signature"]
            .as_str()
            .filter(|v| !v.is_empty() && v.len() <= 128)
            .ok_or("invalid pending signature")?
            .to_owned(),
        slot: value["slot"].as_u64(),
    };
    Ok((parsed.game == game && parsed.faction == faction).then_some(parsed))
}

fn save_donkey_pending(path: &Path, pending: &DonkeyPending) -> Result<(), String> {
    let tmp = path.with_extension("donkey-pending.tmp");
    let bytes = serde_json::to_vec(&serde_json::json!({
        "game": pending.game.to_string(), "faction": pending.faction.to_string(),
        "round": pending.round, "signature": pending.signature, "slot": pending.slot,
    }))
    .map_err(|_| "cannot encode donkey pending state")?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|_| "cannot create donkey pending state")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "cannot persist donkey pending state")?;
    fs::rename(&tmp, path).map_err(|_| "cannot replace donkey pending state")?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))
        .map_err(|_| "cannot protect donkey pending state")?;
    Ok(())
}

fn clear_donkey_pending(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("cannot remove donkey pending state".into()),
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
    donkey_pending: Option<DonkeyPending>,
    donkey_store: Option<PathBuf>,
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

fn ix_initialize(
    admin: Pubkey,
    game: Pubkey,
    game_id: u64,
    phase_duration: i64,
    entropy_mode: u8,
    epoch: u8,
) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Initialize {
            game_id,
            entry_fee: ENTRY_FEE,
            phase_duration,
            entropy_mode,
            epoch,
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

fn ix_join_epoch2(
    name: &str,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    oracle: Pubkey,
) -> Instruction {
    let mut metas = accounts::Join {
        player,
        game,
        faction,
        system_program: anchor_lang::solana_program::system_program::ID,
    }
    .to_account_metas(None);
    metas.push(AccountMeta::new_readonly(oracle, false));
    Instruction::new_with_bytes(
        id(),
        &instruction::Join {
            name: name.to_string(),
        }
        .data(),
        metas,
    )
}

fn ix_init_donkey_source(
    admin: Pubkey,
    game: Pubkey,
    round: u8,
    recent_slot: u64,
    setup: &DonkeyOperatorSetup,
) -> Instruction {
    let (randomness, reward_escrow, lut_signer, queue) = donkey_init_accounts(game, round, setup);
    let oracle = donkey_oracle_pda(&game);
    let provider =
        Pubkey::from_str(SWITCHBOARD_DEVNET_PROGRAM).expect("fixed Switchboard devnet id");
    let lookup = Pubkey::from_str(LOOKUP_TABLE_PROGRAM).expect("fixed lookup table program");
    let lut =
        Pubkey::find_program_address(&[lut_signer.as_ref(), &recent_slot.to_le_bytes()], &lookup).0;
    let token = Pubkey::from_str(SPL_TOKEN_PROGRAM).expect("fixed token program");
    let ata = Pubkey::from_str(ASSOCIATED_TOKEN_PROGRAM).expect("fixed ata program");
    let mint = Pubkey::from_str(NATIVE_MINT).expect("fixed native mint");
    let metas = vec![
        AccountMeta::new(admin, true),
        AccountMeta::new_readonly(game, false),
        AccountMeta::new(randomness, false),
        AccountMeta::new_readonly(oracle, false),
        AccountMeta::new(reward_escrow, false),
        AccountMeta::new(queue, false),
        AccountMeta::new_readonly(system_program::ID, false),
        AccountMeta::new_readonly(token, false),
        AccountMeta::new_readonly(ata, false),
        AccountMeta::new_readonly(mint, false),
        AccountMeta::new_readonly(setup.state, false),
        AccountMeta::new_readonly(lut_signer, false),
        AccountMeta::new(lut, false),
        AccountMeta::new_readonly(lookup, false),
        AccountMeta::new_readonly(provider, false),
    ];
    Instruction::new_with_bytes(
        id(),
        &instruction::InitDonkeySource { round, recent_slot }.data(),
        metas,
    )
}

fn ix_configure_donkey_oracles(
    admin: Pubkey,
    game: Pubkey,
    oracle: Pubkey,
    setup: &DonkeyOperatorSetup,
) -> Instruction {
    let mut metas = accounts::ConfigureDonkeyOracles {
        admin,
        game,
        oracle,
        system_program: system_program::ID,
    }
    .to_account_metas(None);
    metas.extend(
        (1..=constants::ROUNDS)
            .map(|round| AccountMeta::new_readonly(donkey_source_pda(&game, round), false)),
    );
    metas.extend(
        setup
            .oracles
            .iter()
            .map(|key| AccountMeta::new_readonly(*key, false)),
    );
    Instruction::new_with_bytes(id(), &instruction::ConfigureDonkeyOracles {}.data(), metas)
}

fn ix_attempt_donkey(
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    oracle: Pubkey,
    randomness: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::AttemptDonkey {}.data(),
        accounts::AttemptDonkey {
            player,
            game,
            faction,
            oracle,
            randomness,
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

fn ix_advance_epoch2(
    crank: Pubkey,
    game: Pubkey,
    factions: &[Pubkey],
    oracle: Pubkey,
    randomness: Option<Pubkey>,
) -> Instruction {
    let mut metas = accounts::Advance {
        crank,
        game,
        hashes: solana_sysvar::slot_hashes::ID,
    }
    .to_account_metas(None);
    metas.extend(factions.iter().map(|f| AccountMeta::new(*f, false)));
    metas.push(AccountMeta::new(oracle, false));
    if let Some(randomness) = randomness {
        metas.push(AccountMeta::new_readonly(randomness, false));
    }
    Instruction::new_with_bytes(id(), &instruction::Advance {}.data(), metas)
}

fn advance_instruction(
    current: &state::Game,
    now: i64,
    crank: Pubkey,
    game: Pubkey,
    factions: &[Pubkey],
) -> Result<Instruction, DonkeyRiskReason> {
    if current.epoch != constants::EPOCH_DONKEY_RISK {
        return Ok(ix_advance(crank, game, factions));
    }
    // The operator tool owns Action→Law: it atomically commits the exact
    // configured Switchboard source and advances the game. A standalone bot
    // transaction is never a retry substitute for that atomic boundary. Only
    // the canonical post-grace technical abort may omit the randomness account.
    if current.phase == state::Phase::Action
        && now
            < current
                .phase_ends_at
                .saturating_add(constants::DONKEY_COMMIT_GRACE_SECONDS)
    {
        return Err(DonkeyRiskReason::CrankUnavailable);
    }
    Ok(ix_advance_epoch2(
        crank,
        game,
        factions,
        donkey_oracle_pda(&game),
        None,
    ))
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

fn ix_barter_propose(
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    goods: u16,
    price: u64,
) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::BarterPropose { goods, price }.data(),
        accounts::BarterPropose {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_barter_accept(
    offer_id: u64,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    seller: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::BarterAccept { offer_id }.data(),
        accounts::BarterAccept {
            player,
            game,
            faction,
            seller,
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

enum DonkeyRun {
    Attempted,
    Block,
    Pass(DonkeyRiskReason),
}

/// Attempts only after the frozen source reads as configured and the external
/// operator was explicitly required.  It persists the local signature before
/// submission; ambiguous or malformed receipts block a fallback action.
fn run_epoch2_donkey(
    rpc: &RpcClient,
    bot: &mut Bot,
    game: Pubkey,
    current: &state::Game,
    external_crank: bool,
) -> DonkeyRun {
    let source = donkey_risk_source(rpc, &game, current.round);
    let faction = match fetch_faction(rpc, &bot.faction) {
        Some(value) => value,
        None => return DonkeyRun::Pass(DonkeyRiskReason::InactiveFaction),
    };
    let eligibility = donkey_risk_eligibility(DonkeyRiskInput {
        epoch: current.epoch,
        entropy_mode: current.entropy_mode,
        phase: current.phase,
        round: current.round,
        alive: faction.alive,
        acted: faction.acted_stamp == current.stamp(),
        cash: faction.cash,
        goods: faction.goods,
        oracle_config_ready: source.is_ok(),
        crank_ready: external_crank,
        pending: bot.donkey_pending.is_some(),
    });
    if let DonkeyRiskEligibility::Ineligible(reason) = eligibility {
        return DonkeyRun::Pass(reason);
    }
    let source = source.expect("eligibility checked source");
    let tx = match prepare_ix(
        rpc,
        &bot.kp,
        ix_attempt_donkey(
            bot.kp.pubkey(),
            game,
            bot.faction,
            source.oracle,
            source.randomness,
        ),
    ) {
        Ok(tx) => tx,
        Err(_) => return DonkeyRun::Block,
    };
    let pending = DonkeyPending {
        game,
        faction: bot.faction,
        round: current.round,
        signature: tx.signatures[0].to_string(),
        slot: None,
    };
    let Some(path) = bot.donkey_store.as_ref() else {
        return DonkeyRun::Block;
    };
    if save_donkey_pending(path, &pending).is_err() {
        return DonkeyRun::Block;
    }
    bot.donkey_pending = Some(pending);
    match submit_confirmed(rpc, &tx) {
        Ok(receipt) => {
            if let Some(proof) = donkey_risk::verified_attempt(
                &receipt,
                game,
                bot.faction,
                current.round,
                DONKEY_RISK_FEE,
            ) {
                if let Some(pending) = bot.donkey_pending.as_mut() {
                    pending.slot = Some(proof.slot);
                    let _ = save_donkey_pending(path, pending);
                }
                capture_events(&receipt);
                return DonkeyRun::Attempted;
            }
            DonkeyRun::Block
        }
        Err(error) if error["status"] == "rejected" || error["status"] == "failed" => {
            let _ = clear_donkey_pending(path);
            bot.donkey_pending = None;
            DonkeyRun::Pass(DonkeyRiskReason::OracleConfigUnavailable)
        }
        Err(_) => DonkeyRun::Block,
    }
}

/// Normalizes only a successful `getTransaction` result for event proof.
/// Program logs are emitted before a transaction can still roll back, so a
/// non-null `meta.err` must never clear a durable pending donkey receipt.
fn confirmed_transaction_receipt(
    signature: &str,
    transaction: &serde_json::Value,
) -> Option<serde_json::Value> {
    let slot = transaction.get("slot")?.as_u64()?;
    let meta = transaction.get("meta")?.as_object()?;
    if !meta.get("err")?.is_null() {
        return None;
    }
    let logs = meta.get("logMessages")?.as_array()?;
    if !logs.iter().all(serde_json::Value::is_string) {
        return None;
    }
    Some(serde_json::json!({
        "status": "confirmed",
        "signature": signature,
        "slot": slot,
        "log_messages": logs,
    }))
}

fn find_donkey_resolution(
    rpc: &RpcClient,
    pending: &DonkeyPending,
) -> Result<Option<donkey_risk::Resolution>, ()> {
    use solana_rpc_client_api::request::RpcRequest;
    let signatures = rpc
        .send::<serde_json::Value>(
            RpcRequest::GetSignaturesForAddress,
            serde_json::json!([pending.game.to_string(), {"limit": 20, "commitment": "confirmed"}]),
        )
        .map_err(|_| ())?;
    for signature in signatures
        .as_array()
        .ok_or(())?
        .iter()
        .filter_map(|row| row["signature"].as_str())
    {
        let receipt = rpc.send::<serde_json::Value>(
            RpcRequest::GetTransaction,
            serde_json::json!([signature, {"encoding":"json", "commitment":"confirmed", "maxSupportedTransactionVersion":0}]),
        ).map_err(|_| ())?;
        let Some(receipt) = confirmed_transaction_receipt(signature, &receipt) else {
            // Keep the durable receipt pending when RPC data is malformed or
            // describes a rolled-back transaction. A later canonical receipt
            // may still prove the exact resolution.
            continue;
        };
        if let Some(resolution) = donkey_risk::verified_resolution(
            &receipt,
            pending.game,
            pending.faction,
            pending.round,
            DONKEY_RISK_FEE,
        ) {
            return Ok(Some(resolution));
        }
    }
    Ok(None)
}

fn reconcile_donkey_pending(rpc: &RpcClient, bot: &mut Bot) {
    let Some(pending) = bot.donkey_pending.clone() else {
        return;
    };
    match find_donkey_resolution(rpc, &pending) {
        Ok(Some(resolution)) => {
            let label = match resolution {
                donkey_risk::Resolution::Win => "verified win",
                donkey_risk::Resolution::Empty => "verified empty draw",
                donkey_risk::Resolution::Abort => "technical oracle abort",
            };
            println!(
                "[donkey r{}] {label}; clearing pending receipt",
                pending.round
            );
            if let Some(path) = bot.donkey_store.as_ref() {
                if clear_donkey_pending(path).is_ok() {
                    bot.donkey_pending = None;
                }
            }
        }
        Ok(None) | Err(()) => {}
    }
}

fn faction_cash(rpc: &RpcClient, faction: &Pubkey) -> u64 {
    fetch_faction(rpc, faction).map(|f| f.cash).unwrap_or(0)
}

fn epoch_for_barter_demo(enabled: bool) -> u8 {
    if enabled {
        constants::EPOCH_90S
    } else {
        constants::EPOCH_CLASSIC
    }
}

fn barter_demo_allowed(enabled: bool, joining: bool, self_host: bool, no_llm: bool) -> bool {
    !enabled || (!joining && self_host && no_llm)
}

fn confirmed_barter_ix(
    rpc: &RpcClient,
    signer: &Keypair,
    ix: Instruction,
) -> Option<serde_json::Value> {
    match send_ix_confirmed(rpc, signer, ix) {
        Ok(receipt) => {
            capture_events(&receipt);
            Some(receipt)
        }
        Err(error) if error["status"] == "unknown" => {
            eprintln!("barter receipt unknown; stop and inspect chain state before resuming");
            std::process::exit(3);
        }
        Err(error) => {
            eprintln!("barter transaction was not confirmed: {error}");
            None
        }
    }
}

fn report_barter_acceptance(
    client: Option<&chain_wish_client::ChainWishClient>,
    game: Pubkey,
    proposer: Pubkey,
    offer_id: u64,
    receipt: &barter::ConfirmedReceipt,
) {
    let Some(client) = client else {
        eprintln!("barter acceptance confirmed on chain; no bound primary journal capability");
        return;
    };
    for attempt in 0..BARTER_REPORT_ATTEMPTS {
        match client.report_accepted_conversation(
            offer_id,
            &proposer.to_string(),
            &receipt.signature,
            receipt.slot,
        ) {
            Ok(chain_wish_client::ConversationReport::Accepted) => {
                println!(
                    "[barter r2] conversation accepted report acknowledged for game {game} at slot {}",
                    receipt.slot
                );
                return;
            }
            Ok(chain_wish_client::ConversationReport::Rejected) => {
                eprintln!("barter acceptance confirmed on chain; conversation report rejected");
                return;
            }
            Ok(chain_wish_client::ConversationReport::Retryable) | Err(_)
                if attempt + 1 < BARTER_REPORT_ATTEMPTS =>
            {
                sleep(Duration::from_secs(1))
            }
            Ok(chain_wish_client::ConversationReport::Retryable) | Err(_) => {
                eprintln!(
                    "barter acceptance confirmed on chain; conversation report still pending"
                );
                return;
            }
        }
    }
}

/// One deterministic epoch-1 demonstration: in Market r2, the unbound local
/// opponent opens one public offer and the bound primary accepts it. The on-chain
/// offer is open to any player; a missing or changed offer is not a decline.
fn run_barter_demo_market(rpc: &RpcClient, game: Pubkey, current: &state::Game, bots: &mut [Bot]) {
    if bots.len() != 2 {
        return;
    }
    let buyer = 0usize;
    let seller = 1usize;

    // A private owner instruction has priority over the presentation schedule.
    // If it takes or blocks the Market decision, do not add an unrelated barter.
    match run_chain_wish(
        rpc,
        &bots[buyer].kp,
        bots[buyer].faction,
        bots[buyer].wish.as_mut(),
        game,
        current,
        &mut bots[buyer].acted,
        &mut bots[buyer].voted,
    ) {
        WishRun::Pass => {}
        WishRun::Applied(_) => return,
        WishRun::Block => {
            bots[buyer].acted = true;
            return;
        }
    }

    let buyer_goods = fetch_faction(rpc, &bots[buyer].faction)
        .map(|f| f.goods)
        .unwrap_or(0);
    if buyer_goods == 0 {
        eprintln!("barter demo skipped: primary has no goods to sell for barter cash");
        return;
    }
    if !send_ix(
        rpc,
        &bots[buyer].kp,
        ix_sell(
            buyer_goods,
            bots[buyer].kp.pubkey(),
            game,
            bots[buyer].faction,
        ),
    ) {
        return;
    }
    bots[buyer].acted = true;

    let seller_goods = fetch_faction(rpc, &bots[seller].faction)
        .map(|f| f.goods)
        .unwrap_or(0);
    if seller_goods < BARTER_DEMO_GOODS {
        eprintln!("barter demo skipped: opponent has no goods for the offer");
        return;
    }
    let Some(current) = fetch_game(rpc, &game) else {
        eprintln!("barter demo skipped: game state unavailable after market sale");
        return;
    };
    if current.epoch != constants::EPOCH_90S || current.phase != state::Phase::Market {
        eprintln!("barter demo skipped: no longer in epoch-1 Market");
        return;
    }
    let offer_id = current.barter_next_id;
    let proposer = bots[seller].faction;
    let offer_receipt = confirmed_barter_ix(
        rpc,
        &bots[seller].kp,
        ix_barter_propose(
            bots[seller].kp.pubkey(),
            game,
            proposer,
            BARTER_DEMO_GOODS,
            BARTER_DEMO_PRICE,
        ),
    );
    let Some(offer_receipt) = offer_receipt else {
        return;
    };
    let offer_receipt = barter::with_alashi_logs(&offer_receipt);
    if barter::verified_offer(
        &offer_receipt,
        game,
        proposer,
        offer_id,
        BARTER_DEMO_GOODS,
        BARTER_DEMO_PRICE,
    )
    .is_none()
    {
        eprintln!("barter offer receipt lacked the exact successful program event");
        return;
    }
    // Do not use the ordinary fallback sale after a confirmed open offer.
    bots[seller].acted = true;

    let still_open = fetch_game(rpc, &game)
        .map(|latest| {
            latest.barter_offers.iter().any(|offer| {
                offer.id == offer_id
                    && offer.from == bots[seller].kp.pubkey()
                    && offer.goods == BARTER_DEMO_GOODS
                    && offer.price == BARTER_DEMO_PRICE
            })
        })
        .unwrap_or(false);
    if !still_open {
        eprintln!("barter offer is no longer open; no acceptance is claimed");
        return;
    }

    let accept_receipt = confirmed_barter_ix(
        rpc,
        &bots[buyer].kp,
        ix_barter_accept(
            offer_id,
            bots[buyer].kp.pubkey(),
            game,
            bots[buyer].faction,
            proposer,
        ),
    );
    let Some(accept_receipt) = accept_receipt else {
        return;
    };
    let accept_receipt = barter::with_alashi_logs(&accept_receipt);
    let Some(receipt) = barter::verified_accept(
        &accept_receipt,
        game,
        bots[buyer].faction,
        proposer,
        offer_id,
    ) else {
        eprintln!("barter acceptance receipt lacked the exact successful program event");
        return;
    };
    println!(
        "[barter r2] confirmed open offer {offer_id} and acceptance at slot {}",
        receipt.slot
    );
    // Only the primary's bound capability can report acceptance. The server
    // imports the preceding opponent offer from canonical chain history.
    report_barter_acceptance(
        bots[buyer].wish.as_ref(),
        game,
        proposer,
        offer_id,
        &receipt,
    );
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

fn may_join(existing_faction: bool, resume: bool) -> bool {
    !existing_faction && !resume
}

fn may_auto_airdrop(
    has_agent_profile: bool,
    resume: bool,
    has_wish_profile: bool,
    low_balance: bool,
) -> bool {
    !has_agent_profile && !resume && !has_wish_profile && low_balance
}

/// Режим гостя: подключиться к чужой партии (--game <PUBKEY>) и играть
/// только свои ходы. Фазы двигает host: advance и settle гостю запрещены
/// (advance требует все фракции партии, settle требует game.admin).
fn run_join_mode(
    rpc: &RpcClient,
    game_str: &str,
    key_path: &str,
    name: &str,
    agent_file: Option<&str>,
    resume: bool,
) {
    let game = match game_str.parse::<Pubkey>() {
        Ok(p) => p,
        Err(_) => {
            println!("[ERROR] --game: неверный pubkey: {game_str}");
            return;
        }
    };
    if resume && agent_file.is_none() {
        eprintln!("[ERROR] --resume requires --agent-file for the previously registered agent");
        return;
    }
    let kp = match agent_file {
        Some(_) => match agent_cli::read_key(key_path) {
            Ok(key) => key,
            Err(_) => {
                eprintln!("[ERROR] registered runner needs an existing mode-600 local key");
                return;
            }
        },
        None => load_or_create(key_path),
    };
    let name: String = name.chars().take(16).collect();
    let faction = faction_pda(&game, &kp.pubkey());
    let wish_profiles = if agent_file.is_some() {
        None
    } else {
        match chain_wish_client::Profiles::from_env() {
            Ok(profiles) => profiles,
            Err(error) => {
                eprintln!("[ERROR] chain wish opt-in unavailable: {error}");
                return;
            }
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
    if agent_file.is_none() {
        register_agent(&kp.pubkey().to_string(), "join-v1", "guest-heuristic-v1");
    }
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
    if may_auto_airdrop(
        agent_file.is_some(),
        resume,
        wish_profiles.is_some(),
        bal < g0.entry_fee + 20_000_000,
    ) {
        println!("мало средств, пробую devnet airdrop 1 SOL ...");
        if let Ok(sig) = rpc.request_airdrop(&kp.pubkey(), 1_000_000_000) {
            println!("  airdrop tx {sig}");
            sleep(Duration::from_secs(8));
        }
        bal = rpc.get_balance(&kp.pubkey()).unwrap_or(0);
        println!("guest balance: {bal} lamports");
    }
    let existing_faction = rpc.get_account(&faction).is_ok();
    if existing_faction {
        println!("фракция уже существует, повторный join не нужен");
    } else if !may_join(existing_faction, resume) {
        eprintln!("[ERROR] --resume found no existing faction for this registered wallet; no Join was sent");
        return;
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
    if resume {
        let Some(faction_state) = fetch_faction(rpc, &faction) else {
            eprintln!(
                "[ERROR] --resume cannot validate the existing faction; no owner link was minted"
            );
            return;
        };
        if faction_state.game != game || faction_state.wallet != kp.pubkey() {
            eprintln!("[ERROR] --resume faction does not match this Game and local wallet");
            return;
        }
        let agent_file = Path::new(agent_file.expect("--resume required it"));
        let owner = match runner_auth::authenticate_owner(agent_file, key_path) {
            Ok(owner) => owner,
            Err(error) => {
                eprintln!("[ERROR] --resume owner proof unavailable: {error}");
                return;
            }
        };
        let grant = match std::env::var("ALASHI_PAIRING_GRANT") {
            Ok(grant) => grant,
            Err(_) => {
                eprintln!("[ERROR] --resume needs ALASHI_PAIRING_GRANT in the runner environment");
                return;
            }
        };
        match runner_auth::complete_existing_pairing(
            &owner,
            &grant,
            &game.to_string(),
            &faction.to_string(),
        ) {
            Ok(url) => println!("owner link: {url}"),
            Err(error) => eprintln!("[ERROR] --resume owner link unavailable: {error}"),
        }
        return;
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
    if let Some(agent_file) = agent_file {
        let api = std::env::var("ALASHI_CHAIN_WISH_API").unwrap_or_default();
        if api != "https://alashi.network" {
            eprintln!(
                "[ERROR] registered runner requires ALASHI_CHAIN_WISH_API=https://alashi.network"
            );
            return;
        }
        let agent_file = Path::new(agent_file);
        let (_profile, binding) = match runner_auth::authenticate_and_bind(
            agent_file,
            key_path,
            &game.to_string(),
            &faction.to_string(),
        ) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("[ERROR] owner proof/bind unavailable: {error}");
                return;
            }
        };
        wish_client = match chain_wish_client::ChainWishClient::bind_public(
            &api,
            agent_file,
            binding.clone(),
        ) {
            Ok(client) => Some(client),
            Err(error) => {
                eprintln!("[ERROR] public wish capability unavailable: {error}");
                return;
            }
        };
        if let Ok(grant) = std::env::var("ALASHI_PAIRING_GRANT") {
            let owner = match runner_auth::authenticate_owner(agent_file, key_path) {
                Ok(owner) => owner,
                Err(error) => {
                    eprintln!("[ERROR] browser pairing unavailable: {error}");
                    return;
                }
            };
            match runner_auth::complete_pairing(&owner, &binding, &grant) {
                Ok(url) => println!("owner link: {url}"),
                Err(error) => {
                    eprintln!("[ERROR] browser pairing unavailable: {error}");
                    return;
                }
            }
        }
    }
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

#[cfg(test)]
mod runner_mode_tests {
    use super::{
        advance_instruction, barter_demo_allowed, confirmed_transaction_receipt,
        donkey_init_accounts, donkey_risk_eligibility, donkey_source_pda, epoch_for_barter_demo,
        ix_attempt_donkey, ix_barter_accept, ix_barter_propose, ix_configure_donkey_oracles,
        ix_donkey, ix_init_donkey_source, ix_initialize, ix_join_epoch2, may_auto_airdrop,
        may_join, DonkeyOperatorSetup, DonkeyRiskEligibility, DonkeyRiskInput, DonkeyRiskReason,
        DONKEY_RISK_FEE, DONKEY_RISK_GOODS,
    };
    use alashi::{constants, events::DonkeyResolved, id, instruction, state};
    use anchor_lang::{prelude::Pubkey, AnchorDeserialize, Event};
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn resume_never_joins_a_missing_faction() {
        assert!(!may_join(false, true));
        assert!(!may_join(true, true));
        assert!(may_join(false, false));
    }
    #[test]
    fn registered_resume_never_requests_airdrop() {
        assert!(!may_auto_airdrop(true, true, false, true));
        assert!(!may_auto_airdrop(true, false, false, true));
        assert!(may_auto_airdrop(false, false, false, true));
    }

    #[test]
    fn barter_mode_is_explicit_host_only_and_keeps_classic_default() {
        assert_eq!(epoch_for_barter_demo(false), constants::EPOCH_CLASSIC);
        assert_eq!(epoch_for_barter_demo(true), constants::EPOCH_90S);
        assert!(barter_demo_allowed(true, false, true, true));
        assert!(!barter_demo_allowed(true, true, true, true));
        assert!(!barter_demo_allowed(true, false, false, true));
        assert!(!barter_demo_allowed(true, false, true, false));
    }

    #[test]
    fn barter_builders_encode_existing_epoch_one_instructions() {
        let admin = Pubkey::new_unique();
        let game = Pubkey::new_unique();
        let faction = Pubkey::new_unique();
        let seller = Pubkey::new_unique();
        let init = ix_initialize(
            admin,
            game,
            9,
            15,
            constants::ENTROPY_SLOTHASH,
            constants::EPOCH_90S,
        );
        let decoded = instruction::Initialize::try_from_slice(&init.data[8..]).unwrap();
        assert_eq!(decoded.epoch, constants::EPOCH_90S);

        let offer = ix_barter_propose(admin, game, faction, 1, 3_000_000);
        let decoded = instruction::BarterPropose::try_from_slice(&offer.data[8..]).unwrap();
        assert_eq!((decoded.goods, decoded.price), (1, 3_000_000));

        let accept = ix_barter_accept(4, admin, game, faction, seller);
        let decoded = instruction::BarterAccept::try_from_slice(&accept.data[8..]).unwrap();
        assert_eq!(decoded.offer_id, 4);
        assert_eq!(accept.accounts[3].pubkey, seller);
    }

    fn risk_input() -> DonkeyRiskInput {
        DonkeyRiskInput {
            epoch: constants::EPOCH_DONKEY_RISK,
            entropy_mode: constants::ENTROPY_SWITCHBOARD,
            phase: state::Phase::Action,
            round: 1,
            alive: true,
            acted: false,
            cash: DONKEY_RISK_FEE,
            goods: u16::MAX - DONKEY_RISK_GOODS,
            oracle_config_ready: true,
            crank_ready: true,
            pending: false,
        }
    }

    #[test]
    fn epoch_two_donkey_risk_accepts_only_the_safe_exact_boundary() {
        assert_eq!(
            donkey_risk_eligibility(risk_input()),
            DonkeyRiskEligibility::Eligible
        );
    }

    #[test]
    fn epoch_two_donkey_risk_rejects_unfunded_or_unusable_attempts() {
        let mut input = risk_input();
        input.cash = DONKEY_RISK_FEE - 1;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::InsufficientCash)
        );

        let mut input = risk_input();
        input.goods = u16::MAX - DONKEY_RISK_GOODS + 1;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::GoodsOverflow)
        );

        let mut input = risk_input();
        input.round = constants::ROUNDS;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::NoMarketRemaining)
        );

        let mut input = risk_input();
        input.phase = state::Phase::Market;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::WrongPhase)
        );
    }

    #[test]
    fn epoch_two_donkey_risk_fails_closed_for_legacy_entropy_or_pending() {
        let mut input = risk_input();
        input.epoch = constants::EPOCH_CLASSIC;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::LegacyEpoch)
        );

        let mut input = risk_input();
        input.entropy_mode = constants::ENTROPY_SLOTHASH;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::EntropyUnavailable)
        );

        let mut input = risk_input();
        input.oracle_config_ready = false;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::OracleConfigUnavailable)
        );

        let mut input = risk_input();
        input.crank_ready = false;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::CrankUnavailable)
        );

        let mut input = risk_input();
        input.pending = true;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::Pending)
        );

        let mut input = risk_input();
        input.alive = false;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::InactiveFaction)
        );

        let mut input = risk_input();
        input.acted = true;
        assert_eq!(
            donkey_risk_eligibility(input),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::ActionAlreadyUsed)
        );
    }

    #[test]
    fn epoch_two_action_to_law_is_operator_owned_not_runner_advanced() {
        let mut game = state::Game::default();
        game.epoch = constants::EPOCH_DONKEY_RISK;
        game.entropy_mode = constants::ENTROPY_SWITCHBOARD;
        game.phase = state::Phase::Action;
        game.round = 1;
        game.phase_ends_at = 100;
        assert_eq!(
            advance_instruction(&game, 129, Pubkey::new_unique(), Pubkey::new_unique(), &[]),
            Err(DonkeyRiskReason::CrankUnavailable)
        );
        let abort =
            advance_instruction(&game, 130, Pubkey::new_unique(), Pubkey::new_unique(), &[])
                .unwrap();
        assert_eq!(abort.accounts.len(), 4);
        assert!(abort.accounts[3].is_writable);
    }

    #[test]
    fn epoch_two_builders_keep_oracle_accounts_in_canonical_order() {
        let admin = Pubkey::new_unique();
        let game = Pubkey::new_unique();
        let faction = Pubkey::new_unique();
        let oracle = super::donkey_oracle_pda(&game);
        let setup = DonkeyOperatorSetup {
            queue: Pubkey::new_unique(),
            state: Pubkey::new_unique(),
            oracles: std::array::from_fn(|_| Pubkey::new_unique()),
        };
        let config = ix_configure_donkey_oracles(admin, game, oracle, &setup);
        instruction::ConfigureDonkeyOracles::try_from_slice(&config.data[8..]).unwrap();
        assert_eq!(config.accounts[2].pubkey, oracle);
        assert_eq!(config.accounts[4].pubkey, donkey_source_pda(&game, 1));
        assert_eq!(config.accounts[10].pubkey, setup.oracles[0]);
        assert_eq!(config.accounts.len(), 4 + constants::ROUNDS as usize * 2);

        let init = ix_init_donkey_source(admin, game, 1, 42, &setup);
        let decoded = instruction::InitDonkeySource::try_from_slice(&init.data[8..]).unwrap();
        assert_eq!((decoded.round, decoded.recent_slot), (1, 42));
        let (source, escrow, lut_signer, queue) = donkey_init_accounts(game, 1, &setup);
        assert_eq!(init.accounts[2].pubkey, source);
        assert_eq!(init.accounts[4].pubkey, escrow);
        assert_eq!(init.accounts[5].pubkey, queue);
        assert_eq!(init.accounts[11].pubkey, lut_signer);
        assert!(init.accounts[0].is_signer && init.accounts[0].is_writable);
        assert!(
            init.accounts[2].is_writable
                && init.accounts[4].is_writable
                && init.accounts[5].is_writable
        );

        let join = ix_join_epoch2("A", admin, game, faction, oracle);
        assert_eq!(join.accounts.last().unwrap().pubkey, oracle);
        assert!(!join.accounts.last().unwrap().is_writable);

        let attempt = ix_attempt_donkey(admin, game, faction, oracle, source);
        instruction::AttemptDonkey::try_from_slice(&attempt.data[8..]).unwrap();
        assert_eq!(
            attempt
                .accounts
                .iter()
                .map(|meta| meta.pubkey)
                .collect::<Vec<_>>(),
            vec![admin, game, faction, oracle, source]
        );
        assert!(attempt.accounts[2].is_writable && attempt.accounts[3].is_writable);
        assert!(!attempt.accounts[4].is_writable);
    }

    #[test]
    fn donkey_pending_is_private_bound_and_blocks_retry() {
        let game = Pubkey::new_unique();
        let faction = Pubkey::new_unique();
        let path =
            std::env::temp_dir().join(format!("alashi-donkey-pending-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let pending = super::DonkeyPending {
            game,
            faction,
            round: 2,
            signature: "knownsig".into(),
            slot: None,
        };
        super::save_donkey_pending(&path, &pending).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o077,
            0
        );
        assert_eq!(
            super::load_donkey_pending(&path, game, faction).unwrap(),
            Some(pending)
        );
        assert!(
            super::load_donkey_pending(&path, Pubkey::new_unique(), faction)
                .unwrap()
                .is_none()
        );
        let input = risk_input();
        assert_eq!(
            super::donkey_risk_eligibility(super::DonkeyRiskInput {
                pending: true,
                ..input
            }),
            DonkeyRiskEligibility::Ineligible(DonkeyRiskReason::Pending)
        );
        super::clear_donkey_pending(&path).unwrap();
    }

    fn b64_encode(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let a = chunk[0];
            let b = *chunk.get(1).unwrap_or(&0);
            let c = *chunk.get(2).unwrap_or(&0);
            out.push(TABLE[(a >> 2) as usize] as char);
            out.push(TABLE[((a & 3) << 4 | b >> 4) as usize] as char);
            out.push(if chunk.len() > 1 {
                TABLE[((b & 15) << 2 | c >> 6) as usize] as char
            } else {
                '='
            });
            out.push(if chunk.len() > 2 {
                TABLE[(c & 63) as usize] as char
            } else {
                '='
            });
        }
        out
    }

    #[test]
    fn failed_or_malformed_transaction_logs_never_prove_donkey_resolution() {
        let game = Pubkey::new_unique();
        let faction = Pubkey::new_unique();
        let event = DonkeyResolved {
            game,
            faction,
            round: 2,
            success: false,
            goods: 0,
            fee: DONKEY_RISK_FEE,
            reason: 1,
        };
        let program = id().to_string();
        let logs = vec![
            format!("Program {program} invoke [1]"),
            format!("Program data: {}", b64_encode(&event.data())),
            format!("Program {program} failed: custom program error"),
        ];
        let failed = json!({"slot": 9, "meta": {"err": {"InstructionError": [0, "Custom"]}, "logMessages": logs}});
        assert!(confirmed_transaction_receipt("failed-sig", &failed).is_none());

        let malformed = json!({"slot": "9", "meta": {"err": null, "logMessages": [1]}});
        assert!(confirmed_transaction_receipt("bad-sig", &malformed).is_none());
        assert!(confirmed_transaction_receipt("null-sig", &serde_json::Value::Null).is_none());

        let valid = json!({"slot": 9, "meta": {"err": null, "logMessages": [
            format!("Program {program} invoke [1]"),
            format!("Program data: {}", b64_encode(&event.data())),
            format!("Program {program} success"),
        ]}});
        let receipt = confirmed_transaction_receipt("valid-sig", &valid).unwrap();
        assert_eq!(
            super::donkey_risk::verified_resolution(&receipt, game, faction, 2, DONKEY_RISK_FEE,),
            Some(super::donkey_risk::Resolution::Abort)
        );
    }

    #[test]
    fn legacy_donkey_builder_remains_the_existing_buy_instruction() {
        let signer = Pubkey::new_unique();
        let game = Pubkey::new_unique();
        let faction = Pubkey::new_unique();
        let donkey = ix_donkey(signer, game, faction);
        instruction::BuyDonkey::try_from_slice(&donkey.data[8..]).unwrap();
    }
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
        println!("bots [--phase-duration SECONDS] [--timeout SECONDS] [--no-llm] [--epoch90s-barter --agent-file REGISTERED_PROFILE --key PRIMARY_KEY --opponent-key DEMO_OPPONENT_KEY]\nbots --game PUBKEY --name NAME --key LOCAL_FILE [--agent-file REGISTERED_PROFILE] [--resume]\nbots agent inspect|join|act --help");
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
    let barter_demo = args.iter().any(|arg| arg == "--epoch90s-barter");
    let donkey_risk = args.iter().any(|arg| arg == "--epoch2-donkey-risk");
    let external_vrf_crank = args.iter().any(|arg| arg == "--external-vrf-crank");
    if barter_demo && donkey_risk {
        eprintln!("[ERROR] choose either --epoch90s-barter or --epoch2-donkey-risk");
        return;
    }
    if let Some(game_str) = flag_value(&args, "--game") {
        if barter_demo || donkey_risk || external_vrf_crank {
            eprintln!(
                "[ERROR] epoch demo flags are host-only; join and resume never change a Game epoch"
            );
            return;
        }
        let key_path =
            flag_value(&args, "--key").unwrap_or_else(|| format!("{KEYS_DIR}/join.json"));
        let name = flag_value(&args, "--name").unwrap_or_else(|| "Guest".to_string());
        run_join_mode(
            &rpc,
            &game_str,
            &key_path,
            &name,
            flag_value(&args, "--agent-file").as_deref(),
            args.iter().any(|arg| arg == "--resume"),
        );
        return;
    }

    // Self-host mode is explicit: the primary is already registered; the second signer is a local deterministic demo opponent.
    let agent_file = flag_value(&args, "--agent-file");
    let primary_key = flag_value(&args, "--key");
    let opponent_key = flag_value(&args, "--opponent-key");
    let self_host = agent_file.is_some() || primary_key.is_some() || opponent_key.is_some();
    if donkey_risk
        && (!self_host || !args.iter().any(|arg| arg == "--no-llm") || !external_vrf_crank)
    {
        eprintln!("[ERROR] --epoch2-donkey-risk requires explicit self-host keys, --no-llm, and --external-vrf-crank");
        return;
    }
    let donkey_setup = if donkey_risk {
        match parse_donkey_operator_setup(&args).and_then(|setup| {
            validate_donkey_operator_setup(&rpc, &setup)?;
            Ok(setup)
        }) {
            Ok(setup) => Some(setup),
            Err(error) => {
                eprintln!(
                    "[ERROR] epoch-2 operator preflight failed before game creation: {error}"
                );
                return;
            }
        }
    } else {
        None
    };
    if !barter_demo_allowed(
        barter_demo,
        false,
        self_host,
        args.iter().any(|arg| arg == "--no-llm"),
    ) {
        eprintln!("[ERROR] --epoch90s-barter requires explicit self-host keys and --no-llm");
        return;
    }
    let runner_agent: Option<SelfHost> = if self_host {
        let (agent_file, key_path, opponent_path) = match (agent_file, primary_key, opponent_key) {
            (Some(agent_file), Some(key_path), Some(opponent_path)) => {
                (agent_file, key_path, opponent_path)
            }
            _ => {
                eprintln!("[ERROR] self-host requires --agent-file, --key, and --opponent-key");
                return;
            }
        };
        if agent_cli::devnet(&rpc).is_err() {
            eprintln!("[ERROR] self-host requires Solana devnet");
            return;
        }
        if std::env::var("ALASHI_CHAIN_WISH_API").ok().as_deref() != Some("https://alashi.network")
        {
            eprintln!("[ERROR] self-host requires ALASHI_CHAIN_WISH_API=https://alashi.network before it creates a game");
            return;
        }
        let primary = match agent_cli::read_key(&key_path) {
            Ok(key) => key,
            Err(_) => {
                eprintln!("[ERROR] primary key must be an existing mode-600 keypair");
                return;
            }
        };
        let opponent = match agent_cli::read_key(&opponent_path) {
            Ok(key) => key,
            Err(_) => {
                eprintln!("[ERROR] opponent key must be an existing mode-600 keypair");
                return;
            }
        };
        let profile_path = PathBuf::from(agent_file);
        let owner = match runner_auth::authenticate_owner(&profile_path, &key_path) {
            Ok(owner) => owner,
            Err(error) => {
                eprintln!("[ERROR] registered-wallet API preflight failed before any chain action: {error}");
                return;
            }
        };
        if primary.pubkey() == opponent.pubkey() {
            eprintln!("[ERROR] primary and deterministic opponent must use different keys");
            return;
        }
        // Store these only long enough to construct bots below.
        std::mem::drop(opponent);
        std::mem::drop(primary);
        Some(SelfHost {
            owner,
            agent_file: profile_path,
            primary_key_path: key_path,
            opponent_key_path: opponent_path,
        })
    } else {
        None
    };
    let (bot1_kp, bot2_kp) = if let Some(host) = &runner_agent {
        (
            agent_cli::read_key(&host.primary_key_path).expect("checked primary key"),
            agent_cli::read_key(&host.opponent_key_path).expect("checked opponent key"),
        )
    } else {
        (
            load_or_create(&format!("{KEYS_DIR}/bot1.json")),
            load_or_create(&format!("{KEYS_DIR}/bot2.json")),
        )
    };
    let wish_profiles = if self_host {
        None
    } else {
        match chain_wish_client::Profiles::from_env() {
            Ok(profiles) => profiles,
            Err(error) => {
                eprintln!("[ERROR] chain wish opt-in unavailable: {error}");
                return;
            }
        }
    };
    let llm_cfg = if self_host || args.iter().any(|a| a == "--no-llm") {
        None
    } else {
        llm::llm_config()
    };
    // The self-host path is chain-only. It must not create a legacy HTTP registry record.
    if !self_host {
        register_agent(
            &bot1_kp.pubkey().to_string(),
            "greedy-v1",
            "greedy-heuristic-v1",
        );
        register_agent(
            &bot2_kp.pubkey().to_string(),
            llm_cfg
                .as_ref()
                .map(|c| c.model.clone())
                .unwrap_or_default()
                .as_str(),
            llm::SYSTEM,
        );
    }
    println!("rpc: {}", rpc_url());
    println!("bot1: {}", bot1_kp.pubkey());
    println!("bot2: {}", bot2_kp.pubkey());
    if wish_profiles.is_none() && !self_host {
        ensure_funds(&rpc, "bot1", &bot1_kp);
        ensure_funds(&rpc, "bot2", &bot2_kp);
    }
    if self_host {
        println!("self-host: registered primary plus deterministic no-LLM demo opponent; existing pre-funded devnet keys required");
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
            donkey_pending: None,
            donkey_store: runner_agent
                .as_ref()
                .map(|host| donkey_pending_path(&host.agent_file)),
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
            donkey_pending: None,
            donkey_store: runner_agent
                .as_ref()
                .map(|host| donkey_pending_path(&host.agent_file)),
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

    let epoch = if donkey_risk {
        constants::EPOCH_DONKEY_RISK
    } else {
        epoch_for_barter_demo(barter_demo)
    };
    let entropy_mode = if donkey_risk {
        constants::ENTROPY_SWITCHBOARD
    } else {
        constants::ENTROPY_SLOTHASH
    };
    if !send_ix(
        &rpc,
        &bots[0].kp,
        ix_initialize(
            bots[0].kp.pubkey(),
            game,
            game_id,
            phase_duration,
            entropy_mode,
            epoch,
        ),
    ) {
        println!("[ERROR] initialize не отправился");
        return;
    }
    if !wait_account(&rpc, &game, "game") {
        return;
    }
    let donkey_oracle = donkey_setup.as_ref().map(|setup| {
        for round in 1..=constants::ROUNDS {
            let recent_slot = match rpc.get_slot() {
                Ok(slot) if slot > 0 => slot,
                _ => {
                    eprintln!("[ERROR] epoch-2 source setup has no fresh slot; stopping before Join");
                    return None;
                }
            };
            let source = donkey_source_pda(&game, round);
            if !send_ix(
                &rpc,
                &bots[0].kp,
                ix_init_donkey_source(bots[0].kp.pubkey(), game, round, recent_slot, setup),
            ) || !wait_account(&rpc, &source, "donkey source") {
                eprintln!("[ERROR] epoch-2 source initialization failed before Join; stopping without entry fees");
                return None;
            }
        }
        let oracle = donkey_oracle_pda(&game);
        if !send_ix(
            &rpc,
            &bots[0].kp,
            ix_configure_donkey_oracles(bots[0].kp.pubkey(), game, oracle, setup),
        ) || !wait_account(&rpc, &oracle, "donkey oracle config") {
            eprintln!("[ERROR] epoch-2 oracle configuration failed before Join; stopping without entry fees");
            return None;
        }
        Some(oracle)
    });
    let Some(donkey_oracle) = donkey_oracle
        .flatten()
        .or_else(|| (!donkey_risk).then_some(Pubkey::default()))
    else {
        return;
    };
    let join_primary = if donkey_risk {
        ix_join_epoch2(
            "Aibot",
            bots[0].kp.pubkey(),
            game,
            bots[0].faction,
            donkey_oracle,
        )
    } else {
        ix_join("Aibot", bots[0].kp.pubkey(), game, bots[0].faction)
    };
    if !send_ix(&rpc, &bots[0].kp, join_primary) {
        return;
    }
    if !wait_account(&rpc, &bots[0].faction, "faction1") {
        return;
    }
    let join_opponent = if donkey_risk {
        ix_join_epoch2(
            "Botagul",
            bots[1].kp.pubkey(),
            game,
            bots[1].faction,
            donkey_oracle,
        )
    } else {
        ix_join("Botagul", bots[1].kp.pubkey(), game, bots[1].faction)
    };
    if !send_ix(&rpc, &bots[1].kp, join_opponent) {
        return;
    }
    if !wait_account(&rpc, &bots[1].faction, "faction2") {
        return;
    }
    if donkey_risk {
        let Some(path) = bots[0].donkey_store.as_ref() else {
            eprintln!("[ERROR] epoch-2 runner has no protected pending-state path");
            return;
        };
        match load_donkey_pending(path, game, bots[0].faction) {
            Ok(pending) => bots[0].donkey_pending = pending,
            Err(error) => {
                eprintln!("[ERROR] epoch-2 pending-state validation failed: {error}");
                return;
            }
        }
    }
    if let Some(host) = runner_agent.as_ref() {
        let binding = match runner_auth::bind_wallet(
            &host.owner,
            &game.to_string(),
            &bots[0].faction.to_string(),
        ) {
            Ok(binding) => binding,
            Err(error) => {
                eprintln!("[ERROR] public owner proof/bind unavailable before gameplay: {error}");
                return;
            }
        };
        bots[0].wish = match chain_wish_client::ChainWishClient::bind_public(
            "https://alashi.network",
            &host.agent_file,
            binding.clone(),
        ) {
            Ok(client) => Some(client),
            Err(error) => {
                eprintln!("[ERROR] public wish capability unavailable before gameplay: {error}");
                return;
            }
        };
        if let Ok(grant) = std::env::var("ALASHI_PAIRING_GRANT") {
            match runner_auth::complete_pairing(&host.owner, &binding, &grant) {
                Ok(url) => println!("owner link: {url}"),
                Err(error) => {
                    eprintln!("[ERROR] browser pairing unavailable: {error}");
                    return;
                }
            }
        }
    } else if let Err(error) = bind_wishes(wish_profiles.as_ref(), &mut bots, &game) {
        eprintln!("[ERROR] chain wish binding unavailable before gameplay: {error}");
        return;
    }

    let started = now();
    let mut last_stamp: Option<u16> = None;
    let mut hb = 0u32;
    let mut barter_demo_attempted = false;
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
        if donkey_risk && hb % 5 == 1 {
            reconcile_donkey_pending(&rpc, &mut bots[0]);
        }
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
                    match advance_instruction(&g, tnow, bots[0].kp.pubkey(), game, &faction_keys) {
                        Ok(ix) => { send_ix(&rpc, &bots[0].kp, ix); }
                        Err(DonkeyRiskReason::CrankUnavailable) if g.phase == state::Phase::Action => {
                            if hb % 5 == 1 { eprintln!("[action r{}] waiting for Alashi CPI commit + Advance crank", g.round); }
                        }
                        Err(reason) => eprintln!("[advance] epoch-2 configuration unavailable: {:?}; external crank must repair it", reason),
                    }
                }
            }
            state::Phase::Market => {
                if barter_demo && g.round == 2 && !barter_demo_attempted {
                    barter_demo_attempted = true;
                    run_barter_demo_market(&rpc, game, &g, &mut bots);
                }
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
                    match advance_instruction(&g, tnow, bots[0].kp.pubkey(), game, &faction_keys) {
                        Ok(ix) => { send_ix(&rpc, &bots[0].kp, ix); }
                        Err(DonkeyRiskReason::CrankUnavailable) if g.phase == state::Phase::Action => {
                            if hb % 5 == 1 { eprintln!("[action r{}] waiting for Alashi CPI commit + Advance crank", g.round); }
                        }
                        Err(reason) => eprintln!("[advance] epoch-2 configuration unavailable: {:?}; external crank must repair it", reason),
                    }
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
                    if i == 0 && g.epoch == constants::EPOCH_DONKEY_RISK {
                        match run_epoch2_donkey(&rpc, b, game, &g, external_vrf_crank) {
                            DonkeyRun::Attempted => {
                                println!("[action r{}] confirmed epoch-2 donkey attempt; awaiting verified Law resolution", g.round);
                                b.acted = true;
                                continue;
                            }
                            DonkeyRun::Block => {
                                eprintln!("[action r{}] epoch-2 donkey receipt pending or ambiguous; no fallback action", g.round);
                                b.acted = true;
                                continue;
                            }
                            DonkeyRun::Pass(reason) => {
                                println!("[action r{}] epoch-2 donkey unavailable: {:?}; producing instead", g.round, reason);
                                if send_ix(&rpc, &b.kp, ix_produce(b.kp.pubkey(), game, b.faction))
                                {
                                    b.acted = true;
                                    b.goods += 2;
                                }
                                continue;
                            }
                        }
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
                            Some("donkey") if g.epoch == constants::EPOCH_DONKEY_RISK => {
                                let pending = b
                                    .wish
                                    .as_ref()
                                    .map(|wish| wish.has_pending())
                                    .unwrap_or(false);
                                let eligibility = f
                                    .as_ref()
                                    .map(|faction| unavailable_epoch2_donkey(&g, faction, pending))
                                    .unwrap_or(DonkeyRiskEligibility::Ineligible(
                                        DonkeyRiskReason::InactiveFaction,
                                    ));
                                println!(
                                    "[action r{}] epoch-2 donkey risk disabled: {:?}; producing instead",
                                    g.round, eligibility
                                );
                            }
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
                    } else if i == 1 && g.epoch == constants::EPOCH_DONKEY_RISK {
                        let pending = b
                            .wish
                            .as_ref()
                            .map(|wish| wish.has_pending())
                            .unwrap_or(false);
                        let eligibility = fetch_faction(&rpc, &b.faction)
                            .map(|faction| unavailable_epoch2_donkey(&g, &faction, pending))
                            .unwrap_or(DonkeyRiskEligibility::Ineligible(
                                DonkeyRiskReason::InactiveFaction,
                            ));
                        println!(
                            "[action r{}] epoch-2 donkey risk disabled: {:?}; producing instead",
                            g.round, eligibility
                        );
                        if send_ix(&rpc, &b.kp, ix_produce(b.kp.pubkey(), game, b.faction)) {
                            b.acted = true;
                            b.goods += 2;
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
                    match advance_instruction(&g, tnow, bots[0].kp.pubkey(), game, &faction_keys) {
                        Ok(ix) => { send_ix(&rpc, &bots[0].kp, ix); }
                        Err(DonkeyRiskReason::CrankUnavailable) if g.phase == state::Phase::Action => {
                            if hb % 5 == 1 { eprintln!("[action r{}] waiting for Alashi CPI commit + Advance crank", g.round); }
                        }
                        Err(reason) => eprintln!("[advance] epoch-2 configuration unavailable: {:?}; external crank must repair it", reason),
                    }
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
                    match advance_instruction(&g, tnow, bots[0].kp.pubkey(), game, &faction_keys) {
                        Ok(ix) => { send_ix(&rpc, &bots[0].kp, ix); }
                        Err(DonkeyRiskReason::CrankUnavailable) if g.phase == state::Phase::Action => {
                            if hb % 5 == 1 { eprintln!("[action r{}] waiting for Alashi CPI commit + Advance crank", g.round); }
                        }
                        Err(reason) => eprintln!("[advance] epoch-2 configuration unavailable: {:?}; external crank must repair it", reason),
                    }
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
