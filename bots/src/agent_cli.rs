//! A local signer for an external agent. No server-side custody or LLM integration.
use super::*;
use serde_json::{json, Value};
use std::{collections::BTreeMap, io::Read};

const DEVNET_GENESIS: &str = "EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG";
const USAGE: &str = "bots agent inspect --game PUBKEY [--wallet PUBKEY] | join --game PUBKEY --name NAME --key LOCAL_KEYPAIR | act --game PUBKEY --key LOCAL_KEYPAIR (one JSON action on stdin); ALASHI_RPC selects trusted devnet RPC";

type ResultJson = Result<Value, Value>;
fn error(code: &str, message: &str) -> Value {
    json!({"ok":false,"error":{"code":code,"message":message}})
}
fn options(args: &[String]) -> Result<BTreeMap<String, String>, Value> {
    let command = args.first().map(String::as_str).unwrap_or("");
    let allowed = match command {
        "inspect" => &["--game", "--wallet"][..],
        "join" => &["--game", "--key", "--name"][..],
        "act" => &["--game", "--key"][..],
        _ => return Err(error("usage", USAGE)),
    };
    let mut result = BTreeMap::new();
    for pair in args[1..].chunks(2) {
        if pair.len() != 2
            || !allowed.contains(&pair[0].as_str())
            || pair[1].starts_with("--")
            || result.insert(pair[0].clone(), pair[1].clone()).is_some()
        {
            return Err(error("usage", "unknown, duplicate or incomplete option"));
        }
    }
    Ok(result)
}
fn required<'a>(opts: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, Value> {
    opts.get(name)
        .map(String::as_str)
        .ok_or_else(|| error("usage", &format!("missing {name}")))
}
fn pubkey(text: &str) -> Result<Pubkey, Value> {
    text.parse()
        .map_err(|_| error("invalid_pubkey", "expected a base58 public key"))
}
fn devnet(rpc: &RpcClient) -> Result<(), Value> {
    let genesis = rpc
        .get_genesis_hash()
        .map_err(|_| error("rpc_unavailable", "cannot verify cluster genesis"))?;
    if genesis.to_string() != DEVNET_GENESIS {
        return Err(error(
            "wrong_network",
            "this agent CLI only supports Solana devnet",
        ));
    }
    Ok(())
}
fn read_key(path: &str) -> Result<Keypair, Value> {
    let mut file = std::fs::File::open(path)
        .map_err(|_| error("key_unavailable", "cannot open local keypair file"))?;
    let metadata = file
        .metadata()
        .map_err(|_| error("key_unavailable", "cannot inspect local keypair file"))?;
    if !metadata.is_file() || metadata.len() > 1024 {
        return Err(error(
            "invalid_key",
            "expected a small regular Solana keypair JSON file",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(error(
                "key_permissions",
                "local keypair must have mode 600 or stricter",
            ));
        }
    }
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|_| error("invalid_key", "cannot read keypair JSON"))?;
    let bytes: Vec<u8> = serde_json::from_str(&text)
        .map_err(|_| error("invalid_key", "expected a Solana keypair JSON byte array"))?;
    if bytes.len() != 64 {
        return Err(error("invalid_key", "expected a 64-byte Solana keypair"));
    }
    let key = Keypair::new_from_array(bytes[..32].try_into().unwrap());
    if key.to_bytes().as_slice() != bytes.as_slice() {
        return Err(error(
            "invalid_key",
            "keypair public half does not match its seed",
        ));
    }
    Ok(key)
}
fn checked_game(owner: Pubkey, address: &Pubkey, data: &[u8]) -> Result<state::Game, Value> {
    if owner != id() {
        return Err(error("wrong_owner", "Game account is not owned by Alashi"));
    }
    let game = state::Game::try_deserialize(&mut &data[..])
        .map_err(|_| error("invalid_game", "invalid Game account"))?;
    if game_pda(game.game_id) != *address {
        return Err(error("invalid_game", "Game PDA mismatch"));
    }
    Ok(game)
}
fn checked_faction(
    owner: Pubkey,
    address: &Pubkey,
    game: &Pubkey,
    data: &[u8],
) -> Result<state::Faction, Value> {
    if owner != id() {
        return Err(error(
            "wrong_owner",
            "Faction account is not owned by Alashi",
        ));
    }
    let faction = state::Faction::try_deserialize(&mut &data[..])
        .map_err(|_| error("invalid_faction", "invalid Faction account"))?;
    if faction.game != *game || faction_pda(game, &faction.wallet) != *address {
        return Err(error("invalid_faction", "Faction game or PDA mismatch"));
    }
    Ok(faction)
}
fn game_account(rpc: &RpcClient, key: &Pubkey) -> Result<state::Game, Value> {
    let account = rpc
        .get_account(key)
        .map_err(|_| error("game_unavailable", "cannot read Game account"))?;
    checked_game(account.owner, key, &account.data)
}
fn faction_account(rpc: &RpcClient, key: &Pubkey, game: &Pubkey) -> Result<state::Faction, Value> {
    let account = rpc.get_account(key).map_err(|_| {
        error(
            "faction_unavailable",
            "cannot read Faction account; join during Lobby first",
        )
    })?;
    checked_faction(account.owner, key, game, &account.data)
}
fn phase_name(phase: state::Phase) -> &'static str {
    match phase {
        state::Phase::Lobby => "lobby",
        state::Phase::Market => "market",
        state::Phase::Action => "action",
        state::Phase::Law => "law",
        state::Phase::Finished => "finished",
        state::Phase::Aborted => "aborted",
    }
}
fn action_types(game: &state::Game, faction: &state::Faction) -> Vec<&'static str> {
    if !faction.alive || game.epoch != 0 {
        return vec![];
    }
    match game.phase {
        state::Phase::Market if faction.acted_stamp != game.stamp() => vec!["sell", "buy"],
        state::Phase::Action if faction.acted_stamp != game.stamp() => {
            vec!["produce", "donkey", "bribe"]
        }
        state::Phase::Law if game.law_card != constants::NO_LAW => {
            let mut actions = vec![];
            if faction.voted_stamp != game.stamp() {
                actions.push("vote");
            }
            if game.president == faction.wallet && !game.veto_pending {
                actions.push("veto");
            }
            actions
        }
        _ => vec![],
    }
}
fn faction_json(key: &Pubkey, faction: &state::Faction, game: &state::Game) -> Value {
    json!({"faction":key.to_string(),"wallet":faction.wallet.to_string(),"name":faction.name,
        "cash":faction.cash.to_string(),"goods":faction.goods,"influence":faction.influence,
        "alive":faction.alive,"is_president":game.president == faction.wallet,
        "acted":faction.acted_stamp == game.stamp(),"voted":faction.voted_stamp == game.stamp(),
        "vote":match faction.vote {state::VoteChoice::Yes=>"yes",state::VoteChoice::No=>"no",state::VoteChoice::Abstain=>"abstain"},
        "available_action_types":action_types(game, faction)})
}
fn inspect(rpc: &RpcClient, key: Pubkey, wallet: Option<Pubkey>) -> ResultJson {
    use solana_rpc_client_api::{
        config::{RpcAccountInfoConfig, RpcProgramAccountsConfig},
        filter::{Memcmp, RpcFilterType},
        response::UiAccountEncoding,
    };
    let config = RpcProgramAccountsConfig {
        filters: Some(vec![RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
            8,
            key.as_ref(),
        ))]),
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::confirmed()),
            ..Default::default()
        },
        with_context: Some(false),
        sort_results: None,
    };
    let mut keys: Vec<Pubkey> = rpc
        .get_program_ui_accounts_with_config(&id(), config)
        .map_err(|_| error("rpc_unavailable", "cannot discover factions"))?
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    keys.sort();
    keys.insert(0, key);
    let response = rpc
        .get_multiple_accounts_with_commitment(&keys, CommitmentConfig::confirmed())
        .map_err(|_| error("rpc_unavailable", "cannot read match snapshot"))?;
    if response.value.len() != keys.len() {
        return Err(error("rpc_unavailable", "incomplete match snapshot"));
    }
    let account = response.value[0]
        .as_ref()
        .ok_or_else(|| error("game_unavailable", "Game account not found"))?;
    let game = checked_game(account.owner, &key, &account.data)?;
    let mut factions = vec![];
    let mut mine = Value::Null;
    for (address, account) in keys[1..].iter().zip(&response.value[1..]) {
        let account = account
            .as_ref()
            .ok_or_else(|| error("snapshot_changed", "faction disappeared; inspect again"))?;
        let faction = checked_faction(account.owner, address, &key, &account.data)?;
        let value = faction_json(address, &faction, &game);
        if Some(faction.wallet) == wallet {
            mine = value.clone();
        }
        factions.push(value);
    }
    if factions.len() != game.faction_count as usize {
        return Err(error(
            "snapshot_changed",
            "faction count changed during discovery; inspect again",
        ));
    }
    Ok(
        json!({"ok":true,"command":"inspect","network":"devnet","program_id":id().to_string(),
        "game":key.to_string(),"commitment":"confirmed","observed_slot":response.context.slot,
        "state":{"game_id":game.game_id.to_string(),"epoch":game.epoch,"phase":phase_name(game.phase),
            "round":game.round,"phase_ends_at":game.phase_ends_at,"phase_duration":game.phase_duration,
            "faction_count":game.faction_count,"entry_fee_lamports":game.entry_fee.to_string(),"account_lamports":account.lamports.to_string(),
            "settled":game.settled,"president_wallet":game.president.to_string(),"law_card":game.law_card,
            "veto_pending":game.veto_pending,"sold_this_round":game.sold_this_round,
            "active_tax_bps":game.active_tax_bps,"active_price_shift":game.active_price_shift,
            "active_boom":game.active_boom,"price_table":constants::PRICE_TABLE.iter().map(|n|n.to_string()).collect::<Vec<_>>(),
            "factions":factions},"your_faction":mine,
        "units":{"cash":"game units; 1000000 = 1 peso","price_table":"whole pesos per unit","entry_fee_lamports":"devnet SOL; 1000000000 = 1 SOL","account_lamports":"devnet lamports held by Game, including rent reserve"},
        "note":"available_action_types are phase/stamp hints; balances and targets are validated by the program"}),
    )
}
fn integer(value: &Value, field: &str) -> Result<u64, Value> {
    value
        .get(field)
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .ok_or_else(|| {
            error(
                "invalid_action",
                &format!("{field} must be an unsigned integer or decimal string"),
            )
        })
}
fn action_ix(
    value: &Value,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
) -> Result<Instruction, Value> {
    let body = value
        .as_object()
        .ok_or_else(|| error("invalid_action", "action must be a JSON object"))?;
    if body.keys().any(|key| key != "action" && key != "params") {
        return Err(error("invalid_action", "unknown top-level action field"));
    }
    let action = body
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| error("invalid_action", "missing action name"))?;
    let params = body.get("params").cloned().unwrap_or_else(|| json!({}));
    let object = params
        .as_object()
        .ok_or_else(|| error("invalid_action", "params must be an object"))?;
    let fields = match action {
        "sell" | "buy" => &["units"][..],
        "bribe" => &["to", "amount"][..],
        "vote" => &["choice"][..],
        "produce" | "donkey" | "veto" => &[][..],
        _ => return Err(error("invalid_action", "unsupported classic action")),
    };
    if object.keys().any(|key| !fields.contains(&key.as_str())) {
        return Err(error("invalid_action", "unknown action parameter"));
    }
    Ok(match action {
        "sell" | "buy" => {
            let units = u16::try_from(integer(&params, "units")?)
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| error("invalid_action", "units must be in 1..65535"))?;
            if action == "sell" {
                ix_sell(units, player, game, faction)
            } else {
                ix_buy(units, player, game, faction)
            }
        }
        "produce" => ix_produce(player, game, faction),
        "donkey" => ix_donkey(player, game, faction),
        "veto" => ix_veto(player, game, faction),
        "bribe" => {
            let to =
                pubkey(params["to"].as_str().ok_or_else(|| {
                    error("invalid_action", "bribe requires target faction pubkey")
                })?)?;
            let amount = integer(&params, "amount")?;
            if amount == 0 || to == faction {
                return Err(error(
                    "invalid_action",
                    "bribe requires a positive amount and another faction",
                ));
            }
            ix_bribe(amount, player, game, faction, to)
        }
        "vote" => ix_vote(
            match params["choice"].as_str() {
                Some("yes") => state::VoteChoice::Yes,
                Some("no") => state::VoteChoice::No,
                Some("abstain") => state::VoteChoice::Abstain,
                _ => return Err(error("invalid_action", "choice must be yes, no or abstain")),
            },
            player,
            game,
            faction,
        ),
        _ => unreachable!(),
    })
}
fn execute(args: &[String], input: Option<&str>, rpc: &RpcClient) -> ResultJson {
    let opts = options(args)?;
    let command = args[0].as_str();
    let game_key = pubkey(required(&opts, "--game")?)?;
    // Inspect is also devnet-labelled, so it uses the same guard as writes.
    devnet(rpc)?;
    if command == "inspect" {
        return inspect(
            rpc,
            game_key,
            opts.get("--wallet").map(|s| pubkey(s)).transpose()?,
        );
    }
    let key = read_key(required(&opts, "--key")?)?;
    let player = key.pubkey();
    let faction = faction_pda(&game_key, &player);
    let game = game_account(rpc, &game_key)?;
    if game.epoch != 0 {
        return Err(error(
            "unsupported_epoch",
            "this CLI currently supports classic matches only",
        ));
    }
    let ix = if command == "join" {
        let name = required(&opts, "--name")?;
        if name.is_empty() || name.len() > 16 {
            return Err(error("invalid_name", "name must contain 1..16 UTF-8 bytes"));
        }
        let existing = rpc
            .get_account_with_commitment(&faction, CommitmentConfig::confirmed())
            .map_err(|_| error("rpc_unavailable", "cannot check existing faction"))?;
        if let Some(account) = existing.value {
            checked_faction(account.owner, &faction, &game_key, &account.data)?;
            return Ok(
                json!({"ok":true,"command":"join","network":"devnet","game":game_key.to_string(),
                "wallet":player.to_string(),"faction":faction.to_string(),"status":"already_joined","receipt":null}),
            );
        }
        if game.phase != state::Phase::Lobby {
            return Err(error("wrong_phase", "join is only possible during Lobby"));
        }
        if game.faction_count >= constants::MAX_FACTIONS {
            return Err(error("game_full", "all faction slots are occupied"));
        }
        ix_join(name, player, game_key, faction)
    } else {
        let value: Value = serde_json::from_str(input.unwrap_or("")).map_err(|_| {
            error(
                "invalid_action",
                "stdin must contain exactly one JSON action",
            )
        })?;
        let ix = action_ix(&value, player, game_key, faction)?;
        let own = faction_account(rpc, &faction, &game_key)?;
        let action = value["action"].as_str().unwrap_or("");
        if !action_types(&game, &own).contains(&action) {
            return Err(error(
                "action_unavailable",
                "wrong phase, already acted/voted, or no permission; inspect state",
            ));
        }
        if action == "bribe" {
            let target = pubkey(value["params"]["to"].as_str().unwrap_or(""))?;
            faction_account(rpc, &target, &game_key)?;
        }
        ix
    };
    match send_ix_confirmed(rpc, &key, ix) {
        Ok(receipt) => Ok(
            json!({"ok":true,"command":command,"network":"devnet","program_id":id().to_string(),
            "game":game_key.to_string(),"wallet":player.to_string(),"faction":faction.to_string(),"receipt":receipt}),
        ),
        Err(receipt) => Err(
            json!({"ok":false,"command":command,"network":"devnet","game":game_key.to_string(),
            "error":{"code":"transaction_not_confirmed","message":"inspect receipt status and chain state before retrying"},"receipt":receipt}),
        ),
    }
}
pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "{}",
            json!({"ok":true,"usage":USAGE,"actions":["sell","buy","produce","donkey","bribe","vote","veto"]})
        );
        return 0;
    }
    let input = if args.first().map(String::as_str) == Some("act") {
        let mut text = String::new();
        if std::io::stdin()
            .take(4097)
            .read_to_string(&mut text)
            .is_err()
            || text.len() > 4096
        {
            println!(
                "{}",
                error(
                    "invalid_action",
                    "stdin must be UTF-8 JSON, at most 4096 bytes"
                )
            );
            return 2;
        }
        Some(text)
    } else {
        None
    };
    let rpc = RpcClient::new_with_timeout_and_commitment(
        rpc_url(),
        Duration::from_secs(10),
        CommitmentConfig::confirmed(),
    );
    match execute(args, input.as_deref(), &rpc) {
        Ok(value) => {
            println!("{value}");
            0
        }
        Err(value) => {
            println!("{value}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anchor_lang::AccountSerialize;
    use solana_rpc_client_api::request::RpcRequest;

    #[test]
    fn account_validation_rejects_wrong_owner_pda_and_foreign_faction() {
        let game = state::Game {
            game_id: 42,
            ..Default::default()
        };
        let address = game_pda(42);
        let mut data = vec![];
        game.try_serialize(&mut data).unwrap();
        assert!(checked_game(id(), &address, &data).is_ok());
        assert!(checked_game(Pubkey::default(), &address, &data).is_err());
        assert!(checked_game(id(), &game_pda(43), &data).is_err());
        let wallet = Pubkey::new_unique();
        let faction = state::Faction {
            game: address,
            wallet,
            ..Default::default()
        };
        let mut data = vec![];
        faction.try_serialize(&mut data).unwrap();
        assert!(checked_faction(id(), &faction_pda(&address, &wallet), &address, &data).is_ok());
        assert!(
            checked_faction(id(), &faction_pda(&address, &wallet), &game_pda(43), &data).is_err()
        );
    }
    #[test]
    fn action_validation_preserves_instruction_bytes_and_rejects_typos() {
        let player = Pubkey::new_unique();
        let game = game_pda(42);
        let faction = faction_pda(&game, &player);
        let built = action_ix(
            &json!({"action":"sell","params":{"units":2}}),
            player,
            game,
            faction,
        )
        .unwrap();
        let expected = ix_sell(2, player, game, faction);
        assert_eq!(built.data, expected.data);
        assert_eq!(built.accounts, expected.accounts);
        for value in [
            json!({"action":"sell","params":{"units":-1}}),
            json!({"action":"sell","params":{"units":65536}}),
            json!({"action":"sell","params":{"units":2,"unit":3}}),
            json!({"action":"vote","params":{"choice":"maybe"}}),
            json!({"action":"produce","secret":"do not echo"}),
            json!({"action":"produce","params":null}),
        ] {
            assert!(action_ix(&value, player, game, faction).is_err());
        }
        assert!(action_ix(&json!({"action":"bribe","params":{"to":Pubkey::new_unique().to_string(),"amount":"5000000"}}),player,game,faction).is_ok());
    }
    #[test]
    fn phase_hints_use_current_stamp_and_veto_survives_own_vote() {
        let wallet = Pubkey::new_unique();
        let mut game = state::Game {
            phase: state::Phase::Action,
            round: 2,
            ..Default::default()
        };
        let mut faction = state::Faction {
            wallet,
            alive: true,
            acted_stamp: 9,
            ..Default::default()
        };
        assert!(action_types(&game, &faction).contains(&"produce"));
        faction.acted_stamp = game.stamp();
        assert!(action_types(&game, &faction).is_empty());
        game.phase = state::Phase::Law;
        game.law_card = 1;
        game.president = wallet;
        faction.voted_stamp = game.stamp();
        assert_eq!(action_types(&game, &faction), vec!["veto"]);
    }
    #[test]
    fn genesis_guard_fails_closed_without_signing() {
        for genesis in [
            "11111111111111111111111111111111",
            "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d",
        ] {
            let rpc = RpcClient::new_mock_with_mocks(
                "succeeds",
                [(RpcRequest::GetGenesisHash, json!(genesis))].into(),
            );
            assert!(devnet(&rpc).is_err());
        }
        let rpc = RpcClient::new_mock_with_mocks(
            "succeeds",
            [(RpcRequest::GetGenesisHash, json!(DEVNET_GENESIS))].into(),
        );
        assert!(devnet(&rpc).is_ok());
    }
    #[test]
    fn receipt_requires_metadata_and_reports_chain_failure() {
        assert!(receipt_result("sig", &Value::Null).is_none());
        assert!(receipt_result("sig", &json!({"slot":1,"meta":null})).is_none());
        assert!(receipt_result("sig", &json!({"slot":1,"meta":{}})).is_none());
        assert!(receipt_result(
            "sig",
            &json!({"slot":1,"meta":{"err":{"InstructionError":[0,"Custom"]}}})
        )
        .unwrap()
        .is_err());
        assert_eq!(
            receipt_result("sig", &json!({"slot":1,"meta":{"err":null}}))
                .unwrap()
                .unwrap()["status"],
            "confirmed"
        );
        let rpc = RpcClient::new_mock_with_mocks(
            "succeeds",
            [(RpcRequest::GetTransaction, Value::Null)].into(),
        );
        assert_eq!(
            wait_receipt(
                &rpc,
                &solana_signature::Signature::default(),
                Duration::ZERO
            )
            .unwrap_err()["status"],
            "unknown"
        );
    }
    #[test]
    fn event_capture_excludes_nested_foreign_logs() {
        let logs = vec![
            json!(format!("Program {} invoke [1]", id())),
            json!("Program data: ours"),
            json!("Program Foreign invoke [2]"),
            json!("Program data: foreign"),
            json!("Program Foreign success"),
            json!("Program data: ours2"),
            json!(format!("Program {} success", id())),
            json!("Program data: outside"),
        ];
        assert_eq!(
            alashi_event_logs(&logs),
            vec!["Program data: ours", "Program data: ours2"]
        );
    }
    #[test]
    fn host_timing_preserves_default_and_covers_long_phases() {
        assert_eq!(host_timing(&[]).unwrap(), (15, 720));
        assert_eq!(
            host_timing(&["--phase-duration".into(), "60".into()]).unwrap(),
            (60, 1500)
        );
        assert!(host_timing(&[
            "--phase-duration".into(),
            "60".into(),
            "--timeout".into(),
            "720".into()
        ])
        .is_err());
        assert!(host_timing(&["--phase-duration".into()]).is_err());
    }
}
