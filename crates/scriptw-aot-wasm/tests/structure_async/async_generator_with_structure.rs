use crate::host_schema::BindingCellLayout;
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

fn inspect_with_artifact(source: &str, requires_reference: bool) {
    let parsed = parse(source, ParseOptions::script()).expect("actual mixed With fixture parses");
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let artifact = scriptw_aot_wasm::emit(&program)
        .expect("checked mixed With reaches the original native environment lifetime");
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
        .expect("actual mixed head/body/finalizer environment scopes have valid Wasm types");
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
    let owners = structures
        .iter()
        .filter_map(|(&environment, structure)| {
            let fields = &structure.fields;
            if fields.len() != 9
                || fields[2].mutable
                || fields[2].element_type.unpack() != ValType::I32
                || concrete_reference(fields[3].element_type) != Some((true, environment))
            {
                return None;
            }
            let (nullable, object) = concrete_reference(fields[6].element_type)?;
            let object_fields = &structures.get(&object)?.fields;
            if !nullable
                || fields[6].mutable
                || object_fields.len() != 2
                || object_fields.iter().any(|field| field.mutable)
                || object_fields[1].element_type.unpack() != ValType::I32
            {
                return None;
            }
            let (nullable, cell) = concrete_reference(object_fields[0].element_type)?;
            let layout = BindingCellLayout::new(cell, structures.get(&cell)?)?;
            let retained = layout.identifier_reference();
            if nullable
                || !retained.mutable
                || !concrete_reference(retained.element_type).is_some_and(|(nullable, _)| nullable)
            {
                return None;
            }
            Some((environment, object, cell))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        owners.len(),
        1,
        "one original ObjectER and hidden BindingCell domain"
    );
    let (environment, object, cell) = owners[0];
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
        "one original invocation frame with saved lexical ancestry"
    );
    let activations = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let fields = &structure.fields;
            (fields.len() == 12
                && concrete_reference(fields[0].element_type) == Some((false, frames[0]))
                && !fields[0].mutable
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
        "With uses the existing async-generator queue and activation"
    );
    let mut allocated_objects = 0;
    let mut parent_reads = 0;
    let mut saved_reads = 0;
    let mut saved_writes = 0;
    let mut invocation_reads = 0;
    let mut reference_reads = 0;
    let mut reference_writes = 0;
    let mut phase_writes = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            for instruction in body.get_operators_reader().unwrap() {
                match instruction.unwrap() {
                    Operator::StructNew { struct_type_index } if struct_type_index == object => {
                        allocated_objects += 1
                    }
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 3,
                    } if struct_type_index == environment => parent_reads += 1,
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
                    Operator::StructGet {
                        struct_type_index,
                        field_index,
                    } if struct_type_index == cell
                        && field_index == BindingCellLayout::IDENTIFIER_REFERENCE =>
                    {
                        reference_reads += 1
                    }
                    Operator::StructSet {
                        struct_type_index,
                        field_index,
                    } if struct_type_index == cell
                        && field_index == BindingCellLayout::IDENTIFIER_REFERENCE =>
                    {
                        reference_writes += 1
                    }
                    Operator::StructSet {
                        struct_type_index,
                        field_index: 7,
                    } if struct_type_index == activations[0] => phase_writes += 1,
                    _ => {}
                }
            }
        }
    }
    assert!(
        allocated_objects > 0 && parent_reads > 0,
        "fresh With record allocation and outward parent restoration are emitted"
    );
    assert!(
        saved_reads > 0 && saved_writes > 0 && invocation_reads > 0 && phase_writes > 0,
        "head/body resume publications use the original queue, frame and retained head cell"
    );
    if requires_reference {
        assert!(
            reference_reads > 0 && reference_writes > 0,
            "selected Identifier References consume the existing invocation-private edge"
        );
    }
}

#[test]
fn mixed_with_environment_fixture_emits_valid_original_gc_and_request_queue_owners() {
    inspect_with_artifact(
        include_str!("../../../scriptw-engine/tests/fixtures/async_generator_with/environments.js"),
        false,
    );
}

#[test]
fn mixed_with_reference_and_completion_fixture_emits_original_retained_cells_and_cleanup() {
    inspect_with_artifact(
        include_str!(
            "../../../scriptw-engine/tests/fixtures/async_generator_with/references_and_completions.js"
        ),
        true,
    );
}
