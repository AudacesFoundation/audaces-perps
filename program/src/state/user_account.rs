use crate::{error::PerpError, processor::MAX_OPEN_POSITONS_PER_USER, state::PositionType};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::{Pack, Sealed},
};

use super::StateObject;

// Pubkeys are stored as [u8; 32] for use with borsh

#[derive(BorshDeserialize, BorshSerialize, Clone, Debug)]
pub struct OpenPosition {
    pub last_funding_offset: u8,
    pub instance_index: u8,
    pub side: PositionType,
    pub liquidation_index: u64,
    pub collateral: u64,
    pub slot_number: u64,
    pub v_coin_amount: u64,
    pub v_pc_amount: u64,
}

impl OpenPosition {
    pub const INSTANCE_INDEX_OFFSET: usize = 1;
}

#[derive(BorshDeserialize, BorshSerialize, Clone, Copy, PartialEq, Debug)]
pub enum PositionState {
    Inactive,
    Active,
}

impl Sealed for OpenPosition {}

impl Pack for OpenPosition {
    const LEN: usize = 43;

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let mut p = dst;
        self.serialize(&mut p).unwrap();
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let mut p = src;
        OpenPosition::deserialize(&mut p).map_err(|_| {
            msg!("Failed to deserialize Useraccount position");
            ProgramError::InvalidAccountData
        })
    }
}

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct UserAccountState {
    pub version: u8,
    pub owner: [u8; 32],
    pub active: bool,
    pub market: [u8; 32],
    pub balance: u64,
    pub last_funding_offset: u8,
    pub number_of_open_positions: u32,
}

impl Sealed for UserAccountState {}

impl Pack for UserAccountState {
    const LEN: usize = 80;

    fn pack_into_slice(&self, dst: &mut [u8]) {
        dst[0] = StateObject::UserAccount as u8;
        self.serialize(&mut &mut dst[1..]).unwrap();
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src[0] != StateObject::UserAccount as u8 {
            if src[0] == 0 {
                return Err(ProgramError::UninitializedAccount);
            }
            return Err(ProgramError::InvalidAccountData);
        };
        UserAccountState::deserialize(&mut &src[1..]).map_err(|_| {
            msg!("Failed to deserialize user account");
            ProgramError::InvalidAccountData
        })
    }
}

impl UserAccountState {
    pub fn is_initialized(&self) -> bool {
        self.owner != [0u8; 32]
    }
}

pub fn write_position(
    user_account_data: &mut [u8],
    position_index: u16,
    user_account_header: &mut UserAccountState,
    position: &OpenPosition,
    overwrite: bool,
) -> ProgramResult {
    let offset = (position_index as usize)
        .checked_mul(OpenPosition::LEN)
        .and_then(|s| s.checked_add(UserAccountState::LEN))
        .unwrap();
    let offset_end = offset.checked_add(OpenPosition::LEN).unwrap();
    let slice = user_account_data
        .get_mut(offset..offset_end)
        .ok_or(PerpError::OutOfSpace)?;
    if (!overwrite)
        && (slice[0] == (PositionState::Active as u8))
        && ((position_index as i32) < (user_account_header.number_of_open_positions as i32) - 1)
    {
        msg!("A position already exists at the supplied position index!");
        return Err(ProgramError::InvalidArgument);
    }
    if (position_index as i32) > (user_account_header.number_of_open_positions as i32) - 1 {
        if user_account_header.number_of_open_positions > MAX_OPEN_POSITONS_PER_USER - 1 {
            return Err(PerpError::TooManyOpenPositions.into());
        }
        user_account_header.number_of_open_positions += 1;
        user_account_header.active = true;
    }
    position.pack_into_slice(slice);
    Ok(())
}

pub fn remove_position(
    user_account_data: &mut [u8],
    user_account_header: &mut UserAccountState,
    position_index: u32,
) -> ProgramResult {
    if user_account_header.number_of_open_positions == 0 {
        msg!("There are no positions that can be removed.");
        return Err(PerpError::PositionNotFound.into());
    }
    let last_index = user_account_header.number_of_open_positions - 1;
    if position_index != last_index {
        let last_position =
            get_position(user_account_data, user_account_header, last_index as u16)?;
        msg!(
            "Remapping position {:?} to {:?}",
            last_index,
            position_index
        );
        write_position(
            user_account_data,
            position_index as u16,
            user_account_header,
            &last_position,
            true,
        )?;
    }
    user_account_header.number_of_open_positions -= 1;
    if user_account_header.number_of_open_positions == 0 {
        user_account_header.active = false;
    }
    Ok(())
}

pub fn get_position(
    user_account_data: &mut [u8],
    user_account_header: &UserAccountState,
    position_index: u16,
) -> Result<OpenPosition, ProgramError> {
    if (user_account_header.number_of_open_positions as i32) - 1 < (position_index as i32) {
        msg!("The given position index is too large.");
        return Err(PerpError::PositionNotFound.into());
    }
    let offset = (position_index as usize)
        .checked_mul(OpenPosition::LEN)
        .and_then(|s| s.checked_add(UserAccountState::LEN))
        .unwrap();
    let offset_end = offset.checked_add(OpenPosition::LEN).unwrap();

    let slice = user_account_data
        .get_mut(offset..offset_end)
        .ok_or(ProgramError::InvalidArgument)?;
    OpenPosition::unpack_unchecked(slice)
}
