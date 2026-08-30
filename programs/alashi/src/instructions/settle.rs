use crate::{
    constants::*,
    error::GameError,
    events::*,
    logic::{compute_settlement, FactionSnapshot},
    state::*,
};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Settle<'info> {
    pub crank: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
}

pub fn handle_settle<'a>(ctx: Context<'a, Settle<'a>>) -> Result<()> {
    let game = &mut ctx.accounts.game;
    require!(game.phase == Phase::Finished, GameError::NotFinished);
    require!(!game.settled, GameError::AlreadySettled);

    let k = game.faction_count as usize;
    let rem = &ctx.remaining_accounts;
    require!(rem.len() == 2 * k + 1, GameError::InvalidSettleSet);

    let factions: Vec<Account<'_, Faction>> = rem[..k]
        .iter()
        .map(|ai| Account::<Faction>::try_from(ai))
        .collect::<anchor_lang::Result<Vec<_>>>()?;
    let snapshots: Vec<FactionSnapshot> = factions.iter().map(|f| f.into()).collect();
    for (i, f) in factions.iter().enumerate() {
        require!(f.game == game.key(), GameError::InvalidSettleSet);
        require!(f.wallet == rem[k + i].key(), GameError::InvalidSettleSet);
    }
    require!(rem[2 * k].key() == game.admin, GameError::InvalidSettleSet);

    let reserve = Rent::get()?.minimum_balance(8 + Game::INIT_SPACE);
    let bank = **game.to_account_info().lamports.borrow();
    let plan = compute_settlement(&snapshots, bank, reserve, game.rake_bps, &PAYOUT_SHARES)?;

    let bank_info = game.to_account_info();
    for p in plan.payouts.iter() {
        **bank_info.lamports.borrow_mut() -= p.amount;
        **rem[k + p.faction_index].lamports.borrow_mut() += p.amount;
        emit!(Payout {
            game: game.key(),
            wallet: p.wallet,
            rank: p.rank,
            amount: p.amount,
        });
    }
    if plan.rake > 0 {
        **bank_info.lamports.borrow_mut() -= plan.rake;
        **rem[2 * k].lamports.borrow_mut() += plan.rake;
    }

    game.settled = true;
    emit!(Settled {
        game: game.key(),
        pot: plan.pot,
        rake: plan.rake,
        paid: plan.payouts.iter().map(|p| p.amount).sum(),
    });
    Ok(())
}
