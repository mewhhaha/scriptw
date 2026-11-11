//! Shared authority for one actual completed exact nested repeat pair.
use super::*;
use crate::gc_types::I32Local;

pub(super) struct CompletedExactChild<'pair> {
    pair: &'pair CountedLoopLocals,
}

fn intersect(eligible: I32Local, f: &mut Function) {
    eligible.load(f);
    f.instruction(&Instruction::I32And);
    eligible.store(f);
}

pub(super) fn emit_if_completed_exact_child(
    builder: &FunctionBuilder<'_>,
    workspace: &MatcherWorkspace,
    outer: &CountedLoopLocals,
    current: &CountedMatcherLocals,
    begin: I64Local,
    eligible: I32Local,
    f: &mut Function,
    finish: impl for<'pair> FnOnce(CompletedExactChild<'pair>, &FunctionBuilder<'_>, &mut Function),
) {
    let child = CountedLoopLocals::admit_at_begin(builder, workspace, current, begin, f);
    child.begin.load(f);
    outer.guard.load(f);
    f.instruction(&Instruction::I64GtU);
    child.end.load(f);
    outer.end.load(f);
    f.instruction(&Instruction::I64LtU);
    f.instruction(&Instruction::I32And);
    child.maximum_kind.load(f);
    f.instruction(&Instruction::I64Const(
        RegExpRepeatMaximumKind::Finite.word() as i64,
    ));
    f.instruction(&Instruction::I64Eq);
    f.instruction(&Instruction::I32And);
    child.minimum_length.load(f);
    child.maximum_length.load(f);
    f.instruction(&Instruction::I64Eq);
    f.instruction(&Instruction::I32And);
    intersect(eligible, f);
    eligible.load(f);
    f.instruction(&Instruction::If(BlockType::Empty));
    child.minimum_digits.load(f);
    f.instruction(&Instruction::I32WrapI64);
    f.instruction(&Instruction::I32Load8U(FunctionBuilder::memarg8(0)));
    f.instruction(&Instruction::I32Const(i32::from(b'0')));
    f.instruction(&Instruction::I32Ne);
    intersect(eligible, f);
    let schema = builder.runtime_schema();
    let index = schema.reserve_i64_local(f);
    let digit = schema.reserve_i32_local(f);
    f.instruction(&Instruction::I64Const(0));
    index.store(f);
    f.instruction(&Instruction::Block(BlockType::Empty));
    f.instruction(&Instruction::Loop(BlockType::Empty));
    index.load(f);
    child.minimum_length.load(f);
    f.instruction(&Instruction::I64GeU);
    eligible.load(f);
    f.instruction(&Instruction::I32Eqz);
    f.instruction(&Instruction::I32Or);
    f.instruction(&Instruction::BrIf(1));
    child.minimum_digits.load(f);
    index.load(f);
    f.instruction(&Instruction::I64Add);
    f.instruction(&Instruction::I32WrapI64);
    f.instruction(&Instruction::I32Load8U(FunctionBuilder::memarg8(0)));
    digit.store(f);
    child.maximum_digits.load(f);
    index.load(f);
    f.instruction(&Instruction::I64Add);
    f.instruction(&Instruction::I32WrapI64);
    f.instruction(&Instruction::I32Load8U(FunctionBuilder::memarg8(0)));
    digit.load(f);
    f.instruction(&Instruction::I32Eq);
    intersect(eligible, f);
    index.load(f);
    f.instruction(&Instruction::I64Const(1));
    f.instruction(&Instruction::I64Add);
    index.store(f);
    f.instruction(&Instruction::Br(0));
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);
    schema.release_i32_local(digit, f);
    schema.release_i64_local(index, f);
    eligible.load(f);
    f.instruction(&Instruction::If(BlockType::Empty));
    child
        .minimum
        .validate_canonical(builder, workspace, child.state, f);
    child
        .maximum
        .validate_canonical(builder, workspace, child.state, f);
    for word in [
        RegExpRepeatStateWord::Active,
        RegExpRepeatStateWord::MinimumUsed,
        RegExpRepeatStateWord::MaximumUsed,
    ] {
        builder.emit_regexp_scratch_load_word(child.state, word.offset(), child.temporary, f);
        child.temporary.load(f);
        f.instruction(&Instruction::I64Eqz);
        intersect(eligible, f);
    }
    builder.emit_regexp_scratch_load_word(
        child.state,
        RegExpRepeatStateWord::PreUtf16.offset(),
        child.temporary,
        f,
    );
    child.temporary.load(f);
    current.cursor.utf16.load(f);
    f.instruction(&Instruction::I64Eq);
    intersect(eligible, f);
    builder.emit_regexp_scratch_load_word(
        child.state,
        RegExpRepeatStateWord::Stage.offset(),
        child.temporary,
        f,
    );
    child.temporary.load(f);
    f.instruction(&Instruction::I64Const(
        RepeatIterationStage::Required.word() as i64,
    ));
    f.instruction(&Instruction::I64Eq);
    intersect(eligible, f);
    eligible.load(f);
    f.instruction(&Instruction::If(BlockType::Empty));
    finish(CompletedExactChild { pair: &child }, builder, f);
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);
    f.instruction(&Instruction::End);
    child.release(builder, f);
}
