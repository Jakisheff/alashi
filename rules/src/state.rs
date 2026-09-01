use anchor_lang::prelude::*;

anchor_lang::declare_id!("8EikcWzM7d3EjttApmymo2maWp5A3NtMpKoYdWUzzzL");

#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq, InitSpace,
)]
pub enum Phase {
    #[default]
    Lobby,
    Market,
    Action,
    Law,
    Finished,
    Aborted,
}

#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq, InitSpace,
)]
pub enum VoteChoice {
    #[default]
    Yes,
    No,
    Abstain,
}

#[account]
#[derive(InitSpace, Default)]
pub struct Game {
    pub admin: Pubkey,
    pub game_id: u64,
    pub phase: Phase,
    pub round: u8,
    pub phase_ends_at: i64,
    pub faction_count: u8,
    pub laws_passed: u16,
    pub sold_this_round: u16,
    pub entry_fee: u64,
    pub rake_bps: u16,
    pub phase_duration: i64,
    pub last_law_passed: bool,
    pub yes_influence: u32,
    pub no_influence: u32,
    pub bump: u8,
    pub law_card: u8,
    pub laws_used_mask: u8,
    pub veto_pending: bool,
    pub president: Pubkey,
    pub active_tax_bps: u16,
    pub active_subsidy_goods: u8,
    pub active_price_shift: i8,
    pub active_boom: u8,
    pub pending_tax_bps: u16,
    pub pending_subsidy_goods: u8,
    pub pending_price_shift: i8,
    pub pending_boom: u8,
    pub settled: bool,
    pub entropy_mode: u8,
    pub vote_weight_mode: u8,
    /// SPEC_EPOCH_90S: 0 = classic, 1 = эпоха 90-х (девальвация,
    /// крыша, челнок, вексель, завод).
    pub epoch: u8,
    /// карта «взаимозачёт» (id 8) уже выпадала в этой партии (epoch=1).
    pub amnesty_used: bool,
    /// M8 режим границы этого раунда, выбранный президентом вслепую.
    pub customs_tight: bool,
    /// M8 президент уже выбрал режим границы в этом раунде.
    pub customs_decided: bool,
    /// M9 банк партии вырос от ставок аукциона (входит в делёж).
    pub prize_pot: u64,
    /// M9 доходность лицензии раунда аукциона.
    pub license_yield: u64,
    /// M9 держатель лицензии (255 = никто).
    pub license_holder: u8,
    /// M9 аукцион уже проведён.
    pub license_sold: bool,
    pub vrf_account: Pubkey,
    pub commit_slot: u64,
    pub vrf_retries: u8,
    pub vrf_spent: u64,
}

impl Game {
    pub fn stamp(&self) -> u16 {
        ((self.round as u16) << 3) | self.phase as u16
    }
}

#[account]
#[derive(InitSpace, Default)]
pub struct Faction {
    pub game: Pubkey,
    pub wallet: Pubkey,
    #[max_len(16)]
    pub name: String,
    pub cash: u64,
    pub goods: u16,
    pub influence: u16,
    pub acted_stamp: u16,
    pub voted_stamp: u16,
    pub vote: VoteChoice,
    pub is_president: bool,
    pub alive: bool,
    pub bump: u8,
    /// SPEC_EPOCH_90S (epoch=1): товар, добытый серым ходом этого раунда
    /// (конфискуется таможней при закрытии Action).
    pub grey_goods: u16,
    /// непогашенный вексель (продажа в кредит), гасится в начале раунда.
    pub promissory: u64,
    /// SPEC_EPOCH_90S M6: твёрдая валюта («доллары»), не девальвирует,
    /// обмен через валютчика со спредом 20%.
    pub hard: u64,
    /// крыша-контракт: индекс фракции-крыши (валиден при roof_armed).
    pub roof_to: u8,
    /// контракт активен (одноразовый).
    pub roof_armed: bool,
    /// M7 тариф крыши: 0 нет, 1 чёрная (30%, гарантия), 2 красная (10%,
    /// риск беспредела). Живёт до конца партии.
    pub roof_tariff: u8,
    /// M10 голос продан фракции с этим индексом (валиден при vote_sold).
    pub vote_sold_to: u8,
    pub vote_sold: bool,
    /// M9 ставка на лицензию этого раунда (эскроу).
    pub bid: u64,
    /// M9 куплен инсайд о доходности лицензии.
    pub insider: bool,
}
