//! Encoded controls distinguish argument snapshots from dispatch and source bodies.
use super::*;
use crate::gc_types::{GcNullability, GlobalLedger, RuntimeModuleTypes, StoredValue, ValueArray};

#[derive(Debug, Default)]
struct Traffic {
    stored_values: usize,
    argument_arrays: usize,
    dispatch_calls: usize,
}

fn caller_traffic(arity: usize) -> Traffic {
    let arguments = vec!["input"; arity].join(",");
    let artifact = emit_script(&format!(
        "function caller(input) {{ globalThis.consume({arguments}); return 0; }} caller(globalThis.input);"
    )).expect("arity control emits");
    expect_valid_module(&artifact, 1);
    let runtime = artifact.runtime().expect("ordinary calls link R");
    let dispatch = function_names_in_bytes(runtime.bytes())
        .into_iter()
        .find_map(|(index, name)| (name == "helper::proxy_call").then_some(index))
        .unwrap();
    let mut export_name = None;
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                let export = export.unwrap();
                if export.kind == wasmparser::ExternalKind::Func && export.index == dispatch {
                    export_name = Some(export.name.to_owned());
                }
            }
        }
    }
    let export_name = export_name.expect("R exports its actual dispatch function");
    let mut imported = 0;
    let mut dispatch_import = None;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(imports) = payload.unwrap() {
            for import in imports.into_imports() {
                let import = import.unwrap();
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE
                        && import.name == export_name
                    {
                        dispatch_import = Some(imported);
                    }
                    imported += 1;
                }
            }
        }
    }
    let dispatch_import = dispatch_import.expect("P imports the actual runtime dispatch");
    let caller = function_names(&artifact)
        .into_iter()
        .find_map(|(index, name)| name.starts_with("js::caller#").then_some(index))
        .unwrap();
    let registered = RuntimeModuleTypes::register(crate::tests::standalone_kind())
        .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let wasm_encoder::HeapType::Concrete(stored) = schema
        .reference_type::<StoredValue>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("StoredValue must name a concrete registered GC type");
    };
    let wasm_encoder::HeapType::Concrete(values) = schema
        .reference_type::<ValueArray>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("argument List must name a concrete registered GC type");
    };
    let mut index = imported;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if index == caller {
                let mut traffic = Traffic::default();
                for operator in body.get_operators_reader().unwrap() {
                    match operator.unwrap() {
                        Operator::StructNew { struct_type_index } => {
                            traffic.stored_values += usize::from(struct_type_index == stored)
                        }
                        Operator::ArrayNewFixed {
                            array_type_index, ..
                        } => traffic.argument_arrays += usize::from(array_type_index == values),
                        Operator::Call { function_index }
                        | Operator::ReturnCall { function_index } => {
                            traffic.dispatch_calls += usize::from(function_index == dispatch_import)
                        }
                        _ => {}
                    }
                }
                return traffic;
            }
            index += 1;
        }
    }
    panic!("caller source body is absent");
}

#[test]
fn fixed_arities_have_bounded_snapshot_cost_and_keep_the_actual_dispatch_call() {
    let zero = caller_traffic(0);
    assert_eq!(zero.dispatch_calls, 1);
    for arity in [1, 2, 4, 8, 16] {
        let traffic = caller_traffic(arity);
        assert_eq!(
            traffic.dispatch_calls, 1,
            "unknown callable must still execute: {traffic:?}"
        );
        assert!(
            traffic.stored_values <= zero.stored_values + arity,
            "at most one immutable snapshot per fixed argument: {zero:?} -> {traffic:?}"
        );
        assert!(
            traffic.argument_arrays <= zero.argument_arrays + 1,
            "one final private List, not an Array per argument: {zero:?} -> {traffic:?}"
        );
    }
}

#[test]
fn runtime_source_entry_dispatch_remains_a_typed_reference_boundary() {
    let artifact = emit_script("function exact(value) { return value; } exact(7);").unwrap();
    let runtime = artifact.runtime().unwrap();
    let target = function_names_in_bytes(runtime.bytes())
        .into_iter()
        .find_map(|(index, name)| (name == "helper::function_call").then_some(index))
        .unwrap();
    let mut index = imported_function_count_in_bytes(runtime.bytes());
    let mut found = false;
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if index == target {
                found = body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .any(|operator| {
                        matches!(
                            operator.unwrap(),
                            Operator::CallRef { .. } | Operator::ReturnCallRef { .. }
                        )
                    });
                break;
            }
            index += 1;
        }
    }
    assert!(
        found,
        "native inlining cannot be assumed to substitute an arbitrary stored function reference"
    );
}
