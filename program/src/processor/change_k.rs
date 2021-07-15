use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
};

use crate::{
    error::PerpError,
    state::market::MarketState,
    utils::{check_account_owner, check_signer},
};

struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    admin: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();
        let market = next_account_info(accounts_iter)?;
        let admin = next_account_info(accounts_iter)?;
        check_account_owner(market, program_id)?;
        check_signer(admin)?;
        Ok(Self { market, admin })
    }
}

pub fn process_change_k(
    program_id: &Pubkey,
    factor: u64, // FP 32
    accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    if market_state.open_longs_v_coin != market_state.open_shorts_v_coin {
        msg!("The market must be perfectly balanced for this operation to succeed");
        return Err(PerpError::ImbalancedMarket.into());
    }
    let admin_address = Pubkey::new(&market_state.admin_address);

    if &admin_address != accounts.admin.key {
        msg!("The provided admin account is invalid");
        return Err(ProgramError::InvalidArgument);
    }

    market_state.v_coin_amount =
        (((market_state.v_coin_amount as u128) * (factor as u128)) >> 32) as u64;
    market_state.v_pc_amount =
        (((market_state.v_pc_amount as u128) * (factor as u128)) >> 32) as u64;

    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());

    Ok(())
}
