use crate::{constants::*, error::GameError, state::Faction, state::VoteChoice};
use anchor_lang::prelude::*;

pub struct FactionSnapshot {
    pub wallet: Pubkey,
    pub cash: u64,
    pub influence: u16,
    pub alive: bool,
}

impl From<&Faction> for FactionSnapshot {
    fn from(f: &Faction) -> Self {
        FactionSnapshot {
            wallet: f.wallet,
            cash: f.cash,
            influence: f.influence,
            alive: f.alive,
        }
    }
}

impl<'info> From<&anchor_lang::prelude::Account<'info, Faction>> for FactionSnapshot {
    fn from(f: &anchor_lang::prelude::Account<'info, Faction>) -> Self {
        FactionSnapshot::from(&**f)
    }
}

pub struct PayoutPlan {
    pub faction_index: usize,
    pub wallet: Pubkey,
    pub rank: u8,
    pub amount: u64,
}

pub struct SettlementPlan {
    pub payouts: Vec<PayoutPlan>,
    pub rake: u64,
    pub pot: u64,
}

pub fn compute_settlement(
    factions: &[FactionSnapshot],
    bank: u64,
    reserve: u64,
    rake_bps: u16,
    shares: &[u64],
) -> Result<SettlementPlan> {
    if bank <= reserve {
        return Err(GameError::EmptyBank.into());
    }
    let pot = bank - reserve;
    let rake = pot * rake_bps as u64 / 10_000;
    let payout_total = pot - rake;

    let k = factions.len();
    let share_count = k.min(shares.len());
    let total_shares: u64 = shares[..share_count].iter().sum();
    if total_shares == 0 {
        return Err(GameError::EmptyBank.into());
    }

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

    let others_total: u64 = (1..share_count)
        .map(|r| payout_total * shares[r] / total_shares)
        .sum();
    let rank0_amount = payout_total.saturating_sub(others_total);

    let mut payouts = Vec::new();
    for (rank, &fi) in order.iter().enumerate() {
        if rank >= share_count {
            break;
        }
        let amount = if rank == 0 {
            rank0_amount
        } else {
            payout_total * shares[rank] / total_shares
        };
        if amount == 0 {
            continue;
        }
        payouts.push(PayoutPlan {
            faction_index: fi,
            wallet: factions[fi].wallet,
            rank: rank as u8,
            amount,
        });
    }

    Ok(SettlementPlan { payouts, rake, pot })
}

pub struct LawEffect {
    pub tax_bps: Option<u16>,
    pub subsidy_goods: u8,
    pub pending_price_shift: i8,
    pub pending_boom: u8,
    pub influence_gain: Option<usize>,
}

pub fn compute_law_effect(card: u8, factions: &[FactionSnapshot]) -> LawEffect {
    let mut effect = LawEffect {
        tax_bps: None,
        subsidy_goods: 0,
        pending_price_shift: 0,
        pending_boom: 0,
        influence_gain: None,
    };
    match card {
        LAW_TAX_10 => effect.tax_bps = Some(1_000),
        LAW_TAX_20 => effect.tax_bps = Some(2_000),
        LAW_SUBSIDY_PRODUCE => effect.subsidy_goods = 1,
        LAW_SUBSIDY_POOR => effect.influence_gain = Some(extreme_cash_index(factions, true)),
        LAW_SUBSIDY_RICH => effect.influence_gain = Some(extreme_cash_index(factions, false)),
        LAW_EMBARGO => effect.pending_price_shift = -2,
        LAW_BOOM => effect.pending_boom = 2,
        _ => {}
    }
    effect
}

fn extreme_cash_index(factions: &[FactionSnapshot], poorest: bool) -> usize {
    let mut best = 0usize;
    for i in 1..factions.len() {
        if !factions[i].alive {
            continue;
        }
        let better = if poorest {
            factions[i].cash < factions[best].cash
                || (factions[i].cash == factions[best].cash
                    && factions[i].wallet < factions[best].wallet)
        } else {
            factions[i].cash > factions[best].cash
                || (factions[i].cash == factions[best].cash
                    && factions[i].wallet < factions[best].wallet)
        };
        if better {
            best = i;
        }
    }
    best
}

pub fn tally_votes(votes: &[(u16, VoteChoice)]) -> (u32, u32) {
    let mut yes = 0u32;
    let mut no = 0u32;
    for (influence, choice) in votes {
        match choice {
            VoteChoice::Yes => yes += *influence as u32,
            VoteChoice::No => no += *influence as u32,
            VoteChoice::Abstain => {}
        }
    }
    (yes, no)
}

pub fn elect_president(factions: &[FactionSnapshot]) -> Option<Pubkey> {
    factions
        .iter()
        .filter(|f| f.alive)
        .max_by(|a, b| {
            if a.influence != b.influence {
                a.influence.cmp(&b.influence)
            } else {
                b.wallet.cmp(&a.wallet)
            }
        })
        .map(|f| f.wallet)
}

pub fn draw_law_index(seed: u64, used_mask: u8) -> (u8, u8) {
    let mut mask = used_mask;
    if mask == 0xFF {
        mask = 0;
    }
    let mut idx = seed % DECK_SIZE as u64;
    while mask & (1 << idx) != 0 {
        idx = (idx + 1) % DECK_SIZE as u64;
    }
    (idx as u8, mask | (1 << idx))
}

pub fn market_price(sold: u16, price_shift: i8, boom: u8) -> u64 {
    let base = crate::constants::price_at(sold) as i64 + price_shift as i64 + boom as i64;
    (base.max(1) as u64) * PESO
}

pub struct TradeResult {
    pub units: u16,
    pub gross: u64,
    pub counter_after: u16,
}

pub fn compute_sale(units: u16, sold: u16, price_shift: i8, boom: u8) -> TradeResult {
    let mut gross = 0u64;
    let mut s = sold;
    for _ in 0..units {
        gross += market_price(s, price_shift, boom);
        s += 1;
    }
    TradeResult {
        units,
        gross,
        counter_after: s,
    }
}

pub fn compute_purchase(units: u16, sold: u16, price_shift: i8, boom: u8) -> TradeResult {
    let mut gross = 0u64;
    let mut s = sold;
    for _ in 0..units {
        gross += market_price(s, price_shift, boom);
        s = s.saturating_sub(1);
    }
    TradeResult {
        units,
        gross,
        counter_after: s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(i: u8, cash: u64, influence: u16) -> FactionSnapshot {
        FactionSnapshot {
            wallet: Pubkey::new_from_array([i; 32]),
            cash,
            influence,
            alive: true,
        }
    }

    #[test]
    fn settlement_two_factions_exact_split() {
        let f = [snap(1, 100, 1), snap(2, 50, 1)];
        let plan = compute_settlement(&f, 200_000_000, 1_400_000, 500, &PAYOUT_SHARES).unwrap();
        assert_eq!(plan.pot, 198_600_000);
        assert_eq!(plan.rake, 9_930_000);
        let payout_total = 188_670_000;
        let rank1 = payout_total * 30 / 80;
        let rank0 = payout_total - rank1;
        assert_eq!(plan.payouts[0].amount, rank0);
        assert_eq!(plan.payouts[0].faction_index, 0);
        assert_eq!(plan.payouts[1].amount, rank1);
        assert_eq!(plan.payouts[1].rank, 1);
        assert_eq!(
            plan.payouts.iter().map(|p| p.amount).sum::<u64>(),
            payout_total
        );
    }

    #[test]
    fn settlement_tie_break_by_wallet() {
        let a = snap(5, 100, 1);
        let b = snap(3, 100, 1);
        let plan =
            compute_settlement(&[a, b], 200_000_000, 1_400_000, 500, &PAYOUT_SHARES).unwrap();
        assert_eq!(plan.payouts[0].faction_index, 1);
        assert_eq!(plan.payouts[1].faction_index, 0);
    }

    #[test]
    fn settlement_empty_bank() {
        let f = [snap(1, 0, 1), snap(2, 0, 1)];
        assert!(compute_settlement(&f, 1_400_000, 1_400_000, 500, &PAYOUT_SHARES).is_err());
    }

    #[test]
    fn settlement_four_factions_full_shares() {
        let f = [
            snap(1, 40, 1),
            snap(2, 30, 1),
            snap(3, 20, 1),
            snap(4, 10, 1),
        ];
        let plan = compute_settlement(&f, 100_000_000, 0, 500, &PAYOUT_SHARES).unwrap();
        assert_eq!(plan.rake, 5_000_000);
        assert_eq!(plan.payouts[0].amount, 47_500_000);
        assert_eq!(plan.payouts[1].amount, 28_500_000);
        assert_eq!(plan.payouts[2].amount, 14_250_000);
        assert_eq!(plan.payouts[3].amount, 4_750_000);
    }

    #[test]
    fn law_effects_map_cards() {
        let f = [snap(1, 10, 1), snap(2, 90, 1)];
        assert_eq!(compute_law_effect(LAW_TAX_10, &f).tax_bps, Some(1_000));
        assert_eq!(compute_law_effect(LAW_TAX_20, &f).tax_bps, Some(2_000));
        assert_eq!(compute_law_effect(LAW_SUBSIDY_PRODUCE, &f).subsidy_goods, 1);
        assert_eq!(
            compute_law_effect(LAW_SUBSIDY_POOR, &f).influence_gain,
            Some(0)
        );
        assert_eq!(
            compute_law_effect(LAW_SUBSIDY_RICH, &f).influence_gain,
            Some(1)
        );
        assert_eq!(compute_law_effect(LAW_EMBARGO, &f).pending_price_shift, -2);
        assert_eq!(compute_law_effect(LAW_BOOM, &f).pending_boom, 2);
        assert!(compute_law_effect(LAW_STATUS_QUO, &f).tax_bps.is_none());
    }

    #[test]
    fn tally_and_president() {
        let votes = vec![
            (2, VoteChoice::Yes),
            (1, VoteChoice::No),
            (5, VoteChoice::Abstain),
        ];
        assert_eq!(tally_votes(&votes), (2, 1));
        let fs = [snap(9, 0, 3), snap(2, 0, 3)];
        assert_eq!(elect_president(&fs), Some(Pubkey::new_from_array([2; 32])));
    }

    #[test]
    fn draw_law_skips_used_and_wraps() {
        let (idx, mask) = draw_law_index(0, 0b0000_0011);
        assert_eq!(idx, 2);
        assert_eq!(mask, 0b0000_0111);
        let (idx2, _) = draw_law_index(0, 0b1111_1111);
        assert_eq!(idx2, 0);
    }

    #[test]
    fn sale_drops_price_and_purchase_raises_it() {
        let sale = compute_sale(3, 0, 0, 0);
        assert_eq!(sale.gross / PESO, 12 + 10 + 9);
        assert_eq!(sale.counter_after, 3);
        let purchase = compute_purchase(2, 3, 0, 0);
        assert_eq!(purchase.gross / PESO, 8 + 9);
        assert_eq!(purchase.counter_after, 1);
        let clamp = compute_purchase(5, 1, 0, 0);
        assert_eq!(clamp.counter_after, 0);
        assert!(clamp.gross > 0);
    }

    #[test]
    fn embargo_and_boom_move_market_price() {
        assert_eq!(market_price(0, -2, 0) / PESO, 10);
        assert_eq!(market_price(0, 0, 2) / PESO, 14);
        assert_eq!(market_price(9, -2, 0) / PESO, 1);
    }
}
