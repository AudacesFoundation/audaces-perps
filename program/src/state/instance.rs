use crate::positions_book::{memory::Pointer, positions_book_tree::PositionsBook};
use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    program_pack::{Pack, Sealed},
    pubkey::Pubkey,
};

use super::StateObject;

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct Instance {
    pub version: u8,
    pub shorts_pointer: Option<Pointer>,
    pub longs_pointer: Option<Pointer>,
    pub garbage_pointer: Option<Pointer>,
    pub number_of_pages: u32,
}

impl Instance {
    pub fn update(&mut self, book: &PositionsBook, page_infos: &mut Vec<PageInfo>) {
        self.shorts_pointer = book.shorts_root;
        self.longs_pointer = book.longs_root;
        self.garbage_pointer = book.memory.gc_list_hd;
        for (i, page) in page_infos.iter_mut().enumerate() {
            page.unitialized_memory_index = book.memory.pages[i].uninitialized_memory;
            page.free_slot_list_hd = book.memory.pages[i].free_slot_list_hd;
        }
    }
}

impl Sealed for Instance {}

impl Pack for Instance {
    const LEN: usize = 21;

    fn pack_into_slice(&self, dst: &mut [u8]) {
        dst[0] = StateObject::Instance as u8;
        let mut slice = &mut dst[1..];
        self.serialize(&mut slice).unwrap()
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        if src[0] != StateObject::Instance as u8 {
            if src[0] == 0 {
                return Err(ProgramError::UninitializedAccount);
            }
            return Err(ProgramError::InvalidAccountData);
        };
        Instance::deserialize(&mut &src[1..]).map_err(|_| {
            msg!("Failed to deserialize market account");
            ProgramError::InvalidAccountData
        })
    }
}

pub fn parse_instance(
    instance_account_data: &[u8],
) -> Result<(Instance, Vec<PageInfo>), ProgramError> {
    let header_slice = instance_account_data
        .get(0..Instance::LEN)
        .ok_or(ProgramError::InvalidAccountData)?;
    let instance = Instance::unpack_from_slice(header_slice)?;
    let mut offset = Instance::LEN;
    let mut pages = Vec::with_capacity(instance.number_of_pages as usize);
    for _ in 0..instance.number_of_pages {
        let next_offset = offset.checked_add(PageInfo::LEN).unwrap();
        let slice = instance_account_data
            .get(offset..next_offset)
            .ok_or(ProgramError::InvalidAccountData)?;
        let page = PageInfo::unpack_from_slice(slice)?;
        pages.push(page);
        offset = next_offset;
    }
    Ok((instance, pages))
}

pub fn write_instance(
    instance_account_data: &mut [u8],
    instance: &Instance,
) -> Result<(), ProgramError> {
    let header_slice = instance_account_data
        .get_mut(0..Instance::LEN)
        .ok_or(ProgramError::InvalidAccountData)?;
    instance.pack_into_slice(header_slice);
    Ok(())
}

pub fn write_page_info(
    instance_account_data: &mut [u8],
    page_index: usize,
    page_info: &PageInfo,
) -> Result<(), ProgramError> {
    let offset = page_index
        .checked_mul(PageInfo::LEN)
        .and_then(|s| s.checked_add(Instance::LEN))
        .unwrap();
    let offset_end = offset.checked_add(PageInfo::LEN).unwrap();
    let slice = instance_account_data
        .get_mut(offset..offset_end)
        .ok_or(ProgramError::InvalidAccountData)?;
    page_info.pack_into_slice(slice);
    Ok(())
}

pub fn write_instance_and_memory(
    instance_account_data: &mut [u8],
    page_infos: &[PageInfo],
    instance: &Instance,
) -> ProgramResult {
    write_instance(instance_account_data, instance)?;
    for (page_index, page) in page_infos.iter().enumerate() {
        write_page_info(instance_account_data, page_index, page)?;
    }
    Ok(())
}

#[derive(BorshDeserialize, BorshSerialize, Debug)]
pub struct PageInfo {
    pub address: [u8; 32],
    pub unitialized_memory_index: Pointer,
    pub free_slot_list_hd: Option<Pointer>,
}

impl PageInfo {
    pub fn new(account_address: &Pubkey) -> Self {
        Self {
            address: account_address.to_bytes(),
            unitialized_memory_index: 0,
            free_slot_list_hd: None,
        }
    }
}

impl Sealed for PageInfo {}

impl Pack for PageInfo {
    const LEN: usize = 41;

    fn pack_into_slice(&self, dst: &mut [u8]) {
        let mut p = dst;
        self.serialize(&mut p).unwrap();
    }

    fn unpack_from_slice(src: &[u8]) -> Result<Self, ProgramError> {
        let mut p = src;
        PageInfo::deserialize(&mut p).map_err(|_| {
            msg!("Failed to deserialize PageInfo in instance");
            ProgramError::InvalidAccountData
        })
    }
}
