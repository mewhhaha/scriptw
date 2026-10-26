use super::host_schema::is_stored_value;
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

fn inspect_async_for_in_artifact(source: &str, requires_reference: bool) {
    let parsed =
        parse(source, ParseOptions::script()).expect("actual plain Async ForIn source parses");
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    let artifact = scriptw_aot_wasm::emit(&program)
        .expect("actual checked Async ForIn reaches the original native cursor");
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
        .expect("actual Await, whole Completion and original environment branches validate");
    let mut structures = BTreeMap::<u32, StructType>::new();
    let mut arrays = BTreeMap::<u32, ArrayType>::new();
    let mut next_type = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::TypeSection(reader) = payload.unwrap() {
            for group in reader {
                for ty in group.unwrap().types() {
                    match &ty.composite_type.inner {
                        CompositeInnerType::Struct(structure) => {
                            structures.insert(next_type, structure.clone());
                        }
                        CompositeInnerType::Array(array) => {
                            arrays.insert(next_type, array.clone());
                        }
                        CompositeInnerType::Func(_) | CompositeInnerType::Cont(_) => {}
                    }
                    next_type += 1;
                }
            }
        }
    }
    let owners = structures
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
            let cursor_fields = &structures.get(&cursor)?.fields;
            if !nullable
                || cursor_fields.len() != 4
                || cursor_fields.iter().any(|field| !field.mutable)
                || cursor_fields[2].element_type.unpack() != ValType::I32
            {
                return None;
            }
            let (nullable, value) = concrete_reference(cursor_fields[0].element_type)?;
            if nullable || concrete_reference(fields[0].element_type) != Some((false, value)) {
                return None;
            }
            let (nullable, keys) = concrete_reference(cursor_fields[1].element_type)?;
            if !nullable || concrete_reference(cursor_fields[3].element_type) != Some((false, keys))
            {
                return None;
            }
            let key_element = arrays.get(&keys)?.0;
            if !key_element.mutable
                || concrete_reference(key_element.element_type) != Some((false, value))
            {
                return None;
            }
            if !is_stored_value(structures.get(&value)?) {
                return None;
            }
            Some((cell, cursor))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        owners.len(),
        1,
        "one original native cursor and BindingCell edge"
    );
    let (cell, cursor) = owners[0];
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
        "same physical TDZ and iteration Environment type"
    );
    let environment = environments[0];
    let frames = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let fields = &structure.fields;
            (fields.len() == 15
                && [1, 2, 3].iter().all(|&i| {
                    fields[i].mutable
                        && concrete_reference(fields[i].element_type) == Some((true, environment))
                }))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        frames.len(),
        1,
        "same actual Async InvocationFrame environment edges"
    );
    let frame = frames[0];
    let mut cursor_new = 0;
    let mut cursor_reads = [0; 4];
    let mut cursor_writes = [0; 4];
    let mut edge_loads = 0;
    let mut edge_writes = 0;
    let mut invocation_reads = 0;
    let mut lexical_reads = 0;
    let mut lexical_writes = 0;
    let mut reference_reads = 0;
    let mut reference_writes = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
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
                    } if struct_type_index == cell => edge_loads += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index: 12,
                    } if struct_type_index == cell => edge_writes += 1,
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 3,
                    } if struct_type_index == frame => invocation_reads += 1,
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 2,
                    } if struct_type_index == frame => lexical_reads += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index: 2,
                    } if struct_type_index == frame => lexical_writes += 1,
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
            && cursor_reads.into_iter().all(|count| count > 0)
            && cursor_writes.into_iter().all(|count| count > 0),
        "actual shared creation/advance consumes every persisted cursor field"
    );
    assert!(
        edge_loads > 0 && edge_writes > 1 && invocation_reads > 0,
        "cursor publication, resumed load and retirement use the original invocation cell"
    );
    assert!(
        lexical_reads > 0 && lexical_writes > 0,
        "Await retains and reattaches actual TDZ/per-key environment records"
    );
    if requires_reference {
        assert!(
            reference_reads > 0 && reference_writes > 0,
            "body assignment retains its original Identifier Reference across Await"
        );
    }
}

#[test]
fn complete_async_head_proxy_and_body_regions_emit_the_original_native_enumeration_cursor() {
    inspect_async_for_in_artifact(
        include_str!("../../../scriptw-engine/tests/fixtures/async_for_in/enumeration.js"),
        false,
    );
}

#[test]
fn async_reference_and_whole_completion_cohort_uses_original_cursor_and_activation_edges() {
    inspect_async_for_in_artifact(
        include_str!(
            "../../../scriptw-engine/tests/fixtures/async_for_in/references_and_completions.js"
        ),
        true,
    );
}
