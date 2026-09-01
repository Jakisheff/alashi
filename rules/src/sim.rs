//! Off-chain симулятор: партии на чистых правилах без блокчейна.
//! Один запуск = сотни игр для L3 training camp. Энтропия подаётся
//! извне (seed_provider), чтобы матч был воспроизводимым.

use crate::constants::*;
use crate::error::GameError;
use crate::logic::{compute_purchase, compute_sale};
use crate::state::{Faction, Game, Phase, VoteChoice};
use crate::transitions;
use anchor_lang::prelude::Pubkey;
use anchor_lang::AccountSerialize;

pub struct Simulator {
    pub game: Game,
    pub factions: Vec<Faction>,
    pub round_seed: u64,
    /// M11 бартерные оферы (вне Game/Faction: не ончейн-состояние,
    /// влияет только на cash/goods сторон при accept).
    pub barter_offers: Vec<BarterOffer>,
}

/// SPEC_EPOCH_90S M11: бартерный офер — товар за кэш напрямую между
/// фракциями, минуя рынок и валютчика.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BarterOffer {
    pub id: u64,
    pub from: usize,
    /// 255 = любому
    pub to: u8,
    pub goods: u16,
    pub price: u64,
}

impl Simulator {
    pub fn new(game_id: u64, entry_fee: u64, phase_duration: i64, entropy_mode: u8) -> Self {
        let mut game = Game::default();
        game.game_id = game_id;
        game.entry_fee = entry_fee;
        game.phase_duration = phase_duration;
        game.entropy_mode = entropy_mode;
        game.phase = Phase::Lobby;
        game.rake_bps = DEFAULT_RAKE_BPS;
        game.law_card = NO_LAW;
        Simulator {
            game,
            factions: vec![],
            round_seed: 0,
            barter_offers: vec![],
        }
    }

    pub fn join(&mut self, wallet: Pubkey, name: &str) -> Result<(), GameError> {
        if self.game.phase != Phase::Lobby {
            return Err(GameError::GameNotInLobby);
        }
        if self.factions.len() >= MAX_FACTIONS as usize {
            return Err(GameError::GameFull);
        }
        let mut f = Faction::default();
        f.game = Pubkey::default();
        f.wallet = wallet;
        f.name = name.to_string();
        f.influence = 1;
        f.vote = crate::state::VoteChoice::Abstain;
        f.alive = true;
        self.factions.push(f);
        self.game.faction_count = self.factions.len() as u8;
        Ok(())
    }

    pub fn advance(
        &mut self,
        now: i64,
        seed: u64,
    ) -> Result<transitions::AdvanceResult, GameError> {
        transitions::advance(&mut self.game, &mut self.factions, now, seed)
    }

    pub fn advance_with_card(
        &mut self,
        now: i64,
        card: u8,
    ) -> Result<transitions::AdvanceResult, GameError> {
        transitions::advance_with_card(&mut self.game, &mut self.factions, now, card)
    }

    pub fn reveal_for_replay(&mut self, card: u8) -> Result<u8, GameError> {
        transitions::reveal_law_card(&mut self.game, card)
    }

    pub fn sell(&mut self, idx: usize, units: u16) -> Result<u64, GameError> {
        self.sell_impl(idx, units, false)
    }

    /// SPEC_EPOCH_90S M4: продажа в кредит (вексель): выручка ×1.25,
    /// деньги приходят в начале следующего раунда; сгорают картой
    /// «взаимозачёт».
    pub fn sell_credit(&mut self, idx: usize, units: u16) -> Result<u64, GameError> {
        self.sell_impl(idx, units, true)
    }

    fn sell_impl(&mut self, idx: usize, units: u16, credit: bool) -> Result<u64, GameError> {
        if credit && self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        let g = &mut self.game;
        let f = &mut self.factions[idx];
        if g.phase != Phase::Market {
            return Err(GameError::WrongPhase);
        }
        if !f.alive {
            return Err(GameError::NotAlive);
        }
        if f.acted_stamp == g.stamp() {
            return Err(GameError::AlreadyActed);
        }
        if units == 0 || units > f.goods {
            return Err(GameError::NotEnoughGoods);
        }
        let trade = compute_sale(
            units,
            g.sold_this_round,
            g.active_price_shift,
            g.active_boom,
        );
        let tax = trade.gross * g.active_tax_bps as u64 / 10_000;
        let revenue = trade.gross - tax;
        let revenue = if credit { revenue * CREDIT_NUM / CREDIT_DEN } else { revenue };
        g.sold_this_round = trade.counter_after;
        f.goods -= units;
        if credit {
            f.promissory += revenue;
        } else {
            f.cash += revenue;
        }
        f.acted_stamp = g.stamp();
        Ok(revenue)
    }

    pub fn buy(&mut self, idx: usize, units: u16) -> Result<u64, GameError> {
        let g = &mut self.game;
        let f = &mut self.factions[idx];
        if g.phase != Phase::Market {
            return Err(GameError::WrongPhase);
        }
        if !f.alive {
            return Err(GameError::NotAlive);
        }
        if f.acted_stamp == g.stamp() {
            return Err(GameError::AlreadyActed);
        }
        let trade = compute_purchase(
            units,
            g.sold_this_round,
            g.active_price_shift,
            g.active_boom,
        );
        if f.cash < trade.gross {
            return Err(GameError::NotEnoughCash);
        }
        f.cash -= trade.gross;
        f.goods += units;
        g.sold_this_round = trade.counter_after;
        f.acted_stamp = g.stamp();
        Ok(trade.gross)
    }

    pub fn produce(&mut self, idx: usize) -> Result<u16, GameError> {
        let g = &mut self.game;
        let f = &mut self.factions[idx];
        if g.phase != Phase::Action {
            return Err(GameError::WrongPhase);
        }
        if f.acted_stamp == g.stamp() {
            return Err(GameError::AlreadyActed);
        }
        f.goods += PRODUCE_YIELD + g.active_subsidy_goods as u16;
        f.acted_stamp = g.stamp();
        Ok(f.goods)
    }

    /// SPEC_EPOCH_90S M3: серый канал «челнок»: +3 товара (вместо 2),
    /// товар помечается серым и стоит на таможне при закрытии фазы.
    pub fn shuttle(&mut self, idx: usize) -> Result<u16, GameError> {
        if self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        let g = &mut self.game;
        let f = &mut self.factions[idx];
        if g.phase != Phase::Action {
            return Err(GameError::WrongPhase);
        }
        if f.acted_stamp == g.stamp() {
            return Err(GameError::AlreadyActed);
        }
        f.goods += SHUTTLE_GOODS + g.active_subsidy_goods as u16;
        f.grey_goods += SHUTTLE_GOODS + g.active_subsidy_goods as u16;
        f.acted_stamp = g.stamp();
        Ok(f.goods)
    }

    /// SPEC_EPOCH_90S M2+M7: крыша-контракт с тарифом:
    /// чёрный (30% кэша, гарантия от границы и гашение закона) или
    /// красный (10%, гашение закона, p≈25% беспредела на границе).
    pub fn roof(&mut self, from: usize, to: usize, tariff: u8) -> Result<u8, GameError> {
        if self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        if from >= self.factions.len() || to >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        if from == to {
            return Err(GameError::SelfBribe);
        }
        if tariff != ROOF_BLACK && tariff != ROOF_RED {
            return Err(GameError::InvalidVoteWeightMode);
        }
        let g = &mut self.game;
        if g.phase != Phase::Action {
            return Err(GameError::WrongPhase);
        }
        if self.factions[from].roof_armed {
            return Err(GameError::AlreadyActed);
        }
        let (num, den) = if tariff == ROOF_BLACK {
            (ROOF_BLACK_NUM, ROOF_BLACK_DEN)
        } else {
            (ROOF_RED_NUM, ROOF_RED_DEN)
        };
        let price = self.factions[from].cash * num / den;
        if price == 0 {
            return Err(GameError::NotEnoughCash);
        }
        if self.factions[from].cash < price {
            return Err(GameError::NotEnoughCash);
        }
        self.factions[from].cash -= price;
        self.factions[to].cash += price;
        self.factions[from].roof_to = to as u8;
        self.factions[from].roof_armed = true;
        self.factions[from].roof_tariff = tariff;
        self.factions[from].acted_stamp = g.stamp();
        Ok(to as u8)
    }

    /// SPEC_EPOCH_90S M8: президент вслепую выбирает режим границы
    /// этого раунда (tight = досмотр). Не сжигает ход, раз в раунд.
    pub fn set_customs(&mut self, from: usize, tight: bool) -> Result<(), GameError> {
        if self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        if self.game.phase != Phase::Action {
            return Err(GameError::WrongPhase);
        }
        if self.game.customs_decided {
            return Err(GameError::AlreadyActed);
        }
        if self.factions[from].wallet != self.game.president {
            return Err(GameError::NotPresident);
        }
        self.game.customs_tight = tight;
        self.game.customs_decided = true;
        Ok(())
    }

    /// SPEC_EPOCH_90S M9: ставка на слепой аукцион лицензии (эскроу
    /// кэша, платит только победитель). Только в раунд аукциона.
    pub fn bid_license(&mut self, from: usize, amount: u64) -> Result<u64, GameError> {
        if self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        if self.game.phase != Phase::Action || self.game.round != AUCTION_ROUND {
            return Err(GameError::WrongPhase);
        }
        if self.factions[from].cash < amount || amount == 0 {
            return Err(GameError::NotEnoughCash);
        }
        self.factions[from].cash -= amount;
        self.factions[from].bid += amount;
        Ok(self.factions[from].bid)
    }

    /// SPEC_EPOCH_90S M9: инсайд о доходности лицензии (5M, до конца
    /// партии). Ответ — точный доход, известен только купившим.
    pub fn inspect_license(&mut self, from: usize) -> Result<u64, GameError> {
        if self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        if self.factions[from].insider {
            return Err(GameError::AlreadyActed);
        }
        if self.factions[from].cash < LICENSE_INSIGHT_PRICE {
            return Err(GameError::NotEnoughCash);
        }
        self.factions[from].cash -= LICENSE_INSIGHT_PRICE;
        self.factions[from].insider = true;
        // доход фиксирован сидом раунда аукциона, известен заранее
        Ok(self.game.license_yield)
    }

    /// SPEC_EPOCH_90S M10: продажа своего голоса покупателю за кэш
    /// (цена списывается с покупателя сразу, голос в этом законе идёт
    /// по выбору покупателя).
    pub fn sell_vote(&mut self, seller: usize, buyer: usize, price: u64) -> Result<(), GameError> {
        if self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        if seller >= self.factions.len() || buyer >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        if seller == buyer || price == 0 {
            return Err(GameError::SelfBribe);
        }
        if self.game.phase != Phase::Law {
            return Err(GameError::WrongPhase);
        }
        if self.factions[seller].vote_sold {
            return Err(GameError::AlreadyVoted);
        }
        if self.factions[buyer].cash < price {
            return Err(GameError::NotEnoughCash);
        }
        self.factions[buyer].cash -= price;
        self.factions[seller].cash += price;
        self.factions[seller].vote_sold = true;
        self.factions[seller].vote_sold_to = buyer as u8;
        // голос продавца автоматически считается поданным
        self.factions[seller].voted_stamp = self.game.stamp();
        Ok(())
    }

    pub fn donkey(&mut self, idx: usize) -> Result<(), GameError> {
        let g = &mut self.game;
        let f = &mut self.factions[idx];
        if g.phase != Phase::Action {
            return Err(GameError::WrongPhase);
        }
        if f.acted_stamp == g.stamp() {
            return Err(GameError::AlreadyActed);
        }
        let price = DONKEY_PRICE * PESO;
        if f.cash < price {
            return Err(GameError::NotEnoughCash);
        }
        f.cash -= price;
        f.goods += 1;
        f.acted_stamp = g.stamp();
        Ok(())
    }

    pub fn bribe(&mut self, from: usize, to: usize, amount: u64) -> Result<u16, GameError> {
        // R1 (REVIEW_EXTERNAL): индексы валидируются до любого обращения,
        // иначе один POST /act с to=999 кладёт весь arenad (отравленный лок).
        if from >= self.factions.len() || to >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        if self.game.phase != Phase::Action {
            return Err(GameError::WrongPhase);
        }
        if !self.factions[from].alive || !self.factions[to].alive {
            return Err(GameError::NotAlive);
        }
        if self.factions[from].acted_stamp == self.game.stamp() {
            return Err(GameError::AlreadyActed);
        }
        if from == to {
            return Err(GameError::SelfBribe);
        }
        let gain = amount / BRIBE_PRICE;
        if gain == 0 {
            return Err(GameError::BribeTooSmall);
        }
        if self.factions[from].cash < amount {
            return Err(GameError::NotEnoughCash);
        }
        if self.factions[from].influence as u64 + gain > MAX_INFLUENCE as u64 {
            return Err(GameError::BribeTooBig);
        }
        let stamps = self.game.stamp();
        self.factions[from].cash -= amount;
        self.factions[to].cash += amount;
        self.factions[from].influence += gain as u16;
        self.factions[from].acted_stamp = stamps;
        Ok(self.factions[from].influence)
    }

    /// SPEC_EPOCH_90S M6: валютчик. Обмен кэш ↔ твёрдая валюта ×0.8.
    /// Сервисная операция: доступна в любой игровой фазе и не сжигает
    /// ход (acted_stamp не трогаем). Твёрдая валюта не девальвирует,
    /// участвует в ранге, крыша с неё не берёт.
    pub fn exchange(&mut self, idx: usize, to_hard: bool) -> Result<u64, GameError> {
        if self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        if idx >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        let phase = self.game.phase;
        if !matches!(phase, Phase::Market | Phase::Action | Phase::Law) {
            return Err(GameError::WrongPhase);
        }
        if to_hard {
            let pesetas = self.factions[idx].cash;
            let got = pesetas * EXCHANGE_NUM / EXCHANGE_DEN;
            if got == 0 {
                return Err(GameError::NotEnoughCash);
            }
            self.factions[idx].cash -= pesetas;
            self.factions[idx].hard += got;
            Ok(got)
        } else {
            let hard = self.factions[idx].hard;
            let got = hard * EXCHANGE_NUM / EXCHANGE_DEN;
            if got == 0 {
                return Err(GameError::NotEnoughCash);
            }
            self.factions[idx].hard -= hard;
            self.factions[idx].cash += got;
            Ok(got)
        }
    }

    /// SPEC_EPOCH_90S M11: предложить бартер (товар за кэш напрямую).
    pub fn barter_propose(
        &mut self,
        from: usize,
        to: Option<usize>,
        goods: u16,
        price: u64,
    ) -> Result<u64, GameError> {
        if self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        if self.game.phase != Phase::Market {
            return Err(GameError::WrongPhase);
        }
        if from >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        if goods == 0 || goods > self.factions[from].goods {
            return Err(GameError::NotEnoughGoods);
        }
        let id = self.barter_offers.len() as u64 + 1;
        self.barter_offers.push(BarterOffer {
            id,
            from,
            to: to.map(|t| t as u8).unwrap_or(ROOF_NONE),
            goods,
            price,
        });
        Ok(id)
    }

    /// M11: принять бартерный офер (оплата кэшем, товар переходит).
    pub fn barter_accept(&mut self, by: usize, offer_id: u64) -> Result<(), GameError> {
        if self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        if self.game.phase != Phase::Market {
            return Err(GameError::WrongPhase);
        }
        let pos = self
            .barter_offers
            .iter()
            .position(|o| o.id == offer_id)
            .ok_or(GameError::InvalidFactionSet)?;
        let offer = self.barter_offers[pos];
        if offer.from == by {
            return Err(GameError::SelfBribe);
        }
        if offer.to != ROOF_NONE && offer.to as usize != by {
            return Err(GameError::InvalidFactionSet);
        }
        if self.factions[offer.from].goods < offer.goods {
            return Err(GameError::NotEnoughGoods);
        }
        if self.factions[by].cash < offer.price {
            return Err(GameError::NotEnoughCash);
        }
        self.factions[by].cash -= offer.price;
        self.factions[offer.from].cash += offer.price;
        self.factions[offer.from].goods -= offer.goods;
        self.factions[by].goods += offer.goods;
        self.barter_offers.remove(pos);
        Ok(())
    }

    pub fn vote(&mut self, idx: usize, choice: VoteChoice) -> Result<(), GameError> {
        let g = &mut self.game;
        let f = &mut self.factions[idx];
        if g.phase != Phase::Law {
            return Err(GameError::WrongPhase);
        }
        if g.law_card == NO_LAW {
            return Err(GameError::LawNotRevealed);
        }
        if f.voted_stamp == g.stamp() {
            return Err(GameError::AlreadyVoted);
        }
        f.vote = choice;
        f.voted_stamp = g.stamp();
        Ok(())
    }

    pub fn veto(&mut self, idx: usize) -> Result<(), GameError> {
        let g = &mut self.game;
        let f = &mut self.factions[idx];
        if g.phase != Phase::Law {
            return Err(GameError::WrongPhase);
        }
        if g.law_card == NO_LAW {
            return Err(GameError::LawNotRevealed);
        }
        if g.president != f.wallet {
            return Err(GameError::NotPresident);
        }
        if g.veto_pending {
            return Err(GameError::AlreadyVetoed);
        }
        g.veto_pending = true;
        f.is_president = true;
        Ok(())
    }

    pub fn state_bytes(&self) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut g = Vec::new();
        self.game.try_serialize(&mut g).ok();
        let fs = self
            .factions
            .iter()
            .map(|f| {
                let mut out = Vec::new();
                f.try_serialize(&mut out).ok();
                out
            })
            .collect();
        (g, fs)
    }
}
