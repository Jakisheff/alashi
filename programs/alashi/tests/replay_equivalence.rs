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
            epoch: 0,
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

// ---------- M1-M11: хелперы инструкций эпохи 90-х ----------

fn ix_sell_credit(units: u16, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::SellCredit { units }.data(),
        alashi::accounts::SellCredit {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_shuttle(player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Shuttle {}.data(),
        alashi::accounts::Shuttle {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_roof(
    tariff: u8,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    target: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Roof { tariff }.data(),
        alashi::accounts::Roof {
            player,
            game,
            faction,
            target,
        }
        .to_account_metas(None),
    )
}

fn ix_set_customs(tight: bool, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::SetCustoms { tight }.data(),
        alashi::accounts::SetCustoms {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_bid_license(amount: u64, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::BidLicense { amount }.data(),
        alashi::accounts::BidLicense {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_inspect_license(player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::InspectLicense {}.data(),
        alashi::accounts::InspectLicense {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_exchange(
    to_hard: bool,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Exchange { to_hard }.data(),
        alashi::accounts::Exchange {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_offer_vote(
    price: u64,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    buyer: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::OfferVote { price }.data(),
        alashi::accounts::OfferVote {
            player,
            game,
            faction,
            buyer,
        }
        .to_account_metas(None),
    )
}

fn ix_accept_vote_offer(
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    seller: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::AcceptVoteOffer {}.data(),
        alashi::accounts::AcceptVoteOffer {
            player,
            game,
            faction,
            seller,
        }
        .to_account_metas(None),
    )
}

fn ix_barter_propose(
    goods: u16,
    price: u64,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::BarterPropose { goods, price }.data(),
        alashi::accounts::BarterPropose {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_barter_accept(
    offer_id: u64,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    seller: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::BarterAccept { offer_id }.data(),
        alashi::accounts::BarterAccept {
            player,
            game,
            faction,
            seller,
        }
        .to_account_metas(None),
    )
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

/// Эпоха 90-х ончейн: полная партия с каждым M-действием, состояние
/// сверяется байт-в-байт с симулятором после каждого хода.
#[test]
fn replay_epoch_90s_full_party() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/alashi.so"));
    svm.add_program(alashi::id(), bytes).unwrap();
    let a = Keypair::new();
    let b = Keypair::new();
    svm.airdrop(&a.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&b.pubkey(), 2_000_000_000).unwrap();

    let game = game_pda(90);
    let fa = faction_pda(&game, &a.pubkey());
    let fb = faction_pda(&game, &b.pubkey());
    let fkeys = vec![fa, fb];

    let mut sim = Simulator::new(90, FEE, 0, 0);
    sim.game.epoch = alashi::constants::EPOCH_90S;
    sim.game.admin = a.pubkey();
    sim.game.bump = Pubkey::find_program_address(
        &[alashi::constants::GAME_SEED, &90u64.to_le_bytes()],
        &alashi::id(),
    )
    .1;

    let step = |svm: &mut LiteSVM, sim: &mut Simulator, label: &str| {
        let (sim_g, sim_fs) = sim.state_bytes();
        assert_eq!(onchain_game(svm, &game), sim_g, "Game после {label}");
        assert_eq!(onchain_faction(svm, &fa), sim_fs[0], "Faction A после {label}");
        assert_eq!(onchain_faction(svm, &fb), sim_fs[1], "Faction B после {label}");
    };
    let advance = |svm: &mut LiteSVM, sim: &mut Simulator, label: &str| {
        set_seed(svm, 0);
        let now = chain_now(svm);
        assert!(
            send(svm, &a, ix_advance(a.pubkey(), game, fkeys.clone())),
            "advance {label}"
        );
        sim.advance(now, 0).unwrap();
        step(svm, sim, label);
    };

    // initialize(epoch=1) + join
    assert!(send(
        &mut svm,
        &a,
        Instruction::new_with_bytes(
            alashi::id(),
            &alashi::instruction::Initialize {
                game_id: 90,
                entry_fee: FEE,
                phase_duration: 0,
                entropy_mode: 0,
                epoch: 1,
            }
            .data(),
            alashi::accounts::Initialize {
                admin: a.pubkey(),
                game,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    ));
    assert!(send(&mut svm, &a, ix_join("Alpha", a.pubkey(), game, fa)));
    sim.join(a.pubkey(), "Alpha").unwrap();
    sim.factions[0].game = game;
    sim.factions[0].bump = Pubkey::find_program_address(
        &[alashi::constants::FACTION_SEED, game.as_ref(), a.pubkey().as_ref()],
        &alashi::id(),
    )
    .1;
    assert!(send(&mut svm, &b, ix_join("Beta", b.pubkey(), game, fb)));
    sim.join(b.pubkey(), "Beta").unwrap();
    sim.factions[1].game = game;
    sim.factions[1].bump = Pubkey::find_program_address(
        &[alashi::constants::FACTION_SEED, game.as_ref(), b.pubkey().as_ref()],
        &alashi::id(),
    )
    .1;
    step(&mut svm, &mut sim, "join");
    advance(&mut svm, &mut sim, "lobby->market r1");

    // r1 market: пусто (товара нет)
    advance(&mut svm, &mut sim, "market->action r1");
    // r1 action: M3 shuttle A (серой), produce B
    assert!(send(&mut svm, &a, ix_shuttle(a.pubkey(), game, fa)));
    sim.shuttle(0).unwrap();
    assert!(send(&mut svm, &b, ix_produce(b.pubkey(), game, fb)));
    sim.produce(1).unwrap();
    step(&mut svm, &mut sim, "r1 shuttle+produce");
    advance(&mut svm, &mut sim, "action->law r1"); // seed 0: tight → серой A изъят
    assert_eq!(sim.factions[0].goods, 0, "customs confiscated A grey");
    // r1 law: голоса
    assert!(send(&mut svm, &a, ix_vote(VoteChoice::Yes, a.pubkey(), game, fa)));
    sim.vote(0, VoteChoice::Yes).unwrap();
    assert!(send(&mut svm, &b, ix_vote(VoteChoice::No, b.pubkey(), game, fb)));
    sim.vote(1, VoteChoice::No).unwrap();
    advance(&mut svm, &mut sim, "law r1");

    // r2 market: M11 бартер-офер от B (1 товар за 3M, любому)
    assert!(send(
        &mut svm,
        &b,
        ix_barter_propose(1, 3 * PESO, b.pubkey(), game, fb)
    ));
    sim.barter_propose(1, None, 1, 3 * PESO).unwrap();
    step(&mut svm, &mut sim, "r2 barter_propose");
    advance(&mut svm, &mut sim, "market->action r2");
    // r2 action: A produce, B shuttle
    assert!(send(&mut svm, &a, ix_produce(a.pubkey(), game, fa)));
    sim.produce(0).unwrap();
    assert!(send(&mut svm, &b, ix_shuttle(b.pubkey(), game, fb)));
    sim.shuttle(1).unwrap();
    step(&mut svm, &mut sim, "r2 produce+shuttle");
    advance(&mut svm, &mut sim, "action->law r2"); // tight → серой B изъят
    assert!(send(&mut svm, &a, ix_vote(VoteChoice::Yes, a.pubkey(), game, fa)));
    sim.vote(0, VoteChoice::Yes).unwrap();
    assert!(send(&mut svm, &b, ix_vote(VoteChoice::No, b.pubkey(), game, fb)));
    sim.vote(1, VoteChoice::No).unwrap();
    advance(&mut svm, &mut sim, "law r2");

    // r3 market: A обычная продажа 2, B продажа в кредит 1 (второй
    // товар держит для бартер-офера r2)
    assert!(send(&mut svm, &a, ix_sell(2, a.pubkey(), game, fa)));
    sim.sell(0, 2).unwrap();
    assert!(send(&mut svm, &b, ix_sell_credit(1, b.pubkey(), game, fb)));
    sim.sell_credit(1, 1).unwrap();
    step(&mut svm, &mut sim, "r3 sell+sell_credit");
    advance(&mut svm, &mut sim, "market->action r3");
    // r3 action: M2/M7 крыша A → B (красная, 10%)
    assert!(send(
        &mut svm,
        &a,
        ix_roof(alashi::constants::ROOF_RED, a.pubkey(), game, fa, fb)
    ));
    sim.roof(0, 1, alashi::constants::ROOF_RED).unwrap();
    step(&mut svm, &mut sim, "r3 roof");
    advance(&mut svm, &mut sim, "action->law r3");
    // r3 law: M10 продажа голоса B→A за 2M в два шага
    assert!(send(
        &mut svm,
        &b,
        ix_offer_vote(2 * PESO, b.pubkey(), game, fb, fa)
    ));
    sim.offer_vote(1, 0, 2 * PESO).unwrap();
    assert!(send(
        &mut svm,
        &a,
        ix_accept_vote_offer(a.pubkey(), game, fa, fb)
    ));
    sim.accept_vote_offer(0).unwrap();
    step(&mut svm, &mut sim, "r3 offer_vote+accept");
    assert!(send(&mut svm, &a, ix_vote(VoteChoice::Yes, a.pubkey(), game, fa)));
    sim.vote(0, VoteChoice::Yes).unwrap();
    advance(&mut svm, &mut sim, "law r3"); // голос B идёт за A (продан)

    // r4 market: бартер-акцепт A (офер r2 ещё жив: 1 товар за 3M;
    // вексель B погашен при входе в r4 — кэш на ставку есть)
    let offer_id = sim.game.barter_offers[0].id;
    assert!(send(
        &mut svm,
        &a,
        ix_barter_accept(offer_id, a.pubkey(), game, fa, fb)
    ));
    sim.barter_accept(0, offer_id).unwrap();
    step(&mut svm, &mut sim, "r4 barter_accept");
    advance(&mut svm, &mut sim, "market->action r4"); // раунд аукциона: yield зафиксирован
    // r4 action: M8 граница президента (loose = дань с серых), затем M9
    let pres_idx = sim
        .factions
        .iter()
        .position(|f| f.wallet == sim.game.president)
        .unwrap();
    let (pres_kp, pres_f) = if pres_idx == 0 { (&a, fa) } else { (&b, fb) };
    assert!(send(
        &mut svm,
        pres_kp,
        ix_set_customs(false, pres_kp.pubkey(), game, pres_f)
    ));
    sim.set_customs(pres_idx, false).unwrap();
    step(&mut svm, &mut sim, "r4 set_customs loose");
    // M9: инсайд A (5M) и ставки A 6M / B 3M (после инспекта у A 7.1M)
    assert!(send(
        &mut svm,
        &a,
        ix_inspect_license(a.pubkey(), game, fa)
    ));
    sim.inspect_license(0).unwrap();
    assert!(send(
        &mut svm,
        &a,
        ix_bid_license(6 * PESO, a.pubkey(), game, fa)
    ));
    sim.bid_license(0, 6 * PESO).unwrap();
    assert!(send(
        &mut svm,
        &b,
        ix_bid_license(3 * PESO, b.pubkey(), game, fb)
    ));
    sim.bid_license(1, 3 * PESO).unwrap();
    step(&mut svm, &mut sim, "r4 inspect+bids");
    advance(&mut svm, &mut sim, "action->law r4"); // аукцион вскрыт: A победил
    assert!(sim.game.license_sold, "лицензия продана");
    assert!(send(&mut svm, &a, ix_vote(VoteChoice::Yes, a.pubkey(), game, fa)));
    sim.vote(0, VoteChoice::Yes).unwrap();
    advance(&mut svm, &mut sim, "law r4");

    // r5-r6: M6 валютчик + завершение (после "law r4" уже market r5)
    let a_cash = sim.factions[0].cash;
    if a_cash > 0 {
        assert!(send(
            &mut svm,
            &a,
            ix_exchange(true, a.pubkey(), game, fa)
        ));
        sim.exchange(0, true).unwrap();
        step(&mut svm, &mut sim, "r5 exchange buy_hard");
    }
    advance(&mut svm, &mut sim, "market->action r5");
    assert!(send(&mut svm, &a, ix_produce(a.pubkey(), game, fa)));
    sim.produce(0).unwrap();
    advance(&mut svm, &mut sim, "action->law r5");
    assert!(send(&mut svm, &a, ix_vote(VoteChoice::Yes, a.pubkey(), game, fa)));
    sim.vote(0, VoteChoice::Yes).unwrap();
    advance(&mut svm, &mut sim, "law r5 -> market r6");
    advance(&mut svm, &mut sim, "market->action r6");
    assert!(send(&mut svm, &b, ix_produce(b.pubkey(), game, fb)));
    sim.produce(1).unwrap();
    advance(&mut svm, &mut sim, "action->law r6");
    assert!(send(&mut svm, &b, ix_vote(VoteChoice::Yes, b.pubkey(), game, fb)));
    sim.vote(1, VoteChoice::Yes).unwrap();
    advance(&mut svm, &mut sim, "law r6");

    assert_eq!(sim.game.phase, alashi_rules::state::Phase::Finished);
    // ключевые M-состояния достигнуты и идентичны ончейн
    assert!(sim.game.license_sold, "аукцион проведён");
    assert!(sim.factions[0].hard > 0 || sim.factions[1].hard > 0, "hard куплен");
    assert_eq!(sim.game.barter_offers.len(), 0, "бартер исполнен");
}
