use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
};

use crate::{
    state::user_account::UserAccountState,
    utils::{check_account_owner, check_signer},
};

struct Accounts<'a, 'b: 'a> {
    user_account: &'a AccountInfo<'b>,
    user_account_owner: &'a AccountInfo<'b>,
    lamports_target: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();
        let user_account = next_account_info(accounts_iter)?;
        let user_account_owner = next_account_info(accounts_iter)?;
        let lamports_target = next_account_info(accounts_iter)?;
        check_account_owner(user_account, program_id)?;
        check_signer(user_account_owner)?;
        Ok(Self {
            user_account,
            user_account_owner,
            lamports_target,
        })
    }
}

pub fn process_close_account(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let user_account = UserAccountState::unpack_from_slice(&accounts.user_account.data.borrow())?;

    if &Pubkey::new(&user_account.owner) != accounts.user_account_owner.key {
        msg!("Incorrect user account owner provided");
        return Err(ProgramError::InvalidArgument);
    }

    if user_account.number_of_open_positions != 0 {
        msg!("The user account has active positions");
        return Err(ProgramError::InvalidAccountData);
    }

    if user_account.balance != 0 {
        msg!("The user accounts has some remaining balance");
        return Err(ProgramError::InvalidAccountData);
    }

    // Close account

    let mut account_lamports = accounts.user_account.lamports.borrow_mut();
    let mut target_lamports = accounts.lamports_target.lamports.borrow_mut();

    **target_lamports += **account_lamports;
    **account_lamports = 0;

    Ok(())
}
