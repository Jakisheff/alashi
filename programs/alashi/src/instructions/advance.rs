use crate::{constants::*, error::GameError, events::*, state::*};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct Advance<'info> {
    pub crank: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
}

pub fn handle_advance(ctx: Context<Advance>) -> Result<()> {
    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    let checked: Vec<Account<'_, Faction>> = ctx
        .remaining_accounts
        .iter()
        .map(|ai| Account::<Faction>::try_from(ai))
        .collect::<anchor_lang::Result<Vec<_>>>()?;

    let game = &mut ctx.accounts.game;
    require!(
        checked.len() == game.faction_count as usize,
        GameError::InvalidFactionSet
    );

    for (i, f) in checked.iter().enumerate() {
        require!(f.game == game.key(), GameError::InvalidFactionSet);
        for (j, g) in checked.iter().enumerate() {
            if i != j {
                require!(f.key() != g.key(), GameError::InvalidFactionSet);
            }
        }
    }

    let stamp = game.stamp();
    match game.phase {
        Phase::Lobby => {
            require!(
                game.faction_count >= MIN_FACTIONS,
                GameError::NotEnoughFactions
            );
            require!(
                now >= game.phase_ends_at || game.faction_count == MAX_FACTIONS,
                GameError::TooEarly
            );
            game.round = 1;
            game.phase = Phase::Market;
        }
        Phase::Market => {
            require!(now >= game.phase_ends_at, GameError::TooEarly);
            game.phase = Phase::Action;
        }
        Phase::Action => {
            require!(now >= game.phase_ends_at, GameError::TooEarly);
            game.phase = Phase::Law;
        }
        Phase::Law => {
            require!(now >= game.phase_ends_at, GameError::TooEarly);
            let mut yes: u32 = 0;
            let mut no: u32 = 0;
            for f in checked.iter() {
                if !f.alive || f.voted_stamp != stamp {
                    continue;
                }
                match f.vote {
                    VoteChoice::Yes => yes += f.influence as u32,
                    VoteChoice::No => no += f.influence as u32,
                    VoteChoice::Abstain => {}
                }
            }
            let passed = yes > no;
            game.yes_influence = yes;
            game.no_influence = no;
            game.last_law_passed = passed;
            if passed {
                game.laws_passed += 1;
            }
            emit!(LawResult {
                game: game.key(),
                round: game.round,
                yes,
                no,
                passed,
            });
            if game.round >= ROUNDS {
                game.phase = Phase::Finished;
            } else {
                game.round += 1;
                game.phase = Phase::Market;
            }
        }
        Phase::Finished => return Err(GameError::GameFinished.into()),
    }

    if game.phase == Phase::Market {
        game.sold_this_round = 0;
    }
    game.phase_ends_at = now.saturating_add(game.phase_duration);

    emit!(PhaseAdvanced {
        game: game.key(),
        round: game.round,
        phase: game.phase,
    });
    Ok(())
}
