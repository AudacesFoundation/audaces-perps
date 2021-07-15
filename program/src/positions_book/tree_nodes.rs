use crate::error::{PerpError, PerpResult};

use super::memory::{Memory, Pointer};

#[derive(Clone, Copy)]
pub enum InnerNodeSchema {
    Critbit = 1, // Critbit from the right
    LiquidationIndexMin = 2,
    LeftPointer = 10,
    RightPointer = 14,
    Collateral = 22,
    VCoin = 30,
    VPc = 38,
    CalculationFlag = 46, //0 is correct
}

pub enum LeafNodeSchema {
    LiquidationIndex = 1,
    SlotNumber = 9,
    Collateral = 17,
    VCoin = 25,
    VPc = 33,
}

pub struct InnerNode(pub(super) Pointer);

pub struct Leaf(pub(super) Pointer);

pub enum Node {
    InnerNode(InnerNode),
    Leaf(Leaf),
}

impl InnerNode {
    pub(super) fn get_critbit(&self, mem: &Memory) -> Result<u8, PerpError> {
        mem.read_byte(self.0, InnerNodeSchema::Critbit as usize)
    }

    pub(super) fn get_liquidation_index_min_max(
        &self,
        critbit: u8,
        mem: &Memory,
    ) -> Result<(u64, u64), PerpError> {
        let min = mem.read_u64_le(self.0, InnerNodeSchema::LiquidationIndexMin as usize)?;
        let max = min | ((2u64 << critbit) - 1);
        Ok((min, max))
    }
}

impl Node {
    pub(super) fn get_collateral(&self, mem: &Memory) -> Result<u64, PerpError> {
        match self {
            Node::InnerNode(i) => i.get_collateral(mem),
            Node::Leaf(i) => i.get_collateral(mem),
        }
    }
    pub(super) fn get_v_coin(&self, mem: &Memory) -> Result<u64, PerpError> {
        match self {
            Node::InnerNode(i) => i.get_v_coin(mem),
            Node::Leaf(i) => i.get_v_coin(mem),
        }
    }
    pub(super) fn get_v_pc(&self, mem: &Memory) -> Result<u64, PerpError> {
        match self {
            Node::InnerNode(i) => i.get_v_pc(mem),
            Node::Leaf(i) => i.get_v_pc(mem),
        }
    }

    pub(super) fn free(&self, mem: &mut Memory) -> PerpResult {
        match self {
            Node::InnerNode(i) => i.free(mem),
            Node::Leaf(i) => i.free(mem),
        }
    }
}

impl InnerNode {
    pub(super) fn get_collateral(&self, mem: &Memory) -> Result<u64, PerpError> {
        mem.read_u64_le(self.0, InnerNodeSchema::Collateral as usize)
    }

    pub(super) fn get_v_pc(&self, mem: &Memory) -> Result<u64, PerpError> {
        mem.read_u64_le(self.0, InnerNodeSchema::VPc as usize)
    }

    pub(super) fn get_v_coin(&self, mem: &Memory) -> Result<u64, PerpError> {
        mem.read_u64_le(self.0, InnerNodeSchema::VCoin as usize)
    }

    pub(super) fn set_collateral(&self, mem: &mut Memory, collateral: &u64) -> PerpResult {
        mem.write(
            self.0,
            InnerNodeSchema::Collateral as usize,
            &collateral.to_le_bytes(),
        )
    }

    pub(super) fn set_v_coin(&self, mem: &mut Memory, v_coin: &u64) -> PerpResult {
        mem.write(
            self.0,
            InnerNodeSchema::VCoin as usize,
            &v_coin.to_le_bytes(),
        )
    }

    pub(super) fn set_v_pc(&self, mem: &mut Memory, v_pc: &u64) -> PerpResult {
        mem.write(self.0, InnerNodeSchema::VPc as usize, &v_pc.to_le_bytes())
    }

    pub(super) fn free(&self, mem: &mut Memory) -> PerpResult {
        mem.flag_for_gc(self.0)
    }
}

impl Leaf {
    pub fn get_collateral(&self, mem: &Memory) -> Result<u64, PerpError> {
        mem.read_u64_le(self.0, LeafNodeSchema::Collateral as usize)
    }

    pub fn get_slot_number(&self, mem: &Memory) -> Result<u64, PerpError> {
        mem.read_u64_le(self.0, LeafNodeSchema::SlotNumber as usize)
    }

    pub(super) fn get_v_coin(&self, mem: &Memory) -> Result<u64, PerpError> {
        mem.read_u64_le(self.0, LeafNodeSchema::VCoin as usize)
    }
    pub(super) fn get_v_pc(&self, mem: &Memory) -> Result<u64, PerpError> {
        mem.read_u64_le(self.0, LeafNodeSchema::VPc as usize)
    }

    pub(super) fn get_slot(&self, mem: &Memory) -> Result<u64, PerpError> {
        mem.read_u64_le(self.0, LeafNodeSchema::SlotNumber as usize)
    }

    pub(super) fn set_collateral(&self, mem: &mut Memory, collateral: &u64) -> PerpResult {
        mem.write(
            self.0,
            LeafNodeSchema::Collateral as usize,
            &collateral.to_le_bytes(),
        )
    }

    pub(super) fn set_v_coin(&self, mem: &mut Memory, v_coin: &u64) -> PerpResult {
        mem.write(
            self.0,
            LeafNodeSchema::VCoin as usize,
            &v_coin.to_le_bytes(),
        )
    }

    pub(super) fn set_v_pc(&self, mem: &mut Memory, v_pc: &u64) -> PerpResult {
        mem.write(self.0, LeafNodeSchema::VPc as usize, &v_pc.to_le_bytes())
    }

    pub(super) fn get_liquidation_index(&self, mem: &Memory) -> Result<u64, PerpError> {
        mem.read_u64_le(self.0, LeafNodeSchema::LiquidationIndex as usize)
    }

    pub(super) fn free(&self, mem: &mut Memory) -> PerpResult {
        mem.free(self.0)
    }
}
