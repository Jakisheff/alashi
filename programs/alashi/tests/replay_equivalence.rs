use {
    alashi_rules::sim::Simulator,
    alashi_rules::state::{Faction, Game, VoteChoice},
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, AccountSerialize, InitSpace, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_clock::Clock,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const PESO: u64 = alashi::constants::PESO;
const FEE: u64 = 100 * PESO;
const SLOT_HASHES_ID: Pubkey = solana_sysvar::slot_hashes::ID;

fn send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) -> bool {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &blockhash);
    match VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]) {
        Ok(tx) => svm.send_transaction(tx).is_ok(),
        Err(_) => false,
    }
}

fn game_pda(id: u64) -> Pubkey {
    let seed = id.to_le_bytes();
    Pubkey::find_program_address(
        &[alashi::constants::GAME_SEED, seed.as_ref()],
        &alashi::id(),
    )
    .0
}

fn faction_pda(game: &Pubkey, wallet: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            alashi::constants::FACTION_SEED,
            game.as_ref(),
            wallet.as_ref(),
        ],
        &alashi::id(),
    )
    .0
}

fn set_seed(svm: &mut LiteSVM, seed: u8) {
    use anchor_lang::prelude::SlotHashes;
    use solana_hash::Hash;
    let mut b = [0u8; 32];
    b[0] = seed;
    svm.set_sysvar(&SlotHashes::new(&[(1, Hash::new_from_array(b))]));
}

fn chain_now(svm: &LiteSVM) -> i64 {
    svm.get_sysvar::<Clock>().unix_timestamp
}

fn onchain_game(svm: &LiteSVM, game: &Pubkey) -> Vec<u8> {
    let acc = svm.get_account(game).unwrap();
    let mut out = Vec::new();
    Game::try_deserialize(&mut &acc.data[..])
        .unwrap()
        .try_serialize(&mut out)
        .unwrap();
    out
}

fn onchain_faction(svm: &LiteSVM, faction: &Pubkey) -> Vec<u8> {
    let acc = svm.get_account(faction).unwrap();
    let mut out = Vec::new();
    Faction::try_deserialize(&mut &acc.data[..])
        .unwrap()
        .try_serialize(&mut out)
        .unwrap();
    out
}

fn sim_bytes(sim: &Simulator) -> (Vec<u8>, Vec<Vec<u8>>) {
    sim.state_bytes()
}

fn ix_initialize(id: u64, admin: Pubkey, game: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Initialize {
            game_id: id,
            entry_fee: FEE,
            phase_duration: 0,
            entropy_mode: 0,
        }
        .data(),
        alashi::accounts::Initialize {
            admin,
            game,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn ix_join(name: &str, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Join {
            name: name.to_string(),
        }
        .data(),
        alashi::accounts::Join {
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
        alashi::id(),
        &alashi::instruction::Produce {}.data(),
        alashi::accounts::Produce {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_sell(units: u16, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Sell { units }.data(),
        alashi::accounts::Sell {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_vote(choice: VoteChoice, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Vote { choice }.data(),
        alashi::accounts::Vote {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_advance(crank: Pubkey, game: Pubkey, factions: Vec<Pubkey>) -> Instruction {
    let mut metas = alashi::accounts::Advance {
        crank,
        game,
        hashes: SLOT_HASHES_ID,
    }
    .to_account_metas(None);
    for f in factions {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(
            f, false,
        ));
    }
    Instruction::new_with_bytes(alashi::id(), &alashi::instruction::Advance {}.data(), metas)
}

#[test]
fn replay_equivalence_full_party() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/alashi.so"));
    svm.add_program(alashi::id(), bytes).unwrap();
    let a = Keypair::new();
    let b = Keypair::new();
    svm.airdrop(&a.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&b.pubkey(), 2_000_000_000).unwrap();

    let game = game_pda(31);
    let fa = faction_pda(&game, &a.pubkey());
    let fb = faction_pda(&game, &b.pubkey());
    let fkeys = vec![fa, fb];

    let mut sim = Simulator::new(31, FEE, 0, 0);
    sim.game.admin = a.pubkey();
    sim.game.bump = Pubkey::find_program_address(
        &[alashi::constants::GAME_SEED, &31u64.to_le_bytes()],
        &alashi::id(),
    )
    .1;

    let step = |svm: &mut LiteSVM, sim: &mut Simulator, label: &str| {
        let (sim_g, sim_fs) = sim.state_bytes();
        assert_eq!(
            onchain_game(svm, &game),
            sim_g,
            "расхождение Game после {label}"
        );
        assert_eq!(
            onchain_faction(svm, &fa),
            sim_fs[0],
            "расхождение Faction A после {label}"
        );
        assert_eq!(
            onchain_faction(svm, &fb),
            sim_fs[1],
            "расхождение Faction B после {label}"
        );
    };

    assert!(send(&mut svm, &a, ix_initialize(31, a.pubkey(), game)));
    assert!(send(&mut svm, &a, ix_join("Alpha", a.pubkey(), game, fa)));
    sim.join(a.pubkey(), "Alpha").unwrap();
    sim.factions[0].game = game;
    sim.factions[0].bump = Pubkey::find_program_address(
        &[
            alashi::constants::FACTION_SEED,
            game.as_ref(),
            a.pubkey().as_ref(),
        ],
        &alashi::id(),
    )
    .1;
    assert!(send(&mut svm, &b, ix_join("Beta", b.pubkey(), game, fb)));
    sim.join(b.pubkey(), "Beta").unwrap();
    sim.factions[1].game = game;
    sim.factions[1].bump = Pubkey::find_program_address(
        &[
            alashi::constants::FACTION_SEED,
            game.as_ref(),
            b.pubkey().as_ref(),
        ],
        &alashi::id(),
    )
    .1;
    step(&mut svm, &mut sim, "join");

    set_seed(&mut svm, 0);
    let now = chain_now(&svm);
    assert!(send(
        &mut svm,
        &a,
        ix_advance(a.pubkey(), game, fkeys.clone())
    ));
    sim.advance(now, 0).unwrap();
    step(&mut svm, &mut sim, "lobby->market");

    for round in 1..=6u8 {
        let a_goods = sim.factions[0].goods;
        if a_goods > 0 {
            assert!(send(&mut svm, &a, ix_sell(2, a.pubkey(), game, fa)));
            sim.sell(0, 2).unwrap();
            step(&mut svm, &mut sim, "sell A");
        }
        if round > 1 && sim.factions[1].goods > 0 {
            assert!(send(&mut svm, &b, ix_sell(1, b.pubkey(), game, fb)));
            sim.sell(1, 1).unwrap();
            step(&mut svm, &mut sim, "sell B");
        }

        set_seed(&mut svm, 0);
        let now = chain_now(&svm);
        assert!(send(
            &mut svm,
            &a,
            ix_advance(a.pubkey(), game, fkeys.clone())
        ));
        sim.advance(now, 0).unwrap();
        step(&mut svm, &mut sim, "market->action");

        assert!(send(&mut svm, &a, ix_produce(a.pubkey(), game, fa)));
        sim.produce(0).unwrap();
        assert!(send(&mut svm, &b, ix_produce(b.pubkey(), game, fb)));
        sim.produce(1).unwrap();
        step(&mut svm, &mut sim, "produce");

        set_seed(&mut svm, 0);
        let now = chain_now(&svm);
        assert!(send(
            &mut svm,
            &a,
            ix_advance(a.pubkey(), game, fkeys.clone())
        ));
        sim.advance(now, 0).unwrap();
        step(&mut svm, &mut sim, "action->law");

        assert!(send(
            &mut svm,
            &a,
            ix_vote(VoteChoice::Yes, a.pubkey(), game, fa)
        ));
        sim.vote(0, VoteChoice::Yes).unwrap();
        assert!(send(
            &mut svm,
            &b,
            ix_vote(VoteChoice::No, b.pubkey(), game, fb)
        ));
        sim.vote(1, VoteChoice::No).unwrap();
        step(&mut svm, &mut sim, "votes");

        set_seed(&mut svm, 0);
        let now = chain_now(&svm);
        assert!(send(
            &mut svm,
            &a,
            ix_advance(a.pubkey(), game, fkeys.clone())
        ));
        sim.advance(now, 0).unwrap();
        step(&mut svm, &mut sim, &format!("law r{round}"));
    }

    assert_eq!(sim.game.phase, alashi_rules::state::Phase::Finished);
}
