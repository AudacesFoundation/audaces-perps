use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
};

use crate::{
    state::user_account::{get_position, remove_position, write_position, UserAccountState},
    utils::{check_account_owner, check_signer},
};

struct Accounts<'a, 'b: 'a> {
    source_user_account_owner: &'a AccountInfo<'b>,
    source_user_account: &'a AccountInfo<'b>,
    destination_user_account_owner: &'a AccountInfo<'b>,
    destination_user_account: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let source_user_account_owner = next_account_info(accounts_iter)?;
        let source_user_account = next_account_info(accounts_iter)?;
        let destination_user_account_owner = next_account_info(accounts_iter)?;
        let destination_user_account = next_account_info(accounts_iter)?;

        check_signer(source_user_account_owner).unwrap();
        check_signer(destination_user_account_owner).unwrap();
        check_account_owner(source_user_account, program_id).unwrap();
        check_account_owner(destination_user_account, program_id).unwrap();

        Ok(Self {
            source_user_account_owner,
            source_user_account,
            destination_user_account_owner,
            destination_user_account,
        })
    }
}

pub fn process_transfer_position(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    position_index: u16,
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut source_user_account_header =
        UserAccountState::unpack_from_slice(&accounts.source_user_account.data.borrow())?;
    let mut destination_user_account_header =
        UserAccountState::unpack_from_slice(&accounts.destination_user_account.data.borrow())?;

    // Verifications
    if source_user_account_header.owner != accounts.source_user_account_owner.key.to_bytes() {
        msg!("Invalid source user account owner provided");
        return Err(ProgramError::InvalidArgument);
    }
    if destination_user_account_header.owner
        != accounts.destination_user_account_owner.key.to_bytes()
    {
        msg!("Invalid destination user account owner provided");
        return Err(ProgramError::InvalidArgument);
    }

    if destination_user_account_header.market != source_user_account_header.market {
        msg!("The user accounts should be associated to the same market");
        return Err(ProgramError::InvalidArgument);
    }

    let position = get_position(
        &mut accounts.source_user_account.data.borrow_mut(),
        &source_user_account_header,
        position_index,
    )?;
    remove_position(
        &mut accounts.source_user_account.data.borrow_mut(),
        &mut source_user_account_header,
        position_index as u32,
    )?;
    write_position(
        &mut accounts.destination_user_account.data.borrow_mut(),
        destination_user_account_header.number_of_open_positions as u16,
        &mut destination_user_account_header,
        &position,
        false,
    )?;
    source_user_account_header.pack_into_slice(&mut accounts.source_user_account.data.borrow_mut());
    destination_user_account_header
        .pack_into_slice(&mut accounts.destination_user_account.data.borrow_mut());

    Ok(())
}
