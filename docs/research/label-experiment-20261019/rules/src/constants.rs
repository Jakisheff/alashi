use anchor_lang::prelude::*;

#[constant]
pub const GAME_SEED: &[u8] = b"game";

#[constant]
pub const FACTION_SEED: &[u8] = b"faction";

pub const MIN_FACTIONS: u8 = 2;
pub const MAX_FACTIONS: u8 = 6;
// Must match the serialized Game account allocation.
pub const MAX_BARTER_OFFERS: usize = 8;
pub const ROUNDS: u8 = 6;
pub const LOBBY_MULT: i64 = 5;
pub const PESO: u64 = 1_000_000;
pub const DEFAULT_RAKE_BPS: u16 = 500;
pub const PRODUCE_YIELD: u16 = 2;
pub const BRIBE_PRICE: u64 = 5 * PESO;
pub const MAX_INFLUENCE: u16 = 1_000;
pub const MAX_NAME: usize = 16;

pub const NO_LAW: u8 = 255;
pub const LAW_STATUS_QUO: u8 = 0;
pub const LAW_TAX_10: u8 = 1;
pub const LAW_TAX_20: u8 = 2;
pub const LAW_SUBSIDY_PRODUCE: u8 = 3;
pub const LAW_SUBSIDY_POOR: u8 = 4;
pub const LAW_SUBSIDY_RICH: u8 = 5;
pub const LAW_EMBARGO: u8 = 6;
pub const LAW_BOOM: u8 = 7;
pub const DECK_SIZE: u8 = 8;
pub const DONKEY_PRICE: u64 = 1;
pub const PAYOUT_SHARES: [u64; 4] = [50, 30, 15, 5];

pub const ENTROPY_SLOTHASH: u8 = 0;
pub const ENTROPY_SWITCHBOARD: u8 = 1;

// Режим веса голоса (SPEC_VOTE_CONTRIBUTION.md):
// legacy = вес голоса = influence (как было);
// contribution = влияние + SKIP_VOTE_WEIGHT, если фракция пропустила
// фазу Action этого раунда (взнос-как-голос).
pub const VOTE_WEIGHT_LEGACY: u8 = 0;
pub const VOTE_WEIGHT_CONTRIB: u8 = 1;
// +2, а не +1: пропуск стоит упущенного produce (~2 товара ≈ 8-14M),
// взятка даёт +1 за 5M; при +1 пропуск всегда хуже взятки и мёртв.
pub const SKIP_VOTE_WEIGHT: u16 = 2;

// ---------- SPEC_EPOCH_90S: эпоха 90-х (epoch = 1) ----------
pub const EPOCH_CLASSIC: u8 = 0;
pub const EPOCH_90S: u8 = 1;
// M1 девальвация: кэш ×0.62 в начале каждого раунда r>=2.
// Калибровка по фактуре: тенге 4.7/USD (15.11.1993) -> 50+/USD (11.1994),
// падение ×10.6 за год; 5 девальваций партии по ×0.62 дают ×0.09.
// Инфляция 1992 = 2500%, 1994 = 1260% (SOURCE_BIZ_KZ_90S_FULL, п.20, 25).
pub const DEPRECIATION_NUM: u64 = 62;
pub const DEPRECIATION_DEN: u64 = 100;
// M2 крыша: цена 20% кэша, блокирует первый анти-богатый закон против хозяина
pub const ROOF_NUM: u64 = 20;
pub const ROOF_DEN: u64 = 100;
pub const ROOF_NONE: u8 = 255;
// M3 челнок: +3 товара, таможня p≈25% (байт сида < 64 из 256)
pub const SHUTTLE_GOODS: u16 = 3;
pub const CUSTOMS_THRESHOLD: u64 = 64;
// M4 вексель: выручка ×1.25, карта «взаимозачёт»
pub const CREDIT_NUM: u64 = 125;
pub const CREDIT_DEN: u64 = 100;
pub const LAW_AMNESTY: u8 = 8;
// M6 валютчик: обмен кэш <-> твёрдая валюта ×0.8 (спред 20%)
pub const EXCHANGE_NUM: u64 = 80;
pub const EXCHANGE_DEN: u64 = 100;
// M7 крыши: чёрная 30% (гарантия от таможни и закона), красная 10%
// (закон гасит, но p≈25% «беспредела»: горит весь товар)
pub const ROOF_BLACK: u8 = 1;
pub const ROOF_RED: u8 = 2;
pub const ROOF_BLACK_NUM: u64 = 30;
pub const ROOF_BLACK_DEN: u64 = 100;
pub const ROOF_RED_NUM: u64 = 10;
pub const ROOF_RED_DEN: u64 = 100;
pub const RED_MAYHEM_THRESHOLD: u64 = 64;
// M8 таможенник: дань президенту с серого хода при льготной границе
pub const CUSTOMS_TRIBUTE: u64 = 2 * PESO;
// M9 лицензия: аукцион в раунде 4, доход 20-60M, инсайд 5M
pub const AUCTION_ROUND: u8 = 4;
pub const LICENSE_MIN_YIELD: u64 = 20 * PESO;
pub const LICENSE_YIELD_SPAN: u64 = 40 * PESO;
pub const LICENSE_INSIGHT_PRICE: u64 = 5 * PESO;
// M10/M11: «нет офера/держателя» и бартер «любому» = Pubkey::default()
// (кошелёчная семантика вместо индексной: ончейн-аккаунты
// адресуются ключами, порядок массива у кранкера не каноничен)
// M5 завод: +5% банка из рейка фракции с макс влиянием
pub const FACTORY_NUM: u64 = 5;
pub const FACTORY_DEN: u64 = 100;
// анти-богатые карты (блокируются крышей)
pub const ANTI_RICH_TAX10: u8 = 1;
pub const ANTI_RICH_TAX20: u8 = 2;
// TODO: пересчитать при апдейте порога (курс SOL/USD). ~$100 эквивалент,
// консервативно округлено до 1 SOL на момент реализации 01.09.2026.
pub const MAINNET_VRF_THRESHOLD: u64 = 1_000_000_000;
pub const REVEAL_TIMEOUT_SLOTS: u64 = 25;
pub const MAX_VRF_RETRIES: u8 = 3;

#[constant]
pub const PRICE_TABLE: [u64; 16] = [12, 10, 9, 8, 7, 6, 5, 4, 3, 3, 2, 2, 2, 1, 1, 1];

pub fn price_at(sold: u16) -> u64 {
    PRICE_TABLE[(sold as usize).min(PRICE_TABLE.len() - 1)]
}
