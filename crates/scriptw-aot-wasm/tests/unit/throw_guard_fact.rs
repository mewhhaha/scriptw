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
    guard(&mut function, other, 2);
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
    let local = function.reserve_typed_local(ValType::I32);
    guard(&mut function, local, 1);
    function.release_typed_local(local);
    let reused = function.reserve_typed_local(ValType::I32);
    assert_eq!(reused, local);
    guard(&mut function, reused, 1);
    assert_eq!(encoded_guard_count(function), 5);
}

#[test]
fn every_control_boundary_forgets_checked_fallthrough() {
    let empty = BlockType::Empty;
    let cases = vec![
        (vec![], Instruction::Block(empty)),
        (vec![], Instruction::Loop(empty)),
        (vec![], Instruction::If(empty)),
        (vec![Instruction::If(empty)], Instruction::Else),
        (vec![Instruction::Block(empty)], Instruction::End),
        (vec![], Instruction::TryTable(empty, vec![].into())),
        (vec![], Instruction::Try(empty)),
        (vec![Instruction::Try(empty)], Instruction::Catch(0)),
        (vec![Instruction::Try(empty)], Instruction::CatchAll),
        (vec![Instruction::Try(empty)], Instruction::Delegate(0)),
        (
            vec![Instruction::Try(empty), Instruction::CatchAll],
            Instruction::Rethrow(0),
        ),
        (vec![], Instruction::Br(0)),
        (vec![], Instruction::BrIf(0)),
        (vec![], Instruction::BrTable(vec![0].into(), 0)),
        (vec![], Instruction::BrOnNull(0)),
        (vec![], Instruction::BrOnNonNull(0)),
        (
            vec![],
            Instruction::BrOnCast {
                relative_depth: 0,
                from_ref_type: RefType::ANYREF,
                to_ref_type: RefType::EQREF,
            },
        ),
        (
            vec![],
            Instruction::BrOnCastFail {
                relative_depth: 0,
                from_ref_type: RefType::ANYREF,
                to_ref_type: RefType::EQREF,
            },
        ),
        (vec![], Instruction::Return),
        (vec![], Instruction::ReturnCall(1)),
        (vec![], Instruction::ReturnCallRef(0)),
        (
            vec![],
            Instruction::ReturnCallIndirect {
                type_index: 0,
                table_index: 0,
            },
        ),
        (vec![], Instruction::Throw(0)),
        (vec![], Instruction::ThrowRef),
        (vec![], Instruction::Unreachable),
        (vec![], Instruction::Suspend(0)),
        (
            vec![],
            Instruction::Resume {
                cont_type_index: 0,
                resume_table: vec![].into(),
            },
        ),
        (
            vec![],
            Instruction::ResumeThrow {
                cont_type_index: 0,
                tag_index: 0,
                resume_table: vec![].into(),
            },
        ),
        (
            vec![],
            Instruction::ResumeThrowRef {
                cont_type_index: 0,
                resume_table: vec![].into(),
            },
        ),
        (
            vec![],
            Instruction::Switch {
                cont_type_index: 0,
                tag_index: 0,
            },
        ),
    ];
    for (prefix, boundary) in cases {
        let mut function = body();
        for instruction in prefix {
            function.instruction(&instruction);
        }
        guard(&mut function, 0, 1);
        assert_eq!(function.excluded_i32_value, Some((0, 1)));
        function.instruction(&boundary);
        assert_eq!(function.excluded_i32_value, None, "{boundary:?}");
    }
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
