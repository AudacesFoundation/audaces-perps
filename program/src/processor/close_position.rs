use std::{slice::Iter, str::FromStr};

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    clock::Clock,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
    sysvar::{self, Sysvar},
};

use crate::{
    error::PerpError,
    positions_book::{memory::parse_memory, positions_book_tree::PositionsBook},
    processor::{FUNDING_NORMALIZATION, FUNDING_PERIOD, MAX_LEVERAGE},
    state::{
        instance::{parse_instance, write_instance_and_memory},
        market::{get_instance_address, MarketState},
        user_account::{get_position, remove_position, write_position},
    },
    state::{user_account::UserAccountState, PositionType},
    utils::{
        check_account_key, check_account_owner, check_signer, compute_fee_tier, compute_fees,
        compute_liquidation_index, get_oracle_price,
    },
};

use super::{FIDA_BNB, TRADE_LABEL};

struct Accounts<'a, 'b: 'a> {
    spl_token_program: &'a AccountInfo<'b>,
    clock_sysvar: &'a AccountInfo<'b>,
    market: &'a AccountInfo<'b>,
    instance: &'a AccountInfo<'b>,
    market_signer: &'a AccountInfo<'b>,
    market_vault: &'a AccountInfo<'b>,
    bnb_bonfida: &'a AccountInfo<'b>,
    oracle: &'a AccountInfo<'b>,
    user_account_owner: &'a AccountInfo<'b>,
    user_account: &'a AccountInfo<'b>,
    remaining: Iter<'a, AccountInfo<'b>>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let mut accounts_iter = accounts.iter();

        let spl_token_program = next_account_info(&mut accounts_iter)?;
        let clock_sysvar = next_account_info(&mut accounts_iter)?;
        let market = next_account_info(&mut accounts_iter)?;
        let instance = next_account_info(&mut accounts_iter)?;
        let market_signer = next_account_info(&mut accounts_iter)?;
        let market_vault = next_account_info(&mut accounts_iter)?;
        let bnb_bonfida = next_account_info(&mut accounts_iter)?;
        let oracle = next_account_info(&mut accounts_iter)?;
        let user_account_owner = next_account_info(&mut accounts_iter)?;
        let user_account = next_account_info(&mut accounts_iter)?;
        let label = next_account_info(&mut accounts_iter)?;
        check_account_key(label, &Pubkey::from_str(TRADE_LABEL).unwrap()).unwrap();

        check_account_key(spl_token_program, &spl_token::id()).unwrap();
        check_account_key(clock_sysvar, &sysvar::clock::ID).unwrap();
        check_account_owner(market, program_id).unwrap();
        check_account_owner(instance, program_id).unwrap();
        check_account_owner(market_vault, &spl_token::id()).unwrap();
        check_account_key(bnb_bonfida, &Pubkey::from_str(&FIDA_BNB).unwrap()).unwrap();
        check_signer(user_account_owner)?;
        check_account_owner(user_account, program_id).unwrap();

        Ok(Self {
            spl_token_program,
            clock_sysvar,
            market,
            instance,
            market_signer,
            market_vault,
            bnb_bonfida,
            oracle,
            user_account_owner,
            user_account,
            remaining: accounts_iter,
        })
    }
}

pub fn process_close_position(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    position_index: u16,
    closing_collateral: u64,
    closing_v_coin: u64,
    predicted_entry_price: u64,   // 32 bit FP
    maximum_slippage_margin: u64, // 32 bit FP
) -> ProgramResult {
    let mut accounts = Accounts::parse(program_id, accounts)?;

    // Parsing
    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    market_state.slippage_protection(predicted_entry_price, maximum_slippage_margin)?;

    let mut user_account_header =
        UserAccountState::unpack_from_slice(&accounts.user_account.data.borrow())?;
    let mut open_position = get_position(
        &mut accounts.user_account.data.borrow_mut(),
        &user_account_header,
        position_index,
    )?;

    let instance_address = get_instance_address(
        &accounts.market.data.borrow(),
        open_position.instance_index as u32,
    )?;
    if &instance_address != accounts.instance.key {
        msg!("Invalid instance account or instance index provided");
        return Err(ProgramError::InvalidArgument);
    }

    let (mut instance, mut page_infos) = parse_instance(&accounts.instance.data.borrow())?;

    if user_account_header.number_of_open_positions <= (position_index as u32) {
        msg!("Position index is invalid");
        return Err(ProgramError::InvalidArgument);
    }

    let memory = parse_memory(&instance, &page_infos, &mut accounts.remaining)?;
    let mut positions_book =
        PositionsBook::new(instance.shorts_pointer, instance.longs_pointer, memory);

    // Verifications
    if market_state.oracle_address != accounts.oracle.key.to_bytes() {
        msg!("Provided oracle account is incorrect.");
        return Err(ProgramError::InvalidArgument);
    }
    if *accounts.user_account_owner.key != Pubkey::new(&user_account_header.owner) {
        msg!("The user account owner is invalid");
        return Err(ProgramError::InvalidArgument);
    }
    if &Pubkey::new(&user_account_header.market) != accounts.market.key {
        msg!("The user account market doesn't match the given market account");
        return Err(ProgramError::InvalidArgument);
    }
    if user_account_header.last_funding_offset != market_state.funding_history_offset {
        msg!("Funding must be processed for this account.");
        return Err(PerpError::PendingFunding.into());
    }

    let clock = Clock::from_account_info(accounts.clock_sysvar)?;
    let current_timestamp = clock.unix_timestamp;

    let oracle_price = get_oracle_price(
        &accounts.oracle.data.borrow(),
        market_state.coin_decimals,
        market_state.quote_decimals,
    )?;
    let mut closing_collateral_ltd = core::cmp::min(closing_collateral, open_position.collateral);

    let closing_v_coin_ltd = core::cmp::min(closing_v_coin, open_position.v_coin_amount);

    let r = positions_book.close_position(
        open_position.liquidation_index,
        open_position.collateral,
        open_position.v_coin_amount,
        open_position.v_pc_amount,
        open_position.side,
        open_position.slot_number,
    );
    msg!("Close position in memory: {:?}", r);
    match r {
        Ok(()) => {}
        Err(PerpError::PositionNotFound) => {
            msg!("Order not found, it was liquidated at index: {:?}, with collateral {:?}, with parent node slot {:?}",
                    open_position.liquidation_index, open_position.collateral, open_position.slot_number);
            remove_position(
                &mut accounts.user_account.data.borrow_mut(),
                &mut user_account_header,
                position_index as u32,
            )?;
            user_account_header.pack_into_slice(&mut accounts.user_account.data.borrow_mut());
            return Ok(());
        }
        Err(e) => Err(e).unwrap(),
    }
    let side_sign = open_position.side.get_sign();

    let signed_closing_v_coin = side_sign * (closing_v_coin_ltd as i64);
    let v_pc_closing_amount = market_state.compute_add_v_pc(signed_closing_v_coin)?;

    msg!(
        "Transaction info: v_coin_amount {:?}, v_pc_amount {:?}",
        closing_v_coin_ltd,
        v_pc_closing_amount.abs()
    );

    // Keep entry price constant for position
    let v_pc_to_settle = (((closing_v_coin_ltd as u128) * (open_position.v_pc_amount as u128))
        / (open_position.v_coin_amount as u128)) as i64;

    let payout = match open_position.side {
        PositionType::Long => (((v_pc_closing_amount.abs() as u64) + closing_collateral_ltd)
            as i64)
            .checked_sub(v_pc_to_settle),
        PositionType::Short => (v_pc_to_settle + closing_collateral_ltd as i64)
            .checked_sub(v_pc_closing_amount.abs() as i64),
    }
    .ok_or(PerpError::Overflow)?;

    if payout < 0 {
        closing_collateral_ltd = core::cmp::min(
            closing_collateral_ltd + ((-payout) as u64),
            open_position.collateral,
        ); // The insurance fund buffers the payout in the second case
    }

    let (balanced_pc_closing_amount, balanced_closing_v_coin) =
        market_state.balance_operation(v_pc_closing_amount, signed_closing_v_coin, oracle_price)?;

    if v_pc_to_settle < 0 {
        panic!()
    }

    market_state.add_v_coin(balanced_closing_v_coin as i64)?;
    market_state.add_v_pc(balanced_pc_closing_amount)?;
    market_state.sub_open_interest(
        closing_v_coin_ltd,
        v_pc_to_settle as u64,
        open_position.side,
    )?;

    msg!(
        "Mark price for this transaction (FP32): {:?}, with size: {:?} and side {:?}",
        ((v_pc_closing_amount.abs() as u128) << 32)
            .checked_div(closing_v_coin_ltd as u128)
            .unwrap_or(0),
        closing_v_coin_ltd,
        open_position.side
    );

    let payout_ltd = core::cmp::max(payout, 0) as u64;

    // Update the open positions account
    open_position.collateral -= closing_collateral_ltd;
    open_position.v_coin_amount -= closing_v_coin_ltd;
    open_position.v_pc_amount -= v_pc_to_settle as u64;

    // Pay funding on the closed position
    // Closing a position doesn't entitle the user to receiving any funding
    if (current_timestamp as u64) < market_state.last_funding_timestamp + FUNDING_PERIOD {
        // The position doesn't have to pay funding when it happens before the current cycle's funding crank (unlikely)
        // We calculate the funding ratio for the current funding cycle until now

        let s = market_state.funding_samples_sum;
        let denom = (market_state.funding_samples_count as u64) * FUNDING_NORMALIZATION;
        let funding_ratio = s.signum() * ((s.abs() as u64).checked_div(denom).unwrap_or(0)) as i64;

        let position_v_coin = open_position.side.get_sign() * (open_position.v_coin_amount as i64);
        let mut funding_ratio = (position_v_coin.signum() * funding_ratio) as i128;
        if funding_ratio.is_negative() {
            funding_ratio = 0;
        }
        let debt = (((open_position.v_coin_amount as i128) * funding_ratio) >> 32) as i64;

        if debt as u64 > user_account_header.balance {
            msg!("Not enough available balance to pay for current round of funding.");
            return Err(PerpError::NoMoreFunds.into());
        }
        user_account_header.balance -= debt as u64;
        market_state.total_user_balances -= debt as u64;
    }

    if open_position.collateral == 0 {
        remove_position(
            &mut accounts.user_account.data.borrow_mut(),
            &mut user_account_header,
            position_index as u32,
        )?;
    } else {
        msg!("VCoin Amount {:?}", open_position.v_coin_amount);
        if open_position.v_coin_amount == 0 {
            msg!("There is some collateral left on this position. Zero-leverage positions are not supported.");
            return Err(PerpError::AmountTooLow.into());
        }
        // TODO: We don't need to compute the liquidation index here. Optimize
        let new_liquidation_index = compute_liquidation_index(
            open_position.collateral,
            open_position.v_coin_amount,
            open_position.v_pc_amount,
            open_position.side,
            market_state.get_k(),
        );
        let preliquidation = match open_position.side {
            PositionType::Long => new_liquidation_index >= oracle_price,
            PositionType::Short => new_liquidation_index <= oracle_price,
        };
        if preliquidation {
            msg!("Position margin is too low");
            return Err(PerpError::MarginTooLow.into());
        }
        let current_slot = clock.slot;
        let insertion_leaf = positions_book.open_position(
            new_liquidation_index,
            open_position.collateral,
            open_position.v_coin_amount,
            open_position.v_pc_amount,
            open_position.side,
            current_slot,
        )?;
        open_position.slot_number = insertion_leaf.get_slot_number(&positions_book.memory)?;
        open_position.liquidation_index = new_liquidation_index;

        write_position(
            &mut accounts.user_account.data.borrow_mut(),
            position_index,
            &mut user_account_header,
            &open_position,
            true,
        )?;
    }

    let new_leverage = ((open_position.v_pc_amount << 32) as u128)
        .checked_div(open_position.collateral as u128)
        .unwrap_or(0) as u64; // In the case in which there is no collateral (closing the position), the leverage is 0
    if new_leverage > MAX_LEVERAGE {
        msg!(
            "New leverage cannot be higher than: {:?}. Found: {:?}",
            MAX_LEVERAGE >> 32,
            new_leverage >> 32
        );
        return Err(PerpError::MarginTooLow.into());
    }

    // Fees for the partial closing
    let fee_tier = compute_fee_tier(&mut accounts.remaining)?;
    let mut closing_fees = compute_fees(fee_tier, v_pc_closing_amount.abs() as u64, new_leverage)?;

    msg!(
        "Closing_collateral_ltd : {:?}, new_leverage : {:?}",
        closing_collateral_ltd,
        new_leverage,
    );

    let referrer_account_opt = next_account_info(&mut accounts.remaining).ok();
    market_state.transfer_fees(
        &mut closing_fees,
        accounts.spl_token_program,
        accounts.market,
        accounts.market_vault,
        accounts.market_signer,
        accounts.bnb_bonfida,
        referrer_account_opt,
    )?;

    market_state.apply_fees(&closing_fees, open_position.collateral == 0, false)?;

    user_account_header.balance = user_account_header
        .balance
        .checked_add(payout_ltd)
        .and_then(|n| n.checked_sub(closing_fees.fixed))
        .ok_or_else(|| {
            msg!("The user does not have the funds or the payout to pay the fees");
            PerpError::NoMoreFunds
        })?;
    if open_position.collateral == 0 {
        user_account_header.balance = user_account_header
            .balance
            .checked_add(closing_fees.refundable)
            .ok_or_else(|| {
                msg!("The user account balance overflows.");
                PerpError::Overflow
            })?;
    }
    msg!("Payout : {:?}", payout);

    // Transfer the payout
    market_state.total_collateral -= closing_collateral_ltd;
    market_state.total_user_balances += payout_ltd;

    // Write into the states

    user_account_header.pack_into_slice(&mut accounts.user_account.data.borrow_mut());
    instance.update(&positions_book, &mut page_infos);
    write_instance_and_memory(
        &mut accounts.instance.data.borrow_mut(),
        &page_infos,
        &instance,
    )?;
    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());

    Ok(())
}
