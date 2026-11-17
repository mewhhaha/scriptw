//! A function's home slots for the GC heap base and for its `vmctx`.
//!
//! When the GC heap cannot move, its base is one value for a whole function,
//! and so is the function's `vmctx` parameter, which the translator passes
//! wherever it needs the instance. As a single SSA value each is live from
//! its first use to its last, across every call in between. In a large
//! function the register allocator spills such a value and, once it has
//! split the live range twice (`regalloc2`'s limit per value), reloads it
//! from its spill slot before every single use.
//!
//! The translator therefore gives each a home: a stack slot the entry block
//! stores the value to, once. Every use is first translated to a load from
//! that slot ([`FrameHome::load`], [`FrameHome::of_parameter`]), and
//! `frame_reloads` then removes the loads that follow another load or the
//! store with no call in between. What stays is one load after a call for
//! the uses that follow it; where paths that did and did not call meet, the
//! value is read again. No value is then live across a call, so none needs a
//! callee-saved register or a spill, and code pays at most one load from the
//! frame for a call it makes instead of one for every use.
//!
//! Nothing here is needed for correctness: the slot is written once, before
//! any load, and never again, so every load yields the same value. The calls
//! only decide where a load is worth its instruction. When no load remains,
//! `frame_reloads` removes the store and the slot, and a function that makes
//! no call is compiled exactly as it was without a home.

use cranelift_codegen::cursor::{Cursor, FuncCursor};
use cranelift_codegen::ir::{self, InstBuilder};

/// The home slot of a value that one activation of a function never
/// changes.
#[derive(Clone, Copy)]
pub(crate) struct FrameHome {
    slot: ir::StackSlot,
    /// The flags of every access to `slot`, with the home's alias region.
    flags: ir::MemFlagsData,
    pointer_type: ir::Type,
}

impl FrameHome {
    /// Create the home of `value`, which the entry block defines before
    /// `pos`, and store `value` to it at `pos`.
    ///
    /// `region` must be an alias region that only this home's accesses use.
    pub(crate) fn store_at_entry(
        pos: &mut FuncCursor<'_>,
        region: ir::AliasRegion,
        pointer_type: ir::Type,
        value: ir::Value,
    ) -> Self {
        debug_assert_eq!(pos.current_block(), pos.func.layout.entry_block());
        let bytes = pointer_type.bytes();
        let slot = pos.func.create_sized_stack_slot(ir::StackSlotData::new(
            ir::StackSlotKind::ExplicitSlot,
            bytes,
            bytes.trailing_zeros() as u8,
        ));
        let flags = ir::MemFlagsData::trusted().with_alias_region(Some(region));
        let address = pos.ins().stack_addr(pointer_type, slot, 0);
        pos.ins().store(flags, value, address, 0);
        Self {
            slot,
            flags,
            pointer_type,
        }
    }

    /// Give `parameter`, a parameter of `func` that the function never
    /// changes (its `vmctx`), a home, and make every use of it a load from
    /// that home.
    ///
    /// `func` must be completely translated, and `region` an alias region
    /// that only this home's accesses use. `None`, with `func` unchanged,
    /// when the function makes no call or does not use the parameter: its
    /// live range then has nothing to be split at.
    ///
    /// What the function derives from the parameter, such as the address of
    /// a global or the array of type ids, the translator derives where it is
    /// used, and so from the value that is at hand there: none of that is
    /// then live across a call either.
    pub(crate) fn of_parameter(
        func: &mut ir::Function,
        region: ir::AliasRegion,
        parameter: ir::Value,
    ) -> Option<Self> {
        let entry = func.layout.entry_block()?;
        let mut users = Vec::new();
        let mut calls = false;
        for block in func.layout.blocks() {
            for inst in func.layout.block_insts(block) {
                calls |= func.dfg.insts[inst].opcode().is_call();
                if super::super::operands::any(&func.dfg, inst, |value| {
                    func.dfg.resolve_aliases(value) == parameter
                }) {
                    users.push(inst);
                }
            }
        }
        if !calls || users.is_empty() {
            return None;
        }
        let pointer_type = func.dfg.value_type(parameter);
        let home = Self::store_at_entry(
            &mut FuncCursor::new(func).at_first_insertion_point(entry),
            region,
            pointer_type,
            parameter,
        );
        let mut values: Vec<ir::Value> = Vec::new();
        for inst in users {
            let loaded = home.load(&mut FuncCursor::new(func).at_inst(inst));
            values.clear();
            values.extend(func.dfg.inst_values(inst).map(|value| {
                if func.dfg.resolve_aliases(value) == parameter {
                    loaded
                } else {
                    value
                }
            }));
            func.dfg.overwrite_inst_values(inst, values.drain(..));
        }
        Some(home)
    }

    /// The home's stack slot.
    pub(crate) fn slot(&self) -> ir::StackSlot {
        self.slot
    }

    /// Load the value from its home at `pos`.
    pub(crate) fn load(&self, pos: &mut FuncCursor<'_>) -> ir::Value {
        let address = pos.ins().stack_addr(self.pointer_type, self.slot, 0);
        pos.ins().load(self.pointer_type, self.flags, address, 0)
    }
}
