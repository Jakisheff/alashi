//! Чистые функции игровых действий: единая логика для офчейн-симулятора
//! и ончейн-инструкций (ARCH L1: правила живут в rules, обёртки тонкие).
//! Порядок проверок и ошибки — канон, Simulator делегирует сюда.

use crate::constants::*;
use crate::error::GameError;
use crate::logic::{compute_purchase, compute_sale};
use crate::state::{Faction, Game, Phase, VoteChoice};
use anchor_lang::prelude::Pubkey;

// ---------- фаза Market ----------

/// Продажа: обычная или в кредит (M4, epoch=1, выручка ×1.25 векселем).
pub fn sell(
    game: &mut Game,
    f: &mut Faction,
    units: u16,
    credit: bool,
) -> Result<u64, GameError> {
    if credit && game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if game.phase != Phase::Market {
        return Err(GameError::WrongPhase);
    }
    if !f.alive {
        return Err(GameError::NotAlive);
    }
    if f.acted_stamp == game.stamp() {
        return Err(GameError::AlreadyActed);
    }
    if units == 0 {
        return Err(GameError::NoUnits);
    }
    if units > f.goods {
        return Err(GameError::NotEnoughGoods);
    }
    let trade = compute_sale(
        units,
        game.sold_this_round,
        game.active_price_shift,
        game.active_boom,
    );
    let tax = trade.gross * game.active_tax_bps as u64 / 10_000;
    let revenue = trade.gross - tax;
    let revenue = if credit {
        revenue * CREDIT_NUM / CREDIT_DEN
    } else {
        revenue
    };
    game.sold_this_round = trade.counter_after;
    f.goods -= units;
    if credit {
        f.promissory += revenue;
    } else {
        f.cash += revenue;
    }
    f.acted_stamp = game.stamp();
    Ok(revenue)
}

pub fn buy(game: &mut Game, f: &mut Faction, units: u16) -> Result<u64, GameError> {
    if game.phase != Phase::Market {
        return Err(GameError::WrongPhase);
    }
    if !f.alive {
        return Err(GameError::NotAlive);
    }
    if f.acted_stamp == game.stamp() {
        return Err(GameError::AlreadyActed);
    }
    if units == 0 {
        return Err(GameError::NoUnits);
    }
    let trade = compute_purchase(
        units,
        game.sold_this_round,
        game.active_price_shift,
        game.active_boom,
    );
    if f.cash < trade.gross {
        return Err(GameError::NotEnoughCash);
    }
    f.cash -= trade.gross;
    f.goods += units;
    game.sold_this_round = trade.counter_after;
    f.acted_stamp = game.stamp();
    Ok(trade.gross)
}

/// M11: предложить бартер (товар за кэш напрямую, рынок не двигается).
/// Оферы хранятся в Game (ончейн-хранилище = симулятор байт-в-байт).
pub fn barter_propose(
    game: &mut Game,
    f: &Faction,
    to: Option<Pubkey>,
    goods: u16,
    price: u64,
) -> Result<u64, GameError> {
    if game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if game.phase != Phase::Market {
        return Err(GameError::WrongPhase);
    }
    if goods == 0 || goods > f.goods {
        return Err(GameError::NotEnoughGoods);
    }
    if game.barter_offers.len() >= MAX_BARTER_OFFERS {
        return Err(GameError::TooManyOffers);
    }
    let id = game.barter_next_id;
    game.barter_next_id += 1;
    game.barter_offers.push(crate::state::BarterOfferRec {
        id,
        from: f.wallet,
        to: to.unwrap_or(Pubkey::default()),
        goods,
        price,
    });
    Ok(id)
}

/// M11: принять бартерный офер (оплата кэшем, товар переходит).
pub fn barter_accept(
    game: &mut Game,
    f_by: &mut Faction,
    f_from: &mut Faction,
    offer_id: u64,
) -> Result<(), GameError> {
    if game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if game.phase != Phase::Market {
        return Err(GameError::WrongPhase);
    }
    let pos = game
        .barter_offers
        .iter()
        .position(|o| o.id == offer_id)
        .ok_or(GameError::InvalidFactionSet)?;
    let offer = game.barter_offers[pos];
    if offer.from == f_by.wallet {
        return Err(GameError::SelfBribe);
    }
    if offer.to != Pubkey::default() && offer.to != f_by.wallet {
        return Err(GameError::InvalidFactionSet);
    }
    if offer.from != f_from.wallet {
        return Err(GameError::InvalidFactionSet);
    }
    if f_from.goods < offer.goods {
        return Err(GameError::NotEnoughGoods);
    }
    if f_by.cash < offer.price {
        return Err(GameError::NotEnoughCash);
    }
    f_by.cash -= offer.price;
    f_from.cash += offer.price;
    f_from.goods -= offer.goods;
    f_by.goods += offer.goods;
    game.barter_offers.remove(pos);
    Ok(())
}

// ---------- фаза Action ----------

pub fn produce(game: &Game, f: &mut Faction) -> Result<u16, GameError> {
    if game.phase != Phase::Action {
        return Err(GameError::WrongPhase);
    }
    if !f.alive {
        return Err(GameError::NotAlive);
    }
    if f.acted_stamp == game.stamp() {
        return Err(GameError::AlreadyActed);
    }
    f.goods += PRODUCE_YIELD + game.active_subsidy_goods as u16;
    f.acted_stamp = game.stamp();
    Ok(f.goods)
}

/// M3: серый канал «челнок» (+3 товара, серой маркер до закрытия фазы).
pub fn shuttle(game: &Game, f: &mut Faction) -> Result<u16, GameError> {
    if game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if game.phase != Phase::Action {
        return Err(GameError::WrongPhase);
    }
    if !f.alive {
        return Err(GameError::NotAlive);
    }
    if f.acted_stamp == game.stamp() {
        return Err(GameError::AlreadyActed);
    }
    f.goods += SHUTTLE_GOODS + game.active_subsidy_goods as u16;
    f.grey_goods += SHUTTLE_GOODS + game.active_subsidy_goods as u16;
    f.acted_stamp = game.stamp();
    Ok(f.goods)
}

pub fn donkey(game: &Game, f: &mut Faction) -> Result<(), GameError> {
    if game.phase != Phase::Action {
        return Err(GameError::WrongPhase);
    }
    if !f.alive {
        return Err(GameError::NotAlive);
    }
    if f.acted_stamp == game.stamp() {
        return Err(GameError::AlreadyActed);
    }
    let price = DONKEY_PRICE * PESO;
    if f.cash < price {
        return Err(GameError::NotEnoughCash);
    }
    f.cash -= price;
    f.goods += 1;
    f.acted_stamp = game.stamp();
    Ok(())
}

/// R1 (REVIEW_EXTERNAL): индексы валидируются вызывающей стороной ДО
/// обращения к массивам — и симулятор, и ончейн-контекст обязаны
/// проверить границы первыми делом.
pub fn bribe(
    game: &Game,
    f_from: &mut Faction,
    f_to: &mut Faction,
    amount: u64,
) -> Result<u16, GameError> {
    if game.phase != Phase::Action {
        return Err(GameError::WrongPhase);
    }
    if !f_from.alive || !f_to.alive {
        return Err(GameError::NotAlive);
    }
    if f_from.acted_stamp == game.stamp() {
        return Err(GameError::AlreadyActed);
    }
    if f_from.wallet == f_to.wallet {
        return Err(GameError::SelfBribe);
    }
    let gain = amount / BRIBE_PRICE;
    if gain == 0 {
        return Err(GameError::BribeTooSmall);
    }
    if f_from.cash < amount {
        return Err(GameError::NotEnoughCash);
    }
    if f_from.influence as u64 + gain > MAX_INFLUENCE as u64 {
        return Err(GameError::BribeTooBig);
    }
    f_from.cash -= amount;
    f_to.cash += amount;
    f_from.influence += gain as u16;
    f_from.acted_stamp = game.stamp();
    Ok(f_from.influence)
}

/// M2+M7: крыша-контракт с тарифом (чёрный 30% / красный 10%),
/// платёж уходит фракции-крыше.
pub fn roof(
    game: &Game,
    f_from: &mut Faction,
    f_to: &mut Faction,
    tariff: u8,
) -> Result<Pubkey, GameError> {
    if game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if tariff != ROOF_BLACK && tariff != ROOF_RED {
        return Err(GameError::InvalidVoteWeightMode);
    }
    if game.phase != Phase::Action {
        return Err(GameError::WrongPhase);
    }
    if f_from.wallet == f_to.wallet {
        return Err(GameError::SelfBribe);
    }
    if f_from.roof_armed {
        return Err(GameError::AlreadyActed);
    }
    let (num, den) = if tariff == ROOF_BLACK {
        (ROOF_BLACK_NUM, ROOF_BLACK_DEN)
    } else {
        (ROOF_RED_NUM, ROOF_RED_DEN)
    };
    let price = f_from.cash * num / den;
    if price == 0 || f_from.cash < price {
        return Err(GameError::NotEnoughCash);
    }
    f_from.cash -= price;
    f_to.cash += price;
    f_from.roof_to = f_to.wallet;
    f_from.roof_armed = true;
    f_from.roof_tariff = tariff;
    f_from.acted_stamp = game.stamp();
    Ok(f_to.wallet)
}

/// M8: президент вслепую выбирает режим границы (не сжигает ход).
pub fn set_customs(
    game: &mut Game,
    f: &Faction,
    tight: bool,
) -> Result<(), GameError> {
    if game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if game.phase != Phase::Action {
        return Err(GameError::WrongPhase);
    }
    if game.customs_decided {
        return Err(GameError::AlreadyActed);
    }
    if f.wallet != game.president {
        return Err(GameError::NotPresident);
    }
    game.customs_tight = tight;
    game.customs_decided = true;
    Ok(())
}

/// M9: ставка на слепой аукцион лицензии (эскроу кэша).
pub fn bid_license(
    game: &mut Game,
    f: &mut Faction,
    amount: u64,
) -> Result<u64, GameError> {
    if game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if game.phase != Phase::Action || game.round != AUCTION_ROUND {
        return Err(GameError::WrongPhase);
    }
    if f.cash < amount || amount == 0 {
        return Err(GameError::NotEnoughCash);
    }
    f.cash -= amount;
    f.bid += amount;
    Ok(f.bid)
}

/// M9: инсайд о доходности лицензии (5M). ВНИМАНИЕ (ончейн-оговорка):
/// в аккаунте Game доходность публична (данные аккаунтов читаемы),
/// настоящий «инсайд» существует только на HTTP-арене, где state
/// скрывает yield до покупки. Ончейн-инструкция сохраняет плату и
/// флаг (байт-в-байт с симулятором), скрывание — задача VRF-режима.
pub fn inspect_license(game: &Game, f: &mut Faction) -> Result<u64, GameError> {
    if game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if game.license_yield == 0 || !matches!(game.phase, Phase::Market | Phase::Action | Phase::Law) {
        return Err(GameError::WrongPhase);
    }
    if f.insider {
        return Err(GameError::AlreadyActed);
    }
    if f.cash < LICENSE_INSIGHT_PRICE {
        return Err(GameError::NotEnoughCash);
    }
    f.cash -= LICENSE_INSIGHT_PRICE;
    f.insider = true;
    Ok(game.license_yield)
}

/// M6: валютчик — обмен всего кэша ↔ твёрдой валюты ×0.8 (спред 20%).
/// Сервисная операция: любая игровая фаза, ход не сжигает.
pub fn exchange(game: &Game, f: &mut Faction, to_hard: bool) -> Result<u64, GameError> {
    if game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if !matches!(game.phase, Phase::Market | Phase::Action | Phase::Law) {
        return Err(GameError::WrongPhase);
    }
    if to_hard {
        let amount = f.cash;
        let got = amount * EXCHANGE_NUM / EXCHANGE_DEN;
        if got == 0 {
            return Err(GameError::NotEnoughCash);
        }
        f.cash -= amount;
        f.hard += got;
        Ok(got)
    } else {
        let hard = f.hard;
        let got = hard * EXCHANGE_NUM / EXCHANGE_DEN;
        if got == 0 {
            return Err(GameError::NotEnoughCash);
        }
        f.hard -= hard;
        f.cash += got;
        Ok(got)
    }
}

// ---------- фаза Law ----------

pub fn vote(game: &Game, f: &mut Faction, choice: VoteChoice) -> Result<(), GameError> {
    if game.phase != Phase::Law {
        return Err(GameError::WrongPhase);
    }
    if !f.alive {
        return Err(GameError::NotAlive);
    }
    if game.law_card == NO_LAW {
        return Err(GameError::LawNotRevealed);
    }
    if f.voted_stamp == game.stamp() {
        return Err(GameError::AlreadyVoted);
    }
    f.vote = choice;
    f.voted_stamp = game.stamp();
    Ok(())
}

pub fn veto(game: &mut Game, f: &mut Faction) -> Result<(), GameError> {
    if game.phase != Phase::Law {
        return Err(GameError::WrongPhase);
    }
    if game.law_card == NO_LAW {
        return Err(GameError::LawNotRevealed);
    }
    if game.president != f.wallet {
        return Err(GameError::NotPresident);
    }
    if game.veto_pending {
        return Err(GameError::AlreadyVetoed);
    }
    game.veto_pending = true;
    f.is_president = true;
    Ok(())
}

/// M10 (фикс 02.09): продажа голоса в два шага. Офер: продавец
/// объявляет цену и адресата, деньги НЕ списываются. Покупатель —
/// кошелёк и кэш (скаляры), чтобы звать без двойного borrow.
pub fn offer_vote(
    game: &Game,
    f_seller: &mut Faction,
    buyer_wallet: Pubkey,
    buyer_cash: u64,
    price: u64,
) -> Result<(), GameError> {
    if game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if price == 0 {
        return Err(GameError::SelfBribe);
    }
    if game.phase != Phase::Law {
        return Err(GameError::WrongPhase);
    }
    if f_seller.vote_sold || f_seller.vote_offer_to != Pubkey::default() {
        return Err(GameError::AlreadyVoted);
    }
    if buyer_cash < price {
        return Err(GameError::NotEnoughCash);
    }
    f_seller.vote_offer_to = buyer_wallet;
    f_seller.vote_offer_price = price;
    Ok(())
}

/// M10: акцепт покупки голоса (только адресат офера).
pub fn accept_vote_offer(
    game: &Game,
    f_buyer: &mut Faction,
    f_seller: &mut Faction,
) -> Result<u64, GameError> {
    if game.epoch != EPOCH_90S {
        return Err(GameError::WrongPhase);
    }
    if game.phase != Phase::Law {
        return Err(GameError::WrongPhase);
    }
    let price = f_seller.vote_offer_price;
    if f_seller.vote_offer_to == Pubkey::default() || f_seller.vote_offer_to != f_buyer.wallet {
        return Err(GameError::InvalidFactionSet);
    }
    if f_buyer.cash < price {
        return Err(GameError::NotEnoughCash);
    }
    f_buyer.cash -= price;
    f_seller.cash += price;
    f_seller.vote_sold = true;
    f_seller.vote_sold_to = f_buyer.wallet;
    f_seller.vote_offer_to = Pubkey::default();
    f_seller.vote_offer_price = 0;
    f_seller.voted_stamp = game.stamp();
    Ok(price)
}
