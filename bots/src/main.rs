mod llm;

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
const PRICE_TABLE: [u64; 16] = [12, 10, 9, 8, 7, 6, 5, 4, 3, 3, 2, 2, 2, 1, 1, 1];

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
        let seed: [u8; 32] = bytes[..32]
            .try_into()
            .expect("keypair file must hold 64 bytes");
        Keypair::new_from_array(seed)
    } else {
        let kp = Keypair::new();
        std::fs::create_dir_all(KEYS_DIR).unwrap();
        std::fs::write(
            path,
            serde_json::to_string(&kp.to_bytes().to_vec()).unwrap(),
        )
        .unwrap();
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
    bribed: bool,
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
            entropy_mode: 0,
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
            system_program: anchor_lang::solana_program::system_program::ID,
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

fn ix_bribe(
    amount: u64,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    target: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Bribe { amount }.data(),
        accounts::Bribe {
            player,
            game,
            faction,
            target,
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
    let mut metas = accounts::Advance {
        crank,
        game,
        hashes: constants::SLOT_HASHES_ID,
    }
    .to_account_metas(None);
    for f in factions {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(
            *f, false,
        ));
    }
    Instruction::new_with_bytes(id(), &instruction::Advance {}.data(), metas)
}

fn ix_buy(units: u16, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Buy { units }.data(),
        accounts::BuyGoods {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_veto(player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::Veto {}.data(),
        accounts::Veto {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_donkey(player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        id(),
        &instruction::BuyDonkey {}.data(),
        accounts::BuyDonkey {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_settle(
    crank: Pubkey,
    game: Pubkey,
    factions: &[Pubkey],
    wallets: &[Pubkey],
    admin: Pubkey,
) -> Instruction {
    let mut metas = accounts::Settle { crank, game }.to_account_metas(None);
    for f in factions {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new_readonly(*f, false));
    }
    for w in wallets {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(
            *w, false,
        ));
    }
    metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(
        admin, false,
    ));
    Instruction::new_with_bytes(id(), &instruction::Settle {}.data(), metas)
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

fn llm_decide(
    cfg: &llm::LlmConfig,
    phase: &str,
    round: u8,
    cash: u64,
    goods: u16,
    influence: u16,
    sold: u16,
    price_hint: u64,
    law: &str,
    president_is_me: bool,
) -> Option<serde_json::Value> {
    const TPL: &str = "Доступные действия JSON: \"action\":\"sell\",\"units\":N / \"action\":\"buy\",\"units\":N / \"action\":\"produce\" / \"action\":\"bribe\",\"amount\":N / \"action\":\"donkey\" / \"action\":\"vote\",\"choice\":\"yes|no|abstain\",\"veto\":true|false (объект в фигурных скобках, veto только если ты президент)";
    let user = format!(
        "Фаза: {}. Раунд {}/6. Твоё состояние: {} песо, {} товаров, влияние {}. На базаре продано {} единиц, следующая цена ~{} песо. Закон на голосовании: {}. Ты президент: {}. {}",
        phase, round, cash / 1_000_000, goods, influence, sold, price_hint, law, president_is_me, TPL
    );
    let raw = llm::llm_ask(cfg, llm::SYSTEM, &user)?;
    println!("    [llm raw] {}", raw.replace('\n', " "));
    llm::parse_json_block(&raw)
}

fn faction_cash(rpc: &RpcClient, faction: &Pubkey) -> u64 {
    fetch_faction(rpc, faction).map(|f| f.cash).unwrap_or(0)
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

    let llm = llm::llm_config();
    println!(
        "Botagul brain: {}",
        if llm.is_some() {
            "GLM llm"
        } else {
            "greedy heuristic (нет ключа)"
        }
    );
    let game_id = now() as u64;
    let game = game_pda(game_id);
    let mut bots = vec![
        Bot {
            faction: faction_pda(&game, &bot1_kp.pubkey()),
            kp: bot1_kp,
            name: "Aibot",
            acted: false,
            voted: false,
            goods: 0,
            bribed: false,
        },
        Bot {
            faction: faction_pda(&game, &bot2_kp.pubkey()),
            kp: bot2_kp,
            name: "Botagul",
            acted: false,
            voted: false,
            goods: 0,
            bribed: false,
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
        ix_join("Aibot", bots[0].kp.pubkey(), game, bots[0].faction),
    );
    wait_account(&rpc, &bots[0].faction, "faction1");
    send_ix(
        &rpc,
        &bots[1].kp,
        ix_join("Botagul", bots[1].kp.pubkey(), game, bots[1].faction),
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
                let mut sold_now = g.sold_this_round;
                for bi in 0..bots.len() {
                    let b = &mut bots[bi];
                    if b.acted {
                        continue;
                    }
                    if bi == 1 && llm.is_some() {
                        let f = fetch_faction(&rpc, &b.faction);
                        let (cash, goods, infl) = f
                            .as_ref()
                            .map(|f| (f.cash, f.goods, f.influence))
                            .unwrap_or((0, 0, 0));
                        let price_hint = 1_000_000u64.saturating_mul(
                            (PRICE_TABLE[(g.sold_this_round as usize).min(15)] as i64
                                + g.active_price_shift as i64
                                + g.active_boom as i64)
                                .max(1) as u64,
                        );
                        let dec = llm_decide(
                            llm.as_ref().unwrap(),
                            "Базар",
                            g.round,
                            cash,
                            goods,
                            infl,
                            sold_now,
                            price_hint / 1_000_000,
                            "-",
                            false,
                        );
                        let act = dec
                            .as_ref()
                            .and_then(|d| d.get("action"))
                            .and_then(|a| a.as_str())
                            .map(|s| s.to_string());
                        let units = dec
                            .as_ref()
                            .and_then(|d| d.get("units"))
                            .and_then(|u| u.as_u64())
                            .unwrap_or(1) as u16;
                        match act.as_deref() {
                            Some("buy") if cash >= units as u64 * price_hint => {
                                println!(
                                    "[market r{}] Botagul (LLM) buys {} goods",
                                    g.round, units
                                );
                                if send_ix(
                                    &rpc,
                                    &b.kp,
                                    ix_buy(units, b.kp.pubkey(), game, b.faction),
                                ) {
                                    b.acted = true;
                                    if let Some(f) = fetch_faction(&rpc, &b.faction) {
                                        b.goods = f.goods;
                                    }
                                    continue;
                                }
                            }
                            Some("sell") if goods > 0 && units <= goods => {
                                println!(
                                    "[market r{}] Botagul (LLM) sells {} goods",
                                    g.round, units
                                );
                                if send_ix(
                                    &rpc,
                                    &b.kp,
                                    ix_sell(units, b.kp.pubkey(), game, b.faction),
                                ) {
                                    b.acted = true;
                                    sold_now += units;
                                    if let Some(f) = fetch_faction(&rpc, &b.faction) {
                                        b.goods = f.goods;
                                    }
                                    continue;
                                }
                            }
                            _ => {}
                        }
                        println!("[market r{}] Botagul (LLM) fallback: greedy sell", g.round);
                        if b.goods == 0 {
                            b.acted = true;
                        }
                    }
                    if bi == 1 && sold_now >= 2 && faction_cash(&rpc, &b.faction) >= 10_000_000 {
                        println!(
                            "[market r{}] {} BUYS 2 cheap goods (sold={})",
                            g.round, b.name, g.sold_this_round
                        );
                        if send_ix(&rpc, &b.kp, ix_buy(2, b.kp.pubkey(), game, b.faction)) {
                            b.acted = true;
                            b.goods += 2;
                        }
                        continue;
                    }
                    if b.goods > 0 {
                        println!("[market r{}] {} sells {} goods", g.round, b.name, b.goods);
                        if send_ix(
                            &rpc,
                            &b.kp,
                            ix_sell(b.goods, b.kp.pubkey(), game, b.faction),
                        ) {
                            sold_now += b.goods;
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
                let target_faction = bots[1].faction;
                for (i, b) in bots.iter_mut().enumerate() {
                    if b.acted {
                        continue;
                    }
                    if i == 0 && !b.bribed && faction_cash(&rpc, &b.faction) >= 5_000_000 {
                        println!(
                            "[action r{}] {} bribes Botagul for influence",
                            g.round, b.name
                        );
                        if send_ix(
                            &rpc,
                            &b.kp,
                            ix_bribe(
                                5 * 1_000_000,
                                b.kp.pubkey(),
                                game,
                                b.faction,
                                target_faction,
                            ),
                        ) {
                            b.acted = true;
                            b.bribed = true;
                        }
                    } else if i == 1 && llm.is_some() {
                        let f = fetch_faction(&rpc, &b.faction);
                        let (cash, goods, infl) = f
                            .as_ref()
                            .map(|f| (f.cash, f.goods, f.influence))
                            .unwrap_or((0, 0, 0));
                        let dec = llm_decide(
                            llm.as_ref().unwrap(),
                            "Действие",
                            g.round,
                            cash,
                            goods,
                            infl,
                            g.sold_this_round,
                            0,
                            "-",
                            g.president == b.kp.pubkey(),
                        );
                        let act = dec
                            .as_ref()
                            .and_then(|d| d.get("action"))
                            .and_then(|a| a.as_str())
                            .map(|s| s.to_string());
                        match act.as_deref() {
                            Some("donkey") if cash >= 1_000_000 => {
                                println!("[action r{}] Botagul (LLM) buys a donkey", g.round);
                                if send_ix(&rpc, &b.kp, ix_donkey(b.kp.pubkey(), game, b.faction)) {
                                    b.acted = true;
                                    b.goods += 1;
                                }
                            }
                            Some("bribe") if cash >= 5_000_000 => {
                                println!(
                                    "[action r{}] Botagul (LLM) bribes for influence",
                                    g.round
                                );
                                if send_ix(
                                    &rpc,
                                    &b.kp,
                                    ix_bribe(
                                        5_000_000,
                                        b.kp.pubkey(),
                                        game,
                                        b.faction,
                                        target_faction,
                                    ),
                                ) {
                                    b.acted = true;
                                }
                            }
                            Some("produce") => {
                                println!("[action r{}] Botagul (LLM) produces", g.round);
                                if send_ix(&rpc, &b.kp, ix_produce(b.kp.pubkey(), game, b.faction))
                                {
                                    b.acted = true;
                                    b.goods += 2;
                                }
                            }
                            _ => {}
                        }
                        if !b.acted {
                            println!("[action r{}] Botagul fallback: produce", g.round);
                            if send_ix(&rpc, &b.kp, ix_produce(b.kp.pubkey(), game, b.faction)) {
                                b.acted = true;
                                b.goods += 2;
                            }
                        }
                    } else if i == 1 && g.round == 4 && llm.is_none() {
                        println!("[action r{}] {} buys a donkey", g.round, b.name);
                        if send_ix(&rpc, &b.kp, ix_donkey(b.kp.pubkey(), game, b.faction)) {
                            b.acted = true;
                            b.goods += 1;
                        }
                    } else {
                        println!("[action r{}] {} produces", g.round, b.name);
                        if send_ix(&rpc, &b.kp, ix_produce(b.kp.pubkey(), game, b.faction)) {
                            b.acted = true;
                            if let Some(f) = fetch_faction(&rpc, &b.faction) {
                                b.goods = f.goods;
                            }
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
                    if b.voted {
                        continue;
                    }
                    if i == 1 && llm.is_some() {
                        let f = fetch_faction(&rpc, &b.faction);
                        let (cash, goods, infl) = f
                            .as_ref()
                            .map(|f| (f.cash, f.goods, f.influence))
                            .unwrap_or((0, 0, 0));
                        let law_name = match g.law_card {
                            0 => "Статус-кво",
                            1 => "Налог 10%",
                            2 => "Налог 20%",
                            3 => "Субсидия производителям",
                            4 => "Субсидия бедным",
                            5 => "Субсидия богатым",
                            6 => "Эмбарго (цены -2)",
                            7 => "Бум (+2 к цене)",
                            _ => "неизвестен",
                        };
                        let dec = llm_decide(
                            llm.as_ref().unwrap(),
                            "Закон",
                            g.round,
                            cash,
                            goods,
                            infl,
                            g.sold_this_round,
                            0,
                            law_name,
                            g.president == b.kp.pubkey(),
                        );
                        let choice_s = dec
                            .as_ref()
                            .and_then(|d| d.get("choice"))
                            .and_then(|c| c.as_str())
                            .unwrap_or("abstain")
                            .to_string();
                        let choice = match choice_s.as_str() {
                            "yes" => state::VoteChoice::Yes,
                            "no" => state::VoteChoice::No,
                            _ => state::VoteChoice::Abstain,
                        };
                        println!("[law r{}] Botagul (LLM) votes {}", g.round, choice_s);
                        if send_ix(&rpc, &b.kp, ix_vote(choice, b.kp.pubkey(), game, b.faction)) {
                            b.voted = true;
                            let want_veto = dec
                                .as_ref()
                                .and_then(|d| d.get("veto"))
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            if want_veto && g.president == b.kp.pubkey() && !g.veto_pending {
                                println!("[law r{}] Botagul (LLM, president) vetoes", g.round);
                                send_ix(&rpc, &b.kp, ix_veto(b.kp.pubkey(), game, b.faction));
                            }
                        }
                        continue;
                    }
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
                if g.round == 3 && g.president == bots[0].kp.pubkey() && !g.veto_pending {
                    println!("[law r{}] Aibot (president) vetoes", g.round);
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_veto(bots[0].kp.pubkey(), game, bots[0].faction),
                    );
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
                            f.name, b.faction, f.cash, f.goods, f.influence
                        );
                    }
                }
                let bank = rpc.get_balance(&game).unwrap_or(0);
                println!("bank before settle: {bank} lamports");
                let wallets: Vec<Pubkey> = bots.iter().map(|b| b.kp.pubkey()).collect();
                let fkeys: Vec<Pubkey> = bots.iter().map(|b| b.faction).collect();
                if !g.settled {
                    send_ix(
                        &rpc,
                        &bots[0].kp,
                        ix_settle(
                            bots[0].kp.pubkey(),
                            game,
                            &fkeys,
                            &wallets,
                            bots[0].kp.pubkey(),
                        ),
                    );
                    sleep(Duration::from_secs(5));
                    let after = rpc.get_balance(&game).unwrap_or(0);
                    println!("bank after settle: {after} lamports");
                    let rake_wallet = rpc.get_balance(&bots[0].kp.pubkey()).unwrap_or(0);
                    println!("rake receiver balance: {rake_wallet} lamports");
                }
                println!("game: https://explorer.solana.com/address/{game}?cluster=devnet");
                break;
            }
        }

        let _ = std::io::Write::flush(&mut std::io::stdout());
        sleep(Duration::from_secs(2));
    }
}
