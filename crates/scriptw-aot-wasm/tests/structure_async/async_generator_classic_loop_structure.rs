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

fn inspect_mixed_artifact(source: &str) {
    let parsed = parse(source, ParseOptions::script()).expect("actual mixed fixture parses");
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let artifact =
        scriptw_aot_wasm::emit(&program).expect("checked mixed regions reach native Wasm");
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
            "fresh/resumed mixed phase, condition and finalizer branches have valid Wasm types",
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
    let cells = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let cell = BindingCellLayout::new(index, structure)?;
            if cell.accumulation_index().element_type.unpack() != ValType::I64 {
                return None;
            }
            let selected = cell.identifier_reference();
            let (nullable, record) = concrete_reference(selected.element_type)?;
            let reference = &structures.get(&record)?.fields;
            if !nullable
                || !selected.mutable
                || reference.len() != 6
                || reference.iter().any(|field| field.mutable)
                || concrete_reference(reference[4].element_type) != Some((true, index))
            {
                return None;
            }
            Some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cells.len(),
        1,
        "one original BindingCell/native Reference topology"
    );
    let frames = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let fields = &structure.fields;
            if fields.len() != 15
                || fields[13].element_type.unpack() != ValType::I32
                || !fields[1..=3].iter().all(|field| field.mutable)
                || concrete_reference(fields[1].element_type)
                    != concrete_reference(fields[2].element_type)
                || concrete_reference(fields[2].element_type)
                    != concrete_reference(fields[3].element_type)
                || !concrete_reference(fields[3].element_type).is_some_and(|(nullable, _)| nullable)
            {
                return None;
            }
            Some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 1, "one original invocation frame");
    let activations = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let fields = &structure.fields;
            if fields.len() != 12
                || concrete_reference(fields[0].element_type) != Some((false, frames[0]))
                || fields[0].mutable
                || fields[7].element_type.unpack() != ValType::I32
                || !fields[7].mutable
                || concrete_reference(fields[4].element_type)
                    != concrete_reference(fields[5].element_type)
                || !concrete_reference(fields[4].element_type).is_some_and(|(nullable, _)| nullable)
            {
                return None;
            }
            Some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        activations.len(),
        1,
        "mixed phases use the original async-generator activation and queue"
    );
    let mut reference_reads = 0;
    let mut reference_writes = 0;
    let mut invocation_reads = 0;
    let mut phase_writes = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            for instruction in body.get_operators_reader().unwrap() {
                match instruction.unwrap() {
                    Operator::StructGet {
                        struct_type_index,
                        field_index,
                    } if struct_type_index == cells[0]
                        && field_index == BindingCellLayout::IDENTIFIER_REFERENCE =>
                    {
                        reference_reads += 1
                    }
                    Operator::StructSet {
                        struct_type_index,
                        field_index,
                    } if struct_type_index == cells[0]
                        && field_index == BindingCellLayout::IDENTIFIER_REFERENCE =>
                    {
                        reference_writes += 1
                    }
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 3,
                    } if struct_type_index == frames[0] => invocation_reads += 1,
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
        reference_reads > 0 && reference_writes > 0,
        "actual captured Get/Put and retirement retain the native edge"
    );
    assert!(
        invocation_reads > 0 && phase_writes > 0,
        "actual original-frame roots and mixed phase publications are emitted"
    );
}

#[test]
fn mixed_classic_fixture_artifacts_validate_shared_gc_roots_and_reconstructed_control_frames() {
    for source in [
        include_str!("../../../scriptw-engine/tests/fixtures/async_generator_classic_regions/phases_and_branches.js"),
        include_str!("../../../scriptw-engine/tests/fixtures/async_generator_classic_regions/completions_and_environments.js"),
    ] {
        inspect_mixed_artifact(source);
        inspect_mixed_artifact(&format!("\"use strict\";\n{source}"));
    }
}
