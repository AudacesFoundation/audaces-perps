use num_derive::FromPrimitive;
use solana_program::{decode_error::DecodeError, program_error::ProgramError};
use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, FromPrimitive, PartialEq)]
pub enum PerpError {
    #[error("Out of space.")]
    OutOfSpace,
    #[error("Memory error")]
    MemoryError,
    #[error("Position not found")]
    PositionNotFound,
    #[error("No more funds")]
    NoMoreFunds,
    #[error("Given amount is too low")]
    AmountTooLow,
    #[error("Given amount is too large")]
    AmountTooLarge,
    #[error("Given margin is too low")]
    MarginTooLow,
    #[error("This operation is a no-op")]
    Nop,
    #[error("The user account isn't up to date on funding.")]
    PendingFunding,
    #[error("A math operation has overflowed. This shouldn't happen in normal usage.")]
    Overflow,
    #[error("This user account has exceed its maximum number of open positions.")]
    TooManyOpenPositions,
    #[error("This open position cannot be closed as it should be liquidated.")]
    NegativePayout,
    #[error("The market is imbalanced.")]
    ImbalancedMarket,
    #[error("The price slippage due to execution latency exceeds the provided margin")]
    NetworkSlippageTooLarge,
}

pub type PerpResult = Result<(), PerpError>;

impl From<PerpError> for ProgramError {
    fn from(e: PerpError) -> Self {
        ProgramError::Custom(e as u32)
    }
}

impl<T> DecodeError<T> for PerpError {
    fn type_of() -> &'static str {
        "PerpError"
    }
}
