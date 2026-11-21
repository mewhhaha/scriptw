//! `Function::store_constant` elides exactly the stores whose value every
//! path already leaves in the local, `Function::refine_non_null` drops
//! exactly the refinements its next instruction makes redundant, and a
//! `local.set` read back at once is encoded as one `local.tee`.

use super::*;
use wasm_encoder::{BlockType, HeapType, RefType};

const STRUCT: HeapType = HeapType::Concrete(0);
const NULLABLE: ValType = ValType::Ref(RefType {
    nullable: true,
    heap_type: STRUCT,
});
const NON_NULL: ValType = ValType::Ref(RefType {
    nullable: false,
    heap_type: STRUCT,
});

fn body() -> Function {
    Function::with_parameters(LocalDeclarations::from_types([]), vec![ValType::I32])
}

/// The body's operators after its final `end`, without that `end`.
fn operators(mut function: Function) -> Vec<String> {
    function.instruction(&Instruction::End);
    let bytes = function.into_body().into_raw_body();
    let body = wasmparser::FunctionBody::new(wasmparser::BinaryReader::new(&bytes, 0));
    let mut operators = body
        .get_operators_reader()
        .unwrap()
        .into_iter()
        .map(|operator| format!("{:?}", operator.unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(operators.pop().as_deref(), Some("End"));
    operators
}

fn stores(operators: &[String]) -> usize {
    operators
        .iter()
        .filter(|operator| operator.starts_with("LocalSet"))
        .count()
}

#[test]
fn a_default_into_a_never_written_local_outside_loops_is_elided() {
    let mut function = body();
    let i32_local = function.reserve_typed_local(ValType::I32);
    let i64_local = function.reserve_typed_local(ValType::I64);
    let reference = function.reserve_typed_local(NULLABLE);
    function.store_constant(i32_local, LocalConstant::I32(0));
    function.store_constant(i64_local, LocalConstant::I64(0));
    function.store_constant(reference, LocalConstant::Null(STRUCT));
    assert!(operators(function).is_empty());

    // Not a default, or a parameter: each is stored.
    let mut function = body();
    let i32_local = function.reserve_typed_local(ValType::I32);
    let i64_local = function.reserve_typed_local(ValType::I64);
    function.store_constant(i32_local, LocalConstant::I32(1));
    function.store_constant(0, LocalConstant::I32(0));
    function.store_constant(i64_local, LocalConstant::I64(-1));
    assert_eq!(stores(&operators(function)), 3);
    // A float is known by its bits: a raw `-0.0` store is no `+0.0` fact.
    assert_ne!(
        KnownValue::pushed_by(&Instruction::F64Const((-0.0f64).into())),
        KnownValue::pushed_by(&Instruction::F64Const(0.0f64.into()))
    );
}

#[test]
fn an_open_loop_or_an_earlier_write_keeps_a_default_store() {
    // A back edge may bring a later write to the loop's body.
    let mut function = body();
    let local = function.reserve_typed_local(ValType::I32);
    function.instruction(&Instruction::Loop(BlockType::Empty));
    function.store_constant(local, LocalConstant::I32(0));
    function.instruction(&Instruction::I32Const(5));
    function.instruction(&Instruction::LocalSet(local));
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::BrIf(0));
    function.instruction(&Instruction::End);
    assert_eq!(stores(&operators(function)), 2);

    // A previous holder of a pooled local wrote it.
    let mut function = body();
    let local = function.reserve_typed_local(ValType::I32);
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::LocalSet(local));
    function.release_typed_local(local);
    let reused = function.reserve_typed_local(ValType::I32);
    assert_eq!(reused, local);
    function.store_constant(reused, LocalConstant::I32(0));
    assert_eq!(stores(&operators(function)), 2);
}

#[test]
fn a_constant_the_local_holds_on_every_path_is_not_stored_again() {
    let empty = BlockType::Empty;
    let mut function = body();
    let local = function.reserve_typed_local(ValType::I64);
    function.store_constant(local, LocalConstant::I64(7));
    function.instruction(&Instruction::Call(0));
    function.store_constant(local, LocalConstant::I64(7));
    // Both arms store 3: the join keeps it.
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::If(empty));
    function.store_constant(local, LocalConstant::I64(3));
    function.instruction(&Instruction::Else);
    function.store_constant(local, LocalConstant::I64(3));
    function.instruction(&Instruction::End);
    function.store_constant(local, LocalConstant::I64(3));
    // One arm stores 4: the join does not.
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::If(empty));
    function.store_constant(local, LocalConstant::I64(4));
    function.instruction(&Instruction::End);
    function.store_constant(local, LocalConstant::I64(4));
    assert_eq!(stores(&operators(function)), 5);
}

#[test]
fn a_raw_null_store_makes_a_fact_a_later_clear_uses() {
    let mut function = body();
    let local = function.reserve_typed_local(ValType::Ref(RefType::EQREF));
    function.instruction(&Instruction::RefNull(HeapType::Abstract {
        shared: false,
        ty: wasm_encoder::AbstractHeapType::Eq,
    }));
    function.instruction(&Instruction::LocalTee(local));
    function.instruction(&Instruction::Drop);
    function.store_constant(
        local,
        LocalConstant::Null(HeapType::Abstract {
            shared: false,
            ty: wasm_encoder::AbstractHeapType::Eq,
        }),
    );
    assert_eq!(
        operators(function),
        [
            "RefNull { hty: Abstract { shared: false, ty: Eq } }",
            "LocalTee { local_index: 1 }",
            "Drop"
        ]
    );
}

/// The GC local write check takes an elided store as the holder's write:
/// a read after it passes.
#[test]
fn an_elided_store_into_a_typed_reference_local_is_its_holders_write() {
    let mut function = body();
    let local = function.reserve_typed_local(NULLABLE);
    function.store_constant(local, LocalConstant::Null(STRUCT));
    function.instruction(&Instruction::LocalGet(local));
    function.instruction(&Instruction::Drop);
    assert_eq!(operators(function), ["LocalGet { local_index: 1 }", "Drop"]);
}

#[test]
#[should_panic(expected = "does not fit")]
fn a_constant_of_another_type_is_refused() {
    let mut function = body();
    let local = function.reserve_typed_local(NON_NULL);
    function.store_constant(local, LocalConstant::Null(STRUCT));
}

fn refined_then(next: impl FnOnce(&mut Function, u32, u32)) -> Vec<String> {
    let mut function = body();
    let nullable = function.reserve_typed_local(NULLABLE);
    let non_null = function.reserve_typed_local(NON_NULL);
    function.instruction(&Instruction::RefNull(STRUCT));
    function.instruction(&Instruction::LocalSet(nullable));
    function.instruction(&Instruction::LocalGet(nullable));
    function.refine_non_null();
    next(&mut function, nullable, non_null);
    operators(function)
        .into_iter()
        .filter(|operator| operator == "RefAsNonNull")
        .collect()
}

#[test]
fn a_refinement_is_dropped_before_a_consumer_that_accepts_null() {
    for consumer in [
        Instruction::StructGet {
            struct_type_index: 0,
            field_index: 0,
        },
        Instruction::ArrayLen,
        Instruction::RefCastNonNull(STRUCT),
    ] {
        assert!(refined_then(|function, _, _| {
            function.instruction(&consumer);
            function.instruction(&Instruction::Drop);
        })
        .is_empty());
    }
    assert!(refined_then(|function, nullable, _| {
        function.instruction(&Instruction::LocalSet(nullable));
    })
    .is_empty());
}

#[test]
fn a_refinement_is_kept_before_anything_else() {
    assert_eq!(
        refined_then(|function, _, non_null| {
            function.instruction(&Instruction::LocalSet(non_null));
        })
        .len(),
        1
    );
    assert_eq!(
        refined_then(|function, _, _| {
            function.instruction(&Instruction::Call(0));
        })
        .len(),
        1
    );
    // Two refinements in a row: the first is encoded.
    assert_eq!(
        refined_then(|function, nullable, _| {
            function.refine_non_null();
            function.instruction(&Instruction::LocalSet(nullable));
        })
        .len(),
        1
    );
    // A record of the operator ordinal comes after the refinement.
    let mut function = body();
    let nullable = function.reserve_typed_local(NULLABLE);
    function.instruction(&Instruction::RefNull(STRUCT));
    function.instruction(&Instruction::LocalSet(nullable));
    function.instruction(&Instruction::LocalGet(nullable));
    function.refine_non_null();
    function.program_literal_count(crate::gc_types::ProgramLiteralCount::for_test(1));
    // `ref.null`, `local.tee`, `ref.as_non_null`, then the literal.
    assert_eq!(
        function.literal_operands()[0],
        crate::program_literal_operands::FunctionSite::count(
            3,
            crate::gc_types::ProgramLiteralCount::for_test(1)
        )
    );
}

#[test]
fn a_store_read_back_at_once_is_one_tee() {
    let mut function = body();
    let local = function.reserve_typed_local(NULLABLE);
    let other = function.reserve_typed_local(ValType::I32);
    function.instruction(&Instruction::RefNull(STRUCT));
    function.instruction(&Instruction::LocalSet(local));
    function.instruction(&Instruction::LocalGet(local));
    function.instruction(&Instruction::Drop);
    // Another local's read, or a record of the next ordinal, keeps both.
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::LocalSet(other));
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::Drop);
    function.instruction(&Instruction::LocalGet(0));
    function.instruction(&Instruction::LocalSet(other));
    function.program_literal_count(crate::gc_types::ProgramLiteralCount::for_test(1));
    function.instruction(&Instruction::Drop);
    function.instruction(&Instruction::LocalGet(other));
    function.instruction(&Instruction::Drop);
    let ordinal = function.literal_operands()[0];
    assert_eq!(
        ordinal,
        crate::program_literal_operands::FunctionSite::count(
            9,
            crate::gc_types::ProgramLiteralCount::for_test(1)
        )
    );
    assert_eq!(
        operators(function),
        [
            "RefNull { hty: Concrete(Module(0)) }",
            "LocalTee { local_index: 1 }",
            "Drop",
            "LocalGet { local_index: 0 }",
            "LocalSet { local_index: 2 }",
            "LocalGet { local_index: 0 }",
            "Drop",
            "LocalGet { local_index: 0 }",
            "LocalSet { local_index: 2 }",
            "I32Const { value: 1 }",
            "Drop",
            "LocalGet { local_index: 2 }",
            "Drop",
        ]
    );
}
