use std::{cmp, slice::Iter, str::FromStr};

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
    positions_book::{memory::parse_memory, positions_book_tree::PositionsBook},
    state::{
        instance::{parse_instance, write_instance_and_memory},
        market::{get_instance_address, MarketState},
        user_account::{get_position, remove_position, write_position},
    },
    state::{user_account::UserAccountState, PositionType},
    utils::{
        check_account_key, check_account_owner, compute_liquidation_index, compute_payout,
        get_oracle_price,
    },
};

use super::{FUNDING_EXTRACTION_LABEL, MINIMAL_FUNDING};

pub struct Accounts<'a, 'b: 'a> {
    market: &'a AccountInfo<'b>,
    instance: &'a AccountInfo<'b>,
    user_account: &'a AccountInfo<'b>,
    oracle: &'a AccountInfo<'b>,
    remaining: Iter<'a, AccountInfo<'b>>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let mut accounts_iter = accounts.iter();
        let market = next_account_info(&mut accounts_iter)?;
        let instance = next_account_info(&mut accounts_iter)?;
        let user_account = next_account_info(&mut accounts_iter)?;
        let label_account = next_account_info(&mut accounts_iter)?;

        let oracle = next_account_info(&mut accounts_iter)?;

        check_account_owner(market, program_id).unwrap();
        check_account_owner(instance, program_id).unwrap();
        check_account_owner(user_account, program_id).unwrap();
        check_account_key(
            label_account,
            &Pubkey::from_str(FUNDING_EXTRACTION_LABEL).unwrap(),
        )
        .unwrap();

        Ok(Self {
            market,
            instance,
            user_account,
            oracle,
            remaining: accounts_iter,
        })
    }
}

pub fn process_funding_extraction(
    program_id: &Pubkey,
    instance_index: u8,
    accounts: &[AccountInfo],
) -> ProgramResult {
    let mut accounts = Accounts::parse(program_id, accounts)?;

    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    let mut user_account_header =
        UserAccountState::unpack_from_slice(&accounts.user_account.data.borrow())?;

    let mut last_funding_offset = None;
    let funding_history_offset = market_state.funding_history_offset as usize;

    if &Pubkey::new(&user_account_header.market) != accounts.market.key {
        msg!("The user account market doesn't match the given market account");
        return Err(ProgramError::InvalidArgument);
    }

    let instance_address =
        get_instance_address(&accounts.market.data.borrow(), instance_index as u32)?;
    if &instance_address != accounts.instance.key {
        msg!("Invalid instance account or instance index provided");
        return Err(ProgramError::InvalidArgument);
    }

    if market_state.oracle_address != accounts.oracle.key.to_bytes() {
        msg!("Provided oracle account is incorrect.");
        return Err(ProgramError::InvalidArgument);
    }

    let (mut instance, mut page_infos) = parse_instance(&accounts.instance.data.borrow())?;
    let memory = parse_memory(&instance, &page_infos, &mut accounts.remaining)?;
    let mut book = PositionsBook::new(instance.shorts_pointer, instance.longs_pointer, memory);

    let mut positions_v_coin = 0i64;
    let mut positions_collateral = 0u64;
    let mut last_funding_offset_total = 0;

    for position_index in 0..user_account_header.number_of_open_positions as u16 {
        let mut p = get_position(
            &accounts.user_account.data.borrow_mut(),
            &user_account_header,
            position_index,
        )?;
        if p.instance_index == instance_index {
            last_funding_offset = Some(p.last_funding_offset as usize);
            let s = match p.side {
                PositionType::Long => 1i64,
                PositionType::Short => -1,
            };
            if book
                .close_position(p.liquidation_index, 0, 0, 0, p.side, p.slot_number)
                .is_ok()
            {
                msg!("Position {:?} is active", position_index);
                positions_v_coin = s
                    .checked_mul(p.v_coin_amount as i64)
                    .and_then(|n| n.checked_add(positions_v_coin))
                    .unwrap();
                positions_collateral = positions_collateral.checked_add(p.collateral).unwrap();
            }
            p.last_funding_offset = market_state.funding_history_offset;
            write_position(
                &mut accounts.user_account.data.borrow_mut(),
                position_index,
                &mut user_account_header,
                &p,
                true,
            )?;
        } else {
            last_funding_offset_total = cmp::max(
                market_state
                    .funding_history_offset
                    .checked_add(market_state.funding_history.len() as u8)
                    .and_then(|a| a.checked_sub(p.last_funding_offset))
                    .map(|a| a % (market_state.funding_history.len() as u8))
                    .unwrap(),
                last_funding_offset_total,
            );
        }
    }

    if last_funding_offset.is_none() | (last_funding_offset == Some(funding_history_offset)) {
        msg!("No funding to process for this account on this instance");
        return Err(PerpError::Nop.into());
    }

    let mut balanced_funding_ratio = 0;
    let mut i = last_funding_offset.unwrap();
    let cycle = market_state.funding_history.len();
    while i != funding_history_offset {
        // Iterate to account for missed extractions
        // Market Price is included in funding_history
        let mut delta = (positions_v_coin.signum() * market_state.funding_history[i]) as i128;
        if delta.is_negative() {
            // In the case where the funding is positive for the user, inflate it for arbitrage incentive.
            delta = (delta
                * (std::cmp::max(market_state.funding_balancing_factors[i], MINIMAL_FUNDING)
                    as i128))
                >> 32;
        }
        // Add all missed funding ratios together
        balanced_funding_ratio += delta;
        i = (i + 1) % cycle;
    }

    let balanced_debt =
        (((positions_v_coin.abs() as i128) * (balanced_funding_ratio)) >> 32) as i64;

    if balanced_debt > (user_account_header.balance as i64) {
        msg!("This account has insufficient funds and must be liquidated");
        // Liquidate all positions.
        let mut remaining_debt = balanced_debt - (user_account_header.balance as i64);
        for position_index in (0..user_account_header.number_of_open_positions).rev() {
            let mut p = get_position(
                &accounts.user_account.data.borrow_mut(),
                &user_account_header,
                position_index as u16,
            )?;
            if p.instance_index == instance_index {
                let res = book.close_position(
                    p.liquidation_index,
                    p.collateral,
                    p.v_coin_amount,
                    p.v_pc_amount,
                    p.side,
                    p.slot_number,
                );
                let v_coin_amount = (p.v_coin_amount as i64) * p.side.get_sign();
                let v_pc_amount = market_state.compute_add_v_pc(v_coin_amount)?;
                let position_payout = compute_payout(
                    v_pc_amount.abs() as u64,
                    p.v_pc_amount,
                    p.collateral,
                    &p.side,
                );
                let oracle_price = get_oracle_price(
                    &accounts.oracle.data.borrow(),
                    market_state.coin_decimals,
                    market_state.quote_decimals,
                )?;
                if p.collateral > remaining_debt as u64 && res.is_ok() {
                    p.collateral -= remaining_debt as u64;
                    p.liquidation_index = compute_liquidation_index(
                        p.collateral,
                        p.v_coin_amount,
                        p.v_pc_amount,
                        p.side,
                        market_state.get_k(),
                    );
                    let is_liquidated = match p.side {
                        PositionType::Short => p.liquidation_index < oracle_price,
                        PositionType::Long => p.liquidation_index > oracle_price,
                    };
                    if !is_liquidated {
                        p.slot_number = Clock::get()?.slot;
                        book.open_position(
                            p.liquidation_index,
                            p.collateral,
                            p.v_coin_amount,
                            p.v_pc_amount,
                            p.side,
                            p.slot_number,
                        )?;
                        market_state.total_collateral = market_state
                            .total_collateral
                            .checked_sub(remaining_debt as u64)
                            .unwrap();
                        write_position(
                            &mut accounts.user_account.data.borrow_mut(),
                            position_index as u16,
                            &mut user_account_header,
                            &p,
                            true,
                        )?;
                        break;
                    }
                }
                if res.is_ok() {
                    remaining_debt -= position_payout;
                    let (balanced_v_pc, balanced_v_coin) =
                        market_state.balance_operation(v_pc_amount, v_coin_amount, oracle_price)?;
                    market_state.add_v_pc(balanced_v_pc)?;
                    market_state.add_v_coin(balanced_v_coin)?;
                    market_state.total_collateral = market_state
                        .total_collateral
                        .checked_sub(p.collateral)
                        .unwrap();
                    market_state.sub_open_interest(p.v_coin_amount, p.v_pc_amount, p.side)?;
                }
                remove_position(
                    &mut accounts.user_account.data.borrow_mut(),
                    &mut user_account_header,
                    position_index,
                )?;

                if remaining_debt <= 0 {
                    break;
                }
            }
        }
        market_state.total_user_balances = market_state
            .total_user_balances
            .checked_sub(user_account_header.balance)
            .unwrap();
        msg!(
            "Extracting {:?} from user account for funding",
            user_account_header.balance
        );
        user_account_header.balance = 0;
    } else {
        user_account_header.balance = (user_account_header.balance as i64 - balanced_debt) as u64;
        market_state.total_user_balances =
            (market_state.total_user_balances as i64 - balanced_debt) as u64;

        msg!(
            "Extracting {:?} from user account for funding",
            balanced_debt
        );
    }

    user_account_header.last_funding_offset = market_state
        .funding_history_offset
        .wrapping_sub(last_funding_offset_total);

    user_account_header.pack_into_slice(&mut accounts.user_account.data.borrow_mut());
    instance.update(&book, &mut page_infos);
    write_instance_and_memory(
        &mut accounts.instance.data.borrow_mut(),
        &page_infos,
        &instance,
    )?;
    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());

    Ok(())
}
