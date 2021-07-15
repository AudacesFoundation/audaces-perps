use std::str::FromStr;

use pyth_client::{cast, Mapping, Price, PriceStatus, Product};
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
    utils::{check_account_key, check_account_owner, get_pyth_market_symbol},
};

use super::PYTH_MAPPING_ACCOUNT;

pub struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    pyth_oracle_mapping: &'a AccountInfo<'b>,
    pyth_oracle_product: &'a AccountInfo<'b>,
    pyth_oracle_price: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let mut accounts_iter = accounts.iter();

        let market = next_account_info(&mut accounts_iter)?;
        let pyth_oracle_mapping = next_account_info(&mut accounts_iter)?;
        let pyth_oracle_product = next_account_info(&mut accounts_iter)?;
        let pyth_oracle_price = next_account_info(&mut accounts_iter)?;

        check_account_key(
            pyth_oracle_mapping,
            &Pubkey::from_str(PYTH_MAPPING_ACCOUNT).unwrap(),
        )
        .unwrap();
        check_account_owner(market, program_id).unwrap();

        Ok(Self {
            market,
            pyth_oracle_mapping,
            pyth_oracle_product,
            pyth_oracle_price,
        })
    }
}

pub fn process_update_oracle_account(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    // Verify the price account key, this only holds for the Pyth Oracle
    let pyth_mapping_data = accounts.pyth_oracle_mapping.data.borrow();
    let pyth_mapping = cast::<Mapping>(&pyth_mapping_data);
    for (i, product_key) in pyth_mapping.products.iter().enumerate() {
        if product_key.val == accounts.pyth_oracle_product.key.to_bytes() {
            // Get data for this Product
            let pyth_product_data = accounts.pyth_oracle_product.data.borrow();
            let pyth_product = cast::<Product>(&pyth_product_data);
            let market_symbol = get_pyth_market_symbol(pyth_product)?;

            let pyth_price_data = accounts.pyth_oracle_price.data.borrow();
            let pyth_price = cast::<Price>(&pyth_price_data);

            if market_symbol
                == String::from_utf8(market_state.market_symbol.to_vec())
                    .unwrap()
                    .trim_end_matches('\u{0}')
                && pyth_product.px_acc.val == accounts.pyth_oracle_price.key.to_bytes()
                && pyth_product.px_acc.is_valid()
                && matches!(pyth_price.agg.status, PriceStatus::Trading)
            {
                break;
            }
        } else if i == pyth_mapping.products.len() - 1 {
            msg!("The provided product account is not listed in the pyth mapping account.");
            return Err(ProgramError::InvalidArgument);
        }
    }

    if accounts.pyth_oracle_price.key.to_bytes() == market_state.oracle_address {
        return Err(PerpError::Nop.into());
    }
    market_state.oracle_address = accounts.pyth_oracle_price.key.to_bytes();
    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());

    Ok(())
}
