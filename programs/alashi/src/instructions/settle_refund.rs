use crate::{constants::*, error::GameError, events::*, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct SettleRefund<'info> {
    pub crank: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
}

pub fn handle_settle_refund<'a>(ctx: Context<'a, SettleRefund<'a>>) -> Result<()> {
    let game = &mut ctx.accounts.game;
    require!(game.phase == Phase::Aborted, GameError::NotFinished);
    require!(!game.settled, GameError::AlreadySettled);

    let k = game.faction_count as usize;
    let rem = &ctx.remaining_accounts;
    require!(rem.len() == 2 * k, GameError::InvalidSettleSet);

    let factions: Vec<Account<'_, Faction>> = rem[..k]
        .iter()
        .map(|ai| Account::<Faction>::try_from(ai))
        .collect::<anchor_lang::Result<Vec<_>>>()?;
    for (i, f) in factions.iter().enumerate() {
        require!(f.game == game.key(), GameError::InvalidSettleSet);
        require!(f.wallet == rem[k + i].key(), GameError::InvalidSettleSet);
    }

    let rent = Rent::get()?.minimum_balance(8 + Game::INIT_SPACE);
    let bank = **game.to_account_info().lamports.borrow();
    require!(bank > rent, GameError::EmptyBank);
    let pot = bank - rent;

    // Осознанный trade-off (решение владельца): расходы на попытки VRF
    // (vrf_spent, платил кранкер) НЕ компенсируются из отдельного фонда —
    // при редком сбое оракула фракции теряют комиссию попыток.
    let share = pot / k as u64;
    let bank_info = game.to_account_info();
    for i in 0..k {
        if share == 0 {
            break;
        }
        **bank_info.lamports.borrow_mut() -= share;
        **rem[k + i].lamports.borrow_mut() += share;
    }

    game.settled = true;
    emit!(Settled {
        game: game.key(),
        pot,
        rake: 0,
        paid: share * k as u64,
    });
    Ok(())
}
