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
    user_account_owner: &'a AccountInfo<'b>,
    user_account: &'a AccountInfo<'b>,
    new_user_account_owner: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let user_account_owner = next_account_info(accounts_iter)?;
        let user_account = next_account_info(accounts_iter)?;
        let new_user_account_owner = next_account_info(accounts_iter)?;

        check_signer(user_account_owner).unwrap();
        check_account_owner(user_account, program_id).unwrap();

        Ok(Self {
            user_account_owner,
            user_account,
            new_user_account_owner,
        })
    }
}

pub fn process_transfer_user_account(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut user_account_header =
        UserAccountState::unpack_from_slice(&accounts.user_account.data.borrow())?;

    // Verifications
    if user_account_header.owner != accounts.user_account_owner.key.to_bytes() {
        msg!("Invalid user account owner provided");
        return Err(ProgramError::InvalidArgument);
    }

    user_account_header.owner = accounts.new_user_account_owner.key.to_bytes();
    user_account_header.pack_into_slice(&mut accounts.user_account.data.borrow_mut());

    Ok(())
}
