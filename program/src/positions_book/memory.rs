use std::slice::Iter;

use num_traits::FromPrimitive;
use solana_program::account_info::{next_account_info, AccountInfo};
use solana_program::msg;
use solana_program::program_error::ProgramError;
use solana_program::pubkey::Pubkey;

use crate::error::{PerpError, PerpResult};
use crate::positions_book::page::SlotType;
use crate::state::instance::{Instance, PageInfo};

use super::page::Page;
use super::tree_nodes::InnerNodeSchema;

pub const SLOT_SIZE: usize = 47;
pub(super) const PAGE_MASK: u32 = 0xf << 28;
pub const TAG_SIZE: usize = 1;

pub type Pointer = u32;

pub enum GarbageNodeSchema {
    Critbit = InnerNodeSchema::Critbit as isize,
    LeftPointer = InnerNodeSchema::LeftPointer as isize,
    RightPointer = InnerNodeSchema::RightPointer as isize,
    IsLastToCollect = GarbageNodeSchema::RightPointer as isize + 4,
    PointerToNext = GarbageNodeSchema::IsLastToCollect as isize + 1,
}

pub struct Memory<'a> {
    pub pages: Vec<Page<'a>>,
    pub gc_list_hd: Option<Pointer>,
}

impl<'a> Memory<'a> {
    pub fn new(pages: Vec<Page<'a>>, gc_list_hd: Option<Pointer>) -> Self {
        Memory { pages, gc_list_hd }
    }

    pub fn crank_garbage_collector(&mut self, max_iterations: u64) -> Result<u64, PerpError> {
        let mut freed_slots = 0;
        for _ in 0..max_iterations {
            match self.gc_list_hd {
                Some(pt) => {
                    // Check if head of gc list is last to be collected
                    if self.read_byte(pt, GarbageNodeSchema::IsLastToCollect as usize)? == 0 {
                        self.gc_list_hd =
                            Some(self.read_u32_le(pt, GarbageNodeSchema::PointerToNext as usize)?)
                    } else {
                        self.gc_list_hd = None
                    }

                    // Collect head of gc list
                    match FromPrimitive::from_u8(self.read_byte(pt, 0)?).unwrap() {
                        SlotType::InnerNode => {
                            let left_pt =
                                self.read_u32_le(pt, GarbageNodeSchema::LeftPointer as usize)?;
                            let right_pt =
                                self.read_u32_le(pt, GarbageNodeSchema::RightPointer as usize)?;
                            self.flag_for_gc(left_pt)?;
                            self.flag_for_gc(right_pt)?;
                        }
                        SlotType::LeafNode => self.free(pt)?,
                        _ => unreachable!(),
                    }
                    freed_slots += 1;
                }
                None => break,
            }
        }
        Ok(freed_slots)
    }

    pub fn flag_for_gc(&mut self, pointer: Pointer) -> PerpResult {
        if let Some(p) = self.gc_list_hd {
            self.write(
                pointer,
                GarbageNodeSchema::PointerToNext as usize,
                &p.to_le_bytes(),
            )?;
            self.write(pointer, GarbageNodeSchema::IsLastToCollect as usize, &[0])?;
        } else {
            self.write(pointer, GarbageNodeSchema::IsLastToCollect as usize, &[1])?;
        }
        self.gc_list_hd = Some(pointer);
        Ok(())
    }

    pub fn read(
        &self,
        pointer: Pointer,
        offset: usize,
        length: usize,
    ) -> Result<Vec<u8>, PerpError> {
        let page_index = pointer >> 28;
        self.pages[page_index as usize].read(!PAGE_MASK & pointer, offset, length)
    }

    pub fn free(&mut self, pointer: Pointer) -> PerpResult {
        let page_index = pointer >> 28;
        self.pages[page_index as usize].free(!PAGE_MASK & pointer)
    }

    pub fn allocate(&mut self, slot_type: SlotType) -> Result<Pointer, PerpError> {
        for (i, page) in self.pages.iter_mut().enumerate() {
            if page.page_size != page.uninitialized_memory || page.free_slot_list_hd.is_some() {
                let page_index = (i as u32) << 28;
                return page.allocate(slot_type).map(|p| page_index | p);
            }
        }
        Err(PerpError::OutOfSpace)
    }

    pub fn read_byte(&self, pointer: Pointer, offset: usize) -> Result<u8, PerpError> {
        let page_index = pointer >> 28;
        self.pages[page_index as usize].read_byte(!PAGE_MASK & pointer, offset)
    }

    pub fn read_u64_be(&self, pointer: Pointer, offset: usize) -> Result<u64, PerpError> {
        let page_index = pointer >> 28;
        self.pages[page_index as usize].read_u64_be(!PAGE_MASK & pointer, offset)
    }

    pub fn read_u64_le(&self, pointer: Pointer, offset: usize) -> Result<u64, PerpError> {
        let page_index = pointer >> 28;
        self.pages[page_index as usize].read_u64_le(!PAGE_MASK & pointer, offset)
    }

    pub fn read_u32_le(&self, pointer: Pointer, offset: usize) -> Result<u32, PerpError> {
        let page_index = pointer >> 28;
        self.pages[page_index as usize].read_u32_le(!PAGE_MASK & pointer, offset)
    }

    pub fn read_u16_le(&self, pointer: Pointer, offset: usize) -> Result<u16, PerpError> {
        let page_index = pointer >> 28;
        self.pages[page_index as usize].read_u16_le(!PAGE_MASK & pointer, offset)
    }

    pub fn write(&mut self, pointer: Pointer, offset: usize, input: &[u8]) -> PerpResult {
        let page_index = pointer >> 28;
        self.pages[page_index as usize].write(!PAGE_MASK & pointer, offset, input)
    }

    #[cfg(not(target_arch = "bpf"))]
    pub fn get_gc_list_len(&self) -> Result<u64, PerpError> {
        let mut count = 0;
        if let Some(mut pointer) = self.gc_list_hd {
            let mut is_last = false;
            while !is_last {
                is_last =
                    self.read_byte(pointer, GarbageNodeSchema::IsLastToCollect as usize)? == 1;
                pointer = self.read_u32_le(pointer, GarbageNodeSchema::PointerToNext as usize)?;
                count += 1;
            }
        }
        Ok(count)
    }
}

pub fn parse_memory<'a>(
    instance: &Instance,
    pages_infos: &[PageInfo],
    accounts_iter: &mut Iter<AccountInfo<'a>>,
) -> Result<Memory<'a>, ProgramError> {
    let mut pages = vec![];
    for page_info in pages_infos {
        let account = next_account_info(accounts_iter)?;
        if account.key != &Pubkey::new(&page_info.address) {
            msg!("An invalid memory page was provided");
            return Err(ProgramError::InvalidArgument);
        }
        pages.push(Page::new(account, page_info)?);
    }
    Ok(Memory::new(pages, instance.garbage_pointer))
}

#[cfg(all(test, feature = "test-bpf"))]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::{Page, SlotType, SLOT_SIZE};
    use rand::{prelude::SliceRandom, thread_rng, Rng};

    #[cfg(not(feature = "test-bpf"))]
    #[test]
    fn memory_test() {
        let mut data0 = [0u8; 1024];
        let mut data1 = [0u8; 1024];
        let mut data2 = [0u8; 1024];
        let mut data3 = [0u8; 1024];
        let data: Vec<Rc<RefCell<&mut [u8]>>> = vec![
            Rc::new(RefCell::new(&mut data0)),
            Rc::new(RefCell::new(&mut data1)),
            Rc::new(RefCell::new(&mut data2)),
            Rc::new(RefCell::new(&mut data3)),
        ];
        let mut pages = vec![];
        let page_size = (1024 / SLOT_SIZE) as u32;
        println!("Page size: {:?}", page_size);
        for i in 0..4 {
            pages.push(Page {
                page_size: page_size,
                data: Rc::clone(&data[i]),
                free_slot_list_hd: None,
                uninitialized_memory: 0,
            });
        }

        let mut mem = Memory {
            pages: pages,
            gc_list_hd: None,
        };

        let mut rng = thread_rng();

        let mut nodes = vec![];

        for _ in 0..(4 * page_size - 2) {
            let tp = match rng.gen_bool(0.5) {
                true => SlotType::InnerNode,
                false => SlotType::LeafNode,
            };
            let mut test_array = vec![];
            for i in 0..17 {
                test_array.push(rng.next_u32() as u8);
            }
            let test_node = TestNode {
                typ: tp,
                test_u32: rng.next_u32(),
                test_u64: rng.next_u64(),
                test_u16: rng.next_u32() as u16,
                test_byte: rng.next_u32() as u8,
                test_array: test_array,
            };
            let pt = mem.allocate(tp).unwrap();
            mem.write(pt, 1, &test_node.test_u32.to_le_bytes()).unwrap();
            mem.write(pt, 5, &test_node.test_u64.to_le_bytes()).unwrap();
            mem.write(pt, 13, &test_node.test_u16.to_le_bytes())
                .unwrap();
            mem.write(pt, 15, &[test_node.test_byte]).unwrap();
            mem.write(pt, 16, &test_node.test_array).unwrap();
            nodes.push((pt, test_node));
        }

        println!("Length: {:?}", nodes.len());

        for (pt, tp) in &nodes {
            assert_eq!(
                mem.read_byte(*pt, 0).unwrap(),
                tp.typ as u8,
                "with pt: {:?} and page_index {:?}",
                pt,
                ((0xf << 28) & pt) >> 28
            );
            assert_eq!(
                mem.read_u32_le(*pt, 1).unwrap(),
                tp.test_u32,
                "with pt: {:?} and page_index {:?}",
                pt,
                ((0xf << 28) & pt) >> 28
            );
            assert_eq!(
                mem.read_u64_le(*pt, 5).unwrap(),
                tp.test_u64,
                "with pt: {:?} and page_index {:?}",
                pt,
                ((0xf << 28) & pt) >> 28
            );
            assert_eq!(
                mem.read_u16_le(*pt, 13).unwrap(),
                tp.test_u16,
                "with pt: {:?} and page_index {:?}",
                pt,
                ((0xf << 28) & pt) >> 28
            );
            assert_eq!(
                mem.read_byte(*pt, 15).unwrap(),
                tp.test_byte,
                "with pt: {:?} and page_index {:?}",
                pt,
                ((0xf << 28) & pt) >> 28
            );
            assert_eq!(
                mem.read(*pt, 16, 17).unwrap(),
                tp.test_array,
                "with pt: {:?} and page_index {:?}",
                pt,
                ((0xf << 28) & pt) >> 28
            );
        }
        let choice_iter = nodes.choose_multiple(&mut rng, 10);
        println!("Length: {:?}", choice_iter.len());
        for (pt, _) in choice_iter {
            mem.free(*pt).unwrap();
        }

        for _ in 0..10 {
            let tp = match rng.gen_bool(0.5) {
                true => SlotType::InnerNode,
                false => SlotType::LeafNode,
            };
            let mut test_array = vec![];
            for i in 0..17 {
                test_array.push(rng.next_u32() as u8);
            }
            let test_node = TestNode {
                typ: tp,
                test_u32: rng.next_u32(),
                test_u64: rng.next_u64(),
                test_u16: rng.next_u32() as u16,
                test_byte: rng.next_u32() as u8,
                test_array: test_array,
            };
            let pt = mem.allocate(tp).unwrap();
            mem.write(pt, 1, &test_node.test_u32.to_le_bytes()).unwrap();
            mem.write(pt, 5, &test_node.test_u64.to_le_bytes()).unwrap();
            mem.write(pt, 13, &test_node.test_u16.to_le_bytes())
                .unwrap();
            mem.write(pt, 15, &[test_node.test_byte]).unwrap();
            mem.write(pt, 16, &test_node.test_array).unwrap();
            nodes.push((pt, test_node));
        }

        for i in (4 * page_size - 2) as usize..nodes.len() {
            let (pt, tp) = &nodes[i];
            assert_eq!(
                mem.read_byte(*pt, 0).unwrap(),
                tp.typ as u8,
                "with pt: {:?} and page_index {:?}",
                pt,
                ((0xf << 28) & pt) >> 28
            );
            assert_eq!(mem.read_u32_le(*pt, 1).unwrap(), tp.test_u32);
            assert_eq!(mem.read_u64_le(*pt, 5).unwrap(), tp.test_u64);
            assert_eq!(mem.read_u16_le(*pt, 13).unwrap(), tp.test_u16);
            assert_eq!(mem.read_byte(*pt, 15).unwrap(), tp.test_byte);
            assert_eq!(mem.read(*pt, 16, 17).unwrap(), tp.test_array);
        }
    }
}
