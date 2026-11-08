use super::host_schema::BindingCellLayout;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use std::collections::BTreeMap;
use wasmparser::{
    CompositeInnerType, HeapType, Operator, Parser, Payload, StorageType, StructType, ValType,
    Validator, WasmFeatures,
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

fn inspect_async_pattern_artifact(source: &str, requires_reference: bool) {
    let parsed = parse(source, ParseOptions::script()).expect("actual async pattern source parses");
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    let artifact = scriptw_aot_wasm::emit(&program)
        .expect("actual async pattern regions reach the shared native iterator pipeline");
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
        .expect("actual Await/close scopes and retained GC references have valid Wasm types");

    let mut structures = BTreeMap::<u32, StructType>::new();
    let mut next_type = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::TypeSection(reader) = payload.unwrap() {
            for group in reader {
                for ty in group.unwrap().types() {
                    if let CompositeInnerType::Struct(structure) = &ty.composite_type.inner {
                        structures.insert(next_type, structure.clone());
                    }
                    next_type += 1;
                }
            }
        }
    }
    // Follow both existing private BindingCell edges to their actual records.
    // Async patterns append no field and introduce no alternate iterator type.
    let cells = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let layout = BindingCellLayout::new(index, structure)?;
            let iterator_edge = layout.iterator_record();
            let reference_edge = layout.identifier_reference();
            let (nullable, iterator) = concrete_reference(iterator_edge.element_type)?;
            let (reference_nullable, reference) = concrete_reference(reference_edge.element_type)?;
            if !nullable || !iterator_edge.mutable || !reference_nullable || !reference_edge.mutable
            {
                return None;
            }
            let iterator_fields = &structures.get(&iterator)?.fields;
            if iterator_fields.len() != 3
                || iterator_fields[0].mutable
                || iterator_fields[1].mutable
                || !iterator_fields[2].mutable
                || iterator_fields[2].element_type.unpack() != ValType::I32
                || concrete_reference(iterator_fields[0].element_type).is_none()
                || concrete_reference(iterator_fields[0].element_type)
                    != concrete_reference(iterator_fields[1].element_type)
            {
                return None;
            }
            let (nullable, value) = concrete_reference(iterator_fields[0].element_type)?;
            if nullable || !layout.matches_stored_value(structures.get(&value)?) {
                return None;
            }
            let reference_fields = &structures.get(&reference)?.fields;
            if reference_fields.len() != 6
                || reference_fields.iter().any(|field| field.mutable)
                || reference_fields[1].element_type.unpack() != ValType::I32
                || concrete_reference(reference_fields[4].element_type) != Some((true, index))
            {
                return None;
            }
            Some((index, iterator))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cells.len(),
        1,
        "one original BindingCell/IteratorRecord pair"
    );
    let (cell, iterator) = cells[0];
    let mut retained_loads = [0, 0];
    let mut retained_writes = [0, 0];
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            for instruction in body.get_operators_reader().unwrap() {
                match instruction.unwrap() {
                    Operator::StructGet {
                        struct_type_index,
                        field_index,
                    } if struct_type_index == cell
                        && [
                            BindingCellLayout::IDENTIFIER_REFERENCE,
                            BindingCellLayout::ITERATOR_RECORD,
                        ]
                        .contains(&field_index) =>
                    {
                        retained_loads
                            [(field_index - BindingCellLayout::IDENTIFIER_REFERENCE) as usize] += 1;
                    }
                    Operator::StructSet {
                        struct_type_index,
                        field_index,
                    } if struct_type_index == cell
                        && [
                            BindingCellLayout::IDENTIFIER_REFERENCE,
                            BindingCellLayout::ITERATOR_RECORD,
                        ]
                        .contains(&field_index) =>
                    {
                        retained_writes
                            [(field_index - BindingCellLayout::IDENTIFIER_REFERENCE) as usize] += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(
        retained_loads[1] > 0,
        "body resume reloads the actual retained iterator"
    );
    assert!(
        retained_writes[1] >= 2,
        "actual iterator edge is published and retired"
    );
    let (cached_fields, done_writes) =
        super::array_iterator_runtime_ownership::called_iterator_operations(
            &artifact, cell, iterator,
        );
    assert!(
        cached_fields.iter().all(|&count| count > 0),
        "cached iterator and next are consumed"
    );
    assert!(
        done_writes > 0,
        "the shared step and close owners consume native Done"
    );
    if requires_reference {
        assert!(
            retained_loads[0] > 0,
            "resumed Put uses the original captured Reference"
        );
        assert!(
            retained_writes[0] >= 2,
            "actual Reference publication and retirement are emitted"
        );
    }
}

#[test]
fn actual_async_array_semantic_cohorts_emit_valid_existing_gc_records() {
    for fixture in [
        include_str!("../../../scriptw-engine/tests/fixtures/async_array_patterns/bindings.js"),
        include_str!("../../../scriptw-engine/tests/fixtures/async_array_patterns/assignments.js"),
    ] {
        for directive in ["", "'use strict';\n"] {
            inspect_async_pattern_artifact(&format!("{directive}{fixture}"), false);
        }
    }
}

#[test]
fn rejected_await_and_normal_resume_consume_original_iterator_and_identifier_records() {
    inspect_async_pattern_artifact(
        r#"
        var selected = 0;
        async function pattern(source, wait) {
            try {
                [selected = await wait, ...source.rest] = source;
                return selected;
            } catch (error) {
                return error;
            } finally {
                await Promise.resolve('finally');
            }
        }
        pattern([undefined, 2], Promise.resolve(7));
        pattern([undefined, 3], Promise.reject({marker: 17}));
        gc();
        "#,
        true,
    );
}
