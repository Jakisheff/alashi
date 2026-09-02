//! SPEC_EPOCH_90S: инструкции M-действий (epoch=1). Логика целиком в
//! alashi_rules::actions (общая с симулятором), здесь — контексты,
//! события и тонкие обёртки. Все игровые деньги — внутренние поля
//! аккаунтов (песо), lamports не двигаются (кроме join/settle).

use crate::{events::*, state::*};
use alashi_rules::actions;
use alashi_rules::constants::*;
use anchor_lang::prelude::*;

// ---------- M4: продажа в кредит (вексель) ----------

#[derive(Accounts)]
pub struct SellCredit<'info> {
    pub player: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
}

pub fn handle_sell_credit(ctx: Context<SellCredit>, units: u16) -> Result<()> {
    let game = &mut ctx.accounts.game;
    let faction = &mut ctx.accounts.faction;
    let promissory = actions::sell(game, faction, units, true)?;
    emit!(SoldCredit {
        game: game.key(),
        faction: faction.key(),
        units,
        promissory,
    });
    Ok(())
}

// ---------- M3: челнок (серый товар) ----------

#[derive(Accounts)]
pub struct Shuttle<'info> {
    pub player: Signer<'info>,
    #[account(
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
}

pub fn handle_shuttle(ctx: Context<Shuttle>) -> Result<()> {
    let game = &mut ctx.accounts.game;
    let faction = &mut ctx.accounts.faction;
    let goods = actions::shuttle(game, faction)?;
    emit!(Shuttled {
        game: game.key(),
        faction: faction.key(),
        goods,
        grey: faction.grey_goods,
    });
    Ok(())
}

// ---------- M2+M7: крыша (платёж фракции-крыше) ----------

#[derive(Accounts)]
pub struct Roof<'info> {
    pub player: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), target.wallet.as_ref()],
        bump = target.bump,
        has_one = game
    )]
    pub target: Account<'info, Faction>,
}

pub fn handle_roof(ctx: Context<Roof>, tariff: u8) -> Result<()> {
    let before = ctx.accounts.faction.cash;
    let krysha = actions::roof(
        &mut ctx.accounts.game,
        &mut ctx.accounts.faction,
        &mut ctx.accounts.target,
        tariff,
    )?;
    emit!(RoofBought {
        game: ctx.accounts.game.key(),
        from: ctx.accounts.faction.key(),
        to: krysha,
        tariff,
        price: before - ctx.accounts.faction.cash,
    });
    Ok(())
}

// ---------- M8: президент объявляет границу вслепую ----------

#[derive(Accounts)]
pub struct SetCustoms<'info> {
    pub player: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
}

pub fn handle_set_customs(ctx: Context<SetCustoms>, tight: bool) -> Result<()> {
    let game = &mut ctx.accounts.game;
    let faction = &ctx.accounts.faction;
    actions::set_customs(game, faction, tight)?;
    emit!(CustomsSet {
        game: game.key(),
        president: faction.key(),
        tight,
    });
    Ok(())
}

// ---------- M9: ставка на слепой аукцион лицензии ----------

#[derive(Accounts)]
pub struct BidLicense<'info> {
    pub player: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
}

pub fn handle_bid_license(ctx: Context<BidLicense>, amount: u64) -> Result<()> {
    let total = actions::bid_license(&mut ctx.accounts.game, &mut ctx.accounts.faction, amount)?;
    emit!(LicenseBid {
        game: ctx.accounts.game.key(),
        faction: ctx.accounts.faction.key(),
        amount,
        total_bid: total,
    });
    Ok(())
}

// ---------- M9: инсайд о доходности лицензии ----------

#[derive(Accounts)]
pub struct InspectLicense<'info> {
    pub player: Signer<'info>,
    #[account(
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
}

pub fn handle_inspect_license(ctx: Context<InspectLicense>) -> Result<()> {
    let yield_amount = actions::inspect_license(&ctx.accounts.game, &mut ctx.accounts.faction)?;
    emit!(LicenseInsight {
        game: ctx.accounts.game.key(),
        faction: ctx.accounts.faction.key(),
        yield_amount,
    });
    Ok(())
}

// ---------- M6: валютчик (весь кэш <-> твёрдая валюта) ----------

#[derive(Accounts)]
pub struct Exchange<'info> {
    pub player: Signer<'info>,
    #[account(
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
}

pub fn handle_exchange(ctx: Context<Exchange>, to_hard: bool) -> Result<()> {
    let got = actions::exchange(&ctx.accounts.game, &mut ctx.accounts.faction, to_hard)?;
    emit!(Exchanged {
        game: ctx.accounts.game.key(),
        faction: ctx.accounts.faction.key(),
        to_hard,
        got,
    });
    Ok(())
}

// ---------- M10 шаг 1: офер продажи голоса ----------

#[derive(Accounts)]
pub struct OfferVote<'info> {
    pub player: Signer<'info>,
    #[account(
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
    #[account(
        seeds = [FACTION_SEED, game.key().as_ref(), buyer.wallet.as_ref()],
        bump = buyer.bump,
        has_one = game
    )]
    pub buyer: Account<'info, Faction>,
}

pub fn handle_offer_vote(ctx: Context<OfferVote>, price: u64) -> Result<()> {
    actions::offer_vote(
        &ctx.accounts.game,
        &mut ctx.accounts.faction,
        ctx.accounts.buyer.wallet,
        ctx.accounts.buyer.cash,
        price,
    )?;
    emit!(VoteOffered {
        game: ctx.accounts.game.key(),
        seller: ctx.accounts.faction.key(),
        buyer: ctx.accounts.buyer.key(),
        price,
    });
    Ok(())
}

// ---------- M10 шаг 2: акцепт покупки голоса ----------

#[derive(Accounts)]
pub struct AcceptVoteOffer<'info> {
    pub player: Signer<'info>,
    #[account(
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), seller.wallet.as_ref()],
        bump = seller.bump,
        has_one = game
    )]
    pub seller: Account<'info, Faction>,
}

pub fn handle_accept_vote_offer(ctx: Context<AcceptVoteOffer>) -> Result<()> {
    let price = actions::accept_vote_offer(
        &ctx.accounts.game,
        &mut ctx.accounts.faction,
        &mut ctx.accounts.seller,
    )?;
    emit!(VoteSold {
        game: ctx.accounts.game.key(),
        buyer: ctx.accounts.faction.key(),
        seller: ctx.accounts.seller.key(),
        price,
    });
    Ok(())
}

// ---------- M11: бартер ----------

#[derive(Accounts)]
pub struct BarterPropose<'info> {
    pub player: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
}

pub fn handle_barter_propose(
    ctx: Context<BarterPropose>,
    goods: u16,
    price: u64,
) -> Result<()> {
    let id = actions::barter_propose(
        &mut ctx.accounts.game,
        &ctx.accounts.faction,
        None,
        goods,
        price,
    )?;
    emit!(BarterProposed {
        game: ctx.accounts.game.key(),
        from: ctx.accounts.faction.key(),
        offer: id,
        goods,
        price,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct BarterAccept<'info> {
    pub player: Signer<'info>,
    #[account(
        mut,
        seeds = [GAME_SEED, game.game_id.to_le_bytes().as_ref()],
        bump = game.bump
    )]
    pub game: Account<'info, Game>,
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), player.key().as_ref()],
        bump = faction.bump,
        has_one = game
    )]
    pub faction: Account<'info, Faction>,
    /// продавец товара из офера; валидация соответствия — в actions
    #[account(
        mut,
        seeds = [FACTION_SEED, game.key().as_ref(), seller.wallet.as_ref()],
        bump = seller.bump,
        has_one = game
    )]
    pub seller: Account<'info, Faction>,
}

pub fn handle_barter_accept(ctx: Context<BarterAccept>, offer_id: u64) -> Result<()> {
    actions::barter_accept(
        &mut ctx.accounts.game,
        &mut ctx.accounts.faction,
        &mut ctx.accounts.seller,
        offer_id,
    )?;
    emit!(BarterAccepted {
        game: ctx.accounts.game.key(),
        by: ctx.accounts.faction.key(),
        from: ctx.accounts.seller.key(),
        offer: offer_id,
    });
    Ok(())
}
