use anchor_lang::prelude::*;

#[error_code]
pub enum GameError {
    #[msg("entry fee must be greater than zero")]
    InvalidEntryFee,
    #[msg("phase duration cannot be negative")]
    InvalidPhaseDuration,
    #[msg("name longer than 16 bytes")]
    NameTooLong,
    #[msg("game is not in lobby phase")]
    GameNotInLobby,
    #[msg("game is finished")]
    GameFinished,
    #[msg("game is full")]
    GameFull,
    #[msg("action not allowed in this phase")]
    WrongPhase,
    #[msg("faction already acted in this phase")]
    AlreadyActed,
    #[msg("faction already voted in this phase")]
    AlreadyVoted,
    #[msg("no units to sell")]
    NoUnits,
    #[msg("not enough goods")]
    NotEnoughGoods,
    #[msg("not enough cash")]
    NotEnoughCash,
    #[msg("bribe must buy at least one influence")]
    BribeTooSmall,
    #[msg("bribe exceeds influence cap")]
    BribeTooBig,
    #[msg("cannot bribe yourself")]
    SelfBribe,
    #[msg("faction is not alive")]
    NotAlive,
    #[msg("phase deadline not reached yet")]
    TooEarly,
    #[msg("not enough factions joined")]
    NotEnoughFactions,
    #[msg("faction set does not match game factions")]
    InvalidFactionSet,
    #[msg("slot hashes sysvar is empty")]
    NoSlotHashes,
    #[msg("signer is not the president")]
    NotPresident,
    #[msg("veto already cast this law phase")]
    AlreadyVetoed,
    #[msg("game is not finished")]
    NotFinished,
    #[msg("game already settled")]
    AlreadySettled,
    #[msg("settle account set is invalid")]
    InvalidSettleSet,
    #[msg("bank is empty")]
    EmptyBank,
}
