//! One signed devnet Memo on connection; all game decisions use the HTTP arena.
use super::*;
use crate::agent_cli::{devnet, read_key};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    process::{Command, Stdio},
};

const MEMO_ID: &str = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr";
const MAX_RESPONSE: u64 = 1_048_576;
const USAGE: &str = "bots session connect --url URL --game ID --key LOCAL_KEYPAIR --name NAME --model MODEL --prompt-file FILE --session-file PRIVATE | inspect --session-file PRIVATE | act --session-file PRIVATE (JSON on stdin)";
type ResultJson = Result<Value, Value>;
fn error(code: &str, message: &str) -> Value {
    json!({"ok":false,"error":{"code":code,"message":message}})
}
fn text<'a>(value: &'a Value, name: &str) -> Result<&'a str, Value> {
    value[name]
        .as_str()
        .ok_or_else(|| error("invalid_session", "missing or invalid session field"))
}
fn parse_options(args: &[String]) -> Result<BTreeMap<String, String>, Value> {
    let allowed = match args.first().map(String::as_str) {
        Some("connect") => &[
            "--url",
            "--game",
            "--key",
            "--name",
            "--model",
            "--prompt-file",
            "--session-file",
        ][..],
        Some("inspect" | "act") => &["--session-file"][..],
        _ => return Err(error("usage", USAGE)),
    };
    let mut options = BTreeMap::new();
    for pair in args[1..].chunks(2) {
        if pair.len() != 2
            || !allowed.contains(&pair[0].as_str())
            || pair[1].starts_with("--")
            || options.insert(pair[0].clone(), pair[1].clone()).is_some()
        {
            return Err(error("usage", "unknown, duplicate or incomplete option"));
        }
    }
    if allowed.iter().any(|name| !options.contains_key(*name)) {
        return Err(error("usage", USAGE));
    }
    Ok(options)
}
fn validate_url(url: &str) -> Result<String, Value> {
    if url.bytes().any(|c| c <= 32 || c == 127) || url.contains(['?', '#', '@', '\\']) {
        return Err(error(
            "invalid_url",
            "URL must not contain credentials, query, fragment or whitespace",
        ));
    }
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| error("invalid_url", "expected HTTPS or loopback HTTP URL"))?;
    let authority = rest.split('/').next().unwrap_or("");
    let host = if authority.starts_with('[') {
        authority.split(']').next().map(|s| format!("{s}]"))
    } else {
        Some(authority.split(':').next().unwrap_or("").to_string())
    };
    if authority.is_empty()
        || (scheme != "https"
            && !(scheme == "http"
                && matches!(host.as_deref(), Some("127.0.0.1" | "localhost" | "[::1]"))))
    {
        return Err(error(
            "invalid_url",
            "HTTP is allowed only on loopback; use HTTPS for remote arenas",
        ));
    }
    Ok(url.trim_end_matches('/').to_string())
}
fn read_private(path: &Path) -> ResultJson {
    let meta = fs::symlink_metadata(path)
        .map_err(|_| error("session_unavailable", "cannot read private session file"))?;
    if !meta.is_file() || meta.len() > MAX_RESPONSE {
        return Err(error(
            "invalid_session",
            "expected a bounded regular session file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err(error(
                "session_permissions",
                "session file must have mode 600 or stricter",
            ));
        }
    }
    let bytes = fs::read(path)
        .map_err(|_| error("session_unavailable", "cannot read private session file"))?;
    let session: Value = serde_json::from_slice(&bytes)
        .map_err(|_| error("invalid_session", "session JSON is invalid"))?;
    if session["schema"] != "alashi.session.v1" {
        return Err(error("invalid_session", "unsupported session schema"));
    }
    validate_url(text(&session["config"], "url")?)?;
    if session["config"]["game_id"].as_u64().is_none() {
        return Err(error("invalid_session", "missing game ID"));
    }
    let secret = text(&session, "recovery_secret")?;
    if secret.len() != 64 || !secret.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(error("invalid_session", "invalid recovery secret"));
    }
    Ok(session)
}
fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
fn save_private(path: &Path, value: &Value) -> Result<(), Value> {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| error("invalid_session_path", "session path needs a file name"))?
        .to_string_lossy();
    let temporary = parent.join(format!(
        ".{name}.tmp-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = private_options().open(&temporary)?;
        file.write_all(&serde_json::to_vec(value).map_err(std::io::Error::other)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        #[cfg(unix)]
        fs::File::open(parent)?.sync_all()?;
        Ok::<_, std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(error(
            "session_save_failed",
            "private progress could not be saved; no further signing is allowed",
        ));
    }
    Ok(())
}
struct SessionLock(PathBuf);
impl SessionLock {
    fn acquire(path: &Path) -> Result<Self, Value> {
        let lock = PathBuf::from(format!("{}.lock", path.to_string_lossy()));
        let mut file=private_options().open(&lock).map_err(|_|error("session_busy","session is locked; if a previous process crashed, verify it stopped before removing the sibling .lock file"))?;
        let guard = Self(lock);
        writeln!(file, "{}", std::process::id())
            .map_err(|_| error("session_busy", "cannot write session lock"))?;
        Ok(guard)
    }
}
impl Drop for SessionLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
fn http_json(url: &str, path: &str, body: Option<&Value>) -> ResultJson {
    let base = validate_url(url)?;
    let mut command = Command::new("curl");
    command.args([
        "--disable",
        "--silent",
        "--show-error",
        "--connect-timeout",
        "5",
        "--max-time",
        "15",
        "--max-filesize",
        "1048576",
        "--proto",
        "=http,https",
        "--write-out",
        "\n%{http_code}",
        "--url",
        &format!("{base}{path}"),
    ]);
    if body.is_some() {
        command.args([
            "--request",
            "POST",
            "--header",
            "Content-Type: application/json",
            "--data-binary",
            "@-",
        ]);
    }
    let mut child = command
        .stdin(if body.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| error("http_unavailable", "cannot start HTTP client"))?;
    if let Some(body) = body {
        let bytes = serde_json::to_vec(body)
            .map_err(|_| error("invalid_request", "cannot encode request"))?;
        if child
            .stdin
            .take()
            .is_none_or(|mut input| input.write_all(&bytes).is_err())
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error("http_unavailable", "HTTP request could not be sent"));
        }
    }
    let mut bytes = Vec::new();
    let read = child
        .stdout
        .take()
        .unwrap()
        .take(MAX_RESPONSE + 1)
        .read_to_end(&mut bytes);
    if read.is_err() || bytes.len() as u64 > MAX_RESPONSE {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error(
            "http_unavailable",
            "HTTP response is unavailable or too large",
        ));
    }
    let status = child
        .wait()
        .map_err(|_| error("http_unavailable", "HTTP client did not finish"))?;
    if !status.success() {
        return Err(error(
            "http_unavailable",
            "HTTP request failed or timed out; saved progress is preserved",
        ));
    }
    let output = String::from_utf8(bytes)
        .map_err(|_| error("http_unavailable", "HTTP response is not UTF-8 JSON"))?;
    let (json, code) = output
        .rsplit_once('\n')
        .ok_or_else(|| error("http_unavailable", "HTTP response has no status"))?;
    let code = code
        .parse::<u16>()
        .map_err(|_| error("http_unavailable", "invalid HTTP response status"))?;
    if (300..400).contains(&code) {
        return Err(error(
            "http_redirect",
            "redirects are not followed when sending credentials",
        ));
    }
    let value: Value = serde_json::from_str(json)
        .map_err(|_| error("http_unavailable", "HTTP response is not JSON"))?;
    if !value.is_object() || value["ok"].as_bool().is_none() {
        return Err(error("http_unavailable", "unexpected arena response"));
    }
    if !(200..300).contains(&code) && value["ok"] == true {
        return Err(error(
            "http_unavailable",
            "inconsistent HTTP response status",
        ));
    }
    Ok(value)
}
fn server_code(value: &Value) -> &str {
    value["error"]
        .as_str()
        .or_else(|| value["error"]["code"].as_str())
        .unwrap_or("http_rejected")
}
fn safe_server_error(value: &Value) -> Value {
    let code = server_code(value);
    let code = match code {
        "bad_params"
        | "bad_action"
        | "bad_choice"
        | "bad_token"
        | "bad_recovery_secret"
        | "unknown_game"
        | "unknown_agent"
        | "already_joined"
        | "game_full"
        | "join_failed"
        | "name_too_long"
        | "storage_failed"
        | "busy"
        | "not_found"
        | "bad_json"
        | "bad_id"
        | "registration_required"
        | "registration_lobby_closed"
        | "invalid_registration"
        | "invalid_registration_wallet"
        | "invalid_registration_signature"
        | "not_solana_devnet"
        | "registration_not_confirmed"
        | "registration_transaction_failed"
        | "registration_transaction_unavailable"
        | "registration_slot_mismatch"
        | "registration_signature_mismatch"
        | "registration_wallet_not_signer"
        | "registration_memo_mismatch"
        | "registration_rpc_timeout"
        | "registration_rpc_failed"
        | "registration_rpc_response_too_large" => code,
        "GameNotInLobby" | "GameFinished" | "GameFull" | "WrongPhase" | "AlreadyActed"
        | "AlreadyVoted" | "NoUnits" | "NotEnoughGoods" | "NotEnoughCash" | "BribeTooSmall"
        | "BribeTooBig" | "SelfBribe" | "NotAlive" | "TooEarly" | "NotEnoughFactions"
        | "NotPresident" | "AlreadyVetoed" | "LawNotRevealed" | "GameAborted"
        | "DuplicateWallet" | "TooManyOffers" => code,
        _ => "http_rejected",
    };
    error(
        code,
        "arena rejected the request; private credentials and raw error body were not printed",
    )
}
fn identity_body(session: &Value) -> Value {
    let config = &session["config"];
    json!({"name":config["name"],"model":config["model"],"prompt":config["prompt"],"recovery_secret":session["recovery_secret"]})
}
fn identity_ids(session: &Value) -> (String, String) {
    fn hash(prefix: &[u8], parts: &[&str]) -> String {
        let mut hash = Sha256::new();
        hash.update(prefix);
        for part in parts {
            hash.update((part.len() as u64).to_le_bytes());
            hash.update(part.as_bytes());
        }
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }
    let config = &session["config"];
    let owner = hash(
        b"alashi-owner-v1",
        &[session["recovery_secret"].as_str().unwrap_or("")],
    );
    (
        hash(
            b"alashi-character-v1",
            &[&owner, config["name"].as_str().unwrap_or("")],
        ),
        hash(
            b"",
            &[
                config["model"].as_str().unwrap_or(""),
                config["prompt"].as_str().unwrap_or(""),
            ],
        ),
    )
}
fn check_proposal(session: &Value, proposal: &Value) -> Result<String, Value> {
    let (character, agent) = identity_ids(session);
    let party = proposal["party_no"]
        .as_u64()
        .ok_or_else(|| error("invalid_proposal", "proposal has no party number"))?;
    let game = session["config"]["game_id"].as_u64().unwrap();
    let expected = format!("alashi:agent-start:v1:devnet:{game}:{party}:{character}:{agent}");
    if proposal["ok"] != true
        || proposal["required"] != true
        || proposal["network"] != "devnet"
        || proposal["mode"] != "agent_start_v1"
        || proposal["memo_program_id"] != MEMO_ID
        || proposal["memo"] != expected
        || proposal["game_id"] != game
        || proposal["character_id"] != character
        || proposal["agent_id"] != agent
        || proposal["wallet"] != session["config"]["wallet"]
    {
        return Err(error(
            "invalid_proposal",
            "arena proposal does not match this devnet wallet and session identity",
        ));
    }
    Ok(expected)
}
fn memo_ix(wallet: Pubkey, memo: &str) -> Instruction {
    Instruction::new_with_bytes(
        MEMO_ID.parse().unwrap(),
        memo.as_bytes(),
        vec![anchor_lang::solana_program::instruction::AccountMeta::new_readonly(wallet, true)],
    )
}
/// Never create another transaction after a signature has been durably recorded.
fn registration_step(session: &Value) -> &'static str {
    if session["signature"].is_string() {
        if session["receipt"]["status"] == "confirmed" {
            "join"
        } else {
            "confirm"
        }
    } else {
        "sign"
    }
}
fn registration_failure(receipt: Value) -> Value {
    let code = if receipt["status"] == "unknown" {
        "registration_unknown"
    } else {
        "registration_not_confirmed"
    };
    json!({"ok":false,"error":{"code":code,"message":"saved signature will not be replaced; check receipt before continuing"},"receipt":receipt})
}
fn public_result(session: &Value, mut response: Value, command: &str) -> Value {
    let config = &session["config"];
    let idx = session["faction_idx"].as_u64();
    response["command"] = json!(command);
    response["execution"] = json!("offchain_http");
    response["game_id"] = config["game_id"].clone();
    response["party_no"] = session["party_no"].clone();
    response["wallet"] = config["wallet"].clone();
    response["agent_id"] = session["agent_id"].clone();
    response["character_id"] = session["character_id"].clone();
    response["your_faction_idx"] = json!(idx);
    response["your_faction"] = idx
        .and_then(|i| response["state"]["factions"].get(i as usize))
        .cloned()
        .unwrap_or(Value::Null);
    response["registration"] = session["registration"].clone();
    redact(&mut response, session);
    response
}
fn redact(value: &mut Value, session: &Value) {
    match value {
        Value::Object(object) => {
            object.retain(|key, _| {
                ![
                    session["token"].as_str(),
                    session["recovery_secret"].as_str(),
                    session["config"]["prompt"].as_str(),
                ]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .any(|secret| key.contains(secret))
            });
            for key in [
                "token",
                "recovery_secret",
                "owner_key",
                "prompt",
                "key",
                "keypair",
                "secret",
            ] {
                object.remove(key);
            }
            for v in object.values_mut() {
                redact(v, session);
            }
        }
        Value::Array(values) => {
            for v in values {
                redact(v, session);
            }
        }
        Value::String(s) => {
            for secret in [
                session["token"].as_str(),
                session["recovery_secret"].as_str(),
                session["config"]["prompt"].as_str(),
            ]
            .into_iter()
            .flatten()
            .filter(|s| !s.is_empty())
            {
                if s.contains(secret) {
                    *s = s.replace(secret, "[redacted]");
                }
            }
        }
        _ => {}
    }
}
fn load_or_create_session(path: &Path, config: Value) -> ResultJson {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            let session = read_private(path)?;
            if session["config"] != config {
                return Err(error("session_config_mismatch","existing session belongs to different connection settings; nothing was replaced"));
            }
            Ok(session)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let random = Keypair::new();
            let secret: String = random.to_bytes()[..32]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            let session = json!({"schema":"alashi.session.v1","config":config,"recovery_secret":secret,
                "memo":null,"signature":null,"receipt":null,"registration":null,"token":null});
            save_private(path, &session)?;
            Ok(session)
        }
        Err(_) => Err(error("session_unavailable", "cannot inspect session path")),
    }
}
fn connect(options: &BTreeMap<String, String>, rpc: &RpcClient) -> ResultJson {
    let path = Path::new(&options["--session-file"]);
    let _lock = SessionLock::acquire(path)?;
    let key = read_key(&options["--key"])?;
    let mut prompt = String::new();
    fs::File::open(&options["--prompt-file"])
        .and_then(|file| file.take(16385).read_to_string(&mut prompt))
        .map_err(|_| error("invalid_prompt", "cannot read prompt file"))?;
    if prompt.len() > 16384 {
        return Err(error("invalid_prompt", "prompt file exceeds 16384 bytes"));
    }
    connect_session(options, rpc, &key, prompt)
}
fn connect_session(
    options: &BTreeMap<String, String>,
    rpc: &RpcClient,
    key: &Keypair,
    prompt: String,
) -> ResultJson {
    let path = Path::new(&options["--session-file"]);
    let game = options["--game"]
        .parse::<u64>()
        .map_err(|_| error("invalid_game", "game must be an HTTP numeric game ID"))?;
    let config = json!({"url":validate_url(&options["--url"])? ,"game_id":game,"name":options["--name"],
        "model":options["--model"],"prompt":prompt,"wallet":key.pubkey().to_string()});
    let mut session = load_or_create_session(path, config)?;
    let base = text(&session["config"], "url")?.to_string();
    if session["memo"].is_null() {
        let mut body = identity_body(&session);
        body["wallet"] = session["config"]["wallet"].clone();
        let proposal = http_json(&base, &format!("/game/{game}/registration"), Some(&body))?;
        if proposal["ok"] != true {
            return Err(safe_server_error(&proposal));
        }
        session["memo"] = json!(check_proposal(&session, &proposal)?);
        for field in ["party_no", "agent_id", "character_id"] {
            session[field] = proposal[field].clone();
        }
        save_private(path, &session)?;
    }
    match registration_step(&session) {
        "sign" => {
            devnet(rpc)?;
            let tx = prepare_ix(rpc, key, memo_ix(key.pubkey(), text(&session, "memo")?))
                .map_err(registration_failure)?;
            session["signature"] = json!(tx.signatures[0].to_string());
            session["receipt"] = json!({"status":"unknown","signature":tx.signatures[0].to_string(),"error":"prepared_not_yet_confirmed"});
            // A crash from here onward may require manual resolution, never another fee automatically.
            save_private(path, &session)?;
            let result = submit_confirmed(rpc, &tx);
            session["receipt"] = match &result {
                Ok(v) | Err(v) => v.clone(),
            };
            save_private(path, &session)?;
            result.map_err(registration_failure)?;
        }
        "confirm" => {
            devnet(rpc)?;
            let signature = text(&session, "signature")?
                .parse()
                .map_err(|_| error("invalid_session", "saved signature is invalid"))?;
            let result = wait_receipt(rpc, &signature, Duration::from_secs(30));
            session["receipt"] = match &result {
                Ok(v) | Err(v) => v.clone(),
            };
            save_private(path, &session)?;
            result.map_err(registration_failure)?;
        }
        _ => {}
    }
    let mut recovery = identity_body(&session);
    recovery["recover"] = json!(true);
    let mut response = http_json(&base, &format!("/game/{game}/join"), Some(&recovery))?;
    if response["ok"] != true && server_code(&response) == "unknown_agent" {
        let mut body = identity_body(&session);
        body["registration"] =
            json!({"wallet":session["config"]["wallet"],"signature":session["signature"]});
        response = http_json(&base, &format!("/game/{game}/join"), Some(&body))?;
    }
    if response["ok"] != true {
        return Err(safe_server_error(&response));
    }
    if response["token"].as_str().is_none()
        || response["faction_idx"].as_u64().is_none()
        || response["character_id"] != session["character_id"]
        || response["agent_id"] != session["agent_id"]
    {
        return Err(error(
            "invalid_join_response",
            "arena join did not return the expected identity and credentials",
        ));
    }
    session["token"] = response["token"].clone();
    session["faction_idx"] = response["faction_idx"].clone();
    session["registration"] = response["registration"].clone();
    if session["registration"]["signature"] != session["signature"]
        || session["registration"]["wallet"] != session["config"]["wallet"]
        || session["registration"]["network"] != "devnet"
    {
        return Err(error(
            "invalid_join_response",
            "arena did not return this session's verified registration",
        ));
    }
    save_private(path, &session)?;
    Ok(public_result(&session, response, "connect"))
}
fn http_operation(path: &Path, command: &str, input: &str) -> ResultJson {
    let session = read_private(path)?;
    let base = text(&session["config"], "url")?;
    let game = session["config"]["game_id"].as_u64().unwrap();
    let result = if command == "inspect" {
        http_json(base, &format!("/game/{game}/state"), None)?
    } else {
        let mut body: Value = serde_json::from_str(input)
            .map_err(|_| error("invalid_action", "stdin must contain one JSON action"))?;
        let object = body
            .as_object()
            .ok_or_else(|| error("invalid_action", "action must be a JSON object"))?;
        if object
            .keys()
            .any(|k| !matches!(k.as_str(), "action" | "params" | "by"))
            || !body["action"].is_string()
            || body.get("params").is_some_and(|p| !p.is_object())
            || body
                .get("by")
                .is_some_and(|v| !matches!(v.as_str(), Some("llm" | "heuristic" | "unknown")))
        {
            return Err(error(
                "invalid_action",
                "expected action, optional params object, and optional by=llm|heuristic|unknown",
            ));
        }
        body["token"] = json!(text(&session, "token")?);
        http_json(base, &format!("/game/{game}/act"), Some(&body))?
    };
    if result["ok"] != true {
        return Err(safe_server_error(&result));
    }
    Ok(public_result(&session, result, command))
}
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|s| s == "--help" || s == "-h") {
        println!(
            "{}",
            json!({"ok":true,"usage":USAGE,"execution":"offchain_http","onchain":"one devnet Memo registration"})
        );
        return 0;
    }
    let result = (|| {
        let options = parse_options(args)?;
        let command = args[0].as_str();
        if command == "connect" {
            let rpc = RpcClient::new_with_timeout_and_commitment(
                rpc_url(),
                Duration::from_secs(10),
                CommitmentConfig::confirmed(),
            );
            return connect(&options, &rpc);
        }
        let mut input = String::new();
        if command == "act"
            && (std::io::stdin()
                .take(4097)
                .read_to_string(&mut input)
                .is_err()
                || input.len() > 4096)
        {
            return Err(error(
                "invalid_action",
                "action JSON exceeds 4096 bytes or is not UTF-8",
            ));
        }
        http_operation(Path::new(&options["--session-file"]), command, &input)
    })();
    match result {
        Ok(v) => {
            println!("{v}");
            0
        }
        Err(v) => {
            println!("{v}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
    };
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "alashi-session-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            }
            Self(p)
        }
        fn session(&self) -> PathBuf {
            self.0.join("session.json")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn config(url: &str) -> Value {
        json!({"url":url,"game_id":42,"name":"fixture","model":"test-model","prompt":"private strategy prompt","wallet":Keypair::new_from_array([7;32]).pubkey().to_string()})
    }
    fn fixture(
        listener: TcpListener,
        responses: Vec<(u16, Value)>,
    ) -> (Arc<Mutex<Vec<Value>>>, thread::JoinHandle<()>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let out = Arc::clone(&seen);
        let handle = thread::spawn(move || {
            for (status, response) in responses {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    bytes.push(byte[0]);
                    assert!(bytes.len() < 16384);
                }
                let header = String::from_utf8(bytes).unwrap();
                let length = header
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|n| n.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                out.lock().unwrap().push(if body.is_empty() {
                    json!({"method":"GET"})
                } else {
                    serde_json::from_slice(&body).unwrap()
                });
                let body = response.to_string();
                write!(stream,"HTTP/1.1 {status} Fixture\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        });
        (seen, handle)
    }
    fn listener() -> (TcpListener, String) {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let u = format!("http://{}", l.local_addr().unwrap());
        (l, u)
    }
    fn prepared(path: &Path, url: &str) -> Value {
        let mut s = load_or_create_session(path, config(url)).unwrap();
        let (character, agent) = identity_ids(&s);
        s["character_id"] = json!(character);
        s["agent_id"] = json!(agent);
        s["party_no"] = json!(12);
        s["memo"] = json!(format!(
            "alashi:agent-start:v1:devnet:42:12:{character}:{agent}"
        ));
        s["signature"] = json!(solana_signature::Signature::from([1; 64]).to_string());
        s["receipt"] = json!({"status":"confirmed","signature":s["signature"],"slot":123});
        save_private(path, &s).unwrap();
        s
    }
    #[test]
    fn private_file_atomic_permissions_and_immutable_config() {
        use std::os::unix::fs::PermissionsExt;
        let dir = Temp::new();
        let path = dir.session();
        let original = load_or_create_session(&path, config("http://127.0.0.1:8092")).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(original["recovery_secret"].as_str().unwrap().len(), 64);
        let resumed = load_or_create_session(&path, config("http://127.0.0.1:8092")).unwrap();
        assert_eq!(original, resumed);
        assert!(load_or_create_session(&path, config("https://different.invalid")).is_err());
        assert_eq!(read_private(&path).unwrap(), original);
        save_private(&path, &original).unwrap();
        assert_eq!(fs::read_dir(&dir.0).unwrap().count(), 1);
        let guard = SessionLock::acquire(&path).unwrap();
        assert!(SessionLock::acquire(&path).is_err());
        drop(guard);
        assert!(SessionLock::acquire(&path).is_ok());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            read_private(&path).unwrap_err()["error"]["code"],
            "session_permissions"
        );
    }
    #[test]
    fn exact_memo_and_proposal_bound_to_session() {
        let dir = Temp::new();
        let session = prepared(&dir.session(), "https://arena.invalid");
        let wallet: Pubkey = session["config"]["wallet"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let memo = session["memo"].as_str().unwrap();
        let instruction = memo_ix(wallet, memo);
        assert_eq!(instruction.program_id.to_string(), MEMO_ID);
        assert_eq!(instruction.data, memo.as_bytes());
        assert_eq!(instruction.accounts.len(), 1);
        assert_eq!(instruction.accounts[0].pubkey, wallet);
        assert!(instruction.accounts[0].is_signer);
        assert!(!instruction.accounts[0].is_writable);
        let mut proposal = json!({"ok":true,"mode":"agent_start_v1","network":"devnet","required":true,"memo_program_id":MEMO_ID,"memo":memo,"party_no":12,"game_id":42,"character_id":session["character_id"],"agent_id":session["agent_id"],"wallet":wallet.to_string()});
        assert_eq!(check_proposal(&session, &proposal).unwrap(), memo);
        proposal["wallet"] = json!(Pubkey::new_unique().to_string());
        assert!(check_proposal(&session, &proposal).is_err());
    }
    #[test]
    fn recorded_signature_is_never_signed_again() {
        for status in ["unknown", "failed", "rejected", "not_sent", "confirmed"] {
            let session = json!({"signature":"saved","receipt":{"status":status}});
            assert_eq!(
                registration_step(&session),
                if status == "confirmed" {
                    "join"
                } else {
                    "confirm"
                }
            );
        }
        assert_eq!(registration_step(&json!({"signature":null})), "sign");
    }
    #[test]
    fn resume_after_http_failure_reuses_signature_without_rpc() {
        let dir = Temp::new();
        let path = dir.session();
        let (listener, url) = listener();
        let session = prepared(&path, &url);
        let registration = json!({"mode":"agent_start_v1","network":"devnet","wallet":session["config"]["wallet"],"signature":session["signature"],"slot":123,"fee_lamports":"5000","commitment":"confirmed"});
        let joined = json!({"ok":true,"token":"private-fixture-token","faction_idx":1,"character_id":session["character_id"],"agent_id":session["agent_id"],"registration":registration,"state":{"factions":[{"name":"first"},{"name":"fixture"}]}});
        let (seen, handle) = fixture(
            listener,
            vec![
                (404, json!({"ok":false,"error":"unknown_agent"})),
                (
                    500,
                    json!({"ok":false,"error":"storage_failed","message":session["recovery_secret"]}),
                ),
                (404, json!({"ok":false,"error":"unknown_agent"})),
                (200, joined),
            ],
        );
        let options = BTreeMap::from([
            (
                "--session-file".to_string(),
                path.to_string_lossy().into_owned(),
            ),
            ("--url".to_string(), url),
            ("--game".to_string(), "42".into()),
            ("--name".to_string(), "fixture".into()),
            ("--model".to_string(), "test-model".into()),
        ]);
        let rpc = RpcClient::new_mock("fails");
        let key = Keypair::new_from_array([7; 32]);
        let error =
            connect_session(&options, &rpc, &key, "private strategy prompt".into()).unwrap_err();
        assert_eq!(error["error"]["code"], "storage_failed");
        assert!(!error
            .to_string()
            .contains(session["recovery_secret"].as_str().unwrap()));
        assert_eq!(
            read_private(&path).unwrap()["signature"],
            session["signature"]
        );
        let result =
            connect_session(&options, &rpc, &key, "private strategy prompt".into()).unwrap();
        assert_eq!(result["your_faction_idx"], 1);
        assert_eq!(result["your_faction"]["name"], "fixture");
        assert!(result.get("token").is_none());
        assert!(!result.to_string().contains("private-fixture-token"));
        handle.join().unwrap();
        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 4);
        for i in [0, 2] {
            assert_eq!(requests[i]["recover"], true);
            assert!(requests[i].get("registration").is_none());
        }
        for i in [1, 3] {
            assert_eq!(
                requests[i]["registration"]["signature"],
                session["signature"]
            );
            assert_eq!(requests[i]["recovery_secret"], session["recovery_secret"]);
        }
    }
    #[test]
    fn http_actions_only_and_public_response_strips_echoed_credentials() {
        let dir = Temp::new();
        let path = dir.session();
        let (listener, url) = listener();
        let mut session = prepared(&path, &url);
        session["token"] = json!("private-fixture-token");
        session["faction_idx"] = json!(0);
        save_private(&path, &session).unwrap();
        let echoed = format!(
            "{} {} {}",
            session["token"].as_str().unwrap(),
            session["recovery_secret"].as_str().unwrap(),
            session["config"]["prompt"].as_str().unwrap()
        );
        let (seen, handle) = fixture(
            listener,
            vec![
                (
                    200,
                    json!({"ok":true,"token":"private-fixture-token","message":echoed,"state":{"factions":[{"name":"fixture","prompt":"private strategy prompt"}]}}),
                ),
                (
                    200,
                    json!({"ok":true,"state":{"factions":[{"name":"fixture"}]}}),
                ),
            ],
        );
        let result = http_operation(&path, "act", r#"{"action":"produce","by":"llm"}"#).unwrap();
        let output = result.to_string();
        for secret in [
            session["token"].as_str().unwrap(),
            session["recovery_secret"].as_str().unwrap(),
            session["config"]["prompt"].as_str().unwrap(),
        ] {
            assert!(!output.contains(secret));
        }
        assert_eq!(result["execution"], "offchain_http");
        assert!(result["your_faction"].get("prompt").is_none());
        assert_eq!(
            http_operation(&path, "inspect", "").unwrap()["your_faction"]["name"],
            "fixture"
        );
        handle.join().unwrap();
        let requests = seen.lock().unwrap();
        assert_eq!(requests[0]["token"], session["token"]);
        assert_eq!(requests[0]["action"], "produce");
        assert_eq!(requests[1]["method"], "GET");
    }
    #[test]
    fn credentials_cannot_be_echoed_as_error_code_or_object_key() {
        let secret = "a".repeat(64);
        assert_eq!(
            safe_server_error(&json!({"ok":false,"error":secret}))["error"]["code"],
            "http_rejected"
        );
        let session = json!({"recovery_secret":secret,"token":"private-fixture-token","config":{"prompt":"private strategy prompt"}});
        let mut response = json!({"ok":true});
        response[session["recovery_secret"].as_str().unwrap()] = json!("echo");
        response["nested"] = json!({"private-fixture-token":"echo"});
        redact(&mut response, &session);
        assert!(!response
            .to_string()
            .contains(session["recovery_secret"].as_str().unwrap()));
        assert!(!response.to_string().contains("private-fixture-token"));
    }
    #[test]
    fn urls_and_redirects_do_not_leak_credentials() {
        for url in [
            "http://external.invalid",
            "https://user:pass@example.com",
            "https://example.com?key=secret",
            "ftp://127.0.0.1",
            "http://localhost.evil",
        ] {
            assert!(validate_url(url).is_err());
        }
        for url in [
            "https://alashi.network/api",
            "http://127.0.0.1:8092",
            "http://[::1]:8092",
        ] {
            assert!(validate_url(url).is_ok());
        }
        let (listener, url) = listener();
        let (_, handle) = fixture(
            listener,
            vec![(302, json!({"ok":true,"token":"must-not-return"}))],
        );
        assert_eq!(
            http_json(&url, "/join", Some(&json!({"token":"local"}))).unwrap_err()["error"]["code"],
            "http_redirect"
        );
        handle.join().unwrap();
    }
}
