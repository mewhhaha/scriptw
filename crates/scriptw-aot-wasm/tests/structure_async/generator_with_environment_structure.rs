#[path = "../fixtures/linked_bodies.rs"]
mod linked_bodies;

use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use std::collections::{BTreeMap, BTreeSet};
use wasmparser::{
    CompositeInnerType, HeapType, KnownCustom, Name, Operator, Parser, Payload, StorageType,
    StructType, TypeRef, ValType, Validator, WasmFeatures,
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

fn inspect_actual_with_artifact(source: &str, check_shared_global_reads: bool) {
    let parsed = parse(source, ParseOptions::script()).expect("actual With source parses");
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    let artifact =
        scriptw_aot_wasm::emit(&program).expect("checked complete With reaches native emission");
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
        .expect("saved With cell, fresh/resumed scopes and outward completion branches typecheck");
    if check_shared_global_reads {
        inspect_shared_global_reads(&artifact);
    }
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
    // Follow the actual physical Environment -> ObjectEnvironment -> hidden
    // BindingCell edges; no encoded JavaScript environment is involved.
    let owners = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let fields = &structure.fields;
            if fields.len() != 9
                || fields[2].mutable
                || fields[2].element_type.unpack() != ValType::I32
                || concrete_reference(fields[3].element_type) != Some((true, index))
            {
                return None;
            }
            let (nullable, object) = concrete_reference(fields[6].element_type)?;
            if !nullable || fields[6].mutable {
                return None;
            }
            let object_fields = &structures.get(&object)?.fields;
            if object_fields.len() != 2
                || object_fields[0].mutable
                || object_fields[1].mutable
                || object_fields[1].element_type.unpack() != ValType::I32
            {
                return None;
            }
            let (nullable, cell) = concrete_reference(object_fields[0].element_type)?;
            if nullable {
                return None;
            }
            let cell_fields = &structures.get(&cell)?.fields;
            if cell_fields.len() != 13
                || concrete_reference(cell_fields[4].element_type) != Some((true, cell))
                || !cell_fields[12].mutable
                || !concrete_reference(cell_fields[12].element_type)
                    .is_some_and(|(nullable, _)| nullable)
            {
                return None;
            }
            Some((index, object, cell))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        owners.len(),
        1,
        "one existing actual With record/cell topology"
    );
    let (environment, object, _) = owners[0];
    let frames = structures
        .iter()
        .filter_map(|(&index, structure)| {
            let fields = &structure.fields;
            (fields.len() == 15
                && fields[1].mutable
                && fields[2].mutable
                && fields[3].mutable
                && [1, 2, 3].iter().all(|&i| {
                    concrete_reference(fields[i].element_type) == Some((true, environment))
                }))
            .then_some(index)
        })
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 1, "same original resumable InvocationFrame");
    let mut object_records = 0;
    let mut parent_reads = 0;
    let mut saved_reads = 0;
    let mut saved_writes = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            for instruction in body.get_operators_reader().unwrap() {
                match instruction.unwrap() {
                    Operator::StructNew { struct_type_index } if struct_type_index == object => {
                        object_records += 1
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
                    _ => {}
                }
            }
        }
    }
    assert!(
        object_records > 0,
        "original With object records are allocated"
    );
    assert!(
        parent_reads > 0,
        "restoration consumes actual parent records"
    );
    assert!(
        saved_reads > 0 && saved_writes > 0,
        "same saved lexical record is read and published"
    );
}

fn inspect_shared_global_reads(artifact: &scriptw_aot_wasm::WasmArtifact) {
    let runtime = artifact.runtime().expect("With reads link R");
    let modules = [runtime.bytes(), artifact.bytes.as_slice()];
    let mut names = BTreeMap::new();
    for (module, bytes) in modules.into_iter().enumerate() {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect("both linked With modules validate");
        for payload in Parser::new(0).parse_all(bytes) {
            if let Payload::CustomSection(section) = payload.expect("module decodes") {
                if let KnownCustom::Name(subsections) = section.as_known() {
                    for subsection in subsections {
                        if let Name::Function(map) = subsection.expect("name subsection decodes") {
                            for naming in map {
                                let naming = naming.expect("name decodes");
                                assert!(
                                    names
                                        .insert((module, naming.index), naming.name.to_string())
                                        .is_none(),
                                    "each module owns unique function names"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
    let unique_index = |name: &str| {
        let matches = names
            .iter()
            .filter_map(|(&index, actual)| (actual == name).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "one emitted body for {name}");
        matches[0]
    };
    let main = unique_index("scriptw::main");
    let helpers = [
        "helper::global_identifier_read_sloppy",
        "helper::global_identifier_read_strict",
        "helper::global_identifier_typeof_sloppy",
        "helper::global_identifier_typeof_strict",
    ]
    .map(|name| (name, unique_index(name)));
    let imports = linked_bodies::runtime_function_imports(&artifact.bytes, runtime.bytes());
    let mut body_sizes = BTreeMap::new();
    let mut main_calls = BTreeSet::new();
    for (module, bytes) in modules.into_iter().enumerate() {
        let mut next_function = 0u32;
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.expect("shared global read artifact decodes") {
                Payload::ImportSection(reader) => {
                    for import in reader.into_imports() {
                        if matches!(
                            import.expect("import decodes").ty,
                            TypeRef::Func(_) | TypeRef::FuncExact(_)
                        ) {
                            next_function += 1;
                        }
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    assert!(body_sizes
                        .insert((module, next_function), body.range().len())
                        .is_none());
                    if (module, next_function) == main {
                        for operator in body.get_operators_reader().expect("main body opens") {
                            match operator.expect("main instruction decodes") {
                                Operator::Call { function_index }
                                | Operator::ReturnCall { function_index } => {
                                    let target = imports
                                        .get(&function_index)
                                        .map(|index| (0, *index))
                                        .unwrap_or((module, function_index));
                                    main_calls.insert(target);
                                }
                                _ => {}
                            }
                        }
                    }
                    next_function += 1;
                }
                _ => {}
            }
        }
    }
    assert_eq!(body_sizes.len(), names.len());
    for (name, index) in helpers {
        assert!(
            body_sizes.contains_key(&index),
            "{name} has actual encoded code"
        );
    }
    assert!(
        main_calls.contains(&helpers[0].1),
        "main consumes the shared sloppy Value read"
    );
    assert!(
        main_calls.contains(&helpers[2].1),
        "main consumes the shared sloppy typeof read"
    );
    eprintln!(
        "shared-global-read scriptw::main: {} bytes",
        body_sizes[&main]
    );
    let largest = names
        .iter()
        .max_by_key(|(index, _)| body_sizes[*index])
        .expect("at least main is emitted");
    eprintln!(
        "shared-global-read largest: {}: {} bytes",
        largest.1, body_sizes[largest.0]
    );
    let oversized = names
        .iter()
        .filter_map(|(index, name)| {
            let bytes = body_sizes[index];
            (bytes >= 1024 * 1024).then_some((name, bytes))
        })
        .collect::<Vec<_>>();
    assert!(
        oversized.is_empty(),
        "every encoded body must remain below the existing 1 MiB native optimization switch: {oversized:?}"
    );
}

#[test]
fn complete_with_semantic_cohort_emits_valid_original_gc_environment_cells() {
    inspect_actual_with_artifact(
        include_str!("../../../scriptw-engine/tests/fixtures/generator_with.js"),
        false,
    );
}

#[test]
fn escaped_generator_switch_uses_original_captured_with_cells_across_resumes() {
    // Exactly the complete sloppy source from the existing native Switch test.
    // Its predecessor main was 1,638,171 bytes and changed whole-module policy.
    let source = format!(
        "{}\n{}",
        include_str!(
            "../../../scriptw-engine/tests/fixtures/generator_switch_regions/captured_with.js"
        ),
        include_str!("../../../scriptw-engine/tests/fixtures/generator_switch_regions/switches.js"),
    );
    inspect_actual_with_artifact(&source, true);
}

#[test]
fn nested_with_and_iterator_finalizers_share_saved_native_parent_records() {
    inspect_actual_with_artifact(
        r#"
        var retained;
        function* run(object, input) {
            outer: for (let i = 0; i < 2; i++) {
                try {
                    with (yield 'head') {
                        retained = () => [value, i];
                        let [selected = (yield 'default')] = input;
                        with (object) { value = (yield 'write'); }
                        if (selected) continue outer;
                        yield* input;
                        break outer;
                    }
                } finally { yield 'finally'; }
            }
            return retained;
        }
        var iterator = run({value: 3}, [undefined]);
        iterator.next(); gc(); iterator.next({value: 7}); gc();
        iterator.return({marker: 23});
    "#,
        false,
    );
}
