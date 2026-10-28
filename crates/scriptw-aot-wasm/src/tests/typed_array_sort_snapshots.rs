//! Comparator Lists reuse the actual immutable sort snapshots in emitted Wasm.
use super::*;
use crate::gc_types::{GcNullability, GlobalLedger, RuntimeModuleTypes, ValueArray};

#[test]
fn typed_array_sort_callbacks_use_snapshot_roots_without_reboxing() {
    let artifact = emit_script("new Float64Array([2, 1]).sort();").unwrap();
    expect_valid_module(&artifact, 0);
    let runtime = artifact.runtime().unwrap();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(values) = registered
        .globals()
        .schema()
        .reference_type::<ValueArray>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("argument List must have a concrete GC type");
    };
    let names = function_names_in_bytes(runtime.bytes());
    for name in [
        "builtin::TypedArray.prototype.sort",
        "builtin::TypedArray.prototype.toSorted",
    ] {
        let target = names
            .iter()
            .find_map(|(index, actual)| (actual == name).then_some(*index))
            .expect("the real TypedArray builtin must be registered");
        let mut index = imported_function_count_in_bytes(runtime.bytes());
        let mut inspected = false;
        for payload in Parser::new(0).parse_all(runtime.bytes()) {
            let Payload::CodeSectionEntry(body) = payload.unwrap() else {
                continue;
            };
            let current = index;
            index += 1;
            if current != target {
                continue;
            }
            let operators: Vec<_> = body
                .get_operators_reader()
                .unwrap()
                .into_iter()
                .map(Result::unwrap)
                .collect();
            let pairs: Vec<_> = operators
                .iter()
                .enumerate()
                .filter_map(|(position, operator)| {
                    matches!(operator, Operator::ArrayNewFixed { array_type_index, array_size: 2 } if *array_type_index == values)
                        .then_some(position)
                })
                .collect();
            assert_eq!(pairs.len(), 1, "{name} creates a fresh comparator pair");
            let position = pairs[0];
            let [Operator::LocalGet { local_index: left }, Operator::RefAsNonNull, Operator::LocalGet { local_index: right }, Operator::RefAsNonNull] =
                &operators[position - 4..position]
            else {
                panic!("{name} must construct the pair from rooted snapshots");
            };
            assert_ne!(left, right, "the two operands need independent roots");
            for local in [left, right] {
                let assigned = operators[..position - 4]
                    .iter()
                    .rposition(|operator| matches!(operator, Operator::LocalSet { local_index } if local_index == local))
                    .expect("each callback operand must be loaded from the sort List");
                assert!(
                    matches!(operators[assigned - 1], Operator::ArrayGet { array_type_index } if array_type_index == values),
                    "{name} must reuse the List entry, without another StoredValue allocation"
                );
            }
            inspected = true;
            break;
        }
        assert!(
            inspected,
            "the registered builtin must have an emitted body"
        );
    }
}
