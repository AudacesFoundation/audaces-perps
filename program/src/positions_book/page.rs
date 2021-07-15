use std::{cell::RefCell, convert::TryInto, rc::Rc};

use borsh::{BorshDeserialize, BorshSerialize};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use solana_program::{account_info::AccountInfo, program_error::ProgramError};

use crate::{
    error::{PerpError, PerpResult},
    state::{instance::PageInfo, StateObject},
};

use super::memory::{Pointer, SLOT_SIZE, TAG_SIZE};

pub struct Page<'a> {
    pub page_size: u32,
    pub data: Rc<RefCell<&'a mut [u8]>>,
    pub uninitialized_memory: Pointer,
    pub free_slot_list_hd: Option<Pointer>,
}

#[derive(FromPrimitive, Copy, Clone, PartialEq)]
#[repr(u8)]
pub enum SlotType {
    FreeSlot = 0,
    LastFreeSlot,
    InnerNode,
    LeafNode,
}

impl<'a> Page<'a> {
    pub fn new(account: &AccountInfo<'a>, page_info: &PageInfo) -> Result<Self, ProgramError> {
        let obj = {
            let mut buf: &[u8] = &account.data.borrow();
            StateObject::deserialize(&mut buf)?
        };
        match obj {
            StateObject::MemoryPage => {}
            StateObject::Uninitialized => {
                let mut p: &mut [u8] = &mut account.data.borrow_mut();
                StateObject::MemoryPage.serialize(&mut p)?;
            }
            _ => return Err(ProgramError::InvalidAccountData),
        }
        Ok(Page {
            page_size: ((account.data_len() - TAG_SIZE) / SLOT_SIZE) as u32,
            data: Rc::clone(&account.data),
            uninitialized_memory: page_info.unitialized_memory_index,
            free_slot_list_hd: page_info.free_slot_list_hd,
        })
    }

    #[cfg(not(target_arch = "bpf"))]
    pub fn new_from_slice_unchecked(
        account_data: &'a mut [u8],
        page_info: &PageInfo,
    ) -> Result<Self, ProgramError> {
        Ok(Page {
            page_size: ((account_data.len() - TAG_SIZE) / SLOT_SIZE) as u32,
            data: Rc::new(RefCell::new(account_data)),
            uninitialized_memory: page_info.unitialized_memory_index,
            free_slot_list_hd: page_info.free_slot_list_hd,
        })
    }

    pub fn free(&mut self, pointer: Pointer) -> PerpResult {
        let offset = TAG_SIZE + (pointer as usize) * SLOT_SIZE;
        let tag;
        match self.free_slot_list_hd {
            Some(pt) => {
                tag = SlotType::FreeSlot as u8;
                self.data
                    .borrow_mut()
                    .get_mut(offset + 1..offset + 5)
                    .ok_or(PerpError::MemoryError)?
                    .copy_from_slice(&pt.to_le_bytes());
            }
            None => {
                tag = SlotType::LastFreeSlot as u8;
            }
        }

        self.data.borrow_mut()[offset] = tag;
        self.free_slot_list_hd = Some(pointer);
        Ok(())
    }

    pub fn allocate(&mut self, slot_type: SlotType) -> Result<Pointer, PerpError> {
        let pointer: Pointer;
        let offset: usize;
        match self.free_slot_list_hd {
            Some(pt) => {
                offset = TAG_SIZE + (pt as usize) * SLOT_SIZE;
                match FromPrimitive::from_u8(*self.data.borrow().get(offset).unwrap()).unwrap() {
                    SlotType::FreeSlot => {
                        self.free_slot_list_hd = Some(u32::from_le_bytes(
                            self.data
                                .borrow()
                                .get(offset + 1..offset + 5)
                                .unwrap()
                                .try_into()
                                .unwrap(),
                        ))
                    }
                    SlotType::LastFreeSlot => self.free_slot_list_hd = None,
                    _ => unreachable!(),
                };
                pointer = pt;
            }
            None => {
                pointer = self.uninitialized_memory;
                offset = TAG_SIZE + (pointer as usize) * SLOT_SIZE;
                self.uninitialized_memory += 1;
                if self.uninitialized_memory > self.page_size {
                    return Err(PerpError::OutOfSpace);
                }
            }
        };
        *self.data.borrow_mut().get_mut(offset).unwrap() = slot_type as u8;
        Ok(pointer)
    }

    pub fn read(
        &self,
        pointer: Pointer,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, PerpError> {
        let mem_offset = TAG_SIZE + (pointer as usize) * SLOT_SIZE + offset;
        Ok(self.data.borrow()[mem_offset..mem_offset + length].to_vec())
    }

    pub fn read_byte(&self, pointer: Pointer, offset: usize) -> Result<u8, PerpError> {
        let mem_offset = TAG_SIZE + (pointer as usize) * SLOT_SIZE + offset;
        Ok(self.data.borrow()[mem_offset])
    }

    pub fn read_u64_be(&self, pointer: Pointer, offset: usize) -> Result<u64, PerpError> {
        let mem_offset = TAG_SIZE + (pointer as usize) * SLOT_SIZE + offset;
        Ok(u64::from_be_bytes(
            self.data.borrow()[mem_offset..mem_offset + 8]
                .try_into()
                .unwrap(),
        ))
    }

    pub fn read_u64_le(&self, pointer: Pointer, offset: usize) -> Result<u64, PerpError> {
        let mem_offset = TAG_SIZE + (pointer as usize) * SLOT_SIZE + offset;
        Ok(u64::from_le_bytes(
            self.data.borrow()[mem_offset..mem_offset + 8]
                .try_into()
                .unwrap(),
        ))
    }

    pub fn read_u32_le(&self, pointer: Pointer, offset: usize) -> Result<u32, PerpError> {
        let mem_offset = TAG_SIZE + (pointer as usize) * SLOT_SIZE + offset;
        Ok(u32::from_le_bytes(
            self.data.borrow()[mem_offset..mem_offset + 4]
                .try_into()
                .unwrap(),
        ))
    }

    pub fn read_u16_le(&self, pointer: Pointer, offset: usize) -> Result<u16, PerpError> {
        let mem_offset = TAG_SIZE + (pointer as usize) * SLOT_SIZE + offset;
        Ok(u16::from_le_bytes(
            self.data.borrow()[mem_offset..mem_offset + 2]
                .try_into()
                .unwrap(),
        ))
    }

    pub fn write(&mut self, pointer: Pointer, offset: usize, input: &[u8]) -> PerpResult {
        let mem_offset = TAG_SIZE + (pointer as usize) * SLOT_SIZE + offset;
        self.data.borrow_mut()[mem_offset..mem_offset + input.len()].copy_from_slice(&input);
        Ok(())
    }

    #[cfg(not(target_arch = "bpf"))]
    pub fn get_nb_free_slots(&self) -> Result<u64, PerpError> {
        let mut count = 0;

        if let Some(mut pointer) = self.free_slot_list_hd {
            let mut slot_type = SlotType::FreeSlot as usize;
            while slot_type == SlotType::FreeSlot as usize {
                slot_type = self.read_byte(pointer, 0)? as usize;
                pointer = self.read_u32_le(pointer, 1)?;
                count += 1;
            }
        }
        Ok(count)
    }
}

#[cfg(all(test, feature = "test-bpf"))]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::{Page, SlotType, SLOT_SIZE};
    use rand::{prelude::SliceRandom, thread_rng, Rng};

    #[test]
    fn page_test() {
        let data = &mut [0u8; 1024];
        let mut page = Page {
            page_size: (1024 / SLOT_SIZE) as u32,
            data: Rc::new(RefCell::new(data)),
            free_slot_list_hd: None,
            uninitialized_memory: 0,
        };
        let inner_node = page.allocate(SlotType::InnerNode).unwrap();
        let leaf = page.allocate(SlotType::LeafNode).unwrap();

        assert_eq!(
            page.read_byte(inner_node, 0).unwrap(),
            SlotType::InnerNode as u8
        );
        println!(
            "Uninitialized_memory : {:?}, leaf_pointer: {:?}",
            page.uninitialized_memory, leaf
        );
        assert_eq!(page.read_byte(leaf, 0).unwrap(), SlotType::LeafNode as u8);
        let mut rng = thread_rng();

        let mut nodes = vec![];

        for _ in 0..(page.page_size - 2) {
            let tp = match rng.gen_bool(0.5) {
                true => SlotType::InnerNode,
                false => SlotType::LeafNode,
            };
            let pt = page.allocate(tp).unwrap();
            nodes.push((pt, tp));
        }

        for (pt, tp) in &nodes {
            assert_eq!(page.read_byte(*pt, 0).unwrap(), *tp as u8);
        }
        let choice_iter = nodes.choose_multiple(&mut rng, 10);
        for (pt, _) in choice_iter {
            page.free(*pt).unwrap();
        }

        for _ in 0..10 {
            let tp = match rng.gen_bool(0.5) {
                true => SlotType::InnerNode,
                false => SlotType::LeafNode,
            };
            let pt = page.allocate(tp).unwrap();
            nodes.push((pt, tp));
        }

        for (pt, tp) in nodes[(page.page_size - 2) as usize..].iter() {
            assert_eq!(page.read_byte(*pt, 0).unwrap(), *tp as u8);
        }
    }
}
