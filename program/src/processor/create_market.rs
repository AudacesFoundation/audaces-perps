use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
    sysvar::Sysvar,
};
use spl_token::state::Account;

use crate::{
    processor::{FUNDING_PERIOD, HISTORY_PERIOD},
    state::market::MarketState,
    utils::get_oracle_price,
};

pub struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    clock_sysvar: &'a AccountInfo<'b>,
    oracle: &'a AccountInfo<'b>,
    admin: &'a AccountInfo<'b>,
    vault: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(accounts: &'a [AccountInfo<'b>]) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let market = next_account_info(accounts_iter)?;
        let clock_sysvar = next_account_info(accounts_iter)?;
        let oracle = next_account_info(accounts_iter)?;
        let admin = next_account_info(accounts_iter)?;
        let vault = next_account_info(accounts_iter)?;

        if market.data.borrow()[0] != 0 {
            msg!("Market account is already initialized.");
            return Err(ProgramError::InvalidAccountData);
        }

        Ok(Self {
            market,
            clock_sysvar,
            oracle,
            admin,
            vault,
        })
    }
}

pub fn process_create_market(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    market_symbol: String,
    signer_nonce: u8,
    initial_v_pc_amount: u64,
    coin_decimals: u8,
    quote_decimals: u8,
) -> ProgramResult {
    let accounts = Accounts::parse(accounts)?;

    let oracle_price = get_oracle_price(
        &accounts.oracle.data.borrow(),
        coin_decimals,
        quote_decimals,
    )?;
    let v_coin_amount = (((initial_v_pc_amount as u128) << 32) / (oracle_price as u128)) as u64;

    let vault = Account::unpack_from_slice(&accounts.vault.data.borrow())
        .map_err(|_| ProgramError::InvalidArgument)
        .unwrap();

    let market_signer_key = Pubkey::create_program_address(
        &[&accounts.market.key.to_bytes(), &[signer_nonce]],
        &program_id,
    )
    .unwrap();

    if vault.owner != market_signer_key {
        msg!("The vault should be owned by the market_signer");
        return Err(ProgramError::InvalidArgument);
    }

    if vault.delegate.is_some() {
        msg!("The vault shouldn't have a delegate authority");
        return Err(ProgramError::InvalidArgument);
    }

    if vault.close_authority.is_some() {
        msg!("The vault should have no close authority");
        return Err(ProgramError::InvalidArgument);
    }

    let mut market_symbol_slice = [0u8; 32];
    let market_symbol_bytes = market_symbol.as_bytes();
    if market_symbol_bytes.len() > 32 {
        msg!("Given market symbol is too long.");
        return Err(ProgramError::InvalidAccountData);
    }
    market_symbol_slice[..market_symbol_bytes.len()].copy_from_slice(market_symbol_bytes);
    msg!("Creating Market {:?}", market_symbol);

    let current_timestamp = Clock::from_account_info(accounts.clock_sysvar)?.unix_timestamp as u64;

    let market_state = MarketState {
        version: 0,
        signer_nonce,
        market_symbol: market_symbol_slice,
        oracle_address: accounts.oracle.key.to_bytes(),
        admin_address: accounts.admin.key.to_bytes(),
        vault_address: accounts.vault.key.to_bytes(),
        coin_decimals,
        quote_decimals,
        total_collateral: 0,
        total_user_balances: 0,
        open_longs_v_coin: 0,
        open_shorts_v_coin: 0,
        open_longs_v_pc: 0,
        open_shorts_v_pc: 0,
        v_coin_amount,
        v_pc_amount: initial_v_pc_amount,
        last_funding_timestamp: current_timestamp - (current_timestamp % FUNDING_PERIOD), // Align funding and recording to round timestamps
        last_recording_timestamp: current_timestamp - (current_timestamp % HISTORY_PERIOD),
        funding_samples_count: 0,
        funding_samples_sum: 0,
        funding_history_offset: 0,
        funding_history: [0i64; 16],
        funding_balancing_factors: [0u64; 16],
        total_fee_balance: 0,
        rebalancing_funds: 0,
        rebalanced_v_coin: 0,
        number_of_instances: 0,
    };

    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());

    Ok(())
}
