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

    /// SPEC_EPOCH_90S M2: крыша-контракт: 20% кэша целику, первый
    /// анти-богатый закон против хозяина гасится.
    pub fn roof(&mut self, from: usize, to: usize) -> Result<u8, GameError> {
        if self.game.epoch != EPOCH_90S {
            return Err(GameError::WrongPhase);
        }
        if from >= self.factions.len() || to >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        if from == to {
            return Err(GameError::SelfBribe);
        }
        let g = &mut self.game;
        if g.phase != Phase::Action {
            return Err(GameError::WrongPhase);
        }
        if self.factions[from].roof_armed {
            return Err(GameError::AlreadyActed);
        }
        let price = self.factions[from].cash * ROOF_NUM / ROOF_DEN;
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
        self.factions[from].acted_stamp = g.stamp();
        Ok(to as u8)
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
