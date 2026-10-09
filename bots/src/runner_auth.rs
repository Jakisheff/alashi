//! Browser-pairing runner proof. Owner bearers and pairing grants stay in memory.
use crate::agent_cli::read_key;
use serde_json::{json, Value};
use solana_keypair::Keypair;
use solana_signer::Signer;
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Stdio},
};

pub(super) const ORIGIN: &str = "https://alashi.network";
const MAX_PROFILE: u64 = 64 * 1024;
const MAX_RESPONSE: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AgentProfile {
    pub(super) record_id: String,
    pub(super) wallet: String,
}
#[derive(Clone, Eq, PartialEq)]
pub(super) struct OwnerSession {
    pub(super) agent: AgentProfile,
    bearer: String,
}
#[derive(Clone, Eq, PartialEq)]
pub(super) struct Binding {
    pub(super) runner_token: String,
    pub(super) game_pda: String,
    pub(super) faction_pda: String,
    pub(super) faction_wallet: String,
    pub(super) record_id: String,
}

fn string(value: &Value, key: &str) -> Option<String> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 512)
        .map(str::to_owned)
}
fn hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

pub(super) fn profile(path: &Path, key: &Keypair) -> Result<AgentProfile, &'static str> {
    let meta = fs::symlink_metadata(path).map_err(|_| "owner_profile_unavailable")?;
    if !meta.is_file() || meta.len() > MAX_PROFILE || meta.permissions().mode() & 0o077 != 0 {
        return Err("owner_profile_invalid");
    }
    let value: Value =
        serde_json::from_slice(&fs::read(path).map_err(|_| "owner_profile_unavailable")?)
            .map_err(|_| "owner_profile_invalid")?;
    let record_id = string(&value, "agent_record_id")
        .filter(|v| hex64(v))
        .ok_or("owner_profile_invalid")?;
    let wallet = string(&value["config"], "wallet").ok_or("owner_profile_invalid")?;
    if value["schema"] != "alashi.agent.v2"
        || value["config"]["url"] != ORIGIN
        || value["registration"].is_null()
        || wallet != key.pubkey().to_string()
    {
        return Err("owner_profile_unregistered");
    }
    Ok(AgentProfile { record_id, wallet })
}

// curl reads this private config from stdin; the bearer is never argv, stdout, or an error string.
fn post(path: &str, bearer: Option<&str>, body: &Value) -> Result<Value, &'static str> {
    let data = serde_json::to_string(body).map_err(|_| "owner_request_invalid")?;
    if data.contains(['\n', '\r']) {
        return Err("owner_request_invalid");
    }
    let mut config = format!("url = \"{ORIGIN}{path}\"\nrequest = \"POST\"\nheader = \"Origin: {ORIGIN}\"\nheader = \"Content-Type: application/json\"\ndata-binary = {}\nwrite-out = \"\\n%{{http_code}}\"\nsilent\nshow-error\nfail-with-body\nmax-time = 30\nconnect-timeout = 5\nmax-filesize = {MAX_RESPONSE}\nproto = \"=https\"\n", serde_json::to_string(&data).map_err(|_| "owner_request_invalid")?);
    if let Some(token) = bearer {
        if token.bytes().any(|b| b <= 32 || b == 127 || b == b'\"') {
            return Err("owner_session_invalid");
        }
        config.push_str(&format!("header = \"Authorization: Bearer {token}\"\n"));
    }
    let mut child = Command::new("curl")
        .args(["--disable", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "owner_transport")?;
    if child
        .stdin
        .take()
        .is_none_or(|mut input| input.write_all(config.as_bytes()).is_err())
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err("owner_transport");
    }
    let out = child.wait_with_output().map_err(|_| "owner_transport")?;
    if !out.status.success() || out.stdout.len() > MAX_RESPONSE {
        return Err("owner_transport");
    }
    let output = String::from_utf8(out.stdout).map_err(|_| "owner_transport")?;
    let (body, code) = output.rsplit_once('\n').ok_or("owner_transport")?;
    if !(200..300).contains(&code.parse::<u16>().map_err(|_| "owner_transport")?) {
        return Err("owner_rejected");
    }
    serde_json::from_str(body).map_err(|_| "owner_transport")
}

pub(super) fn authenticate_owner(
    agent_file: &Path,
    key_path: &str,
) -> Result<OwnerSession, &'static str> {
    let key = read_key(key_path).map_err(|_| "owner_key_invalid")?;
    let agent = profile(agent_file, &key)?;
    let challenge = post(
        &format!("/agents/{}/owner/challenge", agent.record_id),
        None,
        &json!({}),
    )?;
    let challenge_id = string(&challenge, "challenge_id").ok_or("owner_challenge_rejected")?;
    let message = string(&challenge, "message").ok_or("owner_challenge_rejected")?;
    if string(&challenge, "wallet").as_deref() != Some(agent.wallet.as_str()) {
        return Err("owner_challenge_rejected");
    }
    let signature = key.sign_message(message.as_bytes()).to_string();
    let session = post(
        &format!("/agents/{}/owner/session", agent.record_id),
        None,
        &json!({"challenge_id":challenge_id,"signature":signature}),
    )?;
    let bearer = string(&session, "owner_session").ok_or("owner_session_rejected")?;
    Ok(OwnerSession { agent, bearer })
}

pub(super) fn bind_wallet(
    owner: &OwnerSession,
    game_pda: &str,
    faction_pda: &str,
) -> Result<Binding, &'static str> {
    let result = post(
        &format!("/chain/devnet/games/{game_pda}/runner/bind-wallet"),
        Some(&owner.bearer),
        &json!({"agent_record_id":owner.agent.record_id,"faction_pda":faction_pda}),
    )?;
    let binding = Binding {
        runner_token: string(&result, "runner_token").ok_or("runner_bind_rejected")?,
        game_pda: string(&result, "game_pda").ok_or("runner_bind_rejected")?,
        faction_pda: string(&result, "faction_pda").ok_or("runner_bind_rejected")?,
        faction_wallet: string(&result, "faction_wallet").ok_or("runner_bind_rejected")?,
        record_id: string(&result, "record_id").ok_or("runner_bind_rejected")?,
    };
    if result["ok"] != true
        || binding.game_pda != game_pda
        || binding.faction_pda != faction_pda
        || binding.faction_wallet != owner.agent.wallet
        || binding.record_id != owner.agent.record_id
    {
        return Err("runner_bind_rejected");
    }
    Ok(binding)
}

pub(super) fn authenticate_and_bind(
    agent_file: &Path,
    key_path: &str,
    game_pda: &str,
    faction_pda: &str,
) -> Result<(AgentProfile, Binding), &'static str> {
    let owner = authenticate_owner(agent_file, key_path)?;
    let binding = bind_wallet(&owner, game_pda, faction_pda)?;
    Ok((owner.agent, binding))
}

fn complete_pairing_for(
    owner: &OwnerSession,
    pairing_grant: &str,
    game_pda: &str,
    faction_pda: &str,
) -> Result<String, &'static str> {
    if pairing_grant.is_empty()
        || pairing_grant.len() > 512
        || pairing_grant.bytes().any(|b| b <= 32 || b == 127)
    {
        return Err("pairing_grant_invalid");
    }
    let response = post(
        &format!("/agents/{}/owner/pairing/complete", owner.agent.record_id),
        Some(&owner.bearer),
        &json!({"pairing_grant":pairing_grant,"game_pda":game_pda,"faction_pda":faction_pda}),
    )?;
    let url = string(&response, "owner_url").ok_or("pairing_complete_rejected")?;
    if response["ok"] != true
        || response["agent_record_id"] != owner.agent.record_id
        || response["game_pda"] != game_pda
        || response["faction_pda"] != faction_pda
        || !url.starts_with("https://alashi.network/devnet?")
    {
        return Err("pairing_complete_rejected");
    }
    Ok(url)
}

pub(super) fn complete_pairing(
    owner: &OwnerSession,
    binding: &Binding,
    pairing_grant: &str,
) -> Result<String, &'static str> {
    complete_pairing_for(
        owner,
        pairing_grant,
        &binding.game_pda,
        &binding.faction_pda,
    )
}

pub(super) fn complete_existing_pairing(
    owner: &OwnerSession,
    pairing_grant: &str,
    game_pda: &str,
    faction_pda: &str,
) -> Result<String, &'static str> {
    complete_pairing_for(owner, pairing_grant, game_pda, faction_pda)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::OpenOptions, os::unix::fs::OpenOptionsExt};
    #[test]
    fn profile_rejects_unregistered_or_wrong_wallet() {
        let p = std::env::temp_dir().join(format!("runner-auth-{}", std::process::id()));
        let key = Keypair::new();
        let v = json!({"schema":"alashi.agent.v2","config":{"url":ORIGIN,"wallet":key.pubkey().to_string()},"agent_record_id":"a".repeat(64),"registration":null});
        let mut f = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&p)
            .unwrap();
        f.write_all(v.to_string().as_bytes()).unwrap();
        assert_eq!(profile(&p, &key), Err("owner_profile_unregistered"));
        fs::remove_file(p).unwrap();
    }
}
