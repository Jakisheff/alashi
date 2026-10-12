//! Operator-started, bounded dialogue between two registered epoch-1 factions.
//! This command attaches to a Game; it never creates/funds one or advances phases.
use super::*;
use crate::dialogue::{
    Action, PublicContext, PublicFaction, PublicMessage, PublicOffer, SessionBudget,
};
use crate::dialogue_mailbox::Mailbox;
use crate::dialogue_provider::Provider;
use serde_json::{json, Value};
use std::process::{Command, Stdio};

const USAGE: &str = "bots dialogue --config FILE --execute\nAttaches two registered wallets to an existing devnet epoch-1 Game. Calls the configured decision sources and signs bounded barter actions. Use --check-config to validate without reading keys or contacting services.";
struct AgentConfig {
    profile: PathBuf,
    key: String,
    source: DecisionSource,
}
enum DecisionSource {
    Https(Provider),
    Local(Mailbox),
}
impl DecisionSource {
    fn timeout_ms(&self) -> u64 {
        match self {
            Self::Https(_) => 12_000,
            Self::Local(m) => m.timeout_ms,
        }
    }
    fn label(&self) -> &'static str {
        match self {
            Self::Https(_) => "https_provider",
            Self::Local(_) => "local_harness",
        }
    }
    fn decide(
        &self,
        id: &str,
        c: &PublicContext,
        h: &[Value],
        timeout: u64,
    ) -> Result<dialogue::Decision, &'static str> {
        match self {
            Self::Https(p) => p.decide_with_history(c, h, timeout),
            Self::Local(m) => m.decide_with_history(id, c, h, timeout),
        }
    }
}
struct Config {
    game: Pubkey,
    agents: Vec<AgentConfig>,
    seconds: u64,
    calls: u16,
    transactions: u16,
    audit: PathBuf,
}
fn string(v: &Value, k: &str) -> Result<String, &'static str> {
    v[k].as_str()
        .filter(|s| !s.is_empty() && s.len() <= 4096)
        .map(String::from)
        .ok_or("invalid_config")
}
fn exact(v: &Value, fields: &[&str]) -> bool {
    v.as_object()
        .is_some_and(|o| o.len() == fields.len() && fields.iter().all(|k| o.contains_key(*k)))
}
fn config(v: &Value) -> Result<Config, &'static str> {
    if !exact(
        v,
        &[
            "game",
            "agents",
            "duration_seconds",
            "calls_per_agent",
            "max_transactions",
            "audit_file",
        ],
    ) {
        return Err("invalid_config");
    }
    let game = string(v, "game")?.parse().map_err(|_| "invalid_game")?;
    let seconds = v["duration_seconds"]
        .as_u64()
        .filter(|n| (10..=600).contains(n))
        .ok_or("invalid_budget")?;
    let calls = v["calls_per_agent"]
        .as_u64()
        .filter(|n| (1..=12).contains(n))
        .ok_or("invalid_budget")? as u16;
    let transactions = v["max_transactions"]
        .as_u64()
        .filter(|n| *n <= 8)
        .ok_or("invalid_budget")? as u16;
    let rows = v["agents"]
        .as_array()
        .filter(|r| r.len() == 2)
        .ok_or("two_agents_required")?;
    let mut agents = Vec::new();
    for r in rows {
        let source = if r.get("decision_source").is_some() {
            if !exact(
                r,
                &[
                    "profile",
                    "key",
                    "decision_source",
                    "mailbox_directory",
                    "decision_timeout_ms",
                    "public_strategy",
                ],
            ) || r["decision_source"] != "local_harness"
            {
                return Err("invalid_agent_config");
            }
            let mailbox = Mailbox {
                directory: PathBuf::from(string(r, "mailbox_directory")?),
                public_strategy: string(r, "public_strategy")?,
                timeout_ms: r["decision_timeout_ms"].as_u64().ok_or("invalid_budget")?,
            };
            mailbox.validate()?;
            DecisionSource::Local(mailbox)
        } else {
            if !exact(
                r,
                &[
                    "profile",
                    "key",
                    "endpoint",
                    "model",
                    "api_key_env",
                    "public_strategy",
                ],
            ) {
                return Err("invalid_agent_config");
            }
            let provider = Provider {
                endpoint: string(r, "endpoint")?,
                model: string(r, "model")?,
                api_key_env: string(r, "api_key_env")?,
                public_strategy: string(r, "public_strategy")?,
            };
            provider.validate()?;
            DecisionSource::Https(provider)
        };
        agents.push(AgentConfig {
            profile: PathBuf::from(string(r, "profile")?),
            key: string(r, "key")?,
            source,
        });
    }
    if agents[0].profile == agents[1].profile || agents[0].key == agents[1].key {
        return Err("distinct_agents_required");
    }
    if let (DecisionSource::Local(a), DecisionSource::Local(b)) =
        (&agents[0].source, &agents[1].source)
    {
        if a.directory == b.directory {
            return Err("distinct_mailboxes_required");
        }
    }
    Ok(Config {
        game,
        agents,
        seconds,
        calls,
        transactions,
        audit: PathBuf::from(string(v, "audit_file")?),
    })
}
// Fixed public origin, no credentials in argv, no redirects or curl user config.
fn api(path: &str, body: Option<&Value>) -> Result<Value, &'static str> {
    api_until(path, body, unix_ms().saturating_add(12_000))
}
fn api_until(path: &str, body: Option<&Value>, deadline_ms: u64) -> Result<Value, &'static str> {
    let remaining = deadline_ms.saturating_sub(unix_ms()).min(12_000);
    if remaining == 0 {
        return Err("request_deadline");
    }
    let timeout = remaining as f64 / 1000.0;
    let mut cfg = format!("url = \"{}{path}\"\nsilent\nfail-with-body\nmax-time = {timeout:.3}\nconnect-timeout = 4\nmax-filesize = 262144\nproto = \"=https\"\n",runner_auth::ORIGIN);
    if let Some(body) = body {
        cfg.push_str(&format!("request = \"POST\"\nheader = \"Content-Type: application/json\"\nheader = \"Origin: {}\"\ndata-binary = {}\n",runner_auth::ORIGIN,serde_json::to_string(&body.to_string()).map_err(|_|"request_invalid")?));
    }
    let mut child = Command::new("curl")
        .args(["--disable", "--config", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "api_transport")?;
    if child
        .stdin
        .take()
        .is_none_or(|mut s| s.write_all(cfg.as_bytes()).is_err())
    {
        let _ = child.kill();
        let _ = child.wait();
        return Err("api_transport");
    }
    let out = child.wait_with_output().map_err(|_| "api_transport")?;
    if out.stdout.len() > 262144 {
        return Err("api_transport");
    }
    let v: Value = serde_json::from_slice(&out.stdout).map_err(|_| "api_transport")?;
    if matches!(
        v["error"].as_str(),
        Some("dialogue_rate_limited" | "dialogue_quota_exhausted")
    ) {
        return Err("speech_limited");
    }
    if v["error"] == "receipt_pending" {
        return Err("receipt_pending");
    }
    if v["error"] == "chain_api_unavailable" {
        return Err("api_transport");
    }
    if !out.status.success() || v["ok"] != true {
        return Err("api_rejected");
    }
    Ok(v)
}
fn speech_allowed(
    feed: &Value,
    context: &PublicContext,
    message: Option<&dialogue::Message>,
    at: u64,
) -> bool {
    let Some(message) = message else {
        return false;
    };
    let Some(rows) = feed["entries"].as_array() else {
        return false;
    };
    let own: Vec<_> = rows
        .iter()
        .filter(|r| r["author_faction_pda"] == context.faction)
        .collect();
    if own.len() >= 24
        || own
            .iter()
            .filter(|r| {
                r["round"] == context.round
                    && r["phase"] == context.phase
                    && r["phase_ends_at"] == context.phase_ends_at
            })
            .count()
            >= 4
    {
        return false;
    }
    if message
        .in_reply_to
        .as_ref()
        .is_some_and(|id| own.iter().any(|r| r["reply_to_message_id"] == *id))
    {
        return false;
    }
    // Server performs authoritative five-second timestamp validation. Provider and outer loop are bounded independently.
    at < context.phase_ends_at as u64
}
fn admission_open(at_ms: u64, end_ms: u64, phase_end: i64) -> bool {
    phase_end > 0
        && at_ms.saturating_add(1000) < end_ms.min((phase_end as u64).saturating_mul(1000))
}
fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
fn journal(game: &Pubkey) -> Result<Value, &'static str> {
    let v = api(
        &format!("/chain/devnet/games/{game}/dialogue?after=0&limit=100"),
        None,
    )?;
    if v["schema"] != "alashi.chain_dialogue.v1"
        || v["game_pda"] != game.to_string()
        || v["has_more"] != false
    {
        return Err("journal_invalid");
    }
    Ok(v)
}
fn context(
    rpc: &RpcClient,
    game: &Pubkey,
    keys: &[Keypair],
    author: usize,
    feed: &Value,
) -> Result<(PublicContext, u64), &'static str> {
    let factions: Vec<_> = keys
        .iter()
        .map(|k| faction_pda(game, &k.pubkey()))
        .collect();
    let accounts = rpc
        .get_multiple_accounts(&[*game, factions[0], factions[1]])
        .map_err(|_| "chain_unavailable")?;
    let data: Result<Vec<_>, _> = accounts
        .iter()
        .map(|a| {
            a.as_ref()
                .filter(|a| a.owner == id())
                .ok_or("invalid_account")
        })
        .collect();
    let data = data?;
    let g =
        state::Game::try_deserialize(&mut data[0].data.as_slice()).map_err(|_| "invalid_game")?;
    if g.epoch != 1 || g.settled || g.faction_count != 2 {
        return Err("requires_active_two_faction_epoch1");
    }
    let phase = format!("{:?}", g.phase);
    if !["Market", "Action", "Law"].contains(&phase.as_str())
        || g.phase_ends_at <= (unix_ms() / 1000) as i64
    {
        return Err("inactive_phase");
    }
    let mut fs = Vec::new();
    for i in 0..2 {
        let f = state::Faction::try_deserialize(&mut data[i + 1].data.as_slice())
            .map_err(|_| "invalid_faction")?;
        if f.game != *game || f.wallet != keys[i].pubkey() || !f.alive {
            return Err("invalid_faction");
        }
        fs.push(PublicFaction {
            pda: factions[i].to_string(),
            goods: f.goods as u64,
            cash: f.cash,
        });
    }
    let offers = g
        .barter_offers
        .iter()
        .map(|o| PublicOffer {
            id: o.id.to_string(),
            proposer: faction_pda(game, &o.from).to_string(),
            recipient: (o.to != Pubkey::default()).then(|| faction_pda(game, &o.to).to_string()),
            goods: o.goods as u64,
            price: o.price,
            round: g.round,
        })
        .collect();
    let rows = feed["entries"].as_array().ok_or("journal_invalid")?;
    let mut incoming = Vec::new();
    for r in rows {
        if r["game_pda"] != game.to_string() || r["source"] != "runner_reported" {
            return Err("journal_invalid");
        }
        if r["to_faction_pda"] == factions[author].to_string() {
            incoming.push(PublicMessage {
                id: string(r, "message_id")?,
                author: string(r, "author_faction_pda")?,
                recipient: Some(string(r, "to_faction_pda")?),
                text: string(r, "text")?,
            });
        }
    }
    let c = PublicContext {
        game: game.to_string(),
        faction: factions[author].to_string(),
        round: g.round,
        phase,
        phase_ends_at: g.phase_ends_at,
        observed_cursor: string(feed, "latest_seq")?
            .parse()
            .map_err(|_| "journal_invalid")?,
        factions: fs,
        open_offers: offers,
        incoming,
    };
    c.validate()?;
    Ok((c, g.barter_next_id))
}
fn log(file: &mut fs::File, v: Value) -> Result<(), &'static str> {
    writeln!(file, "{v}")
        .and_then(|_| file.sync_all())
        .map_err(|_| "audit_storage_failed")
}
fn post_repeat(path: &str, body: &Value, deadline_ms: u64) -> Result<Value, &'static str> {
    // Same identity/body on every retry. Failure stops the session; never invent a reply.
    for _ in 0..3 {
        match api_until(path, Some(body), deadline_ms) {
            Ok(v) => return Ok(v),
            Err("api_transport" | "receipt_pending") => sleep(Duration::from_secs(5)),
            Err(e) => return Err(e),
        }
    }
    Err("publication_unknown")
}
fn execute(cfg: Config) -> Result<(), &'static str> {
    let keys: Vec<_> = cfg
        .agents
        .iter()
        .map(|a| agent_cli::read_key(&a.key).map_err(|_| "key_invalid"))
        .collect::<Result<_, _>>()?;
    if keys[0].pubkey() == keys[1].pubkey() {
        return Err("distinct_wallets_required");
    }
    let pending: Vec<_> = cfg
        .agents
        .iter()
        .map(|a| a.profile.with_extension("dialogue-pending.json"))
        .collect();
    if pending.iter().any(|p| p.exists()) {
        return Err("pending_transaction_requires_operator_reconciliation");
    }
    let mut audit = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&cfg.audit)
        .map_err(|_| "audit_requires_new_file")?;
    let rpc = RpcClient::new_with_timeout_and_commitment(
        rpc_url(),
        Duration::from_secs(8),
        CommitmentConfig::confirmed(),
    );
    agent_cli::devnet(&rpc).map_err(|_| "devnet_verification_failed")?;
    let first_feed = journal(&cfg.game)?;
    context(&rpc, &cfg.game, &keys, 0, &first_feed)?;
    let mut bindings = Vec::new();
    for (i, a) in cfg.agents.iter().enumerate() {
        let (_, binding) = runner_auth::authenticate_and_bind(
            &a.profile,
            &a.key,
            &cfg.game.to_string(),
            &faction_pda(&cfg.game, &keys[i].pubkey()).to_string(),
        )?;
        bindings.push(binding);
    }
    if bindings[0].record_id == bindings[1].record_id {
        return Err("distinct_records_required");
    }
    let started = unix_ms();
    let end = started + cfg.seconds * 1000;
    let mut budgets = bindings
        .iter()
        .enumerate()
        .map(|(i, b)| {
            SessionBudget::new(
                b.faction_pda.clone(),
                cfg.calls,
                end,
                cfg.agents[i].source.timeout_ms(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut turns = [0u16; 2];
    let mut last_context: [Option<PublicContext>; 2] = [None, None];
    let mut transactions = 0u16;
    log(
        &mut audit,
        json!({"kind":"session_started","game":cfg.game.to_string(),"wallets":keys.iter().map(|k|k.pubkey().to_string()).collect::<Vec<_>>(),"calls_per_agent":cfg.calls,"max_transactions":cfg.transactions,"deadline_ms":end,"decision_sources":cfg.agents.iter().map(|a|a.source.label()).collect::<Vec<_>>()}),
    )?;
    while unix_ms() < end && turns.iter().any(|n| *n < cfg.calls) {
        for i in 0..2 {
            if turns[i] >= cfg.calls || unix_ms() >= end {
                continue;
            }
            let feed = journal(&cfg.game)?;
            let (observed, _) = match context(&rpc, &cfg.game, &keys, i, &feed) {
                Ok(v) => v,
                Err("inactive_phase") => {
                    sleep(Duration::from_secs(1));
                    continue;
                }
                Err(e) => return Err(e),
            };
            if last_context[i].as_ref() == Some(&observed) {
                continue;
            }
            let remaining = (observed.phase_ends_at * 1000).saturating_sub(unix_ms() as i64) - 2500;
            if remaining < 1000 {
                sleep(Duration::from_secs(1));
                continue;
            }
            let timeout = budgets[i]
                .reserve(&observed, unix_ms())?
                .min(remaining as u64);
            turns[i] += 1;
            let decision_id = format!("d{started}-{i}-{}", turns[i]);
            last_context[i] = Some(observed.clone());
            let history = feed["entries"].as_array().ok_or("journal_invalid")?;
            let mut decision =
                match cfg.agents[i]
                    .source
                    .decide(&decision_id, &observed, history, timeout)
                {
                    Ok(d) => d,
                    Err(e) => {
                        log(
                            &mut audit,
                            json!({"kind":"decision_silent","decision_id":decision_id,"reason":e}),
                        )?;
                        continue;
                    }
                };
            let (fresh, _) = context(&rpc, &cfg.game, &keys, i, &journal(&cfg.game)?)?;
            if decision.validate_fresh(&observed, &fresh, false).is_err() {
                log(
                    &mut audit,
                    json!({"kind":"decision_stale","decision_id":decision_id}),
                )?;
                continue;
            }
            log(
                &mut audit,
                json!({"kind":"decision","decision_id":decision_id,"decision_source":cfg.agents[i].source.label(),"faction":fresh.faction,"public_context":fresh.to_public_json()?,"action":format!("{:?}",decision.action)}),
            )?;
            if !speech_allowed(
                &feed,
                &fresh,
                decision.public_message.as_ref(),
                unix_ms() / 1000,
            ) {
                decision.public_message = None;
            }
            if !admission_open(unix_ms(), end, fresh.phase_ends_at) {
                continue;
            }
            if let Some(m) = &decision.public_message {
                let mut body = json!({"runner_token":bindings[i].runner_token,"client_message_id":decision_id,"expected_round":fresh.round,"expected_phase":fresh.phase,"expected_phase_ends_at":fresh.phase_ends_at,"to_faction_pda":m.to_faction_pda,"text":m.text});
                if let Some(reply) = &m.in_reply_to {
                    body["reply_to_message_id"] = json!(reply);
                }
                match post_repeat(
                    &format!("/chain/devnet/games/{}/runner/dialogue", cfg.game),
                    &body,
                    end.min(fresh.phase_ends_at as u64 * 1000),
                ) {
                    Ok(published) => log(
                        &mut audit,
                        json!({"kind":"message_published","decision_id":decision_id,"message":published["message"]}),
                    )?,
                    Err("speech_limited") => log(
                        &mut audit,
                        json!({"kind":"speech_suppressed","decision_id":decision_id,"reason":"quota_or_interval"}),
                    )?,
                    Err(e) => return Err(e),
                }
            }
            if decision.action == Action::Pass {
                continue;
            }
            if transactions >= cfg.transactions {
                log(
                    &mut audit,
                    json!({"kind":"transaction_budget_exhausted","decision_id":decision_id}),
                )?;
                continue;
            }
            // Include any text received while publishing; only our cursor increment is ignored.
            let (current, next_offer) = context(&rpc, &cfg.game, &keys, i, &journal(&cfg.game)?)?;
            let mut prior = fresh.clone();
            prior.observed_cursor = current.observed_cursor;
            if decision.validate_fresh(&prior, &current, false).is_err() {
                continue;
            }
            let faction = Pubkey::from_str(&current.faction).map_err(|_| "invalid_faction")?;
            let (ix, offer_id, proposer, goods, price, kind) = match &decision.action {
                Action::Pass => continue,
                Action::BarterPropose { goods, price } => (
                    ix_barter_propose(keys[i].pubkey(), cfg.game, faction, *goods as u16, *price),
                    next_offer,
                    current.faction.clone(),
                    Some(*goods as u16),
                    Some(*price),
                    "offer_confirmed",
                ),
                Action::BarterAccept { offer_id } => {
                    let offer = current
                        .open_offers
                        .iter()
                        .find(|o| &o.id == offer_id)
                        .ok_or("stale_offer")?;
                    let oid = offer_id.parse().map_err(|_| "invalid_offer")?;
                    let seller = Pubkey::from_str(&offer.proposer).map_err(|_| "invalid_offer")?;
                    (
                        ix_barter_accept(oid, keys[i].pubkey(), cfg.game, faction, seller),
                        oid,
                        offer.proposer.clone(),
                        None,
                        None,
                        "accepted_confirmed",
                    )
                }
            };
            if unix_ms() + 2000 >= current.phase_ends_at as u64 * 1000 || unix_ms() >= end {
                continue;
            }
            // Blockhash I/O precedes the final account read and local signing gate.
            let bh = rpc
                .get_latest_blockhash()
                .map_err(|_| "blockhash_unavailable")?;
            let signing_feed = journal(&cfg.game)?;
            let (signing_context, signing_offer) =
                context(&rpc, &cfg.game, &keys, i, &signing_feed)?;
            if decision
                .validate_fresh(&current, &signing_context, false)
                .is_err()
                || signing_offer != next_offer
                || !admission_open(unix_ms(), end, signing_context.phase_ends_at)
            {
                continue;
            }
            let msg = Message::new_with_blockhash(&[ix], Some(&keys[i].pubkey()), &bh);
            let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&keys[i]])
                .map_err(|_| "signing_failed")?;
            let signature = tx.signatures[0].to_string();
            let mut guard = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&pending[i])
                .map_err(|_| "pending_transaction_requires_operator_reconciliation")?;
            write!(guard,"{}",json!({"game":cfg.game.to_string(),"faction":current.faction,"signature":signature,"decision_id":decision_id})).and_then(|_|guard.sync_all()).map_err(|_|"pending_storage_failed")?;
            if !admission_open(unix_ms(), end, signing_context.phase_ends_at) {
                log(
                    &mut audit,
                    json!({"kind":"transaction_not_sent_expired","signature":signature}),
                )?;
                fs::remove_file(&pending[i]).map_err(|_| "pending_storage_failed")?;
                continue;
            }
            if let Some(parent) = pending[i].parent() {
                let parent = if parent.as_os_str().is_empty() {
                    Path::new(".")
                } else {
                    parent
                };
                fs::File::open(parent)
                    .and_then(|f| f.sync_all())
                    .map_err(|_| "pending_storage_failed")?;
            }
            if !admission_open(unix_ms(), end, signing_context.phase_ends_at) {
                return Err("transaction_not_sent_expired_pending_file");
            }
            transactions += 1;
            let receipt = match submit_confirmed(&rpc, &tx) {
                Ok(r) => r,
                Err(r) => {
                    log(
                        &mut audit,
                        json!({"kind":"transaction_unresolved","receipt":r}),
                    )?;
                    return Err("transaction_stopped_reconcile_pending_file");
                }
            };
            let scoped = barter::with_alashi_logs(&receipt);
            let verified = if let (Some(g), Some(p)) = (goods, price) {
                barter::verified_offer(&scoped, cfg.game, faction, offer_id, g, p)
            } else {
                barter::verified_accept(
                    &scoped,
                    cfg.game,
                    faction,
                    proposer.parse().map_err(|_| "invalid_offer")?,
                    offer_id,
                )
            };
            let proof = verified.ok_or("receipt_event_mismatch_reconcile_pending_file")?;
            log(
                &mut audit,
                json!({"kind":"transaction_confirmed","decision_id":decision_id,"receipt":receipt}),
            )?;
            let mut body = json!({"runner_token":bindings[i].runner_token,"client_entry_id":decision_id,"kind":kind,"offer_id":offer_id.to_string(),"proposer_faction_pda":proposer,"signature":proof.signature,"slot":proof.slot.to_string()});
            if let (Some(g), Some(p)) = (goods, price) {
                body["goods"] = json!(g);
                body["price"] = json!(p.to_string());
            }
            // The chain projection may lag the receipt; keep the same identity and stop on failure.
            let report = post_repeat(
                &format!("/chain/devnet/games/{}/runner/conversations", cfg.game),
                &body,
                unix_ms().saturating_add(50_000),
            )?;
            log(
                &mut audit,
                json!({"kind":"conversation_published","decision_id":decision_id,"entry":report["entry"]}),
            )?;
            fs::remove_file(&pending[i]).map_err(|_| "pending_storage_failed")?;
        }
        sleep(Duration::from_secs(5));
    }
    log(
        &mut audit,
        json!({"kind":"session_finished","transactions":transactions,"calls":turns}),
    )
}
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help") {
        println!("{USAGE}");
        return 0;
    }
    let result = (|| {
        if args.len() != 3
            || args[0] != "--config"
            || !["--execute", "--check-config"].contains(&args[2].as_str())
        {
            return Err("usage");
        }
        let meta = fs::metadata(&args[1]).map_err(|_| "config_unavailable")?;
        if !meta.is_file() || meta.len() > 32768 {
            return Err("config_invalid");
        }
        let v: Value =
            serde_json::from_slice(&fs::read(&args[1]).map_err(|_| "config_unavailable")?)
                .map_err(|_| "config_invalid")?;
        let cfg = config(&v)?;
        if args[2] == "--check-config" {
            println!("configuration valid; no credentials read or network calls made");
            return Ok(());
        }
        execute(cfg)
    })();
    match result {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("dialogue: {e}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config_value() -> Value {
        let agent = |suffix: &str| json!({"profile":format!("/private/profile-{suffix}.json"),"key":format!("/private/key-{suffix}.json"),"endpoint":"https://example.org/v1/chat/completions","model":"explicit-model","api_key_env":format!("DIALOGUE_{suffix}_KEY"),"public_strategy":"Choose your own trades from public state."});
        json!({"game":Pubkey::new_from_array([3;32]).to_string(),"agents":[agent("A"),agent("B")],"duration_seconds":60,"calls_per_agent":2,"max_transactions":2,"audit_file":"/private/new-audit.jsonl"})
    }
    #[test]
    fn local_sources_are_explicit_distinct_and_bounded_without_api_keys() {
        let mut v = config_value();
        for i in 0..2 {
            v["agents"][i] = json!({"profile":format!("/private/profile-{i}"),"key":format!("/private/key-{i}"),"decision_source":"local_harness","mailbox_directory":format!("/private/mailbox-{i}"),"decision_timeout_ms":60000,"public_strategy":"Choose independently."});
        }
        let c = config(&v).unwrap();
        assert_eq!(c.agents[0].source.label(), "local_harness");
        assert_eq!(c.agents[0].source.timeout_ms(), 60000);
        v["agents"][1]["mailbox_directory"] = v["agents"][0]["mailbox_directory"].clone();
        assert!(config(&v).is_err());
        v["agents"][1]["mailbox_directory"] = json!("/private/other");
        v["agents"][0]["decision_timeout_ms"] = json!(60001);
        assert!(config(&v).is_err());
        v["agents"][0]["decision_timeout_ms"] = json!(1000);
        v["agents"][0]["api_key_env"] = json!("UNUSED_KEY");
        assert!(config(&v).is_err());
    }
    #[test]
    fn config_is_explicit_distinct_and_bounded() {
        let mut v = config_value();
        assert!(config(&v).is_ok());
        v["agents"][1]["profile"] = v["agents"][0]["profile"].clone();
        assert!(config(&v).is_err());
        v = config_value();
        v["duration_seconds"] = json!(601);
        assert!(config(&v).is_err());
        v = config_value();
        v["max_transactions"] = json!(9);
        assert!(config(&v).is_err());
        v = config_value();
        v["legacy_llm_config"] = json!("forbidden");
        assert!(config(&v).is_err());
    }
    #[test]
    fn independent_mock_decisions_use_published_source_not_prescribed_trade() {
        let a = Pubkey::new_from_array([1; 32]).to_string();
        let b = Pubkey::new_from_array([2; 32]).to_string();
        let mut c = PublicContext {
            game: Pubkey::new_from_array([3; 32]).to_string(),
            faction: a.clone(),
            round: 2,
            phase: "Market".into(),
            phase_ends_at: 100,
            observed_cursor: 0,
            factions: vec![
                PublicFaction {
                    pda: a.clone(),
                    goods: 3,
                    cash: 10,
                },
                PublicFaction {
                    pda: b.clone(),
                    goods: 1,
                    cash: 20,
                },
            ],
            open_offers: vec![],
            incoming: vec![],
        };
        // Supplied completions test the pipeline only; they are not a live transcript.
        let completion = |decision: Value| {
            serde_json::to_vec(&json!({"choices":[{"finish_reason":"stop","message":{"content":decision.to_string()}}]})).unwrap()
        };
        let first = completion(
            json!({"action":{"type":"pass"},"public_message":{"text":"Are you looking to buy goods?","to_faction_pda":b,"in_reply_to":null}}),
        );
        let d = crate::dialogue_provider::parse_completion(&first, &c).unwrap();
        assert!(d.public_message.is_some());
        let text = d.public_message.unwrap().text;
        // Simulate the public journal receipt projection with its server identity.
        c.faction = b.clone();
        c.observed_cursor = 1;
        c.incoming.push(PublicMessage {
            id: "published-server-message".into(),
            author: a.clone(),
            recipient: Some(b),
            text,
        });
        let reply = completion(
            json!({"action":{"type":"pass"},"public_message":{"text":"I prefer to hold cash this round.","to_faction_pda":a,"in_reply_to":"published-server-message"}}),
        );
        let answer = crate::dialogue_provider::parse_completion(&reply, &c).unwrap();
        assert_eq!(answer.action, Action::Pass);
        assert_eq!(
            answer.public_message.unwrap().in_reply_to.as_deref(),
            Some("published-server-message")
        );
        c.incoming.clear();
        assert!(crate::dialogue_provider::parse_completion(&reply, &c).is_err());
    }
    #[test]
    fn admission_requires_time_for_work_before_both_deadlines() {
        assert!(admission_open(8_999, 20_000, 10));
        assert!(!admission_open(9_000, 20_000, 10));
        assert!(!admission_open(10_000, 20_000, 10));
        assert!(!admission_open(8_000, 9_000, 20));
        assert!(!admission_open(0, 20_000, 0));
        assert!(!admission_open(0, 20_000, -1));
        assert!(!admission_open(u64::MAX, u64::MAX, i64::MAX));
    }

    #[test]
    fn speech_quotas_and_duplicate_replies_are_author_scoped() {
        let author = Pubkey::new_from_array([1; 32]).to_string();
        let other = Pubkey::new_from_array([2; 32]).to_string();
        let mut c = PublicContext {
            game: Pubkey::new_from_array([3; 32]).to_string(),
            faction: author.clone(),
            round: 2,
            phase: "Market".into(),
            phase_ends_at: 100,
            observed_cursor: 0,
            factions: vec![],
            open_offers: vec![],
            incoming: vec![],
        };
        let mut message = dialogue::Message {
            text: "I will consider the offer.".into(),
            to_faction_pda: Some(other.clone()),
            in_reply_to: Some("incoming-1".into()),
        };
        let entry = |who: &str, round: u8, reply: Option<&str>| json!({"author_faction_pda":who,"round":round,"phase":"Market","phase_ends_at":100,"reply_to_message_id":reply});
        let mut feed = json!({"entries":[entry(&author,2,Some("incoming-1"))]});
        assert!(
            !speech_allowed(&feed, &c, Some(&message), 50),
            "already answered this message"
        );
        feed = json!({"entries":[entry(&other,2,Some("incoming-1"))]});
        assert!(
            speech_allowed(&feed, &c, Some(&message), 50),
            "another author does not consume reply budget"
        );
        message.in_reply_to = None;
        feed = json!({"entries":(0..4).map(|_|entry(&author,2,None)).collect::<Vec<_>>()});
        assert!(!speech_allowed(&feed, &c, Some(&message), 50));
        c.round = 3;
        assert!(
            speech_allowed(&feed, &c, Some(&message), 50),
            "a fresh context resets phase quota"
        );
        feed = json!({"entries":(0..24).map(|_|entry(&author,2,None)).collect::<Vec<_>>()});
        assert!(
            !speech_allowed(&feed, &c, Some(&message), 50),
            "game quota survives context change"
        );
        feed = json!({"entries":(0..24).map(|_|entry(&other,3,None)).collect::<Vec<_>>()});
        assert!(speech_allowed(&feed, &c, Some(&message), 50));
        assert!(!speech_allowed(&feed, &c, Some(&message), 100));
        assert!(!speech_allowed(&feed, &c, None, 50));
    }
}
