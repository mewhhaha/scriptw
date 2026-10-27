//! Inspect the encoded source body, including allocations omitted by the proof.
use super::*;
use crate::gc_types::{
    ArgumentsObject, BindingCell, Environment, GcNullability, GlobalLedger, RuntimeModuleTypes,
};

#[derive(Debug, Default)]
struct ActivationTraffic {
    arguments: usize,
    environments: usize,
    cells: usize,
    metadata_appends: usize,
    depth_guard: bool,
}

fn activation_traffic(source: &str) -> ActivationTraffic {
    let artifact = emit_script(source).expect("activation control emits");
    expect_valid_module(&artifact, 1);
    let runtime = artifact.runtime().expect("source callable links R");
    let append_definition = function_names_in_bytes(runtime.bytes())
        .into_iter()
        .find_map(|(index, name)| (name == "helper::ordinary_property_append").then_some(index))
        .expect("actual runtime append definition");
    let mut append_export = None;
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ExportSection(exports) = payload.expect("R decodes") {
            for export in exports {
                let export = export.expect("R export decodes");
                if export.kind == wasmparser::ExternalKind::Func
                    && export.index == append_definition
                {
                    append_export = Some(export.name.to_owned());
                }
            }
        }
    }
    let append_export = append_export.expect("R exports append");
    let mut append_import = None;
    let mut imported = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(imports) = payload.expect("P decodes") {
            for import in imports.into_imports() {
                let import = import.expect("P import decodes");
                if matches!(
                    import.ty,
                    wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                ) {
                    if import.module == crate::RUNTIME_IMPORT_NAMESPACE
                        && import.name == append_export
                    {
                        append_import = Some(imported);
                    }
                    imported += 1;
                }
            }
        }
    }
    // An unused R import can be absent from the final compacted P.
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Standalone)
        .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let wasm_encoder::HeapType::Concrete(arguments) = schema
        .reference_type::<ArgumentsObject>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("arguments object must name a concrete registered GC type");
    };
    let wasm_encoder::HeapType::Concrete(environment) = schema
        .reference_type::<Environment>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("environment must name a concrete registered GC type");
    };
    let wasm_encoder::HeapType::Concrete(cell) = schema
        .reference_type::<BindingCell>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("binding cell must name a concrete registered GC type");
    };
    let make = function_names(&artifact)
        .into_iter()
        .find_map(|(index, name)| name.starts_with("js::make#").then_some(index))
        .expect("actual source body make");
    let mut index = imported;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        let Payload::CodeSectionEntry(body) = payload.expect("P body decodes") else {
            continue;
        };
        if index == make {
            let mut traffic = ActivationTraffic::default();
            let mut source_limit = false;
            let mut limit_comparison = false;
            let mut global_writes = 0;
            for operator in body.get_operators_reader().expect("body operators open") {
                match operator.expect("body operator decodes") {
                    Operator::StructNew { struct_type_index }
                    | Operator::StructNewDefault { struct_type_index } => {
                        traffic.arguments += usize::from(struct_type_index == arguments);
                        traffic.environments += usize::from(struct_type_index == environment);
                        traffic.cells += usize::from(struct_type_index == cell);
                    }
                    Operator::Call { function_index } => {
                        traffic.metadata_appends +=
                            usize::from(append_import == Some(function_index))
                    }
                    Operator::I32Const { value: 128 } => source_limit = true,
                    Operator::I32GeU => limit_comparison = true,
                    Operator::GlobalSet { .. } => global_writes += 1,
                    _ => {}
                }
            }
            traffic.depth_guard = source_limit && limit_comparison && global_writes >= 2;
            return traffic;
        }
        index += 1;
    }
    panic!("make has no encoded body");
}

#[test]
fn synchronous_leaf_bodies_omit_arguments_and_mapping_only_gc_allocations() {
    for source in [
        "function make(input) { return input; } make(globalThis.input);",
        "function make(input) { return {copy: input, nested: [input, null]}; } make(globalThis.input);",
        "function make(input) { return [input,,input]; } make(globalThis.input);",
        "function make(input, unused, input) { return input; } make(1, 2, 3);",
        "'use strict'; function make(input) { return input; } make(globalThis.input);",
    ] {
        let traffic = activation_traffic(source);
        assert_eq!((traffic.arguments, traffic.environments, traffic.cells), (0, 0, 0), "{source}: {traffic:?}");
        assert!(traffic.depth_guard, "leaf body must retain source admission/epilogue: {source}: {traffic:?}");
    }
    let identity =
        activation_traffic("function make(input) { return input; } make(globalThis.input);");
    assert_eq!(
        identity.metadata_appends, 0,
        "unused arguments metadata remains: {identity:?}"
    );
}

#[test]
fn observers_keep_actual_arguments_allocation_in_the_encoded_body() {
    for source in [
        "function make(input) { return arguments[0]; } make(globalThis.input);",
        "function make(input) { return input(); } make(globalThis.input);",
        "function make(input) { return input.x; } make(globalThis.input);",
        "function make(input) { return +input; } make(globalThis.input);",
        "function make(input) { return () => input; } make(globalThis.input);",
    ] {
        let traffic = activation_traffic(source);
        assert_eq!(
            traffic.arguments, 1,
            "an observer must keep the real object: {source}: {traffic:?}"
        );
        assert!(
            traffic.environments > 0 && traffic.cells > 0,
            "mapped parameters retain their physical cells: {source}: {traffic:?}"
        );
        assert!(
            traffic.metadata_appends >= 3,
            "arguments length/callee/iterator must still be published: {source}: {traffic:?}"
        );
        assert!(traffic.depth_guard, "{source}: {traffic:?}");
    }
}
