//! LengthOfArrayLike in Array builtins and %ArrayIteratorPrototype%.next
//! reads an actual Array's [[ArrayLength]] inline; every other receiver,
//! Proxy included, keeps the generic Get(`length`) and ToLength.
use super::*;
use crate::gc_types::{
    ArrayObject, GcNullability, GcSnapshotField, GlobalLedger, RuntimeModuleTypes,
};

fn named(names: &BTreeMap<u32, String>, requested: &str) -> u32 {
    let found: Vec<_> = names
        .iter()
        .filter_map(|(&index, name)| (name == requested).then_some(index))
        .collect();
    assert_eq!(found.len(), 1, "one actual R body for {requested}");
    found[0]
}

fn body<'a>(bytes: &'a [u8], target: u32) -> Vec<Operator<'a>> {
    let mut index = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if index == target {
                return crate::tests::emitted_operators(&body);
            }
            index += 1;
        }
    }
    panic!("named R body exists");
}

/// The index of the `end` closing the frame that is open at `start`.
fn frame_end(operators: &[Operator<'_>], start: usize) -> usize {
    let mut depth = 0usize;
    for (index, operator) in operators.iter().enumerate().skip(start) {
        match operator {
            Operator::Block { .. }
            | Operator::If { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. } => depth += 1,
            Operator::End if depth == 0 => return index,
            Operator::End => depth -= 1,
            Operator::Else if depth == 0 => panic!("the length frames are plain blocks"),
            _ => {}
        }
    }
    panic!("the frame at {start} closes");
}

fn calls(operators: &[Operator<'_>], target: u32) -> bool {
    operators
        .iter()
        .any(|op| matches!(op, Operator::Call { function_index } if *function_index == target))
}

#[test]
fn array_length_snapshots_read_actual_array_length_and_keep_the_generic_get() {
    let artifact =
        emit_script("var a=[3,2,1]; a.reverse(); a.join(); for (var x of a) {}").unwrap();
    expect_valid_module(&artifact, 0);
    let bytes = artifact.runtime().unwrap().bytes();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let wasm_encoder::HeapType::Concrete(array) = registered
        .globals()
        .schema()
        .reference_type::<ArrayObject>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("the actual Array guard uses its registered concrete type");
    };
    let heap = wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(array));
    let length = GcSnapshotField::ArrayLength.index();
    let names = function_names_in_bytes(bytes);
    let object_read = named(&names, "helper::object_read");
    let to_object = named(&names, "helper::value_to_object");
    for (builtin, converts) in [
        ("builtin::Array.prototype.reverse", true),
        ("builtin::Array.prototype.join", true),
        ("builtin::Array Iterator.prototype.next", false),
    ] {
        let operators = body(bytes, named(&names, builtin));
        let snapshots = operators
            .windows(4)
            .enumerate()
            .filter(|(_, window)| {
                matches!(window, [Operator::BrOnCast { to_ref_type, .. }, Operator::Drop, Operator::Br { .. }, Operator::End]
                    if to_ref_type.heap_type() == heap)
            })
            .filter_map(|(index, _)| {
                // The Array arm: from the cast block's end to its frame's end.
                let arm_start = index + 4;
                let arm_end = frame_end(&operators, arm_start);
                let arm = &operators[arm_start..arm_end];
                let reads_length = arm.windows(2).any(|ops| matches!(ops,
                    [Operator::StructGet { struct_type_index, field_index }, Operator::LocalSet { .. }]
                    if *struct_type_index == array && *field_index == length));
                if !reads_length {
                    return None;
                }
                // The generic fallback: the rest of the enclosing frame.
                let generic_end = frame_end(&operators, arm_end + 1);
                Some((arm, &operators[arm_end + 1..generic_end]))
            })
            .filter(|(_, generic)| calls(generic, object_read))
            .collect::<Vec<_>>();
        assert!(
            !snapshots.is_empty(),
            "{builtin} tests for an actual Array before the generic length Get"
        );
        for (arm, generic) in snapshots {
            assert!(
                !arm.iter().any(|op| matches!(
                    op,
                    Operator::Call { .. } | Operator::CallRef { .. } | Operator::ReturnCall { .. }
                )),
                "{builtin}: an actual Array's length needs no ToObject, Get or ToLength call"
            );
            assert!(
                matches!(arm.last(), Some(Operator::Br { .. })),
                "{builtin}: the Array arm skips the generic path"
            );
            if converts {
                assert!(
                    calls(generic, to_object),
                    "{builtin}: other receivers still take ToObject before Get"
                );
            }
        }
    }
}
