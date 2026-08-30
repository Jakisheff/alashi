use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

use anchor_lang::prelude::SlotHashes;
use solana_hash::Hash;

const PESO: u64 = alashi::constants::PESO;
const SLOT_HASHES_ID: Pubkey = solana_sysvar::slot_hashes::ID;

fn set_law_seed(svm: &mut LiteSVM, seed: u8) {
    let mut b = [0u8; 32];
    b[0] = seed;
    svm.set_sysvar(&SlotHashes::new(&[(1, Hash::new_from_array(b))]));
}
const FEE: u64 = 100 * PESO;

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/alashi.so"));
    svm.add_program(alashi::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    (svm, payer)
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

fn send(svm: &mut LiteSVM, payer: &Keypair, ix: Instruction) -> bool {
    svm.expire_blockhash();
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&payer.pubkey()), &blockhash);
    match VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]) {
        Ok(tx) => match svm.send_transaction(tx) {
            Ok(_) => true,
            Err(e) => {
                eprintln!("tx failed: {e:?}");
                false
            }
        },
        Err(e) => {
            eprintln!("tx build failed: {e:?}");
            false
        }
    }
}

fn game_state(svm: &LiteSVM, game: &Pubkey) -> alashi::state::Game {
    let acc = svm.get_account(game).unwrap();
    let mut data: &[u8] = &acc.data;
    alashi::state::Game::try_deserialize(&mut data).unwrap()
}

fn faction_state(svm: &LiteSVM, faction: &Pubkey) -> alashi::state::Faction {
    let acc = svm.get_account(faction).unwrap();
    let mut data: &[u8] = &acc.data;
    alashi::state::Faction::try_deserialize(&mut data).unwrap()
}

fn ix_initialize(id: u64, fee: u64, dur: i64, admin: Pubkey, game: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Initialize {
            game_id: id,
            entry_fee: fee,
            phase_duration: dur,
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

fn ix_bribe(
    amount: u64,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
    target: Pubkey,
) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Bribe { amount }.data(),
        alashi::accounts::Bribe {
            player,
            game,
            faction,
            target,
        }
        .to_account_metas(None),
    )
}

fn ix_vote(
    choice: alashi::state::VoteChoice,
    player: Pubkey,
    game: Pubkey,
    faction: Pubkey,
) -> Instruction {
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

fn ix_buy(units: u16, player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Buy { units }.data(),
        alashi::accounts::BuyGoods {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_veto(player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Veto {}.data(),
        alashi::accounts::Veto {
            player,
            game,
            faction,
        }
        .to_account_metas(None),
    )
}

fn ix_donkey(player: Pubkey, game: Pubkey, faction: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::BuyDonkey {}.data(),
        alashi::accounts::BuyDonkey {
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
    factions: Vec<Pubkey>,
    wallets: Vec<Pubkey>,
    admin: Pubkey,
) -> Instruction {
    let mut metas = alashi::accounts::Settle { crank, game }.to_account_metas(None);
    for f in factions {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new_readonly(f, false));
    }
    for w in wallets {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(
            w, false,
        ));
    }
    metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(
        admin, false,
    ));
    Instruction::new_with_bytes(alashi::id(), &alashi::instruction::Settle {}.data(), metas)
}

struct Party {
    svm: LiteSVM,
    admin: Keypair,
    b: Keypair,
    game: Pubkey,
    fa: Pubkey,
    fb: Pubkey,
}

fn start_party(id: u64, fee: u64, dur: i64) -> Party {
    let (mut svm, admin) = setup();
    let b = Keypair::new();
    svm.airdrop(&b.pubkey(), 2_000_000_000).unwrap();
    let game = game_pda(id);
    let fa = faction_pda(&game, &admin.pubkey());
    let fb = faction_pda(&game, &b.pubkey());
    assert!(send(
        &mut svm,
        &admin,
        ix_initialize(id, fee, dur, admin.pubkey(), game)
    ));
    assert!(send(
        &mut svm,
        &admin,
        ix_join("Kokzhiek", admin.pubkey(), game, fa)
    ));
    assert!(send(&mut svm, &b, ix_join("Osol", b.pubkey(), game, fb)));
    Party {
        svm,
        admin,
        b,
        game,
        fa,
        fb,
    }
}

impl Party {
    fn advance(&mut self) -> bool {
        set_law_seed(&mut self.svm, 0);
        send(
            &mut self.svm,
            &self.admin,
            ix_advance(self.admin.pubkey(), self.game, vec![self.fa, self.fb]),
        )
    }

    fn to_market_r2(
        &mut self,
        a_vote: Option<alashi::state::VoteChoice>,
        b_vote: Option<alashi::state::VoteChoice>,
    ) {
        assert!(self.advance());
        assert!(self.advance());
        assert!(send(
            &mut self.svm,
            &self.admin,
            ix_produce(self.admin.pubkey(), self.game, self.fa)
        ));
        assert!(send(
            &mut self.svm,
            &self.b,
            ix_produce(self.b.pubkey(), self.game, self.fb)
        ));
        assert!(self.advance());
        if let Some(c) = a_vote {
            assert!(send(
                &mut self.svm,
                &self.admin,
                ix_vote(c, self.admin.pubkey(), self.game, self.fa)
            ));
        }
        if let Some(c) = b_vote {
            assert!(send(
                &mut self.svm,
                &self.b,
                ix_vote(c, self.b.pubkey(), self.game, self.fb)
            ));
        }
        assert!(self.advance());
        let g = game_state(&self.svm, &self.game);
        assert_eq!(g.phase, alashi::state::Phase::Market);
        assert_eq!(g.round, 2);
    }
}

#[test]
fn test_full_game_six_rounds() {
    let mut p = start_party(1, FEE, 0);

    assert!(p.advance());
    for round in 1..=6u8 {
        assert_eq!(
            game_state(&p.svm, &p.game).phase,
            alashi::state::Phase::Market
        );
        assert_eq!(game_state(&p.svm, &p.game).round, round);

        let a_goods = faction_state(&p.svm, &p.fa).goods;
        if a_goods > 0 {
            assert!(send(
                &mut p.svm,
                &p.admin,
                ix_sell(2, p.admin.pubkey(), p.game, p.fa)
            ));
        }

        assert!(p.advance());
        assert_eq!(
            game_state(&p.svm, &p.game).phase,
            alashi::state::Phase::Action
        );
        assert!(send(
            &mut p.svm,
            &p.admin,
            ix_produce(p.admin.pubkey(), p.game, p.fa)
        ));
        assert!(send(
            &mut p.svm,
            &p.b,
            ix_produce(p.b.pubkey(), p.game, p.fb)
        ));

        assert!(p.advance());
        assert_eq!(game_state(&p.svm, &p.game).phase, alashi::state::Phase::Law);
        assert!(send(
            &mut p.svm,
            &p.admin,
            ix_vote(
                alashi::state::VoteChoice::Yes,
                p.admin.pubkey(),
                p.game,
                p.fa
            )
        ));
        assert!(send(
            &mut p.svm,
            &p.b,
            ix_vote(alashi::state::VoteChoice::No, p.b.pubkey(), p.game, p.fb)
        ));

        assert!(p.advance());
        let g = game_state(&p.svm, &p.game);
        if round < 6 {
            assert_eq!(g.phase, alashi::state::Phase::Market);
            assert_eq!(g.round, round + 1);
        } else {
            assert_eq!(g.phase, alashi::state::Phase::Finished);
        }
    }

    let g = game_state(&p.svm, &p.game);
    assert_eq!(g.laws_passed, 0);
    assert!(!g.last_law_passed);
    assert_eq!(g.yes_influence, 1);
    assert_eq!(g.no_influence, 1);

    let a = faction_state(&p.svm, &p.fa);
    assert_eq!(a.cash, 110 * PESO);
    assert_eq!(a.goods, 2);
    assert_eq!(a.influence, 1);

    let bank = p.svm.get_account(&p.game).unwrap().lamports;
    assert!(bank >= 2 * FEE);
}

#[test]
fn test_market_price_drop() {
    let mut p = start_party(2, FEE, 0);
    p.to_market_r2(Some(alashi::state::VoteChoice::Yes), None);

    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_sell(2, p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(send(
        &mut p.svm,
        &p.b,
        ix_sell(1, p.b.pubkey(), p.game, p.fb)
    ));

    let a = faction_state(&p.svm, &p.fa);
    let b = faction_state(&p.svm, &p.fb);
    assert_eq!(a.cash, (12 + 10) * PESO);
    assert_eq!(b.cash, 9 * PESO);
    assert_eq!(game_state(&p.svm, &p.game).sold_this_round, 3);

    let g = game_state(&p.svm, &p.game);
    assert_eq!(g.laws_passed, 1);
    assert!(g.last_law_passed);
}

#[test]
fn test_bribe_and_vote_weight() {
    let mut p = start_party(3, FEE, 0);
    p.to_market_r2(
        Some(alashi::state::VoteChoice::Yes),
        Some(alashi::state::VoteChoice::No),
    );

    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_sell(2, p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(p.advance());

    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_bribe(5 * PESO, p.admin.pubkey(), p.game, p.fa, p.fb)
    ));
    assert!(p.advance());

    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_vote(
            alashi::state::VoteChoice::Yes,
            p.admin.pubkey(),
            p.game,
            p.fa
        )
    ));
    assert!(send(
        &mut p.svm,
        &p.b,
        ix_vote(alashi::state::VoteChoice::No, p.b.pubkey(), p.game, p.fb)
    ));
    assert!(p.advance());

    let g = game_state(&p.svm, &p.game);
    assert_eq!(g.phase, alashi::state::Phase::Market);
    assert_eq!(g.round, 3);
    assert_eq!(g.yes_influence, 2);
    assert_eq!(g.no_influence, 1);
    assert_eq!(g.laws_passed, 1);
    assert!(g.last_law_passed);

    let a = faction_state(&p.svm, &p.fa);
    let b = faction_state(&p.svm, &p.fb);
    assert_eq!(a.influence, 2);
    assert_eq!(a.cash, 17 * PESO);
    assert_eq!(b.cash, 5 * PESO);
}

#[test]
fn test_time_gate() {
    let mut p = start_party(4, FEE, 3600);
    assert!(!p.advance());

    let g = game_state(&p.svm, &p.game);
    assert_eq!(g.phase, alashi::state::Phase::Lobby);
}

#[test]
fn test_phase_and_join_guards() {
    let mut p = start_party(5, FEE, 0);

    assert!(!send(
        &mut p.svm,
        &p.admin,
        ix_produce(p.admin.pubkey(), p.game, p.fa)
    ));

    assert!(p.advance());
    assert!(!send(
        &mut p.svm,
        &p.admin,
        ix_join("Late", p.admin.pubkey(), p.game, p.fa)
    ));

    assert!(!send(
        &mut p.svm,
        &p.admin,
        ix_vote(
            alashi::state::VoteChoice::Yes,
            p.admin.pubkey(),
            p.game,
            p.fa
        )
    ));
}

#[test]
fn test_already_acted() {
    let mut p = start_party(6, FEE, 0);
    p.to_market_r2(None, None);

    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_sell(2, p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(!send(
        &mut p.svm,
        &p.admin,
        ix_sell(1, p.admin.pubkey(), p.game, p.fa)
    ));

    assert!(p.advance());
    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_produce(p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(!send(
        &mut p.svm,
        &p.admin,
        ix_produce(p.admin.pubkey(), p.game, p.fa)
    ));
}

#[test]
fn test_law_tax_and_veto() {
    let mut p = start_party(11, FEE, 0);
    set_law_seed(&mut p.svm, 0);

    assert!(p.advance());
    assert!(p.advance());
    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_produce(p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(send(
        &mut p.svm,
        &p.b,
        ix_produce(p.b.pubkey(), p.game, p.fb)
    ));
    set_law_seed(&mut p.svm, 0);
    assert!(p.advance());
    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_vote(
            alashi::state::VoteChoice::Yes,
            p.admin.pubkey(),
            p.game,
            p.fa
        )
    ));
    assert!(send(
        &mut p.svm,
        &p.b,
        ix_vote(alashi::state::VoteChoice::No, p.b.pubkey(), p.game, p.fb)
    ));
    set_law_seed(&mut p.svm, 0);
    assert!(p.advance());

    set_law_seed(&mut p.svm, 0);
    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_sell(2, p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(send(
        &mut p.svm,
        &p.b,
        ix_sell(1, p.b.pubkey(), p.game, p.fb)
    ));
    assert!(p.advance());
    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_bribe(5 * PESO, p.admin.pubkey(), p.game, p.fa, p.fb)
    ));
    set_law_seed(&mut p.svm, 1);
    assert!(p.advance());

    let g = game_state(&p.svm, &p.game);
    assert_eq!(g.law_card, alashi::constants::LAW_TAX_10);
    assert_eq!(g.president, p.admin.pubkey());

    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_vote(
            alashi::state::VoteChoice::Yes,
            p.admin.pubkey(),
            p.game,
            p.fa
        )
    ));
    assert!(send(
        &mut p.svm,
        &p.b,
        ix_vote(alashi::state::VoteChoice::No, p.b.pubkey(), p.game, p.fb)
    ));
    set_law_seed(&mut p.svm, 1);
    assert!(p.advance());

    let g = game_state(&p.svm, &p.game);
    assert!(g.last_law_passed);
    assert_eq!(g.laws_passed, 1);
    assert_eq!(g.active_tax_bps, 1_000);

    set_law_seed(&mut p.svm, 0);
    let a_goods = faction_state(&p.svm, &p.fa).goods;
    if a_goods > 0 {
        assert!(send(
            &mut p.svm,
            &p.admin,
            ix_sell(2, p.admin.pubkey(), p.game, p.fa)
        ));
    }
    assert!(p.advance());
    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_produce(p.admin.pubkey(), p.game, p.fa)
    ));
    set_law_seed(&mut p.svm, 0);
    assert!(p.advance());
    let g = game_state(&p.svm, &p.game);
    assert_eq!(g.law_card, alashi::constants::LAW_TAX_20);

    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_vote(
            alashi::state::VoteChoice::Yes,
            p.admin.pubkey(),
            p.game,
            p.fa
        )
    ));
    assert!(send(
        &mut p.svm,
        &p.b,
        ix_vote(alashi::state::VoteChoice::No, p.b.pubkey(), p.game, p.fb)
    ));
    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_veto(p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(!send(&mut p.svm, &p.b, ix_veto(p.b.pubkey(), p.game, p.fb)));
    set_law_seed(&mut p.svm, 0);
    assert!(p.advance());

    let g = game_state(&p.svm, &p.game);
    assert!(!g.last_law_passed);
    assert_eq!(g.laws_passed, 1);
}

#[test]
fn test_donkey() {
    let mut p = start_party(12, FEE, 0);
    p.to_market_r2(None, None);

    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_sell(2, p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(p.advance());
    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_donkey(p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(!send(
        &mut p.svm,
        &p.admin,
        ix_produce(p.admin.pubkey(), p.game, p.fa)
    ));

    let a = faction_state(&p.svm, &p.fa);
    assert_eq!(a.cash, (22 - 1) * PESO);
    assert_eq!(a.goods, 1);
}

#[test]
fn test_settle_payout_and_rake() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/alashi.so"));
    svm.add_program(alashi::id(), bytes).unwrap();
    let a = Keypair::new();
    let b = Keypair::new();
    let rake_admin = Keypair::new();
    svm.airdrop(&a.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&b.pubkey(), 2_000_000_000).unwrap();
    svm.airdrop(&rake_admin.pubkey(), 1_000_000_000).unwrap();

    let game = game_pda(13);
    let fa = faction_pda(&game, &a.pubkey());
    let fb = faction_pda(&game, &b.pubkey());
    assert!(send(
        &mut svm,
        &rake_admin,
        ix_initialize(13, FEE, 0, rake_admin.pubkey(), game)
    ));
    assert!(send(&mut svm, &a, ix_join("Alpha", a.pubkey(), game, fa)));
    assert!(send(&mut svm, &b, ix_join("Beta", b.pubkey(), game, fb)));
    let fkeys = vec![fa, fb];

    let adv = |svm: &mut LiteSVM, s: &Keypair| {
        set_law_seed(svm, 0);
        send(svm, s, ix_advance(s.pubkey(), game, fkeys.clone()))
    };

    assert!(adv(&mut svm, &a));
    for _round in 1..=6u8 {
        let a_goods = faction_state(&svm, &fa).goods;
        if a_goods > 0 {
            assert!(send(&mut svm, &a, ix_sell(2, a.pubkey(), game, fa)));
        }
        assert!(adv(&mut svm, &a));
        assert!(send(&mut svm, &a, ix_produce(a.pubkey(), game, fa)));
        assert!(send(&mut svm, &b, ix_produce(b.pubkey(), game, fb)));
        assert!(adv(&mut svm, &a));
        assert!(send(
            &mut svm,
            &a,
            ix_vote(alashi::state::VoteChoice::Abstain, a.pubkey(), game, fa)
        ));
        assert!(adv(&mut svm, &a));
    }
    let g = game_state(&svm, &game);
    assert_eq!(g.phase, alashi::state::Phase::Finished);
    assert_eq!(g.admin, rake_admin.pubkey());

    let a_cash = faction_state(&svm, &fa).cash;
    let b_cash = faction_state(&svm, &fb).cash;
    assert!(a_cash > b_cash);

    let bank_before = svm.get_account(&game).unwrap().lamports;
    let rake_before = svm.get_account(&rake_admin.pubkey()).unwrap().lamports;
    let a_before = svm.get_account(&a.pubkey()).unwrap().lamports;
    let b_before = svm.get_account(&b.pubkey()).unwrap().lamports;

    assert!(send(
        &mut svm,
        &a,
        ix_settle(
            a.pubkey(),
            game,
            vec![fa, fb],
            vec![a.pubkey(), b.pubkey()],
            rake_admin.pubkey(),
        )
    ));
    assert!(!send(
        &mut svm,
        &a,
        ix_settle(
            a.pubkey(),
            game,
            vec![fa, fb],
            vec![a.pubkey(), b.pubkey()],
            rake_admin.pubkey(),
        )
    ));

    let bank_after = svm.get_account(&game).unwrap().lamports;
    let rake_after = svm.get_account(&rake_admin.pubkey()).unwrap().lamports;
    let a_after = svm.get_account(&a.pubkey()).unwrap().lamports;
    let b_after = svm.get_account(&b.pubkey()).unwrap().lamports;

    assert!(game_state(&svm, &game).settled);
    let pot = bank_before - bank_after;
    assert!(pot > 0);
    let rake = rake_after - rake_before;
    assert_eq!(rake, pot * 500 / 10_000);
    let paid = (a_after - a_before) + (b_after - b_before);
    assert_eq!(paid + 2 * 5_000, pot - rake);
    assert!(a_after - a_before > b_after - b_before);
}

#[test]
fn test_market_buy() {
    let mut p = start_party(14, FEE, 0);
    p.to_market_r2(
        Some(alashi::state::VoteChoice::Abstain),
        Some(alashi::state::VoteChoice::Abstain),
    );

    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_sell(2, p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(send(
        &mut p.svm,
        &p.b,
        ix_sell(1, p.b.pubkey(), p.game, p.fb)
    ));
    assert_eq!(game_state(&p.svm, &p.game).sold_this_round, 3);

    assert!(p.advance());
    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_produce(p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(send(
        &mut p.svm,
        &p.b,
        ix_produce(p.b.pubkey(), p.game, p.fb)
    ));
    assert!(p.advance());
    set_law_seed(&mut p.svm, 0);
    assert!(p.advance());

    assert!(send(
        &mut p.svm,
        &p.admin,
        ix_sell(2, p.admin.pubkey(), p.game, p.fa)
    ));
    assert!(send(
        &mut p.svm,
        &p.b,
        ix_buy(1, p.b.pubkey(), p.game, p.fb)
    ));
    let g = game_state(&p.svm, &p.game);
    assert_eq!(g.sold_this_round, 1);
    let a = faction_state(&p.svm, &p.fa);
    let b = faction_state(&p.svm, &p.fb);
    assert_eq!(a.cash, (22 + 22) * PESO);
    assert_eq!(b.cash, (9 - 9) * PESO);
    assert_eq!(b.goods, 4);

    assert!(!send(
        &mut p.svm,
        &p.b,
        ix_buy(1, p.b.pubkey(), p.game, p.fb)
    ));
}
