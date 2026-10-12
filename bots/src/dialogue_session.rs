//! Explicit devnet lifecycle operations, with no participant decisions or providers.
use super::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
const USAGE: &str = "bots dialogue-session keygen --out PRIVATE_NEW_KEY\nbots dialogue-session create --key KEY_A --opponent-key KEY_B --game-id ID --phase-duration 60 --receipt-dir PRIVATE_DIR --execute\nbots dialogue-session airdrop --key KEY --lamports 100000000 --receipt-dir PRIVATE_DIR --execute\nbots dialogue-session join --game PDA --key KEY --name NAME --receipt-dir PRIVATE_DIR --execute\nbots dialogue-session inspect --game PDA\nbots dialogue-session advance|settle --game PDA --key OPERATOR_KEY --receipt-dir PRIVATE_DIR --execute\nCreate fixes epoch=1, slot-hash entropy and entry=0.05 testSOL; it initializes and joins exactly two wallets. Every transaction uses an exclusive signature guard, retained even after confirmation. Never remove a guard to retry; reconcile its receipt instead.";
fn options(args: &[String]) -> Result<BTreeMap<String, String>, &'static str> {
    let mut result = BTreeMap::new();
    let mut i = 0;
    while i < args.len() {
        let key = &args[i];
        if key == "--execute" {
            if result.insert(key.clone(), String::new()).is_some() {
                return Err("duplicate_option");
            }
            i += 1;
            continue;
        }
        if !key.starts_with("--")
            || i + 1 == args.len()
            || args[i + 1].starts_with("--")
            || result.insert(key.clone(), args[i + 1].clone()).is_some()
        {
            return Err("invalid_options");
        }
        i += 2;
    }
    Ok(result)
}
fn required<'a>(o: &'a BTreeMap<String, String>, k: &str) -> Result<&'a str, &'static str> {
    o.get(k)
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("missing_option")
}
fn number(o: &BTreeMap<String, String>, k: &str, min: u64, max: u64) -> Result<u64, &'static str> {
    let n = required(o, k)?
        .parse::<u64>()
        .map_err(|_| "invalid_number")?;
    if !(min..=max).contains(&n) {
        return Err("out_of_bounds");
    }
    Ok(n)
}
fn allowed(o: &BTreeMap<String, String>, fields: &[&str]) -> Result<(), &'static str> {
    if o.keys().all(|k| fields.contains(&k.as_str())) {
        Ok(())
    } else {
        Err("unknown_option")
    }
}
fn private_directory(path: &Path) -> Result<(), &'static str> {
    let m = fs::symlink_metadata(path).map_err(|_| "receipt_directory_missing")?;
    if !m.is_dir() || m.permissions().mode() & 0o077 != 0 {
        return Err("receipt_directory_must_be_private");
    }
    Ok(())
}
fn load_game(rpc: &RpcClient, game: Pubkey) -> Result<state::Game, &'static str> {
    let a = rpc.get_account(&game).map_err(|_| "game_unavailable")?;
    if a.owner != id() {
        return Err("wrong_program");
    }
    let g = state::Game::try_deserialize(&mut a.data.as_slice()).map_err(|_| "invalid_game")?;
    if g.epoch != 1
        || g.entropy_mode != constants::ENTROPY_SLOTHASH
        || g.entry_fee != ENTRY_FEE
        || game_pda(g.game_id) != game
    {
        return Err("game_terms_mismatch");
    }
    Ok(g)
}
fn permitted_count(phase: state::Phase, count: u8, expired: bool) -> bool {
    count == 2
        || (count < 2
            && (phase == state::Phase::Aborted || (phase == state::Phase::Lobby && expired)))
}
fn valid_set(rows: &[(Pubkey, Pubkey)], count: u8) -> bool {
    rows.len() == count as usize
        && rows.iter().enumerate().all(|(i, row)| {
            rows[..i]
                .iter()
                .all(|prior| prior.0 != row.0 && prior.1 != row.1)
        })
}
fn sync_parent(path: &Path) -> Result<(), &'static str> {
    fs::File::open(path)
        .and_then(|f| f.sync_all())
        .map_err(|_| "directory_sync_failed")
}
fn participants(
    rpc: &RpcClient,
    game: Pubkey,
    g: &state::Game,
) -> Result<(Vec<Pubkey>, Vec<Pubkey>), &'static str> {
    let current = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "clock_invalid")?
        .as_secs() as i64;
    if !permitted_count(
        g.phase,
        g.faction_count,
        current >= g.phase_ends_at.saturating_add(5),
    ) {
        return Err("exactly_two_factions_required");
    }
    let mut rows = Vec::new();
    for p in discover_factions(rpc, &game) {
        let a = rpc.get_account(&p).map_err(|_| "faction_unavailable")?;
        if a.owner != id() {
            return Err("wrong_program");
        }
        let f = state::Faction::try_deserialize(&mut a.data.as_slice())
            .map_err(|_| "invalid_faction")?;
        if f.game != game || faction_pda(&game, &f.wallet) != p {
            return Err("invalid_faction");
        }
        rows.push((p, f.wallet));
    }
    rows.sort_by_key(|r| r.0);
    if !valid_set(&rows, g.faction_count) {
        return Err("invalid_faction_set");
    }
    Ok((
        rows.iter().map(|r| r.0).collect(),
        rows.iter().map(|r| r.1).collect(),
    ))
}
fn submit(
    rpc: &RpcClient,
    key: &Keypair,
    ix: Instruction,
    dir: &Path,
    operation: &str,
    game: Pubkey,
) -> Result<Value, &'static str> {
    private_directory(dir)?;
    let path = dir.join(format!("{game}-{operation}.json"));
    if path.exists() {
        return Err("existing_operation_requires_reconciliation");
    }
    let tx = prepare_ix(rpc, key, ix).map_err(|_| "transaction_not_sent")?;
    let signature = tx.signatures[0].to_string();
    let mut guard = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
        .map_err(|_| "operation_guard_unavailable")?;
    write!(guard,"{}",json!({"status":"prepared","game":game.to_string(),"operation":operation,"signature":signature})).and_then(|_|guard.sync_all()).map_err(|_|"guard_storage_failed")?;
    fs::File::open(dir)
        .and_then(|f| f.sync_all())
        .map_err(|_| "guard_storage_failed")?;
    let receipt = match submit_confirmed(rpc, &tx) {
        Ok(r) => r,
        Err(r) => {
            println!("{}", json!({"ok":false,"receipt":r,"guard":path}));
            return Err("transaction_requires_reconciliation");
        }
    };
    // Keep the pre-broadcast guard; a separate exclusive confirmed receipt cannot erase it.
    let confirmed = dir.join(format!("{game}-{operation}.confirmed.json"));
    let mut out = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(confirmed)
        .map_err(|_| "receipt_storage_failed")?;
    write!(out, "{receipt}")
        .and_then(|_| out.sync_all())
        .map_err(|_| "receipt_storage_failed")?;
    sync_parent(dir)?;
    println!(
        "{}",
        json!({"ok":true,"game":game.to_string(),"operation":operation,"receipt":receipt})
    );
    Ok(receipt)
}
fn rpc() -> Result<RpcClient, &'static str> {
    let r = RpcClient::new_with_timeout_and_commitment(
        rpc_url(),
        Duration::from_secs(8),
        CommitmentConfig::confirmed(),
    );
    agent_cli::devnet(&r).map_err(|_| "devnet_required")?;
    Ok(r)
}
fn airdrop_once(url: &str, wallet: Pubkey, amount: u64) -> Result<String, &'static str> {
    // The Solana SDK retries HTTP429 internally. Faucet requests must be a
    // single bounded HTTP attempt, with an unknown result left for reconciliation.
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("curl")
        .args([
            "--disable",
            "--globoff",
            "--silent",
            "--fail",
            "--connect-timeout",
            "5",
            "--max-time",
            "12",
            "--max-filesize",
            "65536",
            "--proto",
            "=https",
            "--request",
            "POST",
            "--header",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
            "--url",
            url,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "airdrop_transport_unavailable")?;
    let body = json!({"jsonrpc":"2.0","id":1,"method":"requestAirdrop","params":[wallet.to_string(),amount,{"commitment":"confirmed"}]}).to_string();
    if child
        .stdin
        .take()
        .unwrap()
        .write_all(body.as_bytes())
        .is_err()
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err("airdrop_unknown_or_rejected_no_retry");
    }
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .unwrap()
        .take(65537)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() > 65536 {
        let _ = child.kill();
        let _ = child.wait();
        return Err("airdrop_unknown_or_rejected_no_retry");
    }
    if !child
        .wait()
        .map_err(|_| "airdrop_unknown_or_rejected_no_retry")?
        .success()
    {
        return Err("airdrop_unknown_or_rejected_no_retry");
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "airdrop_invalid_response")?;
    let signature = value["result"]
        .as_str()
        .filter(|_| value["error"].is_null())
        .ok_or("airdrop_unknown_or_rejected_no_retry")?;
    signature
        .parse::<solana_signature::Signature>()
        .map_err(|_| "airdrop_invalid_signature")?;
    Ok(signature.to_string())
}
fn execute(command: &str, o: BTreeMap<String, String>) -> Result<(), &'static str> {
    if command == "keygen" {
        allowed(&o, &["--out"])?;
        let path = Path::new(required(&o, "--out")?);
        let parent = path.parent().ok_or("private_parent_required")?;
        private_directory(parent)?;
        let key = Keypair::new();
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| "key_path_must_be_new")?;
        let bytes = key.to_bytes().to_vec();
        write!(
            f,
            "{}",
            serde_json::to_string(&bytes).map_err(|_| "key_serialization_failed")?
        )
        .and_then(|_| f.sync_all())
        .map_err(|_| "key_storage_failed")?;
        sync_parent(parent)?;
        println!("{}", json!({"ok":true,"wallet":key.pubkey().to_string()}));
        return Ok(());
    }
    if command == "airdrop" {
        allowed(&o, &["--key", "--lamports", "--receipt-dir", "--execute"])?;
        if !o.contains_key("--execute") {
            return Err("execute_required");
        }
        let amount = number(&o, "--lamports", 1, 1_000_000_000)?;
        let key = agent_cli::read_key(required(&o, "--key")?).map_err(|_| "key_invalid")?;
        let dir = Path::new(required(&o, "--receipt-dir")?);
        private_directory(dir)?;
        let rpc = rpc()?;
        let path = dir.join(format!("{}-airdrop-request.json", key.pubkey()));
        let mut guard = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(|_| "existing_airdrop_requires_reconciliation")?;
        write!(
            guard,
            "{}",
            json!({"wallet":key.pubkey().to_string(),"lamports":amount,"status":"requesting"})
        )
        .and_then(|_| guard.sync_all())
        .map_err(|_| "guard_storage_failed")?;
        sync_parent(dir)?;
        let signature = airdrop_once(&rpc.url(), key.pubkey(), amount)?;
        println!(
            "{}",
            json!({"ok":true,"wallet":key.pubkey().to_string(),"signature":signature.to_string(),"status":"requested_not_confirmed"})
        );
        let mut receipt = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(dir.join(format!("{}-airdrop-signature.json", key.pubkey())))
            .map_err(|_| "receipt_storage_failed")?;
        write!(
            receipt,
            "{}",
            json!({"signature":signature.to_string(),"wallet":key.pubkey().to_string()})
        )
        .and_then(|_| receipt.sync_all())
        .map_err(|_| "receipt_storage_failed")?;
        sync_parent(dir)?;
        return Ok(());
    }
    if command == "join" {
        allowed(
            &o,
            &["--game", "--key", "--name", "--receipt-dir", "--execute"],
        )?;
        if !o.contains_key("--execute") {
            return Err("execute_required");
        }
        let game = required(&o, "--game")?
            .parse::<Pubkey>()
            .map_err(|_| "invalid_game")?;
        let name = required(&o, "--name")?;
        if name.len() > 16 || name.chars().any(char::is_control) {
            return Err("invalid_name");
        }
        let key = agent_cli::read_key(required(&o, "--key")?).map_err(|_| "key_invalid")?;
        let rpc = rpc()?;
        let g = load_game(&rpc, game)?;
        if g.phase != state::Phase::Lobby || g.faction_count >= 2 {
            return Err("lobby_not_joinable");
        }
        let faction = faction_pda(&game, &key.pubkey());
        if rpc
            .get_account_with_commitment(&faction, CommitmentConfig::confirmed())
            .map_err(|_| "faction_preflight_unavailable")?
            .value
            .is_some()
        {
            return Err("already_joined");
        }
        submit(
            &rpc,
            &key,
            ix_join(name, key.pubkey(), game, faction),
            Path::new(required(&o, "--receipt-dir")?),
            &format!("join-wallet-{}", key.pubkey()),
            game,
        )?;
        return Ok(());
    }
    if command == "create" {
        allowed(
            &o,
            &[
                "--key",
                "--opponent-key",
                "--game-id",
                "--phase-duration",
                "--receipt-dir",
                "--execute",
            ],
        )?;
        if !o.contains_key("--execute") {
            return Err("execute_required");
        }
        let game_id = number(&o, "--game-id", 1, u32::MAX as u64)?;
        let duration = number(&o, "--phase-duration", 30, 120)? as i64;
        let dir = Path::new(required(&o, "--receipt-dir")?);
        private_directory(dir)?;
        let a = agent_cli::read_key(required(&o, "--key")?).map_err(|_| "key_invalid")?;
        let b = agent_cli::read_key(required(&o, "--opponent-key")?).map_err(|_| "key_invalid")?;
        if a.pubkey() == b.pubkey() {
            return Err("distinct_wallets_required");
        }
        let rpc = rpc()?;
        let game = game_pda(game_id);
        // Account absence must be proven, not inferred from a transport failure.
        let present = rpc
            .get_account_with_commitment(&game, CommitmentConfig::confirmed())
            .map_err(|_| "game_preflight_unavailable")?;
        if present.value.is_some() {
            return Err("game_must_be_new");
        }
        submit(
            &rpc,
            &a,
            ix_initialize(
                a.pubkey(),
                game,
                game_id,
                duration,
                constants::ENTROPY_SLOTHASH,
                1,
            ),
            dir,
            "initialize",
            game,
        )?;
        for (i, key) in [&a, &b].iter().enumerate() {
            let g = load_game(&rpc, game)?;
            if g.phase != state::Phase::Lobby || g.faction_count != i as u8 {
                return Err("lobby_changed_stop");
            }
            submit(
                &rpc,
                key,
                ix_join(
                    if i == 0 { "Dialogue A" } else { "Dialogue B" },
                    key.pubkey(),
                    game,
                    faction_pda(&game, &key.pubkey()),
                ),
                dir,
                &format!("join-{i}"),
                game,
            )?;
        }
        let g = load_game(&rpc, game)?;
        participants(&rpc, game, &g)?;
        println!(
            "{}",
            json!({"ok":true,"game":game.to_string(),"watch_url":format!("https://alashi.network/devnet?game={game}")})
        );
        return Ok(());
    }
    if !["inspect", "advance", "settle"].contains(&command) {
        return Err("unknown_command");
    }
    allowed(
        &o,
        if command == "inspect" {
            &["--game"]
        } else {
            &["--game", "--key", "--receipt-dir", "--execute"]
        },
    )?;
    let game = required(&o, "--game")?
        .parse::<Pubkey>()
        .map_err(|_| "invalid_game")?;
    let rpc = rpc()?;
    let g = load_game(&rpc, game)?;
    if command == "inspect" {
        println!(
            "{}",
            json!({"ok":true,"game":game.to_string(),"round":g.round,"phase":format!("{:?}",g.phase),"phase_ends_at":g.phase_ends_at,"settled":g.settled,"faction_count":g.faction_count})
        );
        return Ok(());
    }
    if !o.contains_key("--execute") {
        return Err("execute_required");
    }
    if g.settled {
        return Err("already_settled");
    }
    let key = agent_cli::read_key(required(&o, "--key")?).map_err(|_| "key_invalid")?;
    let dir = Path::new(required(&o, "--receipt-dir")?);
    let (factions, wallets) = participants(&rpc, game, &g)?;
    if command == "advance" {
        if matches!(g.phase, state::Phase::Finished | state::Phase::Aborted) {
            return Err("terminal_use_settle");
        }
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "clock_invalid")?
            .as_secs() as i64;
        if t < g.phase_ends_at.saturating_add(5) {
            return Err("phase_not_due");
        }
        submit(
            &rpc,
            &key,
            ix_advance(key.pubkey(), game, &factions),
            dir,
            &format!("advance-{}-{:?}", g.round, g.phase),
            game,
        )?;
    } else {
        if !matches!(g.phase, state::Phase::Finished | state::Phase::Aborted) {
            return Err("game_not_terminal");
        }
        let ix = if g.phase == state::Phase::Aborted {
            let mut metas = accounts::SettleRefund {
                crank: key.pubkey(),
                game,
            }
            .to_account_metas(None);
            for f in &factions {
                metas.push(AccountMeta::new_readonly(*f, false));
            }
            for w in &wallets {
                metas.push(AccountMeta::new(*w, false));
            }
            Instruction::new_with_bytes(id(), &instruction::SettleRefund {}.data(), metas)
        } else {
            ix_settle(key.pubkey(), game, &factions, &wallets, g.admin)
        };
        submit(&rpc, &key, ix, dir, "settle", game)?;
    }
    Ok(())
}
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|s| s == "--help") {
        println!("{USAGE}");
        return 0;
    }
    let result = args
        .split_first()
        .ok_or("command_required")
        .and_then(|(command, rest)| options(rest).and_then(|o| execute(command, o)));
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("dialogue-session: {e}");
            2
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_reject_duplicates_and_unknown_budget() {
        assert!(options(&["--game".into(), "a".into(), "--game".into(), "b".into()]).is_err());
        let o = options(&["--phase-duration".into(), "121".into()]).unwrap();
        assert!(number(&o, "--phase-duration", 30, 120).is_err());
        assert!(allowed(&o, &["--game"]).is_err());
        assert!(options(&["--execute".into(), "--execute".into()]).is_err());
    }
    #[test]
    fn initialize_uses_explicit_epoch_one_terms() {
        let a = Pubkey::new_from_array([1; 32]);
        let game = game_pda(1);
        let ix = ix_initialize(a, game, 1, 60, constants::ENTROPY_SLOTHASH, 1);
        assert_eq!(ix.program_id, id());
        assert!(ix.accounts.iter().any(|m| m.pubkey == a && m.is_signer));
        let mut bytes = &ix.data[8..];
        let parsed =
            <instruction::Initialize as anchor_lang::AnchorDeserialize>::deserialize(&mut bytes)
                .unwrap();
        assert_eq!(parsed.epoch, 1);
        assert_eq!(parsed.entry_fee, 50_000_000);
        assert_eq!(parsed.phase_duration, 60);
        assert_eq!(parsed.entropy_mode, constants::ENTROPY_SLOTHASH);
    }
    #[test]
    fn partial_lobby_recovery_only_after_expiry_or_abort() {
        assert!(permitted_count(state::Phase::Lobby, 0, true));
        assert!(permitted_count(state::Phase::Lobby, 1, true));
        assert!(!permitted_count(state::Phase::Lobby, 1, false));
        assert!(permitted_count(state::Phase::Aborted, 1, false));
        assert!(!permitted_count(state::Phase::Market, 1, true));
        assert!(!permitted_count(state::Phase::Finished, 1, true));
        assert!(permitted_count(state::Phase::Market, 2, false));
        assert!(!permitted_count(state::Phase::Aborted, 3, true));
        let a = Pubkey::new_from_array([1; 32]);
        let b = Pubkey::new_from_array([2; 32]);
        assert!(valid_set(&[], 0));
        assert!(valid_set(&[(a, b)], 1));
        assert!(!valid_set(&[], 1));
        assert!(!valid_set(&[(a, b), (a, b)], 2));
    }
}
