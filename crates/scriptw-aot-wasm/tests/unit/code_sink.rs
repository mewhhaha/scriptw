use super::*;
use std::borrow::Cow;
use wasm_encoder::{BlockType, CodeSection, FunctionSection, Module, RefType, TypeSection};

fn empty_body() -> Function {
    Function::new(LocalDeclarations::from_types(std::iter::empty()))
}

#[test]
fn a_fresh_body_is_one_label_deep() {
    let mut function = empty_body();
    assert_eq!(function.depth(), 1);
    let root = function.label_depth();
    assert_eq!(function.branch_depth_to(root), BranchDepth(0));
    function.branch_to_label(root);
    function.instruction(&Instruction::End);
    let _ = function.into_body();
}

#[test]
fn raw_frames_are_counted_without_being_declared() {
    let mut function = empty_body();
    function.instruction(&Instruction::Block(BlockType::Empty));
    let outer = function.label_depth();
    function.instruction(&Instruction::Block(BlockType::Empty));
    function.instruction(&Instruction::If(BlockType::Empty));
    assert_eq!(function.depth(), 4);
    assert_eq!(function.branch_depth_to(outer), BranchDepth(2));
    assert_eq!(
        function.branch_depth_to(function.label_depth()),
        BranchDepth(0)
    );
}

#[test]
fn else_keeps_the_same_live_label() {
    let mut function = empty_body();
    function.instruction(&Instruction::If(BlockType::Empty));
    let then_arm = function.label_depth();
    function.instruction(&Instruction::Else);
    assert_eq!(function.label_depth(), then_arm);
    assert_eq!(function.branch_depth_to(then_arm), BranchDepth(0));
    function.instruction(&Instruction::End);
    assert_eq!(function.depth(), 1);
}

#[test]
fn a_loop_label_is_the_back_edge() {
    let mut function = empty_body();
    function.instruction(&Instruction::Block(BlockType::Empty));
    let exit = function.label_depth();
    function.instruction(&Instruction::Loop(BlockType::Empty));
    let back_edge = function.label_depth();
    function.instruction(&Instruction::If(BlockType::Empty));
    assert_eq!(function.branch_depth_to(back_edge), BranchDepth(1));
    assert_eq!(function.branch_depth_to(exit), BranchDepth(2));
}

#[test]
#[should_panic(expected = "unclosed control frame")]
fn into_body_rejects_an_unclosed_frame() {
    let mut function = empty_body();
    function.instruction(&Instruction::Block(BlockType::Empty));
    function.instruction(&Instruction::End);
    let _ = function.into_body();
}

#[test]
fn into_body_accepts_a_balanced_body() {
    let mut function = empty_body();
    function.instruction(&Instruction::Block(BlockType::Empty));
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::End);
    assert_eq!(function.depth(), 0);
    let _ = function.into_body();
}

#[test]
#[should_panic(expected = "closed more frames than it opened")]
fn an_extra_end_is_not_silently_absorbed() {
    let mut function = empty_body();
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::End);
}

#[test]
#[should_panic(expected = "after the function body's final end")]
fn instructions_after_the_final_end_are_rejected() {
    let mut function = empty_body();
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::Nop);
}

#[test]
#[should_panic(expected = "after the function body's final end")]
fn a_finished_body_cannot_be_reopened() {
    let mut function = empty_body();
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::Block(BlockType::Empty));
}

#[test]
#[should_panic(expected = "finished body has no live label")]
fn a_finished_body_cannot_issue_a_label_handle() {
    let mut function = empty_body();
    function.instruction(&Instruction::End);
    let _ = function.label_depth();
}

#[test]
#[should_panic(expected = "not open at this point")]
fn branching_to_a_closed_frame_is_rejected() {
    let mut function = empty_body();
    function.instruction(&Instruction::Block(BlockType::Empty));
    let inner = function.label_depth();
    function.instruction(&Instruction::End);
    let _ = function.branch_depth_to(inner);
}

#[test]
#[should_panic(expected = "not open at this point")]
fn a_sibling_cannot_resurrect_a_closed_label() {
    let mut function = empty_body();
    function.instruction(&Instruction::Block(BlockType::Empty));
    let stale = function.label_depth();
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::Block(BlockType::Empty));
    assert_eq!(stale.depth, function.label_depth().depth);
    function.branch_to_label(stale);
}

#[test]
#[should_panic(expected = "not open at this point")]
fn deeper_nesting_cannot_resurrect_a_closed_label() {
    let mut function = empty_body();
    function.instruction(&Instruction::Block(BlockType::Empty));
    let stale = function.label_depth();
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::Block(BlockType::Empty));
    function.instruction(&Instruction::Loop(BlockType::Empty));
    function.branch_if_to_label(stale);
}

#[test]
#[should_panic(expected = "not open at this point")]
fn a_foreign_function_label_is_rejected() {
    let foreign = empty_body().label_depth();
    empty_body().branch_to_label(foreign);
}

#[test]
#[should_panic(expected = "not open at this point")]
fn a_closed_function_label_is_rejected() {
    let mut function = empty_body();
    let root = function.label_depth();
    function.instruction(&Instruction::End);
    let _ = function.branch_depth_to(root);
}

#[test]
#[should_panic(expected = "not open at this point")]
fn a_synthetic_test_position_is_not_an_emission_handle() {
    empty_body().branch_to_label(LabelDepth::for_test(1));
}

#[test]
fn a_clone_retains_its_live_prefix() {
    let mut function = empty_body();
    function.instruction(&Instruction::Block(BlockType::Empty));
    let live = function.label_depth();
    let mut cloned = function.clone();
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::End);
    assert_eq!(cloned.branch_depth_to(live), BranchDepth(0));
    cloned.branch_to_label(live);
    cloned.instruction(&Instruction::End);
    cloned.instruction(&Instruction::End);
    let _ = cloned.into_body();
}

#[test]
#[should_panic(expected = "not open at this point")]
fn independently_opened_clone_frames_have_distinct_identities() {
    let mut function = empty_body();
    let mut cloned = function.clone();
    function.instruction(&Instruction::Block(BlockType::Empty));
    let foreign = function.label_depth();
    cloned.instruction(&Instruction::Block(BlockType::Empty));
    cloned.branch_to_label(foreign);
}

#[test]
#[should_panic(expected = "unmatched `if`")]
fn else_outside_if_is_rejected() {
    empty_body().instruction(&Instruction::Else);
}

#[test]
#[should_panic(expected = "unmatched `if`")]
fn else_cannot_cross_an_unclosed_block() {
    let mut function = empty_body();
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Block(BlockType::Empty));
    function.instruction(&Instruction::Else);
}

#[test]
#[should_panic(expected = "unmatched `if`")]
fn duplicate_else_is_rejected() {
    let mut function = empty_body();
    function.instruction(&Instruction::If(BlockType::Empty));
    function.instruction(&Instruction::Else);
    function.instruction(&Instruction::Else);
}

#[test]
fn every_reference_branch_checks_its_immediate() {
    let branches = [
        Instruction::BrOnNull(1),
        Instruction::BrOnNonNull(1),
        Instruction::BrOnCast {
            relative_depth: 1,
            from_ref_type: RefType::ANYREF,
            to_ref_type: RefType::EQREF,
        },
        Instruction::BrOnCastFail {
            relative_depth: 1,
            from_ref_type: RefType::ANYREF,
            to_ref_type: RefType::EQREF,
        },
    ];
    for instruction in branches {
        let rejected = std::panic::catch_unwind(|| {
            empty_body().instruction(&instruction);
        });
        assert!(
            rejected.is_err(),
            "accepted invalid branch: {instruction:?}"
        );
        let mut nested = empty_body();
        nested.instruction(&Instruction::Block(BlockType::Empty));
        nested.instruction(&instruction);
    }
}

#[test]
fn raw_branches_and_every_table_entry_are_range_checked() {
    let invalid = [
        Instruction::Br(1),
        Instruction::BrIf(1),
        Instruction::BrTable(Cow::Borrowed(&[]), 1),
        Instruction::BrTable(Cow::Borrowed(&[0, 1]), 0),
    ];
    for instruction in invalid {
        assert!(std::panic::catch_unwind(|| {
            empty_body().instruction(&instruction);
        })
        .is_err());
    }
}

#[test]
fn typed_local_allocation_keeps_mixed_types_and_live_frame_identity() {
    let mut function = empty_body();
    let types = [
        ValType::I32,
        ValType::Ref(RefType::ANYREF),
        ValType::Ref(RefType::EQREF),
        ValType::I64,
    ];
    for (index, ty) in types.into_iter().enumerate() {
        assert_eq!(function.reserve_typed_local(ty), index as u32);
    }
    function.instruction(&Instruction::Block(BlockType::Empty));
    let inner = function.label_depth();
    function.release_typed_local(0);
    // A free I32 slot cannot be reused for a live I64 value.
    assert_eq!(function.reserve_typed_local(ValType::I64), 4);
    function.release_typed_local(4);
    assert_eq!(function.reserve_typed_local(ValType::I32), 0);
    assert_eq!(function.allocated_local_count(), 5);
    assert_eq!(function.label_depth(), inner);
    assert_eq!(function.branch_depth_to(inner), BranchDepth(0));
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::End);

    let mut expected = wasm_encoder::Function::new([
        (1, ValType::I32),
        (1, ValType::Ref(RefType::ANYREF)),
        (1, ValType::Ref(RefType::EQREF)),
        (2, ValType::I64),
    ]);
    expected.instruction(&Instruction::Block(BlockType::Empty));
    expected.instruction(&Instruction::End);
    expected.instruction(&Instruction::End);
    assert_eq!(function.into_body(), expected);
}

#[test]
fn valid_control_flow_keeps_exact_encoder_bytes_and_validates() {
    let instructions = [
        Instruction::Block(BlockType::Empty),
        Instruction::Loop(BlockType::Empty),
        Instruction::I32Const(1),
        Instruction::If(BlockType::Empty),
        Instruction::Br(2),
        Instruction::Else,
        Instruction::I32Const(0),
        Instruction::BrIf(1),
        Instruction::I32Const(0),
        Instruction::BrTable(Cow::Borrowed(&[2, 1]), 3),
        Instruction::End,
        Instruction::End,
        Instruction::End,
        Instruction::End,
    ];
    let mut function = empty_body();
    let mut encoder = wasm_encoder::Function::new([]);
    for instruction in &instructions {
        function.instruction(instruction);
        encoder.instruction(instruction);
    }
    let body = function.into_body();
    assert_eq!(body, encoder);

    let mut types = TypeSection::new();
    types.ty().function([], []);
    let mut functions = FunctionSection::new();
    functions.function(0);
    let mut code = CodeSection::new();
    code.function(&body);
    let mut module = Module::new();
    module.section(&types).section(&functions).section(&code);
    wasmparser::Validator::new()
        .validate_all(&module.finish())
        .expect("structured control-flow bytes must validate");
}

#[test]
fn a_counted_call_scope_brackets_each_call_and_nothing_else() {
    let mut function = empty_body();
    function.instruction(&Instruction::I32Const(1));
    function.instruction(&Instruction::Drop);
    let scope = function.open_counted_calls(7);
    function.instruction(&Instruction::Call(3));
    function.instruction(&Instruction::Drop);
    function.close_counted_calls(scope);
    function.instruction(&Instruction::Call(4));
    function.instruction(&Instruction::End);
    let mut bytes = Vec::new();
    wasm_encoder::Encode::encode(&function.into_body(), &mut bytes);
    let mut expected = Vec::new();
    let mut reference = wasm_encoder::Function::new([]);
    for instruction in [
        Instruction::I32Const(1),
        Instruction::Drop,
        Instruction::GlobalGet(7),
        Instruction::I32Const(1),
        Instruction::I32Add,
        Instruction::GlobalSet(7),
        Instruction::Call(3),
        Instruction::GlobalGet(7),
        Instruction::I32Const(-1),
        Instruction::I32Add,
        Instruction::GlobalSet(7),
        Instruction::Drop,
        Instruction::Call(4),
        Instruction::End,
    ] {
        reference.instruction(&instruction);
    }
    wasm_encoder::Encode::encode(&reference, &mut expected);
    assert_eq!(bytes, expected);
}

#[test]
#[should_panic(expected = "close innermost first")]
fn counted_call_scopes_close_innermost_first() {
    let mut function = empty_body();
    let outer = function.open_counted_calls(1);
    let _inner = function.open_counted_calls(2);
    function.close_counted_calls(outer);
}
