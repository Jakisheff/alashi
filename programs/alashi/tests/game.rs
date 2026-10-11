use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::Instruction, system_program},
        AccountDeserialize, AccountSerialize, InstructionData, ToAccountMetas,
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
const SB_QUEUE: &str = "EYiAmGSdsQTuCw413V5BzaruWuCCSDgTPtBGvLkXHbe7";
const SB_STATE: &str = "4UFmCebEmzESoDTtHrmaftXj7YAsAH4HMios3yMWyVUT";
const SB_ORACLE: &str = "Hdu1niJgqVGhesoxgy37p6WBunVDoBacZJZVK7VRRevg";
const SB_NATIVE_MINT: &str = "So11111111111111111111111111111111111111112";
const SB_TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const SB_ATA: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
const SB_LUT: &str = "AddressLookupTab1e1111111111111111111111111";
const EPOCH2_FIXTURE_SLOT: u64 = 424_242;

fn sb_key(value: &str) -> Pubkey { value.parse().unwrap() }

fn assert_fixture(data: &[u8], expected_len: usize, expected_sha256: &str) {
    assert_eq!(data.len(), expected_len);
    let digest = solana_sha256_hasher::hash(data).to_bytes();
    let actual = digest.iter().map(|byte| format!("{byte:02x}")).collect::<String>();
    assert_eq!(actual, expected_sha256);
}

fn set_epoch2_clock(svm: &mut LiteSVM, slot: u64) {
    let mut clock = svm.get_sysvar::<solana_clock::Clock>();
    clock.slot = slot;
    clock.unix_timestamp = slot as i64;
    svm.set_sysvar(&clock);
    svm.set_sysvar(&SlotHashes::new(&[
        (slot - 1, Hash::new_from_array([7; 32])),
        (slot - 2, Hash::new_from_array([6; 32])),
    ]));
}

fn install_real_provider_fixture(svm: &mut LiteSVM) {
    use solana_account::Account as SysAccount;
    let provider = sb_key(SB_DEVNET_PID);
    let executable = include_bytes!("fixtures/switchboard_devnet_509984/provider.elf");
    assert_fixture(executable, 2_108_896, "fbb0ba2f36af9be4b16b2a94c41e8678e9ec73a638ad8a71ac3a11f1737c1451");
    svm.add_program(provider, executable).unwrap();
    for (key, owner, lamports, data, len, sha256) in [
        (SB_QUEUE, SB_DEVNET_PID, 444_599_680, include_bytes!("fixtures/switchboard_devnet_509984/queue.bin").as_slice(), 6_280, "261a50cdad3f2b35e81cdddf5da638e36f02fbcd7df90de5705a8fdc4f6a578a"),
        (SB_STATE, SB_DEVNET_PID, 15_757_440, include_bytes!("fixtures/switchboard_devnet_509984/state.bin").as_slice(), 2_136, "515b99f11aac13cfc21e4808722b6320445069070182eac508b283819209aa4e"),
        (SB_ORACLE, SB_DEVNET_PID, 34_410_240, include_bytes!("fixtures/switchboard_devnet_509984/oracle.bin").as_slice(), 4_816, "47ae6d6d33f1ad6da6b20a7154c30803a2599a37f9aae9b18b45f20817c83616"),
        (SB_NATIVE_MINT, SB_TOKEN, 1_509_155_950_425, include_bytes!("fixtures/switchboard_devnet_509984/native_mint.bin").as_slice(), 82, "b0f919425a529f4971ef8dabd535aec34947d5c8c4885df80dae91635b98c5f3"),
    ] {
        assert_fixture(data, len, sha256);
        svm.set_account(sb_key(key), SysAccount {
            lamports, data: data.to_vec(), owner: sb_key(owner), executable: false,
            rent_epoch: u64::MAX,
        }).unwrap();
    }
    set_epoch2_clock(svm, EPOCH2_FIXTURE_SLOT);
}

fn epoch2_source(game: &Pubkey, round: u8) -> Pubkey {
    Pubkey::find_program_address(
        &[alashi::donkey_state::DONKEY_SOURCE_SEED, game.as_ref(), &[round]], &alashi::id(),
    ).0
}

fn init_epoch2_sources(svm: &mut LiteSVM, admin: &Keypair, game: Pubkey) -> Vec<Pubkey> {
    let provider = sb_key(SB_DEVNET_PID);
    let token = sb_key(SB_TOKEN);
    let mint = sb_key(SB_NATIVE_MINT);
    let ata = sb_key(SB_ATA);
    let lut_program = sb_key(SB_LUT);
    let authority = Pubkey::find_program_address(
        &[alashi::constants::DONKEY_ORACLE_SEED, game.as_ref()], &alashi::id(),
    ).0;
    (1..=alashi::constants::ROUNDS).map(|round| {
        let source = epoch2_source(&game, round);
        let escrow = Pubkey::find_program_address(
            &[source.as_ref(), token.as_ref(), mint.as_ref()], &ata,
        ).0;
        let lut_signer = Pubkey::find_program_address(&[b"LutSigner", source.as_ref()], &provider).0;
        let lut = Pubkey::find_program_address(
            &[lut_signer.as_ref(), &(EPOCH2_FIXTURE_SLOT - 1).to_le_bytes()], &lut_program,
        ).0;
        let ix = Instruction::new_with_bytes(
            alashi::id(),
            &alashi::instruction::InitDonkeySource { round, recent_slot: EPOCH2_FIXTURE_SLOT - 1 }.data(),
            alashi::accounts::InitDonkeySource {
                admin: admin.pubkey(), game, randomness: source, authority,
                reward_escrow: escrow, queue: sb_key(SB_QUEUE), system_program: system_program::ID,
                token_program: token, associated_token_program: ata, native_mint: mint,
                switchboard_state: sb_key(SB_STATE), lut_signer, lut,
                lut_program, switchboard_program: provider,
            }.to_account_metas(None),
        );
        assert!(send(svm, admin, ix), "provider CPI Init round {round}");
        source
    }).collect()
}

fn configure_epoch2(svm: &mut LiteSVM, admin: &Keypair, game: Pubkey, sources: &[Pubkey]) -> Pubkey {
    let config = Pubkey::find_program_address(
        &[alashi::constants::DONKEY_ORACLE_SEED, game.as_ref()], &alashi::id(),
    ).0;
    let mut metas = alashi::accounts::ConfigureDonkeyOracles {
        admin: admin.pubkey(), game, oracle: config, system_program: system_program::ID,
    }.to_account_metas(None);
    metas.extend(sources.iter().map(|source| {
        anchor_lang::solana_program::instruction::AccountMeta::new_readonly(*source, false)
    }));
    metas.extend((0..alashi::constants::ROUNDS).map(|_| {
        anchor_lang::solana_program::instruction::AccountMeta::new_readonly(sb_key(SB_ORACLE), false)
    }));
    assert!(send(svm, admin, Instruction::new_with_bytes(
        alashi::id(), &alashi::instruction::ConfigureDonkeyOracles {}.data(), metas,
    )));
    config
}

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

fn ix_reveal_donkey_law(crank: Pubkey, game: Pubkey, rng: Pubkey, tickets: &[Pubkey]) -> Instruction {
    let provider = sb_key(SB_DEVNET_PID);
    let oracle = sb_key(SB_ORACLE);
    let config = Pubkey::find_program_address(
        &[alashi::constants::DONKEY_ORACLE_SEED, game.as_ref()], &alashi::id(),
    ).0;
    let stats = Pubkey::find_program_address(
        &[b"OracleRandomnessStats", oracle.as_ref()], &provider,
    ).0;
    let escrow = Pubkey::find_program_address(
        &[rng.as_ref(), sb_key(SB_TOKEN).as_ref(), sb_key(SB_NATIVE_MINT).as_ref()],
        &sb_key(SB_ATA),
    ).0;
    let mut accounts = alashi::accounts::RevealDonkeyLaw {
        crank, game, randomness: rng, oracle, queue: sb_key(SB_QUEUE), stats,
        recent_slothashes: SLOT_HASHES_ID, system_program: system_program::ID,
        reward_escrow: escrow, token_program: sb_key(SB_TOKEN),
        native_mint: sb_key(SB_NATIVE_MINT), provider_state: sb_key(SB_STATE),
        provider_program: provider,
    }.to_account_metas(None);
    accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new(config, false));
    accounts.extend(tickets.iter().map(|key|
        anchor_lang::solana_program::instruction::AccountMeta::new(*key, false)));
    Instruction::new_with_bytes(
        alashi::id(),
        &alashi::instruction::RevealDonkeyLaw {
            signature: [0; 64], recovery_id: 0, value: [0; 32],
        }.data(),
        accounts,
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
fn test_epoch90s_factory_pays_in_lamports_and_preserves_rent() {
    let (mut svm, admin) = setup();
    let a = Keypair::new();
    let b = Keypair::new();
    let crank = Keypair::new();
    for signer in [&a, &b, &crank] {
        svm.airdrop(&signer.pubkey(), 2_000_000_000).unwrap();
    }
    let gid = 606;
    let game = game_pda(gid);
    let fa = faction_pda(&game, &a.pubkey());
    let fb = faction_pda(&game, &b.pubkey());
    let mut init = ix_initialize(gid, FEE, 0, admin.pubkey(), game, 0);
    init.data = alashi::instruction::Initialize {
        game_id: gid, entry_fee: FEE, phase_duration: 0, entropy_mode: 0,
        epoch: alashi::constants::EPOCH_90S,
    }.data();
    assert!(send(&mut svm, &admin, init));
    let reserve = svm.get_account(&game).unwrap().lamports;
    assert!(reserve > 0);
    assert!(send(&mut svm, &a, ix_join("Alpha", a.pubkey(), game, fa)));
    assert!(send(&mut svm, &b, ix_join("Beta", b.pubkey(), game, fb)));
    for _ in 0..19 {
        set_law_seed(&mut svm, 0);
        assert!(send(&mut svm, &crank, ix_advance(crank.pubkey(), game, vec![fa, fb])));
    }
    assert_eq!(game_state(&svm, &game).phase, alashi::state::Phase::Finished);
    let admin_before = svm.get_account(&admin.pubkey()).unwrap().lamports;
    let a_before = svm.get_account(&a.pubkey()).unwrap().lamports;
    let b_before = svm.get_account(&b.pubkey()).unwrap().lamports;
    assert!(send(&mut svm, &crank, ix_settle(
        crank.pubkey(), game, vec![fa, fb], vec![a.pubkey(), b.pubkey()], admin.pubkey(),
    )));
    assert_eq!(svm.get_account(&game).unwrap().lamports, reserve);
    assert_eq!(svm.get_account(&admin.pubkey()).unwrap().lamports, admin_before);
    let a_paid = svm.get_account(&a.pubkey()).unwrap().lamports - a_before;
    let b_paid = svm.get_account(&b.pubkey()).unwrap().lamports - b_before;
    let pot = 2 * FEE;
    assert_eq!(a_paid + b_paid, pot);
    let expected_factory = pot / 20;
    let rank_one = (pot - expected_factory) * 30 / 80;
    let expected_winner = pot - rank_one;
    let winner_paid = if a.pubkey() < b.pubkey() { a_paid } else { b_paid };
    assert_eq!(winner_paid, expected_winner);
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

    svm.warp_to_slot(100);
    let slot = svm.get_sysvar::<solana_clock::Clock>().slot;
    install_rng(&mut svm, &rng, slot - 1, 0, 0);
    assert!(adv(&mut svm, &a, vec![rng]));

    let g = game_state(&svm, &game);
    assert_eq!(g.law_card, 255);
    assert_eq!(g.vrf_account, rng);
    assert_eq!(g.commit_slot, slot - 1);

    assert!(!send(
        &mut svm,
        &a,
        ix_vote(alashi::state::VoteChoice::Yes, a.pubkey(), game, fa)
    ));

    svm.warp_to_slot(slot + 5);
    install_rng(&mut svm, &rng, slot - 1, slot + 5, 5);
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

    svm.warp_to_slot(100);
    let slot = svm.get_sysvar::<solana_clock::Clock>().slot;
    let commit_slot = slot - 1;
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

fn ix_close_game(crank: Pubkey, game: Pubkey, admin: Pubkey, factions: Vec<Pubkey>, wallets: Vec<Pubkey>) -> Instruction {
    let mut metas = alashi::accounts::CloseGame { crank, game, admin }.to_account_metas(None);
    for key in factions.into_iter().chain(wallets) {
        metas.push(anchor_lang::solana_program::instruction::AccountMeta::new(key, false));
    }
    Instruction::new_with_bytes(alashi::id(), &alashi::instruction::CloseGame {}.data(), metas)
}

#[test]
fn test_expired_lobby_refunds_and_closes_empty_or_single_player() {
    for count in [0, 1] {
        let (mut svm, admin) = setup();
        let player = Keypair::new();
        let crank = Keypair::new();
        svm.airdrop(&player.pubkey(), 2_000_000_000).unwrap();
        svm.airdrop(&crank.pubkey(), 1_000_000_000).unwrap();
        let id = 900 + count;
        let game = game_pda(id);
        let faction = faction_pda(&game, &player.pubkey());
        assert!(send(&mut svm, &admin, ix_initialize(id, FEE, 10, admin.pubkey(), game, 0)));
        let game_rent = svm.get_account(&game).unwrap().lamports;
        let (factions, wallets) = if count == 1 {
            assert!(send(&mut svm, &player, ix_join("Solo", player.pubkey(), game, faction)));
            (vec![faction], vec![player.pubkey()])
        } else { (vec![], vec![]) };
        assert!(!send(&mut svm, &crank, ix_advance(crank.pubkey(), game, factions.clone())));
        assert!(!send(&mut svm, &crank, ix_close_game(crank.pubkey(), game, admin.pubkey(), factions.clone(), wallets.clone())));
        let mut clock = svm.get_sysvar::<solana_clock::Clock>();
        clock.unix_timestamp = game_state(&svm, &game).phase_ends_at;
        svm.set_sysvar(&clock);
        assert!(send(&mut svm, &crank, ix_advance(crank.pubkey(), game, factions.clone())));
        assert_eq!(game_state(&svm, &game).phase, alashi::state::Phase::Aborted);
        let player_before = svm.get_account(&player.pubkey()).unwrap().lamports;
        let faction_rent = if count == 1 { svm.get_account(&faction).unwrap().lamports } else { 0 };
        assert!(send(&mut svm, &crank, ix_settle_refund(crank.pubkey(), game, factions.clone(), wallets.clone())));
        assert_eq!(svm.get_account(&player.pubkey()).unwrap().lamports - player_before, FEE * count);
        assert!(!send(&mut svm, &crank, ix_settle_refund(crank.pubkey(), game, factions.clone(), wallets.clone())));
        let admin_before = svm.get_account(&admin.pubkey()).unwrap().lamports;
        assert!(send(&mut svm, &crank, ix_close_game(crank.pubkey(), game, admin.pubkey(), factions, wallets)));
        assert_eq!(svm.get_account(&admin.pubkey()).unwrap().lamports - admin_before, game_rent);
        assert_eq!(svm.get_account(&player.pubkey()).unwrap().lamports - player_before, FEE * count + faction_rent);
        assert!(svm.get_account(&game).is_none_or(|a| a.lamports == 0 && a.data.is_empty()));
        if count == 1 { assert!(svm.get_account(&faction).is_none_or(|a| a.lamports == 0 && a.data.is_empty())); }
    }
}

#[test]
fn epoch2_unconfigured_lobby_rejects_join_before_fee_and_can_abort() {
    let (mut svm, admin) = setup();
    let player = Keypair::new();
    svm.airdrop(&player.pubkey(), 2_000_000_000).unwrap();
    let id = 9_202;
    let game = game_pda(id);
    let faction = faction_pda(&game, &player.pubkey());
    let initialize = alashi::instruction::Initialize {
        game_id: id,
        entry_fee: FEE,
        phase_duration: 10,
        entropy_mode: alashi::constants::ENTROPY_SWITCHBOARD,
        epoch: alashi::constants::EPOCH_DONKEY_RISK,
    };
    let ix = Instruction::new_with_bytes(
        alashi::id(),
        &initialize.data(),
        alashi::accounts::Initialize {
            admin: admin.pubkey(),
            game,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert!(send(&mut svm, &admin, ix));
    let player_before = svm.get_account(&player.pubkey()).unwrap().lamports;
    let game_before = svm.get_account(&game).unwrap().lamports;
    assert!(!send(
        &mut svm,
        &player,
        ix_join("Unconfigured", player.pubkey(), game, faction)
    ));
    // A rejected transaction still pays the LiteSVM signature fee, but it
    // must not transfer the game's entry fee or create a faction.
    assert_eq!(
        player_before - svm.get_account(&player.pubkey()).unwrap().lamports,
        5_000
    );
    assert_eq!(svm.get_account(&game).unwrap().lamports, game_before);
    assert_eq!(game_state(&svm, &game).faction_count, 0);
    assert!(svm.get_account(&faction).is_none());

    let mut clock = svm.get_sysvar::<solana_clock::Clock>();
    clock.unix_timestamp = game_state(&svm, &game).phase_ends_at;
    svm.set_sysvar(&clock);
    set_law_seed(&mut svm, 0);
    assert!(send(
        &mut svm,
        &admin,
        ix_advance(admin.pubkey(), game, vec![])
    ));
    assert_eq!(game_state(&svm, &game).phase, alashi::state::Phase::Aborted);
}

#[test]
fn epoch2_configured_lobby_accepts_join() {
    let (mut svm, admin) = setup();
    install_real_provider_fixture(&mut svm);
    let player = Keypair::new();
    svm.airdrop(&player.pubkey(), 2_000_000_000).unwrap();
    let id = 9_203;
    let game = game_pda(id);
    let faction = faction_pda(&game, &player.pubkey());
    let oracle = Pubkey::find_program_address(
        &[alashi::constants::DONKEY_ORACLE_SEED, game.as_ref()],
        &alashi::id(),
    )
    .0;
    let initialize = alashi::instruction::Initialize {
        game_id: id,
        entry_fee: FEE,
        phase_duration: 10,
        entropy_mode: alashi::constants::ENTROPY_SWITCHBOARD,
        epoch: alashi::constants::EPOCH_DONKEY_RISK,
    };
    assert!(send(
        &mut svm,
        &admin,
        Instruction::new_with_bytes(
            alashi::id(),
            &initialize.data(),
            alashi::accounts::Initialize {
                admin: admin.pubkey(),
                game,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    ));
    let sources = init_epoch2_sources(&mut svm, &admin, game);
    assert_eq!(configure_epoch2(&mut svm, &admin, game, &sources), oracle);
    let mut join = ix_join("Configured", player.pubkey(), game, faction);
    join.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new_readonly(oracle, false));
    let game_before = svm.get_account(&game).unwrap().lamports;
    assert!(send(&mut svm, &player, join));
    assert_eq!(game_state(&svm, &game).faction_count, 1);
    assert_eq!(svm.get_account(&game).unwrap().lamports - game_before, FEE);
}

fn synthetic_epoch2_draw_is_win(value: u8, game: &Pubkey, faction: &Pubkey) -> bool {
    let mut vrf = [0u8; 32];
    vrf[0] = value;
    let digest = solana_sha256_hasher::hashv(&[
        b"alashi-donkey-risk-v1".as_slice(),
        vrf.as_slice(),
        game.as_ref(),
        faction.as_ref(),
        &[2],
        &0u32.to_le_bytes(),
    ])
    .to_bytes();
    assert_ne!(digest, [0xff; 32]);
    digest.iter().fold(0u8, |r, b| ((r as u16 * 256 + *b as u16) % 5) as u8) < 3
}

fn synthetic_epoch2_case(want_win: Option<bool>, two_tickets: bool, precommit_abort: bool) {
    let (mut svm, admin) = setup();
    install_real_provider_fixture(&mut svm);
    let other = Keypair::new();
    svm.airdrop(&other.pubkey(), 2_000_000_000).unwrap();
    let id = match want_win { Some(true) => 9_204, Some(false) => 9_205, None => 9_206 };
    let game = game_pda(id);
    let faction = faction_pda(&game, &admin.pubkey());
    let other_faction = faction_pda(&game, &other.pubkey());
    let oracle = Pubkey::find_program_address(
        &[alashi::constants::DONKEY_ORACLE_SEED, game.as_ref()],
        &alashi::id(),
    )
    .0;
    let initialize = alashi::instruction::Initialize {
        game_id: id,
        entry_fee: FEE,
        phase_duration: 0,
        entropy_mode: alashi::constants::ENTROPY_SWITCHBOARD,
        epoch: alashi::constants::EPOCH_DONKEY_RISK,
    };
    assert!(send(
        &mut svm,
        &admin,
        Instruction::new_with_bytes(
            alashi::id(),
            &initialize.data(),
            alashi::accounts::Initialize {
                admin: admin.pubkey(),
                game,
                system_program: system_program::ID,
            }
            .to_account_metas(None),
        )
    ));
    let sources = init_epoch2_sources(&mut svm, &admin, game);
    assert_eq!(configure_epoch2(&mut svm, &admin, game, &sources), oracle);
    for (wallet, name, faction_pda) in [(&admin, "A", faction), (&other, "B", other_faction)] {
        let mut join = ix_join(name, wallet.pubkey(), game, faction_pda);
        join.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new_readonly(oracle, false));
        assert!(send(&mut svm, wallet, join));
    }

    // Synthetic account fixture starts after Market income. The instructions
    // below still execute in SBF and prove the actual fee/turn and resolution.
    let mut g = game_state(&svm, &game);
    g.phase = alashi::state::Phase::Action;
    g.round = 2;
    g.phase_ends_at = (EPOCH2_FIXTURE_SLOT + 100) as i64;
    let mut game_account = svm.get_account(&game).unwrap();
    let mut bytes = Vec::new();
    g.try_serialize(&mut bytes).unwrap();
    bytes.resize(game_account.data.len(), 0);
    game_account.data = bytes;
    svm.set_account(game, game_account).unwrap();
    let mut f = faction_state(&svm, &faction);
    f.cash = alashi::constants::DONKEY_RISK_FEE;
    let mut faction_account = svm.get_account(&faction).unwrap();
    let mut bytes = Vec::new();
    f.try_serialize(&mut bytes).unwrap();
    bytes.resize(faction_account.data.len(), 0);
    faction_account.data = bytes;
    svm.set_account(faction, faction_account).unwrap();
    if two_tickets {
        let mut second = faction_state(&svm, &other_faction);
        second.cash = alashi::constants::DONKEY_RISK_FEE;
        let mut account = svm.get_account(&other_faction).unwrap();
        let mut bytes = Vec::new();
        second.try_serialize(&mut bytes).unwrap();
        bytes.resize(account.data.len(), 0);
        account.data = bytes;
        svm.set_account(other_faction, account).unwrap();
    }

    let attempt = |source: Pubkey| {
        Instruction::new_with_bytes(
            alashi::id(),
            &alashi::instruction::AttemptDonkey {}.data(),
            alashi::accounts::AttemptDonkey {
                player: admin.pubkey(),
                game,
                faction,
                oracle,
                randomness: source,
            }
            .to_account_metas(None),
        )
    };
    let mut final_round = game_state(&svm, &game);
    final_round.round = alashi::constants::ROUNDS;
    let mut account = svm.get_account(&game).unwrap();
    let mut bytes = Vec::new();
    final_round.try_serialize(&mut bytes).unwrap();
    bytes.resize(account.data.len(), 0);
    account.data = bytes;
    svm.set_account(game, account).unwrap();
    assert!(!send(&mut svm, &admin, attempt(sources[5])));
    assert_eq!(faction_state(&svm, &faction).cash, alashi::constants::DONKEY_RISK_FEE);
    let mut account = svm.get_account(&game).unwrap();
    let mut bytes = Vec::new();
    g.try_serialize(&mut bytes).unwrap();
    bytes.resize(account.data.len(), 0);
    account.data = bytes;
    svm.set_account(game, account).unwrap();
    assert!(!send(&mut svm, &admin, attempt(sources[0])));
    assert_eq!(faction_state(&svm, &faction).cash, alashi::constants::DONKEY_RISK_FEE);
    assert!(send(&mut svm, &admin, attempt(sources[1])));
    let after_attempt = faction_state(&svm, &faction);
    assert_eq!(after_attempt.cash, 0);
    assert_eq!(after_attempt.goods, 0);
    assert_eq!(after_attempt.acted_stamp, game_state(&svm, &game).stamp());
    assert!(!send(&mut svm, &admin, attempt(sources[1])));
    assert_eq!(faction_state(&svm, &faction).cash, 0);
    if two_tickets {
        let second_attempt = Instruction::new_with_bytes(
            alashi::id(),
            &alashi::instruction::AttemptDonkey {}.data(),
            alashi::accounts::AttemptDonkey {
                player: other.pubkey(), game, faction: other_faction,
                oracle, randomness: sources[1],
            }.to_account_metas(None),
        );
        assert!(send(&mut svm, &other, second_attempt));
        assert_eq!(faction_state(&svm, &other_faction).cash, 0);
    }

    if precommit_abort {
        let mut clock = svm.get_sysvar::<solana_clock::Clock>();
        clock.unix_timestamp = game_state(&svm, &game).phase_ends_at
            + alashi::constants::DONKEY_COMMIT_GRACE_SECONDS - 1;
        svm.set_sysvar(&clock);
        let mut abort = ix_advance(admin.pubkey(), game, vec![faction, other_faction]);
        abort.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new(oracle, false));
        assert!(!send(&mut svm, &admin, abort.clone()));
        set_epoch2_clock(&mut svm, EPOCH2_FIXTURE_SLOT + 100);
        clock = svm.get_sysvar::<solana_clock::Clock>();
        clock.unix_timestamp = game_state(&svm, &game).phase_ends_at
            + alashi::constants::DONKEY_COMMIT_GRACE_SECONDS;
        svm.set_sysvar(&clock);
        let mut late_commit = abort.clone();
        late_commit.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new(sources[1], false));
        assert!(!send(&mut svm, &admin, late_commit));
        assert!(send(&mut svm, &admin, abort));
        assert_eq!(game_state(&svm, &game).phase, alashi::state::Phase::Aborted);
        assert_eq!(faction_state(&svm, &faction).cash, 0);
        assert_eq!(faction_state(&svm, &faction).goods, 0);
        let before = svm.get_account(&other.pubkey()).unwrap().lamports;
        assert!(send(&mut svm, &admin,
            ix_settle_refund(admin.pubkey(), game,
                vec![faction, other_faction], vec![admin.pubkey(), other.pubkey()])));
        assert_eq!(svm.get_account(&other.pubkey()).unwrap().lamports - before, FEE);
        return;
    }

    let commit_clock = EPOCH2_FIXTURE_SLOT + 100;
    set_epoch2_clock(&mut svm, commit_clock);
    let advance = |source: Pubkey| {
        let mut ix = ix_advance(admin.pubkey(), game, vec![faction, other_faction]);
        ix.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new(oracle, false));
        ix.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new(source, false));
        ix.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new_readonly(sb_key(SB_QUEUE), false));
        ix.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new(sb_key(SB_ORACLE), false));
        ix.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new_readonly(sb_key(SB_DEVNET_PID), false));
        ix
    };
    assert!(!send(&mut svm, &admin, advance(sources[0])));
    let mut wrong_oracle = advance(sources[1]);
    let selected_oracle_index = wrong_oracle.accounts.len() - 2;
    wrong_oracle.accounts[selected_oracle_index] =
        anchor_lang::solana_program::instruction::AccountMeta::new(sb_key(SB_STATE), false);
    assert!(!send(&mut svm, &admin, wrong_oracle));
    let pristine = svm.get_account(&sources[1]).unwrap();
    assert_eq!(u64::from_le_bytes(pristine.data[104..112].try_into().unwrap()), 0);
    let mut stale = pristine.clone();
    stale.data[104..112].copy_from_slice(&(commit_clock - 2).to_le_bytes());
    svm.set_account(sources[1], stale).unwrap();
    assert!(!send(&mut svm, &admin, advance(sources[1])));
    svm.set_account(sources[1], pristine).unwrap();
    assert!(send(&mut svm, &admin, advance(sources[1])));
    assert_eq!(game_state(&svm, &game).phase, alashi::state::Phase::Law);
    assert_eq!(game_state(&svm, &game).commit_slot, commit_clock - 1);
    let account = svm.get_account(&oracle).unwrap();
    let mut data: &[u8] = &account.data;
    let config = alashi::donkey_state::DonkeyOracleConfig::try_deserialize(&mut data).unwrap();
    assert_eq!(config.committed_rounds_mask & 0b10, 0b10);
    assert!(!send(&mut svm, &admin, advance(sources[1])));

    if want_win == Some(true) && !two_tickets {
        let before_source = svm.get_account(&sources[1]).unwrap().data;
        let before_game = svm.get_account(&game).unwrap().data;
        // A forged gateway proof cannot reveal, settle, or mint a ticket.
        svm.expire_blockhash();
        let ix = ix_reveal_donkey_law(admin.pubkey(), game, sources[1], &[faction]);
        let msg = Message::new_with_blockhash(&[ix], Some(&admin.pubkey()), &svm.latest_blockhash());
        let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[&admin]).unwrap();
        let failed = svm.send_transaction(tx).unwrap_err();
        assert!(failed.meta.logs.iter().any(|line| line.contains("Instruction: RandomnessReveal")));
        assert!(failed.meta.logs.iter().any(|line| line.contains("SecpRecoverFailure")));
        assert_eq!(svm.get_account(&sources[1]).unwrap().data, before_source);
        assert_eq!(svm.get_account(&game).unwrap().data, before_game);
        assert_eq!(faction_state(&svm, &faction).goods, 0);
    }

    if want_win.is_none() {
        set_epoch2_clock(&mut svm, commit_clock + 50);
        for _ in 0..=alashi::constants::MAX_VRF_RETRIES {
            set_law_seed(&mut svm, 0);
            let mut ix = ix_advance(admin.pubkey(), game, vec![faction, other_faction]);
            ix.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new(oracle, false));
            assert!(send(&mut svm, &admin, ix));
        }
        assert_eq!(game_state(&svm, &game).phase, alashi::state::Phase::Aborted);
        assert_eq!(faction_state(&svm, &faction).goods, 0);
        assert_eq!(faction_state(&svm, &faction).cash, 0);
        let account = svm.get_account(&oracle).unwrap();
        let mut data: &[u8] = &account.data;
        let config = alashi::donkey_state::DonkeyOracleConfig::try_deserialize(&mut data).unwrap();
        assert!(config.resolved);
        return;
    }

    let win = want_win.unwrap();
    let chosen = (0..=255).find(|value| synthetic_epoch2_draw_is_win(*value, &game, &faction) == win).unwrap();
    let reveal_clock = commit_clock + 2;
    set_epoch2_clock(&mut svm, reveal_clock);
    let mut source = svm.get_account(&sources[1]).unwrap();
    source.data[144..152].copy_from_slice(&reveal_clock.to_le_bytes());
    source.data[152] = chosen;
    svm.set_account(sources[1], source).unwrap();
    let reveal = |include_ticket: bool| {
        let mut ix = ix_reveal_law(admin.pubkey(), game, sources[1]);
        ix.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new(oracle, false));
        if include_ticket {
            ix.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new(faction, false));
            if two_tickets {
                ix.accounts.push(anchor_lang::solana_program::instruction::AccountMeta::new(other_faction, false));
            }
        }
        ix
    };
    assert!(!send(&mut svm, &admin, reveal(false)));
    assert_eq!(faction_state(&svm, &faction).goods, 0);
    if two_tickets {
        let mut omitted = reveal(true);
        omitted.accounts.pop();
        assert!(!send(&mut svm, &admin, omitted));
        assert_eq!(faction_state(&svm, &other_faction).goods, 0);
    }
    assert!(send(&mut svm, &admin, reveal(true)));
    assert_eq!(faction_state(&svm, &faction).goods, if win { 5 } else { 0 });
    assert_eq!(faction_state(&svm, &faction).cash, 0);
    if two_tickets {
        assert_eq!(faction_state(&svm, &other_faction).cash, 0);
        assert_eq!(faction_state(&svm, &other_faction).goods,
            if synthetic_epoch2_draw_is_win(chosen, &game, &other_faction) { 5 } else { 0 });
    }
    assert!(!send(&mut svm, &admin, reveal(true)));
}

#[test]
fn epoch2_verified_win_is_paid_once() {
    synthetic_epoch2_case(Some(true), false, false);
}

#[test]
fn epoch2_verified_empty_is_paid_once() {
    synthetic_epoch2_case(Some(false), false, false);
}

#[test]
fn epoch2_oracle_abort_never_mints_goods() {
    synthetic_epoch2_case(None, false, false);
}

#[test]
fn epoch2_multi_ticket_reveal_requires_complete_ordered_set() {
    synthetic_epoch2_case(Some(true), true, false);
}

#[test]
fn epoch2_uncommitted_action_aborts_after_grace_and_refunds() {
    synthetic_epoch2_case(None, false, true);
}

#[test]
fn test_close_settled_game_rejects_redirection_and_duplicates() {
    let mut party = start_party(902, FEE, 0);
    let crank = Keypair::new();
    party.svm.airdrop(&crank.pubkey(), 1_000_000_000).unwrap();
    let factions = vec![party.fa, party.fb];
    let wallets = vec![party.admin.pubkey(), party.b.pubkey()];
    for _ in 0..19 { assert!(party.advance()); }
    assert!(!send(&mut party.svm, &crank, ix_close_game(crank.pubkey(), party.game, party.admin.pubkey(), factions.clone(), wallets.clone())));
    assert!(send(&mut party.svm, &crank, ix_settle(crank.pubkey(), party.game, factions.clone(), wallets.clone(), party.admin.pubkey())));
    let a_before = party.svm.get_account(&party.admin.pubkey()).unwrap().lamports;
    let b_before = party.svm.get_account(&party.b.pubkey()).unwrap().lamports;
    let game_rent = party.svm.get_account(&party.game).unwrap().lamports;
    let fa_rent = party.svm.get_account(&party.fa).unwrap().lamports;
    let fb_rent = party.svm.get_account(&party.fb).unwrap().lamports;
    for (admin, fs, ws) in [
        (crank.pubkey(), factions.clone(), wallets.clone()),
        (party.admin.pubkey(), factions.clone(), vec![crank.pubkey(), party.b.pubkey()]),
        (party.admin.pubkey(), vec![party.fa, party.fa], vec![party.admin.pubkey(), party.admin.pubkey()]),
        (party.admin.pubkey(), vec![party.fa], vec![party.admin.pubkey()]),
    ] {
        assert!(!send(&mut party.svm, &crank, ix_close_game(crank.pubkey(), party.game, admin, fs, ws)));
        assert_eq!(party.svm.get_account(&party.fa).unwrap().lamports, fa_rent);
        assert_eq!(party.svm.get_account(&party.game).unwrap().lamports, game_rent);
    }
    assert!(send(&mut party.svm, &crank, ix_close_game(crank.pubkey(), party.game, party.admin.pubkey(), factions, wallets)));
    assert_eq!(party.svm.get_account(&party.admin.pubkey()).unwrap().lamports - a_before, game_rent + fa_rent);
    assert_eq!(party.svm.get_account(&party.b.pubkey()).unwrap().lamports - b_before, fb_rent);
}

#[test]
fn test_vrf_future_seed_and_prerevealed_value_are_rejected() {
    let (mut svm, a) = setup();
    let b = Keypair::new();
    svm.airdrop(&b.pubkey(), 2_000_000_000).unwrap();
    let game = game_pda(903);
    let fa = faction_pda(&game, &a.pubkey());
    let fb = faction_pda(&game, &b.pubkey());
    let rng = Keypair::new().pubkey();
    assert!(send(&mut svm, &a, ix_initialize(903, FEE, 0, a.pubkey(), game, 1)));
    assert!(send(&mut svm, &a, ix_join("A", a.pubkey(), game, fa)));
    assert!(send(&mut svm, &b, ix_join("B", b.pubkey(), game, fb)));
    for _ in 0..2 { assert!(send(&mut svm, &a, ix_advance(a.pubkey(), game, vec![fa, fb]))); }
    svm.warp_to_slot(100);
    let slot = svm.get_sysvar::<solana_clock::Clock>().slot;
    let before = svm.get_account(&game).unwrap().data;
    for (seed_slot, reveal_slot) in [(slot, 0), (slot + 100000, 0), (u64::MAX, 0), (slot - 1, slot)] {
        install_rng(&mut svm, &rng, seed_slot, reveal_slot, 4);
        assert!(!send(&mut svm, &a, ix_advance(a.pubkey(), game, vec![fa, fb, rng])));
        assert_eq!(svm.get_account(&game).unwrap().data, before);
    }
    install_rng(&mut svm, &rng, slot - 1, 0, 4);
    assert!(send(&mut svm, &a, ix_advance(a.pubkey(), game, vec![fa, fb, rng])));
}
