use anchor_lang::prelude::*;

anchor_lang::declare_id!("3jwunaFDRrSWFfeJ5hFZu3DmxPNTmkdoCweHDvqcXTqC");

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
    /// M9 держатель лицензии (Pubkey::default() = никто). Кошелек, не
    /// индекс: ончейн-аккаунты адресуются ключами, порядок массива
    /// фракций у кранкера не каноничен (анти-гриф).
    pub license_holder: Pubkey,
    /// M9 аукцион уже проведён.
    pub license_sold: bool,
    /// M11 бартерные оферы (в Game: ончейн-хранилище и симулятор
    /// сериализуются одинаково байт-в-байт).
    #[max_len(crate::constants::MAX_BARTER_OFFERS)]
    pub barter_offers: Vec<BarterOfferRec>,
    /// M11 счётчик идентификаторов оферов.
    pub barter_next_id: u64,
    pub vrf_account: Pubkey,
    pub commit_slot: u64,
    pub vrf_retries: u8,
    pub vrf_spent: u64,
}

/// M11 бартерный офер: товар за кэш напрямую между фракциями.
/// from/to — кошельки (to = Pubkey::default() = любому).
#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq, InitSpace,
)]
pub struct BarterOfferRec {
    pub id: u64,
    pub from: Pubkey,
    pub to: Pubkey,
    pub goods: u16,
    pub price: u64,
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
    /// крыша-контракт: кошелёк фракции-крыши, которой заплачено
    /// (валиден при roof_armed). Информационное поле: гасит закон
    /// сам факт armed у богатейшего.
    pub roof_to: Pubkey,
    /// контракт активен (одноразовый).
    pub roof_armed: bool,
    /// M7 тариф крыши: 0 нет, 1 чёрная (30%, гарантия), 2 красная (10%,
    /// риск беспредела). Живёт до конца партии.
    pub roof_tariff: u8,
    /// M10 голос продан фракции с этим кошельком (валиден при vote_sold).
    pub vote_sold_to: Pubkey,
    pub vote_sold: bool,
    /// M10 офер продажи голоса: кому (Pubkey::default() = нет) и за
    /// сколько. Фикс по дебрифу: покупатель подтверждает акцептом,
    /// деньги не списываются без его действия.
    pub vote_offer_to: Pubkey,
    pub vote_offer_price: u64,
    /// M9 ставка на лицензию этого раунда (эскроу).
    pub bid: u64,
    /// M9 куплен инсайд о доходности лицензии.
    pub insider: bool,
}
