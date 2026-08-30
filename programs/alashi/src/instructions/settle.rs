use crate::{constants::*, error::GameError, events::*, state::*};
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
    for (i, f) in factions.iter().enumerate() {
        require!(f.game == game.key(), GameError::InvalidSettleSet);
        require!(f.wallet == rem[k + i].key(), GameError::InvalidSettleSet);
    }
    require!(rem[2 * k].key() == game.admin, GameError::InvalidSettleSet);

    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|&a, &b| {
        let ca = factions[a].cash;
        let cb = factions[b].cash;
        if ca != cb {
            cb.cmp(&ca)
        } else {
            factions[a].wallet.cmp(&factions[b].wallet)
        }
    });

    let rent = Rent::get()?;
    let data_len = 8 + Game::INIT_SPACE;
    let reserve = rent.minimum_balance(data_len);
    let bank = **game.to_account_info().lamports.borrow();
    require!(bank > reserve, GameError::EmptyBank);
    let pot = bank - reserve;

    let rake = pot * game.rake_bps as u64 / 10_000;
    let payout_total = pot - rake;

    let share_count = k.min(PAYOUT_SHARES.len());
    let total_shares: u64 = PAYOUT_SHARES[..share_count].iter().sum();
    require!(total_shares > 0, GameError::EmptyBank);

    let bank_info = game.to_account_info();
    let others_total: u64 = (1..share_count)
        .map(|r| payout_total * PAYOUT_SHARES[r] / total_shares)
        .sum();
    let rank0_amount = payout_total.saturating_sub(others_total);

    let mut paid: u64 = 0;
    for (rank, &fi) in order.iter().enumerate() {
        if rank >= share_count {
            break;
        }
        let amount = if rank == 0 {
            rank0_amount
        } else {
            payout_total * PAYOUT_SHARES[rank] / total_shares
        };
        if amount == 0 {
            continue;
        }
        **bank_info.lamports.borrow_mut() -= amount;
        **rem[k + fi].lamports.borrow_mut() += amount;
        paid += amount;
        emit!(Payout {
            game: game.key(),
            wallet: rem[k + fi].key(),
            rank: rank as u8,
            amount,
        });
    }

    if rake > 0 {
        **bank_info.lamports.borrow_mut() -= rake;
        **rem[2 * k].lamports.borrow_mut() += rake;
    }

    game.settled = true;
    emit!(Settled {
        game: game.key(),
        pot,
        rake,
        paid,
    });
    Ok(())
}
