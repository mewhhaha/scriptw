use std::collections::BTreeSet;
use std::sync::Arc;

use scriptw_aot_wasm::{
    emit, emit_with_rooted_snapshot, GcSnapshotLayout, PromiseRejectionPolicy,
    SNAPSHOT_ENTRY_REALM_EXPORT, SNAPSHOT_REALMS_EXPORT, SNAPSHOT_RESOLVE_PROPERTY_VALUE_EXPORT,
    SNAPSHOT_SYMBOLS_EXPORT,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{CompositeInnerType, ExternalKind, HeapType, Parser, Payload, TypeRef, ValType};

fn exports(
    bytes: &[u8],
) -> (
    Vec<(String, ExternalKind, u32)>,
    Vec<wasmparser::GlobalType>,
) {
    let mut rows = Vec::new();
    let mut globals = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::ExportSection(section) => {
                rows.extend(section.into_iter().map(|row| {
                    let row = row.unwrap();
                    (row.name.to_owned(), row.kind, row.index)
                }));
            }
            Payload::GlobalSection(section) => {
                globals.extend(section.into_iter().map(|global| global.unwrap().ty));
            }
            _ => {}
        }
    }
    (rows, globals)
}

fn exported_function_type(bytes: &[u8], function_index: u32) -> wasmparser::FuncType {
    let mut types = Vec::new();
    let mut functions = Vec::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::TypeSection(section) => {
                for group in section {
                    for subtype in group.unwrap().types().cloned() {
                        types.push(match subtype.composite_type.inner {
                            CompositeInnerType::Func(function) => Some(function),
                            _ => None,
                        });
                    }
                }
            }
            Payload::ImportSection(section) => {
                for import in section.into_imports() {
                    if let TypeRef::Func(index) | TypeRef::FuncExact(index) = import.unwrap().ty {
                        functions.push(index);
                    }
                }
            }
            Payload::FunctionSection(section) => {
                functions.extend(section.into_iter().map(Result::unwrap));
            }
            _ => {}
        }
    }
    types[functions[function_index as usize] as usize]
        .clone()
        .expect("resolver export must have an actual function type")
}

fn snapshot_export_names(rows: &[(String, ExternalKind, u32)]) -> BTreeSet<&str> {
    rows.iter()
        .map(|(name, _, _)| name.as_str())
        .filter(|name| name.starts_with("scriptw_snapshot_"))
        .collect()
}

/// The runtime module declares snapshot witnesses, roots and its resolver for every
/// program; a program module never does. Asking for a snapshot only forces
/// the runtime link of a program that would otherwise be self-contained.
#[test]
fn snapshot_exports_have_actual_gc_roots_witnesses_and_a_typed_resolver() {
    let heap = lower(&parse("({ value: 7 })", ParseOptions::script()).unwrap());
    let normal = emit(&heap).unwrap();
    let graph = emit_with_rooted_snapshot(
        &heap,
        PromiseRejectionPolicy::default(),
        &scriptw_intl::IntlCompilationProfile::Minimal,
    )
    .unwrap();
    let runtime = normal.runtime().expect("heap program links R");
    assert!(Arc::ptr_eq(
        runtime,
        graph.runtime().expect("snapshot links R")
    ));
    assert_eq!(
        normal.bytes, graph.bytes,
        "a heap program's snapshot is the same program module"
    );

    let (program_exports, _) = exports(&normal.bytes);
    assert!(snapshot_export_names(&program_exports).is_empty());

    let (runtime_exports, runtime_globals) = exports(runtime.bytes());
    let expected = GcSnapshotLayout::ALL
        .iter()
        .map(|layout| layout.witness_export())
        .chain([
            SNAPSHOT_ENTRY_REALM_EXPORT,
            SNAPSHOT_REALMS_EXPORT,
            SNAPSHOT_SYMBOLS_EXPORT,
            SNAPSHOT_RESOLVE_PROPERTY_VALUE_EXPORT,
        ])
        .collect::<BTreeSet<_>>();
    assert_eq!(
        snapshot_export_names(&runtime_exports),
        expected,
        "R exports three roots, declaration-derived witnesses and the value resolver"
    );
    for name in expected {
        assert_eq!(
            runtime_exports
                .iter()
                .filter(|(export, _, _)| export == name)
                .count(),
            1
        );
        let (_, kind, index) = runtime_exports
            .iter()
            .find(|(export, _, _)| export == name)
            .unwrap();
        if name == SNAPSHOT_RESOLVE_PROPERTY_VALUE_EXPORT {
            assert_eq!(*kind, ExternalKind::Func, "{name}");
            let signature = exported_function_type(runtime.bytes(), *index);
            assert_eq!(signature.params().len(), 1, "one property value input");
            assert_eq!(
                signature.results().len(),
                1,
                "one immutable snapshot result"
            );
            for (actual, layout) in [
                (signature.params()[0], GcSnapshotLayout::PropertyValue),
                (signature.results()[0], GcSnapshotLayout::Stored),
            ] {
                let (_, kind, index) = runtime_exports
                    .iter()
                    .find(|(name, _, _)| name == layout.witness_export())
                    .unwrap();
                assert_eq!(*kind, ExternalKind::Global);
                let ValType::Ref(witness) = runtime_globals[*index as usize].content_type else {
                    panic!("the published witness must be a GC reference");
                };
                let ValType::Ref(actual) = actual else {
                    panic!("the resolver boundary must use concrete GC references");
                };
                assert!(!actual.is_nullable(), "resolver values are non-null");
                assert!(matches!(actual.heap_type(), HeapType::Concrete(_)));
                assert_eq!(
                    actual.heap_type(),
                    witness.heap_type(),
                    "resolver signature must use the exact published {layout:?} type"
                );
            }
            continue;
        }
        assert_eq!(*kind, ExternalKind::Global, "{name}");
        let global = &runtime_globals[*index as usize];
        let ValType::Ref(reference) = global.content_type else {
            panic!("{name} must expose its actual typed GC reference");
        };
        assert!(reference.is_nullable(), "{name}");
        assert!(
            matches!(reference.heap_type(), HeapType::Concrete(_)),
            "{name}"
        );
        assert_eq!(
            global.mutable,
            !GcSnapshotLayout::ALL
                .iter()
                .any(|layout| layout.witness_export() == name),
            "type witnesses are immutable; retained execution roots are mutable"
        );
    }

    let free = lower(&parse("1;", ParseOptions::script()).unwrap());
    let plain = emit(&free).unwrap();
    assert!(
        plain.runtime().is_none(),
        "runtime-free IR stays self-contained"
    );
    assert!(snapshot_export_names(&exports(&plain.bytes).0).is_empty());
    let observed = emit_with_rooted_snapshot(
        &free,
        PromiseRejectionPolicy::default(),
        &scriptw_intl::IntlCompilationProfile::Minimal,
    )
    .unwrap();
    assert!(Arc::ptr_eq(
        runtime,
        observed.runtime().expect("a snapshot links R")
    ));
    assert!(snapshot_export_names(&exports(&observed.bytes).0).is_empty());
}
