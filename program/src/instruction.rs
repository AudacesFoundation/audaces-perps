use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{instruction::Instruction, pubkey::Pubkey};

#[cfg(feature = "fuzz")]
use arbitrary::Arbitrary;

use crate::state::PositionType;
#[repr(C)]
#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum PerpInstruction {
    /// Creates a new perpetuals Market based on a currency
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[writable]` The market account
    ///   2. `[]` The sysvar clock account
    ///   2. `[]` The oracle account that will provide the index price for the coin (the current price account in the case of Pyth)
    ///   3. `[]` The admin account that will be able to add leverages to the market
    ///   4. `[writable]` The market vault account that will hold the funds, owned by the Market signer account
    CreateMarket {
        signer_nonce: u8,
        market_symbol: String,
        initial_v_pc_amount: u64,
        coin_decimals: u8,
        quote_decimals: u8,
    },
    /// Adds a new leverage to the existing market
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[writable]` The market account
    ///   2. `[signer]` The market admin account
    ///   3. `[writable]` The instance account
    ///   4... `[writable]` The positions book page accounts
    AddInstance,
    /// Updates the oracle account key that is stored in the market state, following the Pyth Oracle mapping.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[writable]` The market account
    ///   2. `[]` The pyth oracle mapping account
    ///   3. `[]` The pyth oracle product account
    ///   4. `[]` The pyth oracle price account
    UpdateOracleAccount,
    /// Open a new position
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[]` The spl token program account
    ///   2. `[]` The clock sysvar account
    ///   3. `[writable]` The market account
    ///   4. `[writable]` The instance account
    ///   5. `[]` The market signer program account
    ///   6. `[writable]` The market vault account
    ///   7. `[writable]` The bonfida buy and burn account
    ///   8. `[signer]` The owner account of the open positions account
    ///   9. `[writable]` The open positions account
    ///   10..N `[writable]` The positions book page accounts
    ///   N+1. `[]` (Optional) The discount account to calculate the fee tiers
    ///   N+2. `[signer]` (Optional) The owner account of the discount account
    ///   N+3. `[writable]` (Optional) The referrer USDC account which receives 10 percent of the fees
    OpenPosition {
        side: PositionType,
        collateral: u64,
        instance_index: u8,
        leverage: u64,
        predicted_entry_price: u64,   // 32 bit FP
        maximum_slippage_margin: u64, // 32 bit FP
    },
    /// Add USDC tokens to the user budget. The current budget is saved in the open position
    /// accounts state while the tokens are stored in the market vault. When opening, closing (etc)
    /// positions, the user budget is updated is the state. The instance_index argument except when creating
    /// a new account with this insturuction.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[]` The spl token program account
    ///   2. `[writable]` The market account
    ///   3. `[writable]` The market vault account
    ///   4. `[writable]` The open positions account
    ///   5. `[signer]` The owner account of the source USDC account
    ///   6. `[writable]` The source USDC account
    AddBudget {
        amount: u64,
    },
    /// Wightdraw USDC tokens from the user budget. The current budget is saved in the open position
    /// accounts state while the tokens are stored in the market vault. When opening, closing (etc)
    /// positions, the user budget is updated is the state.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[]` The spl token program account
    ///   2. `[writable]` The market account
    ///   3. `[]` The market signer program account
    ///   4. `[writable]` The market vault account
    ///   5. `[signer]` The open positions owner account
    ///   6. `[writable]` The open positions account
    ///   7. `[writable]` The target USDC account
    WithdrawBudget {
        amount: u64,
    },
    /// Increase a position by adding collateral which will be invested in the vAMM.
    /// This also allows to shift the liquidation index accordingly.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[]` The spl token program account
    ///   2. `[]` The clock sysvar account
    ///   3. `[writable]` The market account
    ///   4. `[]` The market signer program account
    ///   5. `[writable]` The market vault account
    ///   6. `[writable]` The bonfida buy and burn account
    ///   7. `[writable]` The instance account
    ///   8. `[signer]` The open position owner account
    ///   9. `[writable]` The corresponding open positions account
    ///   10... `[writable]` The positions book page accounts
    ///   N+1. `[]` (Optional) The discount account to calculate the fee tiers
    ///   N+2. `[signer]` (Optional) The owner account of the discount account
    ///   N+3. `[writable]` (Optional) The referrer USDC account which receives 10 percent of the fees
    IncreasePosition {
        add_collateral: u64,
        instance_index: u8,
        leverage: u64,
        position_index: u16,
        predicted_entry_price: u64,   // 32 bit FP
        maximum_slippage_margin: u64, // 32 bit FP
    },
    /// Close a position
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[]` The spl token program account
    ///   2. `[]` The clock sysvar account
    ///   3. `[writable]` The market account
    ///   4. `[writable]` The instance account
    ///   5. `[]` The market signer program account
    ///   6. `[writable]` The market vault account
    ///   7. `[writable]` The bonfida buy and burn account
    ///   8. `[]` The oracle account
    ///   9. `[signer]` The open position owner account
    ///   10. `[writable]` The corresponding open positions account
    ///   11..N `[writable]` The positions book page accounts
    ///   N+1. `[]` (Optional) The discount account to calculate the fee tiers
    ///   N+2. `[signer]` (Optional) The owner account of the discount account
    ///   N+3. `[writable]` (Optional) The referrer USDC account which receives 10 percent of the fees
    ClosePosition {
        position_index: u16,
        closing_collateral: u64,
        closing_v_coin: u64,
        predicted_entry_price: u64,   // 32 bit FP
        maximum_slippage_margin: u64, // 32 bit FP
    },
    /// Garbage collection in the distributed positons database.
    /// Reward is flat fee per freed slot
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[]` The spl-token program account
    ///   2. `[writable]` The market account
    ///   3. `[writable]` The instance account
    ///   4. `[writable]` The market vault account
    ///   5. `[]` The market signer program account
    ///   6. `[writable]` The target USDC account
    ///   7... `[writable]` The positions book page accounts
    CollectGarbage {
        instance_index: u8,
        max_iterations: u64,
    },
    /// Crank the liquidation of the losing positions in the market
    /// A reward is transferred to the cranker.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[]` The spl token program account
    ///   2. `[writable]` The market account
    ///   3. `[writable]` The instance account
    ///   4. `[]` The market signer program account
    ///   5. `[writable]` The bonfida buy and burn account
    ///   6. `[writable]` The market vault account
    ///   7. `[]` The price oracle account
    ///   8. `[writable]` The target USDC account
    ///   9... `[writable]` The positions book page accounts
    CrankLiquidation {
        instance_index: u8,
    },
    /// Crank the funding of the market
    /// A reward is transferred to the cranker.
    /// Crank the recording of the price history into the MarketState.
    /// That way a buffer of the index and market prices over the past
    /// is maintained and can be used for the funding ratio calculation
    /// which uses an average over this period.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[]` The clock sysvar account
    ///   3. `[writable]` The market account
    ///   6. `[]` The price oracle account
    CrankFunding,
    /// Crank the funding of the market
    /// A reward is transferred to the cranker.
    /// Crank the recording of the price history into the MarketState.
    /// That way a buffer of the index and market prices over the past
    /// is maintained and can be used for the funding ratio calculation
    /// which uses an average over this period.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[writable]` The market account
    ///   2. `[writable]` The instance account
    ///   3. `[writable]` The user account
    ///   4... `[writable]` The positions book page accounts
    FundingExtraction {
        instance_index: u8,
    },
    ChangeK {
        factor: u64,
    },
    CloseAccount,
    /// Add a page to the instance of given index.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[]` The market account
    ///   3. `[signer]` The market admin account
    ///   2. `[writable]` The instance account
    ///   4. `[writable]` The new page account
    AddPage {
        instance_index: u8,
    },
    Rebalance {
        collateral: u64,
        instance_index: u8,
    },
    /// Transfer a user account ownership to a new address.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[signer]` The user account owner
    ///   2. `[writable]` The user account
    ///   3. `[]` The new user account owner
    TransferUserAccount {},
    /// Transfer a position from one user account to another.
    ///
    /// Accounts expected by this instruction:
    ///
    ///   1. `[signer]` The source user account owner
    ///   2. `[writable]` The source user account
    ///   3. `[signer]` The destination user account owner
    ///   4. `[writable]` The destination user account
    TransferPosition {
        position_index: u16,
    },
}

pub enum CloseOrOpen {
    OpenPosition,
    ClosePosition,
}

pub struct MarketContext {
    pub audaces_protocol_program_id: Pubkey,
    pub signer_nonce: u8,
    pub market_signer_account: Pubkey,
    pub oracle_account: Pubkey,
    pub market_account: Pubkey,
    pub admin_account: Pubkey,
    pub market_vault: Pubkey,
    pub bonfida_bnb: Pubkey,
    pub instances: Vec<InstanceContext>,
}

pub struct InstanceContext {
    pub instance_account: Pubkey,
    pub memory_pages: Vec<Pubkey>,
}

pub struct DiscountAccount {
    pub owner: Pubkey,
    pub address: Pubkey,
}

pub struct PositionInfo {
    pub user_account: Pubkey,
    pub user_account_owner: Pubkey,
    pub instance_index: u8,
    pub side: PositionType,
}

pub fn create_market(
    ctx: &MarketContext,
    market_symbol: String,
    initial_v_pc_amount: u64,
    coin_decimals: u8,
    quote_decimals: u8,
) -> Instruction {
    cpi::create_market(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.oracle_account,
        ctx.admin_account,
        ctx.market_vault,
        market_symbol,
        ctx.signer_nonce,
        initial_v_pc_amount,
        coin_decimals,
        quote_decimals,
    )
}

pub fn update_oracle_account(
    ctx: &MarketContext,
    pyth_oracle_mapping_account: Pubkey,
    pyth_oracle_product_account: Pubkey,
    pyth_oracle_price_account: Pubkey,
) -> Instruction {
    cpi::update_oracle_account(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        pyth_oracle_mapping_account,
        pyth_oracle_product_account,
        pyth_oracle_price_account,
    )
}

pub fn add_instance(
    ctx: &MarketContext,
    instance_account: Pubkey,
    memory_pages: &[Pubkey],
) -> Instruction {
    cpi::add_instance(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.admin_account,
        instance_account,
        memory_pages,
    )
}

pub fn add_budget(
    ctx: &MarketContext,
    amount: u64,
    source_owner: Pubkey,
    source_token_account: Pubkey,
    open_positions_account: Pubkey,
) -> Instruction {
    cpi::add_budget(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.market_vault,
        amount,
        source_owner,
        source_token_account,
        open_positions_account,
    )
}

pub fn withdraw_budget(
    ctx: &MarketContext,
    amount: u64,
    target_account: Pubkey,
    open_positions_owner_account: Pubkey,
    open_positions_account: Pubkey,
) -> Instruction {
    cpi::withdraw_budget(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.market_signer_account,
        ctx.market_vault,
        amount,
        target_account,
        open_positions_owner_account,
        open_positions_account,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn open_position(
    ctx: &MarketContext,
    position: &PositionInfo,
    collateral: u64,
    leverage: u64,
    predicted_entry_price: u64,                     // 32 bit FP
    maximum_slippage_margin: u64,                   // 32 bit FP
    discount_account_opt: Option<&DiscountAccount>, // To specify if discount account is present
    referrer_account_opt: Option<Pubkey>,
) -> Instruction {
    let instance = &ctx.instances[position.instance_index as usize];
    cpi::open_position(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.market_signer_account,
        ctx.market_vault,
        ctx.oracle_account,
        instance.instance_account,
        position.user_account,
        position.user_account_owner,
        ctx.bonfida_bnb,
        &instance.memory_pages,
        position.side,
        position.instance_index,
        collateral,
        leverage,
        predicted_entry_price,
        maximum_slippage_margin,
        discount_account_opt,
        referrer_account_opt,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn increase_position(
    ctx: &MarketContext,
    add_collateral: u64,
    leverage: u64, // 32 bit FP
    instance_index: u8,
    position_index: u16,
    user_account_owner: Pubkey,
    user_account: Pubkey,
    predicted_entry_price: u64,                     // 32 bit FP
    maximum_slippage_margin: u64,                   // 32 bit FP
    discount_account_opt: Option<&DiscountAccount>, // To specify if discount account is present
    referrer_account_opt: Option<Pubkey>,
) -> Instruction {
    let instance = &ctx.instances[instance_index as usize];
    cpi::increase_position(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.market_signer_account,
        ctx.market_vault,
        ctx.oracle_account,
        instance.instance_account,
        user_account,
        user_account_owner,
        ctx.bonfida_bnb,
        &instance.memory_pages,
        add_collateral,
        leverage,
        instance_index,
        position_index,
        predicted_entry_price,
        maximum_slippage_margin,
        discount_account_opt,
        referrer_account_opt,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn close_position(
    ctx: &MarketContext,
    position_info: &PositionInfo,
    closing_collateral: u64,
    closing_v_coin: u64,
    position_index: u16,
    predicted_entry_price: u64,                 // 32 bit FP
    maximum_slippage_margin: u64,               // 32 bit FP
    discount_account: Option<&DiscountAccount>, // To specify if discount account is present
    referrer_account_opt: Option<Pubkey>,
) -> Instruction {
    let instance = &ctx.instances[position_info.instance_index as usize];
    cpi::close_position(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.market_signer_account,
        ctx.market_vault,
        ctx.oracle_account,
        instance.instance_account,
        position_info.user_account,
        position_info.user_account_owner,
        ctx.bonfida_bnb,
        &instance.memory_pages,
        closing_collateral,
        closing_v_coin,
        position_index,
        predicted_entry_price,
        maximum_slippage_margin,
        discount_account,
        referrer_account_opt,
    )
}

pub fn collect_garbage(
    ctx: &MarketContext,
    instance_index: u8,
    max_iterations: u64,
    target_token_account: Pubkey,
) -> Instruction {
    let instance = &ctx.instances[instance_index as usize];
    cpi::collect_garbage(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.market_signer_account,
        ctx.market_vault,
        instance.instance_account,
        &instance.memory_pages,
        instance_index,
        max_iterations,
        target_token_account,
    )
}

pub fn crank_liquidation(
    ctx: &MarketContext,
    instance_index: u8,
    target_token_account: Pubkey,
) -> Instruction {
    let instance = &ctx.instances[instance_index as usize];
    cpi::crank_liquidation(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.market_signer_account,
        ctx.market_vault,
        ctx.oracle_account,
        instance.instance_account,
        ctx.bonfida_bnb,
        &instance.memory_pages,
        instance_index,
        target_token_account,
    )
}

pub fn crank_funding(ctx: &MarketContext) -> Instruction {
    cpi::crank_funding(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.oracle_account,
    )
}

pub fn extract_funding(
    ctx: &MarketContext,
    instance_index: u8,
    open_positions_account: Pubkey,
) -> Instruction {
    let instance = &ctx.instances[instance_index as usize];
    cpi::extract_funding(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.oracle_account,
        instance.instance_account,
        &instance.memory_pages,
        instance_index,
        open_positions_account,
    )
}

pub fn change_k(ctx: &MarketContext, factor: u64) -> Instruction {
    cpi::change_k(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.admin_account,
        factor,
    )
}

pub fn close_account(
    ctx: &MarketContext,
    user_account: Pubkey,
    user_account_owner: Pubkey,
    lamports_target: Pubkey,
) -> Instruction {
    cpi::close_account(
        ctx.audaces_protocol_program_id,
        user_account,
        user_account_owner,
        lamports_target,
    )
}

pub fn add_page(ctx: &MarketContext, instance_index: u8, new_memory_page: Pubkey) -> Instruction {
    cpi::add_page(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.admin_account,
        ctx.instances[instance_index as usize].instance_account,
        instance_index,
        new_memory_page,
    )
}

pub fn rebalance(
    ctx: &MarketContext,
    user_account: Pubkey,
    user_account_owner: Pubkey,
    instance_index: u8,
    collateral: u64,
) -> Instruction {
    let instance = &ctx.instances[instance_index as usize];
    cpi::rebalance(
        ctx.audaces_protocol_program_id,
        ctx.market_account,
        ctx.market_signer_account,
        ctx.market_vault,
        ctx.admin_account,
        instance.instance_account,
        user_account,
        user_account_owner,
        ctx.bonfida_bnb,
        &instance.memory_pages,
        instance_index,
        collateral,
    )
}

pub fn transfer_user_account(
    ctx: &MarketContext,
    user_account: Pubkey,
    user_account_owner: Pubkey,
    new_user_account_owner: Pubkey,
) -> Instruction {
    cpi::transfer_user_account(
        ctx.audaces_protocol_program_id,
        user_account,
        user_account_owner,
        new_user_account_owner,
    )
}

pub fn transfer_position(
    ctx: &MarketContext,
    position_index: u16,
    source_user_account: Pubkey,
    source_user_account_owner: Pubkey,
    destination_user_account: Pubkey,
    destination_user_account_owner: Pubkey,
) -> Instruction {
    cpi::transfer_position(
        ctx.audaces_protocol_program_id,
        position_index,
        source_user_account,
        source_user_account_owner,
        destination_user_account,
        destination_user_account_owner,
    )
}

pub mod cpi {
    use std::str::FromStr;

    use crate::{
        processor::{FUNDING_EXTRACTION_LABEL, FUNDING_LABEL, LIQUIDATION_LABEL, TRADE_LABEL},
        state::PositionType,
    };

    use super::{DiscountAccount, PerpInstruction};
    use borsh::BorshSerialize;
    use solana_program::{
        instruction::{AccountMeta, Instruction},
        pubkey::Pubkey,
        sysvar::clock,
    };

    #[allow(clippy::clippy::too_many_arguments)]
    pub fn create_market(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        oracle_account: Pubkey,
        admin_account: Pubkey,
        market_vault: Pubkey,
        market_symbol: String,
        signer_nonce: u8,
        initial_v_pc_amount: u64,
        coin_decimals: u8,
        quote_decimals: u8,
    ) -> Instruction {
        let instruction_data = PerpInstruction::CreateMarket {
            signer_nonce,
            market_symbol,
            initial_v_pc_amount,
            coin_decimals,
            quote_decimals,
        };
        let data = instruction_data.try_to_vec().unwrap();
        let accounts = vec![
            AccountMeta::new(market_account, false),
            AccountMeta::new_readonly(clock::id(), false),
            AccountMeta::new_readonly(oracle_account, false),
            AccountMeta::new_readonly(admin_account, false),
            AccountMeta::new_readonly(market_vault, false),
        ];

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }

    pub fn update_oracle_account(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        pyth_oracle_mapping_account: Pubkey,
        pyth_oracle_product_account: Pubkey,
        pyth_oracle_price_account: Pubkey,
    ) -> Instruction {
        let instruction_data = PerpInstruction::UpdateOracleAccount;
        let data = instruction_data.try_to_vec().unwrap();
        let accounts = vec![
            AccountMeta::new(market_account, false),
            AccountMeta::new_readonly(pyth_oracle_mapping_account, false),
            AccountMeta::new_readonly(pyth_oracle_product_account, false),
            AccountMeta::new_readonly(pyth_oracle_price_account, false),
        ];

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }

    pub fn add_instance(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        admin_account: Pubkey,
        instance_account: Pubkey,
        memory_pages: &[Pubkey],
    ) -> Instruction {
        let instruction_data = PerpInstruction::AddInstance;
        let data = instruction_data.try_to_vec().unwrap();
        let mut accounts = Vec::with_capacity(3 + memory_pages.len());
        accounts.push(AccountMeta::new(market_account, false));
        accounts.push(AccountMeta::new(admin_account, true));
        accounts.push(AccountMeta::new(instance_account, false));

        for p in memory_pages {
            accounts.push(AccountMeta::new(*p, false))
        }
        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }

    pub fn add_budget(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        market_vault: Pubkey,
        amount: u64,
        source_owner: Pubkey,
        source_token_account: Pubkey,
        open_positions_account: Pubkey,
    ) -> Instruction {
        let instruction_data = PerpInstruction::AddBudget { amount };
        let data = instruction_data.try_to_vec().unwrap();
        let accounts = vec![
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new(market_account, false),
            AccountMeta::new(market_vault, false),
            AccountMeta::new(open_positions_account, false),
            AccountMeta::new_readonly(source_owner, true),
            AccountMeta::new(source_token_account, false),
        ];

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }

    #[allow(clippy::clippy::too_many_arguments)]
    pub fn withdraw_budget(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        market_signer_account: Pubkey,
        market_vault: Pubkey,
        amount: u64,
        target_account: Pubkey,
        open_positions_owner_account: Pubkey,
        open_positions_account: Pubkey,
    ) -> Instruction {
        let instruction_data = PerpInstruction::WithdrawBudget { amount };
        let data = instruction_data.try_to_vec().unwrap();
        let accounts = vec![
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new(market_account, false),
            AccountMeta::new_readonly(market_signer_account, false),
            AccountMeta::new(market_vault, false),
            AccountMeta::new_readonly(open_positions_owner_account, true),
            AccountMeta::new(open_positions_account, false),
            AccountMeta::new(target_account, false),
        ];

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open_position(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        market_signer_account: Pubkey,
        market_vault: Pubkey,
        oracle_account: Pubkey,
        instance_account: Pubkey,
        user_account: Pubkey,
        user_account_owner: Pubkey,
        bonfida_bnb: Pubkey,
        memory_pages: &[Pubkey],
        side: PositionType,
        instance_index: u8,
        collateral: u64,
        leverage: u64,
        predicted_entry_price: u64,                     // 32 bit FP
        maximum_slippage_margin: u64,                   // 32 bit FP
        discount_account_opt: Option<&DiscountAccount>, // To specify if discount account is present
        referrer_account_opt: Option<Pubkey>,
    ) -> Instruction {
        let instruction_data = PerpInstruction::OpenPosition {
            side,
            collateral,
            instance_index,
            leverage,
            predicted_entry_price,
            maximum_slippage_margin,
        };
        let data = instruction_data.try_to_vec().unwrap();
        let mut accounts = Vec::with_capacity(13);

        accounts.push(AccountMeta::new_readonly(spl_token::id(), false));
        accounts.push(AccountMeta::new_readonly(clock::id(), false));
        accounts.push(AccountMeta::new(market_account, false));
        accounts.push(AccountMeta::new(instance_account, false));
        accounts.push(AccountMeta::new_readonly(market_signer_account, false));
        accounts.push(AccountMeta::new(market_vault, false));
        accounts.push(AccountMeta::new(bonfida_bnb, false));
        accounts.push(AccountMeta::new_readonly(user_account_owner, true));
        accounts.push(AccountMeta::new(user_account, false));
        accounts.push(AccountMeta::new_readonly(
            Pubkey::from_str(TRADE_LABEL).unwrap(),
            false,
        ));
        accounts.push(AccountMeta::new_readonly(oracle_account, false));

        for p in memory_pages {
            accounts.push(AccountMeta::new(*p, false))
        }

        if let Some(d) = discount_account_opt {
            accounts.push(AccountMeta::new_readonly(d.address, false));
            accounts.push(AccountMeta::new_readonly(d.owner, true));
        }
        if let Some(referrer_account) = referrer_account_opt {
            accounts.push(AccountMeta::new(referrer_account, false));
        }

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn increase_position(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        market_signer_account: Pubkey,
        market_vault: Pubkey,
        oracle_account: Pubkey,
        instance_account: Pubkey,
        user_account: Pubkey,
        user_account_owner: Pubkey,
        bonfida_bnb: Pubkey,
        memory_pages: &[Pubkey],
        add_collateral: u64,
        leverage: u64, // 32 bit FP
        instance_index: u8,
        position_index: u16,
        predicted_entry_price: u64,                     // 32 bit FP
        maximum_slippage_margin: u64,                   // 32 bit FP
        discount_account_opt: Option<&DiscountAccount>, // To specify if discount account is present
        referrer_account_opt: Option<Pubkey>,
    ) -> Instruction {
        let instruction_data = PerpInstruction::IncreasePosition {
            instance_index,
            add_collateral,
            position_index,
            leverage,
            predicted_entry_price,
            maximum_slippage_margin,
        };
        let data = instruction_data.try_to_vec().unwrap();
        let mut accounts = Vec::with_capacity(5 + memory_pages.len());

        accounts.push(AccountMeta::new_readonly(spl_token::id(), false));
        accounts.push(AccountMeta::new_readonly(clock::id(), false));
        accounts.push(AccountMeta::new(market_account, false));
        accounts.push(AccountMeta::new_readonly(market_signer_account, false));
        accounts.push(AccountMeta::new(market_vault, false));
        accounts.push(AccountMeta::new(bonfida_bnb, false));
        accounts.push(AccountMeta::new(instance_account, false));
        accounts.push(AccountMeta::new_readonly(user_account_owner, true));
        accounts.push(AccountMeta::new(user_account, false));
        accounts.push(AccountMeta::new_readonly(
            Pubkey::from_str(TRADE_LABEL).unwrap(),
            false,
        ));
        accounts.push(AccountMeta::new_readonly(oracle_account, false));

        for p in memory_pages {
            accounts.push(AccountMeta::new(*p, false))
        }

        if let Some(d) = discount_account_opt {
            accounts.push(AccountMeta::new_readonly(d.address, false));
            accounts.push(AccountMeta::new_readonly(d.owner, true));
        }
        if let Some(referrer_account) = referrer_account_opt {
            accounts.push(AccountMeta::new(referrer_account, false));
        }

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn close_position(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        market_signer_account: Pubkey,
        market_vault: Pubkey,
        oracle_account: Pubkey,
        instance_account: Pubkey,
        user_account: Pubkey,
        user_account_owner: Pubkey,
        bonfida_bnb: Pubkey,
        memory_pages: &[Pubkey],
        closing_collateral: u64,
        closing_v_coin: u64,
        position_index: u16,
        predicted_entry_price: u64,                 // 32 bit FP
        maximum_slippage_margin: u64,               // 32 bit FP
        discount_account: Option<&DiscountAccount>, // To specify if discount account is present
        referrer_account_opt: Option<Pubkey>,
    ) -> Instruction {
        let instruction_data = PerpInstruction::ClosePosition {
            closing_collateral,
            closing_v_coin,
            position_index,
            predicted_entry_price,
            maximum_slippage_margin,
        };
        let data = instruction_data.try_to_vec().unwrap();
        let mut accounts = Vec::with_capacity(13 + memory_pages.len());
        accounts.push(AccountMeta::new_readonly(spl_token::id(), false));
        accounts.push(AccountMeta::new_readonly(clock::id(), false));
        accounts.push(AccountMeta::new(market_account, false));
        accounts.push(AccountMeta::new(instance_account, false));
        accounts.push(AccountMeta::new_readonly(market_signer_account, false));
        accounts.push(AccountMeta::new(market_vault, false));
        accounts.push(AccountMeta::new(bonfida_bnb, false));
        accounts.push(AccountMeta::new_readonly(oracle_account, false));
        accounts.push(AccountMeta::new_readonly(user_account_owner, true));
        accounts.push(AccountMeta::new(user_account, false));
        accounts.push(AccountMeta::new_readonly(
            Pubkey::from_str(TRADE_LABEL).unwrap(),
            false,
        ));

        for p in memory_pages {
            accounts.push(AccountMeta::new(*p, false))
        }
        if let Some(d) = discount_account {
            accounts.push(AccountMeta::new_readonly(d.address, false));
            accounts.push(AccountMeta::new_readonly(d.owner, true));
        }
        if let Some(referrer_account) = referrer_account_opt {
            accounts.push(AccountMeta::new(referrer_account, false));
        }

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }

    #[allow(clippy::clippy::too_many_arguments)]
    pub fn collect_garbage(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        market_signer_account: Pubkey,
        market_vault: Pubkey,
        instance_account: Pubkey,
        memory_pages: &[Pubkey],
        instance_index: u8,
        max_iterations: u64,
        target_token_account: Pubkey,
    ) -> Instruction {
        let instruction_data = PerpInstruction::CollectGarbage {
            instance_index,
            max_iterations,
        };
        let data = instruction_data.try_to_vec().unwrap();
        let mut accounts = Vec::with_capacity(6 + memory_pages.len());

        accounts.push(AccountMeta::new_readonly(spl_token::id(), false));
        accounts.push(AccountMeta::new(market_account, false));
        accounts.push(AccountMeta::new(instance_account, false));
        accounts.push(AccountMeta::new(market_vault, false));
        accounts.push(AccountMeta::new_readonly(market_signer_account, false));
        accounts.push(AccountMeta::new(target_token_account, false));

        for p in memory_pages {
            accounts.push(AccountMeta::new(*p, false))
        }
        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }

    #[allow(clippy::clippy::too_many_arguments)]
    pub fn crank_liquidation(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        market_signer_account: Pubkey,
        market_vault: Pubkey,
        oracle_account: Pubkey,
        instance_account: Pubkey,
        bonfida_bnb: Pubkey,
        memory_pages: &[Pubkey],
        instance_index: u8,
        target_token_account: Pubkey,
    ) -> Instruction {
        let instruction_data = PerpInstruction::CrankLiquidation { instance_index };
        let data = instruction_data.try_to_vec().unwrap();
        let mut accounts = Vec::with_capacity(7 + memory_pages.len());

        accounts.push(AccountMeta::new_readonly(spl_token::id(), false));
        accounts.push(AccountMeta::new(market_account, false));
        accounts.push(AccountMeta::new(instance_account, false));
        accounts.push(AccountMeta::new_readonly(market_signer_account, false));
        accounts.push(AccountMeta::new(bonfida_bnb, false));
        accounts.push(AccountMeta::new(market_vault, false));
        accounts.push(AccountMeta::new_readonly(oracle_account, false));
        accounts.push(AccountMeta::new(target_token_account, false));
        accounts.push(AccountMeta::new_readonly(
            Pubkey::from_str(LIQUIDATION_LABEL).unwrap(),
            false,
        ));

        for p in memory_pages {
            accounts.push(AccountMeta::new(*p, false))
        }
        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }
    #[allow(clippy::clippy::too_many_arguments)]
    pub fn crank_funding(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        oracle_account: Pubkey,
    ) -> Instruction {
        let instruction_data = PerpInstruction::CrankFunding;
        let data = instruction_data.try_to_vec().unwrap();
        let accounts = vec![
            AccountMeta::new_readonly(clock::id(), false),
            AccountMeta::new(market_account, false),
            AccountMeta::new_readonly(oracle_account, false),
            AccountMeta::new_readonly(Pubkey::from_str(FUNDING_LABEL).unwrap(), false),
        ];

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }
    #[allow(clippy::clippy::too_many_arguments)]
    pub fn extract_funding(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        oracle_account: Pubkey,
        instance_account: Pubkey,
        memory_pages: &[Pubkey],
        instance_index: u8,
        open_positions_account: Pubkey,
    ) -> Instruction {
        let instruction_data = PerpInstruction::FundingExtraction { instance_index };
        let data = instruction_data.try_to_vec().unwrap();
        let mut accounts = Vec::with_capacity(7 + memory_pages.len());
        accounts.push(AccountMeta::new(market_account, false));
        accounts.push(AccountMeta::new(instance_account, false));
        accounts.push(AccountMeta::new(open_positions_account, false));
        accounts.push(AccountMeta::new_readonly(
            Pubkey::from_str(FUNDING_EXTRACTION_LABEL).unwrap(),
            false,
        ));
        accounts.push(AccountMeta::new_readonly(oracle_account, false));
        for p in memory_pages {
            accounts.push(AccountMeta::new(*p, false))
        }
        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }
    #[allow(clippy::clippy::too_many_arguments)]
    pub fn change_k(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        admin_account: Pubkey,
        factor: u64,
    ) -> Instruction {
        let data = PerpInstruction::ChangeK { factor }.try_to_vec().unwrap();
        let accounts = vec![
            AccountMeta::new(market_account, false),
            AccountMeta::new_readonly(admin_account, true),
        ];
        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }
    #[allow(clippy::clippy::too_many_arguments)]
    pub fn close_account(
        audaces_protocol_program_id: Pubkey,
        user_account: Pubkey,
        user_account_owner: Pubkey,
        lamports_target: Pubkey,
    ) -> Instruction {
        let data = PerpInstruction::CloseAccount.try_to_vec().unwrap();
        let accounts = vec![
            AccountMeta::new(user_account, false),
            AccountMeta::new_readonly(user_account_owner, true),
            AccountMeta::new(lamports_target, false),
        ];
        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }
    #[allow(clippy::clippy::too_many_arguments)]
    pub fn add_page(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        admin_account: Pubkey,
        instance_account: Pubkey,
        instance_index: u8,
        new_memory_page: Pubkey,
    ) -> Instruction {
        let instruction_data = PerpInstruction::AddPage { instance_index };
        let data = instruction_data.try_to_vec().unwrap();
        let accounts = vec![
            AccountMeta::new_readonly(market_account, false),
            AccountMeta::new_readonly(admin_account, true),
            AccountMeta::new(instance_account, false),
            AccountMeta::new_readonly(new_memory_page, false),
        ];

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }
    #[allow(clippy::clippy::too_many_arguments)]
    pub fn rebalance(
        audaces_protocol_program_id: Pubkey,
        market_account: Pubkey,
        market_signer_account: Pubkey,
        market_vault: Pubkey,
        admin_account: Pubkey,
        instance_account: Pubkey,
        user_account: Pubkey,
        user_account_owner: Pubkey,
        bonfida_bnb: Pubkey,
        memory_pages: &[Pubkey],
        instance_index: u8,
        collateral: u64,
    ) -> Instruction {
        let data = PerpInstruction::Rebalance {
            collateral,
            instance_index,
        }
        .try_to_vec()
        .unwrap();
        let mut accounts = vec![
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(clock::id(), false),
            AccountMeta::new(market_account, false),
            AccountMeta::new(instance_account, false),
            AccountMeta::new_readonly(market_signer_account, false),
            AccountMeta::new(market_vault, false),
            AccountMeta::new(bonfida_bnb, false),
            AccountMeta::new_readonly(user_account_owner, true),
            AccountMeta::new(user_account, false),
            AccountMeta::new_readonly(admin_account, true),
        ];

        for p in memory_pages {
            accounts.push(AccountMeta::new(*p, false))
        }
        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }
    #[allow(clippy::clippy::too_many_arguments)]
    pub fn transfer_user_account(
        audaces_protocol_program_id: Pubkey,
        user_account: Pubkey,
        user_account_owner: Pubkey,
        new_user_account_owner: Pubkey,
    ) -> Instruction {
        let data = PerpInstruction::TransferUserAccount {}
            .try_to_vec()
            .unwrap();
        let accounts = vec![
            AccountMeta::new_readonly(user_account_owner, true),
            AccountMeta::new(user_account, false),
            AccountMeta::new_readonly(new_user_account_owner, false),
        ];

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }
    #[allow(clippy::clippy::too_many_arguments)]
    pub fn transfer_position(
        audaces_protocol_program_id: Pubkey,
        position_index: u16,
        source_user_account: Pubkey,
        source_user_account_owner: Pubkey,
        destination_user_account: Pubkey,
        destination_user_account_owner: Pubkey,
    ) -> Instruction {
        let data = PerpInstruction::TransferPosition { position_index }
            .try_to_vec()
            .unwrap();
        let accounts = vec![
            AccountMeta::new_readonly(source_user_account_owner, true),
            AccountMeta::new(source_user_account, false),
            AccountMeta::new_readonly(destination_user_account_owner, true),
            AccountMeta::new(destination_user_account, false),
        ];

        Instruction {
            program_id: audaces_protocol_program_id,
            accounts,
            data,
        }
    }
}
