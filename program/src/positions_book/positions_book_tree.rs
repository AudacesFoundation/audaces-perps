use crate::{
    error::{PerpError, PerpResult},
    positions_book::{
        memory::{Memory, Pointer},
        page::SlotType,
        tree_nodes::{InnerNode, InnerNodeSchema, Leaf, Node},
    },
    state::PositionType,
    utils::compute_liquidation_index_inverse,
};
#[cfg(feature = "fuzz")]
use arbitrary::Arbitrary;
use num_traits::FromPrimitive;

use super::tree_nodes::LeafNodeSchema;

pub struct PositionsBook<'a> {
    pub shorts_root: Option<u32>,
    pub longs_root: Option<u32>,
    pub memory: Memory<'a>,
}

impl<'a> PositionsBook<'a> {
    pub fn new(shorts_root: Option<u32>, longs_root: Option<u32>, memory: Memory<'a>) -> Self {
        PositionsBook {
            shorts_root,
            longs_root,
            memory,
        }
    }
}

impl<'a> PositionsBook<'a> {
    pub fn get_collateral(&self) -> Result<u64, PerpError> {
        let longs_collateral = self
            .longs_root
            .map(|n| self.get_node(n)?.get_collateral(&self.memory))
            .unwrap_or(Ok(0))?;
        let shorts_collateral = self
            .shorts_root
            .map(|n| self.get_node(n)?.get_collateral(&self.memory))
            .unwrap_or(Ok(0))?;
        Ok(longs_collateral + shorts_collateral)
    }

    pub fn get_v_coin(&self) -> Result<(u64, u64), PerpError> {
        let longs_v_coin = self
            .longs_root
            .map(|n| self.get_node(n)?.get_v_coin(&self.memory))
            .unwrap_or(Ok(0))?;
        let shorts_v_coin = self
            .shorts_root
            .map(|n| self.get_node(n)?.get_v_coin(&self.memory))
            .unwrap_or(Ok(0))?;
        Ok((longs_v_coin, shorts_v_coin))
    }

    pub fn get_v_pc(&self) -> Result<(u64, u64), PerpError> {
        let longs_v_pc = self
            .longs_root
            .map(|n| self.get_node(n)?.get_v_pc(&self.memory))
            .unwrap_or(Ok(0))?;
        let shorts_v_pc = self
            .shorts_root
            .map(|n| self.get_node(n)?.get_v_pc(&self.memory))
            .unwrap_or(Ok(0))?;
        Ok((longs_v_pc, shorts_v_pc))
    }

    fn walk(
        &self,
        pt: Pointer,
        liquidation_index: &u64,
        critbit: &u8,
    ) -> Result<(bool, InnerNodeSchema, Pointer, Pointer), PerpError> {
        let direction = liquidation_index & (1u64 << critbit) == 0;
        let sibling_pt;
        let next_pt;
        let next_offset;
        match direction {
            true => {
                next_offset = InnerNodeSchema::LeftPointer;
                next_pt = self
                    .memory
                    .read_u32_le(pt, InnerNodeSchema::LeftPointer as usize)?;
                sibling_pt = self
                    .memory
                    .read_u32_le(pt, InnerNodeSchema::RightPointer as usize)?;
            }
            false => {
                next_offset = InnerNodeSchema::RightPointer;
                next_pt = self
                    .memory
                    .read_u32_le(pt, InnerNodeSchema::RightPointer as usize)?;
                sibling_pt = self
                    .memory
                    .read_u32_le(pt, InnerNodeSchema::LeftPointer as usize)?;
            }
        };
        Ok((direction, next_offset, next_pt, sibling_pt))
    }

    pub fn get_node(&self, pt: Pointer) -> Result<Node, PerpError> {
        let tag = self.memory.read_byte(pt, 0)?;
        match FromPrimitive::from_u8(tag).unwrap() {
            SlotType::InnerNode => Ok(Node::InnerNode(InnerNode(pt))),
            SlotType::LeafNode => Ok(Node::Leaf(Leaf(pt))),
            _ => unreachable!(),
        }
    }

    pub fn set_root(&mut self, pt: Option<Pointer>, position_type: PositionType) {
        match position_type {
            PositionType::Long => self.longs_root = pt,
            PositionType::Short => self.shorts_root = pt,
        }
    }

    pub fn remove_node(
        &mut self,
        pt: Pointer,
        position_type: PositionType,
        mother_pt: Option<Pointer>,
        grandmother_pt: Option<Pointer>,
        mother_offset: Option<InnerNodeSchema>,
        grandmother_offset: Option<InnerNodeSchema>,
    ) -> PerpResult {
        if let Some(m_pt) = mother_pt {
            let sibling_offset = match mother_offset.unwrap() {
                InnerNodeSchema::LeftPointer => InnerNodeSchema::RightPointer,
                InnerNodeSchema::RightPointer => InnerNodeSchema::LeftPointer,
                _ => unreachable!(),
            };
            let sibling_pt = self.memory.read_u32_le(m_pt, sibling_offset as usize)?;
            match grandmother_pt {
                Some(gm_pt) => {
                    self.memory.write(
                        gm_pt,
                        grandmother_offset.unwrap() as usize,
                        &sibling_pt.to_le_bytes(),
                    )?;
                }
                None => {
                    self.set_root(Some(sibling_pt), position_type);
                }
            }
            self.memory.free(m_pt)?;
        } else {
            self.set_root(None, position_type);
        }
        self.get_node(pt)?.free(&mut self.memory)?;
        Ok(())
    }

    fn write_leaf(
        &mut self,
        liquidation_index: u64,
        slot_number: u64,
        collateral: u64,
        v_coin: u64,
        v_pc: u64,
    ) -> Result<Pointer, PerpError> {
        let pt = self.memory.allocate(SlotType::LeafNode)?;
        self.memory.write(
            pt,
            LeafNodeSchema::Collateral as usize,
            &collateral.to_le_bytes(),
        )?;
        self.memory
            .write(pt, LeafNodeSchema::VCoin as usize, &v_coin.to_le_bytes())?;
        self.memory.write(
            pt,
            LeafNodeSchema::SlotNumber as usize,
            &slot_number.to_le_bytes(),
        )?;
        self.memory.write(
            pt,
            LeafNodeSchema::LiquidationIndex as usize,
            &liquidation_index.to_le_bytes(),
        )?;
        self.memory
            .write(pt, LeafNodeSchema::VPc as usize, &v_pc.to_le_bytes())?;
        Ok(pt)
    }

    #[allow(clippy::clippy::too_many_arguments)]
    fn write_inner(
        &mut self,
        critbit: u8,
        liquidation_index_min: u64,
        left_pt: Pointer,
        right_pt: Pointer,
        collateral: u64,
        v_coin: u64,
        v_pc: u64,
    ) -> Result<Pointer, PerpError> {
        let pt = self.memory.allocate(SlotType::InnerNode)?;
        self.memory
            .write(pt, InnerNodeSchema::Critbit as usize, &[critbit])?;
        self.memory.write(
            pt,
            InnerNodeSchema::LiquidationIndexMin as usize,
            &liquidation_index_min.to_le_bytes(),
        )?;
        self.memory.write(
            pt,
            InnerNodeSchema::LeftPointer as usize,
            &left_pt.to_le_bytes(),
        )?;
        self.memory.write(
            pt,
            InnerNodeSchema::RightPointer as usize,
            &right_pt.to_le_bytes(),
        )?;
        self.memory.write(
            pt,
            InnerNodeSchema::Collateral as usize,
            &collateral.to_le_bytes(),
        )?;
        self.memory
            .write(pt, InnerNodeSchema::VCoin as usize, &v_coin.to_le_bytes())?;
        self.memory
            .write(pt, InnerNodeSchema::VPc as usize, &v_pc.to_le_bytes())?;
        self.memory
            .write(pt, InnerNodeSchema::CalculationFlag as usize, &[0])?;
        Ok(pt)
    }

    pub fn liquidate(&mut self, liquidation_index: u64, position_type: PositionType) -> PerpResult {
        let (root, is_short) = match position_type {
            PositionType::Short => (self.shorts_root, true),
            PositionType::Long => (self.longs_root, false),
        };
        if root.is_none() {
            println!("Early return");
            return Ok(());
        }
        let mut pt = root.unwrap();
        let mut collateral_to_liquidate = 0;
        let mut v_coin_to_liquidate = 0;
        let mut v_pc_to_liquidate = 0;

        loop {
            match self.get_node(pt)? {
                Node::InnerNode(inner_node) => {
                    let critbit = inner_node.get_critbit(&self.memory)?;
                    let (liq_index_min, liq_index_max) =
                        inner_node.get_liquidation_index_min_max(critbit, &self.memory)?;
                    println!("On Inner node : critbit {:#4x}", 1u64 << critbit);
                    if liquidation_index > liq_index_max || liquidation_index < liq_index_min {
                        if is_short ^ (liquidation_index < liq_index_min) {
                            // The walk ends here; Liquidate current pt
                            println!("Liquidating current node");
                            collateral_to_liquidate += inner_node.get_collateral(&self.memory)?;
                            v_coin_to_liquidate += inner_node.get_v_coin(&self.memory)?;
                            v_pc_to_liquidate += inner_node.get_v_pc(&self.memory)?;
                            pt = root.unwrap();
                            let liquidation_critbit = critbit;
                            let mut mother_pt = None;
                            let mut mother_offset = None;
                            let mut grandmother_pt = None;
                            let mut grandmother_offset = None;
                            loop {
                                match self.get_node(pt)? {
                                    Node::InnerNode(inner_node) => {
                                        let critbit = inner_node.get_critbit(&self.memory)?;
                                        if liquidation_critbit == critbit {
                                            self.remove_node(
                                                pt,
                                                position_type,
                                                mother_pt,
                                                grandmother_pt,
                                                mother_offset,
                                                grandmother_offset,
                                            )?;
                                            break;
                                        }
                                        let current_collateral =
                                            inner_node.get_collateral(&self.memory)?;
                                        let current_v_coin = inner_node.get_v_coin(&self.memory)?;
                                        let current_v_pc = inner_node.get_v_pc(&self.memory)?;
                                        inner_node.set_collateral(
                                            &mut self.memory,
                                            &(current_collateral - collateral_to_liquidate),
                                        )?;
                                        inner_node.set_v_coin(
                                            &mut self.memory,
                                            &(current_v_coin - v_coin_to_liquidate),
                                        )?;
                                        inner_node.set_v_pc(
                                            &mut self.memory,
                                            &(current_v_pc - v_pc_to_liquidate),
                                        )?;
                                        let (direction, next_offset, next_pt, sibling_pt) =
                                            self.walk(pt, &liquidation_index, &critbit)?;

                                        if direction ^ is_short {
                                            let sibling_node = self.get_node(sibling_pt)?;
                                            collateral_to_liquidate -=
                                                sibling_node.get_collateral(&self.memory)?;
                                            v_coin_to_liquidate -=
                                                sibling_node.get_v_coin(&self.memory)?;
                                            v_pc_to_liquidate -=
                                                sibling_node.get_v_pc(&self.memory)?;
                                            match mother_pt {
                                                Some(m_pt) => {
                                                    self.memory.write(
                                                        m_pt,
                                                        mother_offset.unwrap() as usize,
                                                        &next_pt.to_le_bytes(),
                                                    )?;
                                                    self.memory.free(pt)?;
                                                }
                                                None => {
                                                    self.memory.free(pt)?;
                                                    self.set_root(Some(next_pt), position_type);
                                                }
                                            }
                                            sibling_node.free(&mut self.memory)?;
                                            pt = next_pt;
                                        } else {
                                            grandmother_pt = mother_pt;
                                            grandmother_offset = mother_offset;
                                            mother_pt = Some(pt);
                                            pt = next_pt;
                                            mother_offset = Some(next_offset);
                                        }
                                    }
                                    _ => unreachable!(),
                                }
                            }
                        } else {
                            pt = root.unwrap();
                            let mut mother_pt = None;
                            let mut mother_offset = None;
                            while collateral_to_liquidate != 0 {
                                match self.get_node(pt)? {
                                    Node::InnerNode(inner_node) => {
                                        let critbit = inner_node.get_critbit(&self.memory)?;
                                        let (direction, next_offset, next_pt, sibling_pt) =
                                            self.walk(pt, &liquidation_index, &critbit)?;
                                        let current_collateral =
                                            inner_node.get_collateral(&self.memory)?;
                                        let current_v_coin = inner_node.get_v_coin(&self.memory)?;
                                        let current_v_pc = inner_node.get_v_pc(&self.memory)?;
                                        inner_node.set_collateral(
                                            &mut self.memory,
                                            &current_collateral
                                                .checked_sub(collateral_to_liquidate)
                                                .unwrap(),
                                        )?;
                                        inner_node.set_v_coin(
                                            &mut self.memory,
                                            &current_v_coin
                                                .checked_sub(v_coin_to_liquidate)
                                                .unwrap(),
                                        )?;
                                        inner_node.set_v_pc(
                                            &mut self.memory,
                                            &current_v_pc.checked_sub(v_pc_to_liquidate).unwrap(),
                                        )?;

                                        if direction ^ is_short {
                                            let sibling_node = self.get_node(sibling_pt)?;
                                            collateral_to_liquidate -=
                                                sibling_node.get_collateral(&self.memory)?;
                                            v_coin_to_liquidate -=
                                                sibling_node.get_v_coin(&self.memory)?;
                                            v_pc_to_liquidate -=
                                                sibling_node.get_v_pc(&self.memory)?;
                                            match mother_pt {
                                                Some(m_pt) => {
                                                    self.memory.write(
                                                        m_pt,
                                                        mother_offset.unwrap() as usize,
                                                        &next_pt.to_le_bytes(),
                                                    )?;
                                                    self.memory.free(pt)?;
                                                }
                                                None => {
                                                    self.memory.free(pt)?;
                                                    self.set_root(Some(next_pt), position_type);
                                                }
                                            }
                                            sibling_node.free(&mut self.memory)?;
                                            pt = next_pt;
                                        } else {
                                            mother_pt = Some(pt);
                                            pt = next_pt;
                                            mother_offset = Some(next_offset);
                                        }
                                    }
                                    _ => unreachable!(),
                                }
                            }
                        }
                        return Ok(());
                    }

                    let direction = liquidation_index & (1u64 << critbit) == 0;
                    println!("Headed towards {:?}", !direction);
                    let sibling_pt;
                    match direction {
                        true => {
                            sibling_pt = self
                                .memory
                                .read_u32_le(pt, InnerNodeSchema::RightPointer as usize)?;
                            pt = self
                                .memory
                                .read_u32_le(pt, InnerNodeSchema::LeftPointer as usize)?;
                        }
                        false => {
                            sibling_pt = self
                                .memory
                                .read_u32_le(pt, InnerNodeSchema::LeftPointer as usize)?;
                            pt = self
                                .memory
                                .read_u32_le(pt, InnerNodeSchema::RightPointer as usize)?;
                        }
                    };
                    if direction ^ is_short {
                        // We liquidate positions which are lower than the liquidation index in the shorts tree and vice versa.
                        collateral_to_liquidate +=
                            self.get_node(sibling_pt)?.get_collateral(&self.memory)?;
                        v_coin_to_liquidate +=
                            self.get_node(sibling_pt)?.get_v_coin(&self.memory)?;
                        v_pc_to_liquidate += self.get_node(sibling_pt)?.get_v_pc(&self.memory)?;
                    }
                }
                Node::Leaf(leaf) => {
                    let leaf_liquidation_index = leaf.get_liquidation_index(&self.memory)?;
                    if ((liquidation_index < leaf_liquidation_index) ^ is_short)
                        || liquidation_index == leaf_liquidation_index
                    {
                        println!("Liquidating this leaf");
                        collateral_to_liquidate += leaf.get_collateral(&self.memory)?;
                        v_coin_to_liquidate += leaf.get_v_coin(&self.memory)?;
                        v_pc_to_liquidate += leaf.get_v_pc(&self.memory)?;
                    }
                    if collateral_to_liquidate == 0 {
                        //Nothing to liquidate
                        return Ok(());
                    }
                    println!("Starting liquidation walk.");
                    pt = root.unwrap();
                    let mut mother_pt = None;
                    let mut mother_offset = None;
                    let mut grandmother_pt = None;
                    let mut grandmother_offset = None;
                    loop {
                        match self.get_node(pt)? {
                            Node::InnerNode(inner_node) => {
                                let critbit = inner_node.get_critbit(&self.memory)?;
                                let current_collateral = inner_node.get_collateral(&self.memory)?;
                                let current_v_coin = inner_node.get_v_coin(&self.memory)?;
                                let current_v_pc = inner_node.get_v_pc(&self.memory)?;
                                inner_node.set_collateral(
                                    &mut self.memory,
                                    &(current_collateral - collateral_to_liquidate),
                                )?;
                                inner_node.set_v_coin(
                                    &mut self.memory,
                                    &(current_v_coin - v_coin_to_liquidate),
                                )?;
                                inner_node.set_v_pc(
                                    &mut self.memory,
                                    &(current_v_pc - v_pc_to_liquidate),
                                )?;

                                let (direction, next_offset, next_pt, sibling_pt) =
                                    self.walk(pt, &liquidation_index, &critbit)?;
                                println!("Headed towards {:?}", !direction);

                                if direction ^ is_short {
                                    println!("Liquidating sibling");
                                    let sibling_node = self.get_node(sibling_pt)?;
                                    collateral_to_liquidate -=
                                        sibling_node.get_collateral(&self.memory)?;
                                    v_coin_to_liquidate -= sibling_node.get_v_coin(&self.memory)?;
                                    v_pc_to_liquidate -= sibling_node.get_v_pc(&self.memory)?;
                                    match mother_pt {
                                        Some(m_pt) => {
                                            self.memory.write(
                                                m_pt,
                                                mother_offset.unwrap() as usize,
                                                &next_pt.to_le_bytes(),
                                            )?;
                                            self.memory.free(pt)?;
                                        }
                                        None => {
                                            self.memory.free(pt)?;
                                            self.set_root(Some(next_pt), position_type);
                                        }
                                    }
                                    sibling_node.free(&mut self.memory)?;
                                    pt = next_pt;
                                } else {
                                    grandmother_pt = mother_pt;
                                    grandmother_offset = mother_offset;
                                    mother_pt = Some(pt);
                                    pt = next_pt;
                                    mother_offset = Some(next_offset);
                                }
                            }
                            Node::Leaf(_) => {
                                if collateral_to_liquidate != 0 {
                                    println!("Liquidating leaf");
                                    self.remove_node(
                                        pt,
                                        position_type,
                                        mother_pt,
                                        grandmother_pt,
                                        mother_offset,
                                        grandmother_offset,
                                    )?;
                                }
                                break;
                            }
                        }
                    }
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn close_position(
        &mut self,
        liquidation_index: u64,
        position_collateral: u64,
        position_v_coin: u64,
        position_v_pc: u64,
        position_type: PositionType,
        position_slot: u64,
    ) -> PerpResult {
        let root = match position_type {
            PositionType::Short => self.shorts_root,
            PositionType::Long => self.longs_root,
        };

        if root.is_none() {
            // Position has been liquidated
            return Err(PerpError::PositionNotFound);
        }
        let mut pt = root.unwrap();
        loop {
            match self.get_node(pt)? {
                Node::InnerNode(inner_node) => {
                    let critbit = inner_node.get_critbit(&self.memory)?;
                    let (liq_index_min, liq_index_max) =
                        inner_node.get_liquidation_index_min_max(critbit, &self.memory)?;
                    if liquidation_index > liq_index_max || liquidation_index < liq_index_min {
                        //Position has been liquidated
                        return Err(PerpError::PositionNotFound);
                    }

                    match liquidation_index & (1u64 << critbit) == 0 {
                        true => {
                            pt = self
                                .memory
                                .read_u32_le(pt, InnerNodeSchema::LeftPointer as usize)?;
                        }
                        false => {
                            pt = self
                                .memory
                                .read_u32_le(pt, InnerNodeSchema::RightPointer as usize)?;
                        }
                    }
                }
                Node::Leaf(leaf) => {
                    let leaf_liquidation_index = leaf.get_liquidation_index(&self.memory)?;
                    let leaf_slot = leaf.get_slot(&self.memory)?;
                    if (leaf_liquidation_index == liquidation_index) && (leaf_slot == position_slot)
                    {
                        if (position_collateral == 0) && (position_v_coin == 0) {
                            // We use this to check if an order has been liquidated without affecting the tree
                            return Ok(());
                        }
                        let leaf_collateral = leaf.get_collateral(&self.memory)?;
                        let leaf_v_coin = leaf.get_v_coin(&self.memory)?;
                        let leaf_v_pc = leaf.get_v_pc(&self.memory)?;
                        let new_collateral = leaf_collateral - position_collateral;
                        let new_v_coin = leaf_v_coin - position_v_coin;
                        let new_v_pc = leaf_v_pc - position_v_pc;
                        pt = root.unwrap();
                        let mut mother_pt = None;
                        let mut mother_offset = None;
                        let mut grandmother_pt = None;
                        let mut grandmother_offset = None;
                        loop {
                            match self.get_node(pt)? {
                                Node::InnerNode(inner_node) => {
                                    let critbit = inner_node.get_critbit(&self.memory)?;
                                    let current_collateral =
                                        inner_node.get_collateral(&self.memory)?;
                                    inner_node.set_collateral(
                                        &mut self.memory,
                                        &(current_collateral - position_collateral),
                                    )?;
                                    let current_v_coin = inner_node.get_v_coin(&self.memory)?;
                                    inner_node.set_v_coin(
                                        &mut self.memory,
                                        &(current_v_coin - position_v_coin),
                                    )?;
                                    let current_v_pc = inner_node.get_v_pc(&self.memory)?;
                                    inner_node.set_v_pc(
                                        &mut self.memory,
                                        &(current_v_pc - position_v_pc),
                                    )?;

                                    grandmother_pt = mother_pt;
                                    grandmother_offset = mother_offset;
                                    mother_pt = Some(pt);

                                    match liquidation_index & (1u64 << critbit) == 0 {
                                        true => {
                                            mother_offset = Some(InnerNodeSchema::LeftPointer);
                                            pt = self.memory.read_u32_le(
                                                pt,
                                                InnerNodeSchema::LeftPointer as usize,
                                            )?;
                                        }
                                        false => {
                                            mother_offset = Some(InnerNodeSchema::RightPointer);
                                            pt = self.memory.read_u32_le(
                                                pt,
                                                InnerNodeSchema::RightPointer as usize,
                                            )?;
                                        }
                                    }
                                }
                                Node::Leaf(leaf) => {
                                    if new_collateral == 0 {
                                        self.remove_node(
                                            pt,
                                            position_type,
                                            mother_pt,
                                            grandmother_pt,
                                            mother_offset,
                                            grandmother_offset,
                                        )?;
                                    } else {
                                        leaf.set_collateral(&mut self.memory, &new_collateral)?;
                                        leaf.set_v_coin(&mut self.memory, &new_v_coin)?;
                                        leaf.set_v_pc(&mut self.memory, &new_v_pc)?;
                                    }
                                    break;
                                }
                            }
                        }
                    } else {
                        // Position has been liquidated
                        return Err(PerpError::PositionNotFound);
                    }
                    break;
                }
            }
        }
        Ok(())
    }

    pub fn open_position(
        &mut self,
        liquidation_index: u64,
        collateral: u64,
        v_coin: u64,
        v_pc: u64,
        position_type: PositionType,
        current_slot: u64,
    ) -> Result<Leaf, PerpError> {
        let root = match position_type {
            PositionType::Short => self.shorts_root,
            PositionType::Long => self.longs_root,
        };
        if root.is_none() {
            let new_leaf_pt =
                self.write_leaf(liquidation_index, current_slot, collateral, v_coin, v_pc)?;
            self.set_root(Some(new_leaf_pt), position_type);

            return Ok(Leaf(new_leaf_pt));
        }
        let mut pt = root.unwrap();
        let mut mother_pt = None;
        let mut mother_offset = None;

        loop {
            match self.get_node(pt)? {
                Node::InnerNode(inner_node) => {
                    let critbit = inner_node.get_critbit(&self.memory)?;
                    let (liq_index_min, liq_index_max) =
                        inner_node.get_liquidation_index_min_max(critbit, &self.memory)?;
                    let current_collateral = inner_node.get_collateral(&self.memory)?;
                    let current_v_pc = inner_node.get_v_pc(&self.memory)?;
                    let current_v_coin = inner_node.get_v_coin(&self.memory)?;

                    if liquidation_index > liq_index_max || liquidation_index < liq_index_min {
                        let new_critbit = find_critbit(&liquidation_index, &liq_index_min);
                        let new_liq_index_min = liquidation_index & !((2u64 << new_critbit) - 1);
                        let new_leaf_pt = self.write_leaf(
                            liquidation_index,
                            current_slot,
                            collateral,
                            v_coin,
                            v_pc,
                        )?;

                        let (left_pt, right_pt) = match liquidation_index & (1 << new_critbit) == 0
                        {
                            true => (new_leaf_pt, pt),
                            false => (pt, new_leaf_pt),
                        };

                        let new_inner_node_pt = self.write_inner(
                            new_critbit,
                            new_liq_index_min,
                            left_pt,
                            right_pt,
                            collateral + current_collateral,
                            v_coin + current_v_coin,
                            v_pc + current_v_pc,
                        )?;

                        match mother_pt {
                            Some(m_pt) => self.memory.write(
                                m_pt,
                                mother_offset.unwrap() as usize,
                                &new_inner_node_pt.to_le_bytes(),
                            )?,
                            None => self.set_root(Some(new_inner_node_pt), position_type),
                        }
                        pt = new_leaf_pt;
                        break;
                    } else {
                        self.memory.write(
                            pt,
                            InnerNodeSchema::Collateral as usize,
                            &(current_collateral + collateral).to_le_bytes(),
                        )?;
                        self.memory.write(
                            pt,
                            InnerNodeSchema::VCoin as usize,
                            &(current_v_coin + v_coin).to_le_bytes(),
                        )?;
                        self.memory.write(
                            pt,
                            InnerNodeSchema::VPc as usize,
                            &(current_v_pc + v_pc).to_le_bytes(),
                        )?;
                        mother_pt = Some(pt);
                        match liquidation_index & (1 << critbit) == 0 {
                            true => {
                                pt = self
                                    .memory
                                    .read_u32_le(pt, InnerNodeSchema::LeftPointer as usize)?;
                                mother_offset = Some(InnerNodeSchema::LeftPointer);
                            }
                            false => {
                                pt = self
                                    .memory
                                    .read_u32_le(pt, InnerNodeSchema::RightPointer as usize)?;

                                mother_offset = Some(InnerNodeSchema::RightPointer);
                            }
                        }
                    }
                }
                Node::Leaf(leaf) => {
                    let leaf_liquidation_index = leaf.get_liquidation_index(&self.memory)?;

                    if leaf_liquidation_index == liquidation_index {
                        let current_collateral = leaf.get_collateral(&self.memory)?;
                        let current_v_coin = leaf.get_v_coin(&self.memory)?;
                        let current_v_pc = leaf.get_v_pc(&self.memory)?;
                        leaf.set_collateral(&mut self.memory, &(collateral + current_collateral))?;
                        leaf.set_v_coin(&mut self.memory, &(v_coin + current_v_coin))?;
                        leaf.set_v_pc(&mut self.memory, &(v_pc + current_v_pc))?;
                        return Ok(Leaf(pt));
                    }
                    let critbit = find_critbit(&liquidation_index, &leaf_liquidation_index);
                    let new_liq_index_min =
                        leaf_liquidation_index & liquidation_index & !((1u64 << critbit) - 1);
                    let new_leaf_pt =
                        self.write_leaf(liquidation_index, current_slot, collateral, v_coin, v_pc)?;

                    let (left_pt, right_pt) = match liquidation_index & (1 << critbit) == 0 {
                        true => (new_leaf_pt, pt),
                        false => (pt, new_leaf_pt),
                    };

                    let existing_collateral = leaf.get_collateral(&self.memory)?;
                    let existing_v_coin = leaf.get_v_coin(&self.memory)?;
                    let existing_v_pc = leaf.get_v_pc(&self.memory)?;

                    let new_inner_node_pt = self.write_inner(
                        critbit,
                        new_liq_index_min,
                        left_pt,
                        right_pt,
                        existing_collateral + collateral,
                        v_coin + existing_v_coin,
                        v_pc + existing_v_pc,
                    )?;

                    match mother_pt {
                        Some(m_pt) => self.memory.write(
                            m_pt,
                            mother_offset.unwrap() as usize,
                            &new_inner_node_pt.to_le_bytes(),
                        )?,
                        None => self.set_root(Some(new_inner_node_pt), position_type),
                    }
                    pt = new_leaf_pt;
                    break;
                }
            }
        }
        Ok(Leaf(pt))
    }

    pub fn compute_aggregate_position(
        &self,
        side: PositionType,
    ) -> Result<(u64, u64, u64), PerpError> {
        let root = match side {
            PositionType::Short => self.shorts_root,
            PositionType::Long => self.longs_root,
        };
        let mut total_v_pc = 0;
        let mut total_v_coin = 0;
        let mut total_collateral = 0;
        if root.is_none() {
            return Ok((total_v_pc, total_v_coin, total_collateral));
        }
        let mut stack: Vec<u32> = Vec::with_capacity(64); // Avoid reallocation in worst case
        let mut current = self.get_node(root.unwrap())?;
        loop {
            match current {
                Node::InnerNode(ref n) => {
                    // println!("Walk on inner node {:?}", n.0);
                    let left_pt = self
                        .memory
                        .read_u32_le(n.0, InnerNodeSchema::LeftPointer as usize)?;
                    let right_pt = self
                        .memory
                        .read_u32_le(n.0, InnerNodeSchema::RightPointer as usize)?;
                    current = self.get_node(left_pt).unwrap();
                    stack.push(right_pt);
                }
                Node::Leaf(l) => {
                    // println!("Walk on leaf {:?}", l.0);
                    let liquidation_index = l.get_liquidation_index(&self.memory)?;
                    if liquidation_index != 0 {
                        let leaf_v_coin = l.get_v_coin(&self.memory)?;
                        let leaf_collateral = l.get_collateral(&self.memory)?;
                        total_v_coin = total_v_coin.checked_add(leaf_v_coin).unwrap();
                        total_v_pc = total_v_pc
                            .checked_add(compute_liquidation_index_inverse(
                                leaf_collateral,
                                leaf_v_coin,
                                liquidation_index,
                                side,
                            ))
                            .unwrap();
                        total_collateral = total_collateral.checked_add(leaf_collateral).unwrap();
                    }

                    if let Some(pt) = stack.pop() {
                        current = self.get_node(pt).unwrap();
                    } else {
                        break;
                    }
                }
            }
        }

        Ok((total_v_pc, total_v_coin, total_collateral))
    }
}

fn find_critbit(first_liquidation_index: &u64, second_liquidation_index: &u64) -> u8 {
    let lz = (first_liquidation_index ^ second_liquidation_index).leading_zeros() as u8;
    63 - lz
}

#[cfg(test)]
mod tests {

    use std::{cell::RefCell, rc::Rc};

    use super::*;
    use crate::{
        positions_book::{
            memory::{Memory, SLOT_SIZE},
            page::Page,
        },
        utils::print_tree,
    };

    fn init_tree<'a>(data: &[Rc<RefCell<&'a mut [u8]>>]) -> PositionsBook<'a> {
        let mut pages = vec![];
        let page_size = (data[0].borrow().len() / SLOT_SIZE) as u32;

        for d in data {
            pages.push(Page {
                page_size,
                data: Rc::clone(d),
                free_slot_list_hd: None,
                uninitialized_memory: 0,
            });
        }

        let mem = Memory::new(pages, None);

        PositionsBook {
            shorts_root: None,
            longs_root: None,
            memory: mem,
        }
    }

    fn test_build(position_type: PositionType) {
        let (mut data0, mut data1, mut data2, mut data3) =
            ([0u8; 1024], [0u8; 1024], [0u8; 1024], [0u8; 1024]);
        let data: Vec<Rc<RefCell<&mut [u8]>>> = vec![
            Rc::new(RefCell::new(&mut data0)),
            Rc::new(RefCell::new(&mut data1)),
            Rc::new(RefCell::new(&mut data2)),
            Rc::new(RefCell::new(&mut data3)),
        ];
        let mut book = init_tree(&data);

        let positions = vec![
            (0x84, 100, 42, 908),
            (0xfe, 101, 75, 98),
            (0x0f, 107, 4500, 708),
            (0x9b, 123, 78000, 408),
            (0x52, 144, 9685, 958),
            (0xc1, 177, 7584, 108),
            (0xaf, 295, 4681, 444),
            (0x2f, 1045, 12346, 333),
            (0xfb, 4049, 47958413, 12),
            (0xb7, 7940, 42, 24),
        ];

        let mut total_coll = 0;
        let mut total_v_coin = 0;
        let mut total_v_pc = 0;

        for (liq_index, coll, v_coin, v_pc) in &positions {
            book.open_position(*liq_index, *coll, *v_coin, *v_pc, position_type, 0)
                .unwrap();
            total_coll += coll;
            total_v_coin += v_coin;
            total_v_pc += v_pc;
        }
        let root = match position_type {
            PositionType::Long => book.longs_root,
            PositionType::Short => book.shorts_root,
        };
        assert!(root.is_some());
        assert_eq!(
            total_coll,
            book.get_node(root.unwrap())
                .unwrap()
                .get_collateral(&book.memory)
                .unwrap()
        );
        assert_eq!(
            total_v_coin,
            book.get_node(root.unwrap())
                .unwrap()
                .get_v_coin(&book.memory)
                .unwrap()
        );
        assert_eq!(
            total_v_pc,
            book.get_node(root.unwrap())
                .unwrap()
                .get_v_pc(&book.memory)
                .unwrap()
        );
    }

    fn test_close(position_type: PositionType, close_index: usize) {
        let (mut data0, mut data1, mut data2, mut data3) =
            ([0u8; 1024], [0u8; 1024], [0u8; 1024], [0u8; 1024]);
        let data: Vec<Rc<RefCell<&mut [u8]>>> = vec![
            Rc::new(RefCell::new(&mut data0)),
            Rc::new(RefCell::new(&mut data1)),
            Rc::new(RefCell::new(&mut data2)),
            Rc::new(RefCell::new(&mut data3)),
        ];
        let mut book = init_tree(&data);

        let positions = vec![
            (0x84, 100, 42, 908),
            (0xfe, 101, 75, 98),
            (0x0f, 107, 4500, 708),
            (0x9b, 123, 78000, 408),
            (0x52, 144, 9685, 958),
            (0xc1, 177, 7584, 108),
            (0xaf, 295, 4681, 444),
            (0x2f, 1045, 12346, 333),
            (0xfb, 4049, 47958413, 12),
            (0xb7, 7940, 42, 24),
        ];

        let mut total_coll = 0;
        let mut total_v_coin = 0;
        let mut total_v_pc = 0;

        for (liq_index, coll, v_coin, v_pc) in &positions {
            book.open_position(*liq_index, *coll, *v_coin, *v_pc, position_type, 0)
                .unwrap();
            total_coll += coll;
            total_v_coin += v_coin;
            total_v_pc += v_pc;
        }
        book.close_position(
            positions[close_index].0,
            positions[close_index].1,
            positions[close_index].2,
            positions[close_index].3,
            position_type,
            0,
        )
        .unwrap();
        let root = match position_type {
            PositionType::Long => book.longs_root,
            PositionType::Short => book.shorts_root,
        };

        assert_eq!(
            book.get_node(root.unwrap())
                .unwrap()
                .get_collateral(&book.memory)
                .unwrap(),
            total_coll - positions[close_index].1
        );
        assert_eq!(
            total_v_coin - positions[close_index].2,
            book.get_node(root.unwrap())
                .unwrap()
                .get_v_coin(&book.memory)
                .unwrap()
        );
        assert_eq!(
            total_v_pc - positions[close_index].3,
            book.get_node(root.unwrap())
                .unwrap()
                .get_v_pc(&book.memory)
                .unwrap()
        );
    }

    fn test_liquidate(
        liquidation_index: u64,
        position_type: PositionType,
        positions: Vec<(u64, u64, u64, u64)>,
    ) {
        let (mut data0, mut data1, mut data2, mut data3) =
            ([0u8; 1024], [0u8; 1024], [0u8; 1024], [0u8; 1024]);
        let data: Vec<Rc<RefCell<&mut [u8]>>> = vec![
            Rc::new(RefCell::new(&mut data0)),
            Rc::new(RefCell::new(&mut data1)),
            Rc::new(RefCell::new(&mut data2)),
            Rc::new(RefCell::new(&mut data3)),
        ];
        let mut book = init_tree(&data);

        let mut total_coll = 0;
        let mut total_coll_after_liquidation = 0;

        let mut total_v_coin = 0;
        let mut total_v_coin_after_liquidation = 0;

        let mut total_v_pc = 0;
        let mut total_v_pc_after_liquidation = 0;

        for (liq_index, coll, v_coin, v_pc) in &positions {
            book.open_position(*liq_index, *coll, *v_coin, *v_pc, position_type, 0)
                .unwrap();
            total_coll += coll;
            total_v_coin += v_coin;
            total_v_pc += v_pc;
            let will_be_liquidated = match position_type {
                PositionType::Long => *liq_index >= liquidation_index,
                PositionType::Short => *liq_index <= liquidation_index,
            };
            if !will_be_liquidated {
                total_coll_after_liquidation += coll;
                total_v_coin_after_liquidation += v_coin;
                total_v_pc_after_liquidation += v_pc;
            }
        }
        let root = match position_type {
            PositionType::Long => book.longs_root,
            PositionType::Short => book.shorts_root,
        };

        println!("============BEFORE============");

        print_tree(root.unwrap(), &book.memory, 0);

        book.liquidate(liquidation_index, position_type).unwrap();
        println!("============AFTER=============");

        let root = match position_type {
            PositionType::Long => book.longs_root,
            PositionType::Short => book.shorts_root,
        };

        if root.is_none() {
            println!("Empty tree!");
            assert_eq!(total_coll_after_liquidation, 0);
            return;
        }

        assert_eq!(
            book.get_node(root.unwrap())
                .unwrap()
                .get_collateral(&book.memory)
                .unwrap(),
            total_coll_after_liquidation
        );

        assert_eq!(
            book.get_node(root.unwrap())
                .unwrap()
                .get_v_coin(&book.memory)
                .unwrap(),
            total_v_coin_after_liquidation
        );
        assert_eq!(
            book.get_node(root.unwrap())
                .unwrap()
                .get_v_pc(&book.memory)
                .unwrap(),
            total_v_pc_after_liquidation
        );
    }

    #[test]
    fn test_liquidations() {
        let positions = vec![
            (0x84, 100, 42, 908),
            (0xfe, 101, 75, 98),
            (0x0f, 107, 4500, 708),
            (0x9b, 123, 78000, 408),
            (0x52, 144, 9685, 958),
            (0xc1, 177, 7584, 108),
            (0xaf, 295, 4681, 444),
            (0x2f, 1045, 12346, 322),
            (0xfb, 4049, 47958413, 2),
            (0xb7, 7940, 42, 907),
        ];
        test_liquidate(0x85, PositionType::Short, positions.clone());
        test_liquidate(0x85, PositionType::Long, positions.clone());
        test_liquidate(0xf4, PositionType::Short, positions.clone());
        test_liquidate(0xf4, PositionType::Long, positions.clone());
        test_liquidate(0x01, PositionType::Short, positions.clone());
        test_liquidate(0x01, PositionType::Long, positions);

        let positions = vec![
            // (0x84, 100, 42, 908),
            (0xfe, 101, 75, 98),
            (0x0f, 107, 4500, 708),
            // (0x9b, 123, 78000, 408),
            (0x52, 144, 9685, 958),
            (0xc1, 177, 7584, 108),
            // (0xaf, 295, 4681, 444),
            (0x2f, 1045, 12346, 322),
            (0xfb, 4049, 47958413, 2),
            // (0xb7, 7940, 42, 907),
        ];
        test_liquidate(0xb7, PositionType::Short, positions.clone());
        test_liquidate(0xb7, PositionType::Long, positions.clone());
        test_liquidate(0x84, PositionType::Short, positions.clone());
        test_liquidate(0x84, PositionType::Long, positions);

        let positions = vec![(0x429, 2964775883, 26179077, 590)];
        test_liquidate(4299262263296, PositionType::Short, positions);
    }

    #[test]
    fn test_builds() {
        test_build(PositionType::Long);
        test_build(PositionType::Short);
    }

    #[test]
    fn batch_test_close() {
        for i in 0..10 {
            test_close(PositionType::Long, i);
            test_close(PositionType::Short, i);
        }
    }

    // #[test]
    // fn test_aggregate_position() {
    //     let (mut data0, mut data1, mut data2, mut data3) =
    //         ([0u8; 1024], [0u8; 1024], [0u8; 1024], [0u8; 1024]);
    //     let data: Vec<Rc<RefCell<&mut [u8]>>> = vec![
    //         Rc::new(RefCell::new(&mut data0)),
    //         Rc::new(RefCell::new(&mut data1)),
    //         Rc::new(RefCell::new(&mut data2)),
    //         Rc::new(RefCell::new(&mut data3)),
    //     ];
    //     let positions = vec![
    //         (0x84, 100, 42, 908),
    //         (0xfe, 101, 75, 98),
    //         (0x0f, 107, 4500, 708),
    //         (0x9b, 123, 78000, 408),
    //         (0x52, 144, 9685, 958),
    //         (0xc1, 177, 7584, 108),
    //         (0xaf, 295, 4681, 444),
    //         (0x2f, 1045, 12346, 322),
    //         (0xfb, 4049, 47958413, 2),
    //         (0xb7, 7940, 42, 907),
    //     ];
    //     let mut book = init_tree(&data);
    //     let position_type = PositionType::Short;

    //     let mut total_coll = 0;
    //     let mut total_v_coin = 0;
    //     let mut total_v_pc = 0;

    //     let k = 10u128.pow(14);

    //     for (v_pc, coll, v_coin) in &positions {
    //         let liq_index = compute_liquidation_index(*coll, *v_coin, *v_pc, position_type, k);
    //         book.open_position(liq_index, *coll, *v_coin, position_type, 0)
    //             .unwrap();
    //         total_coll += coll;
    //         total_v_pc +=
    //             compute_liquidation_index_inverse(*coll, *v_coin, liq_index, position_type);
    //         total_v_coin += v_coin;
    //     }
    //     let (res_v_pc, res_v_coin, res_collateral) =
    //         book.compute_aggregate_position(position_type).unwrap();
    //     assert_eq!(total_v_pc, res_v_pc);
    //     assert_eq!(total_v_coin, res_v_coin);
    //     assert_eq!(total_coll, res_collateral);
    // }
}
