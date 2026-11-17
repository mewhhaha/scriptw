//! The values an instruction uses, read without the parts of
//! `DataFlowGraph::inst_values` that only terminators have.
//!
//! `inst_values` chains an instruction's arguments with the arguments of its
//! branch destinations and the contexts of its exception table. Only a
//! terminator has either, so for every other instruction the values are its
//! arguments alone, in the same order. The passes after translation that
//! look at every value of a large function spent most of their time in that
//! chain.

use cranelift_codegen::ir;

/// Call `f` with each value `inst` uses, in the order of
/// `DataFlowGraph::inst_values`, aliases unresolved.
pub(crate) fn for_each(dfg: &ir::DataFlowGraph, inst: ir::Inst, mut f: impl FnMut(ir::Value)) {
    if dfg.insts[inst].opcode().is_terminator() {
        dfg.inst_values(inst).for_each(f);
    } else {
        debug_assert_only_arguments(dfg, inst);
        for &value in dfg.inst_args(inst) {
            f(value);
        }
    }
}

/// Whether `inst` uses a value for which `predicate` holds, as
/// `dfg.inst_values(inst).any(predicate)` would answer.
pub(crate) fn any(
    dfg: &ir::DataFlowGraph,
    inst: ir::Inst,
    mut predicate: impl FnMut(ir::Value) -> bool,
) -> bool {
    if dfg.insts[inst].opcode().is_terminator() {
        dfg.inst_values(inst).any(predicate)
    } else {
        debug_assert_only_arguments(dfg, inst);
        dfg.inst_args(inst).iter().any(|&value| predicate(value))
    }
}

fn debug_assert_only_arguments(dfg: &ir::DataFlowGraph, inst: ir::Inst) {
    debug_assert!(
        dfg.insts[inst]
            .branch_destination(&dfg.jump_tables, &dfg.exception_tables)
            .is_empty()
            && dfg.insts[inst].exception_table().is_none(),
        "only a terminator has branch destinations or an exception table"
    );
}
