use crate::{
    constants::*, donkey_state::DonkeyOracleConfig, error::GameError, events::*, state::*,
};
use anchor_lang::prelude::*;
use solana_sha256_hasher::hashv;
use switchboard_on_demand::accounts::RandomnessAccountData;
use switchboard_on_demand::ON_DEMAND_DEVNET_PID;
use switchboard_on_demand::ON_DEMAND_MAINNET_PID;

#[derive(Accounts)]
pub struct RevealLaw<'info> {
    pub crank: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    /// CHECK: ключ сверен с game.vrf_account, владелец — программа Switchboard
    /// On-Demand, данные парсятся RandomnessAccountData::parse; значение
    /// валидируется get_value в слоте reveal (клиент бандлит revealIx с
    /// этой инструкцией одной транзакцией, подпись оракула проверяет
    /// программа Switchboard в revealIx).
    pub randomness: UncheckedAccount<'info>,
}

pub fn handle_reveal_law(ctx: Context<RevealLaw>) -> Result<()> {
    let game = &mut ctx.accounts.game;
    require!(
        game.entropy_mode == ENTROPY_SWITCHBOARD,
        GameError::InvalidEntropyMode
    );
    require!(game.phase == Phase::Law, GameError::WrongPhase);
    require!(game.law_card == NO_LAW, GameError::LawNotRevealed);
    require!(
        *ctx.accounts.randomness.key == game.vrf_account,
        GameError::RandomnessMismatch
    );
    let rng = ctx.accounts.randomness.to_account_info();
    let owner = rng.owner;
    require!(
        owner.as_ref() == ON_DEMAND_DEVNET_PID.as_ref()
            || owner.as_ref() == ON_DEMAND_MAINNET_PID.as_ref(),
        GameError::RandomnessMismatch
    );
    let data = RandomnessAccountData::parse(rng.data.borrow())
        .map_err(|_| GameError::RandomnessMismatch)?;
    require!(
        data.seed_slot == game.commit_slot,
        GameError::RandomnessMismatch
    );
    let clock = Clock::get()?;
    let value = data
        .get_value(clock.slot)
        .map_err(|_| GameError::RandomnessNotReady)?;
    let is_risk = game.epoch == EPOCH_DONKEY_RISK;
    let mut oracle = if is_risk {
        require!(
            game.round > 0 && game.round <= ROUNDS,
            GameError::DonkeyOracleInvalid
        );
        let ai = ctx
            .remaining_accounts
            .first()
            .ok_or(GameError::DonkeyOracleInvalid)?;
        require!(ai.is_writable, GameError::DonkeyOracleInvalid);
        let config = Account::<DonkeyOracleConfig>::try_from(ai)?;
        let expected =
            Pubkey::find_program_address(&[DONKEY_ORACLE_SEED, game.key().as_ref()], &crate::id())
                .0;
        require_keys_eq!(config.key(), expected, GameError::DonkeyOracleInvalid);
        require_keys_eq!(config.game, game.key(), GameError::DonkeyOracleInvalid);
        require_keys_eq!(
            config.accounts[(game.round - 1) as usize],
            rng.key(),
            GameError::RandomnessMismatch
        );
        require!(
            data.queue.as_ref() == config.queue.as_ref()
                && data.authority.as_ref() == config.authority.as_ref(),
            GameError::RandomnessMismatch
        );
        let due = config.round == game.round && !config.resolved;
        require!(due || config.resolved, GameError::DonkeyOracleInvalid);
        require!(
            ctx.remaining_accounts.len() == 1 + if due { config.ticket_count as usize } else { 0 },
            GameError::InvalidFactionSet
        );
        Some(config)
    } else {
        require!(
            ctx.remaining_accounts.is_empty(),
            GameError::InvalidFactionSet
        );
        None
    };
    let mut b = [0u8; 8];
    b.copy_from_slice(&value[..8]);
    let seed = u64::from_le_bytes(b);

    // Аудит 27.09 (S1): вывод карты — общее правило ядра, включая
    // особую карту «взаимозачёт» эпохи 90-х (раньше ветка жила только
    // в слот-хэш пути advance)
    let card = alashi_rules::transitions::reveal_law(game, seed)?;
    if let Some(config) = oracle.as_mut() {
        let due_count = if config.round == game.round && !config.resolved {
            config.ticket_count as usize
        } else {
            0
        };
        for (i, ticket) in config.tickets[..due_count].iter().enumerate() {
            let ai = &ctx.remaining_accounts[i + 1];
            require_keys_eq!(*ai.key, *ticket, GameError::InvalidFactionSet);
            require!(ai.is_writable, GameError::InvalidFactionSet);
            let mut faction = Account::<Faction>::try_from(ai)?;
            require_keys_eq!(faction.game, game.key(), GameError::InvalidFactionSet);
            let win = donkey_success(&value, &game.key(), ticket, game.round)?;
            let goods = alashi_rules::actions::donkey_risk_resolve(&mut faction, win)?;
            faction.exit(&crate::id())?;
            emit!(DonkeyResolved {
                game: game.key(),
                faction: *ticket,
                round: game.round,
                success: win,
                goods,
                fee: DONKEY_RISK_FEE,
                reason: 0,
            });
        }
        config.resolved = true;
        config.exit(&crate::id())?;
    }
    emit!(LawDrawn {
        game: game.key(),
        round: game.round,
        card,
    });
    Ok(())
}

/// Rejection sampling is exact: 2^256 - 1 is divisible by 5, so the only
/// rejected digest is all 0xff. A domain-separated rehash supplies the retry.
fn donkey_success(vrf: &[u8; 32], game: &Pubkey, faction: &Pubkey, round: u8) -> Result<bool> {
    let mut counter = 0u32;
    loop {
        let digest = hashv(&[
            b"alashi-donkey-risk-v1".as_slice(),
            vrf.as_slice(),
            game.as_ref(),
            faction.as_ref(),
            &[round],
            &counter.to_le_bytes(),
        ])
        .to_bytes();
        if let Some(success) = digest_success(&digest) {
            return Ok(success);
        }
        counter = counter
            .checked_add(1)
            .ok_or(GameError::RandomnessNotReady)?;
    }
}

fn digest_success(digest: &[u8; 32]) -> Option<bool> {
    if *digest == [0xff; 32] {
        return None;
    }
    let residue = digest.iter().fold(0u8, |r, b| {
        ((r as u16 * 256 + *b as u16) % DONKEY_RISK_OUTCOMES as u16) as u8
    });
    Some(residue < DONKEY_RISK_SUCCESS_COUNT)
}

#[cfg(test)]
mod donkey_rng_tests {
    use super::*;

    #[test]
    fn exact_three_of_five_and_rejection_boundary() {
        for (last, expected) in [(0, true), (1, true), (2, true), (3, false), (4, false)] {
            let mut digest = [0; 32];
            digest[31] = last;
            assert_eq!(digest_success(&digest), Some(expected));
        }
        let mut threshold_minus_one = [0xff; 32];
        threshold_minus_one[31] = 0xfe;
        assert_eq!(digest_success(&threshold_minus_one), Some(false));
        assert_eq!(digest_success(&[0xff; 32]), None);
    }

    #[test]
    fn same_inputs_are_stable_and_distinct_factions_are_domain_separated() {
        let vrf = [9; 32];
        let game = Pubkey::new_from_array([1; 32]);
        let a = Pubkey::new_from_array([2; 32]);
        let b = Pubkey::new_from_array([3; 32]);
        assert_eq!(
            donkey_success(&vrf, &game, &a, 2).unwrap(),
            donkey_success(&vrf, &game, &a, 2).unwrap()
        );
        let digest = |f: &Pubkey, round: u8| {
            hashv(&[
                b"alashi-donkey-risk-v1".as_slice(),
                vrf.as_slice(),
                game.as_ref(),
                f.as_ref(),
                &[round],
                &0u32.to_le_bytes(),
            ])
            .to_bytes()
        };
        assert_ne!(digest(&a, 2), digest(&b, 2));
        assert_ne!(digest(&a, 2), digest(&a, 3));
    }
}
