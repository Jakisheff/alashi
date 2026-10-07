use crate::{
    constants::*,
    error::GameError,
    events::*,
    state::*,
};
use alashi_rules::logic::compute_settlement_epoch;
use anchor_lang::prelude::*;
use std::ops::Deref;

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
    // прозрачные копии для rules (общая математика сеттла)
    let plain: Vec<Faction> = factions.iter().map(|f| f.deref().clone()).collect();
    for (i, f) in factions.iter().enumerate() {
        require!(f.game == game.key(), GameError::InvalidSettleSet);
        require!(f.wallet == rem[k + i].key(), GameError::InvalidSettleSet);
        // R2 (REVIEW_EXTERNAL): дедуп как в advance — без него кранкер
        // подаёт один Faction k раз и забирает 95% банка одной транзакцией.
        for (j, g) in factions.iter().enumerate() {
            if i != j {
                require!(f.key() != g.key(), GameError::InvalidSettleSet);
            }
        }
    }
    require!(rem[2 * k].key() == game.admin, GameError::InvalidSettleSet);

    let reserve = Rent::get()?.minimum_balance(8 + Game::INIT_SPACE);
    let bank = **game.to_account_info().lamports.borrow();
    // Единая математика с ареной (rules), но external_rent=false:
    // ончейн v1 не платит ренту лицензии в lamports — источник
    // отсутствует (ставки — внутренние alashi; эскроу-вариант запаркован).
    // Завод (из рейка) и ранги cash+hard работают полностью.
    let plan = compute_settlement_epoch(game, &plain, bank, reserve, false, false)?;

    let bank_info = game.to_account_info();
    let mut paid: u64 = 0;
    for line in plan.lines.iter() {
        if line.total == 0 {
            continue;
        }
        **bank_info.lamports.borrow_mut() -= line.total;
        **rem[k + line.idx].lamports.borrow_mut() += line.total;
        paid += line.total;
        emit!(Payout {
            game: game.key(),
            wallet: line.wallet,
            rank: line.rank,
            amount: line.total,
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
        paid,
    });
    Ok(())
}
