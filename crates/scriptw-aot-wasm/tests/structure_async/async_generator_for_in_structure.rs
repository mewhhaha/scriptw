use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use std::collections::BTreeMap;
use wasmparser::{
    ArrayType, CompositeInnerType, HeapType, Operator, Parser, Payload, StorageType, StructType,
    ValType, Validator, WasmFeatures,
};

fn concrete_reference(field: StorageType) -> Option<(bool, u32)> {
    let StorageType::Val(ValType::Ref(reference)) = field else {
        return None;
    };
    let HeapType::Concrete(index) = reference.heap_type() else {
        return None;
    };
    Some((reference.is_nullable(), index.as_module_index()?))
}

fn inspect_mixed_for_in_artifact(source: &str, requires_reference: bool) {
    let parsed = parse(source, ParseOptions::script()).expect("actual mixed ForIn fixture parses");
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let artifact = scriptw_aot_wasm::emit(&program)
        .expect("checked mixed ForIn reaches the original retained native enumeration");
    let mut features = WasmFeatures::default();
    for feature in [
        WasmFeatures::THREADS,
        WasmFeatures::MULTI_MEMORY,
        WasmFeatures::REFERENCE_TYPES,
        WasmFeatures::FUNCTION_REFERENCES,
        WasmFeatures::GC,
        WasmFeatures::EXCEPTIONS,
        WasmFeatures::TAIL_CALL,
    ] {
        features.set(feature, true);
    }
    Validator::new_with_features(features)
        .validate_all(&artifact.bytes)
        .expect("mixed head/body/finalizer cursor and lexical branches have valid Wasm types");
    let mut structures = BTreeMap::<u32, StructType>::new();
    let mut arrays = BTreeMap::<u32, ArrayType>::new();
    let mut next_type = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::TypeSection(reader) = payload.unwrap() {
            for group in reader {
                for ty in group.unwrap().types() {
                    match &ty.composite_type.inner {
                        CompositeInnerType::Struct(value) => {
                            structures.insert(next_type, value.clone());
                        }
                        CompositeInnerType::Array(value) => {
                            arrays.insert(next_type, value.clone());
                        }
                        CompositeInnerType::Func(_) | CompositeInnerType::Cont(_) => {}
                    }
                    next_type += 1;
                }
            }
        }
    }
    let cursors = structures
        .iter()
        .filter_map(|(&cell, structure)| {
            let fields = &structure.fields;
            if fields.len() != 13
                || concrete_reference(fields[4].element_type) != Some((true, cell))
                || !fields[12].mutable
            {
                return None;
            }
            let (nullable, cursor) = concrete_reference(fields[12].element_type)?;
            let record = &structures.get(&cursor)?.fields;
            if !nullable
                || record.len() != 4
                || record.iter().any(|field| !field.mutable)
                || record[2].element_type.unpack() != ValType::I32
            {
                return None;
            }
            let (nullable, value) = concrete_reference(record[0].element_type)?;
            if nullable || concrete_reference(fields[0].element_type) != Some((false, value)) {
                return None;
            }
            let (nullable, keys) = concrete_reference(record[1].element_type)?;
            if !nullable || concrete_reference(record[3].element_type) != Some((false, keys)) {
                return None;
            }
            let element = arrays.get(&keys)?.0;
            if !element.mutable || concrete_reference(element.element_type) != Some((false, value))
            {
                return None;
            }
            Some((cell, cursor))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cursors.len(),
        1,
        "the original cursor retains its physical snapshot/index/visited owners"
    );
    let (cell, cursor) = cursors[0];
    let environments = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let fields = &structure.fields;
            (fields.len() == 9
                && !fields[2].mutable
                && fields[2].element_type.unpack() == ValType::I32
                && concrete_reference(fields[3].element_type) == Some((true, index)))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        environments.len(),
        1,
        "TDZ and per-key cells use the original lexical Environment"
    );
    let frames = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let fields = &structure.fields;
            (fields.len() == 15
                && [1, 2, 3].iter().all(|&i| {
                    fields[i].mutable
                        && concrete_reference(fields[i].element_type)
                            == Some((true, environments[0]))
                }))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        frames.len(),
        1,
        "cursor edge belongs to the original invocation frame"
    );
    let activations = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let fields = &structure.fields;
            (fields.len() == 12
                && !fields[0].mutable
                && concrete_reference(fields[0].element_type) == Some((false, frames[0]))
                && fields[7].mutable
                && fields[7].element_type.unpack() == ValType::I32
                && concrete_reference(fields[4].element_type)
                    == concrete_reference(fields[5].element_type)
                && concrete_reference(fields[4].element_type).is_some_and(|(nullable, _)| nullable))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        activations.len(),
        1,
        "mixed enumeration uses the original request queue activation"
    );
    let mut cursor_new = 0;
    let mut cursor_reads = [0; 4];
    let mut cursor_writes = [0; 4];
    let mut edge_reads = 0;
    let mut edge_writes = 0;
    let mut ancestry_reads = 0;
    let mut saved_reads = 0;
    let mut saved_writes = 0;
    let mut invocation_reads = 0;
    let mut phase_writes = 0;
    let mut reference_reads = 0;
    let mut reference_writes = 0;
    // Shared cursor helpers live in R; source activation edges live in P.
    let runtime = artifact
        .runtime()
        .expect("enumeration links the shared runtime");
    for payload in Parser::new(0)
        .parse_all(&artifact.bytes)
        .chain(Parser::new(0).parse_all(runtime.bytes()))
    {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            for instruction in body.get_operators_reader().unwrap() {
                match instruction.unwrap() {
                    Operator::StructNew { struct_type_index } if struct_type_index == cursor => {
                        cursor_new += 1
                    }
                    Operator::StructGet {
                        struct_type_index,
                        field_index,
                    } if struct_type_index == cursor => cursor_reads[field_index as usize] += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index,
                    } if struct_type_index == cursor => cursor_writes[field_index as usize] += 1,
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 12,
                    } if struct_type_index == cell => edge_reads += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index: 12,
                    } if struct_type_index == cell => edge_writes += 1,
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 3,
                    } if struct_type_index == environments[0] => ancestry_reads += 1,
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 2,
                    } if struct_type_index == frames[0] => saved_reads += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index: 2,
                    } if struct_type_index == frames[0] => saved_writes += 1,
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 3,
                    } if struct_type_index == frames[0] => invocation_reads += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index: 7,
                    } if struct_type_index == activations[0] => phase_writes += 1,
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 10,
                    } if struct_type_index == cell => reference_reads += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index: 10,
                    } if struct_type_index == cell => reference_writes += 1,
                    _ => {}
                }
            }
        }
    }
    assert!(
        cursor_new > 0
            && cursor_reads.iter().all(|n| *n > 0)
            && cursor_writes.iter().all(|n| *n > 0)
            && edge_reads > 0
            && edge_writes > 0,
        "actual create/advance/retire accesses the one original field-12 cursor"
    );
    assert!(
        ancestry_reads > 0
            && saved_reads > 0
            && saved_writes > 0
            && invocation_reads > 0
            && phase_writes > 0,
        "per-key and nested source scopes resume through the original ancestry and request owners"
    );
    if requires_reference {
        assert!(
            reference_reads > 0 && reference_writes > 0,
            "body RHS References retain the original invocation-private field-10 edge"
        );
    }
}

#[test]
fn mixed_for_in_enumeration_fixture_emits_original_cursor_and_nested_lexical_ancestry() {
    inspect_mixed_for_in_artifact(
        include_str!(
            "../../../scriptw-engine/tests/fixtures/async_generator_for_in/enumeration.js"
        ),
        false,
    );
}

#[test]
fn mixed_for_in_completion_fixture_emits_original_reference_cursor_and_request_owners() {
    inspect_mixed_for_in_artifact(
        include_str!(
            "../../../scriptw-engine/tests/fixtures/async_generator_for_in/references_and_completions.js"
        ),
        true,
    );
}
