pub mod constants {
    pub use alashi_rules::constants::*;
}
pub mod error {
    pub use alashi_rules::error::*;
}
pub mod events;
pub mod instructions;
pub mod logic {
    pub use alashi_rules::logic::*;
}
pub mod state {
    pub use alashi_rules::state::*;
}

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("8EikcWzM7d3EjttApmymo2maWp5A3NtMpKoYdWUzzzL");

#[program]
pub mod alashi {
    use super::*;

    pub fn initialize(
        ctx: Context<Initialize>,
        game_id: u64,
        entry_fee: u64,
        phase_duration: i64,
        entropy_mode: u8,
        epoch: u8,
    ) -> Result<()> {
        crate::instructions::initialize::handle_initialize(
            ctx,
            game_id,
            entry_fee,
            phase_duration,
            entropy_mode,
            epoch,
        )
    }

    pub fn join(ctx: Context<Join>, name: String) -> Result<()> {
        crate::instructions::join::handle_join(ctx, name)
    }

    pub fn produce(ctx: Context<Produce>) -> Result<()> {
        crate::instructions::produce::handle_produce(ctx)
    }

    pub fn sell(ctx: Context<Sell>, units: u16) -> Result<()> {
        crate::instructions::sell::handle_sell(ctx, units)
    }

    pub fn buy(ctx: Context<BuyGoods>, units: u16) -> Result<()> {
        crate::instructions::buy::handle_buy(ctx, units)
    }

    pub fn bribe(ctx: Context<Bribe>, amount: u64) -> Result<()> {
        crate::instructions::bribe::handle_bribe(ctx, amount)
    }

    pub fn vote(ctx: Context<Vote>, choice: VoteChoice) -> Result<()> {
        crate::instructions::vote::handle_vote(ctx, choice)
    }

    pub fn veto(ctx: Context<Veto>) -> Result<()> {
        crate::instructions::veto::handle_veto(ctx)
    }

    pub fn buy_donkey(ctx: Context<BuyDonkey>) -> Result<()> {
        crate::instructions::donkey::handle_buy_donkey(ctx)
    }

    pub fn settle<'a>(ctx: Context<'a, Settle<'a>>) -> Result<()> {
        crate::instructions::settle::handle_settle(ctx)
    }

    pub fn reveal_law(ctx: Context<RevealLaw>) -> Result<()> {
        crate::instructions::reveal_law::handle_reveal_law(ctx)
    }

    pub fn settle_refund<'a>(ctx: Context<'a, SettleRefund<'a>>) -> Result<()> {
        crate::instructions::settle_refund::handle_settle_refund(ctx)
    }

    pub fn advance(ctx: Context<Advance>) -> Result<()> {
        crate::instructions::advance::handle_advance(ctx)
    }

    pub fn set_vote_mode(ctx: Context<SetVoteMode>, mode: u8) -> Result<()> {
        crate::instructions::set_vote_mode::handle_set_vote_mode(ctx, mode)
    }

    // ---------- SPEC_EPOCH_90S (epoch=1) ----------

    pub fn sell_credit(ctx: Context<SellCredit>, units: u16) -> Result<()> {
        crate::instructions::epoch90s::handle_sell_credit(ctx, units)
    }

    pub fn shuttle(ctx: Context<Shuttle>) -> Result<()> {
        crate::instructions::epoch90s::handle_shuttle(ctx)
    }

    pub fn roof(ctx: Context<Roof>, tariff: u8) -> Result<()> {
        crate::instructions::epoch90s::handle_roof(ctx, tariff)
    }

    pub fn set_customs(ctx: Context<SetCustoms>, tight: bool) -> Result<()> {
        crate::instructions::epoch90s::handle_set_customs(ctx, tight)
    }

    pub fn bid_license(ctx: Context<BidLicense>, amount: u64) -> Result<()> {
        crate::instructions::epoch90s::handle_bid_license(ctx, amount)
    }

    pub fn inspect_license(ctx: Context<InspectLicense>) -> Result<()> {
        crate::instructions::epoch90s::handle_inspect_license(ctx)
    }

    pub fn exchange(ctx: Context<Exchange>, to_hard: bool) -> Result<()> {
        crate::instructions::epoch90s::handle_exchange(ctx, to_hard)
    }

    pub fn offer_vote(ctx: Context<OfferVote>, price: u64) -> Result<()> {
        crate::instructions::epoch90s::handle_offer_vote(ctx, price)
    }

    pub fn accept_vote_offer(ctx: Context<AcceptVoteOffer>) -> Result<()> {
        crate::instructions::epoch90s::handle_accept_vote_offer(ctx)
    }

    pub fn barter_propose(ctx: Context<BarterPropose>, goods: u16, price: u64) -> Result<()> {
        crate::instructions::epoch90s::handle_barter_propose(ctx, goods, price)
    }

    pub fn barter_accept(ctx: Context<BarterAccept>, offer_id: u64) -> Result<()> {
        crate::instructions::epoch90s::handle_barter_accept(ctx, offer_id)
    }
}
