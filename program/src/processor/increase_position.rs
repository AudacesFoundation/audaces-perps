use std::{slice::Iter, str::FromStr};

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
    processor::{MAX_LEVERAGE, MAX_POSITION_SIZE},
    state::{
        instance::{parse_instance, write_instance_and_memory},
        market::{get_instance_address, MarketState},
        user_account::{get_position, write_position},
    },
    state::{user_account::UserAccountState, PositionType},
    utils::{
        check_account_key, check_account_owner, check_signer, compute_fee_tier, compute_fees,
        compute_liquidation_index, get_oracle_price,
    },
};

use super::{FIDA_BNB, TRADE_LABEL};

pub struct Accounts<'a, 'b: 'a> {
    spl_token_program: &'a AccountInfo<'b>,
    clock_sysvar: &'a AccountInfo<'b>,
    market: &'a AccountInfo<'b>,
    market_signer: &'a AccountInfo<'b>,
    market_vault: &'a AccountInfo<'b>,
    bnb_bonfida: &'a AccountInfo<'b>,
    instance: &'a AccountInfo<'b>,
    user_account_owner: &'a AccountInfo<'b>,
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

        let spl_token_program = next_account_info(&mut accounts_iter)?;
        let clock_sysvar = next_account_info(&mut accounts_iter)?;
        let market = next_account_info(&mut accounts_iter)?;
        let market_signer = next_account_info(&mut accounts_iter)?;
        let market_vault = next_account_info(&mut accounts_iter)?;
        let bnb_bonfida = next_account_info(&mut accounts_iter)?;
        let instance = next_account_info(&mut accounts_iter)?;
        let user_account_owner = next_account_info(&mut accounts_iter)?;
        let user_account = next_account_info(&mut accounts_iter)?;
        let label = next_account_info(&mut accounts_iter)?;
        let oracle = next_account_info(&mut accounts_iter)?;

        check_account_key(label, &Pubkey::from_str(TRADE_LABEL).unwrap()).unwrap();
        check_account_key(spl_token_program, &spl_token::id()).unwrap();
        check_account_key(clock_sysvar, &solana_program::sysvar::clock::ID).unwrap();
        check_account_key(bnb_bonfida, &Pubkey::from_str(&FIDA_BNB).unwrap()).unwrap();

        check_signer(user_account_owner).unwrap();
        check_account_owner(user_account, program_id).unwrap();
        check_account_owner(market, program_id).unwrap();

        Ok(Self {
            spl_token_program,
            clock_sysvar,
            market,
            market_signer,
            market_vault,
            bnb_bonfida,
            instance,
            user_account_owner,
            user_account,
            oracle,
            remaining: accounts_iter,
        })
    }
}
#[allow(clippy::too_many_arguments)]
pub fn process_increase_position(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    instance_index: u8,
    leverage: u64, // 32 bit FP
    position_index: u16,
    add_collateral: u64,
    predicted_entry_price: u64,   // 32 bit FP
    maximum_slippage_margin: u64, // 32 bit FP
) -> ProgramResult {
    let mut accounts = Accounts::parse(program_id, accounts)?;

    // Parsing
    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    msg!(
        "Market_state before: v_coin {:?} - v_pc {:?}",
        market_state.v_coin_amount,
        market_state.v_pc_amount
    );
    market_state.slippage_protection(predicted_entry_price, maximum_slippage_margin)?;

    let mut user_account_header =
        UserAccountState::unpack_from_slice(&accounts.user_account.data.borrow())?;

    let instance_address =
        get_instance_address(&accounts.market.data.borrow(), instance_index as u32)?;
    if &instance_address != accounts.instance.key {
        msg!("Invalid instance account or instance index provided");
        return Err(ProgramError::InvalidArgument);
    }

    let (mut instance, mut page_infos) = parse_instance(&accounts.instance.data.borrow())?;
    let memory = parse_memory(&instance, &page_infos, &mut accounts.remaining)?;
    let mut book = PositionsBook::new(instance.shorts_pointer, instance.longs_pointer, memory);

    let mut open_position = get_position(
        &mut accounts.user_account.data.borrow_mut(),
        &user_account_header,
        position_index,
    )?;

    // Verifications
    if leverage > MAX_LEVERAGE {
        msg!(
            "New leverage cannot be higher than: {:?}. Found: {:?}",
            MAX_LEVERAGE >> 32,
            leverage >> 32
        );
        return Err(PerpError::MarginTooLow.into());
    }
    if *accounts.user_account_owner.key != Pubkey::new_from_array(user_account_header.owner) {
        msg!("The open position is not correctly configured");
        return Err(ProgramError::InvalidArgument);
    }
    if &Pubkey::new(&user_account_header.market) != accounts.market.key {
        msg!("The user account market doesn't match the given market account");
        return Err(ProgramError::InvalidArgument);
    }
    if user_account_header.balance < add_collateral {
        msg!("The user budget is not sufficient");
        return Err(PerpError::NoMoreFunds.into());
    }

    if user_account_header.last_funding_offset != market_state.funding_history_offset {
        msg!("Funding must be processed for this account.");
        return Err(PerpError::PendingFunding.into());
    }

    if market_state.oracle_address != accounts.oracle.key.to_bytes() {
        msg!("Provided oracle account is incorrect.");
        return Err(ProgramError::InvalidArgument);
    }

    user_account_header.balance -= add_collateral;
    market_state.total_collateral += add_collateral;
    market_state.total_user_balances -= add_collateral;

    // Calculations
    book.close_position(
        open_position.liquidation_index,
        open_position.collateral,
        open_position.v_coin_amount,
        open_position.v_pc_amount,
        open_position.side,
        open_position.slot_number,
    )?;

    let add_v_pc_amount = (((add_collateral as u128) * (leverage as u128)) >> 32) as u64;
    let add_v_pc_amount_signed = open_position.side.get_sign() * (add_v_pc_amount as i64);
    let add_v_coin_amount = market_state.compute_add_v_coin(add_v_pc_amount_signed)?;

    let new_collateral = add_collateral + open_position.collateral;
    let new_v_pc_amount = add_v_pc_amount + open_position.v_pc_amount;
    let new_v_coin_amount = (add_v_coin_amount.abs() as u64) + open_position.v_coin_amount;

    msg!(
        "Transaction info: v_coin_amount {:?}, v_pc_amount {:?}",
        add_v_coin_amount.abs(),
        add_v_pc_amount
    );

    if add_v_pc_amount >= market_state.v_pc_amount && open_position.side == PositionType::Long {
        msg!("The given order size is too large!");
        return Err(PerpError::AmountTooLarge.into());
    }
    if new_v_coin_amount >= MAX_POSITION_SIZE {
        msg!(
            "The given order size is too large! The maximum size is: {:?}",
            MAX_POSITION_SIZE
        );
        return Err(PerpError::AmountTooLarge.into());
    }

    msg!("Add_v_pc_amount: {:?}", add_v_pc_amount_signed);
    msg!("Add_v_coin_amount: {:?}", add_v_coin_amount);

    msg!(
        "Mark price for this transaction (FP32): {:?}, with size: {:?} and side {:?}",
        ((add_v_pc_amount as u128) << 32)
            .checked_div(add_v_coin_amount.abs() as u128)
            .unwrap_or(0),
        add_v_coin_amount.abs(),
        open_position.side
    );

    let new_liquidation_index = compute_liquidation_index(
        new_collateral,
        new_v_coin_amount,
        new_v_pc_amount,
        open_position.side,
        market_state.get_k(),
    );

    println!(
        "Liquidation index for this position: {:?}",
        new_liquidation_index
    );
    let current_slot = Clock::from_account_info(accounts.clock_sysvar)?.slot;
    let insertion_leaf = book.open_position(
        new_liquidation_index,
        new_collateral,
        new_v_coin_amount,
        new_v_pc_amount,
        open_position.side,
        current_slot,
    )?;

    let oracle_price = get_oracle_price(
        &accounts.oracle.data.borrow(),
        market_state.coin_decimals,
        market_state.quote_decimals,
    )?;

    let (balanced_v_pc_amount, balanced_v_coin_amount) =
        market_state.balance_operation(add_v_pc_amount_signed, add_v_coin_amount, oracle_price)?;

    // Update the market state
    market_state.add_v_pc(balanced_v_pc_amount)?;
    market_state.add_v_coin(balanced_v_coin_amount)?;
    market_state.add_open_interest(
        add_v_coin_amount.abs() as u64,
        add_v_pc_amount,
        open_position.side,
    )?;

    // Fees
    let fee_tier = compute_fee_tier(&mut accounts.remaining)?;
    let mut fees = compute_fees(fee_tier, add_v_pc_amount, leverage)?;

    let referrer_account_opt = next_account_info(&mut accounts.remaining).ok();
    market_state.transfer_fees(
        &mut fees,
        accounts.spl_token_program,
        accounts.market,
        accounts.market_vault,
        accounts.market_signer,
        accounts.bnb_bonfida,
        referrer_account_opt,
    )?;

    market_state.apply_fees(&fees, false, false)?;
    if user_account_header.balance < fees.fixed {
        msg!("The user does not have the funds or the payout to pay the fees");
        return Err(PerpError::NoMoreFunds.into());
    }
    user_account_header.balance -= fees.fixed;

    // Update the open positions account
    open_position.collateral = new_collateral;
    open_position.liquidation_index = new_liquidation_index;
    open_position.slot_number = insertion_leaf.get_slot_number(&book.memory)?;
    open_position.v_coin_amount = new_v_coin_amount;
    open_position.v_pc_amount = new_v_pc_amount;

    msg!(
        "Market_state after: v_coin {:?} - v_pc {:?}",
        market_state.v_coin_amount,
        market_state.v_pc_amount
    );

    write_position(
        &mut accounts.user_account.data.borrow_mut(),
        position_index,
        &mut user_account_header,
        &open_position,
        true,
    )?;
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
