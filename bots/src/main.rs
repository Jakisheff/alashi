use {
    alashi::{accounts, constants, id, instruction, state},
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    solana_commitment_config::CommitmentConfig,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_rpc_client::rpc_client::RpcClient,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
    std::{
        path::Path,
        thread::sleep,
        time::{Duration, SystemTime, UNIX_EPOCH},
    },
};

const RPC_URL: &str = "https://api.devnet.solana.com";
const ENTRY_FEE: u64 = 50_000_000;
const PHASE_DURATION: i64 = 15;
const KEYS_DIR: &str = "bots/keys";

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn chain_now(rpc: &RpcClient) -> i64 {
    match rpc.get_slot() {
        Ok(slot) => match rpc.get_block_time(slot) {
            Ok(t) => t,
            Err(e) => {
                println!("[ERROR] get_block_time({slot}): {e}");
                let _ = std::io::Write::flush(&mut std::io::stdout());
                0
            }
        },
        Err(e) => {
            println!("[ERROR] get_slot: {e}");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            0
        }
    }
}

fn game_pda(game_id: u64) -> Pubkey {
    let seed = game_id.to_le_bytes();
    Pubkey::find_program_address(&[constants::GAME_SEED, seed.as_ref()], &id()).0
}

fn faction_pda(game: &Pubkey, wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[constants::FACTION_SEED, game.as_ref(), wallet.as_ref()],
        &id(),
    )
    .0
}

fn load_or_create(path: &str) -> Keypair {
    if Path::new(path).exists() {
        let bytes: Vec<u8> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let seed: [u8; 32] = bytes[..32].try_into().expect("keypair file must hold 64 bytes");
        Keypair::new_from_array(seed)
    } else {
        let kp = Keypair::new();
        std::fs::create_dir_all(KEYS_DIR).unwrap();
        std::fs::write(path, serde_json::to_string(&kp.to_bytes().to_vec()).unwrap()).unwrap();
        kp
    }
}

fn send_ix(rpc: &RpcClient, signer: &Keypair, ix: Instruction) -> bool {
    let bh = match rpc.get_latest_blockhash() {
        Ok(bh) => bh,
        Err(e) => {
            println!("  blockhash err: {e}");
            return false;
        }
    };
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &bh);
    let tx = match VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]) {
        Ok(tx) => tx,
        Err(e) => {
            println!("  tx build err: {e}");
            return false;
        }
    };
    match rpc.send_transaction(&tx) {
        Ok(sig) => {
            println!("  tx https://explorer.solana.com/tx/{sig}?cluster=devnet");
            let _ = std::io::Write::flush(&mut std::io::stdout());
            true
        }
        Err(e) => {
            println!("  tx err: {e}");
            false
        }
    }
}

fn fetch_game(rpc: &RpcClient, game: &Pubkey) -> Option<state::Game> {
    match rpc.get_account(game) {
        Ok(acc) => {
            let mut d: &[u8] = &acc.data;
            Some(state::Game::try_deserialize(&mut d).ok()?)
        }
        Err(_) => None,
    }
}

fn fetch_faction(rpc: &RpcClient, faction: &Pubkey) -> Option<state::Faction> {
    match rpc.get_account(faction) {
        Ok(acc) => {
            let mut d: &[u8] = &acc.data;
            Some(state::Faction::try_deserialize(&mut d).ok()?)
        }
        Err(_) => None,
    }
}

struct Bot {
    kp: Keypair,
    faction: Pubkey,
    name: &'static str,
    acted: bool,
    voted: bool,
    goods: u16,
}

fn ensure_funds(rpc: &RpcClient, who: &str, kp: &Keypair) {
    let bal = rpc.get_balance(&kp.pubkey()).unwrap_or(0);
    println!("{who} balance: {bal} lamports");
    if bal < 200_000_000 {
        println!("{who} requesting airdrop 1 SOL...");
        match rpc.request_airdrop(&kp.pubkey(), 1_000_000_000) {
            Ok(sig) => {
                println!("  airdrop tx {sig}");
                sleep(Duration::from_secs(8));
            }
            Err(e) => println!("  airdrop err: {e}"),
        }
    }
}

fn ix_initialize(admin: Pubkey, game: Pubkey, game_id: u64) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Initialize {
            game_id,
            entry_fee: ENTRY_FEE,
            phase_duration: PHASE_DURATION,
        }
        .data(),
        accounts::Initialize {
            admin,
            game,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_join(name: &str, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Join {
            name: name.to_string(),
        }
        .data(),
        accounts::Join {
            player,
            game,
            faction,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_produce(player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Produce {}.data(),
        accounts::Produce {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_sell(units: u16, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Sell { units }.data(),
        accounts::Sell {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_vote(
    choice: state::VoteChoice,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Vote { choice }.data(),
        accounts::Vote {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_advance(crank: Pubkey, game: Pubkey, factions: &[Pubkey]) -> Instruction {
    let mut metas = accounts::Advance { crank, game }.to_account_metas(None);
    for f in factions {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new_readonly(
            *f, false,
        ));
    }
    Instruction::new_with_bytes(id(), &instruction::Advance {}.data(), metas)
}

fn rpc_url() -> String {
    std::env::var("ALASHI_RPC").unwrap_or_else(|_| "https://api.devnet.solana.com".to_string())
}

fn wait_account(rpc: &RpcClient, key: &Pubkey, label: &str) -> bool {
    for _ in 0..40 {
        if rpc.get_account(key).is_ok() {
            return true;
        }
        sleep(Duration::from_millis(500));
    }
    println!("[ERROR] account {label} не появился за 20с: {key}");
    false
}

fn main() {
    let rpc = RpcClient::new_with_commitment(rpc_url(), CommitmentConfig::confirmed());

    let bot1_kp = load_or_create(&format!("{KEYS_DIR}/bot1.json"));
    let bot2_kp = load_or_create(&format!("{KEYS_DIR}/bot2.json"));
    println!("rpc: {}", rpc_url());
    println!("bot1: {}", bot1_kp.pubkey());
    println!("bot2: {}", bot2_kp.pubkey());

    ensure_funds(&rpc, "bot1", &bot1_kp);
    ensure_funds(&rpc, "bot2", &bot2_kp);

    let game_id = now() as u64;
    let game = game_pda(game_id);
    let mut bots = vec![
        Bot {
            faction: faction_pda(&game, &bot1_kp.pubkey()),
            kp: bot1_kp,
            name: "Zhora",
            acted: false,
            voted: false,
            goods: 0,
        },
        Bot {
            faction: faction_pda(&game, &bot2_kp.pubkey()),
            kp: bot2_kp,
            name: "Osol",
            acted: false,
            voted: false,
            goods: 0,
        },
    ];
    let faction_keys: Vec<Pubkey> = bots.iter().map(|b| b.faction).collect();

    println!("=== ALASHI devnet party ===");
    println!("program: {}", id());
    println!("game {game_id}: https://explorer.solana.com/address/{game}?cluster=devnet");

    if !send_ix(
        &rpc,
        &bots[0].kp,
        ix_initialize(bots[0].kp.pubkey(), game, game_id),
    ) {
        println!("[ERROR] initialize не отправился");
        return;
    }
    wait_account(&rpc, &game, "game");
    send_ix(
        &rpc,
        &bots[0].kp,
        ix_join("Zhora", bots[0].kp.pubkey(), game, bots[0].faction),
    );
    wait_account(&rpc, &bots[0].faction, "faction1");
    send_ix(
        &rpc,
        &bots[1].kp,
        ix_join("Osol", bots[1].kp.pubkey(), game, bots[1].faction),
    );
    wait_account(&rpc, &bots[1].faction, "faction2");

    let started = now();
    let mut last_stamp: Option<u16> = None;
    let mut hb = 0u32;
    loop {
        hb += 1;
        if now() - started > 12 * 60 {
            println!("timeout, party unfinished");
            break;
        }
        let tnow = chain_now(&rpc);
        if hb % 5 == 1 {
            println!("[hb] tnow={tnow}");
            let _ = std::io::Write::flush(&mut std::io::stdout());
        }
        let g = match fetch_game(&rpc, &game) {
            Some(g) => g,
            None => {
                sleep(Duration::from_secs(4));
                continue;
            }
        };
        let stamp = g.stamp();
        if last_stamp != Some(stamp) {
            for b in bots.iter_mut() {
                b.acted = false;
                b.voted = false;
            }
            last_stamp = Some(stamp);
        }

        match g.phase {
            state::Phase::Lobby => {
                if tnow + 2 >= g.phase_ends_at {
                    println!("[lobby] advance");
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_advance(bots[0].kp.pubkey(), game, &faction_keys),
                    );
                }
            }
            state::Phase::Market => {
                for b in bots.iter_mut() {
                    if !b.acted && b.goods > 0 {
                        println!("[market r{}] {} sells {} goods", g.round, b.name, b.goods);
                        if send_ix(
                            &rpc,
                            &b.kp,
                            ix_sell(b.goods, b.kp.pubkey(), game, b.faction),
                        ) {
                            b.acted = true;
                            b.goods = 0;
                        }
                    }
                }
                if tnow >= g.phase_ends_at {
                    println!("[market r{}] advance", g.round);
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_advance(bots[0].kp.pubkey(), game, &faction_keys),
                    );
                }
            }
            state::Phase::Action => {
                for b in bots.iter_mut() {
                    if !b.acted {
                        println!("[action r{}] {} produces", g.round, b.name);
                        if send_ix(&rpc, &b.kp, ix_produce(b.kp.pubkey(), game, b.faction)) {
                            b.acted = true;
                            b.goods += constants::PRODUCE_YIELD;
                        }
                    }
                }
                if tnow >= g.phase_ends_at {
                    println!("[action r{}] advance", g.round);
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_advance(bots[0].kp.pubkey(), game, &faction_keys),
                    );
                }
            }
            state::Phase::Law => {
                for (i, b) in bots.iter_mut().enumerate() {
                    if !b.voted {
                        let choice = if i == 0 {
                            state::VoteChoice::Yes
                        } else {
                            state::VoteChoice::No
                        };
                        println!("[law r{}] {} votes {:?}", g.round, b.name, choice);
                        if send_ix(&rpc, &b.kp, ix_vote(choice, b.kp.pubkey(), game, b.faction)) {
                            b.voted = true;
                        }
                    }
                }
                if tnow >= g.phase_ends_at {
                    println!("[law r{}] advance", g.round);
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_advance(bots[0].kp.pubkey(), game, &faction_keys),
                    );
                }
            }
            state::Phase::Finished => {
                println!("=== FINISHED ===");
                println!(
                    "rounds done, laws_passed: {}, last vote {}:{}",
                    g.laws_passed, g.yes_influence, g.no_influence
                );
                for b in bots.iter() {
                    if let Some(f) = fetch_faction(&rpc, &b.faction) {
                        println!(
                            "faction {} ({}): cash {} lamports, goods {}, influence {}",
                            f.name,
                            b.faction,
                            f.cash,
                            f.goods,
                            f.influence
                        );
                    }
                }
                let bank = rpc.get_balance(&game).unwrap_or(0);
                println!("bank: {bank} lamports");
                println!("game: https://explorer.solana.com/address/{game}?cluster=devnet");
                break;
            }
        }

        let _ = std::io::Write::flush(&mut std::io::stdout());
        sleep(Duration::from_secs(2));
    }
}
