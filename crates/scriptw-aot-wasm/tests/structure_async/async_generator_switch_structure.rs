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

fn inspect_switch_artifact(source: &str, requires_abrupt_reference: bool) {
    let parsed = parse(source, ParseOptions::script()).expect("actual mixed Switch fixture parses");
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let artifact = scriptw_aot_wasm::emit(&program).expect(
        "complete mixed selectors consume their retained terminal scope in native emission",
    );
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
        .expect("CaseBlock, selector and finalizer branches have valid experimental Wasm types");
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
        "one original saved Environment ancestry domain"
    );
    let cells = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let cell = BindingCellLayout::new(index, structure)?;
            let selected = cell.identifier_reference();
            (selected.mutable
                && concrete_reference(selected.element_type).is_some_and(|(nullable, _)| nullable))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        cells.len(),
        1,
        "retained discriminant, V and references use existing BindingCells"
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
        "one original invocation and saved lexical frame"
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
        "Switch continues the original async-generator request queue"
    );
    let mut caseblock_allocations = 0;
    let mut parent_reads = 0;
    let mut saved_reads = 0;
    let mut saved_writes = 0;
    let mut invocation_reads = 0;
    let mut phase_writes = 0;
    let mut pending_reads = 0;
    let mut pending_writes = 0;
    let mut reference_reads = 0;
    let mut reference_writes = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            for instruction in body.get_operators_reader().unwrap() {
                match instruction.unwrap() {
                    Operator::StructNew { struct_type_index }
                        if struct_type_index == environments[0] =>
                    {
                        caseblock_allocations += 1
                    }
                    Operator::StructGet {
                        struct_type_index,
                        field_index: 3,
                    } if struct_type_index == environments[0] => parent_reads += 1,
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
                        field_index: 9,
                    } if struct_type_index == frames[0] => pending_reads += 1,
                    Operator::StructSet {
                        struct_type_index,
                        field_index: 9,
                    } if struct_type_index == frames[0] => pending_writes += 1,
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
                    _ => {}
                }
            }
        }
    }
    assert!(
        caseblock_allocations > 0 && parent_reads > 0,
        "captured CaseBlock cells allocate and restore the original lexical parent"
    );
    assert!(
        saved_reads > 0 && saved_writes > 0 && invocation_reads > 0 && phase_writes > 0,
        "full selector/body ranges publish through original activation and frame owners"
    );
    if requires_abrupt_reference {
        assert!(pending_reads > 0 && pending_writes > 0 && reference_reads > 0 && reference_writes > 0,
            "awaiting finalizers and captured RHS References use original private whole-completion edges");
    }
}

#[test]
fn mixed_switch_selection_fixture_emits_valid_caseblock_cells_and_original_queue() {
    inspect_switch_artifact(
        include_str!("../../../scriptw-engine/tests/fixtures/async_generator_switch/selection.js"),
        false,
    );
}

#[test]
fn mixed_switch_completion_fixture_emits_original_pending_completion_and_reference_edges() {
    inspect_switch_artifact(
        include_str!(
            "../../../scriptw-engine/tests/fixtures/async_generator_switch/completions.js"
        ),
        true,
    );
}
