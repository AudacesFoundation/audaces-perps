use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::account_info::AccountInfo;

pub mod instance;
pub mod market;
pub mod user_account;

#[derive(BorshDeserialize, BorshSerialize)]
pub enum StateObject {
    Uninitialized,
    MarketState,
    UserAccount,
    MemoryPage,
    Instance,
}
pub fn is_initialized(account: &AccountInfo) -> bool {
    account.data.borrow()[0] != (StateObject::Uninitialized as u8)
}

#[derive(Debug)]
pub struct Fees {
    pub total: i64,      // In the case of a refund, the cummulated fees can be negative
    pub refundable: u64, // Allocation fee
    pub fixed: u64,      // = total - refundable = buy_and_burn + rebalancing + referrer,
}

#[cfg_attr(feature = "fuzz", derive(Arbitrary))]
#[derive(Clone, Copy, BorshSerialize, BorshDeserialize, Debug, PartialEq)]
pub enum PositionType {
    Short,
    Long,
}

impl PositionType {
    pub fn get_sign(&self) -> i64 {
        (2 * (*self as i64)) - 1
    }
}
