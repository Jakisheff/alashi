#[cfg(feature = "chain")]
use anchor_lang::error_code;

#[cfg(feature = "chain")]
macro_rules! game_error_enum {
    ($( $name:ident => $msg:expr, )*) => {
        #[error_code]
        pub enum GameError {
            $( #[msg($msg)] $name, )*
        }
    };
}

#[cfg(not(feature = "chain"))]
macro_rules! game_error_enum {
    ($( $name:ident => $msg:expr, )*) => {
        #[derive(Debug)]
        pub enum GameError {
            $( $name, )*
        }
    };
}

game_error_enum! {
    InvalidEntryFee => "entry fee must be greater than zero",
    InvalidPhaseDuration => "phase duration cannot be negative",
    NameTooLong => "name longer than 16 bytes",
    GameNotInLobby => "game is not in lobby phase",
    GameFinished => "game is finished",
    GameFull => "game is full",
    WrongPhase => "action not allowed in this phase",
    AlreadyActed => "faction already acted in this phase",
    AlreadyVoted => "faction already voted in this phase",
    NoUnits => "no units to sell",
    NotEnoughGoods => "not enough goods",
    NotEnoughCash => "not enough cash",
    BribeTooSmall => "bribe must buy at least one influence",
    BribeTooBig => "bribe exceeds influence cap",
    SelfBribe => "cannot bribe yourself",
    NotAlive => "faction is not alive",
    TooEarly => "phase deadline not reached yet",
    NotEnoughFactions => "not enough factions joined",
    InvalidFactionSet => "faction set does not match game factions",
    NoSlotHashes => "slot hashes sysvar is empty",
    NotPresident => "signer is not the president",
    AlreadyVetoed => "veto already cast this law phase",
    NotFinished => "game is not finished",
    AlreadySettled => "game already settled",
    InvalidSettleSet => "settle account set is invalid",
    EmptyBank => "bank is empty",
    InvalidEntropyMode => "invalid entropy mode",
    VrfRequired => "bank above threshold requires VRF entropy mode",
    LawNotRevealed => "law card not revealed yet",
    RandomnessMismatch => "randomness account mismatch",
    RandomnessNotReady => "randomness not resolved for this slot",
    GameAborted => "game aborted by oracle failure",
    NotAdmin => "caller is not the game admin",
    InvalidVoteWeightMode => "invalid vote weight mode",
    InvalidEpoch => "invalid epoch (0 classic | 1 90s | 2 donkey risk)",
    DuplicateWallet => "wallet already joined this game",
    TooManyOffers => "at most eight open barter offers per game",
    NoMarketRemaining => "donkey risk is unavailable in the final round",
    DonkeyRewardOverflow => "donkey reward would overflow goods",
    DonkeyOracleInvalid => "donkey oracle configuration is missing or invalid",
}

#[cfg(test)]
mod tests {
    use super::GameError;

    #[test]
    fn legacy_error_codes_stay_stable() {
        assert_eq!(GameError::DuplicateWallet as u32, 35);
        assert_eq!(GameError::TooManyOffers as u32, 36);
        assert_eq!(GameError::NoMarketRemaining as u32, 37);
    }
}
