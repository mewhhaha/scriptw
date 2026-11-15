//! A function's home slot for the GC heap base.
//!
//! When the GC heap cannot move, its base is one value for a whole function.
//! As a single SSA value it is live from the function's first heap access to
//! its last, across every call in between. In a large function the register
//! allocator spills such a value and, once it has split the live range twice
//! (`regalloc2`'s limit per value), reloads it from its spill slot before
//! every single use.
//!
//! The translator therefore gives the base a home: a stack slot the entry
//! block stores it to, once. Every use of the base is first translated to a
//! load from that slot, and [`GcHeapBaseHome::split_at_calls`] then rewrites
//! the function so that the base is, wherever it can be, a value that no
//! call is inside the live range of:
//!
//! * up to the first call on any path it is the value the entry block loaded;
//! * after a call it is loaded from the home again, right after the call, but
//!   only when some use can be reached from there without another call;
//! * where paths with different such values join, a block parameter selects
//!   between them, so a path that made no call passes its value on and does
//!   not load anything;
//! * a call inside a loop does not split: a loop keeps the value it is
//!   entered with on every path and in every iteration, so that what it
//!   computes from the base stays loop-invariant and its iterations load
//!   nothing, and the value is loaded again where a loop that calls is left;
//! * when no load from the home remains, the entry block's store goes too, and
//!   a function that makes no call is compiled exactly as it was without a
//!   home.
//!
//! Outside loops each value needs a register only between two calls, so it
//! needs neither a callee-saved register nor a spill, and code pays one load
//! from the frame for a call it makes instead of one for every use of the
//! base. A loop is compiled as it was with a single value, except that the
//! value it has to keep across its calls is one that ends with the loop.
//!
//! Nothing here is needed for correctness: the slot is written once, before
//! any load, and never again, so every load yields the same value, and the
//! rewrite only has to produce valid SSA. The calls only decide where a load
//! is worth its instruction.

use cranelift_codegen::cursor::{Cursor, FuncCursor};
use cranelift_codegen::dominator_tree::DominatorTree;
use cranelift_codegen::flowgraph::ControlFlowGraph;
use cranelift_codegen::ir::{self, InstBuilder};
use cranelift_codegen::loop_analysis::{Loop, LoopAnalysis};
use cranelift_entity::SecondaryMap;
use cranelift_entity::packed_option::PackedOption;
use std::collections::{HashMap, HashSet};

/// The home slot of the GC heap base in one function.
#[derive(Clone, Copy)]
pub(crate) struct GcHeapBaseHome {
    slot: ir::StackSlot,
    /// The flags of every access to `slot`, with the home's alias region.
    flags: ir::MemFlagsData,
    region: ir::AliasRegion,
    pointer_type: ir::Type,
    /// The base as the entry block loads it from the store context.
    base: ir::Value,
    /// The entry block's store of `base` to `slot`.
    store: ir::Inst,
}

impl GcHeapBaseHome {
    /// Create the home of `base`, which the entry block defines before `pos`,
    /// and store `base` to it at `pos`.
    ///
    /// `region` must be an alias region that only this home's accesses use.
    pub(crate) fn store_at_entry(
        pos: &mut FuncCursor<'_>,
        region: ir::AliasRegion,
        pointer_type: ir::Type,
        base: ir::Value,
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
        let store = pos.ins().store(flags, base, address, 0);
        Self {
            slot,
            flags,
            region,
            pointer_type,
            base,
            store,
        }
    }

    /// Load the base from its home at `pos`.
    pub(crate) fn load(&self, pos: &mut FuncCursor<'_>) -> ir::Value {
        self.load_from(pos, 0)
    }

    /// Load the base from its home at `pos`, addressing the slot from
    /// `distance` bytes past it.
    ///
    /// Loads with different distances read the same bytes through different
    /// `stack_addr` values. The x64 backend folds a `stack_addr` into the
    /// address of a load, but not when it merges the load into the
    /// instruction that uses it: then it computes the address into a
    /// register, at the place where that `stack_addr` value was first needed.
    /// For an address that every load shares that place is the entry block,
    /// and the address occupies a callee-saved register or a spill slot from
    /// there to the merged load. A load that has its own `stack_addr` keeps
    /// such an address next to it.
    fn load_from(&self, pos: &mut FuncCursor<'_>, distance: i32) -> ir::Value {
        let address = pos.ins().stack_addr(self.pointer_type, self.slot, distance);
        pos.ins()
            .load(self.pointer_type, self.flags, address, -distance)
    }

    fn is_load(&self, func: &ir::Function, inst: ir::Inst) -> bool {
        func.dfg.insts[inst].opcode() == ir::Opcode::Load
            && func.dfg.insts[inst]
                .memflags()
                .is_some_and(|flags| func.dfg.mem_flags[flags].alias_region() == Some(self.region))
    }

    /// Make the base a value that is not live across a call of `func` outside
    /// a loop (see the module documentation).
    ///
    /// `func` must be completely translated, with every use of the base a
    /// [`GcHeapBaseHome::load`]. This computes the function's control-flow
    /// graph, dominator tree and loops.
    pub(crate) fn split_at_calls(self, func: &mut ir::Function) {
        let cfg = ControlFlowGraph::with_function(func);
        let domtree = DominatorTree::with_function(func, &cfg);
        let mut loops = LoopAnalysis::new();
        loops.compute(func, &cfg, &domtree);
        let Some(entry) = func.layout.entry_block() else {
            return;
        };
        // Every reachable block, each after its dominators. Unreachable
        // blocks keep their loads from the home.
        let blocks: Vec<ir::Block> = domtree.cfg_rpo().copied().collect();

        // A call inside a loop does not split the base: the loop keeps the
        // definition it is entered with. Where an edge leaves a loop that
        // calls, that definition ends.
        let outermost_loop = |block: ir::Block| {
            let mut outermost = loops.innermost_loop(block)?;
            while let Some(parent) = loops.loop_parent(outermost) {
                outermost = parent;
            }
            Some(outermost)
        };
        let splits = |func: &ir::Function, block: ir::Block, inst: ir::Inst| {
            func.dfg.insts[inst].opcode().is_call() && loops.innermost_loop(block).is_none()
        };
        let mut calls_in_loop: SecondaryMap<Loop, bool> = SecondaryMap::new();
        for &block in &blocks {
            if let Some(outermost) = outermost_loop(block) {
                if func
                    .layout
                    .block_insts(block)
                    .any(|inst| func.dfg.insts[inst].opcode().is_call())
                {
                    calls_in_loop[outermost] = true;
                }
            }
        }
        // Whether the base's definition at the end of `from` is still the
        // base's definition at the start of `to`.
        let carries = |func: &ir::Function, from: ir::Block, to: ir::Block| {
            let ends_in_call = func
                .layout
                .last_inst(from)
                .is_some_and(|terminator| splits(func, from, terminator));
            let leaves_calling_loop = outermost_loop(from).is_some_and(|outermost| {
                calls_in_loop[outermost] && !loops.is_in_loop(to, outermost)
            });
            domtree.is_reachable(from) && !ends_in_call && !leaves_calling_loop
        };

        // What each block does first, and its last call that splits.
        let mut first: SecondaryMap<ir::Block, First> = SecondaryMap::new();
        let mut last_call: SecondaryMap<ir::Block, PackedOption<ir::Inst>> = SecondaryMap::new();
        for &block in &blocks {
            for inst in func.layout.block_insts(block) {
                let event = if self.is_load(func, inst) {
                    First::Use
                } else if splits(func, block, inst) {
                    last_call[block] = inst.into();
                    First::Call
                } else {
                    continue;
                };
                if first[block] == First::Neither {
                    first[block] = event;
                }
            }
        }

        // The base is wanted at a point when a use can be reached from it
        // with the definition the point has. A backward problem whose only
        // change is from "not wanted" to "wanted", iterated in post-order to
        // its fixed point.
        let mut wanted_at_start: SecondaryMap<ir::Block, bool> = SecondaryMap::new();
        let wanted_at_end =
            |func: &ir::Function, wanted_at_start: &SecondaryMap<ir::Block, bool>, block| {
                cfg.succ_iter(block)
                    .any(|successor| wanted_at_start[successor] && carries(func, block, successor))
            };
        loop {
            let mut changed = false;
            for &block in blocks.iter().rev() {
                let wanted = match first[block] {
                    First::Use => true,
                    First::Call => false,
                    First::Neither => wanted_at_end(func, &wanted_at_start, block),
                };
                if wanted && !wanted_at_start[block] {
                    wanted_at_start[block] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        // Where the base gets a definition other than the one its
        // predecessors end with: after a block's last splitting call, and at
        // the start of a block that an edge enters without carrying one.
        let mut loads_at_start: SecondaryMap<ir::Block, bool> = SecondaryMap::new();
        let mut defines: SecondaryMap<ir::Block, bool> = SecondaryMap::new();
        defines[entry] = true;
        for &block in &blocks {
            loads_at_start[block] = cfg
                .pred_iter(block)
                .any(|predecessor| !carries(func, predecessor.block, block));
            defines[block] |= loads_at_start[block] || last_call[block].is_some();
        }

        // A block where paths with different definitions first meet takes a
        // parameter: the iterated dominance frontier of the defining blocks
        // (Cytron et al.), with each frontier computed from the join's
        // predecessors (Cooper, Harvey and Kennedy).
        let mut frontier: SecondaryMap<ir::Block, Vec<ir::Block>> = SecondaryMap::new();
        for &block in &blocks {
            let Some(dominator) = domtree.idom(block) else {
                continue;
            };
            let reachable = |predecessor: &ir::Block| domtree.is_reachable(*predecessor);
            let predecessors = || {
                cfg.pred_iter(block)
                    .map(|edge| edge.block)
                    .filter(reachable)
            };
            if predecessors().nth(1).is_none() {
                continue;
            }
            for predecessor in predecessors() {
                let mut runner = predecessor;
                while runner != dominator {
                    if frontier[runner].last() != Some(&block) {
                        frontier[runner].push(block);
                    }
                    runner = domtree
                        .idom(runner)
                        .expect("a join's dominator dominates its predecessors");
                }
            }
        }
        let mut joins: SecondaryMap<ir::Block, bool> = SecondaryMap::new();
        let mut worklist: Vec<ir::Block> = blocks
            .iter()
            .copied()
            .filter(|&block| defines[block])
            .collect();
        while let Some(block) = worklist.pop() {
            for &join in &frontier[block] {
                if !joins[join] {
                    joins[join] = true;
                    if !defines[join] {
                        defines[join] = true;
                        worklist.push(join);
                    }
                }
            }
        }

        // The definition at each block's start: its own where it has one,
        // else the one its immediate dominator ends with.
        let mut reaching: SecondaryMap<ir::Block, Option<Definition>> = SecondaryMap::new();
        let mut leaving: SecondaryMap<ir::Block, Option<Definition>> = SecondaryMap::new();
        for &block in &blocks {
            let at_start = if joins[block] || loads_at_start[block] {
                Definition::Start(block)
            } else {
                match domtree.idom(block) {
                    Some(dominator) => leaving[dominator].expect("dominators come first"),
                    None => Definition::Entry,
                }
            };
            reaching[block] = Some(at_start);
            leaving[block] = Some(match last_call[block].expand() {
                Some(call) => Definition::After(call),
                None => at_start,
            });
        }

        // Create the definitions that are wanted: a load after a call, and at
        // a block's start a load, or the parameter of a join.
        let mut after: HashMap<ir::Inst, ir::Value> = HashMap::new();
        let mut at_start: SecondaryMap<ir::Block, PackedOption<ir::Value>> = SecondaryMap::new();
        let mut takes_parameter: SecondaryMap<ir::Block, bool> = SecondaryMap::new();
        let mut loads: HashSet<ir::Inst> = HashSet::new();
        let mut calls = Vec::new();
        // Each load addresses the slot from its own distance (see
        // `load_from`); should the distances ever run out, loads share the
        // slot's own address.
        let slot_bytes = self.pointer_type.bytes() as usize;
        let own_distance =
            |loads: &HashSet<ir::Inst>| i32::try_from((loads.len() + 1) * slot_bytes).unwrap_or(0);
        for &block in &blocks {
            let mut wanted = wanted_at_end(func, &wanted_at_start, block);
            calls.clear();
            for inst in func.layout.block_insts(block).rev() {
                if self.is_load(func, inst) {
                    wanted = true;
                } else if splits(func, block, inst) {
                    // Nothing can follow a call that ends its block; the
                    // edges from it carry no definition.
                    if wanted && !func.dfg.insts[inst].opcode().is_terminator() {
                        calls.push(inst);
                    }
                    wanted = false;
                }
            }
            for &call in &calls {
                let value = self.load_from(
                    &mut FuncCursor::new(func).after_inst(call),
                    own_distance(&loads),
                );
                loads.insert(func.dfg.value_def(value).unwrap_inst());
                after.insert(call, value);
            }
            if !wanted_at_start[block] {
                continue;
            }
            if loads_at_start[block] {
                let value = self.load_from(
                    &mut FuncCursor::new(func).at_first_insertion_point(block),
                    own_distance(&loads),
                );
                loads.insert(func.dfg.value_def(value).unwrap_inst());
                at_start[block] = value.into();
            } else if joins[block] {
                takes_parameter[block] = true;
                at_start[block] = func.dfg.append_block_param(block, self.pointer_type).into();
            }
        }

        // Replace every use by the definition that reaches it, and pass each
        // block's last definition to the joins that take a parameter.
        for &block in &blocks {
            let mut current = wanted_at_start[block].then(|| {
                match reaching[block].expect("a block in reverse post-order is reached") {
                    Definition::Entry => self.base,
                    Definition::After(call) => after[&call],
                    Definition::Start(join) => at_start[join].expect("a wanted join is defined"),
                }
            });
            let mut pos = FuncCursor::new(func).at_top(block);
            while let Some(inst) = pos.next_inst() {
                if loads.contains(&inst) {
                    continue;
                }
                if self.is_load(pos.func, inst) {
                    let value = current.expect("a definition of the base reaches each use");
                    let result = pos.func.dfg.first_result(inst);
                    pos.func.dfg.clear_results(inst);
                    pos.func.dfg.change_to_alias(result, value);
                    pos.remove_inst_and_step_back();
                } else if splits(pos.func, block, inst) {
                    current = after.get(&inst).copied();
                }
            }
            let Some(terminator) = func.layout.last_inst(block) else {
                continue;
            };
            let dfg = &mut func.dfg;
            for destination in dfg.insts[terminator]
                .branch_destination_mut(&mut dfg.jump_tables, &mut dfg.exception_tables)
            {
                if takes_parameter[destination.block(&dfg.value_lists)] {
                    let value = current.expect("a block before a wanted join defines the base");
                    destination.append_argument(value, &mut dfg.value_lists);
                }
            }
        }

        let reads_home = !loads.is_empty()
            || func.layout.blocks().any(|block| {
                !domtree.is_reachable(block)
                    && func
                        .layout
                        .block_insts(block)
                        .any(|inst| self.is_load(func, inst))
            });
        if !reads_home {
            func.layout.remove_inst(self.store);
            func.sized_stack_slots[self.slot].size = 0;
        }
    }
}

/// The first use of the base or call in a block.
#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum First {
    #[default]
    Neither,
    Use,
    Call,
}

/// A definition of the base.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Definition {
    /// The entry block's value.
    Entry,
    /// The load after this call, which does not end its block.
    After(ir::Inst),
    /// A parameter of this block, or a load at its start.
    Start(ir::Block),
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::{
        AbiParam, BlockArg, ExceptionTableData, Function, Signature, UserFuncName, types::I64,
    };
    use cranelift_codegen::isa::{CallConv, TargetFrontendConfig};
    use cranelift_codegen::settings;
    use cranelift_codegen::verifier::verify_function;
    use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};

    const FRONTEND: TargetFrontendConfig = TargetFrontendConfig {
        default_call_conv: CallConv::SystemV,
        pointer_width: target_lexicon::PointerWidth::U64,
        page_size_align_log2: 12,
    };

    /// A function `(i64 base, i64 condition) -> i64` under construction, with
    /// its home stored in the entry block and a callee to call.
    struct Harness {
        func: Function,
        context: FunctionBuilderContext,
    }

    struct Body<'a> {
        builder: FunctionBuilder<'a>,
        home: GcHeapBaseHome,
        callee: ir::FuncRef,
        signature: ir::SigRef,
        condition: ir::Value,
    }

    impl Harness {
        fn new() -> Self {
            let mut signature = Signature::new(CallConv::SystemV);
            signature.params.push(AbiParam::new(I64));
            signature.params.push(AbiParam::new(I64));
            signature.returns.push(AbiParam::new(I64));
            Self {
                func: Function::with_name_signature(UserFuncName::testcase("home"), signature),
                context: FunctionBuilderContext::new(),
            }
        }

        fn body(&mut self) -> Body<'_> {
            let mut builder = FunctionBuilder::new(&mut self.func, &mut self.context);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let base = builder.block_params(entry)[0];
            let condition = builder.block_params(entry)[1];
            let signature = builder
                .func
                .import_signature(Signature::new(CallConv::SystemV));
            let name = builder
                .func
                .declare_imported_user_function(ir::UserExternalName::new(0, 0));
            let callee = builder.func.import_function(ir::ExtFuncData {
                name: ir::ExternalName::user(name),
                signature,
                colocated: true,
                patchable: false,
            });
            let region = builder.func.dfg.alias_regions.insert(ir::AliasRegionData {
                user_id: 1,
                description: "home".into(),
            });
            let home = GcHeapBaseHome::store_at_entry(&mut builder.cursor(), region, I64, base);
            Body {
                builder,
                home,
                callee,
                signature,
                condition,
            }
        }

        /// Split the base at calls and return how many loads from the home
        /// remain and whether its store does.
        fn split(&mut self, home: GcHeapBaseHome) -> (usize, bool) {
            home.split_at_calls(&mut self.func);
            let flags = settings::Flags::new(settings::builder());
            if let Err(errors) = verify_function(&self.func, &flags) {
                panic!("{errors}\n{}", self.func.display());
            }
            let func = &self.func;
            let loads = func
                .layout
                .blocks()
                .flat_map(|block| func.layout.block_insts(block))
                .filter(|&inst| home.is_load(func, inst))
                .count();
            (loads, func.layout.inst_block(home.store).is_some())
        }

        fn resolve(&self, value: ir::Value) -> ir::Value {
            self.func.dfg.resolve_aliases(value)
        }

        /// Whether `value` is the result of a load from the home.
        fn is_load(&self, home: GcHeapBaseHome, value: ir::Value) -> bool {
            self.func
                .dfg
                .value_def(self.resolve(value))
                .inst()
                .is_some_and(|inst| home.is_load(&self.func, inst))
        }

        /// The arguments `from`'s terminator passes to `to`, once resolved.
        fn arguments(&self, from: ir::Block, to: ir::Block) -> Vec<ir::Value> {
            let dfg = &self.func.dfg;
            let terminator = self.func.layout.last_inst(from).unwrap();
            dfg.insts[terminator]
                .branch_destination(&dfg.jump_tables, &dfg.exception_tables)
                .iter()
                .filter(|destination| destination.block(&dfg.value_lists) == to)
                .flat_map(|destination| destination.args(&dfg.value_lists))
                .map(|argument| match argument {
                    BlockArg::Value(value) => self.resolve(value),
                    other => panic!("unexpected block argument {other:?}"),
                })
                .collect()
        }
    }

    impl Body<'_> {
        fn load(&mut self) -> ir::Value {
            self.home.load(&mut self.builder.cursor())
        }

        fn call(&mut self) {
            self.builder.ins().call(self.callee, &[]);
        }

        fn block(&mut self) -> ir::Block {
            self.builder.create_block()
        }

        fn switch_to(&mut self, block: ir::Block) {
            self.builder.switch_to_block(block);
        }

        fn finish(mut self, result: ir::Value) -> GcHeapBaseHome {
            self.builder.ins().return_(&[result]);
            self.builder.seal_all_blocks();
            let home = self.home;
            self.builder.finalize(FRONTEND);
            home
        }
    }

    #[test]
    fn a_function_without_calls_keeps_no_home() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let first = body.load();
        let second = body.load();
        let sum = body.builder.ins().iadd(first, second);
        let home = body.finish(sum);
        assert_eq!(harness.split(home), (0, false));
        assert_eq!(harness.func.sized_stack_slots[home.slot].size, 0);
        assert_eq!(harness.resolve(first), home.base);
        assert_eq!(harness.resolve(second), home.base);
    }

    #[test]
    fn a_call_is_followed_by_one_load_when_a_use_follows() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let before = body.load();
        body.call();
        let after = [body.load(), body.load()];
        // A trapping instruction is no call.
        body.builder
            .ins()
            .trapz(body.condition, ir::TrapCode::unwrap_user(1));
        let later = body.load();
        // No use follows these calls before the next one.
        body.call();
        body.call();
        let last = body.load();
        body.call();
        let mut sum = before;
        for value in after.into_iter().chain([later, last]) {
            sum = body.builder.ins().iadd(sum, value);
        }
        let home = body.finish(sum);
        assert_eq!(harness.split(home), (2, true));
        assert_eq!(harness.resolve(before), home.base);
        assert!(harness.is_load(home, after[0]));
        assert_eq!(harness.resolve(after[1]), harness.resolve(after[0]));
        assert_eq!(harness.resolve(later), harness.resolve(after[0]));
        assert!(harness.is_load(home, last));
        assert_ne!(harness.resolve(last), harness.resolve(after[0]));
    }

    #[test]
    fn a_path_without_a_call_passes_its_value_to_the_join() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let entry = body.builder.current_block().unwrap();
        let slow = body.block();
        let join = body.block();
        let before = body.load();
        body.builder
            .ins()
            .brif(body.condition, slow, &[], join, &[]);

        body.switch_to(slow);
        body.call();
        body.builder.ins().jump(join, &[]);

        body.switch_to(join);
        let first = body.load();
        let second = body.load();
        let mut sum = body.builder.ins().iadd(before, first);
        sum = body.builder.ins().iadd(sum, second);
        let home = body.finish(sum);
        // Only the path that called loads.
        assert_eq!(harness.split(home), (1, true));
        assert_eq!(harness.resolve(before), home.base);
        let parameter = harness.resolve(first);
        assert_eq!(harness.resolve(second), parameter);
        assert_eq!(harness.func.dfg.block_params(join), &[parameter]);
        assert_eq!(harness.arguments(entry, join), [home.base]);
        let reloaded = harness.arguments(slow, join);
        assert_eq!(reloaded.len(), 1);
        assert!(harness.is_load(home, reloaded[0]));
    }

    #[test]
    fn blocks_below_a_join_use_its_parameter() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let slow = body.block();
        let join = body.block();
        let then = body.block();
        let otherwise = body.block();
        let last = body.block();
        body.builder
            .ins()
            .brif(body.condition, slow, &[], join, &[]);

        body.switch_to(slow);
        body.call();
        body.builder.ins().jump(join, &[]);

        body.switch_to(join);
        body.builder
            .ins()
            .brif(body.condition, then, &[], otherwise, &[]);

        body.switch_to(then);
        let in_then = body.load();
        body.builder.ins().jump(last, &[]);

        body.switch_to(otherwise);
        body.builder.ins().jump(last, &[]);

        body.switch_to(last);
        let in_last = body.load();
        let sum = body.builder.ins().iadd(in_last, in_last);
        let home = body.finish(sum);
        assert_eq!(harness.split(home), (1, true));
        let parameter = harness.resolve(in_then);
        assert_eq!(harness.func.dfg.block_params(join), &[parameter]);
        assert_eq!(harness.resolve(in_last), parameter);
        for block in [then, otherwise, last] {
            assert!(harness.func.dfg.block_params(block).is_empty());
        }
    }

    #[test]
    fn a_join_of_paths_with_one_value_takes_no_parameter() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let then = body.block();
        let otherwise = body.block();
        let join = body.block();
        body.call();
        let before = body.load();
        body.builder
            .ins()
            .brif(body.condition, then, &[], otherwise, &[]);

        body.switch_to(then);
        let in_then = body.load();
        body.builder.ins().jump(join, &[]);

        body.switch_to(otherwise);
        body.builder.ins().jump(join, &[]);

        body.switch_to(join);
        let after = body.load();
        let sum = body.builder.ins().iadd(in_then, after);
        let home = body.finish(sum);
        assert_eq!(harness.split(home), (1, true));
        assert!(harness.is_load(home, before));
        for value in [in_then, after] {
            assert_eq!(harness.resolve(value), harness.resolve(before));
        }
        assert!(harness.func.dfg.block_params(join).is_empty());
    }

    #[test]
    fn a_loop_that_calls_keeps_the_value_it_was_entered_with() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let header = body.block();
        let exit = body.block();
        body.builder.ins().jump(header, &[]);

        body.switch_to(header);
        let in_loop = body.load();
        body.call();
        let after_call = body.load();
        body.builder.ins().iadd(in_loop, after_call);
        body.builder
            .ins()
            .brif(body.condition, header, &[], exit, &[]);

        body.switch_to(exit);
        let after = body.load();
        let home = body.finish(after);
        // The loop's value ends where the loop is left.
        assert_eq!(harness.split(home), (1, true));
        assert_eq!(harness.resolve(in_loop), home.base);
        assert_eq!(harness.resolve(after_call), home.base);
        assert!(harness.func.dfg.block_params(header).is_empty());
        assert!(harness.is_load(home, after));
        assert_eq!(
            harness.func.layout.inst_block(
                harness
                    .func
                    .dfg
                    .value_def(harness.resolve(after))
                    .unwrap_inst()
            ),
            Some(exit)
        );
    }

    #[test]
    fn a_loop_entered_with_two_values_passes_its_parameter_back() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let entry = body.builder.current_block().unwrap();
        let slow = body.block();
        let header = body.block();
        let exit = body.block();
        body.builder
            .ins()
            .brif(body.condition, slow, &[], header, &[]);

        body.switch_to(slow);
        body.call();
        body.builder.ins().jump(header, &[]);

        body.switch_to(header);
        let in_loop = body.load();
        body.call();
        body.builder
            .ins()
            .brif(body.condition, header, &[], exit, &[]);

        body.switch_to(exit);
        let home = body.finish(in_loop);
        assert_eq!(harness.split(home), (1, true));
        let parameter = harness.resolve(in_loop);
        assert_eq!(harness.func.dfg.block_params(header), &[parameter]);
        assert_eq!(harness.arguments(entry, header), [home.base]);
        assert!(harness.is_load(home, harness.arguments(slow, header)[0]));
        assert_eq!(harness.arguments(header, header), [parameter]);
    }

    #[test]
    fn a_loop_without_a_call_keeps_the_value_it_was_entered_with() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let header = body.block();
        let exit = body.block();
        body.call();
        body.builder.ins().jump(header, &[]);

        body.switch_to(header);
        let in_loop = body.load();
        body.builder
            .ins()
            .brif(body.condition, header, &[], exit, &[]);

        body.switch_to(exit);
        let after = body.load();
        let sum = body.builder.ins().iadd(in_loop, after);
        let home = body.finish(sum);
        assert_eq!(harness.split(home), (1, true));
        assert!(harness.is_load(home, in_loop));
        assert_eq!(harness.resolve(after), harness.resolve(in_loop));
        assert!(harness.func.dfg.block_params(header).is_empty());
    }

    #[test]
    fn the_successors_of_a_call_that_ends_its_block_load_the_base() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let returned = body.block();
        let caught = body.block();
        let join = body.block();
        let before = body.load();
        let normal = body.builder.func.dfg.block_call(returned, &[]);
        let handler = body.builder.func.dfg.block_call(caught, &[]);
        let table = body
            .builder
            .func
            .dfg
            .exception_tables
            .push(ExceptionTableData::new(
                body.signature,
                normal,
                [ir::ExceptionTableItem::Default(handler)],
            ));
        body.builder.ins().try_call(body.callee, &[], table);

        body.switch_to(returned);
        let in_returned = body.load();
        body.builder.ins().iadd(in_returned, in_returned);
        body.builder.ins().jump(join, &[]);

        // This path does not use the base, so it loads it only for the join.
        body.switch_to(caught);
        body.builder.ins().jump(join, &[]);

        body.switch_to(join);
        let after = body.load();
        let sum = body.builder.ins().iadd(before, after);
        let home = body.finish(sum);
        assert_eq!(harness.split(home), (2, true));
        assert_eq!(harness.resolve(before), home.base);
        assert!(harness.is_load(home, in_returned));
        let parameter = harness.resolve(after);
        assert_eq!(harness.func.dfg.block_params(join), &[parameter]);
        assert_eq!(
            harness.arguments(returned, join),
            [harness.resolve(in_returned)]
        );
        let from_caught = harness.arguments(caught, join);
        assert!(harness.is_load(home, from_caught[0]));
        assert_ne!(from_caught[0], harness.resolve(in_returned));
    }

    #[test]
    fn every_branch_of_a_table_passes_the_value() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let entry = body.builder.current_block().unwrap();
        let slow = body.block();
        let join = body.block();
        // Each branch owns its argument list, as the frontend requires.
        let default = body.builder.func.dfg.block_call(slow, &[]);
        let branches = [(); 2].map(|()| body.builder.func.dfg.block_call(join, &[]));
        let table = body
            .builder
            .create_jump_table(ir::JumpTableData::new(default, &branches));
        let index = body.builder.ins().ireduce(ir::types::I32, body.condition);
        body.builder.ins().br_table(index, table);

        body.switch_to(slow);
        body.call();
        body.builder.ins().jump(join, &[]);

        body.switch_to(join);
        let after = body.load();
        let home = body.finish(after);
        assert_eq!(harness.split(home), (1, true));
        assert_eq!(harness.arguments(entry, join), [home.base, home.base]);
    }
}
