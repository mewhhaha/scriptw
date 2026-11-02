//! The generic packed publisher skips index probes only for sealed layouts.
use super::*;
use crate::gc_types::{
    ArrayObject, GcNullability, GcSnapshotField, GlobalLedger, IntrinsicPropertyStorage,
    RuntimeModuleTypes,
};

#[test]
fn packed_push_skips_key_probes_only_for_index_free_intrinsic_storage() {
    let artifact = emit_script("var a=[]; a.push(1); a.push(2,3); a.push(4,5,6);").unwrap();
    expect_valid_module(&artifact, 1);
    let bytes = artifact.runtime().unwrap().bytes();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let type_index = |heap_type| {
        let wasm_encoder::HeapType::Concrete(index) = heap_type else {
            panic!("registered GC type has a concrete index");
        };
        index
    };
    let intrinsic = wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(type_index(
        schema
            .reference_type::<IntrinsicPropertyStorage>(GcNullability::NonNullable)
            .heap_type,
    )));
    let array = type_index(
        schema
            .reference_type::<ArrayObject>(GcNullability::NonNullable)
            .heap_type,
    );
    let names = function_names_in_bytes(bytes);
    let named = |requested: &str| {
        let matches: Vec<_> = names
            .iter()
            .filter_map(|(&index, name)| (name == requested).then_some(index))
            .collect();
        assert_eq!(matches.len(), 1, "one physical R body for {requested}");
        matches[0]
    };
    let decimal = named("helper::integer_decimal_string");
    let descriptor = named("helper::ordinary_own_descriptor");
    let mut bodies = std::collections::BTreeMap::new();
    let mut index = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            bodies.insert(
                index,
                body.get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect::<Vec<_>>(),
            );
            index += 1;
        }
    }
    for helper in [
        "helper::array_packed_push1",
        "helper::array_packed_push2",
        "helper::array_packed_push3",
    ] {
        let operators = &bodies[&named(helper)];
        let guards: Vec<_> = operators
            .iter()
            .enumerate()
            .filter_map(|(index, op)| {
                matches!(op, Operator::RefTestNonNull { hty } if *hty == intrinsic).then_some(index)
            })
            .collect();
        assert_eq!(guards.len(), 1, "one actual-storage guard in {helper}");
        let guard = guards[0];
        assert!(operators[..guard].windows(4).any(|ops| matches!(ops,
            [Operator::StructGet { struct_type_index, field_index }, Operator::LocalSet { local_index: a }, Operator::LocalGet { local_index: b }, Operator::I64Eqz]
            if *struct_type_index == array && *field_index == GcSnapshotField::ArrayCount.index() && a == b)),
            "Array prototypes still require actual indexed COUNT zero");
        let generic = guard
            + 1
            + operators[guard + 1..]
                .iter()
                .position(|op| matches!(op, Operator::If { .. }))
                .unwrap();
        assert!(
            matches!(operators[generic - 1], Operator::I32Eqz),
            "only non-intrinsic storage enters the generic probes"
        );
        assert!(
            operators[guard + 1..generic - 1]
                .iter()
                .all(|op| matches!(op, Operator::RefNull { .. } | Operator::LocalSet { .. })),
            "the temporary storage root is cleared before either branch"
        );
        let mut depth = 1;
        let end = (generic + 1..operators.len())
            .find(|&index| {
                match operators[index] {
                    Operator::Block { .. }
                    | Operator::Loop { .. }
                    | Operator::If { .. }
                    | Operator::TryTable { .. } => depth += 1,
                    Operator::End => depth -= 1,
                    Operator::Else if depth == 1 => {
                        panic!("the intrinsic path must bypass all key probes")
                    }
                    _ => {}
                }
                depth == 0
            })
            .unwrap();
        for callee in [decimal, descriptor] {
            let calls: Vec<_> = operators
                .iter()
                .enumerate()
                .filter_map(|(index, op)| {
                    matches!(op, Operator::Call { function_index } if *function_index == callee)
                        .then_some(index)
                })
                .collect();
            assert_eq!(calls.len(), 1, "one generic fallback probe in {helper}");
            assert!(
                (generic + 1..end).contains(&calls[0]),
                "sealed storage bypasses the probe"
            );
        }
    }
}
