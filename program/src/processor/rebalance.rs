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
    processor::MAX_LEVERAGE,
    state::PositionType,
    state::{
        instance::{parse_instance, write_instance_and_memory},
        market::{get_instance_address, MarketState},
        user_account::{write_position, OpenPosition, UserAccountState},
    },
    utils::{
        check_account_key, check_account_owner, check_signer, compute_fee_tier, compute_fees,
        compute_liquidation_index,
    },
};

use super::FIDA_BNB;

pub struct Accounts<'a, 'b: 'a> {
    spl_token_program: &'a AccountInfo<'b>,
    clock_sysvar: &'a AccountInfo<'b>,
    market: &'a AccountInfo<'b>,
    instance: &'a AccountInfo<'b>,
    market_signer: &'a AccountInfo<'b>,
    market_vault: &'a AccountInfo<'b>,
    bnb_bonfida: &'a AccountInfo<'b>,
    user_account_owner: &'a AccountInfo<'b>,
    user_account: &'a AccountInfo<'b>,
    remaining: Iter<'a, AccountInfo<'b>>,
    admin_account: &'a AccountInfo<'b>,
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
        let user_account_owner = next_account_info(&mut accounts_iter)?;
        let user_account = next_account_info(&mut accounts_iter)?;
        let admin_account = next_account_info(&mut accounts_iter)?;
        check_account_key(clock_sysvar, &solana_program::sysvar::clock::ID).unwrap();
        check_account_owner(user_account, program_id).unwrap();
        check_account_owner(market, program_id).unwrap();
        check_account_key(bnb_bonfida, &Pubkey::from_str(&FIDA_BNB).unwrap()).unwrap();

        check_signer(user_account_owner).unwrap();
        check_signer(admin_account).unwrap();

        Ok(Self {
            spl_token_program,
            clock_sysvar,
            market,
            instance,
            market_signer,
            market_vault,
            bnb_bonfida,
            user_account_owner,
            user_account,
            remaining: accounts_iter,
            admin_account,
        })
    }
}

pub fn process_rebalance(
    program_id: &Pubkey,
    accounts: &[AccountInfo<'_>],
    instance_index: u8,
    collateral: u64,
) -> ProgramResult {
    let mut accounts = Accounts::parse(program_id, accounts)?;

    // Parsing
    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    let signed_v_coin_amount =
        (market_state.open_longs_v_coin as i64) - (market_state.open_shorts_v_coin as i64);

    let signed_v_pc_amount = market_state.compute_add_v_pc(signed_v_coin_amount)?;

    let leverage = ((signed_v_pc_amount.abs() as u128) << 32) / (collateral as u128);
    if leverage as u64 > MAX_LEVERAGE {
        msg!("Attempting to rebalance with excessive leverage");
        return Err(PerpError::MarginTooLow.into());
    }

    let side = match signed_v_coin_amount.signum() {
        -1 => PositionType::Long,
        1 => PositionType::Short,
        0 => {
            msg!("The market is already balanced!");
            return Ok(());
        }
        _ => unreachable!(),
    };

    msg!(
        "Market_state before: v_coin {:?} - v_pc {:?}",
        market_state.v_coin_amount,
        market_state.v_pc_amount
    );

    let mut user_account_header =
        UserAccountState::unpack_from_slice(&accounts.user_account.data.borrow())?;

    let instance_address =
        get_instance_address(&accounts.market.data.borrow(), instance_index as u32)?;
    if &instance_address != accounts.instance.key {
        msg!("Invalid instance account or instance index provided");
        return Err(ProgramError::InvalidArgument);
    }

    if &Pubkey::new(&market_state.admin_address) != accounts.admin_account.key {
        msg!("Incorrect admin account");
        return Err(ProgramError::InvalidArgument);
    }

    let (mut instance, mut page_infos) = parse_instance(&accounts.instance.data.borrow())?;
    let memory = parse_memory(&instance, &page_infos, &mut accounts.remaining)?;
    let mut book = PositionsBook::new(instance.shorts_pointer, instance.longs_pointer, memory);

    //Verifications
    if accounts.user_account_owner.key != &Pubkey::new(&user_account_header.owner) {
        msg!("The user account owner doesn't match");
        return Err(ProgramError::InvalidArgument);
    }
    if &Pubkey::new(&user_account_header.market) != accounts.market.key {
        msg!("The user account market doesn't match the given market account");
        return Err(ProgramError::InvalidArgument);
    }
    if user_account_header.last_funding_offset != market_state.funding_history_offset {
        if user_account_header.number_of_open_positions == 0 {
            user_account_header.last_funding_offset = market_state.funding_history_offset;
        } else {
            msg!("Funding must be processed for this account.");
            return Err(PerpError::PendingFunding.into());
        }
    }

    // Fees (leverage is set to 0 to minimize fees)
    let fee_tier = compute_fee_tier(&mut accounts.remaining)?;
    let mut fees = compute_fees(fee_tier, 0, 0)?;
    let referrer_account_opt = next_account_info(&mut accounts.remaining).ok();
    if (user_account_header.balance as i64) < collateral as i64 + fees.total {
        msg!("The user budget is not sufficient");
        return Err(PerpError::NoMoreFunds.into());
    }
    user_account_header.balance = ((user_account_header.balance as i64) - fees.total) as u64;

    market_state.apply_fees(&fees, false, true)?;

    // Transfer collateral
    market_state.total_user_balances -= collateral;
    market_state.total_collateral += collateral;
    user_account_header.balance -= collateral;

    market_state.add_v_pc(signed_v_pc_amount)?;
    market_state.add_v_coin(signed_v_coin_amount)?;

    let v_coin_amount = signed_v_coin_amount.abs() as u64;
    let v_pc_amount = signed_v_pc_amount.abs() as u64;
    market_state.add_open_interest(v_coin_amount, v_pc_amount, side)?;

    msg!("Add_v_pc_amount: {:?}", signed_v_pc_amount);
    msg!("Add_v_coin_amount: {:?}", signed_v_coin_amount);

    if v_coin_amount == 0 {
        msg!("The given order size is not sufficient!");
        return Err(PerpError::AmountTooLow.into());
    }

    let current_slot = Clock::from_account_info(accounts.clock_sysvar)?.slot;

    let liquidation_index = compute_liquidation_index(
        collateral,
        v_coin_amount,
        v_pc_amount,
        side,
        market_state.get_k(),
    );
    msg!(
        "Liquidation Index for this position: {:?}",
        liquidation_index
    );

    msg!(
        "Mark price for this transaction (FP32): {:?}, with size: {:?} and side {:?}",
        ((v_pc_amount as u128) << 32) / (v_coin_amount as u128),
        v_coin_amount,
        side
    );

    let insertion_leaf = book.open_position(
        liquidation_index,
        collateral,
        v_coin_amount,
        v_pc_amount,
        side,
        current_slot,
    )?;

    let position = OpenPosition {
        last_funding_offset: market_state.funding_history_offset,
        instance_index,
        side,
        liquidation_index,
        collateral,
        slot_number: insertion_leaf.get_slot_number(&book.memory)?,
        v_coin_amount,
        v_pc_amount,
    };
    msg!(
        "Transaction info: v_coin_amount {:?}, v_pc_amount {:?}",
        v_coin_amount,
        v_pc_amount
    );
    write_position(
        &mut accounts.user_account.data.borrow_mut(),
        user_account_header.number_of_open_positions as u16,
        &mut user_account_header,
        &position,
        false,
    )?;

    instance.update(&book, &mut page_infos);

    msg!(
        "Market_state: v_coin {:?} - v_pc {:?}",
        market_state.v_coin_amount,
        market_state.v_pc_amount
    );

    write_instance_and_memory(
        &mut accounts.instance.data.borrow_mut(),
        &page_infos,
        &instance,
    )?;
    user_account_header.pack_into_slice(&mut accounts.user_account.data.borrow_mut());

    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());

    market_state.transfer_fees(
        &mut fees,
        accounts.spl_token_program,
        accounts.market,
        accounts.market_vault,
        accounts.market_signer,
        accounts.bnb_bonfida,
        referrer_account_opt,
    )?;

    Ok(())
}
