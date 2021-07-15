use crate::error::PerpError;
use crate::processor::Processor;
use num_traits::FromPrimitive;
use solana_program::{
    account_info::AccountInfo, decode_error::DecodeError, entrypoint::ProgramResult, msg,
    program_error::PrintProgramError, pubkey::Pubkey,
};

#[cfg(not(feature = "no-entrypoint"))]
use solana_program::entrypoint;
#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    msg!("Entrypoint");
    if let Err(error) = Processor::process_instruction(program_id, accounts, instruction_data) {
        // catch the error so we can print it
        error.print::<PerpError>();
        return Err(error);
    }
    Ok(())
}

impl PrintProgramError for PerpError {
    fn print<E>(&self)
    where
        E: 'static + std::error::Error + DecodeError<E> + PrintProgramError + FromPrimitive,
    {
        match self {
            PerpError::OutOfSpace => msg!("Error: Out of space!"),
            PerpError::MemoryError => msg!("Error: Memory Error!"),
            PerpError::PositionNotFound => msg!("Error: Position not found!"),
            PerpError::NoMoreFunds => msg!("Error: The account is out of funds!"),
            PerpError::AmountTooLow => msg!("Error: The given amount is too low!"),
            PerpError::AmountTooLarge => msg!("Error: The given amount is too large!"),
            PerpError::MarginTooLow => msg!("Error: The given margin is too small!"),
            PerpError::Nop => msg!("Error: The operation is a no-op."),
            PerpError::PendingFunding => {
                msg!("Error: The user account isn't up to date on funding.")
            }
            PerpError::Overflow => msg!(
                "Error: A math operation has overflowed. This shouldn't happen in normal usage."
            ),
            PerpError::TooManyOpenPositions => msg!(
                "Error: This open positions account has exceed its maximum number of open positions."
            ),
            PerpError::NegativePayout => msg!("Error: This open position cannot be closed as it should be liquidated."),
            PerpError::ImbalancedMarket => msg!("Error: The market is imbalanced."),
            PerpError::NetworkSlippageTooLarge => msg!("Error: The price slippage due to execution latency exceeds the specified margin")
        }
    }
}
