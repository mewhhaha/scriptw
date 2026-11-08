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

fn inspect_native_pattern_artifact(source: &str) {
    let parsed = parse(source, ParseOptions::script()).expect("actual source fixture parses");
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    let artifact = scriptw_aot_wasm::emit(&program)
        .expect("actual Array-pattern regions reach native codegen");
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
        .expect(
            "retained iterator field, constructor and resumed close branches have valid Wasm types",
        );

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
    // Identify the real recursive BindingCell shape and its appended nullable
    // edge, then follow that exact edge to the existing native IteratorRecord.
    let cells = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let layout = BindingCellLayout::new(index, structure)?;
            let iterator_edge = layout.iterator_record();
            let enumerator_edge = layout.for_in_record();
            let (nullable, record) = concrete_reference(iterator_edge.element_type)?;
            let (enumerator_nullable, _) = concrete_reference(enumerator_edge.element_type)?;
            if !nullable
                || !iterator_edge.mutable
                || !enumerator_nullable
                || !enumerator_edge.mutable
            {
                return None;
            }
            let record_fields = &structures.get(&record)?.fields;
            if record_fields.len() != 3
                || record_fields[0].mutable
                || record_fields[1].mutable
                || !record_fields[2].mutable
                || record_fields[2].element_type.unpack() != ValType::I32
                || concrete_reference(record_fields[0].element_type).is_none()
                || concrete_reference(record_fields[0].element_type)
                    != concrete_reference(record_fields[1].element_type)
            {
                return None;
            }
            let (nullable, value) = concrete_reference(record_fields[0].element_type)?;
            if nullable || !layout.matches_stored_value(structures.get(&value)?) {
                return None;
            }
            Some((index, record))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cells.len(),
        1,
        "one actual native BindingCell/IteratorRecord pair"
    );
    let (cell, record) = cells[0];
    let mut record_loads = 0;
    let mut record_publications_and_retirements = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            for instruction in body.get_operators_reader().unwrap() {
                match instruction.unwrap() {
                    Operator::StructGet {
                        struct_type_index,
                        field_index: BindingCellLayout::ITERATOR_RECORD,
                    } if struct_type_index == cell => record_loads += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index: BindingCellLayout::ITERATOR_RECORD,
                    } if struct_type_index == cell => record_publications_and_retirements += 1,
                    _ => {}
                }
            }
        }
    }
    assert!(
        record_loads > 0,
        "resumed regions consume the retained native record"
    );
    assert!(
        record_publications_and_retirements >= 2,
        "actual record edge is published and retired"
    );
    let (cached_iterator_and_next, done_writes) =
        super::array_iterator_runtime_ownership::called_iterator_operations(
            &artifact, cell, record,
        );
    assert!(
        cached_iterator_and_next.iter().all(|&count| count > 0),
        "both actual cached fields are consumed"
    );
    assert!(
        done_writes > 0,
        "step and close mutate the native DONE field"
    );
}

#[test]
fn actual_array_pattern_semantic_cohorts_emit_valid_retained_record_gc_code() {
    let fixture = include_str!(
        "../../../scriptw-engine/tests/fixtures/generator_array_patterns/array_patterns.js"
    );
    for directive in ["", "'use strict';\n"] {
        inspect_native_pattern_artifact(&format!("{directive}{fixture}"));
    }
    inspect_native_pattern_artifact(include_str!(
        "../../../scriptw-engine/tests/fixtures/generator_array_patterns/with_references.js"
    ));
}

#[test]
fn nested_patterns_and_delegated_defaults_share_the_existing_gc_iterator_record() {
    inspect_native_pattern_artifact(
        r#"
        function* defaults() { yield 'inner'; return 3; }
        function* pattern(source) {
            let closures = [];
            try {
                for (let [first = (yield 'head'), step] = source; first < 2; first++) {
                    closures.push(() => [first, step]);
                    [source.slot, [source.nested = (yield* defaults())], ...source.rest] = source;
                    yield closures[0]();
                }
            } finally { yield 'finally'; }
            return closures;
        }
        var iterator = pattern([undefined, [undefined], 7]);
        iterator.next();
        gc();
        iterator.next(0);
        iterator.throw({ marker: 17 });
    "#,
    );
}
