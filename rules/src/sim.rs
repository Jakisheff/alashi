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

    pub fn sell(&mut self, idx: usize, units: u16) -> Result<u64, GameError> {
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
        g.sold_this_round = trade.counter_after;
        f.goods -= units;
        f.cash += revenue;
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
        if self.factions[from].acted_stamp == self.game.stamp() {
            return Err(GameError::AlreadyActed);
        }
        if self.game.phase != Phase::Action {
            return Err(GameError::WrongPhase);
        }
        let gain = amount / BRIBE_PRICE;
        if gain == 0 {
            return Err(GameError::BribeTooSmall);
        }
        if self.factions[from].cash < amount {
            return Err(GameError::NotEnoughCash);
        }
        let stamps = self.game.stamp();
        self.factions[from].cash -= amount;
        self.factions[to].cash += amount;
        self.factions[from].influence += gain as u16;
        self.factions[from].acted_stamp = stamps;
        Ok(self.factions[from].influence)
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
