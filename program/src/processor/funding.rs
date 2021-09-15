use std::str::FromStr;

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

use crate::{
    error::PerpError,
    state::market::MarketState,
    utils::{check_account_key, check_account_owner, get_oracle_price},
};

use super::{FUNDING_LABEL, FUNDING_NORMALIZATION, FUNDING_PERIOD, HISTORY_PERIOD};

pub struct Accounts<'a, 'b: 'a> {
    clock_sysvar: &'a AccountInfo<'b>,
    market: &'a AccountInfo<'b>,
    oracle: &'a AccountInfo<'b>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let accounts_iter = &mut accounts.iter();

        let clock_sysvar = next_account_info(accounts_iter)?;
        let market = next_account_info(accounts_iter)?;
        let oracle = next_account_info(accounts_iter)?;
        let label = next_account_info(accounts_iter)?;

        check_account_key(clock_sysvar, &solana_program::sysvar::clock::ID).unwrap();
        check_account_key(label, &Pubkey::from_str(FUNDING_LABEL).unwrap()).unwrap();
        check_account_owner(market, program_id).unwrap();

        Ok(Self {
            clock_sysvar,
            market,
            oracle,
        })
    }
}

pub fn process_funding(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accounts = Accounts::parse(program_id, accounts)?;

    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    if market_state.oracle_address != accounts.oracle.key.to_bytes() {
        msg!("Provided oracle account is incorrect.");
        return Err(ProgramError::InvalidArgument);
    }

    let current_timestamp = Clock::from_account_info(accounts.clock_sysvar)?.unix_timestamp as u64;

    let mut nop = true;

    if current_timestamp > market_state.last_recording_timestamp + HISTORY_PERIOD {
        let oracle_price = get_oracle_price(
            &accounts.oracle.data.borrow(),
            market_state.coin_decimals,
            market_state.quote_decimals,
        )?;
        let mark_price = (((market_state.v_pc_amount as u128) << 32)
            / (market_state.v_coin_amount as u128)) as u64;
        let current_delta = (mark_price as i64) - (oracle_price as i64);
        let current_value = current_delta.signum()
            * ((((current_delta.abs() as u128) << 32) / (oracle_price as u128)) as i64);
        market_state.funding_samples_sum += current_value;
        market_state.funding_samples_count += 1;
        market_state.last_recording_timestamp += HISTORY_PERIOD;
        nop = false;
    }

    if current_timestamp > market_state.last_funding_timestamp + FUNDING_PERIOD {
        let s = market_state.funding_samples_sum;
        let denom = (market_state.funding_samples_count as u64) * FUNDING_NORMALIZATION;
        let funding_ratio = s.signum() * ((s.abs() as u64) / denom) as i64;

        let mut funding_balancing_factor = match funding_ratio.is_positive() {
            true => ((market_state.open_longs_v_coin as u128) << 32)
                .checked_div(market_state.open_shorts_v_coin as u128)
                .unwrap_or(0),
            false => ((market_state.open_shorts_v_coin as u128) << 32)
                .checked_div(market_state.open_longs_v_coin as u128)
                .unwrap_or(0),
        } as u64;
        funding_balancing_factor = core::cmp::min(1 << 32, funding_balancing_factor);

        let funding_history_offset = market_state.funding_history_offset as usize;

        let mark_price = (((market_state.v_pc_amount as u128) << 32)
            / (market_state.v_coin_amount as u128)) as u64;

        market_state.funding_history[funding_history_offset] =
            (((funding_ratio as i128) * (mark_price as i128)) >> 32) as i64;
        market_state.funding_balancing_factors[funding_history_offset] = funding_balancing_factor;
        market_state.funding_history_offset =
            (market_state.funding_history_offset + 1) % (market_state.funding_history.len() as u8);
        let elapsed_funding_cycles =
            (current_timestamp - market_state.last_funding_timestamp) / FUNDING_PERIOD;
        market_state.last_funding_timestamp += elapsed_funding_cycles * FUNDING_PERIOD;
        market_state.funding_samples_sum = 0;
        market_state.funding_samples_count = 0;
        nop = false;
    }

    if nop {
        return Err(PerpError::Nop.into());
    }

    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());
    Ok(())
}
