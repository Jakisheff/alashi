use crate::{constants::*, error::GameError, events::*, logic::draw_law_index, state::*};
use anchor_lang::prelude::*;
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
    let mut b = [0u8; 8];
    b.copy_from_slice(&value[..8]);
    let seed = u64::from_le_bytes(b);

    let (card, mask) = draw_law_index(seed, game.laws_used_mask);
    game.law_card = card;
    game.laws_used_mask = mask;
    emit!(LawDrawn {
        game: game.key(),
        round: game.round,
        card,
    });
    Ok(())
}
