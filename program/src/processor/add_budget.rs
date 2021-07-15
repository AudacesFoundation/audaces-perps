use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::invoke,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
};
use spl_token::instruction::transfer;

use crate::{
    state::{is_initialized, market::MarketState, user_account::UserAccountState},
    utils::{check_account_key, check_account_owner, check_signer},
};

struct Accounts<'a, 'b: 'a> {
    spl_token_program: &'a AccountInfo<'b>,
    market: &'a AccountInfo<'b>,
    market_vault: &'a AccountInfo<'b>,
    user_account: &'a AccountInfo<'b>,
    source_owner: &'a AccountInfo<'b>,
    source: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let spl_token_program = next_account_info(accounts_iter)?;
        let market = next_account_info(accounts_iter)?;
        let market_vault = next_account_info(accounts_iter)?;
        let user_account = next_account_info(accounts_iter)?;
        let source_owner = next_account_info(accounts_iter)?;
        let source = next_account_info(accounts_iter)?;

        check_account_key(spl_token_program, &spl_token::id()).unwrap();
        check_account_owner(market, program_id).unwrap();
        check_account_owner(user_account, program_id).unwrap();
        check_signer(source_owner).unwrap();

        Ok(Self {
            spl_token_program,
            market,
            market_vault,
            user_account,
            source_owner,
            source,
        })
    }
}

pub fn process_add_budget(
    program_id: &Pubkey,
    amount: u64,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    // Parsing
    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    let mut user_account_header = match is_initialized(accounts.user_account) {
        true => UserAccountState::unpack_from_slice(&accounts.user_account.data.borrow())?,
        false => UserAccountState {
            version: 0,
            owner: accounts.source_owner.key.to_bytes(),
            active: false,
            market: accounts.market.key.to_bytes(),
            balance: 0,
            last_funding_offset: market_state.funding_history_offset,
            number_of_open_positions: 0,
        },
    };

    // Verifications
    if !accounts.source_owner.is_signer {
        msg!("The account owner should be a signer");
        return Err(ProgramError::MissingRequiredSignature);
    }
    if accounts.user_account.owner != program_id {
        msg!("The open position should be owned by the program");
        return Err(ProgramError::InvalidArgument);
    }
    if &Pubkey::new(&user_account_header.market) != accounts.market.key {
        msg!("The user account market doesn't match the given market account");
        return Err(ProgramError::InvalidArgument);
    } else if &Pubkey::new(&market_state.vault_address) != accounts.market_vault.key {
        msg!(
            "Invalid vault account provided: {:?} vs expected {:?}",
            accounts.market_vault.key,
            Pubkey::new(&market_state.vault_address)
        );
        return Err(ProgramError::InvalidArgument);
    }

    market_state.total_user_balances += amount;
    user_account_header.balance += amount;

    //Transfer the funds to the vault
    let instruction = transfer(
        &spl_token::id(),
        accounts.source.key,
        accounts.market_vault.key,
        accounts.source_owner.key,
        &[],
        amount,
    )?;

    invoke(
        &instruction,
        &[
            accounts.spl_token_program.clone(),
            accounts.source.clone(),
            accounts.market_vault.clone(),
            accounts.source_owner.clone(),
        ],
    )?;

    user_account_header.pack_into_slice(&mut accounts.user_account.data.borrow_mut());
    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());

    Ok(())
}
