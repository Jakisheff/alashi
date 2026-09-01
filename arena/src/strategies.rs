//! Стратегии ботов для off-chain партий.
//! Контракт: решение только по публичной информации (как видит агент
//! ончейн-аккаунты). Никакого знания чужих будущих ходов.

use alashi_rules::constants::*;
use alashi_rules::state::VoteChoice;
use alashi_rules::anchor_lang::prelude::Pubkey;

/// Что стратегия видит в момент решения. Всё публично.
pub struct Obs<'a> {
    pub round: u8,
    pub my_idx: usize,
    pub n_factions: usize,
    /// (cash, goods, influence) по фракциям, мой индекс включён.
    pub cash: &'a [u64],
    pub goods: &'a [u16],
    pub influence: &'a [u16],
    pub alive: &'a [bool],
    /// Счётчик проданного в раунде (позиция в ценовой таблице).
    pub sold_counter: u16,
    pub active_tax_bps: u16,
    pub active_price_shift: i8,
    pub active_boom: u8,
    /// Карта закона в фазе Law (публична, как ончейн-аккаунт).
    pub law_card: u8,
    pub president: Pubkey,
    pub my_wallet: Pubkey,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MarketAction {
    Sell(u16),
    Buy(u16),
    Pass,
    /// SPEC_EPOCH_90S: продажа в кредит, вексель ×1.25.
    SellCredit(u16),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActionAction {
    Produce,
    Donkey,
    Bribe { to: usize, amount: u64 },
    Pass,
    /// SPEC_EPOCH_90S: серый канал «челнок» (+3 товара, риск таможни).
    Shuttle,
    /// SPEC_EPOCH_90S: крыша-контракт, тариф чёрный/красный.
    Roof { to: usize, tariff: u8 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum LawAction {
    Vote(VoteChoice),
    Veto,
    Pass,
}

pub trait Strategy: Send {
    fn name(&self) -> &'static str;
    fn market(&mut self, obs: &Obs) -> MarketAction;
    fn action(&mut self, obs: &Obs) -> ActionAction;
    fn law(&mut self, obs: &Obs) -> LawAction;
}

/// Эффективная цена единицы на позиции counter (та же арифметика,
/// что в compute_sale; дублируется только для наблюдения, не для расчёта).
pub fn eff_price(counter: u16, shift: i8, boom: u8) -> i64 {
    (price_at(counter) as i64 + shift as i64 + boom as i64).max(1)
}

/// Доход от продажи units на текущем счётчике (брутто, без налога).
pub fn sale_gross(counter: u16, units: u16, shift: i8, boom: u8) -> u64 {
    (1..=units)
        .map(|j| eff_price(counter + j - 1, shift, boom) as u64)
        .sum()
}

// ---------- Random ----------

pub struct RandomBot {
    pub rng: u64,
}

impl RandomBot {
    fn next(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }
}

impl Strategy for RandomBot {
    fn name(&self) -> &'static str {
        "random"
    }
    fn market(&mut self, obs: &Obs) -> MarketAction {
        let g = obs.goods[obs.my_idx];
        match self.next() % 4 {
            0 if g > 0 => MarketAction::Sell((self.next() % g as u64 + 1) as u16),
            1 => MarketAction::Buy((self.next() % 2 + 1) as u16),
            _ => MarketAction::Pass,
        }
    }
    fn action(&mut self, obs: &Obs) -> ActionAction {
        match self.next() % 6 {
            0 if obs.cash[obs.my_idx] >= DONKEY_PRICE * PESO => ActionAction::Donkey,
            1 if obs.cash[obs.my_idx] >= 2 * BRIBE_PRICE && obs.n_factions > 1 => {
                ActionAction::Bribe {
                    to: (self.next() as usize) % obs.n_factions,
                    amount: BRIBE_PRICE,
                }
            }
            _ => ActionAction::Produce,
        }
    }
    fn law(&mut self, obs: &Obs) -> LawAction {
        if obs.president == obs.my_wallet && self.next() % 8 == 0 {
            return LawAction::Veto;
        }
        LawAction::Vote(match self.next() % 3 {
            0 => VoteChoice::Yes,
            1 => VoteChoice::No,
            _ => VoteChoice::Abstain,
        })
    }
}

// ---------- Greedy (как ончейн-бот Aibot/Botagul fallback) ----------

pub struct GreedyBot;

impl Strategy for GreedyBot {
    fn name(&self) -> &'static str {
        "greedy"
    }
    fn market(&mut self, obs: &Obs) -> MarketAction {
        let g = obs.goods[obs.my_idx];
        if g > 0 {
            MarketAction::Sell(g)
        } else {
            MarketAction::Pass
        }
    }
    fn action(&mut self, _obs: &Obs) -> ActionAction {
        ActionAction::Produce
    }
    fn law(&mut self, _obs: &Obs) -> LawAction {
        LawAction::Vote(VoteChoice::Yes)
    }
}

// ---------- Tactical (эластичность + интерес + влияние) ----------

pub struct TacticalBot {
    /// Взяток за партию: одна проба — влияние в v0 не окупается (см. FINDINGS).
    pub bribes: u8,
}

impl TacticalBot {
    /// Мой ранг по cash (0 = богатейший).
    fn cash_rank(obs: &Obs) -> usize {
        let me = obs.cash[obs.my_idx];
        obs.cash
            .iter()
            .filter(|&&c| c > me)
            .count()
    }

    /// Интерес закона для меня: >0 за, <0 против, 0 воздержаться.
    fn law_interest(card: u8, rank: usize, n: usize) -> i32 {
        let bottom_half = rank * 2 >= n;
        match card {
            LAW_TAX_10 => -1,
            LAW_TAX_20 => -2,
            LAW_SUBSIDY_PRODUCE => 2,
            LAW_SUBSIDY_POOR if bottom_half => 2,
            LAW_SUBSIDY_POOR => -1,
            LAW_SUBSIDY_RICH if !bottom_half => 2,
            LAW_SUBSIDY_RICH => -1,
            LAW_EMBARGO => -2,
            LAW_BOOM => 2,
            LAW_STATUS_QUO => 0,
            _ => 0,
        }
    }
}

impl Strategy for TacticalBot {
    fn name(&self) -> &'static str {
        "tactical"
    }
    fn market(&mut self, obs: &Obs) -> MarketAction {
        let goods = obs.goods[obs.my_idx];
        let cash = obs.cash[obs.my_idx];
        let price_now = eff_price(obs.sold_counter, obs.active_price_shift, obs.active_boom) as u64;
        // Дёшево (кто-то слил много) — покупаю и держу до начала следующего
        // раунда, там counter=0 и цена высокая. Дорогo — сливаю всё.
        // В толпе ротация редко даёт раннюю позицию следующего раунда —
        // покупаем только на настоящем дне.
        let dip = match obs.n_factions {
            0..=3 => 6,
            4 => 4,
            _ => 3,
        };
        if cash >= 8 * PESO && price_now <= dip && obs.round < ROUNDS {
            let free_cash = cash - 4 * PESO;
            let units = ((free_cash / (price_now * PESO)).max(1) as u16).min(3);
            if cash >= units as u64 * price_now * PESO {
                return MarketAction::Buy(units);
            }
        }
        // Продать k, максимизируя брутто на падающей таблице (налог монотонен):
        // цены всех позиций >= 1, значит максимум всегда на «продать всё»,
        // но проверяем честно перебором — правило может измениться.
        let mut best_k = 0u16;
        let mut best_rev = 0u64;
        for k in 1..=goods {
            let rev = sale_gross(obs.sold_counter, k, obs.active_price_shift, obs.active_boom);
            if rev > best_rev {
                best_rev = rev;
                best_k = k;
            }
        }
        if best_k > 0 {
            MarketAction::Sell(best_k)
        } else {
            MarketAction::Pass
        }
    }
    fn action(&mut self, obs: &Obs) -> ActionAction {
        let cash = obs.cash[obs.my_idx];
        let am_president = obs.president == obs.my_wallet;
        let top_rival = (0..obs.n_factions)
            .filter(|&i| i != obs.my_idx)
            .map(|i| obs.influence[i])
            .max()
            .unwrap_or(0);
        // Влияние = вес голоса + президентство (вето). Покупаем власть, только
        // когда реально спорим (мы не впереди) и не дороже двух раз за партию.
        if !am_president
            && self.bribes < 1
            && obs.influence[obs.my_idx] <= top_rival
            && cash >= 6 * BRIBE_PRICE
        {
            self.bribes += 1;
            let target = (0..obs.n_factions)
                .filter(|&i| i != obs.my_idx && obs.alive[i])
                .min_by_key(|&i| obs.cash[i])
                .unwrap_or(obs.my_idx);
            if target != obs.my_idx {
                return ActionAction::Bribe {
                    to: target,
                    amount: BRIBE_PRICE,
                };
            }
        }
        // Осёл доминируется produce (2 бесплатных товара против 1 платного),
        // не используем. Produce всегда.
        ActionAction::Produce
    }
    fn law(&mut self, obs: &Obs) -> LawAction {
        let rank = Self::cash_rank(obs);
        let interest = Self::law_interest(obs.law_card, rank, obs.n_factions);
        let am_president = obs.president == obs.my_wallet;
        if am_president && interest < 0 {
            return LawAction::Veto;
        }
        LawAction::Vote(if interest > 0 {
            VoteChoice::Yes
        } else if interest < 0 {
            VoteChoice::No
        } else {
            VoteChoice::Abstain
        })
    }
}

/// Собрать стратегию по имени (для CLI-миксов).
pub fn by_name(name: &str, seed: u64) -> Option<Box<dyn Strategy>> {
    match name {
        "random" => Some(Box::new(RandomBot { rng: seed | 1 })),
        "greedy" => Some(Box::new(GreedyBot)),
        "tactical" => Some(Box::new(TacticalBot { bribes: 0 })),
        _ => None,
    }
}

pub const ALL: &[&str] = &["random", "greedy", "tactical"];
