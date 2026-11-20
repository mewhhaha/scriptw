use super::*;
use wasm_encoder::{BlockType, CodeSection, FunctionSection, Module, RefType, TypeSection};
use wasmparser::{Operator, Parser, Payload, Validator};

fn body() -> Function {
    Function::with_parameters(LocalDeclarations::from_types([]), vec![ValType::I32])
}

fn guard(function: &mut Function, local: u32, value: i32) {
    let target = function.label_depth();
    function.if_i32_equal_escape(local, value, |function| function.branch_to_label(target));
}

fn encoded_guard_count(mut function: Function) -> usize {
    function.instruction(&Instruction::End);
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32], []);
    types.ty().function([], []);
    let mut declarations = FunctionSection::new();
    declarations.function(0).function(1);
    let mut code = CodeSection::new();
    code.function(&function.into_body());
    let mut callee = wasm_encoder::Function::new([]);
    callee.instruction(&Instruction::End);
    code.function(&callee);
    let mut module = Module::new();
    module.section(&types).section(&declarations).section(&code);
    let bytes = module.finish();
    Validator::new().validate_all(&bytes).unwrap();
    for payload in Parser::new(0).parse_all(&bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            return body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .filter(|op| matches!(op.as_ref().unwrap(), Operator::If { .. }))
                .count();
        }
    }
    panic!("the first function has an encoded body");
}

#[test]
fn checked_escape_elides_only_the_same_local_and_excluded_value() {
    let mut function = body();
    let other = function.reserve_typed_local(ValType::I32);
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::I32Const(7));
    function.instruction(&Instruction::LocalSet(other));
    function.instruction(&Instruction::Call(1));
    guard(&mut function, 0, 1);
    guard(&mut function, 0, 2);
    // The stored constant 7 is not 2; 7 itself stays checked.
    guard(&mut function, other, 2);
    guard(&mut function, other, 7);
    assert_eq!(encoded_guard_count(function), 3);
}

#[test]
fn checked_escape_is_invalidated_by_stores_tees_and_slot_reuse() {
    let mut function = body();
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::I32Const(1));
    function.instruction(&Instruction::LocalSet(0));
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::I32Const(1));
    function.instruction(&Instruction::LocalTee(0));
    function.instruction(&Instruction::Drop);
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::LocalSet(0));
    guard(&mut function, 0, 1);
    let local = function.reserve_typed_local(ValType::I32);
    guard(&mut function, local, 1);
    function.release_typed_local(local);
    let reused = function.reserve_typed_local(ValType::I32);
    assert_eq!(reused, local);
    guard(&mut function, reused, 1);
    assert_eq!(encoded_guard_count(function), 6);
}

#[test]
fn a_fact_flows_into_frames_and_out_of_joins_that_every_edge_carries() {
    let empty = BlockType::Empty;
    let mut function = body();
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::Block(empty));
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::If(empty));
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::BrIf(1));
    function.instruction(&Instruction::Else);
    function.instruction(&Instruction::Call(1));
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::End);
    guard(&mut function, 0, 1);
    assert!(function.excludes_i32_value(0, 1));
    // The first guard and the explicit `if`.
    assert_eq!(encoded_guard_count(function), 2);
}

#[test]
fn a_join_forgets_a_fact_that_one_edge_lacks() {
    let empty = BlockType::Empty;
    // An earlier branch out of the block.
    let mut function = body();
    function.instruction(&Instruction::Block(empty));
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::BrIf(0));
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::End);
    assert!(!function.excludes_i32_value(0, 1));
    // An `if` arm that stores, and an `if` without `else` whose only arm
    // establishes the fact.
    let mut function = body();
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::If(empty));
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::LocalSet(0));
    function.instruction(&Instruction::Else);
    function.instruction(&Instruction::End);
    assert!(!function.excludes_i32_value(0, 1));
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::If(empty));
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::End);
    assert!(!function.excludes_i32_value(0, 1));
    // A branch table edge.
    let mut function = body();
    function.instruction(&Instruction::Block(empty));
    function.instruction(&Instruction::Block(empty));
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::BrTable(vec![1].into(), 0));
    function.instruction(&Instruction::End);
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::End);
    assert!(!function.excludes_i32_value(0, 1));
}

#[test]
fn loop_heads_and_handler_entries_start_without_facts() {
    let empty = BlockType::Empty;
    let mut function = body();
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::Loop(empty));
    assert!(!function.excludes_i32_value(0, 1));
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::End);
    // The loop's body falls out with its own facts.
    assert!(function.excludes_i32_value(0, 1));

    let mut function = body();
    function.instruction(&Instruction::Block(empty));
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::TryTable(
        empty,
        vec![Catch::All { label: 0 }].into(),
    ));
    assert!(function.excludes_i32_value(0, 1));
    function.instruction(&Instruction::End);
    function.instruction(&Instruction::End);
    assert!(!function.excludes_i32_value(0, 1));

    let mut function = body();
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::Try(empty));
    function.instruction(&Instruction::CatchAll);
    assert!(!function.excludes_i32_value(0, 1));
    function.instruction(&Instruction::End);
    assert!(!function.excludes_i32_value(0, 1));
}

#[test]
fn unreachable_code_adds_no_edge() {
    let empty = BlockType::Empty;
    let mut function = body();
    guard(&mut function, 0, 1);
    function.instruction(&Instruction::Block(empty));
    function.instruction(&Instruction::Br(0));
    function.instruction(&Instruction::I32Const(1));
    function.instruction(&Instruction::LocalSet(0));
    function.instruction(&Instruction::End);
    assert!(function.excludes_i32_value(0, 1));
}

/// A resume table's handlers would need the facts at every suspension; no
/// emitter produces stack switching, so the sink refuses it.
#[test]
#[should_panic(expected = "does not model stack switching")]
fn stack_switching_is_refused() {
    let mut function = body();
    function.instruction(&Instruction::I32Const(0));
    function.instruction(&Instruction::LocalSet(0));
    function.instruction(&Instruction::Suspend(0));
}

/// A program literal's immediate is relocated at link time: a store of it
/// makes no fact.
#[test]
fn a_relocated_literal_immediate_makes_no_fact() {
    let mut function = body();
    function.program_literal_count(crate::gc_types::ProgramLiteralCount::for_test(1));
    function.instruction(&Instruction::LocalSet(0));
    assert!(!function.excludes_i32_value(0, 0));
    assert!(!function.excludes_i32_value(0, 1));
}

#[test]
#[should_panic(expected = "checked arm must unconditionally escape")]
fn checked_escape_rejects_a_fallthrough_true_arm() {
    body().if_i32_equal_escape(0, 1, |function| {
        function.instruction(&Instruction::Nop);
    });
}

#[test]
#[should_panic(expected = "checked escape cannot branch to its own continuation")]
fn checked_escape_rejects_br_zero() {
    body().if_i32_equal_escape(0, 1, |function| {
        function.instruction(&Instruction::Br(0));
    });
}

#[test]
#[should_panic(expected = "checked escape cannot branch to its own continuation")]
fn checked_escape_rejects_an_earlier_conditional_path_to_its_end() {
    body().if_i32_equal_escape(0, 1, |function| {
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::BrIf(0));
        function.instruction(&Instruction::Return);
    });
}

#[test]
#[should_panic(expected = "checked escape cannot branch to its own continuation")]
fn checked_escape_rejects_exception_handlers_targeting_its_end() {
    body().if_i32_equal_escape(0, 1, |function| {
        function.instruction(&Instruction::TryTable(
            BlockType::Empty,
            vec![Catch::All { label: 0 }].into(),
        ));
    });
}
