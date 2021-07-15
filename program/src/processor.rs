use borsh::BorshDeserialize;
use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, msg, program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::{
    instruction::PerpInstruction,
    processor::{
        add_budget::process_add_budget, add_instance::process_add_instance,
        add_page::process_add_page, change_k::process_change_k,
        close_account::process_close_account, close_position::process_close_position,
        create_market::process_create_market, funding::process_funding,
        funding_extraction::process_funding_extraction,
        garbage_collection::process_garbage_collection,
        increase_position::process_increase_position, liquidation::process_liquidation,
        open_position::process_open_position, rebalance::process_rebalance,
        transfer_position::process_transfer_position,
        transfer_user_account::process_transfer_user_account,
        update_oracle_account::process_update_oracle_account,
        withdraw_budget::process_withdraw_budget,
    },
};

////////////////////////////////////////////////////////////

pub(crate) const MARGIN_RATIO: u64 = ((1u128 << 64) / 20) as u64; // 64 fixed point
const FUNDING_PERIOD: u64 = 3_600; // in s
const FUNDING_NORMALIZATION: u64 = 86400 / FUNDING_PERIOD; // in s
const HISTORY_PERIOD: u64 = 300; // in s
pub const REBALANCING_MARGIN: i64 = 429496729; // FP32 the relative difference in longs vs shorts open interests which enables rebalancing.
pub const REBALANCING_LEVERAGE: u64 = 1;

pub const FIDA_MINT: &str = "EchesyfXePKdLtoiZSL8pBe8Myagyy8ZRqsACNCFGnvp"; // Mainnet
pub const FIDA_BNB: &str = "4qZA7RixzEgQ53cc6ittMeUtkaXgCnjZYkP8L1nxFD25"; // Bonfida buy and burn mainnet address
pub const PYTH_MAPPING_ACCOUNT: &str = "AHtgzX45WTKfkPG53L6WYhGEXwQkN1BVknET3sVsLL8J"; // Mainnet
pub const LIQUIDATION_LABEL: &str = "LiquidationRecord11111111111111111111111111";
pub const FUNDING_LABEL: &str = "FundingRecord1111111111111111111111111111111";
pub const TRADE_LABEL: &str = "TradeRecord11111111111111111111111111111111";
pub const FUNDING_EXTRACTION_LABEL: &str = "FundingExtraction111111111111111111111111111";

pub const MAX_LEVERAGE: u64 = 20 << 32;
pub const MAX_POSITION_SIZE: u64 = 500_000_000_000; // in USDC
pub const MAX_OPEN_POSITONS_PER_USER: u32 = 20;

// Fees
pub const FEE_BUY_BURN_BONFIDA: u64 = 30; // Percentage of total fee
pub const _FEE_INSURANCE_FUND: u64 = 30; // Percentage of total fee
pub const FEE_REBALANCING_FUND: u64 = 30; // Percentage of total fee
pub const FEE_REFERRER: u64 = 10; // Percentage of total fee, gets split up between Insurance fund and BNB if referrer is not specified
pub const ALLOCATION_FEE: u64 = 10_000; // Flat fee that balances out the rewards, refunded if closing without liquidation
pub const HIGH_LEVERAGE_MIN: u64 = 8 << 32;
// Amount of fees taken for opening or closing an order, expressed in bps of order size
pub const FEES_LOW_LEVERAGE: &[u64] = &[20, 15, 15, 10, 10, 10]; // Fees for low leverage orders for tiers [0, 1 ,2 ,3, 4, 5]
pub const FEES_HIGH_LEVERAGE: &[u64] = &[50, 40, 30, 25, 20, 15]; // Fees for high leverage orders for tiers [0, 1 ,2 ,3, 4, 5]
pub const FEE_TIERS: [u64; 5] = [
    500_000_000,
    1_000_000_000,
    10_000_000_000,
    100_000_000_000,
    1_000_000_000_000,
]; // Amount of FIDA tokens (with precision) that the discount account needs to hold

// | Tier | Low Leverage (i.e < 8x) | High Leverage (i.e > 8x) | Requirements   |
// | ---- | ----------------------- | ------------------------ | -------------- |
// | 0    | 20bps                   | 50bps                    | None           |
// | 1    | 15bps                   | 40bps                    | 500 FIDA       |
// | 2    | 15bps                   | 30bps                    | 1,000 FIDA     |
// | 3    | 10bps                   | 25bps                    | 10,000 FIDA    |
// | 4    | 10bps                   | 20bps                    | 100,000 FIDA   |
// | 5    | 10bps                   | 15bps                    | 1,000,000 FIDA |

////////////////////////////////////////////////////////////

pub mod add_budget;
pub mod add_instance;
pub mod add_page;
pub mod change_k;
pub mod close_account;
pub mod close_position;
pub mod create_market;
pub mod funding;
pub mod funding_extraction;
pub mod garbage_collection;
pub mod increase_position;
pub mod liquidation;
pub mod open_position;
pub mod rebalance;
pub mod transfer_position;
pub mod transfer_user_account;
pub mod update_oracle_account;
pub mod withdraw_budget;

pub struct Processor {}

impl Processor {
    pub fn process_instruction(
        program_id: &Pubkey,
        accounts: &[AccountInfo],
        instruction_data: &[u8],
    ) -> ProgramResult {
        msg!("Beginning processing");
        let instruction = PerpInstruction::try_from_slice(instruction_data)
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        msg!("Instruction unpacked");

        match instruction {
            PerpInstruction::CreateMarket {
                signer_nonce,
                market_symbol,
                initial_v_pc_amount,
                coin_decimals,
                quote_decimals,
            } => {
                msg!("Instruction: Create Market");
                process_create_market(
                    program_id,
                    accounts,
                    market_symbol,
                    signer_nonce,
                    initial_v_pc_amount,
                    coin_decimals,
                    quote_decimals,
                )?;
            }

            PerpInstruction::AddInstance => {
                msg!("Instruction: Add Instance");
                process_add_instance(program_id, accounts)?;
            }
            PerpInstruction::OpenPosition {
                side,
                collateral,
                instance_index,
                leverage,
                predicted_entry_price,
                maximum_slippage_margin,
            } => {
                msg!("Instruction: Open Position");
                process_open_position(
                    program_id,
                    accounts,
                    side,
                    instance_index,
                    collateral,
                    leverage,
                    predicted_entry_price,
                    maximum_slippage_margin,
                )?;
            }
            PerpInstruction::IncreasePosition {
                add_collateral,
                instance_index,
                leverage,
                position_index,
                predicted_entry_price,
                maximum_slippage_margin,
            } => {
                msg!("Instruction: Increase Position");
                process_increase_position(
                    program_id,
                    accounts,
                    instance_index,
                    leverage,
                    position_index,
                    add_collateral,
                    predicted_entry_price,
                    maximum_slippage_margin,
                )?;
            }
            PerpInstruction::ClosePosition {
                position_index,
                closing_collateral,
                closing_v_coin,
                predicted_entry_price,
                maximum_slippage_margin,
            } => {
                msg!("Instruction: Close Position");
                process_close_position(
                    program_id,
                    accounts,
                    position_index,
                    closing_collateral,
                    closing_v_coin,
                    predicted_entry_price,
                    maximum_slippage_margin,
                )?;
            }
            PerpInstruction::CollectGarbage {
                instance_index: leverage_index,
                max_iterations,
            } => {
                msg!("Instruction: Collect Garbage");
                process_garbage_collection(program_id, accounts, leverage_index, max_iterations)?;
            }
            PerpInstruction::CrankLiquidation {
                instance_index: leverage_index,
            } => {
                msg!("Instruction: Liquidate positions");
                process_liquidation(program_id, accounts, leverage_index)?;
            }
            PerpInstruction::CrankFunding => {
                msg!("Instruction: Crank Funding");
                process_funding(program_id, accounts)?;
            }
            PerpInstruction::FundingExtraction { instance_index } => {
                msg!("Instruction: Funding extraction");
                process_funding_extraction(program_id, instance_index, accounts)?;
            }
            PerpInstruction::AddBudget { amount } => {
                msg!("Instruction: Add budget");
                process_add_budget(program_id, amount, accounts)?;
            }
            PerpInstruction::WithdrawBudget { amount } => {
                msg!("Instruction: Withdraw budget");
                process_withdraw_budget(program_id, amount, accounts)?;
            }
            PerpInstruction::UpdateOracleAccount => {
                msg!("Instruction: Update Oracle Account");
                process_update_oracle_account(program_id, accounts)?;
            }
            PerpInstruction::ChangeK { factor } => {
                msg!("Instruction: Change K");
                process_change_k(program_id, factor, accounts)?;
            }
            PerpInstruction::CloseAccount => {
                msg!("Instruction: Close account");
                process_close_account(program_id, accounts)?;
            }
            PerpInstruction::AddPage { instance_index } => {
                msg!("Instruction: Add Page");
                process_add_page(program_id, accounts, instance_index)?;
            }
            PerpInstruction::Rebalance {
                collateral,
                instance_index,
            } => {
                msg!("Instruction: Rebalance");
                process_rebalance(program_id, accounts, instance_index, collateral)?;
            }
            PerpInstruction::TransferUserAccount {} => {
                msg!("Instruction: Transfer User Account");
                process_transfer_user_account(program_id, accounts)?;
            }
            PerpInstruction::TransferPosition { position_index } => {
                msg!("Instruction: Transfer Position");
                process_transfer_position(program_id, accounts, position_index)?;
            }
        }
        Ok(())
    }
}
