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
const VOTE_WEIGHT_LEGACY: u8 = alashi_rules::constants::VOTE_WEIGHT_LEGACY;
const VOTE_WEIGHT_CONTRIB: u8 = alashi_rules::constants::VOTE_WEIGHT_CONTRIB;
const SLOT_HASHES_ID: Pubkey = solana_sysvar::slot_hashes::ID;

const SB_DEVNET_PID: &str = "Aio4gaXjXzJNVLtzwtNVmSqGKpANtXhybbkhtAC94ji2";

fn rng_account_bytes(seed_slot: u64, reveal_slot: u64, value_first: u8) -> Vec<u8> {
    let mut d = Vec::with_capacity(408);
    d.extend_from_slice(&[10, 66, 229, 135, 220, 239, 217, 114]);
    d.extend_from_slice(&[0u8; 32]);
    d.extend_from_slice(&[1u8; 32]);
    d.extend_from_slice(&[2u8; 32]);
    d.extend_from_slice(&seed_slot.to_le_bytes());
    d.extend_from_slice(&[3u8; 32]);
    d.extend_from_slice(&reveal_slot.to_le_bytes());
    let mut value = [0u8; 32];
    value[0] = value_first;
    d.extend_from_slice(&value);
    d.extend_from_slice(&[0u8; 224]);
    d
}

fn install_rng(svm: &mut LiteSVM, rng: &Pubkey, seed_slot: u64, reveal_slot: u64, value_first: u8) {
    use solana_account::Account as SysAccount;
    let owner = solana_address::Address::from_str_const(SB_DEVNET_PID);
    svm.set_account(
        *rng,
        SysAccount {
            lamports: 1_000_000,
            data: rng_account_bytes(seed_slot, reveal_slot, value_first),
            owner,
            executable: false,
            rent_epoch: u64::MAX,
        },
    )
    .unwrap();
}

fn ix_reveal_law(crank: Pubkey, game: Pubkey, rng: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::RevealLaw {}.data(),
        alashi::accounts::RevealLaw {
            crank,
            game,
            randomness: rng,
        }
        .to_account_metas(None),
    )
}

fn ix_settle_refund(
    crank: Pubkey,
    game: Pubkey,
    factions: Vec<Pubkey>,
    wallets: Vec<Pubkey>,
) -> Instruction {
    let mut metas = alashi::accounts::SettleRefund { crank, game }.to_account_metas(None);
    for f in factions {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new_readonly(f, false));
    }
    for w in wallets {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(
            w, false,
        ));
    }
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::SettleRefund {}.data(),
        metas,
    )
}

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

fn ix_initialize(
    id: u64,
    fee: u64,
    dur: i64,
    admin: Pubkey,
    game: Pubkey,
    mode: u8,
) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::Initialize {
            game_id: id,
            entry_fee: fee,
            phase_duration: dur,
            entropy_mode: mode,
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
        ix_initialize(id, fee, dur, admin.pubkey(), game, 0)
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
        ix_initialize(13, FEE, 0, rake_admin.pubkey(), game, 0)
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

#[test]
fn test_vrf_threshold() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/alashi.so"));
    svm.add_program(alashi::id(), bytes).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 10_000_000_000).unwrap();
    let big_fee = 300_000_000;
    let game1 = game_pda(21);
    let game2 = game_pda(22);

    assert!(!send(
        &mut svm,
        &payer,
        ix_initialize(21, big_fee, 0, payer.pubkey(), game1, 0)
    ));
    assert!(send(
        &mut svm,
        &payer,
        ix_initialize(22, big_fee, 0, payer.pubkey(), game2, 1)
    ));
    assert_eq!(
        game_state(&svm, &game2).entropy_mode,
        alashi::constants::ENTROPY_SWITCHBOARD
    );
}

#[test]
fn test_vrf_mode_flow() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/alashi.so"));
    svm.add_program(alashi::id(), bytes).unwrap();
    let a = Keypair::new();
    let b = Keypair::new();
    svm.airdrop(&a.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&b.pubkey(), 2_000_000_000).unwrap();

    let game = game_pda(23);
    let fa = faction_pda(&game, &a.pubkey());
    let fb = faction_pda(&game, &b.pubkey());
    let rng = Keypair::new().pubkey();
    assert!(send(
        &mut svm,
        &a,
        ix_initialize(23, FEE, 0, a.pubkey(), game, 1)
    ));
    assert!(send(&mut svm, &a, ix_join("Alpha", a.pubkey(), game, fa)));
    assert!(send(&mut svm, &b, ix_join("Beta", b.pubkey(), game, fb)));
    let fkeys = vec![fa, fb];

    let adv = |svm: &mut LiteSVM, s: &Keypair, extra: Vec<Pubkey>| {
        set_law_seed(svm, 0);
        let mut keys = fkeys.clone();
        keys.extend(extra);
        send(svm, s, ix_advance(s.pubkey(), game, keys))
    };

    assert!(adv(&mut svm, &a, vec![]));
    assert!(adv(&mut svm, &a, vec![]));
    assert!(send(&mut svm, &a, ix_produce(a.pubkey(), game, fa)));
    assert!(send(&mut svm, &b, ix_produce(b.pubkey(), game, fb)));

    let slot = svm.get_sysvar::<solana_clock::Clock>().slot;
    install_rng(&mut svm, &rng, slot + 5, 0, 0);
    assert!(adv(&mut svm, &a, vec![rng]));

    let g = game_state(&svm, &game);
    assert_eq!(g.law_card, 255);
    assert_eq!(g.vrf_account, rng);
    assert_eq!(g.commit_slot, slot + 5);

    assert!(!send(
        &mut svm,
        &a,
        ix_vote(alashi::state::VoteChoice::Yes, a.pubkey(), game, fa)
    ));

    svm.warp_to_slot(slot + 5);
    install_rng(&mut svm, &rng, slot + 5, slot + 5, 5);
    assert!(send(&mut svm, &a, ix_reveal_law(a.pubkey(), game, rng)));
    let g = game_state(&svm, &game);
    assert_eq!(g.law_card, 5);

    assert!(send(
        &mut svm,
        &a,
        ix_vote(alashi::state::VoteChoice::Yes, a.pubkey(), game, fa)
    ));
    assert!(send(
        &mut svm,
        &b,
        ix_vote(alashi::state::VoteChoice::No, b.pubkey(), game, fb)
    ));
    set_law_seed(&mut svm, 0);
    assert!(send(
        &mut svm,
        &a,
        ix_advance(a.pubkey(), game, fkeys.clone())
    ));
    let g = game_state(&svm, &game);
    assert_eq!(g.round, 2);
    assert_eq!(g.phase, alashi::state::Phase::Market);
}

#[test]
fn test_vrf_timeout_abort_refund() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/alashi.so"));
    svm.add_program(alashi::id(), bytes).unwrap();
    let a = Keypair::new();
    let b = Keypair::new();
    let crank = Keypair::new();
    svm.airdrop(&a.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&b.pubkey(), 2_000_000_000).unwrap();
    svm.airdrop(&crank.pubkey(), 1_000_000_000).unwrap();

    let game = game_pda(24);
    let fa = faction_pda(&game, &a.pubkey());
    let fb = faction_pda(&game, &b.pubkey());
    let rng = Keypair::new().pubkey();
    assert!(send(
        &mut svm,
        &a,
        ix_initialize(24, FEE, 0, a.pubkey(), game, 1)
    ));
    assert!(send(&mut svm, &a, ix_join("Alpha", a.pubkey(), game, fa)));
    assert!(send(&mut svm, &b, ix_join("Beta", b.pubkey(), game, fb)));
    let fkeys = vec![fa, fb];

    set_law_seed(&mut svm, 0);
    assert!(send(
        &mut svm,
        &a,
        ix_advance(a.pubkey(), game, fkeys.clone())
    ));
    assert!(send(
        &mut svm,
        &a,
        ix_advance(a.pubkey(), game, fkeys.clone())
    ));
    assert!(send(&mut svm, &a, ix_produce(a.pubkey(), game, fa)));
    assert!(send(&mut svm, &b, ix_produce(b.pubkey(), game, fb)));

    let slot = svm.get_sysvar::<solana_clock::Clock>().slot;
    let commit_slot = slot + 5;
    install_rng(&mut svm, &rng, commit_slot, 0, 0);
    assert!(send(
        &mut svm,
        &a,
        ix_advance(a.pubkey(), game, vec![fa, fb, rng])
    ));

    svm.warp_to_slot(commit_slot + 40);
    set_law_seed(&mut svm, 0);
    assert!(send(
        &mut svm,
        &a,
        ix_advance(a.pubkey(), game, fkeys.clone())
    ));
    assert!(send(
        &mut svm,
        &a,
        ix_advance(a.pubkey(), game, fkeys.clone())
    ));
    assert!(send(
        &mut svm,
        &a,
        ix_advance(a.pubkey(), game, fkeys.clone())
    ));
    let g = game_state(&svm, &game);
    assert_eq!(g.vrf_retries, 3);
    assert_eq!(g.phase, alashi::state::Phase::Law);

    assert!(send(
        &mut svm,
        &a,
        ix_advance(a.pubkey(), game, fkeys.clone())
    ));
    let g = game_state(&svm, &game);
    assert_eq!(g.phase, alashi::state::Phase::Aborted);

    let a_before = svm.get_account(&a.pubkey()).unwrap().lamports;
    let b_before = svm.get_account(&b.pubkey()).unwrap().lamports;
    assert!(send(
        &mut svm,
        &crank,
        ix_settle_refund(
            crank.pubkey(),
            game,
            vec![fa, fb],
            vec![a.pubkey(), b.pubkey()],
        )
    ));
    let a_gain = svm.get_account(&a.pubkey()).unwrap().lamports - a_before;
    let b_gain = svm.get_account(&b.pubkey()).unwrap().lamports - b_before;
    println!("DBG a_gain={a_gain} b_gain={b_gain}");
    assert_eq!(a_gain + b_gain, 2 * FEE);
    assert!(game_state(&svm, &game).settled);
}

// ---------- R2 (REVIEW_EXTERNAL): дедуп аккаунтов в settle ----------

#[test]
fn test_settle_rejects_duplicate_faction_accounts() {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/alashi.so"));
    svm.add_program(alashi::id(), bytes).unwrap();
    let a = Keypair::new();
    let b = Keypair::new();
    let rake_admin = Keypair::new();
    svm.airdrop(&a.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&b.pubkey(), 2_000_000_000).unwrap();
    svm.airdrop(&rake_admin.pubkey(), 1_000_000_000).unwrap();

    let game = game_pda(9100);
    let fa = faction_pda(&game, &a.pubkey());
    let fb = faction_pda(&game, &b.pubkey());
    assert!(send(
        &mut svm,
        &rake_admin,
        ix_initialize(9100, FEE, 0, rake_admin.pubkey(), game, 0)
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
    assert_eq!(game_state(&svm, &game).phase, alashi::state::Phase::Finished);

    // атака R2: свой Faction дважды + свой кошелёк дважды → отказ
    assert!(!send(
        &mut svm,
        &a,
        ix_settle(
            a.pubkey(),
            game,
            vec![fa, fa],
            vec![a.pubkey(), a.pubkey()],
            rake_admin.pubkey(),
        )
    ));
    // состояние не тронуто: честный settle проходит тем же банком
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
    assert!(game_state(&svm, &game).settled);
}

// ---------- взнос-как-голос (SPEC_VOTE_CONTRIBUTION) ----------

fn ix_set_vote_mode(admin: Pubkey, game: Pubkey, mode: u8) -> Instruction {
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::SetVoteMode { mode }.data(),
        alashi::accounts::SetVoteMode {
            admin,
            game,
        }
        .to_account_metas(None),
    )
}

struct Party3 {
    svm: LiteSVM,
    admin: Keypair,
    b: Keypair,
    c: Keypair,
    game: Pubkey,
    fa: Pubkey,
    fb: Pubkey,
    fc: Pubkey,
}

fn start_party3(id: u64, vote_mode: Option<u8>) -> Party3 {
    let (mut svm, admin) = setup();
    let b = Keypair::new();
    let c = Keypair::new();
    svm.airdrop(&b.pubkey(), 2_000_000_000).unwrap();
    svm.airdrop(&c.pubkey(), 2_000_000_000).unwrap();
    let game = game_pda(id);
    let fa = faction_pda(&game, &admin.pubkey());
    let fb = faction_pda(&game, &b.pubkey());
    let fc = faction_pda(&game, &c.pubkey());
    assert!(send(
        &mut svm,
        &admin,
        ix_initialize(id, FEE, 0, admin.pubkey(), game, 0)
    ));
    if let Some(mode) = vote_mode {
        assert!(send(
            &mut svm,
            &admin,
            ix_set_vote_mode(admin.pubkey(), game, mode)
        ));
    }
    assert!(send(
        &mut svm,
        &admin,
        ix_join("Aibot", admin.pubkey(), game, fa)
    ));
    assert!(send(&mut svm, &b, ix_join("Botagul", b.pubkey(), game, fb)));
    assert!(send(&mut svm, &c, ix_join("Zhambyl", c.pubkey(), game, fc)));
    Party3 {
        svm,
        admin,
        b,
        c,
        game,
        fa,
        fb,
        fc,
    }
}

impl Party3 {
    fn advance(&mut self) -> bool {
        set_law_seed(&mut self.svm, 0); // карта 0 = status_quo, нейтральна
        send(
            &mut self.svm,
            &self.admin,
            ix_advance(
                self.admin.pubkey(),
                self.game,
                vec![self.fa, self.fb, self.fc],
            ),
        )
    }
}

/// Раунд с известным исходом: A и B действуют и голосуют ЗА,
/// C пропускает действие и голосует ПРОТИВ.
fn play_round_skipper(p: &mut Party3) -> alashi::state::Game {
    assert!(p.advance()); // lobby -> market r1
    assert!(p.advance()); // market -> action
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
    // C молчит: пропуск = взнос
    assert!(p.advance()); // action -> law
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
        ix_vote(alashi::state::VoteChoice::Yes, p.b.pubkey(), p.game, p.fb)
    ));
    assert!(send(
        &mut p.svm,
        &p.c,
        ix_vote(alashi::state::VoteChoice::No, p.c.pubkey(), p.game, p.fc)
    ));
    assert!(p.advance()); // law -> market r2
    game_state(&p.svm, &p.game)
}

#[test]
fn test_contribution_skip_flips_vote() {
    let mut p = start_party3(9001, Some(VOTE_WEIGHT_CONTRIB as u8));
    let g = play_round_skipper(&mut p);
    // да = 2 (оба действовали), нет = 1 + 2 (пропуск) = 3 → закон упал
    assert_eq!((g.yes_influence, g.no_influence), (2, 3));
    assert!(!g.last_law_passed);
    // поле влияния C не тронуто: бонус жил только в подсчёте
    let c = faction_state(&p.svm, &p.fc);
    assert_eq!(c.influence, 1);
    assert_eq!(g.vote_weight_mode, VOTE_WEIGHT_CONTRIB as u8);
}

#[test]
fn test_legacy_same_scenario_passes() {
    let mut p = start_party3(9002, None);
    let g = play_round_skipper(&mut p);
    // legacy: да 2 > нет 1 → прошёл, пропуск ничего не весит
    assert_eq!((g.yes_influence, g.no_influence), (2, 1));
    assert!(g.last_law_passed);
}

#[test]
fn test_set_vote_mode_guards() {
    let mut p = start_party3(9003, Some(VOTE_WEIGHT_CONTRIB as u8));
    // не-админ не может менять режим
    assert!(!send(
        &mut p.svm,
        &p.b,
        ix_set_vote_mode(p.b.pubkey(), p.game, VOTE_WEIGHT_LEGACY as u8)
    ));
    // неверный режим отклоняется
    assert!(!send(
        &mut p.svm,
        &p.admin,
        ix_set_vote_mode(p.admin.pubkey(), p.game, 2)
    ));
    // после старта партии режим менять нельзя
    assert!(p.advance());
    assert!(!send(
        &mut p.svm,
        &p.admin,
        ix_set_vote_mode(p.admin.pubkey(), p.game, VOTE_WEIGHT_LEGACY as u8)
    ));
}
