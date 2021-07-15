use crate::instruction;
use borsh::BorshDeserialize;
use instruction::MockOracleInstruction;
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
};

pub struct Processor {}

impl Processor {
    pub fn process_change_price(accounts: &[AccountInfo], new_price: u64) -> ProgramResult {
        let accounts_iter = &mut accounts.iter();
        let oracle_account = next_account_info(accounts_iter)?;

        // &new_price.to_le_bytes()[..].pack_into_slice(oracle_account.data.borrow_mut());
        let buff: &mut [u8] = &mut oracle_account.data.borrow_mut();
        buff[0..8].copy_from_slice(&new_price.to_le_bytes());

        Ok(())
    }

    pub fn process_instruction(accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
        msg!("Beginning processing");
        let instruction = MockOracleInstruction::try_from_slice(instruction_data)
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        msg!("Instruction unpacked");

        match instruction {
            MockOracleInstruction::ChangePrice { new_price } => {
                msg!("Instruction: Change Price to {:?}", new_price);
                Processor::process_change_price(accounts, new_price)?;
            }
        }
        Ok(())
    }
}
