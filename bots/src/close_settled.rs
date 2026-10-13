//! `bots close-settled`: return rent by closing settled games through `close_game`.
//! A game is closed only after chain-api has served it as terminal, because that
//! first terminal read is what writes the viewer archive.
use super::*;
use anchor_lang::Discriminator;
use serde_json::{json, Value};

const USAGE: &str = "bots close-settled --key LOCAL_KEYPAIR [--chain-api https://alashi.network] [--min-age-secs 3600] [--keep GAME_PDA]... [--dry-run]; ALASHI_RPC selects devnet RPC";
/// The home page replays this game; it is never closed.
const HOME_GAME: &str = "GqHZBaWuJDNERi8xJHXnYF5eWBsLSmEXcAGJYAP94M1b";

/// Pure close/skip decision. `None` means that input has not been fetched yet:
/// the caller checks cheap on-chain fields first and passes both on the final call.
#[allow(clippy::too_many_arguments)]
fn decide(
    game: &state::Game,
    address: &Pubkey,
    crank: &Pubkey,
    keep: &[Pubkey],
    now: i64,
    min_age: i64,
    factions_complete: Option<bool>,
    chain: Option<&Value>,
) -> Result<(), &'static str> {
    if keep.contains(address) {
        return Err("keep");
    }
    if !game.settled {
        return Err("not_settled");
    }
    if !matches!(game.phase, state::Phase::Finished | state::Phase::Aborted) {
        return Err("not_terminal_phase");
    }
    if game.admin != *crank {
        return Err("admin_mismatch");
    }
    if game.phase_ends_at > now - min_age {
        return Err("too_recent");
    }
    if factions_complete == Some(false) {
        return Err("incomplete_factions");
    }
    if let Some(v) = chain {
        if v.is_null() {
            return Err("chain_api_unavailable");
        }
        if v["ok"] != true
            || v["game_pda"] != address.to_string()
            || v["game"]["settled"] != true
            || v["history_complete"] != true
        {
            return Err("archive_not_terminal");
        }
    }
    Ok(())
}

fn chain_view(base: &str, address: &Pubkey) -> Option<Value> {
    session_cli::http_json(base, &format!("/chain/devnet/games/{address}"), None).ok()
}

/// Faction PDAs, wallets and lamports, or None if the set is not exactly `faction_count`.
fn faction_set(
    rpc: &RpcClient,
    address: &Pubkey,
    count: u8,
) -> Option<(Vec<Pubkey>, Vec<Pubkey>, Vec<u64>)> {
    let mut keys = program_keys(rpc, 8, address.as_ref()).ok()?;
    keys.sort();
    if keys.len() != count as usize {
        return None;
    }
    let accounts = rpc.get_multiple_accounts(&keys).ok()?;
    let mut wallets = vec![];
    let mut lamports = vec![];
    for (key, account) in keys.iter().zip(accounts) {
        let account = account?;
        let faction =
            agent_cli::checked_faction(account.owner, key, address, &account.data).ok()?;
        wallets.push(faction.wallet);
        lamports.push(account.lamports);
    }
    Some((keys, wallets, lamports))
}

fn ix_close_game(
    crank: Pubkey,
    game: Pubkey,
    admin: Pubkey,
    factions: &[Pubkey],
    wallets: &[Pubkey],
) -> Instruction {
    let mut metas = accounts::CloseGame { crank, game, admin }.to_account_metas(None);
    metas.extend(
        factions
            .iter()
            .chain(wallets)
            .map(|k| AccountMeta::new(*k, false)),
    );
    Instruction::new_with_bytes(id(), &instruction::CloseGame {}.data(), metas)
}

fn options(args: &[String]) -> Result<(String, String, i64, Vec<Pubkey>, bool), String> {
    let mut keep = vec![HOME_GAME.parse::<Pubkey>().unwrap()];
    let (mut key, mut base, mut min_age, mut dry_run) =
        (None, "https://alashi.network".to_string(), 3600i64, false);
    let mut i = 0;
    while i < args.len() {
        let value = || {
            args.get(i + 1)
                .cloned()
                .ok_or(format!("{} requires a value", args[i]))
        };
        match args[i].as_str() {
            "--dry-run" => {
                dry_run = true;
                i += 1;
                continue;
            }
            "--key" => key = Some(value()?),
            "--chain-api" => base = value()?,
            "--min-age-secs" => {
                min_age = value()?
                    .parse()
                    .ok()
                    .filter(|v| *v >= 0)
                    .ok_or("--min-age-secs must be a non-negative integer")?
            }
            "--keep" => keep.push(
                value()?
                    .parse()
                    .map_err(|_| "--keep must be a public key")?,
            ),
            other => return Err(format!("unknown argument {other}")),
        }
        i += 2;
    }
    Ok((key.ok_or("--key required")?, base, min_age, keep, dry_run))
}

pub fn run(args: &[String]) -> i32 {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!("{USAGE}");
        return 0;
    }
    let (key_path, base, min_age, keep, dry_run) = match options(args) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}\n{USAGE}");
            return 2;
        }
    };
    let Ok(signer) = agent_cli::read_key(&key_path) else {
        eprintln!("key must be an existing mode-600 keypair");
        return 2;
    };
    let rpc = RpcClient::new_with_timeout_and_commitment(
        rpc_url(),
        Duration::from_secs(10),
        CommitmentConfig::confirmed(),
    );
    if agent_cli::devnet(&rpc).is_err() {
        eprintln!("close-settled requires Solana devnet");
        return 2;
    }
    let games = match program_keys(&rpc, 0, state::Game::DISCRIMINATOR) {
        Ok(keys) => keys,
        Err(e) => {
            eprintln!("game discovery failed: {e}");
            return 1;
        }
    };
    let crank = signer.pubkey();
    let mut failed = false;
    for chunk in games.chunks(100) {
        let accounts = match rpc.get_multiple_accounts(chunk) {
            Ok(a) => a,
            Err(_) => {
                for address in chunk {
                    println!(
                        "{}",
                        json!({"game":address.to_string(),"decision":"error","error":"game_read_failed"})
                    );
                }
                failed = true;
                continue;
            }
        };
        for (address, account) in chunk.iter().zip(accounts) {
            let line = |decision: &str| json!({"game":address.to_string(),"decision":decision});
            let Some(account) = account else {
                println!("{}", line("skip:account_gone"));
                continue;
            };
            let Ok(game) = agent_cli::checked_game(account.owner, address, &account.data) else {
                println!("{}", line("skip:invalid_game"));
                continue;
            };
            let now = now();
            let check = |factions: Option<bool>, chain: Option<&Value>| {
                decide(&game, address, &crank, &keep, now, min_age, factions, chain)
            };
            if let Err(reason) = check(None, None) {
                println!("{}", line(&format!("skip:{reason}")));
                continue;
            }
            let set = faction_set(&rpc, address, game.faction_count);
            if let Err(reason) = check(Some(set.is_some()), None) {
                println!("{}", line(&format!("skip:{reason}")));
                continue;
            }
            let (factions, wallets, lamports) = set.unwrap();
            // This GET is what creates the viewer archive on chain-api.
            let view = chain_view(&base, address);
            if let Err(reason) = check(Some(true), Some(view.as_ref().unwrap_or(&Value::Null))) {
                println!("{}", line(&format!("skip:{reason}")));
                continue;
            }
            let mut out = line(if dry_run { "dry_run" } else { "closed" });
            out["lamports_to_admin"] = json!(account.lamports);
            out["lamports_to_wallets"] = wallets
                .iter()
                .zip(&lamports)
                .map(|(w, l)| json!({"wallet":w.to_string(),"lamports":l}))
                .collect();
            if !dry_run {
                let ix = ix_close_game(crank, *address, game.admin, &factions, &wallets);
                match send_ix_confirmed(&rpc, &signer, ix) {
                    Ok(receipt) => out["signature"] = receipt["signature"].clone(),
                    Err(err) => {
                        failed = true;
                        out["decision"] = json!("error");
                        out["error"] = err;
                        out.as_object_mut().unwrap().remove("lamports_to_admin");
                        out.as_object_mut().unwrap().remove("lamports_to_wallets");
                    }
                }
                if out["decision"] == "closed"
                    && chain_view(&base, address)
                        .is_none_or(|v| check(Some(true), Some(&v)).is_err())
                {
                    failed = true;
                    out["archive"] = json!("archive_missing_after_close");
                    eprintln!("[ERROR] archive_missing_after_close {address}: chain-api no longer serves this game");
                }
            }
            println!("{out}");
        }
    }
    i32::from(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closes_only_old_complete_operator_games_with_terminal_archive() {
        let crank = Pubkey::new_unique();
        let address = Pubkey::new_unique();
        let game = state::Game {
            admin: crank,
            phase: state::Phase::Finished,
            phase_ends_at: 1_000,
            settled: true,
            ..Default::default()
        };
        let terminal = json!({"ok":true,"game_pda":address.to_string(),
            "game":{"settled":true},"history_complete":true});
        let at = |game: &state::Game, now, keep: &[Pubkey], factions, chain| {
            decide(game, &address, &crank, keep, now, 3600, factions, chain)
        };
        assert_eq!(at(&game, 4_600, &[], Some(true), Some(&terminal)), Ok(()));
        assert_eq!(
            at(&game, 4_599, &[], Some(true), Some(&terminal)),
            Err("too_recent")
        );
        let other = state::Game {
            admin: Pubkey::new_unique(),
            ..game.clone()
        };
        assert_eq!(
            at(&other, 9_000, &[], Some(true), Some(&terminal)),
            Err("admin_mismatch")
        );
        assert_eq!(
            at(&game, 9_000, &[address], Some(true), Some(&terminal)),
            Err("keep")
        );
        assert_eq!(
            at(&game, 9_000, &[], Some(false), Some(&terminal)),
            Err("incomplete_factions")
        );
        let partial = json!({"ok":true,"game_pda":address.to_string(),
            "game":{"settled":true},"history_complete":false});
        assert_eq!(
            at(&game, 9_000, &[], Some(true), Some(&partial)),
            Err("archive_not_terminal")
        );
        assert_eq!(
            at(&game, 9_000, &[], Some(true), Some(&Value::Null)),
            Err("chain_api_unavailable")
        );
        let live = state::Game {
            settled: false,
            ..game.clone()
        };
        assert_eq!(
            at(&live, 9_000, &[], Some(true), Some(&terminal)),
            Err("not_settled")
        );
        let market = state::Game {
            phase: state::Phase::Market,
            ..game.clone()
        };
        assert_eq!(
            at(&market, 9_000, &[], Some(true), Some(&terminal)),
            Err("not_terminal_phase")
        );
        assert!(options(&["--key".into(), "k".into()])
            .unwrap()
            .3
            .contains(&HOME_GAME.parse().unwrap()));
    }
}
