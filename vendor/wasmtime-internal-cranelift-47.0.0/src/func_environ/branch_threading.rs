//! Branches on a truth value that paths only just chose.
//!
//! Translating Wasm leaves many blocks of this shape:
//!
//! ```text
//! block9(v20: i32):
//!     brif v20, block10, block11
//! ```
//!
//! whose predecessors each pass a constant or the result of a comparison:
//! the end of `ref.test` followed by `if`, `br_on_cast`, the end of a Wasm
//! `if ... else ... end` that yields a condition. Cranelift compiles the
//! block as written: every path materializes the truth value in a register
//! (`sete`, `movzx`), moves it to the block's parameter and tests it again.
//!
//! [`thread`] sends each such edge where the block would send it:
//!
//! * a predecessor that passes a constant jumps to the successor that the
//!   constant selects;
//! * a predecessor that reaches the block on the edge of its own `brif` and
//!   passes that `brif`'s condition goes to the successor that the edge's
//!   outcome selects;
//! * a predecessor that ends in a `jump` and passes any other value ends in
//!   a copy of the block's `brif` on that value instead.
//!
//! Other edges, and so possibly the block, stay as they were.
//!
//! # Why this is correct
//!
//! The pass handles a block `C` only when the `brif` on one of its
//! parameters is its only instruction and no instruction of the function
//! other than that `brif` uses any of `C`'s parameters. Entering `C` on an
//! edge with arguments `a` therefore does exactly one thing: it continues at
//! the successor that `a`'s condition argument selects, with that
//! successor's arguments, in which `C`'s parameters stand for `a`. The new
//! edge does the same, and nothing else read the parameters that the path
//! no longer defines. `C` itself is not changed, so the edges that still
//! enter it behave as before.
//!
//! The values the new edge passes are the predecessor's own arguments, which
//! are available at its terminator, and values that the `brif` named and
//! that are not parameters of `C`: those are defined on every path to `C`
//! before `C` is entered, so on every path to the end of each predecessor of
//! `C`. The result is valid SSA.
//!
//! On the true edge of a predecessor's `brif v` the value `v` is nonzero,
//! on its false edge zero, which is how the block's `brif` on the same
//! value would decide.
//!
//! No call, store, trap or stack map is added, removed or reordered.

use cranelift_codegen::ir::{self, InstBuilder};
use cranelift_entity::SecondaryMap;
use cranelift_entity::packed_option::PackedOption;
use smallvec::SmallVec;

/// How often the pass looks at the whole function. An edge threaded in one
/// round can make its source a block of the handled shape for the next.
const MAX_ROUNDS: usize = 3;

/// A block whose only instruction is a `brif` on its parameter `condition`.
#[derive(Clone, Copy)]
struct Candidate {
    brif: ir::Inst,
    /// The index of the parameter the `brif` tests.
    condition: usize,
}

type Arguments = SmallVec<[ir::BlockArg; 8]>;

/// Thread the edges of `func` into blocks that only branch on a parameter
/// (see the module documentation), and return how many edges were changed.
pub(crate) fn thread(func: &mut ir::Function) -> usize {
    let mut threaded = 0;
    let mut uses: Option<OutsideUses> = None;
    for _ in 0..MAX_ROUNDS {
        let round = thread_once(func, &mut uses);
        threaded += round;
        if round == 0 {
            break;
        }
    }
    threaded
}

/// For each block parameter, how many uses of it (aliases resolved) the
/// instructions of other blocks make.
///
/// A block of the handled shape is a candidate exactly when none of its
/// parameters has such a use. Counted once, by one walk over the function,
/// and then kept current through each terminator the pass rewrites, the only
/// instructions it changes; the rounds after the first therefore look at
/// blocks, not at every value of every instruction again.
struct OutsideUses {
    /// The block that has each value as its parameter.
    owners: SecondaryMap<ir::Value, PackedOption<ir::Block>>,
    counts: SecondaryMap<ir::Value, u32>,
}

impl OutsideUses {
    fn new(func: &ir::Function) -> Self {
        let mut owners = SecondaryMap::with_capacity(func.dfg.num_values());
        for block in func.layout.blocks() {
            for &parameter in func.dfg.block_params(block) {
                owners[parameter] = block.into();
            }
        }
        let mut uses = Self {
            owners,
            counts: SecondaryMap::with_capacity(func.dfg.num_values()),
        };
        for block in func.layout.blocks() {
            for inst in func.layout.block_insts(block) {
                uses.count(func, block, inst, true);
            }
        }
        uses
    }

    /// Count the uses of `inst`, an instruction of `block`, in (`add`) or
    /// out.
    fn count(&mut self, func: &ir::Function, block: ir::Block, inst: ir::Inst, add: bool) {
        super::operands::for_each(&func.dfg, inst, |value| {
            let value = func.dfg.resolve_aliases(value);
            if let Some(owner) = self.owners[value].expand() {
                if owner != block {
                    if add {
                        self.counts[value] += 1;
                    } else {
                        self.counts[value] -= 1;
                    }
                }
            }
        });
    }

    fn used_outside(&self, func: &ir::Function, block: ir::Block) -> bool {
        func.dfg
            .block_params(block)
            .iter()
            .any(|&parameter| self.counts[parameter] != 0)
    }
}

fn thread_once(func: &mut ir::Function, uses: &mut Option<OutsideUses>) -> usize {
    // Blocks of the shape, by their layout.
    let mut candidates: SecondaryMap<ir::Block, Option<Candidate>> = SecondaryMap::new();
    let mut any = false;
    for block in func.layout.blocks() {
        let Some(inst) = func.layout.first_inst(block) else {
            continue;
        };
        if func.layout.last_inst(block) != Some(inst) {
            continue;
        }
        let ir::InstructionData::Brif { arg, blocks, .. } = func.dfg.insts[inst] else {
            continue;
        };
        let ir::ValueDef::Param(owner, condition) =
            func.dfg.value_def(func.dfg.resolve_aliases(arg))
        else {
            continue;
        };
        if owner != block
            || blocks
                .iter()
                .any(|call| call.block(&func.dfg.value_lists) == block)
        {
            continue;
        }
        candidates[block] = Some(Candidate {
            brif: inst,
            condition,
        });
        any = true;
    }
    if !any {
        return 0;
    }

    // A candidate whose parameter something other than its own `brif` uses
    // is not one; and each candidate's entering edges.
    let uses = uses.get_or_insert_with(|| OutsideUses::new(func));
    let mut edges: Vec<(ir::Inst, ir::Block)> = Vec::new();
    for block in func.layout.blocks() {
        if candidates[block].is_some() && uses.used_outside(func, block) {
            candidates[block] = None;
        }
    }
    for block in func.layout.blocks() {
        if let Some(terminator) = func.layout.last_inst(block) {
            let first = edges.len();
            for destination in func.dfg.insts[terminator]
                .branch_destination(&func.dfg.jump_tables, &func.dfg.exception_tables)
            {
                let target = destination.block(&func.dfg.value_lists);
                // A block that branches to itself is left alone.
                if target != block
                    && candidates[target].is_some()
                    && !edges[first..].contains(&(terminator, target))
                {
                    edges.push((terminator, target));
                }
            }
        }
    }

    let mut threaded = 0;
    for (terminator, target) in edges {
        let Some(candidate) = candidates[target] else {
            continue;
        };
        // A candidate's own `brif` may have been retargeted in this round;
        // read it as it is now.
        let ir::InstructionData::Brif {
            blocks: outcomes, ..
        } = func.dfg.insts[candidate.brif]
        else {
            unreachable!("a candidate's instruction is its `brif`")
        };
        // The arguments `call` would pass if `target`'s parameters were
        // `passed`.
        let substituted = |func: &ir::Function, call: ir::BlockCall, passed: &Arguments| {
            let arguments: Arguments = call
                .args(&func.dfg.value_lists)
                .map(|argument| match argument {
                    ir::BlockArg::Value(value) => {
                        let value = func.dfg.resolve_aliases(value);
                        match func.dfg.value_def(value) {
                            ir::ValueDef::Param(owner, index) if owner == target => passed[index],
                            _ => ir::BlockArg::Value(value),
                        }
                    }
                    other => other,
                })
                .collect();
            (call.block(&func.dfg.value_lists), arguments)
        };
        let passed_on = |func: &ir::Function, call: ir::BlockCall| -> Option<Arguments> {
            let passed: Arguments = call.args(&func.dfg.value_lists).collect();
            // Only plain values can be passed on.
            passed
                .iter()
                .all(|argument| matches!(argument, ir::BlockArg::Value(_)))
                .then_some(passed)
        };
        let block = func
            .layout
            .inst_block(terminator)
            .expect("an edge's terminator is in the layout");
        match func.dfg.insts[terminator] {
            ir::InstructionData::Jump { destination, .. } => {
                debug_assert_eq!(destination.block(&func.dfg.value_lists), target);
                let Some(passed) = passed_on(func, destination) else {
                    continue;
                };
                let ir::BlockArg::Value(condition) = passed[candidate.condition] else {
                    unreachable!()
                };
                let condition = func.dfg.resolve_aliases(condition);
                uses.count(func, block, terminator, false);
                match constant(func, condition) {
                    Some(truth) => {
                        let (destination, arguments) =
                            substituted(func, outcomes[usize::from(!truth)], &passed);
                        func.replace(terminator).jump(destination, &arguments);
                    }
                    None => {
                        let (then_block, then_arguments) = substituted(func, outcomes[0], &passed);
                        let (else_block, else_arguments) = substituted(func, outcomes[1], &passed);
                        func.replace(terminator).brif(
                            condition,
                            then_block,
                            &then_arguments,
                            else_block,
                            &else_arguments,
                        );
                    }
                }
                uses.count(func, block, terminator, true);
                threaded += 1;
            }
            ir::InstructionData::Brif { arg, blocks, .. } => {
                let tested = func.dfg.resolve_aliases(arg);
                for (index, call) in blocks.into_iter().enumerate() {
                    if call.block(&func.dfg.value_lists) != target {
                        continue;
                    }
                    let Some(passed) = passed_on(func, call) else {
                        continue;
                    };
                    let ir::BlockArg::Value(condition) = passed[candidate.condition] else {
                        unreachable!()
                    };
                    let condition = func.dfg.resolve_aliases(condition);
                    // The first destination is taken when `tested` is not
                    // zero, the second when it is.
                    let truth = match constant(func, condition) {
                        Some(truth) => truth,
                        None if condition == tested => index == 0,
                        None => continue,
                    };
                    let (destination, arguments) =
                        substituted(func, outcomes[usize::from(!truth)], &passed);
                    uses.count(func, block, terminator, false);
                    let call = func.dfg.block_call(destination, &arguments);
                    let ir::InstructionData::Brif { blocks, .. } = &mut func.dfg.insts[terminator]
                    else {
                        unreachable!()
                    };
                    blocks[index] = call;
                    uses.count(func, block, terminator, true);
                    threaded += 1;
                }
            }
            // A jump table or a call that ends its block keeps its edge.
            _ => {}
        }
    }
    threaded
}

/// Whether `value` is a constant, and then whether it is nonzero.
fn constant(func: &ir::Function, value: ir::Value) -> Option<bool> {
    let inst = func.dfg.value_def(value).inst()?;
    match func.dfg.insts[inst] {
        ir::InstructionData::UnaryImm {
            opcode: ir::Opcode::Iconst,
            imm,
        } => {
            let bits = func.dfg.value_type(value).bits();
            let mask = if bits >= 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            Some(imm.bits() as u64 & mask != 0)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::{
        AbiParam, Function, Signature, UserFuncName, condcodes::IntCC, types::I32, types::I64,
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

    /// A function `(i64 selector, i32 left, i32 right) -> i32` under
    /// construction.
    struct Harness {
        func: Function,
        context: FunctionBuilderContext,
    }

    struct Body<'a> {
        builder: FunctionBuilder<'a>,
        selector: ir::Value,
        left: ir::Value,
        right: ir::Value,
    }

    impl Harness {
        fn new() -> Self {
            let mut signature = Signature::new(CallConv::SystemV);
            signature.params.push(AbiParam::new(I64));
            signature.params.push(AbiParam::new(I32));
            signature.params.push(AbiParam::new(I32));
            signature.returns.push(AbiParam::new(I32));
            Self {
                func: Function::with_name_signature(UserFuncName::testcase("thread"), signature),
                context: FunctionBuilderContext::new(),
            }
        }

        fn body(&mut self) -> Body<'_> {
            let mut builder = FunctionBuilder::new(&mut self.func, &mut self.context);
            let entry = builder.create_block();
            builder.append_block_params_for_function_params(entry);
            builder.switch_to_block(entry);
            builder.seal_block(entry);
            let [selector, left, right] = *builder.block_params(entry) else {
                unreachable!()
            };
            Body {
                builder,
                selector,
                left,
                right,
            }
        }

        fn thread(&mut self) -> usize {
            let threaded = thread(&mut self.func);
            let flags = settings::Flags::new(settings::builder());
            if let Err(errors) = verify_function(&self.func, &flags) {
                panic!("{errors}\n{}", self.func.display());
            }
            threaded
        }

        /// The blocks `block`'s terminator can go to, with the arguments it
        /// passes, and the condition of a `brif`.
        fn terminator(
            &self,
            block: ir::Block,
        ) -> (Option<ir::Value>, Vec<(ir::Block, Vec<ir::Value>)>) {
            let dfg = &self.func.dfg;
            let inst = self.func.layout.last_inst(block).unwrap();
            let condition = match dfg.insts[inst] {
                ir::InstructionData::Brif { arg, .. } => Some(dfg.resolve_aliases(arg)),
                _ => None,
            };
            let destinations = dfg.insts[inst]
                .branch_destination(&dfg.jump_tables, &dfg.exception_tables)
                .iter()
                .map(|call| {
                    let arguments = call
                        .args(&dfg.value_lists)
                        .map(|argument| match argument {
                            ir::BlockArg::Value(value) => dfg.resolve_aliases(value),
                            other => panic!("unexpected block argument {other:?}"),
                        })
                        .collect();
                    (call.block(&dfg.value_lists), arguments)
                })
                .collect();
            (condition, destinations)
        }
    }

    impl Body<'_> {
        fn block(&mut self) -> ir::Block {
            self.builder.create_block()
        }

        fn switch_to(&mut self, block: ir::Block) {
            self.builder.switch_to_block(block);
        }

        fn constant(&mut self, value: i64) -> ir::Value {
            self.builder.ins().iconst(I32, value)
        }

        /// End `block` with `return value`.
        fn returns(&mut self, block: ir::Block, value: ir::Value) {
            self.switch_to(block);
            self.builder.ins().return_(&[value]);
        }

        fn finish(mut self) {
            self.builder.seal_all_blocks();
            self.builder.finalize(FRONTEND);
        }
    }

    /// Three paths into a block that only branches on its parameter: one
    /// passes true, one false, one a comparison.
    #[test]
    fn each_edge_goes_where_its_argument_selects() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let entry = body.builder.current_block().unwrap();
        let [yes, no, maybe, join, then, otherwise] = [(); 6].map(|()| body.block());
        let table = {
            let default = body.builder.func.dfg.block_call(maybe, &[]);
            let branches = [yes, no].map(|block| body.builder.func.dfg.block_call(block, &[]));
            body.builder
                .create_jump_table(ir::JumpTableData::new(default, &branches))
        };
        let index = body.builder.ins().ireduce(I32, body.selector);
        body.builder.ins().br_table(index, table);

        body.switch_to(yes);
        let one = body.constant(1);
        body.builder.ins().jump(join, &[one.into()]);

        body.switch_to(no);
        let zero = body.constant(0);
        body.builder.ins().jump(join, &[zero.into()]);

        body.switch_to(maybe);
        let equal = body.builder.ins().icmp(IntCC::Equal, body.left, body.right);
        let equal = body.builder.ins().uextend(I32, equal);
        body.builder.ins().jump(join, &[equal.into()]);

        body.switch_to(join);
        let truth = body.builder.append_block_param(join, I32);
        body.builder.ins().brif(truth, then, &[], otherwise, &[]);

        let (left, right) = (body.left, body.right);
        body.returns(then, left);
        body.returns(otherwise, right);
        body.finish();
        assert_eq!(harness.thread(), 3);
        assert_eq!(harness.terminator(yes), (None, vec![(then, vec![])]));
        assert_eq!(harness.terminator(no), (None, vec![(otherwise, vec![])]));
        assert_eq!(
            harness.terminator(maybe),
            (Some(equal), vec![(then, vec![]), (otherwise, vec![])])
        );
        // The entry's jump table is untouched.
        assert_eq!(harness.terminator(entry).1.len(), 3);
    }

    #[test]
    fn the_block_s_other_parameters_stand_for_what_each_edge_passes() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let [slow, join, then, otherwise] = [(); 4].map(|()| body.block());
        let (left, right) = (body.left, body.right);
        let wide = body.builder.ins().ireduce(I32, body.selector);
        body.builder
            .ins()
            .brif(body.selector, slow, &[], join, &[left.into(), right.into()]);

        body.switch_to(slow);
        let one = body.constant(1);
        body.builder.ins().jump(join, &[one.into(), wide.into()]);

        body.switch_to(join);
        let truth = body.builder.append_block_param(join, I32);
        let carried = body.builder.append_block_param(join, I32);
        // The `brif` passes the block's parameters and an outer value on.
        body.builder.ins().brif(
            truth,
            then,
            &[carried.into(), truth.into()],
            otherwise,
            &[wide.into(), carried.into()],
        );

        body.switch_to(then);
        let first = body.builder.append_block_param(then, I32);
        let second = body.builder.append_block_param(then, I32);
        let sum = body.builder.ins().iadd(first, second);
        body.builder.ins().return_(&[sum]);

        body.switch_to(otherwise);
        let first = body.builder.append_block_param(otherwise, I32);
        let second = body.builder.append_block_param(otherwise, I32);
        let sum = body.builder.ins().iadd(first, second);
        body.builder.ins().return_(&[sum]);
        body.finish();
        // The entry reaches the join on the false edge of a `brif` on
        // another value: that edge stays.
        assert_eq!(harness.thread(), 1);
        assert_eq!(
            harness.terminator(slow),
            (None, vec![(then, vec![wide, one])])
        );
    }

    #[test]
    fn an_edge_that_passes_its_own_condition_knows_the_outcome() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let entry = body.builder.current_block().unwrap();
        let [slow, join, then, otherwise] = [(); 4].map(|()| body.block());
        let (left, right) = (body.left, body.right);
        // The shape of a subtype test: equal types need no call.
        body.builder
            .ins()
            .brif(left, join, &[left.into()], slow, &[]);

        body.switch_to(slow);
        body.builder.ins().jump(join, &[right.into()]);

        body.switch_to(join);
        let truth = body.builder.append_block_param(join, I32);
        body.builder.ins().brif(truth, then, &[], otherwise, &[]);

        body.returns(then, left);
        body.returns(otherwise, right);
        body.finish();
        assert_eq!(harness.thread(), 2);
        assert_eq!(
            harness.terminator(entry),
            (Some(left), vec![(then, vec![]), (slow, vec![])])
        );
        assert_eq!(
            harness.terminator(slow),
            (Some(right), vec![(then, vec![]), (otherwise, vec![])])
        );
    }

    #[test]
    fn a_threaded_block_is_threaded_in_turn() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let entry = body.builder.current_block().unwrap();
        let [inner, outer, then, otherwise] = [(); 4].map(|()| body.block());
        let (left, right) = (body.left, body.right);
        let one = body.constant(1);
        body.builder.ins().jump(inner, &[one.into()]);

        // After the first round this block only branches on its parameter.
        body.switch_to(inner);
        let passed = body.builder.append_block_param(inner, I32);
        body.builder.ins().jump(outer, &[passed.into()]);

        body.switch_to(outer);
        let truth = body.builder.append_block_param(outer, I32);
        body.builder.ins().brif(truth, then, &[], otherwise, &[]);

        body.returns(then, left);
        body.returns(otherwise, right);
        body.finish();
        assert_eq!(harness.thread(), 2);
        assert_eq!(harness.terminator(entry), (None, vec![(then, vec![])]));
    }

    #[test]
    fn a_parameter_with_another_use_keeps_its_block() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let entry = body.builder.current_block().unwrap();
        let [join, then, otherwise] = [(); 3].map(|()| body.block());
        let right = body.right;
        let one = body.constant(1);
        body.builder.ins().jump(join, &[one.into()]);

        body.switch_to(join);
        let truth = body.builder.append_block_param(join, I32);
        body.builder.ins().brif(truth, then, &[], otherwise, &[]);

        // A block the join dominates reads the parameter.
        body.returns(then, truth);
        body.returns(otherwise, right);
        body.finish();
        assert_eq!(harness.thread(), 0);
        assert_eq!(harness.terminator(entry), (None, vec![(join, vec![one])]));
    }

    #[test]
    fn a_block_that_does_more_than_branch_is_kept() {
        let mut harness = Harness::new();
        let mut body = harness.body();
        let entry = body.builder.current_block().unwrap();
        let [join, then, otherwise] = [(); 3].map(|()| body.block());
        let (left, right) = (body.left, body.right);
        let one = body.constant(1);
        body.builder.ins().jump(join, &[one.into()]);

        body.switch_to(join);
        let truth = body.builder.append_block_param(join, I32);
        body.builder.ins().trapz(left, ir::TrapCode::unwrap_user(1));
        body.builder.ins().brif(truth, then, &[], otherwise, &[]);

        body.returns(then, left);
        body.returns(otherwise, right);
        body.finish();
        assert_eq!(harness.thread(), 0);
        assert_eq!(harness.terminator(entry), (None, vec![(join, vec![one])]));
    }
}
