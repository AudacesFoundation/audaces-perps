use std::{slice::Iter, str::FromStr};

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
    positions_book::{memory::parse_memory, positions_book_tree::PositionsBook},
    processor::{FEE_REBALANCING_FUND, LIQUIDATION_LABEL},
    state::{
        instance::{parse_instance, write_instance_and_memory},
        market::{get_instance_address, MarketState},
    },
    state::{Fees, PositionType},
    utils::{check_account_key, check_account_owner, get_oracle_price},
};

pub struct Accounts<'a, 'b: 'a> {
    spl_token_program: &'a AccountInfo<'b>,
    market: &'a AccountInfo<'b>,
    instance: &'a AccountInfo<'b>,
    market_signer: &'a AccountInfo<'b>,
    bnb_bonfida: &'a AccountInfo<'b>,
    market_vault: &'a AccountInfo<'b>,
    oracle: &'a AccountInfo<'b>,
    target: &'a AccountInfo<'b>,
    remaining: Iter<'a, AccountInfo<'b>>,
}

impl<'a, 'b: 'a> Accounts<'a, 'b> {
    pub fn parse(
        program_id: &Pubkey,
        accounts: &'a [AccountInfo<'b>],
    ) -> Result<Self, ProgramError> {
        let mut accounts_iter = accounts.iter();

        let spl_token_program = next_account_info(&mut accounts_iter)?;
        let market = next_account_info(&mut accounts_iter)?;
        let instance = next_account_info(&mut accounts_iter)?;
        let market_signer = next_account_info(&mut accounts_iter)?;
        let bnb_bonfida = next_account_info(&mut accounts_iter)?;
        let market_vault = next_account_info(&mut accounts_iter)?;
        let oracle = next_account_info(&mut accounts_iter)?;
        let target = next_account_info(&mut accounts_iter)?;
        let label = next_account_info(&mut accounts_iter)?;

        check_account_key(spl_token_program, &spl_token::id()).unwrap();
        check_account_key(label, &Pubkey::from_str(LIQUIDATION_LABEL).unwrap()).unwrap();
        check_account_owner(market, program_id).unwrap();

        Ok(Self {
            spl_token_program,
            market,
            instance,
            market_signer,
            bnb_bonfida,
            market_vault,
            oracle,
            target,
            remaining: accounts_iter,
        })
    }
}

pub fn process_liquidation(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instance_index: u8,
) -> ProgramResult {
    let mut accounts = Accounts::parse(program_id, accounts)?;

    // Parsing

    let mut market_state = MarketState::unpack_from_slice(&accounts.market.data.borrow())?;

    let instance_address =
        get_instance_address(&accounts.market.data.borrow(), instance_index as u32)?;
    if &instance_address != accounts.instance.key {
        msg!("Invalid instance account or instance index provided");
        return Err(ProgramError::InvalidArgument);
    }

    let (mut instance, mut page_infos) = parse_instance(&accounts.instance.data.borrow())?;
    let memory = parse_memory(&instance, &page_infos, &mut accounts.remaining)?;
    let mut book = PositionsBook::new(instance.shorts_pointer, instance.longs_pointer, memory);

    let liquidation_index = get_oracle_price(
        &accounts.oracle.data.borrow(),
        market_state.coin_decimals,
        market_state.quote_decimals,
    )?;

    msg!("Liquidation index: {:?}", liquidation_index);

    // Verifications
    if market_state.oracle_address != accounts.oracle.key.to_bytes() {
        msg!("Provided oracle account is incorrect.");
        return Err(ProgramError::InvalidArgument);
    }

    let collateral = book.get_collateral()?;
    let (longs_v_coin_before, shorts_v_coin_before) = book.get_v_coin()?;
    let (longs_v_pc_before, shorts_v_pc_before) = book.get_v_pc()?;

    book.liquidate(liquidation_index, PositionType::Short)?;
    book.liquidate(liquidation_index, PositionType::Long)?;

    let (longs_v_coin_after, shorts_v_coin_after) = book.get_v_coin()?;
    let (longs_v_pc_after, shorts_v_pc_after) = book.get_v_pc()?;
    let liquidated_collateral = collateral - book.get_collateral()?;
    let liquidated_longs = longs_v_coin_before - longs_v_coin_after;
    let liquidated_shorts = shorts_v_coin_before - shorts_v_coin_after;
    let liquidated_longs_v_pc = longs_v_pc_before - longs_v_pc_after;
    let liquidated_shorts_v_pc = shorts_v_pc_before - shorts_v_pc_after;

    if liquidated_collateral == 0 {
        msg!("No orders to liquidate.");
        return Err(PerpError::Nop.into());
    }

    market_state.total_collateral -= liquidated_collateral;
    market_state.sub_open_interest(liquidated_longs, liquidated_longs_v_pc, PositionType::Long)?;
    market_state.sub_open_interest(
        liquidated_shorts,
        liquidated_shorts_v_pc,
        PositionType::Short,
    )?;

    let total_v_coin_difference = (liquidated_longs as i64) - (liquidated_shorts as i64);

    let total_v_pc_difference = market_state.compute_add_v_pc(total_v_coin_difference)?;

    let (balanced_v_pc, balanced_v_coin) = market_state.balance_operation(
        total_v_pc_difference,
        total_v_coin_difference,
        liquidation_index,
    )?;
    market_state.add_v_pc(balanced_v_pc)?;
    market_state.add_v_coin(balanced_v_coin)?;

    let mut liq_payout =
        (liquidated_shorts_v_pc as i64) - (liquidated_longs_v_pc as i64) - total_v_pc_difference
            + (liquidated_collateral as i64);

    liq_payout = std::cmp::max(0, liq_payout);

    // Transfer the Reward using the fees structure
    let mut liq_payout_wrapped = Fees {
        total: liq_payout,
        refundable: 0,
        fixed: liq_payout as u64,
    };
    market_state.rebalancing_funds +=
        ((liq_payout_wrapped.fixed as u128) * (FEE_REBALANCING_FUND as u128) / 100) as u64 + 1;

    market_state.transfer_fees(
        &mut liq_payout_wrapped,
        accounts.spl_token_program,
        accounts.market,
        accounts.market_vault,
        accounts.market_signer,
        accounts.bnb_bonfida,
        Some(accounts.target),
    )?;

    instance.update(&book, &mut page_infos);
    write_instance_and_memory(
        &mut accounts.instance.data.borrow_mut(),
        &page_infos,
        &instance,
    )?;
    market_state.pack_into_slice(&mut accounts.market.data.borrow_mut());
    Ok(())
}
