//! Loopback-only private wish adapter for the devnet bot runner.
//! Secrets stay in memory and are never included in errors or stdout.

use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const MAX_PROFILE_BYTES: u64 = 64 * 1024;
const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

#[derive(Clone)]
pub(super) struct Profile {
    pub(super) wallet: String,
    agent_record_id: String,
    recovery_secret: String,
}

pub(super) struct Profiles {
    base: String,
    profiles: Vec<Profile>,
    profile_path: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Intent {
    Produce,
    SellOne,
    BuyOne,
    VoteYes,
    VoteNo,
    Unsupported,
}

#[derive(Clone)]
pub(super) struct Wish {
    pub(super) wish_id: String,
    lease_id: String,
    pub(super) intent: Intent,
}

pub(super) struct ChainWishClient {
    base: String,
    game_pda: String,
    faction_pda: String,
    runner_token: String,
    pending_store: PendingStore,
    pending: Option<PendingReceipt>,
}

#[derive(Clone)]
struct PendingReceipt {
    wish: Wish,
    game_pda: String,
    faction_pda: String,
    signature: String,
    slot: Option<u64>,
}

#[derive(Clone)]
struct PendingStore {
    path: PathBuf,
}

pub(super) enum Confirmed {
    Accepted,
    ReceiptPending,
}

impl Intent {
    fn parse(value: &str) -> Self {
        match value {
            "produce" => Self::Produce,
            "sell_one" => Self::SellOne,
            "buy_one" => Self::BuyOne,
            "vote_yes" => Self::VoteYes,
            "vote_no" => Self::VoteNo,
            _ => Self::Unsupported,
        }
    }

    pub(super) fn matches_phase(self, phase: &str) -> bool {
        matches!(
            (self, phase),
            (Self::Produce, "action")
                | (Self::SellOne | Self::BuyOne, "market")
                | (Self::VoteYes | Self::VoteNo, "law")
        )
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Produce => "produce",
            Self::SellOne => "sell_one",
            Self::BuyOne => "buy_one",
            Self::VoteYes => "vote_yes",
            Self::VoteNo => "vote_no",
            Self::Unsupported => "unsupported",
        }
    }
}

impl Profiles {
    /// Returns None unless the explicit opt-in URL is set.
    pub(super) fn from_env() -> Result<Option<Self>, &'static str> {
        let Ok(base) = std::env::var("ALASHI_CHAIN_WISH_API") else {
            return Ok(None);
        };
        let base = validate_api_base(&base)?;
        let path = std::env::var("ALASHI_CHAIN_WISH_PROFILE_FILE")
            .map_err(|_| "chain_wish_profile_required")?;
        let profile_path = PathBuf::from(path);
        let profiles = read_profiles(&profile_path)?;
        Ok(Some(Self {
            base,
            profiles,
            profile_path,
        }))
    }

    pub(super) fn matching_profile(&self, wallet: &str) -> Result<Option<Profile>, &'static str> {
        let mut found = self
            .profiles
            .iter()
            .filter(|profile| profile.wallet == wallet);
        let first = found.next().cloned();
        if found.next().is_some() {
            return Err("chain_wish_duplicate_profile");
        }
        Ok(first)
    }

    pub(super) fn bind(
        &self,
        profile: Profile,
        game_pda: &str,
        faction_pda: &str,
    ) -> Result<ChainWishClient, &'static str> {
        let pending_store =
            PendingStore::for_profile(&self.profile_path, &profile.agent_record_id)?;
        let body = json!({
            "agent_record_id": profile.agent_record_id,
            "recovery_secret": profile.recovery_secret,
            "faction_pda": faction_pda,
        });
        let response = post(
            &self.base,
            &format!("/chain/devnet/games/{game_pda}/runner/bind"),
            &body,
        )?;
        if response["ok"] != true
            || !same(response["game_pda"].as_str(), game_pda)
            || !same(response["faction_pda"].as_str(), faction_pda)
            || !same(response["faction_wallet"].as_str(), &profile.wallet)
            || !same(response["record_id"].as_str(), &profile.agent_record_id)
        {
            return Err("chain_wish_bind_rejected");
        }
        let runner_token =
            bounded_string(&response, "runner_token").ok_or("chain_wish_bind_rejected")?;
        let pending = pending_store.load(game_pda, faction_pda)?;
        Ok(ChainWishClient {
            base: self.base.clone(),
            game_pda: game_pda.to_string(),
            faction_pda: faction_pda.to_string(),
            runner_token,
            pending_store,
            pending,
        })
    }
}

impl ChainWishClient {
    pub(super) fn bind_public(
        base: &str,
        profile_path: &Path,
        binding: crate::runner_auth::Binding,
    ) -> Result<Self, &'static str> {
        let base = validate_api_base(base)?;
        if binding.runner_token.is_empty()
            || binding.runner_token.len() > 512
            || !is_hex64(&binding.record_id)
        {
            return Err("chain_wish_bind_rejected");
        }
        let pending_store = PendingStore::for_profile(profile_path, &binding.record_id)?;
        let pending = pending_store.load(&binding.game_pda, &binding.faction_pda)?;
        Ok(Self {
            base,
            game_pda: binding.game_pda,
            faction_pda: binding.faction_pda,
            runner_token: binding.runner_token,
            pending_store,
            pending,
        })
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) fn pending_signature_without_slot(&self) -> Option<&str> {
        self.pending
            .as_ref()
            .filter(|pending| pending.slot.is_none())
            .map(|pending| pending.signature.as_str())
    }

    pub(super) fn remember_pending(
        &mut self,
        wish: &Wish,
        signature: &str,
        slot: Option<u64>,
    ) -> Result<(), &'static str> {
        let pending = PendingReceipt {
            wish: wish.clone(),
            game_pda: self.game_pda.clone(),
            faction_pda: self.faction_pda.clone(),
            signature: signature.to_string(),
            slot,
        };
        self.pending = Some(pending.clone());
        self.pending_store.save(&pending)
    }

    pub(super) fn record_pending_slot(&mut self, slot: u64) -> Result<(), &'static str> {
        let Some(pending) = self.pending.as_mut() else {
            return Err("chain_wish_pending_missing");
        };
        pending.slot = Some(slot);
        self.pending_store.save(pending)
    }

    pub(super) fn mark_pending_unconfirmed(&mut self) -> Result<(), &'static str> {
        let Some(pending) = self.pending.clone() else {
            return Err("chain_wish_pending_missing");
        };
        self.unconfirmed(&pending.wish)?;
        self.pending_store.clear()?;
        self.pending = None;
        Ok(())
    }

    pub(super) fn retry_pending(&mut self) -> Result<Confirmed, &'static str> {
        let Some(pending) = self.pending.clone() else {
            return Ok(Confirmed::Accepted);
        };
        let Some(slot) = pending.slot else {
            return Ok(Confirmed::ReceiptPending);
        };
        match self.confirmed(&pending.wish, &pending.signature, slot) {
            Ok(Confirmed::Accepted) => {
                self.pending_store.clear()?;
                self.pending = None;
                Ok(Confirmed::Accepted)
            }
            Ok(Confirmed::ReceiptPending) | Err(_) => Ok(Confirmed::ReceiptPending),
        }
    }

    pub(super) fn claim(&self) -> Result<Option<Wish>, &'static str> {
        let body = json!({"runner_token": self.runner_token, "after": 0, "limit": 1});
        let response = post(
            &self.base,
            &format!("/chain/devnet/games/{}/runner/wishes/claim", self.game_pda),
            &body,
        )?;
        if response["ok"] != true {
            return Err("chain_wish_claim_rejected");
        }
        let Some(item) = response["wishes"]
            .as_array()
            .and_then(|items| items.first())
        else {
            return Ok(None);
        };
        let wish_id = bounded_string(item, "wish_id").ok_or("chain_wish_claim_rejected")?;
        let lease_id = bounded_string(item, "lease_id").ok_or("chain_wish_claim_rejected")?;
        let intent = item["intent"]
            .as_str()
            .map(Intent::parse)
            .unwrap_or(Intent::Unsupported);
        // `text` is deliberately not copied from this private response.
        Ok(Some(Wish {
            wish_id,
            lease_id,
            intent,
        }))
    }

    pub(super) fn defer(&self, wish: &Wish) -> Result<(), &'static str> {
        self.status(wish, "deferred", None).map(|_| ())
    }

    pub(super) fn decline(&self, wish: &Wish) -> Result<(), &'static str> {
        self.status(wish, "declined", None).map(|_| ())
    }

    pub(super) fn consume(&self, wish: &Wish) -> Result<u64, &'static str> {
        self.status(wish, "consumed", None)?["consumed_after_slot"]
            .as_u64()
            .ok_or("chain_wish_status_rejected")
    }

    pub(super) fn unconfirmed(&self, wish: &Wish) -> Result<(), &'static str> {
        self.status(wish, "unconfirmed", None).map(|_| ())
    }

    pub(super) fn confirmed(
        &self,
        wish: &Wish,
        signature: &str,
        slot: u64,
    ) -> Result<Confirmed, &'static str> {
        let response = self.status(wish, "confirmed", Some((signature, slot)))?;
        if response["ok"] == true {
            Ok(Confirmed::Accepted)
        } else if error_code(&response) == Some("receipt_pending") {
            Ok(Confirmed::ReceiptPending)
        } else {
            Err("chain_wish_status_rejected")
        }
    }

    fn status(
        &self,
        wish: &Wish,
        status: &str,
        receipt: Option<(&str, u64)>,
    ) -> Result<Value, &'static str> {
        let mut body = json!({
            "runner_token": self.runner_token,
            "lease_id": wish.lease_id,
            "status": status,
        });
        if let Some((signature, slot)) = receipt {
            body["signature"] = json!(signature);
            body["slot"] = json!(slot);
        }
        let response = post(
            &self.base,
            &format!(
                "/chain/devnet/games/{}/runner/wishes/{}/status",
                self.game_pda, wish.wish_id
            ),
            &body,
        )?;
        if response["ok"] == true || error_code(&response) == Some("receipt_pending") {
            Ok(response)
        } else {
            Err("chain_wish_status_rejected")
        }
    }
}

impl PendingStore {
    fn for_profile(profile_path: &Path, record_id: &str) -> Result<Self, &'static str> {
        let parent = profile_path.parent().ok_or("chain_wish_profile_invalid")?;
        Ok(Self {
            path: parent.join(format!(".alashi-chain-wish-{record_id}.pending")),
        })
    }

    fn load(
        &self,
        game_pda: &str,
        faction_pda: &str,
    ) -> Result<Option<PendingReceipt>, &'static str> {
        let Ok(meta) = fs::symlink_metadata(&self.path) else {
            return Ok(None);
        };
        if !meta.is_file()
            || meta.len() > MAX_PROFILE_BYTES
            || meta.permissions().mode() & 0o077 != 0
        {
            return Err("chain_wish_pending_invalid");
        }
        let value: Value = serde_json::from_slice(
            &fs::read(&self.path).map_err(|_| "chain_wish_pending_unavailable")?,
        )
        .map_err(|_| "chain_wish_pending_invalid")?;
        let pending = PendingReceipt {
            wish: Wish {
                wish_id: bounded_string(&value, "wish_id").ok_or("chain_wish_pending_invalid")?,
                lease_id: bounded_string(&value, "lease_id").ok_or("chain_wish_pending_invalid")?,
                intent: value["intent"]
                    .as_str()
                    .map(Intent::parse)
                    .filter(|intent| *intent != Intent::Unsupported)
                    .ok_or("chain_wish_pending_invalid")?,
            },
            game_pda: bounded_string(&value, "game_pda").ok_or("chain_wish_pending_invalid")?,
            faction_pda: bounded_string(&value, "faction_pda")
                .ok_or("chain_wish_pending_invalid")?,
            signature: bounded_string(&value, "signature").ok_or("chain_wish_pending_invalid")?,
            slot: match value.get("slot") {
                Some(Value::Null) | None => None,
                Some(value) => Some(value.as_u64().ok_or("chain_wish_pending_invalid")?),
            },
        };
        if pending.game_pda != game_pda || pending.faction_pda != faction_pda {
            return Err("chain_wish_pending_scope_mismatch");
        }
        Ok(Some(pending))
    }

    fn save(&self, pending: &PendingReceipt) -> Result<(), &'static str> {
        if let Ok(meta) = fs::symlink_metadata(&self.path) {
            if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
                return Err("chain_wish_pending_invalid");
            }
        }
        let encoded = serde_json::to_vec(&json!({
            "wish_id": pending.wish.wish_id,
            "lease_id": pending.wish.lease_id,
            "intent": pending.wish.intent.as_str(),
            "game_pda": pending.game_pda,
            "faction_pda": pending.faction_pda,
            "signature": pending.signature,
            "slot": pending.slot,
        }))
        .map_err(|_| "chain_wish_pending_unavailable")?;
        let temporary = self
            .path
            .with_extension(format!("pending-{}", std::process::id()));
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| "chain_wish_pending_unavailable")?;
        if file.write_all(&encoded).is_err() || file.sync_all().is_err() {
            let _ = fs::remove_file(&temporary);
            return Err("chain_wish_pending_unavailable");
        }
        fs::rename(&temporary, &self.path).map_err(|_| "chain_wish_pending_unavailable")
    }

    fn clear(&self) -> Result<(), &'static str> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("chain_wish_pending_unavailable"),
        }
    }
}

fn same(actual: Option<&str>, expected: &str) -> bool {
    actual == Some(expected)
}

fn error_code(value: &Value) -> Option<&str> {
    value["error"]
        .as_str()
        .or_else(|| value["error"]["code"].as_str())
}

fn bounded_string(value: &Value, field: &str) -> Option<String> {
    value[field]
        .as_str()
        .filter(|value| !value.is_empty() && value.len() <= 512)
        .map(str::to_owned)
}

fn is_hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn read_profiles(path: &Path) -> Result<Vec<Profile>, &'static str> {
    let meta = fs::symlink_metadata(path).map_err(|_| "chain_wish_profile_unavailable")?;
    if !meta.is_file() || meta.len() > MAX_PROFILE_BYTES || meta.permissions().mode() & 0o077 != 0 {
        return Err("chain_wish_profile_invalid");
    }
    let value: Value =
        serde_json::from_slice(&fs::read(path).map_err(|_| "chain_wish_profile_unavailable")?)
            .map_err(|_| "chain_wish_profile_invalid")?;
    let profiles = value["profiles"]
        .as_array()
        .ok_or("chain_wish_profile_invalid")?;
    if profiles.is_empty() || profiles.len() > 8 {
        return Err("chain_wish_profile_invalid");
    }
    let mut result = Vec::with_capacity(profiles.len());
    for item in profiles {
        let wallet = bounded_string(item, "wallet").ok_or("chain_wish_profile_invalid")?;
        let agent_record_id =
            bounded_string(item, "agent_record_id").ok_or("chain_wish_profile_invalid")?;
        let recovery_secret =
            bounded_string(item, "recovery_secret").ok_or("chain_wish_profile_invalid")?;
        if wallet.len() > 64 || !is_hex64(&agent_record_id) || !is_hex64(&recovery_secret) {
            return Err("chain_wish_profile_invalid");
        }
        result.push(Profile {
            wallet,
            agent_record_id,
            recovery_secret,
        });
    }
    Ok(result)
}

fn validate_api_base(base: &str) -> Result<String, &'static str> {
    if base == "https://alashi.network" {
        return Ok(base.to_string());
    }
    if base.bytes().any(|byte| byte <= 32 || byte == 127)
        || base.contains(['?', '#', '@', '\\'])
        || !base.starts_with("http://127.0.0.1:")
    {
        return Err("chain_wish_api_invalid");
    }
    let port = base.trim_start_matches("http://127.0.0.1:");
    if port.is_empty()
        || !port.bytes().all(|byte| byte.is_ascii_digit())
        || port.parse::<u16>().ok().filter(|port| *port > 0).is_none()
    {
        return Err("chain_wish_api_invalid");
    }
    Ok(base.to_string())
}

fn post(base: &str, path: &str, body: &Value) -> Result<Value, &'static str> {
    let payload = serde_json::to_vec(body).map_err(|_| "chain_wish_request_invalid")?;
    let mut command = Command::new("curl");
    command.args([
        "--disable",
        "--silent",
        "--show-error",
        "--connect-timeout",
        "2",
        "--max-time",
        "30",
        "--max-filesize",
        "65536",
        "--proto",
        if base == "https://alashi.network" {
            "=https"
        } else {
            "=http"
        },
        "--request",
        "POST",
        "--header",
        "Content-Type: application/json",
        "--data-binary",
        "@-",
        "--write-out",
        "\n%{http_code}",
        "--url",
        &format!("{base}{path}"),
    ]);
    if base == "https://alashi.network" {
        command.args(["--header", "Origin: https://alashi.network"]);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "chain_wish_transport")?;
    if child
        .stdin
        .take()
        .is_none_or(|mut input| input.write_all(&payload).is_err())
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err("chain_wish_transport");
    }
    let mut bytes = Vec::new();
    if child
        .stdout
        .take()
        .unwrap()
        .take(MAX_RESPONSE_BYTES + 1)
        .read_to_end(&mut bytes)
        .is_err()
        || bytes.len() as u64 > MAX_RESPONSE_BYTES
        || !child.wait().map_err(|_| "chain_wish_transport")?.success()
    {
        return Err("chain_wish_transport");
    }
    let output = String::from_utf8(bytes).map_err(|_| "chain_wish_transport")?;
    let (json, code) = output.rsplit_once('\n').ok_or("chain_wish_transport")?;
    let code = code.parse::<u16>().map_err(|_| "chain_wish_transport")?;
    let response: Value = serde_json::from_str(json).map_err(|_| "chain_wish_transport")?;
    if (200..300).contains(&code) || response.is_object() {
        Ok(response)
    } else {
        Err("chain_wish_rejected")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs::OpenOptions,
        io::{Read, Write},
        net::TcpListener,
        os::unix::fs::OpenOptionsExt,
        path::PathBuf,
        thread,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn profile_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "alashi-chain-wish-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn intent_only_matches_its_phase() {
        assert!(Intent::SellOne.matches_phase("market"));
        assert!(Intent::BuyOne.matches_phase("market"));
        assert!(Intent::Produce.matches_phase("action"));
        assert!(Intent::VoteNo.matches_phase("law"));
        assert!(!Intent::VoteYes.matches_phase("market"));
        assert!(!Intent::Unsupported.matches_phase("law"));
    }

    #[test]
    fn rejects_non_loopback_api_bases() {
        for value in [
            "https://evil.example",
            "http://localhost:8095",
            "http://127.0.0.1:0",
            "http://127.0.0.1:8095/path",
        ] {
            assert!(validate_api_base(value).is_err(), "{value}");
        }
        assert_eq!(
            validate_api_base("http://127.0.0.1:8095").unwrap(),
            "http://127.0.0.1:8095"
        );
        assert_eq!(
            validate_api_base("https://alashi.network").unwrap(),
            "https://alashi.network"
        );
    }

    #[test]
    fn profile_requires_owner_only_permissions_and_hex_credentials() {
        let path = profile_path();
        let body = json!({"profiles":[{"wallet":"wallet","agent_record_id":"a".repeat(64),"recovery_secret":"b".repeat(64)}]}).to_string();
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        file.write_all(body.as_bytes()).unwrap();
        file.sync_all().unwrap();
        assert_eq!(read_profiles(&path).unwrap().len(), 1);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        assert!(matches!(
            read_profiles(&path),
            Err("chain_wish_profile_invalid")
        ));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn bind_rejects_a_response_for_another_faction() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut head = Vec::new();
            let mut byte = [0];
            while !head.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                head.push(byte[0]);
            }
            let length = String::from_utf8(head)
                .unwrap()
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Content-Length: ")
                        .and_then(|n| n.parse::<usize>().ok())
                })
                .unwrap();
            let mut body = vec![0; length];
            stream.read_exact(&mut body).unwrap();
            let body = json!({"ok":true,"runner_token":"token","game_pda":"game","faction_pda":"other","faction_wallet":"wallet","record_id":"a".repeat(64)}).to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        let profile = Profile {
            wallet: "wallet".into(),
            agent_record_id: "a".repeat(64),
            recovery_secret: "b".repeat(64),
        };
        let profile_path = profile_path();
        let profiles = Profiles {
            base,
            profiles: vec![profile.clone()],
            profile_path,
        };
        assert!(matches!(
            profiles.bind(profile, "game", "faction"),
            Err("chain_wish_bind_rejected")
        ));
        server.join().unwrap();
    }

    #[test]
    fn bind_claim_and_confirm_keep_private_text_out_of_runner_state() {
        let record = "c".repeat(64);
        let expected_record = record.clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let output = seen.clone();
        let server = thread::spawn(move || {
            for response in [
                json!({"ok":true,"runner_token":"runner-token","game_pda":"game","faction_pda":"faction","faction_wallet":"wallet","record_id":expected_record}),
                json!({"ok":true,"wishes":[{"wish_id":"wish","lease_id":"lease","intent":"sell_one","text":"PRIVATE-MARKER"}]}),
                json!({"ok":true,"consumed_after_slot":41}),
                json!({"ok":false,"error":"receipt_pending"}),
                json!({"ok":true}),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut head = Vec::new();
                let mut byte = [0];
                while !head.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    head.push(byte[0]);
                }
                let header = String::from_utf8(head).unwrap();
                let length = header
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length: ")
                            .and_then(|n| n.parse::<usize>().ok())
                    })
                    .unwrap();
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                output
                    .lock()
                    .unwrap()
                    .push((header, String::from_utf8(body).unwrap()));
                let body = response.to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        let profile = Profile {
            wallet: "wallet".into(),
            agent_record_id: record.clone(),
            recovery_secret: "b".repeat(64),
        };
        let profile_path = profile_path();
        let profiles = Profiles {
            base,
            profiles: vec![profile.clone()],
            profile_path: profile_path.clone(),
        };
        let client = profiles.bind(profile, "game", "faction").unwrap();
        let wish = client.claim().unwrap().unwrap();
        assert_eq!(wish.intent, Intent::SellOne);
        assert_eq!(client.consume(&wish).unwrap(), 41);
        let mut client = client;
        client.remember_pending(&wish, "sig", Some(42)).unwrap();
        assert!(matches!(
            client.retry_pending().unwrap(),
            Confirmed::ReceiptPending
        ));
        assert!(matches!(
            client.retry_pending().unwrap(),
            Confirmed::Accepted
        ));
        server.join().unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 5);
        assert!(seen
            .iter()
            .all(|(_, body)| !body.contains("PRIVATE-MARKER")));
        assert!(seen[4].1.contains("\"signature\":\"sig\""));
        assert!(seen[4].1.contains("\"slot\":42"));
        let _ = fs::remove_file(
            PendingStore::for_profile(&profile_path, &record)
                .unwrap()
                .path,
        );
    }

    #[test]
    fn pending_receipt_survives_restart_and_transient_status() {
        let record = "d".repeat(64);
        let expected_record = record.clone();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let output = seen.clone();
        let server = thread::spawn(move || {
            for (code, response) in [
                (
                    200,
                    json!({"ok":true,"runner_token":"first-token","game_pda":"game","faction_pda":"faction","faction_wallet":"wallet","record_id":expected_record}),
                ),
                (
                    200,
                    json!({"ok":true,"runner_token":"second-token","game_pda":"game","faction_pda":"faction","faction_wallet":"wallet","record_id":record}),
                ),
                (503, json!({"ok":false,"error":"storage_failed"})),
                (200, json!({"ok":true})),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut head = Vec::new();
                let mut byte = [0];
                while !head.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    head.push(byte[0]);
                }
                let header = String::from_utf8(head).unwrap();
                let length = header
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length: ")
                            .and_then(|n| n.parse::<usize>().ok())
                    })
                    .unwrap();
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                output
                    .lock()
                    .unwrap()
                    .push(String::from_utf8(body).unwrap());
                let body = response.to_string();
                write!(
                    stream,
                    "HTTP/1.1 {code} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        let profile_path = profile_path();
        let profile = Profile {
            wallet: "wallet".into(),
            agent_record_id: "d".repeat(64),
            recovery_secret: "b".repeat(64),
        };
        let profiles = Profiles {
            base,
            profiles: vec![profile.clone()],
            profile_path: profile_path.clone(),
        };
        let wish = Wish {
            wish_id: "wish".into(),
            lease_id: "lease".into(),
            intent: Intent::SellOne,
        };
        let mut first = profiles.bind(profile.clone(), "game", "faction").unwrap();
        first
            .remember_pending(&wish, "confirmed-signature", None)
            .unwrap();
        let state = PendingStore::for_profile(&profile_path, &profile.agent_record_id).unwrap();
        assert_eq!(
            fs::metadata(&state.path).unwrap().permissions().mode() & 0o077,
            0
        );
        let saved = fs::read_to_string(&state.path).unwrap();
        assert!(saved.contains("confirmed-signature"));
        assert!(!saved.contains(&"b".repeat(64)));
        drop(first);

        let mut resumed = profiles.bind(profile, "game", "faction").unwrap();
        assert!(resumed.has_pending());
        assert!(matches!(
            resumed.retry_pending().unwrap(),
            Confirmed::ReceiptPending
        ));
        assert!(resumed.has_pending());
        // The unsigned slot blocks a status post; only the exact receipt resolver
        // in main.rs may supply it after a restart.
        assert_eq!(seen.lock().unwrap().len(), 2);
        resumed.record_pending_slot(42).unwrap();
        assert!(matches!(
            resumed.retry_pending().unwrap(),
            Confirmed::ReceiptPending
        ));
        assert!(resumed.has_pending());
        assert!(matches!(
            resumed.retry_pending().unwrap(),
            Confirmed::Accepted
        ));
        assert!(!resumed.has_pending());
        assert!(!state.path.exists());
        server.join().unwrap();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 4);
        assert!(seen[2].contains("confirmed-signature") && seen[3].contains("confirmed-signature"));
    }
}
