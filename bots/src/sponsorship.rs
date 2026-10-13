//! Bounded, durable testSOL sponsorship before a visitor can pay the devnet Memo.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::File,
    sync::{Arc, Mutex},
};

const MAX_DAILY: usize = 24;
const MAX_ACTIVE: usize = 4;
// Lamports (1 SOL = 1_000_000_000). Target covers the 0.005 SOL entry, faction rent and fees for a few games.
pub(crate) const FUNDING_TARGET: u64 = 20_000_000;
pub(crate) const FUNDING_MINIMUM: u64 = 12_000_000;
const FUNDING_RESERVE: u64 = 200_000_000;
// Completed entries keep the amount they were sent with; the target was 0.1 testSOL before 13 Oct 2026.
const MAX_RECORDED_FUNDING: u64 = 100_000_000;
const MAX_FUNDED_WALLETS: usize = 1000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Book {
    version: u8,
    operator: String,
    funding: Vec<Funding>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Funding {
    wallet: String,
    created_at: i64,
    transaction: Option<solana_transaction::Transaction>,
    ready: bool,
    failed: bool,
    #[serde(skip)]
    error: Option<String>,
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

fn valid_funding(book: &Book) -> bool {
    if book.funding.len() > MAX_FUNDED_WALLETS {
        return false;
    }
    let Ok(operator) = book.operator.parse::<Pubkey>() else {
        return false;
    };
    let mut seen = std::collections::HashSet::new();
    book.funding.iter().all(|f| {
        let Ok(wallet) = f.wallet.parse::<Pubkey>() else {
            return false;
        };
        if wallet == operator
            || !wallet.is_on_curve()
            || wallet.to_string() != f.wallet
            || !seen.insert(&f.wallet)
            || f.created_at <= 0
            || (f.failed && f.ready)
        {
            return false;
        }
        f.transaction.as_ref().is_none_or(|tx| {
            let Some(ix) = tx.message.instructions.first() else {
                return false;
            };
            if ix.data.len() != 12 || tx.signatures.len() != 1 {
                return false;
            }
            let amount = u64::from_le_bytes(ix.data[4..].try_into().unwrap());
            let limit = if f.ready || f.failed { MAX_RECORDED_FUNDING } else { FUNDING_TARGET };
            amount > 0
                && amount <= limit
                && tx.message
                    == Message::new_with_blockhash(
                        &[funding_transfer(operator, wallet, amount)],
                        Some(&operator),
                        &tx.message.recent_blockhash,
                    )
        })
    })
}

fn funding_reply(book: &Book, i: usize) -> Value {
    let f = &book.funding[i];
    if f.failed {
        return json!({"ok":false,"error":"funding_failed"});
    }
    if let Some(code) = &f.error {
        return json!({"ok":false,"error":code});
    }
    json!({"ok":true,"network":"devnet","wallet":f.wallet,
        "status":if f.ready {"ready"} else {"preparing"},
        "target_lamports":FUNDING_TARGET.to_string(),"retry_after_seconds":5,
        "signature":f.transaction.as_ref().map(|tx| tx.signatures[0].to_string())})
}

fn request_funding(book: &mut Book, path: &Path, wallet: &str, at: i64) -> Value {
    if wallet == book.operator
        || wallet
            .parse::<Pubkey>()
            .ok()
            .is_none_or(|p| !p.is_on_curve() || p.to_string() != wallet)
    {
        return json!({"ok":false,"error":"invalid_wallet"});
    }
    if let Some(i) = book.funding.iter().position(|f| f.wallet == wallet) {
        return funding_reply(book, i);
    }
    if book
        .funding
        .iter()
        .filter(|f| !f.ready && !f.failed)
        .count()
        >= MAX_ACTIVE
        || book.funding.len() >= MAX_FUNDED_WALLETS
        || book
            .funding
            .iter()
            .filter(|f| at.saturating_sub(f.created_at) < 86400)
            .count()
            >= MAX_DAILY
    {
        return json!({"ok":false,"error":"funding_capacity"});
    }
    let before = book.clone();
    book.funding.push(Funding {
        wallet: wallet.into(),
        created_at: at,
        transaction: None,
        ready: false,
        failed: false,
        error: None,
    });
    if save(path, book).is_err() {
        *book = before;
        return json!({"ok":false,"error":"storage_failed"});
    }
    funding_reply(book, book.funding.len() - 1)
}

fn funding_transfer(from: Pubkey, to: Pubkey, lamports: u64) -> Instruction {
    // SystemInstruction::Transfer has a fixed bincode u32 tag and u64 amount.
    let mut data = 2u32.to_le_bytes().to_vec();
    data.extend_from_slice(&lamports.to_le_bytes());
    Instruction::new_with_bytes(
        system_program::ID,
        &data,
        vec![AccountMeta::new(from, true), AccountMeta::new(to, false)],
    )
}

fn update_funding(
    book: &Mutex<Book>,
    path: &Path,
    wallet: &str,
    change: impl FnOnce(&mut Funding),
) -> Result<(), &'static str> {
    let mut book = book.lock().unwrap();
    let before = book.clone();
    let entry = book
        .funding
        .iter_mut()
        .find(|f| f.wallet == wallet)
        .ok_or("funding_unavailable")?;
    change(entry);
    if save(path, &book).is_err() {
        *book = before;
        return Err("storage_failed");
    }
    Ok(())
}

fn tick_funding(
    rpc: &RpcClient,
    key: &Keypair,
    book: &Mutex<Book>,
    path: &Path,
    wallet: &str,
) -> Result<(), &'static str> {
    let entry = book
        .lock()
        .unwrap()
        .funding
        .iter()
        .find(|f| f.wallet == wallet)
        .cloned()
        .ok_or("funding_unavailable")?;
    if entry.ready || entry.failed {
        return Ok(());
    }
    let tx = if let Some(tx) = entry.transaction {
        tx
    } else {
        let recipient: Pubkey = wallet.parse().map_err(|_| "invalid_wallet")?;
        let balance = rpc
            .get_balance(&recipient)
            .map_err(|_| "funding_rpc_unavailable")?;
        if balance >= FUNDING_MINIMUM {
            return update_funding(book, path, wallet, |f| {
                f.ready = true;
                f.error = None;
            });
        }
        let amount = FUNDING_TARGET - balance;
        let pending: Vec<_> = book
            .lock()
            .unwrap()
            .funding
            .iter()
            .filter(|f| !f.ready && !f.failed)
            .filter_map(|f| f.transaction.as_ref())
            .cloned()
            .collect();
        let prepared = prepare_ix(rpc, key, funding_transfer(key.pubkey(), recipient, amount))
            .map_err(|_| "funding_rpc_unavailable")?;
        let VersionedMessage::Legacy(message) = prepared.message else {
            return Err("funding_unavailable");
        };
        let tx = solana_transaction::Transaction {
            signatures: prepared.signatures,
            message,
        };
        let mut required = amount
            + FUNDING_RESERVE
            + rpc
                .get_fee_for_message(&tx.message)
                .map_err(|_| "funding_rpc_unavailable")?;
        for tx in pending {
            required +=
                u64::from_le_bytes(tx.message.instructions[0].data[4..].try_into().unwrap())
                    + rpc
                        .get_fee_for_message(&tx.message)
                        .map_err(|_| "funding_rpc_unavailable")?;
        }
        if rpc
            .get_balance(&key.pubkey())
            .map_err(|_| "funding_rpc_unavailable")?
            < required
        {
            return Err("host_funds_low");
        }
        // Store the entire signed transaction before sending. All retries reuse it.
        update_funding(book, path, wallet, |f| {
            f.transaction = Some(tx.clone());
            f.error = None;
        })?;
        tx
    };
    match wait_receipt(rpc, &tx.signatures[0], Duration::ZERO) {
        Ok(_) => update_funding(book, path, wallet, |f| {
            f.ready = true;
            f.error = None;
        }),
        Err(receipt) if receipt["status"] == "failed" => update_funding(book, path, wallet, |f| {
            f.failed = true;
            f.error = None;
        }),
        Err(_) => {
            if !rpc
                .is_blockhash_valid(&tx.message.recent_blockhash, CommitmentConfig::confirmed())
                .map_err(|_| "funding_rpc_unavailable")?
            {
                return update_funding(book, path, wallet, |f| {
                    f.failed = true;
                    f.error = None;
                });
            }
            // A lost reply only resubmits the saved transaction with the same signature.
            let _ = rpc.send_transaction(&tx);
            Ok(())
        }
    }
}

pub(super) struct Host {
    book: Arc<Mutex<Book>>,
    path: PathBuf,
    _lock: File,
}

impl Host {
    pub(super) fn start(key: &Keypair, path: PathBuf) -> Result<Self, String> {
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path.with_extension("funding.lock"))
            .map_err(|_| "funding_lock_unavailable")?;
        lock.try_lock().map_err(|_| "funding_already_running")?;
        let book = match fs::symlink_metadata(&path) {
            Ok(meta) => {
                if !meta.is_file()
                    || meta.len() > 2 * 1024 * 1024
                    || meta.permissions().mode() & 0o077 != 0
                {
                    return Err("invalid_funding_state".into());
                }
                let b: Book = serde_json::from_slice(
                    &fs::read(&path).map_err(|_| "funding_state_unavailable")?,
                )
                .map_err(|_| "invalid_funding_state")?;
                if b.version != 1 || b.operator != key.pubkey().to_string() || !valid_funding(&b) {
                    return Err("invalid_funding_state".into());
                }
                b
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Book {
                version: 1,
                operator: key.pubkey().to_string(),
                funding: vec![],
            },
            Err(_) => return Err("funding_state_unavailable".into()),
        };
        let rpc = RpcClient::new_with_timeout_and_commitment(
            rpc_url(),
            Duration::from_secs(10),
            CommitmentConfig::confirmed(),
        );
        agent_cli::devnet(&rpc).map_err(|_| "devnet_required")?;
        let book = Arc::new(Mutex::new(book));
        let worker_book = Arc::clone(&book);
        let worker_path = path.clone();
        let key = Keypair::new_from_array(key.to_bytes()[..32].try_into().unwrap());
        std::thread::spawn(move || loop {
            let wallets: Vec<_> = worker_book
                .lock()
                .unwrap()
                .funding
                .iter()
                .filter(|f| !f.ready && !f.failed)
                .map(|f| f.wallet.clone())
                .collect();
            for wallet in wallets {
                if let Err(code) = tick_funding(&rpc, &key, &worker_book, &worker_path, &wallet) {
                    let mut book = worker_book.lock().unwrap();
                    if let Some(f) = book.funding.iter_mut().find(|f| f.wallet == wallet) {
                        f.error = Some(code.into());
                    }
                }
            }
            sleep(Duration::from_secs(5));
        });
        Ok(Self {
            book,
            path,
            _lock: lock,
        })
    }

    pub(super) fn request(&self, wallet: &str) -> Value {
        request_funding(&mut self.book.lock().unwrap(), &self.path, wallet, now())
    }
}

#[cfg(test)]
pub(super) fn fund_local_player(rpc: &RpcClient, host: &Keypair, player: &Keypair) {
    let genesis = rpc.get_genesis_hash().unwrap().to_string();
    assert_ne!(genesis, "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG");
    assert_ne!(genesis, "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d");
    assert_eq!(rpc.get_balance(&player.pubkey()).unwrap(), 0);
    let dir = std::env::temp_dir().join(format!(
        "alashi-signed-sponsor-{}-{}",
        std::process::id(),
        player.pubkey()
    ));
    fs::create_dir(&dir).unwrap();
    let path = dir.join("state.json");
    let mut book = Book {
        version: 1,
        operator: host.pubkey().to_string(),
        funding: vec![],
    };
    let wallet = player.pubkey().to_string();
    request_funding(&mut book, &path, &wallet, now());
    let locked = Mutex::new(book);
    let started = std::time::Instant::now();
    while !locked.lock().unwrap().funding[0].ready {
        assert!(started.elapsed() < Duration::from_secs(20));
        tick_funding(rpc, host, &locked, &path, &wallet).unwrap();
        sleep(Duration::from_millis(100));
    }
    assert_eq!(rpc.get_balance(&player.pubkey()).unwrap(), FUNDING_TARGET);
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    fn funding_path() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "alashi-funding-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        dir.join("state.json")
    }

    #[test]
    fn funding_is_once_per_wallet_and_reserved_before_reply() {
        let path = funding_path();
        let mut b = book();
        let wallet = Keypair::new().pubkey().to_string();
        assert_eq!(
            request_funding(&mut b, &path, &wallet, 10)["status"],
            "preparing"
        );
        let mut restarted: Book = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            request_funding(&mut restarted, &path, &wallet, 90000)["status"],
            "preparing"
        );
        assert_eq!(restarted.funding.len(), 1);
        restarted.funding[0].ready = true;
        assert_eq!(
            request_funding(&mut restarted, &path, &wallet, 90001)["status"],
            "ready"
        );
        assert_eq!(restarted.funding.len(), 1);
        let before = serde_json::to_vec(&restarted).unwrap();
        assert_eq!(
            request_funding(
                &mut restarted,
                Path::new("/missing-parent/state.json"),
                &Keypair::new().pubkey().to_string(),
                90002
            )["error"],
            "storage_failed"
        );
        assert_eq!(serde_json::to_vec(&restarted).unwrap(), before);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn funding_limits_and_invalid_destinations_cannot_allocate() {
        let path = funding_path();
        let mut b = book();
        for wallet in [
            b.operator.clone(),
            "not-wallet".into(),
            Pubkey::find_program_address(&[b"funding-test"], &id())
                .0
                .to_string(),
        ] {
            assert_eq!(
                request_funding(&mut b, &path, &wallet, 10)["error"],
                "invalid_wallet"
            );
        }
        assert!(b.funding.is_empty());
        for _ in 0..MAX_DAILY {
            assert_eq!(
                request_funding(&mut b, &path, &Keypair::new().pubkey().to_string(), 10)["ok"],
                true
            );
            b.funding.last_mut().unwrap().ready = true;
        }
        let original = b.funding[0].wallet.clone();
        assert_eq!(
            request_funding(&mut b, &path, &Keypair::new().pubkey().to_string(), 11)["error"],
            "funding_capacity"
        );
        assert_eq!(request_funding(&mut b, &path, &original, 11)["ok"], true);
        assert_eq!(
            request_funding(&mut b, &path, &Keypair::new().pubkey().to_string(), 90000)["ok"],
            true
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn zero_balance_sponsorship_is_saved_before_send_and_preserves_reserve() {
        use solana_rpc_client_api::request::RpcRequest;
        let path = funding_path();
        let key = Keypair::new();
        let wallet = Keypair::new().pubkey().to_string();
        let mut b = book();
        b.operator = key.pubkey().to_string();
        request_funding(&mut b, &path, &wallet, 10);
        let locked = Mutex::new(b);
        let rpc = RpcClient::new_mock_with_mocks_map(
            "succeeds",
            [
                (
                    RpcRequest::GetBalance,
                    json!({"context":{"slot":1},"value":0}),
                ),
                (
                    RpcRequest::GetBalance,
                    json!({"context":{"slot":1},"value":FUNDING_TARGET + FUNDING_RESERVE + 5000}),
                ),
                (RpcRequest::GetTransaction, Value::Null),
                (
                    RpcRequest::GetFeeForMessage,
                    json!({"context":{"slot":1},"value":5000}),
                ),
                (
                    RpcRequest::IsBlockhashValid,
                    json!({"context":{"slot":1},"value":true}),
                ),
            ]
            .into_iter()
            .collect(),
        );
        tick_funding(&rpc, &key, &locked, &path, &wallet).unwrap();
        let restored: Book = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let tx = restored.funding[0].transaction.as_ref().unwrap();
        assert_eq!(tx.message.instructions.len(), 1);
        assert_eq!(
            tx.message.instructions[0].data,
            [
                2u32.to_le_bytes().as_slice(),
                FUNDING_TARGET.to_le_bytes().as_slice()
            ]
            .concat()
        );
        assert!(!restored.funding[0].ready);
        assert_eq!(tx.message.account_keys[0], key.pubkey());
        assert_eq!(tx.message.account_keys[1].to_string(), wallet);
        let another = Keypair::new().pubkey().to_string();
        request_funding(&mut locked.lock().unwrap(), &path, &another, 11);
        let rpc = RpcClient::new_mock_with_mocks_map(
            "succeeds",
            [
                (
                    RpcRequest::GetBalance,
                    json!({"context":{"slot":1},"value":0}),
                ),
                (
                    RpcRequest::GetBalance,
                    json!({"context":{"slot":1},"value":2 * FUNDING_TARGET + FUNDING_RESERVE + 5000}),
                ),
                (
                    RpcRequest::GetFeeForMessage,
                    json!({"context":{"slot":1},"value":5000}),
                ),
                (
                    RpcRequest::GetFeeForMessage,
                    json!({"context":{"slot":1},"value":5000}),
                ),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(
            tick_funding(&rpc, &key, &locked, &path, &another).unwrap_err(),
            "host_funds_low"
        );
        assert!(locked.lock().unwrap().funding[1].transaction.is_none());
        let poor_path = funding_path();
        let mut b = book();
        b.operator = key.pubkey().to_string();
        let poor_wallet = Keypair::new().pubkey().to_string();
        request_funding(&mut b, &poor_path, &poor_wallet, 10);
        let poor = Mutex::new(b);
        let rpc = RpcClient::new_mock_with_mocks_map(
            "succeeds",
            [
                (
                    RpcRequest::GetBalance,
                    json!({"context":{"slot":1},"value":0}),
                ),
                (
                    RpcRequest::GetBalance,
                    json!({"context":{"slot":1},"value":FUNDING_TARGET + FUNDING_RESERVE}),
                ),
                (
                    RpcRequest::GetFeeForMessage,
                    json!({"context":{"slot":1},"value":5000}),
                ),
            ]
            .into_iter()
            .collect(),
        );
        assert_eq!(
            tick_funding(&rpc, &key, &poor, &poor_path, &poor_wallet).unwrap_err(),
            "host_funds_low"
        );
        assert!(poor.lock().unwrap().funding[0].transaction.is_none());
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
        fs::remove_dir_all(poor_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn funding_restart_and_lost_receipt_keep_the_exact_signed_transfer() {
        use solana_rpc_client_api::request::RpcRequest;
        let path = funding_path();
        let key = Keypair::new();
        let wallet = Keypair::new().pubkey();
        let tx = VersionedTransaction::try_new(
            VersionedMessage::Legacy(Message::new_with_blockhash(
                &[funding_transfer(key.pubkey(), wallet, FUNDING_TARGET)],
                Some(&key.pubkey()),
                &Default::default(),
            )),
            &[&key],
        )
        .unwrap();
        let VersionedMessage::Legacy(message) = tx.message else {
            panic!("legacy transfer");
        };
        let tx = solana_transaction::Transaction {
            signatures: tx.signatures,
            message,
        };
        let mut b = book();
        b.operator = key.pubkey().to_string();
        request_funding(&mut b, &path, &wallet.to_string(), 10);
        let locked = Mutex::new(b);
        update_funding(&locked, &path, &wallet.to_string(), |f| {
            f.transaction = Some(tx.clone())
        })
        .unwrap();
        let restored: Book = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(valid_funding(&restored));
        let mut damaged = restored.clone();
        damaged.funding[0]
            .transaction
            .as_mut()
            .unwrap()
            .signatures
            .clear();
        assert!(!valid_funding(&damaged));
        let mut damaged = restored.clone();
        damaged.funding.push(damaged.funding[0].clone());
        assert!(!valid_funding(&damaged));
        let locked = Mutex::new(restored);
        for _ in 0..2 {
            let rpc = RpcClient::new_mock_with_mocks(
                "succeeds",
                [
                    (RpcRequest::GetTransaction, Value::Null),
                    (
                        RpcRequest::IsBlockhashValid,
                        json!({"context":{"slot":1},"value":true}),
                    ),
                ]
                .into(),
            );
            tick_funding(&rpc, &key, &locked, &path, &wallet.to_string()).unwrap();
            let saved: Book = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            assert_eq!(
                serde_json::to_vec(&saved.funding[0].transaction).unwrap(),
                serde_json::to_vec(&Some(tx.clone())).unwrap()
            );
            assert!(!saved.funding[0].ready);
        }
        let rpc = RpcClient::new_mock_with_mocks(
            "succeeds",
            [(
                RpcRequest::GetTransaction,
                json!({"slot":42,"meta":{"err":null}}),
            )]
            .into(),
        );
        tick_funding(&rpc, &key, &locked, &path, &wallet.to_string()).unwrap();
        assert!(locked.lock().unwrap().funding[0].ready);
        // Ready is durable and a later request never makes another transaction.
        let mut restored: Book = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            request_funding(&mut restored, &path, &wallet.to_string(), 90000)["status"],
            "ready"
        );
        assert_eq!(restored.funding.len(), 1);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn expired_funding_never_replaces_the_payment_and_frees_capacity() {
        use solana_rpc_client_api::request::RpcRequest;
        let path = funding_path();
        let key = Keypair::new();
        let wallet = Keypair::new().pubkey();
        let message = Message::new_with_blockhash(
            &[funding_transfer(key.pubkey(), wallet, FUNDING_TARGET)],
            Some(&key.pubkey()),
            &Default::default(),
        );
        let mut tx = solana_transaction::Transaction::new_unsigned(message);
        tx.sign(&[&key], Default::default());
        let mut b = book();
        b.operator = key.pubkey().to_string();
        request_funding(&mut b, &path, &wallet.to_string(), 10);
        let locked = Mutex::new(b);
        update_funding(&locked, &path, &wallet.to_string(), |f| {
            f.transaction = Some(tx.clone())
        })
        .unwrap();
        let rpc = RpcClient::new_mock_with_mocks(
            "succeeds",
            [
                (RpcRequest::GetTransaction, Value::Null),
                (
                    RpcRequest::IsBlockhashValid,
                    json!({"context":{"slot":2},"value":false}),
                ),
            ]
            .into(),
        );
        tick_funding(&rpc, &key, &locked, &path, &wallet.to_string()).unwrap();
        let mut restored: Book = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(restored.funding[0].failed);
        assert_eq!(
            request_funding(&mut restored, &path, &wallet.to_string(), 90000)["error"],
            "funding_failed"
        );
        assert_eq!(
            restored.funding[0].transaction.as_ref().unwrap().signatures,
            tx.signatures
        );
        assert_eq!(
            request_funding(
                &mut restored,
                &path,
                &Keypair::new().pubkey().to_string(),
                90000
            )["ok"],
            true
        );
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    #[ignore = "requires a dedicated local validator, never a public cluster"]
    fn signed_zero_sol_wallet_funding_and_paid_memo() {
        let url = std::env::var("ALASHI_LOCAL_TEST_RPC").expect("dedicated local RPC required");
        let port = url
            .strip_prefix("http://127.0.0.1:")
            .expect("local RPC only");
        assert!(port.parse::<u16>().is_ok());
        let rpc = RpcClient::new_with_timeout_and_commitment(
            url,
            Duration::from_secs(10),
            CommitmentConfig::confirmed(),
        );
        let genesis = rpc.get_genesis_hash().unwrap().to_string();
        assert_ne!(genesis, "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG");
        assert_ne!(genesis, "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d");
        let host = Keypair::new();
        let player = Keypair::new();
        assert_eq!(rpc.get_balance(&player.pubkey()).unwrap(), 0);
        let sig = rpc.request_airdrop(&host.pubkey(), 2_000_000_000).unwrap();
        let started = std::time::Instant::now();
        while !rpc.confirm_transaction(&sig).unwrap() {
            assert!(started.elapsed() < Duration::from_secs(20));
            sleep(Duration::from_millis(100));
        }
        let path = funding_path();
        let mut b = book();
        b.operator = host.pubkey().to_string();
        let wallet = player.pubkey().to_string();
        request_funding(&mut b, &path, &wallet, now());
        let locked = Mutex::new(b);
        tick_funding(&rpc, &host, &locked, &path, &wallet).unwrap();
        // The process can die after broadcast. Restore only the saved signed bytes.
        let restored: Book = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let saved_signature = restored.funding[0].transaction.as_ref().unwrap().signatures[0];
        let locked = Mutex::new(restored);
        let started = std::time::Instant::now();
        while !locked.lock().unwrap().funding[0].ready {
            assert!(started.elapsed() < Duration::from_secs(20));
            tick_funding(&rpc, &host, &locked, &path, &wallet).unwrap();
            sleep(Duration::from_millis(100));
        }
        assert_eq!(rpc.get_balance(&player.pubkey()).unwrap(), FUNDING_TARGET);
        assert_eq!(
            request_funding(&mut locked.lock().unwrap(), &path, &wallet, now())["status"],
            "ready"
        );
        tick_funding(&rpc, &host, &locked, &path, &wallet).unwrap();
        assert_eq!(rpc.get_balance(&player.pubkey()).unwrap(), FUNDING_TARGET);
        let memo = Instruction::new_with_bytes(
            "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr"
                .parse()
                .unwrap(),
            b"alashi:sponsorship-local-test:paid-memo",
            vec![AccountMeta::new_readonly(player.pubkey(), true)],
        );
        let receipt = send_ix_confirmed(&rpc, &player, memo).unwrap();
        assert_eq!(receipt["status"], "confirmed");
        let after_memo = rpc.get_balance(&player.pubkey()).unwrap();
        assert!(after_memo >= FUNDING_MINIMUM && after_memo < FUNDING_TARGET);
        println!("verified local 0-SOL sponsorship: signature={saved_signature}, after_memo={after_memo}");
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn completed_entries_keep_their_recorded_amount_after_the_target_drops() {
        let key = Keypair::new();
        let entry = |amount: u64, ready: bool| {
            let wallet = Keypair::new().pubkey();
            let tx = VersionedTransaction::try_new(
                VersionedMessage::Legacy(Message::new_with_blockhash(
                    &[funding_transfer(key.pubkey(), wallet, amount)],
                    Some(&key.pubkey()),
                    &Default::default(),
                )),
                &[&key],
            )
            .unwrap();
            let VersionedMessage::Legacy(message) = tx.message else { panic!("legacy transfer") };
            Funding { wallet: wallet.to_string(), created_at: 1, ready, failed: false, error: None,
                transaction: Some(solana_transaction::Transaction { signatures: tx.signatures, message }) }
        };
        let with = |f: Funding| Book { version: 1, operator: key.pubkey().to_string(), funding: vec![f] };
        assert!(valid_funding(&with(entry(MAX_RECORDED_FUNDING, true))));
        assert!(!valid_funding(&with(entry(MAX_RECORDED_FUNDING, false))));
        assert!(valid_funding(&with(entry(FUNDING_TARGET, false))));
        assert!(!valid_funding(&with(entry(MAX_RECORDED_FUNDING + 1, true))));
    }

    fn book() -> Book {
        Book {
            version: 1,
            operator: Pubkey::new_unique().to_string(),
            funding: vec![],
        }
    }
}
