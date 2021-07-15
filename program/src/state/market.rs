use crate::{
    error::{PerpError, PerpResult},
    processor::{
        ALLOCATION_FEE, FEE_BUY_BURN_BONFIDA, FEE_REBALANCING_FUND, FEE_REFERRER,
        REBALANCING_LEVERAGE, REBALANCING_MARGIN,
    },
    state::PositionType,
    utils::compute_bias,
};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    msg,
    program::invoke_signed,
    program_error::ProgramError,
    program_pack::{Pack, Sealed},
    pubkey::Pubkey,
};
use spl_token::instruction::transfer;

use super::{Fees, StateObject};

// Pubkeys are stored as [u8; 32] for use with borsh

#[derive(BorshDeserialize, BorshSerialize, Debug, Clone)]
pub struct MarketState {
    pub version: u8,
    pub signer_nonce: u8,
    pub market_symbol: [u8; 32], // Needed to identify the correct pyth oracle price account, example: "BTC/USD".to_bytes()
    pub oracle_address: [u8; 32], // For the Pyth oracle, this is the current price account address
    pub admin_address: [u8; 32],
    pub vault_address: [u8; 32],
    pub quote_decimals: u8,
    pub coin_decimals: u8,
    pub total_collateral: u64,
    pub total_user_balances: u64,
    pub total_fee_balance: u64,
    pub rebalancing_funds: u64,
    pub rebalanced_v_coin: i64,
    pub v_coin_amount: u64,
    pub v_pc_amount: u64,
    pub open_shorts_v_coin: u64,
    pub open_longs_v_coin: u64,
    pub open_shorts_v_pc: u64,
    pub open_longs_v_pc: u64,
    pub last_funding_timestamp: u64,
    pub last_recording_timestamp: u64,
    pub funding_samples_count: u8,
    pub funding_samples_sum: i64,
    pub funding_history_offset: u8,
    pub funding_history: [i64; 16],
    pub funding_balancing_factors: [u64; 16], // FP 32 measure of payment capping to ensure that the insurance fund does not pay funding.
    pub number_of_instances: u32,
}

impl Sealed for MarketState {}

impl Pack for MarketState {
    const LEN: usize = 507;

    fn pack_into_slice(&self, dst: &mut [u8]) {
        dst[0] = StateObject::MarketState as u8;
        self.serialize(&mut &mut dst[1..]).unwrap();
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src[0] != StateObject::MarketState as u8 {
            if src[0] == 0 {
                return Err(ProgramError::UninitializedAccount);
            }
            return Err(ProgramError::InvalidAccountData);
        };
        MarketState::deserialize(&mut &src[1..]).map_err(|_| {
            msg!("Failed to deserialize market account");
            ProgramError::InvalidAccountData
        })
    }
}

impl MarketState {
    pub fn compute_add_v_coin(&self, v_pc_amount: i64) -> Result<i64, PerpError> {
        let final_v_pc = self.v_pc_amount as i64 + v_pc_amount;
        if final_v_pc.is_negative() {
            msg!("Vpc amount is too large!");
            return Err(PerpError::AmountTooLarge);
        }
        let add_v_coin_amount = (((v_pc_amount.abs() as u128) * (self.v_coin_amount as u128))
            / (final_v_pc as u128)) as u64;
        Ok(-v_pc_amount.signum() * (add_v_coin_amount as i64))
    }

    pub fn compute_add_v_pc(&self, v_coin_amount: i64) -> Result<i64, PerpError> {
        let final_v_coin = self.v_coin_amount as i64 + v_coin_amount;
        if final_v_coin.is_negative() {
            msg!("Vcoin amount is too large!");
            return Err(PerpError::AmountTooLarge);
        }
        let add_pc_amount = (((v_coin_amount.abs() as u128) * (self.v_pc_amount as u128))
            / (final_v_coin as u128)) as u64;
        Ok(-v_coin_amount.signum() * (add_pc_amount as i64))
    }

    pub fn add_v_coin(&mut self, amount: i64) -> Result<(), PerpError> {
        let res = (self.v_coin_amount as i64)
            .checked_add(amount)
            .ok_or(PerpError::AmountTooLarge)?;
        if res.is_negative() {
            return Err(PerpError::AmountTooLarge);
        }
        self.v_coin_amount = res as u64;
        Ok(())
    }

    pub fn add_v_pc(&mut self, amount: i64) -> Result<(), PerpError> {
        let res = (self.v_pc_amount as i64)
            .checked_add(amount)
            .ok_or(PerpError::AmountTooLarge)?;
        if res.is_negative() {
            return Err(PerpError::AmountTooLarge);
        }
        self.v_pc_amount = res as u64;
        Ok(())
    }

    pub fn add_open_interest(
        &mut self,
        amount_v_coin: u64,
        amount_v_pc: u64,
        side: PositionType,
    ) -> Result<(), PerpError> {
        let (pt_v_coin, pt_v_pc) = match side {
            PositionType::Long => (&mut self.open_longs_v_coin, &mut self.open_longs_v_pc),
            PositionType::Short => (&mut self.open_shorts_v_coin, &mut self.open_shorts_v_pc),
        };
        pt_v_coin
            .checked_add(amount_v_coin)
            .map(|s| *pt_v_coin = s)
            .unwrap();
        pt_v_pc
            .checked_add(amount_v_pc)
            .map(|s| *pt_v_pc = s)
            .unwrap();
        Ok(())
    }

    pub fn sub_open_interest(
        &mut self,
        amount_v_coin: u64,
        amount_v_pc: u64,
        side: PositionType,
    ) -> Result<(), PerpError> {
        let (pt_v_coin, pt_v_pc) = match side {
            PositionType::Long => (&mut self.open_longs_v_coin, &mut self.open_longs_v_pc),
            PositionType::Short => (&mut self.open_shorts_v_coin, &mut self.open_shorts_v_pc),
        };
        pt_v_coin
            .checked_sub(amount_v_coin)
            .map(|s| *pt_v_coin = s)
            .unwrap();
        pt_v_pc
            .checked_sub(amount_v_pc)
            .map(|s| *pt_v_pc = s)
            .unwrap();
        Ok(())
    }

    pub fn balance_operation(
        &mut self,
        v_pc_to_add: i64,
        v_coin_to_add: i64,
        oracle_price: u64,
    ) -> Result<(i64, i64), ProgramError> {
        let side_sign = -v_coin_to_add.signum();
        let mut balanced_pc_to_add = v_pc_to_add;
        let mut balanced_v_coin_to_add = v_coin_to_add;
        let open_longs = self.open_longs_v_coin as i64;
        let open_shorts = self.open_shorts_v_coin as i64;
        let delta = open_longs - open_shorts;
        let current_market_bias =
            compute_bias(delta, self.v_coin_amount, self.v_pc_amount, oracle_price);

        if -side_sign * current_market_bias > REBALANCING_MARGIN {
            let mut rebalancing_contribution_v_coin;
            let mut rebalancing_contribution_pc = 0;
            if (side_sign * self.rebalanced_v_coin) < 0 {
                // Close the Vamm's opposite position
                rebalancing_contribution_v_coin = side_sign
                    * core::cmp::min(balanced_v_coin_to_add.abs(), self.rebalanced_v_coin.abs());
                balanced_v_coin_to_add += rebalancing_contribution_v_coin;
                balanced_pc_to_add = self.compute_add_v_pc(balanced_v_coin_to_add)?;
            } else {
                rebalancing_contribution_pc = core::cmp::min(
                    REBALANCING_LEVERAGE * self.rebalancing_funds,
                    v_pc_to_add.abs() as u64,
                );
                balanced_pc_to_add -= side_sign * (rebalancing_contribution_pc as i64);
                balanced_v_coin_to_add = self.compute_add_v_coin(balanced_pc_to_add)?;

                rebalancing_contribution_v_coin = v_coin_to_add - balanced_v_coin_to_add;
            }

            let updated_bias = compute_bias(
                delta - v_coin_to_add,
                ((self.v_coin_amount as i64) + balanced_v_coin_to_add) as u64,
                ((self.v_pc_amount as i64) + balanced_pc_to_add) as u64,
                oracle_price,
            );
            if -side_sign * updated_bias < REBALANCING_MARGIN {
                // To avoid overshooting the margin, which might induce market instability and fast depletion of rebalancing funds, we
                // cancel the rebalancing operation.
                rebalancing_contribution_pc = 0;
                rebalancing_contribution_v_coin = 0;
                balanced_pc_to_add = v_pc_to_add;
                balanced_v_coin_to_add = v_coin_to_add;
            } else {
                msg!("Rebalancing!");
            }

            self.rebalancing_funds -= rebalancing_contribution_pc / REBALANCING_LEVERAGE;
            self.rebalanced_v_coin += rebalancing_contribution_v_coin;
        };

        Ok((balanced_pc_to_add, balanced_v_coin_to_add))
    }

    pub fn apply_fees(
        &mut self,
        fees: &Fees,
        apply_refunds: bool,
        apply_allocation_fee: bool,
    ) -> Result<(), PerpError> {
        self.total_user_balances = self.total_user_balances.checked_sub(fees.fixed).unwrap();
        self.rebalancing_funds +=
            ((fees.fixed as u128) * (FEE_REBALANCING_FUND as u128) / 100) as u64 + 1;

        if apply_refunds {
            self.total_fee_balance = self.total_fee_balance.checked_sub(fees.refundable).unwrap();
            self.total_user_balances += fees.refundable;
        } else if apply_allocation_fee {
            self.total_fee_balance += ALLOCATION_FEE;
            self.total_user_balances = self
                .total_user_balances
                .checked_sub(ALLOCATION_FEE)
                .unwrap();
        }
        Ok(())
    }

    #[allow(clippy::clippy::too_many_arguments)]
    pub fn transfer_fees<'a>(
        &mut self,
        fees: &mut Fees,
        spl_token_program: &AccountInfo<'a>,
        market_account: &AccountInfo<'a>,
        market_vault_account: &AccountInfo<'a>,
        market_signer_account: &AccountInfo<'a>,
        bnb_bonfida: &AccountInfo<'a>,
        referrer_account_opt: Option<&AccountInfo<'a>>,
    ) -> ProgramResult {
        let mut buy_and_burn_fee =
            ((fees.fixed as u128) * (FEE_BUY_BURN_BONFIDA as u128) / 100) as u64;
        let referrer_fee = ((fees.fixed as u128) * (FEE_REFERRER as u128) / 100) as u64;
        if let Some(referrer_account) = referrer_account_opt {
            let instruction = transfer(
                &spl_token::id(),
                market_vault_account.key,
                referrer_account.key,
                market_signer_account.key,
                &[],
                referrer_fee,
            )?;
            invoke_signed(
                &instruction,
                &[
                    spl_token_program.clone(),
                    market_vault_account.clone(),
                    referrer_account.clone(),
                    market_signer_account.clone(),
                ],
                &[&[&market_account.key.to_bytes(), &[self.signer_nonce]]],
            )?;
        } else {
            // Referrer fee gets split between buy and burn and insurance fund when not specified
            buy_and_burn_fee += referrer_fee / 2;
        }
        let instruction = transfer(
            &spl_token::id(),
            market_vault_account.key,
            bnb_bonfida.key,
            market_signer_account.key,
            &[],
            buy_and_burn_fee,
        )?;
        invoke_signed(
            &instruction,
            &[
                spl_token_program.clone(),
                market_vault_account.clone(),
                bnb_bonfida.clone(),
                market_signer_account.clone(),
            ],
            &[&[&market_account.key.to_bytes(), &[self.signer_nonce]]],
        )?;
        Ok(())
    }

    pub fn get_insurance_fund(&self, market_vault_balance: u64) -> i64 {
        let delta = -self
            .compute_add_v_pc((self.open_longs_v_coin as i64) - (self.open_shorts_v_coin as i64))
            .unwrap();
        let total_payout = delta
            .checked_add(self.total_collateral as i64)
            .and_then(|s| s.checked_add(self.open_shorts_v_pc as i64))
            .and_then(|s| s.checked_sub(self.open_longs_v_pc as i64))
            .unwrap();
        let total_payout = std::cmp::max(0, total_payout) as u64;
        (market_vault_balance as i64)
            - (total_payout as i64)
            - (self.total_user_balances as i64)
            - (self.total_fee_balance as i64)
            - (self.rebalancing_funds as i64)
    }

    pub fn slippage_protection(
        &self,
        desired_mark_price: u64,
        slippage_margin: u64,
    ) -> Result<(), PerpError> {
        let current_mark_price =
            (((self.v_pc_amount as u128) << 32) / (self.v_coin_amount as u128)) as i64;
        let margin = (current_mark_price - (desired_mark_price as i64)).abs() as u64;
        if margin > slippage_margin {
            return Err(PerpError::NetworkSlippageTooLarge);
        }
        Ok(())
    }

    pub fn get_k(&self) -> u128 {
        (self.v_coin_amount as u128)
            .checked_mul(self.v_pc_amount as u128)
            .unwrap()
    }
}

// Getter and setter functions

pub fn get_instance_address(
    market_account_data: &[u8],
    instance_index: u32,
) -> Result<Pubkey, ProgramError> {
    let offset = (instance_index as usize)
        .checked_mul(32)
        .and_then(|s| s.checked_add(MarketState::LEN))
        .unwrap();
    let slice = market_account_data
        .get(offset..offset + 32)
        .ok_or(ProgramError::InvalidArgument)?;
    Ok(Pubkey::new(slice))
}

pub fn write_instance_address(
    market_account_data: &mut [u8],
    instance_index: u32,
    instance_address: &Pubkey,
) -> PerpResult {
    let offset = (instance_index as usize)
        .checked_mul(32)
        .and_then(|s| s.checked_add(MarketState::LEN))
        .unwrap();
    market_account_data
        .get_mut(offset..offset + 32)
        .ok_or(PerpError::OutOfSpace)?
        .copy_from_slice(&instance_address.to_bytes());
    Ok(())
}

// Struct used to store data about markets for monitoring purposes
#[derive(Debug)]
pub struct MarketDataPoint {
    pub total_collateral: u64,
    pub total_user_balances: u64,
    pub total_fee_balance: u64,
    pub rebalancing_funds: u64,
    pub rebalanced_v_coin: i64,
    pub v_coin_amount: u64,
    pub v_pc_amount: u64,
    pub open_shorts_v_coin: u64,
    pub open_longs_v_coin: u64,
    pub last_funding_timestamp: u64,
    pub last_recording_timestamp: u64,
    pub funding_samples_count: u8,
    pub funding_samples_sum: i64,
    pub funding_history_offset: u8,
    pub funding_history: [i64; 16],
    pub funding_balancing_factors: [u64; 16], // FP 32 measure of payment capping to ensure that the insurance fund does not pay funding.
    pub number_of_instances: u32,
    pub insurance_fund: i64,
    pub market_price: f64,
    pub oracle_price: f64,
    pub equilibrium_price: f64,
    pub gc_list_lengths: Vec<u64>,
    pub page_full_ratios: Vec<Vec<f64>>,
    pub longs_depths: Vec<u64>,
    pub shorts_depths: Vec<u64>,
}
