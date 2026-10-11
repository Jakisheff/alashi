//! Managed devnet lobbies. The platform signs only for its own opponent.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::fs::OpenOptionsExt,
    sync::{Arc, Mutex},
};

const MAX_ACTIVE: usize = 4;
const MAX_DAILY: usize = 24;
const JOIN_MARGIN: i64 = 30;
const OPPONENT_MARGIN: i64 = 20;
const DURATION: i64 = 20;
const OPPONENT_NAME: &str = "Alashi Bot";

#[derive(Clone, Serialize, Deserialize)]
struct Match {
    id: u64,
    created_at: i64,
    wallets: Vec<String>,
    done: bool,
    #[serde(skip)]
    view: Option<View>,
    #[serde(skip)]
    error: Option<String>,
}

#[derive(Clone)]
struct View {
    phase: state::Phase,
    ends_at: i64,
    count: u8,
    observed_at: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Book {
    version: u8,
    operator: String,
    matches: Vec<Match>,
}

fn save(path: &Path, book: &Book) -> Result<(), String> {
    let pending = path.with_extension("pending");
    let bytes = serde_json::to_vec(book).map_err(|_| "state_encode")?;
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&pending)
        .map_err(|_| "state_write")?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "state_write")?;
    fs::rename(&pending, path).map_err(|_| "state_replace")?;
    File::open(path.parent().unwrap_or(Path::new(".")))
        .and_then(|f| f.sync_all())
        .map_err(|_| "state_sync")?;
    Ok(())
}

fn allocation(book: &mut Book, wallet: &str, at: i64, id: u64) -> Result<usize, &'static str> {
    if wallet == book.operator
        || wallet
            .parse::<Pubkey>()
            .ok()
            .is_none_or(|p| p.to_string() != wallet)
    {
        return Err("invalid_wallet");
    }
    // A retried request, including after a restart, keeps its original Game.
    if let Some(i) = book
        .matches
        .iter()
        .position(|m| !m.done && m.wallets.iter().any(|w| w == wallet))
    {
        return Ok(i);
    }
    if let Some(i) = book.matches.iter().position(|m| {
        !m.done
            && m.wallets.len() < constants::MAX_FACTIONS as usize
            && m.view.as_ref().is_some_and(|v| {
                v.phase == state::Phase::Lobby
                    && at - v.observed_at <= 15
                    && v.ends_at - at > JOIN_MARGIN
                    && v.count < constants::MAX_FACTIONS
            })
    }) {
        book.matches[i].wallets.push(wallet.into());
        return Ok(i);
    }
    // Coalesce requests received while the first Initialize is confirming.
    if let Some(i) = book.matches.iter().position(|m| {
        !m.done
            && m.view.is_none()
            && at - m.created_at < JOIN_MARGIN
            && m.wallets.len() < constants::MAX_FACTIONS as usize
    }) {
        book.matches[i].wallets.push(wallet.into());
        return Ok(i);
    }
    if book.matches.iter().filter(|m| !m.done).count() >= MAX_ACTIVE
        || book
            .matches
            .iter()
            .filter(|m| at - m.created_at < 86400)
            .count()
            >= MAX_DAILY
    {
        return Err("match_capacity");
    }
    book.matches
        .retain(|m| !m.done || at - m.created_at < 86400);
    if book.matches.iter().any(|m| m.id == id) {
        return Err("match_busy");
    }
    book.matches.push(Match {
        id,
        created_at: at,
        wallets: vec![wallet.into()],
        done: false,
        view: None,
        error: None,
    });
    Ok(book.matches.len() - 1)
}

fn request(book: &mut Book, path: &Path, wallet: &str, at: i64) -> Value {
    let before = book.clone();
    let game_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let index = match allocation(book, wallet, at, game_id) {
        Ok(i) => i,
        Err(code) => return json!({"ok":false,"error":code}),
    };
    if save(path, book).is_err() {
        *book = before;
        return json!({"ok":false,"error":"storage_failed"});
    }
    let m = &book.matches[index];
    if let Some(code) = &m.error {
        return json!({"ok":false,"error":if code == "host_funds_low" { "host_funds_low" } else { "match_host_unavailable" }});
    }
    let ready = m.view.as_ref().is_some_and(|v| at - v.observed_at <= 15);
    json!({"ok":true,"status":if ready {"assigned"} else {"preparing"},
        "network":"devnet","program_id":id().to_string(),
        "game_pda":game_pda(m.id).to_string(),"opponent_wallet":book.operator,
        "opponent_name":OPPONENT_NAME,"opponent_kind":"platform_heuristic",
        "entry_fee_lamports":ENTRY_FEE.to_string(),"epoch":constants::EPOCH_90S,
        "phase_duration":DURATION,"retry_after_seconds":5})
}

fn should_join(game: &state::Game, bot_present: bool, chain_time: i64) -> bool {
    game.phase == state::Phase::Lobby
        && game.faction_count == 1
        && !bot_present
        && chain_time > 0
        && chain_time >= game.phase_ends_at - OPPONENT_MARGIN
}

fn opponent_action(
    game: &state::Game,
    own: &state::Faction,
    address: Pubkey,
) -> Option<Instruction> {
    if !own.alive {
        return None;
    }
    match game.phase {
        state::Phase::Market if own.acted_stamp != game.stamp() && own.goods > 0 => {
            Some(ix_sell(own.goods, own.wallet, own.game, address))
        }
        state::Phase::Action if own.acted_stamp != game.stamp() => {
            Some(ix_produce(own.wallet, own.game, address))
        }
        state::Phase::Law
            if own.voted_stamp != game.stamp() && game.law_card != constants::NO_LAW =>
        {
            // Vote for the law when its direct effect benefits this faction.
            let choice = if matches!(
                game.law_card,
                constants::LAW_SUBSIDY_PRODUCE | constants::LAW_BOOM
            ) {
                state::VoteChoice::Yes
            } else {
                state::VoteChoice::No
            };
            Some(ix_vote(choice, own.wallet, own.game, address))
        }
        _ => None,
    }
}

fn refund_ix(crank: Pubkey, game: Pubkey, factions: &[Pubkey], wallets: &[Pubkey]) -> Instruction {
    use anchor_lang::solana_program::instruction::AccountMeta;
    let mut metas = accounts::SettleRefund { crank, game }.to_account_metas(None);
    metas.extend(
        factions
            .iter()
            .map(|p| AccountMeta::new_readonly(*p, false)),
    );
    metas.extend(wallets.iter().map(|p| AccountMeta::new(*p, false)));
    Instruction::new_with_bytes(id(), &instruction::SettleRefund {}.data(), metas)
}

fn submit(rpc: &RpcClient, key: &Keypair, ix: Instruction) -> Result<(), &'static str> {
    match send_ix_confirmed(rpc, key, ix) {
        Ok(receipt) => {
            println!("{}", json!({"match_host_receipt":receipt}));
            Ok(())
        }
        Err(receipt) => {
            eprintln!("{}", json!({"match_host_unconfirmed":receipt}));
            Err("transaction_unconfirmed")
        }
    }
}

fn tick(
    rpc: &RpcClient,
    key: &Keypair,
    m: &Match,
    duration: i64,
) -> Result<(View, bool), &'static str> {
    let address = game_pda(m.id);
    let account = rpc
        .get_account_with_commitment(&address, CommitmentConfig::confirmed())
        .map_err(|_| "game_read_failed")?;
    let Some(account) = account.value else {
        // The same persisted PDA makes Initialize safe to retry after a lost reply.
        if rpc
            .get_balance(&key.pubkey())
            .map_err(|_| "balance_unavailable")?
            < ENTRY_FEE + 30_000_000
        {
            return Err("host_funds_low");
        }
        submit(
            rpc,
            key,
            ix_initialize(
                key.pubkey(),
                address,
                m.id,
                duration,
                constants::ENTROPY_SLOTHASH,
                constants::EPOCH_90S,
            ),
        )?;
        return Err("initializing");
    };
    let game = agent_cli::checked_game(account.owner, &address, &account.data)
        .map_err(|_| "invalid_game")?;
    if game.admin != key.pubkey()
        || game.entry_fee != ENTRY_FEE
        || game.phase_duration != duration
        || game.epoch != constants::EPOCH_90S
        || game.entropy_mode != constants::ENTROPY_SLOTHASH
    {
        return Err("game_config_mismatch");
    }
    let view = View {
        phase: game.phase,
        ends_at: game.phase_ends_at,
        count: game.faction_count,
        observed_at: now(),
    };
    if game.settled {
        return Ok((view, true));
    }
    let chain_time = chain_now(rpc);
    if chain_time <= 0 {
        return Err("clock_unavailable");
    }
    let bot = faction_pda(&address, &key.pubkey());
    let own_account = rpc
        .get_account_with_commitment(&bot, CommitmentConfig::confirmed())
        .map_err(|_| "faction_read_failed")?;
    let own = own_account
        .value
        .map(|a| agent_cli::checked_faction(a.owner, &bot, &address, &a.data))
        .transpose()
        .map_err(|_| "invalid_faction")?;
    if should_join(&game, own.is_some(), chain_time) {
        if rpc
            .get_balance(&key.pubkey())
            .map_err(|_| "balance_unavailable")?
            < ENTRY_FEE + 5_000_000
        {
            return Err("host_funds_low");
        }
        submit(rpc, key, ix_join(OPPONENT_NAME, key.pubkey(), address, bot))?;
        // Re-read the Game before advancing: Join may have succeeded after a lost reply.
        return Ok((view, false));
    }
    if let Some(own) = own {
        if let Some(ix) = opponent_action(&game, &own, bot) {
            submit(rpc, key, ix)?;
        }
    }
    if matches!(game.phase, state::Phase::Finished | state::Phase::Aborted)
        || chain_time >= game.phase_ends_at
    {
        let factions = discover_factions(rpc, &address);
        if factions.len() != game.faction_count as usize {
            return Err("incomplete_factions");
        }
        let wallets: Option<Vec<_>> = factions
            .iter()
            .map(|f| fetch_faction(rpc, f).map(|v| v.wallet))
            .collect();
        let wallets = wallets.ok_or("incomplete_factions")?;
        let ix = match game.phase {
            state::Phase::Finished => {
                ix_settle(key.pubkey(), address, &factions, &wallets, game.admin)
            }
            state::Phase::Aborted => refund_ix(key.pubkey(), address, &factions, &wallets),
            _ => ix_advance(key.pubkey(), address, &factions),
        };
        submit(rpc, key, ix)?;
    }
    Ok((view, false))
}

fn read_match(stream: &mut TcpStream) -> Option<(bool, String)> {
    stream.set_read_timeout(Some(Duration::from_secs(3))).ok()?;
    let mut reader = BufReader::new(stream);
    let mut remaining = 2048;
    let mut line = String::new();
    let n = reader
        .by_ref()
        .take(remaining + 1)
        .read_line(&mut line)
        .ok()?;
    let funding = line == "POST /fund HTTP/1.1\r\n";
    if n as u64 > remaining || (!funding && line != "POST /match HTTP/1.1\r\n") {
        return None;
    }
    remaining -= n as u64;
    let mut length = None;
    loop {
        line.clear();
        let n = reader
            .by_ref()
            .take(remaining + 1)
            .read_line(&mut line)
            .ok()?;
        if n == 0 || n as u64 > remaining || !line.ends_with("\r\n") {
            return None;
        }
        remaining -= n as u64;
        if line == "\r\n" {
            break;
        }
        let (name, value) = line.trim_end().split_once(':')?;
        if name.eq_ignore_ascii_case("transfer-encoding") {
            return None;
        }
        if name.eq_ignore_ascii_case("content-length") {
            if length.is_some() {
                return None;
            }
            length = Some(value.trim().parse::<usize>().ok()?);
        }
    }
    let length = length.filter(|n| *n <= 128)?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    let body: Value = serde_json::from_slice(&body).ok()?;
    if body.as_object()?.len() != 1 {
        return None;
    }
    Some((funding, body["wallet"].as_str()?.into()))
}

pub fn run(args: &[String]) -> Result<(), String> {
    if args.iter().any(|a| a == "--help") {
        println!("bots match-host --key PRIVATE_DEVNET_KEY --state PRIVATE_STATE; ALASHI_MATCH_PORT (default 8098); loopback only, authenticated requests must pass through arenad");
        return Ok(());
    }
    let key_path = flag_value(args, "--key").ok_or("--key required")?;
    let path = PathBuf::from(flag_value(args, "--state").ok_or("--state required")?);
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map_err(|_| "state_directory_unavailable")?
            .join(path)
    };
    let key = agent_cli::read_key(&key_path).map_err(|_| "invalid_host_key")?;
    let rpc = RpcClient::new_with_timeout_and_commitment(
        rpc_url(),
        Duration::from_secs(10),
        CommitmentConfig::confirmed(),
    );
    agent_cli::devnet(&rpc).map_err(|_| "devnet_required")?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path.with_extension("lock"))
        .map_err(|_| "state_lock_unavailable")?;
    lock.try_lock().map_err(|_| "match_host_already_running")?;
    let book = if path.exists() {
        let meta = fs::symlink_metadata(&path).map_err(|_| "state_unavailable")?;
        if !meta.is_file() || meta.len() > 128 * 1024 {
            return Err("invalid_state".into());
        }
        let book: Book = serde_json::from_slice(&fs::read(&path).map_err(|_| "state_unavailable")?)
            .map_err(|_| "invalid_state")?;
        if book.version != 1 || book.operator != key.pubkey().to_string() {
            return Err("host_identity_changed".into());
        }
        book
    } else {
        Book {
            version: 1,
            operator: key.pubkey().to_string(),
            matches: vec![],
        }
    };
    let port = std::env::var("ALASHI_MATCH_PORT")
        .unwrap_or_else(|_| "8098".into())
        .parse::<u16>()
        .map_err(|_| "invalid_port")?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(|_| "bind_failed")?;
    let sponsorship = sponsorship::Host::start(&key, path.with_extension("funding.json"))?;
    let book = Arc::new(Mutex::new(book));
    let worker_book = Arc::clone(&book);
    let worker_path = path.clone();
    std::thread::spawn(move || {
        let mut running = std::collections::HashMap::<u64, std::thread::JoinHandle<()>>::new();
        loop {
            running.retain(|_, worker| !worker.is_finished());
            let jobs: Vec<_> = worker_book
                .lock()
                .unwrap()
                .matches
                .iter()
                .filter(|m| !m.done)
                .cloned()
                .collect();
            for job in jobs {
                if running.contains_key(&job.id) {
                    continue;
                }
                let book = Arc::clone(&worker_book);
                let path = worker_path.clone();
                let signer = Keypair::new_from_array(key.to_bytes()[..32].try_into().unwrap());
                running.insert(
                    job.id,
                    std::thread::spawn(move || {
                        let rpc = RpcClient::new_with_timeout_and_commitment(
                            rpc_url(),
                            Duration::from_secs(10),
                            CommitmentConfig::confirmed(),
                        );
                        loop {
                            match tick(&rpc, &signer, &job, DURATION) {
                                Ok((view, done)) => {
                                    let mut book = book.lock().unwrap();
                                    if let Some(m) =
                                        book.matches.iter_mut().find(|m| m.id == job.id)
                                    {
                                        m.view = Some(view);
                                        m.error = None;
                                        m.done = done;
                                    }
                                    if done {
                                        if save(&path, &book).is_err() {
                                            eprintln!("match_host: state_save_failed");
                                        }
                                        break;
                                    }
                                }
                                Err(code) => {
                                    eprintln!("match_host {}: {code}", game_pda(job.id));
                                    let mut book = book.lock().unwrap();
                                    if let Some(m) =
                                        book.matches.iter_mut().find(|m| m.id == job.id)
                                    {
                                        m.error = (code != "initializing").then(|| code.to_owned());
                                    }
                                }
                            }
                            sleep(Duration::from_secs(5));
                        }
                    }),
                );
            }
            sleep(Duration::from_secs(1));
        }
    });
    println!("match_host listening on 127.0.0.1:{port}");
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        let body = if let Some((funding, wallet)) = read_match(&mut stream) {
            if funding { sponsorship.request(&wallet) }
            else { request(&mut book.lock().unwrap(), &path, &wallet, now()) }
        } else {
            json!({"ok":false,"error":"invalid_request"})
        };
        let status = if body["ok"] == true {
            "200 OK"
        } else {
            "503 Service Unavailable"
        };
        let text = body.to_string();
        let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
        let _ = write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
    }
    drop(lock);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore = "requires a dedicated local validator with the deployed Alashi program"]
    fn signed_games_finish_with_solo_two_players_and_empty_lobby() {
        let url = std::env::var("ALASHI_LOCAL_TEST_RPC").expect("dedicated local RPC required");
        let port = url
            .strip_prefix("http://127.0.0.1:")
            .expect("local RPC only");
        assert!(port.parse::<u16>().is_ok(), "bare loopback URL required");
        let rpc = RpcClient::new_with_timeout_and_commitment(
            url,
            Duration::from_secs(10),
            CommitmentConfig::confirmed(),
        );
        let genesis = rpc.get_genesis_hash().unwrap().to_string();
        assert_ne!(genesis, "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG");
        assert_ne!(genesis, "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d");

        for player_count in [1, 2, 0] {
            let host = Keypair::new();
            let players: Vec<_> = (0..player_count).map(|_| Keypair::new()).collect();
            for key in std::iter::once(&host) {
                let sig = rpc.request_airdrop(&key.pubkey(), 2_000_000_000).unwrap();
                let started = Instant::now();
                while !rpc.confirm_transaction(&sig).unwrap() {
                    assert!(started.elapsed() < Duration::from_secs(20));
                    sleep(Duration::from_millis(100));
                }
            }
            for player in &players {
                sponsorship::fund_local_player(&rpc, &host, player);
            }
            let mut m = Match {
                id: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos() as u64,
                created_at: now(),
                wallets: players.iter().map(|k| k.pubkey().to_string()).collect(),
                done: false,
                view: None,
                error: None,
            };
            let address = game_pda(m.id);
            assert_eq!(tick(&rpc, &host, &m, 1).err(), Some("initializing"));
            for player in &players {
                send_ix_confirmed(
                    &rpc,
                    player,
                    ix_join(
                        "Visitor",
                        player.pubkey(),
                        address,
                        faction_pda(&address, &player.pubkey()),
                    ),
                )
                .unwrap();
            }
            let mut restarted = false;
            let mut rounds = std::collections::BTreeSet::new();
            let mut player_actions = 0;
            let mut balances_before_settle = None;
            let started = Instant::now();
            loop {
                assert!(
                    started.elapsed() < Duration::from_secs(180),
                    "match timed out"
                );
                let game = fetch_game(&rpc, &address).expect("initialized game");
                if game.round > 0 {
                    rounds.insert(game.round);
                }
                for player in &players {
                    let faction = faction_pda(&address, &player.pubkey());
                    let own = fetch_faction(&rpc, &faction).unwrap();
                    if let Some(ix) = opponent_action(&game, &own, faction) {
                        send_ix_confirmed(&rpc, player, ix).unwrap();
                        player_actions += 1;
                    }
                }
                if game.phase == state::Phase::Finished && !game.settled {
                    balances_before_settle = Some(
                        players
                            .iter()
                            .map(|p| rpc.get_balance(&p.pubkey()).unwrap())
                            .collect::<Vec<_>>(),
                    );
                }
                if !restarted && game.round >= 2 {
                    m = serde_json::from_slice(&serde_json::to_vec(&m).unwrap()).unwrap();
                    restarted = true;
                }
                if let Ok((_, done)) = tick(&rpc, &host, &m, 1) {
                    if done {
                        break;
                    }
                }
                sleep(Duration::from_millis(100));
            }
            let game = fetch_game(&rpc, &address).unwrap();
            assert!(game.settled);
            let bot = fetch_faction(&rpc, &faction_pda(&address, &host.pubkey()));
            if player_count == 0 {
                assert_eq!(game.phase, state::Phase::Aborted);
                assert!(bot.is_none());
            } else {
                assert_eq!(game.phase, state::Phase::Finished);
                assert_eq!(game.round, 6);
                assert_eq!(game.faction_count, 2);
                assert!(player_actions > 0);
                assert!(restarted);
                assert_eq!(rounds.len(), 6);
                assert_eq!(bot.is_some(), player_count == 1);
                let account = rpc.get_account(&address).unwrap();
                assert_eq!(
                    account.lamports,
                    rpc.get_minimum_balance_for_rent_exemption(account.data.len())
                        .unwrap()
                );
                for (player, before) in players.iter().zip(balances_before_settle.unwrap()) {
                    assert!(
                        rpc.get_balance(&player.pubkey()).unwrap() > before,
                        "entry must receive an actual settlement payout"
                    );
                }
            }
            println!("verified signed match: players={player_count}, game={address}, round={}, settled={}", game.round, game.settled);
        }
    }

    fn book() -> Book {
        Book {
            version: 1,
            operator: Pubkey::new_unique().to_string(),
            matches: vec![],
        }
    }

    #[test]
    fn assignment_is_persisted_before_reply_and_failed_save_rolls_back() {
        let root = std::env::temp_dir().join(format!("alashi-match-state-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let path = root.join("matches.json");
        let wallet = Pubkey::new_unique().to_string();
        let mut b = book();
        let reply = request(&mut b, &path, &wallet, 100);
        assert_eq!(reply["status"], "preparing");
        let mut restored: Book = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            request(&mut restored, &path, &wallet, 200)["game_pda"],
            reply["game_pda"]
        );
        let before = serde_json::to_vec(&restored).unwrap();
        let failed = request(
            &mut restored,
            &root.join("missing/state"),
            &Pubkey::new_unique().to_string(),
            201,
        );
        assert_eq!(failed["error"], "storage_failed");
        assert_eq!(serde_json::to_vec(&restored).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_host_rejects_ambiguous_framing_and_extra_fields() {
        fn parse(raw: String) -> Option<(bool, String)> {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let writer = std::thread::spawn(move || {
                let mut stream = TcpStream::connect(address).unwrap();
                stream.write_all(raw.as_bytes()).unwrap();
            });
            let (mut stream, _) = listener.accept().unwrap();
            let result = read_match(&mut stream);
            writer.join().unwrap();
            result
        }
        let body = json!({"wallet":Pubkey::new_unique().to_string()}).to_string();
        let prefix = format!("POST /match HTTP/1.1\r\nContent-Length: {}\r\n", body.len());
        assert!(parse(format!("{prefix}\r\n{body}")).is_some());
        assert!(parse(format!(
            "{prefix}Content-Length: {}\r\n\r\n{body}",
            body.len()
        ))
        .is_none());
        assert!(parse(format!("{prefix}Transfer-Encoding: chunked\r\n\r\n{body}")).is_none());
        assert!(parse("POST /match HTTP/1.1\r\nContent-Length: 9999\r\n\r\n".into()).is_none());
        let extra = "{\"wallet\":\"public\",\"key\":\"forbidden\"}";
        assert!(parse(format!(
            "POST /match HTTP/1.1\r\nContent-Length: {}\r\n\r\n{extra}",
            extra.len()
        ))
        .is_none());
    }

    #[test]
    fn retries_restart_and_concurrent_reservations_share_one_lobby() {
        let mut b = book();
        let a = Pubkey::new_unique().to_string();
        let c = Pubkey::new_unique().to_string();
        assert_eq!(allocation(&mut b, &a, 100, 7).unwrap(), 0);
        assert_eq!(allocation(&mut b, &a, 101, 8).unwrap(), 0);
        let mut restored: Book = serde_json::from_slice(&serde_json::to_vec(&b).unwrap()).unwrap();
        assert_eq!(allocation(&mut restored, &a, 200, 9).unwrap(), 0);
        assert_eq!(allocation(&mut b, &c, 102, 10).unwrap(), 0);
        assert_eq!(b.matches.len(), 1);
        assert_eq!(b.matches[0].wallets.len(), 2);
    }

    #[test]
    fn one_player_gets_one_opponent_but_empty_and_two_player_lobbies_do_not() {
        let mut game = state::Game {
            phase: state::Phase::Lobby,
            faction_count: 1,
            phase_ends_at: 100,
            ..Default::default()
        };
        assert!(!should_join(&game, false, 79));
        assert!(should_join(&game, false, 80));
        assert!(should_join(&game, false, 105));
        assert!(!should_join(&game, true, 90));
        for count in [0, 2, 6] {
            game.faction_count = count;
            assert!(!should_join(&game, false, 90));
        }
        game.faction_count = 1;
        game.phase = state::Phase::Market;
        assert!(!should_join(&game, false, 90));
    }

    #[test]
    fn stale_full_and_starting_lobbies_never_receive_a_new_assignment() {
        for (count, ends_at, observed_at) in [(6, 300, 100), (1, 120, 100), (1, 300, 70)] {
            let mut b = book();
            b.matches.push(Match {
                id: 1,
                created_at: 1,
                wallets: vec![],
                done: false,
                error: None,
                view: Some(View {
                    phase: state::Phase::Lobby,
                    count,
                    ends_at,
                    observed_at,
                }),
            });
            assert_eq!(
                allocation(&mut b, &Pubkey::new_unique().to_string(), 100, 2).unwrap(),
                1
            );
        }
    }

    #[test]
    fn active_capacity_and_daily_budget_are_bounded() {
        let mut b = book();
        for i in 0..MAX_ACTIVE {
            allocation(
                &mut b,
                &Pubkey::new_unique().to_string(),
                100 + (i as i64) * 100,
                i as u64,
            )
            .unwrap();
        }
        assert_eq!(
            allocation(&mut b, &Pubkey::new_unique().to_string(), 1000, 99),
            Err("match_capacity")
        );
        for m in &mut b.matches {
            m.done = true;
        }
        for i in MAX_ACTIVE..MAX_DAILY {
            let index =
                allocation(&mut b, &Pubkey::new_unique().to_string(), 1000, i as u64).unwrap();
            b.matches[index].done = true;
        }
        assert_eq!(
            allocation(&mut b, &Pubkey::new_unique().to_string(), 1000, 99),
            Err("match_capacity")
        );
        assert!(allocation(&mut b, &Pubkey::new_unique().to_string(), 90000, 100).is_ok());
    }

    #[test]
    fn opponent_uses_confirmed_turn_stamps_after_restart() {
        let wallet = Pubkey::new_unique();
        let mut game = state::Game {
            phase: state::Phase::Market,
            round: 2,
            ..Default::default()
        };
        let mut own = state::Faction {
            game: game_pda(1),
            wallet,
            alive: true,
            goods: 3,
            ..Default::default()
        };
        let address = faction_pda(&own.game, &wallet);
        assert_eq!(
            opponent_action(&game, &own, address).unwrap().data,
            ix_sell(3, wallet, own.game, address).data
        );
        own.acted_stamp = game.stamp();
        assert!(opponent_action(&game, &own, address).is_none());
        game.phase = state::Phase::Action;
        assert_eq!(
            opponent_action(&game, &own, address).unwrap().data,
            ix_produce(wallet, own.game, address).data
        );
        game.phase = state::Phase::Law;
        game.law_card = constants::LAW_BOOM;
        assert_eq!(
            opponent_action(&game, &own, address).unwrap().data,
            ix_vote(state::VoteChoice::Yes, wallet, own.game, address).data
        );
        own.voted_stamp = game.stamp();
        assert!(opponent_action(&game, &own, address).is_none());
    }
}
