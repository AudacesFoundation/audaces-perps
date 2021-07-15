use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
};
use spl_token::instruction::transfer;

use crate::{
    error::PerpError,
    state::{market::MarketState, user_account::UserAccountState},
    utils::{check_account_key, check_account_owner, check_signer},
};

pub struct Accounts<'a, 'b: 'a> {
    spl_token_program: &'a AccountInfo<'b>,
    market: &'a AccountInfo<'b>,
    market_signer: &'a AccountInfo<'b>,
    market_vault: &'a AccountInfo<'b>,
    user_account_owner: &'a AccountInfo<'b>,
    user_account: &'a AccountInfo<'b>,
    target: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let mut accounts_iter = accounts.iter();

        let spl_token_program = next_account_info(&mut accounts_iter)?;
        let market = next_account_info(&mut accounts_iter)?;
        let market_signer = next_account_info(&mut accounts_iter)?;
        let market_vault = next_account_info(&mut accounts_iter)?;
        let user_account_owner = next_account_info(&mut accounts_iter)?;
        let user_account = next_account_info(&mut accounts_iter)?;
        let target = next_account_info(&mut accounts_iter)?;

        check_account_key(spl_token_program, &spl_token::id()).unwrap();
        check_signer(user_account_owner).unwrap();
        check_account_owner(user_account, program_id).unwrap();
        check_account_owner(market, program_id).unwrap();

        Ok(Self {
            spl_token_program,
            market,
            market_signer,
            market_vault,
            user_account_owner,
            user_account,
            target,
        })
    }
}

pub fn process_withdraw_budget(
    program_id: &Pubkey,
    amount: u64,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    // Parsing
    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    let mut user_account_header =
        UserAccountState::unpack_from_slice(&accounts.user_account.data.borrow())?;

    // Verifications
    if accounts.user_account_owner.key != &Pubkey::new(&user_account_header.owner) {
        msg!("The user account owner doesn't match");
        return Err(ProgramError::InvalidArgument);
    }
    if &Pubkey::new(&user_account_header.market) != accounts.market.key {
        msg!("The user account market doesn't match the given market account");
        return Err(ProgramError::InvalidArgument);
    }
    if &Pubkey::new(&market_state.vault_address) != accounts.market_vault.key {
        msg!("Invalid vault account provided");
        return Err(ProgramError::InvalidArgument);
    }
    if user_account_header.balance < amount {
        msg!("The user budget is not sufficient");
        return Err(PerpError::NoMoreFunds.into());
    }

    user_account_header.balance -= amount;
    market_state.total_user_balances -= amount;

    //Transfer the funds to the vault
    let instruction = transfer(
        &spl_token::id(),
        accounts.market_vault.key,
        accounts.target.key,
        accounts.market_signer.key,
        &[],
        amount,
    )?;

    invoke_signed(
        &instruction,
        &[
            accounts.spl_token_program.clone(),
            accounts.market_vault.clone(),
            accounts.target.clone(),
            accounts.market_signer.clone(),
        ],
        &[&[
            &accounts.market.key.to_bytes(),
            &[market_state.signer_nonce],
        ]],
    )?;

    user_account_header.pack_into_slice(&mut accounts.user_account.data.borrow_mut());
    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());

    Ok(())
}
