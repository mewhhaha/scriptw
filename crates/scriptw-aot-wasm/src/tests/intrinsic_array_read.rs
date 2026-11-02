//! Inspect the actual source read, its guarded load, and the complete miss arm.
use super::*;
use crate::gc_types::{
    ArrayObject, GcHeapType, GcNullability, GcSnapshotField, GlobalLedger,
    IntrinsicPropertyStorage, IntrinsicPropertyValueCache, OrdinaryObject, OrdinaryPropertyStorage,
    RuntimeModuleTypes, RuntimeSchema, StoredValue,
};
use std::ops::Range;

struct Types {
    array: u32,
    object: u32,
    dictionary: u32,
    intrinsic: u32,
    cache: u32,
    stored: u32,
}

fn type_index<T: GcHeapType>(schema: &RuntimeSchema) -> u32 {
    let wasm_encoder::HeapType::Concrete(index) = schema
        .reference_type::<T>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("registered GC type has a concrete index");
    };
    index
}

fn tests_type(operator: &Operator<'_>, index: u32) -> bool {
    matches!(operator, Operator::RefTestNonNull { hty }
        if *hty == wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index)))
}

fn reads_field(operator: &Operator<'_>, owner: u32, field: GcSnapshotField) -> bool {
    matches!(operator, Operator::StructGet { struct_type_index, field_index }
        if *struct_type_index == owner && *field_index == field.index())
}

fn matching_end(operators: &[Operator<'_>], start: usize) -> usize {
    let mut depth = 0;
    for (index, operator) in operators.iter().enumerate().skip(start) {
        match operator {
            Operator::Block { .. }
            | Operator::If { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. } => depth += 1,
            Operator::End => {
                depth -= 1;
                if depth == 0 {
                    return index;
                }
            }
            _ => {}
        }
    }
    panic!("structured source arm ends");
}

fn guards(operators: &[Operator<'_>], intrinsic: u32) -> Vec<Range<usize>> {
    let mut stack = Vec::new();
    let mut ranges = Vec::new();
    for (index, operator) in operators.iter().enumerate() {
        match operator {
            Operator::Block { .. }
            | Operator::If { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. } => stack.push(index),
            Operator::End => {
                stack.pop();
            }
            _ if tests_type(operator, intrinsic) => {
                // Ordinary own-data probes also test this storage type inside
                // an If. The observed inherited read instead exits its Block
                // directly on a failed subtype test.
                if !matches!(
                    operators.get(index + 1..index + 3),
                    Some([Operator::I32Eqz, Operator::BrIf { relative_depth: 0 }])
                ) {
                    continue;
                }
                let start = *stack.last().expect("guard has an enclosing block");
                assert!(matches!(operators[start], Operator::Block { .. }));
                ranges.push(start..matching_end(operators, start) + 1);
            }
            _ => {}
        }
    }
    ranges
}

fn helper_imports(artifact: &WasmArtifact) -> BTreeMap<String, u32> {
    let runtime = artifact.runtime().unwrap();
    let names = function_names_in_bytes(runtime.bytes());
    let mut exports = BTreeMap::new();
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ExportSection(section) = payload.unwrap() {
            for export in section {
                let export = export.unwrap();
                if export.kind == wasmparser::ExternalKind::Func {
                    if let Some(name) = names.get(&export.index) {
                        exports.insert(export.name, name.clone());
                    }
                }
            }
        }
    }
    let mut helpers = BTreeMap::new();
    let mut index = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(section) = payload.unwrap() {
            for import in section.into_imports() {
                let import = import.unwrap();
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE {
                        if let Some(name) = exports.get(import.name) {
                            helpers.insert(name.clone(), index);
                        }
                    }
                    index += 1;
                }
            }
        }
    }
    helpers
}

fn with_bodies(
    source: &str,
    name: &str,
    mut check: impl FnMut(&str, &[Operator<'_>], &Types, &BTreeMap<String, u32>),
) {
    let artifact = emit_script(source).expect("Array read control emits");
    expect_valid_module(&artifact, 1);
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Program(
        artifact.runtime().unwrap(),
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let types = Types {
        array: type_index::<ArrayObject>(schema),
        object: type_index::<OrdinaryObject>(schema),
        dictionary: type_index::<OrdinaryPropertyStorage>(schema),
        intrinsic: type_index::<IntrinsicPropertyStorage>(schema),
        cache: type_index::<IntrinsicPropertyValueCache>(schema),
        stored: type_index::<StoredValue>(schema),
    };
    let helpers = helper_imports(&artifact);
    let names = function_names(&artifact)
        .into_iter()
        .filter(|(_, actual)| actual.starts_with(&format!("js::{name}#")))
        .collect::<BTreeMap<_, _>>();
    assert!(!names.is_empty());
    let mut index = imported_function_count(&artifact);
    let mut checked = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if names.contains_key(&index) {
                let operators = body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect::<Vec<_>>();
                check(&names[&index], &operators, &types, &helpers);
                checked += 1;
            }
            index += 1;
        }
    }
    assert_eq!(
        checked,
        names.len(),
        "inspect every physical source variant"
    );
}

fn check_guard(operators: &[Operator<'_>], types: &Types) {
    assert!(
        operators.iter().all(|op| !matches!(
            op,
            Operator::Call { .. }
                | Operator::CallRef { .. }
                | Operator::CallIndirect { .. }
                | Operator::StructNew { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. }
        )),
        "a completed DATA hit performs no lookup, coercion or allocation"
    );
    assert_eq!(
        operators
            .iter()
            .filter(|op| tests_type(op, types.array))
            .count(),
        2,
        "both the actual receiver and its live immediate prototype are Arrays"
    );
    assert!(operators.iter().any(|op| tests_type(op, types.dictionary)));
    let own = operators
        .iter()
        .position(|op| reads_field(op, types.dictionary, GcSnapshotField::PropertiesLength))
        .unwrap();
    let prototype = operators
        .iter()
        .position(|op| reads_field(op, types.object, GcSnapshotField::ObjectPrototype))
        .unwrap();
    let layout = operators
        .iter()
        .position(|op| tests_type(op, types.intrinsic))
        .unwrap();
    let cache = operators.iter().position(|op| matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == types.cache)).unwrap();
    let completed = operators
        .iter()
        .position(|op| tests_type(op, types.stored))
        .unwrap();
    assert!(own < prototype && prototype < layout && layout < cache && cache < completed);
    assert!(
        operators.iter().all(
            |op| !reads_field(op, types.array, GcSnapshotField::ArrayLength)
                && !reads_field(op, types.array, GcSnapshotField::ArrayCount)
        ),
        "named absence does not require an empty indexed Array"
    );
    assert_eq!(
        operators[layout..cache]
            .windows(6)
            .filter(|ops| matches!(ops,
        [Operator::StructGet { struct_type_index, .. }, Operator::LocalSet { local_index: a },
         Operator::LocalGet { local_index: b }, Operator::I32Const { .. }, Operator::I32Ne,
         Operator::BrIf { relative_depth: 0 }]
        if *struct_type_index == types.intrinsic && a == b))
            .count(),
        2,
        "the actual immutable layout address and row count are checked before indexing"
    );
    for field in [
        GcSnapshotField::StoredTag,
        GcSnapshotField::StoredScalar,
        GcSnapshotField::StoredRef,
    ] {
        assert!(
            operators[completed..]
                .iter()
                .any(|op| reads_field(op, types.stored, field)),
            "read the completed record's whole value after its subtype guard"
        );
    }
}

#[test]
fn named_data_hits_load_observed_values_before_materializing_the_key() {
    for key in ["push", "join", "values", "constructor"] {
        with_bodies(
            &format!("function read(value){{return value['{key}'];}} read([]);"),
            "read",
            |variant, operators, types, helpers| {
                let ranges = guards(operators, types.intrinsic);
                assert_eq!(ranges.len(), 1, "{key} in {variant}");
                let guard = &ranges[0];
                check_guard(&operators[guard.clone()], types);
                let fallback = (guard.end..operators.len())
                    .find(|&i| matches!(operators[i], Operator::If { .. }))
                    .unwrap();
                let end = matching_end(operators, fallback);
                let resolve = helpers["helper::pooled_string_resolve"];
                let pooled_calls = operators
                    .iter()
                    .enumerate()
                    .filter_map(|(i, op)| {
                        matches!(op, Operator::Call { function_index } if *function_index == resolve)
                            .then_some(i)
                    })
                    .map(|index| {
                        let sequence = &operators[index.saturating_sub(3)..=index];
                        let [Operator::I32Const { value }, Operator::LocalSet { local_index: written },
                            Operator::LocalGet { local_index: read }, Operator::Call { .. }] = sequence
                        else {
                            panic!("{key} in {variant}: unresolved pooled ordinal at {index}: {sequence:?}");
                        };
                        assert_eq!(written, read, "{key} in {variant}: pooled ordinal at {index}");
                        (index, *value)
                    })
                    .collect::<Vec<_>>();
                // This source read's miss starts with its static key operand.
                // Other pooled strings, including receiver-binding names and
                // diagnostics, may legitimately be resolved before admission.
                let key_ordinal = pooled_calls
                    .iter()
                    .find_map(|(index, ordinal)| (fallback + 1..end).contains(index).then_some(*ordinal))
                    .unwrap_or_else(|| panic!("{key} in {variant}: miss {fallback}..{end} has no key resolution; pooled calls {pooled_calls:?}"));
                assert!(
                    pooled_calls.iter().all(|(index, ordinal)| *ordinal != key_ordinal || (fallback + 1..end).contains(index)),
                    "{key} in {variant}: key ordinal {key_ordinal} must resolve only in miss {fallback}..{end}; pooled calls {pooled_calls:?}"
                );
                assert!(
                    ["helper::cached_property_read", "helper::object_read"]
                        .iter()
                        .any(|name| helpers.get(*name).is_some_and(|callee| {
                            operators[fallback + 1..end].iter().any(|op|
                    matches!(op, Operator::Call { function_index } if function_index == callee))
                        })),
                    "the same source read retains its full ordinary miss kernel"
                );
            },
        );
    }
}

#[test]
fn mixed_join_keeps_source_calls_and_uses_the_guarded_reads() {
    with_bodies(
        include_str!(
            "../../../scriptw-cli/tests/fixtures/v8_execution_hotpaths/join-mixed-1024.js"
        ),
        "workload",
        |variant, operators, types, helpers| {
            let ranges = guards(operators, types.intrinsic);
            assert_eq!(
                ranges.len(),
                2,
                "both push and join use the source read mechanism in {variant}"
            );
            for range in ranges {
                check_guard(&operators[range], types);
            }
            let append = helpers["helper::array_packed_push1"];
            assert!(operators.iter().any(|op| matches!(op, Operator::Call { function_index } if *function_index == append)),
            "the existing native append call remains after callee capture");
        },
    );
}

#[test]
fn dynamic_keys_known_objects_and_exotic_keys_keep_their_existing_reads() {
    for source in [
        "function read(value,key){return value[key];} read([],globalThis.key);",
        "function read(){var value={push:7};return value.push;} read();",
        "function read(value){return value.length;} read([]);",
        "function read(value){return value[0];} read([]);",
        "function read(value){return value.notAnIntrinsic;} read([]);",
    ] {
        with_bodies(source, "read", |variant, operators, types, _| {
            assert!(
                guards(operators, types.intrinsic).is_empty(),
                "no unrelated intrinsic admission in {variant}: {source}"
            );
        });
    }
}
