use crate::{
    error::PerpError,
    positions_book::{
        memory::{Memory, Pointer, SLOT_SIZE, TAG_SIZE},
        page::{Page, SlotType},
        tree_nodes::{InnerNodeSchema, LeafNodeSchema},
    },
    processor::{
        ALLOCATION_FEE, FEES_HIGH_LEVERAGE, FEES_LOW_LEVERAGE, FEE_TIERS, FIDA_MINT,
        HIGH_LEVERAGE_MIN, MARGIN_RATIO,
    },
    state::{
        instance::parse_instance,
        market::{get_instance_address, MarketDataPoint, MarketState},
        Fees, PositionType,
    },
};
use num_traits::FromPrimitive;
use pyth_client::{cast, Price, Product, PROD_HDR_SIZE};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::Pack,
    pubkey::Pubkey,
};
use spl_token::state::Account;
use std::{cell::RefCell, convert::TryInto, rc::Rc, slice::Iter};

// Safety verification functions
pub fn check_account_key(account: &AccountInfo, key: &Pubkey) -> ProgramResult {
    if account.key != key {
        return Err(ProgramError::InvalidArgument);
    }
    Ok(())
}

pub fn check_account_owner(account: &AccountInfo, owner: &Pubkey) -> ProgramResult {
    if account.owner != owner {
        return Err(ProgramError::InvalidArgument);
    }
    Ok(())
}

pub fn check_signer(account: &AccountInfo) -> ProgramResult {
    if !(account.is_signer) {
        return Err(ProgramError::MissingRequiredSignature);
    }
    Ok(())
}

////////////////////////////////////////
// Numerical computations

pub fn compute_margin(
    collateral: u64,
    v_coin_amount: u64,
    v_pc_amount: u64,
    oracle_price: u64,
    position_type: PositionType,
) -> u64 {
    let denominator = (v_coin_amount * oracle_price) as u128;
    let numerator = match position_type {
        PositionType::Long => {
            ((collateral + v_coin_amount * oracle_price - v_pc_amount) as u128) << 64
        }
        PositionType::Short => {
            ((collateral - v_coin_amount * oracle_price + v_pc_amount) as u128) << 64
        }
    };

    ((numerator / denominator) >> 64) as u64
}

pub fn compute_fee_tier(accounts_iter: &mut Iter<AccountInfo>) -> Result<usize, ProgramError> {
    let mut fee_tier = 0;
    if accounts_iter.len() > 1 {
        // The discount account and owner were given, calculate fee tier
        let discount_account = next_account_info(accounts_iter)?;
        let discount_owner = next_account_info(accounts_iter)?;
        let discount_data = Account::unpack(&discount_account.data.borrow()).map_err(|e| {
            msg!("The discount account is not initialized.");
            e
        })?;
        if discount_data.mint.to_string() != FIDA_MINT {
            msg!("The discount account should be a fida token account");
            return Err(ProgramError::InvalidArgument);
        }
        if &discount_data.owner != discount_owner.key {
            msg!("The discount owner should own the discount account");
            return Err(ProgramError::InvalidArgument);
        }
        if !discount_owner.is_signer {
            msg!("The discount account owner should be a signer");
            return Err(ProgramError::MissingRequiredSignature);
        }
        let discount_balance = discount_data.amount;
        fee_tier = match FEE_TIERS
            .iter()
            .position(|&t| discount_balance < (t as u64))
        {
            Some(i) => i,
            None => FEE_TIERS.len(),
        };
    }
    Ok(fee_tier)
}

pub fn compute_fees(
    fee_tier: usize,
    size: u64,
    leverage: u64, // FP 32
) -> Result<Fees, ProgramError> {
    // Compute the fees
    let fee_tiers = match leverage < HIGH_LEVERAGE_MIN {
        true => FEES_LOW_LEVERAGE,
        false => FEES_HIGH_LEVERAGE,
    };
    // We add one to round up the results
    let fixed_fee = ((size as u128) * (fee_tiers[fee_tier] as u128) / 10_000) + 1;
    let refundable_fees = ALLOCATION_FEE;
    let total_fees = (fixed_fee as u64) + ALLOCATION_FEE;

    let fees = Fees {
        total: total_fees as i64,
        refundable: refundable_fees,
        fixed: fixed_fee as u64,
    };
    msg!("Fees : {:?}", fees);

    Ok(fees)
}

pub fn compute_liquidation_index(
    // Returns the liquidation index as fixed point 32
    collateral: u64,
    v_coin_amount: u64,
    v_pc_amount: u64,
    position_type: PositionType,
    k: u128,
) -> u64 {
    let f = match position_type {
        PositionType::Long => {
            if v_pc_amount <= collateral {
                return 0;
            }
            (((v_pc_amount - collateral) as u128) << 64) / ((1u128 << 64) - (MARGIN_RATIO as u128))
        }
        PositionType::Short => {
            (((v_pc_amount + collateral) as u128) << 64) / ((1u128 << 64) + (MARGIN_RATIO as u128))
        }
    };
    // FP32 calculation
    let g = (1 << 32) + ((k << 34) / f / (v_coin_amount as u128));
    let mut r = spl_math::approximations::sqrt(g).unwrap(); // Becomes FP16
    r = match position_type {
        PositionType::Long => r.checked_add(1 << 16).unwrap(),
        PositionType::Short => r.checked_sub(1 << 16).unwrap(),
    };
    let r2 = r.checked_pow(2).unwrap(); // Back to FP32

    // msg!("f : {:?}", f);
    // msg!("r2 : {:?}", r2);
    // msg!("k : {:?}", k);
    ((f.checked_pow(2).unwrap().checked_mul(r2).unwrap() / k) >> 2) as u64
}

pub fn compute_liquidation_index_old(
    // Returns the liquidation index as fixed point 32
    collateral: u64,
    v_coin_amount: u64,
    v_pc_amount: u64,
    position_type: PositionType,
) -> u64 {
    let (numerator, denominator) = match position_type {
        PositionType::Short => (
            ((v_pc_amount + collateral) as u128),
            ((v_coin_amount as u128) * (((MARGIN_RATIO) as u128 + (1 << 64)) as u128)) >> 64, // Optimized
        ),
        PositionType::Long => (
            (v_pc_amount.saturating_sub(collateral) as u128),
            ((v_coin_amount as u128) * (((1 + !MARGIN_RATIO) as u128) as u128)) >> 64, // Optimized
        ),
    };
    ((numerator << 32)
        .checked_div(denominator)
        .ok_or(PerpError::AmountTooLow)
        .unwrap()) as u64
}

pub fn compute_liquidation_index_inverse(
    collateral: u64,
    v_coin_amount: u64,
    liquidation_index: u64,
    position_type: PositionType,
) -> u64 {
    match position_type {
        PositionType::Short => {
            let a =
                ((v_coin_amount as u128) * (((MARGIN_RATIO) as u128 + (1 << 64)) as u128)) >> 64;
            ((((liquidation_index as u128) * a) >> 32) - (collateral as u128)) as u64
            // Optimized
        }
        PositionType::Long => {
            let a = ((v_coin_amount as u128) * (((1 + !MARGIN_RATIO) as u128) as u128)) >> 64;
            // Optimized
            ((((liquidation_index as u128) * a) >> 32) + (collateral as u128)) as u64
        }
    }
}

pub fn compute_bias(delta: i64, v_coin_amount: u64, v_pc_amount: u64, oracle_price: u64) -> i64 {
    let num = (delta + (v_coin_amount as i64)) as u128;
    let num2 = num.pow(2);
    let denom = (v_coin_amount as u128) * (v_pc_amount as u128);
    let r = (num2 << 32) / denom;
    ((r * oracle_price as u128) >> 32) as i64 - (1i64 << 32)
}

pub fn compute_payout(
    v_pc_amount: u64,
    position_v_pc_amount: u64,
    collateral: u64,
    side: &PositionType,
) -> i64 {
    match side {
        PositionType::Long => (v_pc_amount as i64)
            .checked_sub(position_v_pc_amount as i64)
            .and_then(|f| f.checked_add(collateral as i64))
            .unwrap(),
        PositionType::Short => (-(v_pc_amount as i64))
            .checked_add(position_v_pc_amount as i64)
            .and_then(|f| f.checked_add(collateral as i64))
            .unwrap(),
    }
}

////////////////////////////////////////
// Oracle utils

pub fn get_oracle_price(
    account_data: &[u8],
    coin_decimals: u8,
    quote_decimals: u8,
) -> Result<u64, ProgramError> {
    #[cfg(feature = "mock-oracle")]
    {
        // Mock testing oracle
        if account_data.len() == 8 {
            return Ok(u64::from_le_bytes(account_data[0..8].try_into().unwrap()));
        }
    };
    // Pyth Oracle
    let price_account = cast::<Price>(account_data);
    let price = ((price_account.agg.price as u128) << 32)
        / 10u128.pow(price_account.expo.abs().try_into().unwrap());

    let corrected_price =
        (price * 10u128.pow(quote_decimals as u32)) / 10u128.pow(coin_decimals as u32);
    msg!("Oracle value: {:?}", corrected_price >> 32);

    Ok(corrected_price as u64)
}

pub fn get_pyth_market_symbol(pyth_product: &Product) -> Result<String, ProgramError> {
    let mut psz = pyth_product.size as usize - PROD_HDR_SIZE;
    let mut pit = (&pyth_product.attr[..]).iter();

    let mut key;
    let mut val;
    while psz > 0 {
        key = get_attr_bytes(&mut pit);
        val = get_attr_bytes(&mut pit);
        if String::from_utf8(key.to_owned()).unwrap() == "symbol" {
            return Ok(String::from_utf8(val).unwrap());
        }
        psz -= 2 + key.len() + val.len();
    }
    msg!("The provided pyth product account has no attribute 'symbol'.");
    Err(ProgramError::InvalidArgument)
}

pub fn get_attr_bytes<'a, T>(ite: &mut T) -> Vec<u8>
where
    T: Iterator<Item = &'a u8>,
{
    let mut len = *ite.next().unwrap() as usize;
    let mut val = Vec::with_capacity(len);
    while len > 0 {
        val.push(*ite.next().unwrap());
        len -= 1;
    }
    val
}

pub fn get_attr_str<'a, T>(ite: &mut T) -> String
where
    T: Iterator<Item = &'a u8>,
{
    let mut len = *ite.next().unwrap() as usize;
    let mut val = String::with_capacity(len);
    while len > 0 {
        val.push(*ite.next().unwrap() as char);
        len -= 1;
    }
    val
}

////////////////////////////////////////
// Misc Utils unused in the program on-chain

#[cfg(not(target_arch = "bpf"))]
pub fn print_tree(pt: Pointer, mem: &Memory, offset: u8) {
    print_node(pt, mem, offset);
    if SlotType::InnerNode == FromPrimitive::from_u8(mem.read_byte(pt, 0).unwrap()).unwrap() {
        let left_pt = mem
            .read_u32_le(pt, InnerNodeSchema::LeftPointer as usize)
            .unwrap();
        let right_pt = mem
            .read_u32_le(pt, InnerNodeSchema::RightPointer as usize)
            .unwrap();

        print_tree(left_pt, mem, offset + 1);
        print_tree(right_pt, mem, offset + 1);
    }
}

#[cfg(not(target_arch = "bpf"))]
pub fn print_node(pt: Pointer, mem: &Memory, offset: u8) {
    let indent = vec![" "; 8 * offset as usize].join("");
    match FromPrimitive::from_u8(mem.read_byte(pt, 0).unwrap()).unwrap() {
        SlotType::InnerNode => {
            let critbit = mem
                .read_byte(pt, InnerNodeSchema::Critbit as usize)
                .unwrap();
            let liq_index_min = mem
                .read_u64_le(pt, InnerNodeSchema::LiquidationIndexMin as usize)
                .unwrap();
            let liq_index_max = liq_index_min | ((2u64 << critbit) - 1);

            // Printing
            println!("Tree: {}InnerNode: ", indent);
            println!("Tree: {}  Critbit: {:#04x}", indent, 1u64 << critbit);
            println!(
                "Tree: {}  LiquidationIndexMin: {:#4x}",
                indent, liq_index_min
            );
            println!(
                "Tree: {}  LiquidationIndexMax: {:#4x}",
                indent, liq_index_max
            );
            println!(
                "Tree: {}  LeftPointer: {:?}",
                indent,
                mem.read_u32_le(pt, InnerNodeSchema::LeftPointer as usize)
                    .unwrap()
            );
            println!(
                "Tree: {}  RightPointer: {:?}",
                indent,
                mem.read_u32_le(pt, InnerNodeSchema::RightPointer as usize)
                    .unwrap()
            );
            println!(
                "Tree: {}  Collateral: {:?}",
                indent,
                mem.read_u64_le(pt, InnerNodeSchema::Collateral as usize)
                    .unwrap()
            );
            println!(
                "Tree: {}  VCoin: {:?}",
                indent,
                mem.read_u64_le(pt, InnerNodeSchema::VCoin as usize)
                    .unwrap()
            );
            println!(
                "Tree: {}  VPc: {:?}",
                indent,
                mem.read_u64_le(pt, InnerNodeSchema::VPc as usize).unwrap()
            );
            println!(
                "Tree: {}  CalculationFlag: {:?}",
                indent,
                mem.read_byte(pt, InnerNodeSchema::CalculationFlag as usize)
                    .unwrap()
            );

            // Logging, will not be effective if not set up
            log::info!("Tree: {}InnerNode: ", indent);
            log::info!("Tree: {}  Critbit: {:#04x}", indent, 1u64 << critbit);
            log::info!(
                "Tree: {}  LiquidationIndexMin: {:#4x}",
                indent,
                liq_index_min
            );
            log::info!(
                "Tree: {}  LiquidationIndexMax: {:#4x}",
                indent,
                liq_index_max
            );
            log::info!(
                "Tree: {}  LeftPointer: {:?}",
                indent,
                mem.read_u32_le(pt, InnerNodeSchema::LeftPointer as usize)
                    .unwrap()
            );
            log::info!(
                "Tree: {}  RightPointer: {:?}",
                indent,
                mem.read_u32_le(pt, InnerNodeSchema::RightPointer as usize)
                    .unwrap()
            );
            log::info!(
                "Tree: {}  Collateral: {:?}",
                indent,
                mem.read_u64_le(pt, InnerNodeSchema::Collateral as usize)
                    .unwrap()
            );
            log::info!(
                "Tree: {}  VCoin: {:?}",
                indent,
                mem.read_u64_le(pt, InnerNodeSchema::VCoin as usize)
                    .unwrap()
            );
            log::info!(
                "Tree: {}  VPc: {:?}",
                indent,
                mem.read_u64_le(pt, InnerNodeSchema::VPc as usize).unwrap()
            );
            log::info!(
                "Tree: {}  CalculationFlag: {:?}",
                indent,
                mem.read_byte(pt, InnerNodeSchema::CalculationFlag as usize)
                    .unwrap()
            );
        }
        SlotType::LeafNode => {
            println!("Tree: {}LeafNode: ", indent);
            println!(
                "Tree: {}  LiquidationIndex: {:#04x}",
                indent,
                mem.read_u64_le(pt, LeafNodeSchema::LiquidationIndex as usize)
                    .unwrap()
            );
            println!(
                "Tree: {}  SlotNumber: {:?}",
                indent,
                mem.read_u64_le(pt, LeafNodeSchema::SlotNumber as usize)
                    .unwrap()
            );
            println!(
                "Tree: {}  Collateral: {:?}",
                indent,
                mem.read_u64_le(pt, LeafNodeSchema::Collateral as usize)
                    .unwrap()
            );
            println!(
                "Tree: {}  VCoin: {:?}",
                indent,
                mem.read_u64_le(pt, LeafNodeSchema::VCoin as usize).unwrap()
            );
            println!(
                "Tree: {}  VPc: {:?}",
                indent,
                mem.read_u64_le(pt, LeafNodeSchema::VPc as usize).unwrap()
            );

            // Logging
            log::info!("Tree: {}LeafNode: ", indent);
            log::info!(
                "Tree: {}  LiquidationIndex: {:#04x}",
                indent,
                mem.read_u64_le(pt, LeafNodeSchema::LiquidationIndex as usize)
                    .unwrap()
            );
            log::info!(
                "Tree: {}  SlotNumber: {:?}",
                indent,
                mem.read_u64_le(pt, LeafNodeSchema::SlotNumber as usize)
                    .unwrap()
            );
            log::info!(
                "Tree: {}  Collateral: {:?}",
                indent,
                mem.read_u64_le(pt, LeafNodeSchema::Collateral as usize)
                    .unwrap()
            );
            log::info!(
                "Tree: {}  VCoin: {:?}",
                indent,
                mem.read_u64_le(pt, LeafNodeSchema::VCoin as usize).unwrap()
            );
            log::info!(
                "Tree: {}  VPc: {:?}",
                indent,
                mem.read_u64_le(pt, LeafNodeSchema::VPc as usize).unwrap()
            );
        }
        _ => unreachable!(),
    }
}

#[cfg(not(target_arch = "bpf"))]
pub fn get_market_data(
    market_key: Pubkey,
    get_account_data: &dyn Fn(&Pubkey) -> Vec<u8>,
) -> Result<MarketDataPoint, ProgramError> {
    let market_account_data = get_account_data(&market_key);
    let market_state = MarketState::unpack_from_slice(&market_account_data)?;

    let market_vault_balance = spl_token::state::Account::unpack(&get_account_data(&Pubkey::new(
        &market_state.vault_address,
    )))
    .unwrap()
    .amount;

    let mut instances = Vec::with_capacity(market_state.number_of_instances as usize);
    for i in 0..market_state.number_of_instances {
        let instance_address = get_instance_address(&market_account_data, i).unwrap();
        let instance_account_data = get_account_data(&instance_address);
        instances.push(parse_instance(&instance_account_data).unwrap());
    }

    let mut gc_list_lengths = Vec::with_capacity(market_state.number_of_instances as usize);
    let mut page_full_ratios = Vec::with_capacity(market_state.number_of_instances as usize);
    for (instance, page_infos) in &instances {
        let mut page_datas = page_infos
            .iter()
            .map(|p| {
                (
                    get_account_data(&Pubkey::new(&p.address)),
                    p.unitialized_memory_index,
                    p.free_slot_list_hd,
                )
            })
            .collect::<Vec<_>>();
        let mut pages = Vec::with_capacity(page_datas.len());
        let mut instance_page_full_ratios = vec![];
        for (page_data, u_mem_index, free_slot_list_hd) in &mut page_datas {
            let page = Page {
                page_size: ((page_data.len() - TAG_SIZE) / SLOT_SIZE) as u32,
                data: Rc::new(RefCell::new(page_data)),
                uninitialized_memory: u_mem_index.to_owned(),
                free_slot_list_hd: free_slot_list_hd.to_owned(),
            };
            let page_ratio = ((page.uninitialized_memory as f64)
                - (page.get_nb_free_slots().unwrap() as f64))
                / (page.page_size as f64);

            instance_page_full_ratios.push(page_ratio);
            pages.push(page);
        }
        page_full_ratios.push(instance_page_full_ratios);
        let mem = Memory::new(pages, instance.garbage_pointer);
        gc_list_lengths.push(mem.get_gc_list_len().unwrap());
    }
    let insurance_fund = market_state.get_insurance_fund(market_vault_balance);

    // Get the current index price
    let oracle_account_data = get_account_data(&Pubkey::new(&market_state.oracle_address));
    let oracle_price = (get_oracle_price(
        &oracle_account_data,
        market_state.coin_decimals,
        market_state.quote_decimals,
    )
    .unwrap() as f64)
        / (2u64.pow(32) as f64);

    println!("Market vault balance: {}", market_vault_balance);

    let market_data = MarketDataPoint {
        total_collateral: market_state.total_collateral,
        total_user_balances: market_state.total_user_balances,
        total_fee_balance: market_state.total_fee_balance,
        rebalancing_funds: market_state.rebalancing_funds,
        rebalanced_v_coin: market_state.rebalanced_v_coin,
        v_coin_amount: market_state.v_coin_amount,
        v_pc_amount: market_state.v_pc_amount,
        open_shorts_v_coin: market_state.open_shorts_v_coin,
        open_longs_v_coin: market_state.open_longs_v_coin,
        last_funding_timestamp: market_state.last_funding_timestamp,
        last_recording_timestamp: market_state.last_recording_timestamp,
        funding_samples_count: market_state.funding_samples_count,
        funding_samples_sum: market_state.funding_samples_sum,
        funding_history_offset: market_state.funding_history_offset,
        funding_history: market_state.funding_history,
        funding_balancing_factors: market_state.funding_balancing_factors,
        number_of_instances: market_state.number_of_instances,
        insurance_fund,
        market_price: (market_state.v_pc_amount as f64) / (market_state.v_coin_amount as f64),
        oracle_price,
        equilibrium_price: ((market_state.v_pc_amount as f64)
            * (market_state.v_coin_amount as f64))
            / (((market_state.v_coin_amount + market_state.open_longs_v_coin
                - market_state.open_shorts_v_coin) as u128)
                .pow(2) as f64),
        gc_list_lengths,
        page_full_ratios,
        longs_depths: vec![],
        shorts_depths: vec![],
    };
    Ok(market_data)
}
pub fn get_tree_depth(pt: Option<Pointer>, mem: &Memory) -> usize {
    let mut depth = 0;
    let mut stack = Vec::with_capacity(64);
    if pt.is_none() {
        return 0;
    }
    stack.push(pt.unwrap());
    while !stack.is_empty() {
        let current = stack.pop().unwrap();
        match FromPrimitive::from_u8(mem.read_byte(current, 0).unwrap()).unwrap() {
            SlotType::InnerNode => {
                let left_pt = mem
                    .read_u32_le(current, InnerNodeSchema::LeftPointer as usize)
                    .unwrap();
                let right_pt = mem
                    .read_u32_le(current, InnerNodeSchema::RightPointer as usize)
                    .unwrap();
                stack.push(right_pt);
                stack.push(left_pt);
            }
            SlotType::LeafNode => {
                depth = std::cmp::max(depth, stack.len() + 1);
            }
            _ => unreachable!(),
        }
    }
    depth
}

#[cfg(test)]
mod tests {
    // use super::*;

    #[test]
    pub fn test_liq_index_inverse() {
        // let collateral = 1_000_000;
        // let v_coin_amount = 7_000_000;
        // let v_pc_ammount = 10_000_000;
        // let liquidation_index =
        //     compute_liquidation_index(collateral, v_coin_amount, v_pc_ammount, PositionType::Long);
        // let predicted_v_pc_amount = compute_liquidation_index_inverse(
        //     collateral,
        //     v_coin_amount,
        //     liquidation_index,
        //     PositionType::Long,
        // );

        // assert!(((predicted_v_pc_amount as i64) - (v_pc_ammount as i64)).abs() < 10);
        // let liquidation_index =
        //     compute_liquidation_index(collateral, v_coin_amount, v_pc_ammount, PositionType::Short);
        // let predicted_v_pc_amount = compute_liquidation_index_inverse(
        //     collateral,
        //     v_coin_amount,
        //     liquidation_index,
        //     PositionType::Short,
        // );

        // assert!(((predicted_v_pc_amount as i64) - (v_pc_ammount as i64)).abs() < 10);
    }
}
