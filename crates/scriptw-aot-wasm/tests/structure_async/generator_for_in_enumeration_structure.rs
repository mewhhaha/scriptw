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

fn inspect_actual_enumeration_artifact(source: &str) {
    let parsed = parse(source, ParseOptions::script()).expect("actual ForIn source parses");
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    let artifact =
        scriptw_aot_wasm::emit(&program).expect("checked complete ForIn reaches native emission");
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
    Validator::new_with_features(features).validate_all(&artifact.bytes)
        .expect("actual shared cursor, retained invocation cells and all continuation branches validate");
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
    // Follow the actual appended BindingCell edge into the unique physical
    // cursor and both accepted-key tables. They contain real StoredValues.
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
            if !nullable {
                return None;
            }
            let cursor_fields = &structures.get(&cursor)?.fields;
            if cursor_fields.len() != 4
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
        "one native BindingCell/ForIn cursor topology"
    );
    let (cell, cursor) = owners[0];
    let mut cursor_new = 0;
    let mut cursor_read = [0; 4];
    let mut cursor_write = [0; 4];
    let mut edge_loads = 0;
    let mut edge_publications_and_retirements = 0;
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
                    } if struct_type_index == cursor => cursor_read[field_index as usize] += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index,
                    } if struct_type_index == cursor => cursor_write[field_index as usize] += 1,
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 12,
                    } if struct_type_index == cell => edge_loads += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index: 12,
                    } if struct_type_index == cell => edge_publications_and_retirements += 1,
                    _ => {}
                }
            }
        }
    }
    assert!(
        cursor_new > 0,
        "native creation is an actual emitted consumer"
    );
    assert!(
        cursor_read.into_iter().all(|count| count > 0),
        "advance reads every persisted cursor field"
    );
    assert!(
        cursor_write.into_iter().all(|count| count > 0),
        "native advance publishes actual transitions"
    );
    assert!(
        edge_loads > 0 && edge_publications_and_retirements > 1,
        "the exact original invocation cell publishes, resumes and retires its cursor"
    );
}

#[test]
fn full_head_body_and_proxy_regions_emit_the_same_valid_native_cursor() {
    inspect_actual_enumeration_artifact(include_str!(
        "../../../scriptw-engine/tests/fixtures/generator_for_in/regions.js"
    ));
}

#[test]
fn original_reference_and_pattern_heads_share_native_enumeration_and_retirement() {
    inspect_actual_enumeration_artifact(include_str!(
        "../../../scriptw-engine/tests/fixtures/generator_for_in/assignments.js"
    ));
}
