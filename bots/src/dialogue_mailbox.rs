//! Public-observation mailbox for an independent local harness. No signer inputs.
use crate::dialogue::{parse_decision, Decision, PublicContext};
use crate::dialogue_provider::{project_history, INSTRUCTIONS};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[derive(Clone, Debug)]
pub struct Mailbox {
    pub directory: PathBuf,
    pub public_strategy: String,
    pub timeout_ms: u64,
}
fn request_id_ok(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 96
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn unix_ms() -> Result<u64, &'static str> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "mailbox_clock_invalid")?
        .as_millis();
    u64::try_from(millis).map_err(|_| "mailbox_clock_invalid")
}
impl Mailbox {
    /// Syntax only: config checking neither opens files nor creates directories.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.directory.is_absolute()
            || self
                .directory
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            || self.public_strategy.trim().is_empty()
            || self.public_strategy.len() > 4096
            || self.public_strategy.chars().any(|c| c == '\0')
            || !(1000..=60000).contains(&self.timeout_ms)
        {
            return Err("invalid_mailbox_config");
        }
        Ok(())
    }
    pub fn decide_with_history(
        &self,
        request_id: &str,
        context: &PublicContext,
        history: &[Value],
        timeout_ms: u64,
    ) -> Result<Decision, &'static str> {
        self.validate()?;
        if !request_id_ok(request_id) || timeout_ms == 0 || timeout_ms > 60000 {
            return Err("invalid_mailbox_request");
        }
        let directory = private_directory(&self.directory)?;
        let public = context.to_public_json()?;
        let history = project_history(context, history)?;
        let input = json!({"public_strategy":self.public_strategy,"public_context":public,"public_history":history});
        let hash = format!("{:x}", Sha256::digest(input.to_string().as_bytes()));
        let duration = Duration::from_millis(timeout_ms.min(self.timeout_ms));
        let began = Instant::now();
        let deadline = unix_ms()?
            .checked_add(duration.as_millis() as u64)
            .ok_or("mailbox_clock_invalid")?;
        let request = json!({"schema":"alashi.local_decision.v1","request_id":request_id,"context_sha256":hash,"deadline_unix_ms":deadline,"instructions":INSTRUCTIONS,"public_strategy":self.public_strategy,"public_context":input["public_context"],"public_history":input["public_history"]});
        let req = directory.join(format!("{request_id}.request.json"));
        let response = directory.join(format!("{request_id}.response.json"));
        if fs::symlink_metadata(&response).is_ok() {
            return Err("mailbox_response_already_exists");
        }
        // Reserve the final filename atomically via hard link. A failed duplicate
        // never overwrites another observation. The temporary also remains audit evidence.
        let temporary = directory.join(format!(".{request_id}.request-writing"));
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| "mailbox_request_exists")?;
        if output
            .metadata()
            .map_err(|_| "mailbox_storage_failed")?
            .uid()
            != fs::metadata(&directory)
                .map_err(|_| "mailbox_directory_missing")?
                .uid()
        {
            return Err("mailbox_directory_wrong_owner");
        }
        let bytes = serde_json::to_vec(&request).map_err(|_| "mailbox_request_invalid")?;
        if bytes.len() > 65536 {
            return Err("mailbox_request_too_large");
        }
        output
            .write_all(&bytes)
            .and_then(|_| output.sync_all())
            .map_err(|_| "mailbox_storage_failed")?;
        fs::hard_link(&temporary, &req).map_err(|_| "mailbox_request_exists")?;
        fs::File::open(&directory)
            .and_then(|f| f.sync_all())
            .map_err(|_| "mailbox_storage_failed")?;
        loop {
            if began.elapsed() >= duration || unix_ms()? >= deadline {
                return Err("mailbox_timeout");
            }
            match read_response(&response) {
                Ok(raw) => {
                    if began.elapsed() >= duration || unix_ms()? >= deadline {
                        return Err("mailbox_timeout");
                    }
                    return decode_response(&raw, request_id, &hash, context);
                }
                Err("mailbox_response_missing") => {}
                Err(e) => return Err(e),
            }
            std::thread::sleep(
                Duration::from_millis(25).min(duration.saturating_sub(began.elapsed())),
            );
        }
    }
}
fn private_directory(path: &Path) -> Result<PathBuf, &'static str> {
    let meta = fs::symlink_metadata(path).map_err(|_| "mailbox_directory_missing")?;
    if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 {
        return Err("mailbox_directory_not_private");
    }
    let canonical = fs::canonicalize(path).map_err(|_| "mailbox_directory_missing")?;
    // Reject intermediate symlinks too; normal absolute paths are required.
    if canonical != path {
        return Err("mailbox_directory_not_canonical");
    }
    Ok(canonical)
}
fn read_response(path: &Path) -> Result<Vec<u8>, &'static str> {
    let before = fs::symlink_metadata(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            "mailbox_response_missing"
        } else {
            "mailbox_response_unavailable"
        }
    })?;
    if !before.is_file() || before.len() > 4096 || before.permissions().mode() & 0o077 != 0 {
        return Err("mailbox_response_invalid");
    }
    let mut file = fs::File::open(path).map_err(|_| "mailbox_response_unavailable")?;
    let opened = file
        .metadata()
        .map_err(|_| "mailbox_response_unavailable")?;
    if before.dev() != opened.dev()
        || before.ino() != opened.ino()
        || before.uid() != opened.uid()
        || opened.len() > 4096
    {
        return Err("mailbox_response_changed");
    }
    let parent = path
        .parent()
        .and_then(|p| fs::metadata(p).ok())
        .ok_or("mailbox_directory_missing")?;
    if opened.uid() != parent.uid() {
        return Err("mailbox_response_wrong_owner");
    }
    let mut raw = Vec::new();
    Read::by_ref(&mut file)
        .take(4097)
        .read_to_end(&mut raw)
        .map_err(|_| "mailbox_response_unavailable")?;
    let after = file
        .metadata()
        .map_err(|_| "mailbox_response_unavailable")?;
    if raw.len() > 4096
        || after.len() != opened.len()
        || after.mtime() != opened.mtime()
        || after.mtime_nsec() != opened.mtime_nsec()
    {
        return Err("mailbox_response_changed");
    }
    Ok(raw)
}
fn decode_response(
    raw: &[u8],
    id: &str,
    hash: &str,
    context: &PublicContext,
) -> Result<Decision, &'static str> {
    let v: Value = serde_json::from_slice(raw).map_err(|_| "mailbox_response_invalid")?;
    if !v.as_object().is_some_and(|o| {
        o.len() == 3
            && ["request_id", "context_sha256", "decision"]
                .iter()
                .all(|k| o.contains_key(*k))
    }) || v["request_id"] != id
        || v["context_sha256"] != hash
    {
        return Err("mailbox_response_binding_mismatch");
    }
    parse_decision(&v["decision"].to_string(), context, false)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dialogue::PublicFaction;
    fn context() -> PublicContext {
        let a = anchor_lang::prelude::Pubkey::new_from_array([1; 32]).to_string();
        let b = anchor_lang::prelude::Pubkey::new_from_array([2; 32]).to_string();
        PublicContext {
            game: anchor_lang::prelude::Pubkey::new_from_array([3; 32]).to_string(),
            faction: a.clone(),
            round: 1,
            phase: "Market".into(),
            phase_ends_at: 100,
            observed_cursor: 0,
            factions: vec![
                PublicFaction {
                    pda: a,
                    goods: 1,
                    cash: 2,
                },
                PublicFaction {
                    pda: b,
                    goods: 1,
                    cash: 2,
                },
            ],
            open_offers: vec![],
            incoming: vec![],
        }
    }
    fn temp() -> PathBuf {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let nonce = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let p = std::env::temp_dir().join(format!(
            "alashi-mailbox-{}-{}-{}",
            std::process::id(),
            unix_ms().unwrap(),
            nonce
        ));
        fs::create_dir(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        fs::canonicalize(p).unwrap()
    }
    #[test]
    fn binding_and_exact_decision() {
        let c = context();
        let mut v = json!({"request_id":"r1","context_sha256":"hash","decision":{"action":{"type":"pass"},"public_message":null}});
        assert!(decode_response(&serde_json::to_vec(&v).unwrap(), "r1", "hash", &c).is_ok());
        assert!(decode_response(&serde_json::to_vec(&v).unwrap(), "r2", "hash", &c).is_err());
        v["extra"] = json!(1);
        assert!(decode_response(&serde_json::to_vec(&v).unwrap(), "r1", "hash", &c).is_err());
    }
    #[test]
    fn timeout_duplicate_and_symlink_size() {
        let dir = temp();
        let m = Mailbox {
            directory: dir.clone(),
            public_strategy: "Choose independently".into(),
            timeout_ms: 1000,
        };
        assert_eq!(
            m.decide_with_history("r1", &context(), &[], 1).unwrap_err(),
            "mailbox_timeout"
        );
        assert_eq!(
            m.decide_with_history("r1", &context(), &[], 1).unwrap_err(),
            "mailbox_request_exists"
        );
        let path = dir.join("response");
        std::os::unix::fs::symlink(dir.join("r1.request.json"), &path).unwrap();
        assert!(read_response(&path).is_err());
        let large = dir.join("large");
        let mut f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&large)
            .unwrap();
        f.write_all(&vec![b'x'; 4097]).unwrap();
        assert!(read_response(&large).is_err());
        fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn atomic_file_protocol_preserves_public_reply_history() {
        // Supplied fixture tests transport; it is not live participant speech.
        let dir = temp();
        let c = context();
        let peer = c.factions[1].pda.clone();
        let first = json!({"message_id":"first","game_pda":c.game,"source":"runner_reported","author_faction_pda":c.faction,"to_faction_pda":peer,"text":"Public fixture question","reply_to_message_id":null,"private_extra":"must be stripped"});
        let second = json!({"message_id":"second","game_pda":c.game,"source":"runner_reported","author_faction_pda":peer,"to_faction_pda":c.faction,"text":"Public fixture reply","reply_to_message_id":"first"});
        let mailbox = Mailbox {
            directory: dir.clone(),
            public_strategy: "Independent fixture strategy".into(),
            timeout_ms: 2000,
        };
        let reader_context = c.clone();
        let thread = std::thread::spawn(move || {
            mailbox.decide_with_history("happy", &reader_context, &[first, second], 2000)
        });
        let request_path = dir.join("happy.request.json");
        let began = Instant::now();
        while !request_path.exists() {
            assert!(began.elapsed() < Duration::from_secs(1));
            std::thread::sleep(Duration::from_millis(5));
        }
        let request: Value = serde_json::from_slice(&fs::read(request_path).unwrap()).unwrap();
        assert_eq!(request["public_history"][0]["message_id"], "first");
        assert_eq!(request["public_history"][1]["reply_to_message_id"], "first");
        assert!(request["public_history"][0].get("private_extra").is_none());
        assert_eq!(request["public_context"]["faction"], c.faction);
        let response = json!({"request_id":request["request_id"],"context_sha256":request["context_sha256"],"decision":{"action":{"type":"pass"},"public_message":null}});
        let temporary = dir.join("test-response-writing");
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .unwrap();
        f.write_all(&serde_json::to_vec(&response).unwrap())
            .unwrap();
        f.sync_all().unwrap();
        fs::hard_link(temporary, dir.join("happy.response.json")).unwrap();
        let decision = thread.join().unwrap().unwrap();
        assert_eq!(decision.action, crate::dialogue::Action::Pass);
        assert!(decision.public_message.is_none());
        assert!(dir.join("happy.request.json").exists());
        assert!(dir.join("happy.response.json").exists());
        fs::remove_dir_all(dir).unwrap();
    }
}
