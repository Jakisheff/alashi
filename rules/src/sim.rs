//! Off-chain симулятор: партии на чистых правилах без блокчейна.
//! Один запуск = сотни игр для L3 training camp. Энтропия подаётся
//! извне (seed_provider), чтобы матч был воспроизводимым.
//! Вся логика ходов — в actions.rs (общая с ончейн-инструкциями),
//! здесь только индексные обёртки над массивом фракций.

use crate::actions;
use crate::constants::*;
use crate::error::GameError;
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
        f.vote = VoteChoice::Abstain;
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
        self.round_seed = seed;
        let mut refs: Vec<&mut Faction> = self.factions.iter_mut().collect();
        transitions::advance(&mut self.game, &mut refs, now, 0, seed, None)
    }

    pub fn advance_with_card(
        &mut self,
        now: i64,
        card: u8,
    ) -> Result<transitions::AdvanceResult, GameError> {
        let mut refs: Vec<&mut Faction> = self.factions.iter_mut().collect();
        transitions::advance_with_card(&mut self.game, &mut refs, now, card)
    }

    pub fn reveal_for_replay(&mut self, card: u8) -> Result<u8, GameError> {
        transitions::reveal_law_card(&mut self.game, card)
    }

    // ---------- индексные обёртки над actions::* ----------

    /// Две фракции по индексам без двойного borrow.
    fn pair(
        factions: &mut [Faction],
        a: usize,
        b: usize,
    ) -> Result<(&mut Faction, &mut Faction), GameError> {
        if a >= factions.len() || b >= factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        if a == b {
            return Err(GameError::SelfBribe);
        }
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        let (left, right) = factions.split_at_mut(hi);
        if lo == a {
            Ok((&mut left[lo], &mut right[0]))
        } else {
            Ok((&mut right[0], &mut left[lo]))
        }
    }

    pub fn sell(&mut self, idx: usize, units: u16) -> Result<u64, GameError> {
        let (g, f) = (&mut self.game, &mut self.factions[idx]);
        actions::sell(g, f, units, false)
    }

    /// SPEC_EPOCH_90S M4: продажа в кредит (вексель).
    pub fn sell_credit(&mut self, idx: usize, units: u16) -> Result<u64, GameError> {
        let (g, f) = (&mut self.game, &mut self.factions[idx]);
        actions::sell(g, f, units, true)
    }

    pub fn buy(&mut self, idx: usize, units: u16) -> Result<u64, GameError> {
        let (g, f) = (&mut self.game, &mut self.factions[idx]);
        actions::buy(g, f, units)
    }

    pub fn produce(&mut self, idx: usize) -> Result<u16, GameError> {
        let (g, f) = (&mut self.game, &mut self.factions[idx]);
        actions::produce(g, f)
    }

    /// M3: серый канал «челнок».
    pub fn shuttle(&mut self, idx: usize) -> Result<u16, GameError> {
        let (g, f) = (&mut self.game, &mut self.factions[idx]);
        actions::shuttle(g, f)
    }

    pub fn donkey(&mut self, idx: usize) -> Result<(), GameError> {
        let (g, f) = (&mut self.game, &mut self.factions[idx]);
        actions::donkey(g, f)
    }

    pub fn bribe(&mut self, from: usize, to: usize, amount: u64) -> Result<u16, GameError> {
        // R1 (REVIEW_EXTERNAL): границы первыми, потом any-borrow
        let (fa, fb) = Self::pair(&mut self.factions, from, to)?;
        actions::bribe(&mut self.game, fa, fb, amount)
    }

    /// M2+M7: крыша-контракт (to — индекс фракции-крыши).
    pub fn roof(&mut self, from: usize, to: usize, tariff: u8) -> Result<Pubkey, GameError> {
        if from >= self.factions.len() || to >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        if from == to {
            return Err(GameError::SelfBribe);
        }
        let (fa, fb) = Self::pair(&mut self.factions, from, to)?;
        actions::roof(&mut self.game, fa, fb, tariff)
    }

    /// M8: президент вслепую выбирает режим границы.
    pub fn set_customs(&mut self, from: usize, tight: bool) -> Result<(), GameError> {
        let (g, f) = (&mut self.game, &self.factions[from]);
        actions::set_customs(g, f, tight)
    }

    /// M9: ставка на слепой аукцион лицензии.
    pub fn bid_license(&mut self, from: usize, amount: u64) -> Result<u64, GameError> {
        let (g, f) = (&mut self.game, &mut self.factions[from]);
        actions::bid_license(g, f, amount)
    }

    /// M9: инсайд о доходности лицензии.
    pub fn inspect_license(&mut self, from: usize) -> Result<u64, GameError> {
        let (g, f) = (&self.game, &mut self.factions[from]);
        actions::inspect_license(g, f)
    }

    /// M6: валютчик (обмен всего кэша ↔ твёрдой валюты ×0.8).
    pub fn exchange(&mut self, idx: usize, to_hard: bool) -> Result<u64, GameError> {
        if idx >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        let (g, f) = (&self.game, &mut self.factions[idx]);
        actions::exchange(g, f, to_hard)
    }

    /// M11: предложить бартер (to — индекс или None = любому).
    pub fn barter_propose(
        &mut self,
        from: usize,
        to: Option<usize>,
        goods: u16,
        price: u64,
    ) -> Result<u64, GameError> {
        if from >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        let to_wallet = match to {
            Some(t) => Some(
                self.factions
                    .get(t)
                    .ok_or(GameError::InvalidFactionSet)?
                    .wallet,
            ),
            None => None,
        };
        let (g, f) = (&mut self.game, &self.factions[from]);
        actions::barter_propose(g, f, to_wallet, goods, price)
    }

    /// M11: принять бартерный офер.
    pub fn barter_accept(&mut self, by: usize, offer_id: u64) -> Result<(), GameError> {
        let pos = self
            .game
            .barter_offers
            .iter()
            .position(|o| o.id == offer_id)
            .ok_or(GameError::InvalidFactionSet)?;
        let from_wallet = self.game.barter_offers[pos].from;
        let from = self
            .factions
            .iter()
            .position(|f| f.wallet == from_wallet)
            .ok_or(GameError::InvalidFactionSet)?;
        if from == by {
            return Err(GameError::SelfBribe);
        }
        let (f_by, f_from) = Self::pair(&mut self.factions, by, from)?;
        actions::barter_accept(&mut self.game, f_by, f_from, offer_id)
    }

    pub fn vote(&mut self, idx: usize, choice: VoteChoice) -> Result<(), GameError> {
        let (g, f) = (&mut self.game, &mut self.factions[idx]);
        actions::vote(g, f, choice)
    }

    pub fn veto(&mut self, idx: usize) -> Result<(), GameError> {
        let (g, f) = (&mut self.game, &mut self.factions[idx]);
        actions::veto(g, f)
    }

    /// M10: офер продажи голоса (продавец idx, покупатель buyer_idx).
    pub fn offer_vote(&mut self, seller: usize, buyer: usize, price: u64) -> Result<(), GameError> {
        if seller >= self.factions.len() || buyer >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        if seller == buyer {
            return Err(GameError::SelfBribe);
        }
        let (buyer_wallet, buyer_cash) = {
            let b = &self.factions[buyer];
            (b.wallet, b.cash)
        };
        let (g, f) = (&self.game, &mut self.factions[seller]);
        actions::offer_vote(g, f, buyer_wallet, buyer_cash, price)
    }

    /// M10: акцепт покупки голоса (покупатель buyer_idx).
    pub fn accept_vote_offer(&mut self, buyer: usize) -> Result<u64, GameError> {
        if buyer >= self.factions.len() {
            return Err(GameError::InvalidFactionSet);
        }
        // находим продавца: офер адресован покупателю
        let seller = self
            .factions
            .iter()
            .position(|f| f.vote_offer_to == self.factions[buyer].wallet && f.vote_offer_to != Pubkey::default())
            .ok_or(GameError::InvalidFactionSet)?;
        let (f_buyer, f_seller) = Self::pair(&mut self.factions, buyer, seller)?;
        actions::accept_vote_offer(&self.game, f_buyer, f_seller)
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
