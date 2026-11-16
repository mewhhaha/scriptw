//! Code that runs only after an unlikely branch.
//!
//! Cranelift lays out the blocks that the layout marks cold behind all other
//! code of the function, and marks nothing itself. The translator marks the
//! one block an unlikely branch enters: the arm of a hinted `if`, the block a
//! hinted branch falls through to or its edge (see `conditional_branch` in
//! the code translator), the block that calls the collector, a trap. What
//! that block goes on to, the rest of a slow path of several blocks, would
//! stay between the likely code: the calls of a helper fallback, the loop of
//! a growth path, the nested tests of an initialization.
//!
//! [`spread`] marks every block that control reaches only through a block
//! already marked. A block is likely when a path of blocks that are not
//! marked leads to it from the entry; all other blocks are marked. A join
//! that a likely path reaches stays where it was, so a slow path ends where
//! it rejoins.
//!
//! # Why this is correct
//!
//! The mark is a layout property. It selects where the backend emits a
//! block; no instruction, value, edge, stack map or exception table depends
//! on it, and the verifier only requires that the entry block is not cold,
//! which is likely by definition here. The function computes what it
//! computed before.
//!
//! Whether the placement is good is a matter of the marks it starts from: a
//! block marked here executes only when a marked block executed first, so it
//! runs no more often than the unlikely code before it.

use cranelift_codegen::ir;
use cranelift_entity::EntitySet;

/// Mark cold every block of `func` that is reached only through cold blocks,
/// and return how many blocks that marked.
pub(crate) fn spread(func: &mut ir::Function) -> usize {
    let Some(entry) = func.layout.entry_block() else {
        return 0;
    };
    if !func.layout.blocks().any(|block| func.layout.is_cold(block)) {
        return 0;
    }
    let mut likely = EntitySet::with_capacity(func.dfg.num_blocks());
    likely.insert(entry);
    let mut pending = vec![entry];
    while let Some(block) = pending.pop() {
        let Some(terminator) = func.layout.last_inst(block) else {
            continue;
        };
        for destination in func.dfg.insts[terminator]
            .branch_destination(&func.dfg.jump_tables, &func.dfg.exception_tables)
        {
            let target = destination.block(&func.dfg.value_lists);
            if !func.layout.is_cold(target) && likely.insert(target) {
                pending.push(target);
            }
        }
    }
    let rest: Vec<ir::Block> = func
        .layout
        .blocks()
        .filter(|block| !likely.contains(*block) && !func.layout.is_cold(*block))
        .collect();
    for block in &rest {
        func.layout.set_cold(*block);
    }
    rest.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::{
        AbiParam, Function, InstBuilder, Signature, UserFuncName, types::I32,
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

    /// Builds `(i32, i32) -> i32` from `body`, spreads and verifies it.
    fn spread_of(
        body: impl FnOnce(&mut FunctionBuilder<'_>, ir::Value, ir::Value),
    ) -> (Function, usize) {
        let mut signature = Signature::new(CallConv::SystemV);
        signature.params.push(AbiParam::new(I32));
        signature.params.push(AbiParam::new(I32));
        signature.returns.push(AbiParam::new(I32));
        let mut func = Function::with_name_signature(UserFuncName::testcase("cold"), signature);
        let mut context = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut context);
        let entry = builder.create_block();
        builder.append_block_params_for_function_params(entry);
        builder.switch_to_block(entry);
        let [left, right] = *builder.block_params(entry) else {
            unreachable!()
        };
        body(&mut builder, left, right);
        builder.seal_all_blocks();
        builder.finalize(FRONTEND);
        let marked = spread(&mut func);
        let flags = settings::Flags::new(settings::builder());
        if let Err(errors) = verify_function(&func, &flags) {
            panic!("{errors}\n{}", func.display());
        }
        (func, marked)
    }

    fn cold(func: &Function) -> Vec<bool> {
        func.layout
            .blocks()
            .map(|block| func.layout.is_cold(block))
            .collect()
    }

    #[test]
    fn a_function_without_cold_blocks_is_left_alone() {
        let (func, marked) = spread_of(|builder, left, right| {
            let then = builder.create_block();
            let join = builder.create_block();
            builder.ins().brif(left, then, &[], join, &[]);
            builder.switch_to_block(then);
            builder.ins().jump(join, &[]);
            builder.switch_to_block(join);
            builder.ins().return_(&[right]);
        });
        assert_eq!(marked, 0);
        assert_eq!(cold(&func), [false, false, false]);
    }

    #[test]
    fn the_blocks_of_a_slow_path_follow_its_first_block_and_the_join_does_not() {
        let (func, marked) = spread_of(|builder, left, right| {
            let slow = builder.create_block();
            let slow_then = builder.create_block();
            let slow_join = builder.create_block();
            let join = builder.create_block();
            builder.set_cold_block(slow);
            builder.ins().brif(left, slow, &[], join, &[]);
            builder.switch_to_block(slow);
            builder.ins().brif(right, slow_then, &[], slow_join, &[]);
            builder.switch_to_block(slow_then);
            builder.ins().jump(slow_join, &[]);
            builder.switch_to_block(slow_join);
            builder.ins().jump(join, &[]);
            builder.switch_to_block(join);
            builder.ins().return_(&[right]);
        });
        assert_eq!(marked, 2);
        // entry, slow, slow_then, slow_join, join
        assert_eq!(cold(&func), [false, true, true, true, false]);
    }

    #[test]
    fn a_block_that_only_cold_edges_reach_is_cold_with_what_follows_it() {
        // Two unlikely branches meet in a fallback that is not marked itself.
        let (func, marked) = spread_of(|builder, left, right| {
            let first_edge = builder.create_block();
            let second = builder.create_block();
            let second_edge = builder.create_block();
            let fast = builder.create_block();
            let fallback = builder.create_block();
            let fallback_tail = builder.create_block();
            let join = builder.create_block();
            builder.set_cold_block(first_edge);
            builder.set_cold_block(second_edge);
            builder.ins().brif(left, first_edge, &[], second, &[]);
            builder.switch_to_block(first_edge);
            builder.ins().jump(fallback, &[]);
            builder.switch_to_block(second);
            builder.ins().brif(right, second_edge, &[], fast, &[]);
            builder.switch_to_block(second_edge);
            builder.ins().jump(fallback, &[]);
            builder.switch_to_block(fast);
            builder.ins().jump(join, &[]);
            builder.switch_to_block(fallback);
            builder.ins().jump(fallback_tail, &[]);
            builder.switch_to_block(fallback_tail);
            builder.ins().jump(join, &[]);
            builder.switch_to_block(join);
            builder.ins().return_(&[right]);
        });
        assert_eq!(marked, 2);
        // entry, first_edge, second, second_edge, fast, fallback,
        // fallback_tail, join
        assert_eq!(
            cold(&func),
            [false, true, false, true, false, true, true, false]
        );
    }

    #[test]
    fn a_likely_edge_keeps_a_block_likely() {
        // The fallback is also where the likely path goes.
        let (func, marked) = spread_of(|builder, left, right| {
            let edge = builder.create_block();
            let shared = builder.create_block();
            builder.set_cold_block(edge);
            builder.ins().brif(left, edge, &[], shared, &[]);
            builder.switch_to_block(edge);
            builder.ins().jump(shared, &[]);
            builder.switch_to_block(shared);
            builder.ins().return_(&[right]);
        });
        assert_eq!(marked, 0);
        assert_eq!(cold(&func), [false, true, false]);
    }

    #[test]
    fn a_loop_inside_a_slow_path_is_cold_and_a_loop_it_returns_to_is_not() {
        let (func, marked) = spread_of(|builder, left, right| {
            let head = builder.create_block();
            let slow = builder.create_block();
            let slow_loop = builder.create_block();
            let slow_exit = builder.create_block();
            let exit = builder.create_block();
            builder.set_cold_block(slow);
            builder.ins().jump(head, &[]);
            builder.switch_to_block(head);
            builder.ins().brif(left, slow, &[], exit, &[]);
            builder.switch_to_block(slow);
            builder.ins().jump(slow_loop, &[]);
            builder.switch_to_block(slow_loop);
            builder.ins().brif(right, slow_loop, &[], slow_exit, &[]);
            builder.switch_to_block(slow_exit);
            builder.ins().jump(head, &[]);
            builder.switch_to_block(exit);
            builder.ins().return_(&[right]);
        });
        assert_eq!(marked, 2);
        // entry, head, slow, slow_loop, slow_exit, exit
        assert_eq!(cold(&func), [false, false, true, true, true, false]);
    }
}
