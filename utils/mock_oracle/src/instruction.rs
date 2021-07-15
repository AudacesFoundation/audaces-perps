use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    instruction::{AccountMeta, Instruction},
    program_error::ProgramError,
    pubkey::Pubkey,
};

#[repr(C)]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum MockOracleInstruction {
    /// Creates a new perpetuals Market based on a currency
    ///
    /// Accounts expected by this instruction:
    ///
    ///   * Single owner
    ///   1. `[writable]` The oracle account
    ChangePrice { new_price: u64 },
}

pub fn change_price(
    mock_oracle_program_id: Pubkey,
    new_price: u64,
    oracle_account: Pubkey,
) -> Result<Instruction, ProgramError> {
    let instruction_data = MockOracleInstruction::ChangePrice { new_price };
    let data = instruction_data.try_to_vec().unwrap();
    let mut accounts = Vec::with_capacity(1);
    accounts.push(AccountMeta::new(oracle_account, false));

    Ok(Instruction {
        program_id: mock_oracle_program_id,
        accounts,
        data,
    })
}
