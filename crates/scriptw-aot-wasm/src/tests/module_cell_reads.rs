//! Inspect real module bodies: cell loads, cold TDZ calls and dynamic fallback.
use super::*;
use crate::gc_types::{BindingCell, GcNullability, GlobalLedger, RuntimeModuleTypes};
use scriptw_ir::{ExprIr, ProgramIr, StatementIr, TypedExpr, ValueInfo};

fn module(source: &str) -> ProgramIr {
    lower(&parse(source, ParseOptions::module()).expect("module parses"))
}

fn helper_imports(artifact: &WasmArtifact) -> BTreeMap<u32, String> {
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
                            helpers.insert(index, name.clone());
                        }
                    }
                    index += 1;
                }
            }
        }
    }
    helpers
}

#[derive(Debug, Default)]
struct Reads {
    fields: BTreeMap<u32, usize>,
    calls: BTreeMap<String, usize>,
}

impl Reads {
    fn calls(&self, name: &str) -> usize {
        self.calls.get(name).copied().unwrap_or(0)
    }
}

fn inspect(artifact: &WasmArtifact, requested: &str) -> BTreeMap<String, Reads> {
    expect_valid_module(artifact, 1);
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Program(
        artifact.runtime().unwrap(),
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let wasm_encoder::HeapType::Concrete(cell) = schema
        .reference_type::<BindingCell>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("BindingCell has a concrete registered type");
    };
    let helpers = helper_imports(artifact);
    let names = function_names(artifact)
        .into_iter()
        .filter(|(_, name)| name.starts_with(&format!("js::{requested}#")))
        .collect::<BTreeMap<_, _>>();
    assert!(!names.is_empty(), "named source body must be emitted");
    let mut index = imported_function_count(artifact);
    let mut result = BTreeMap::new();
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if let Some(name) = names.get(&index) {
                let mut reads = Reads::default();
                for operator in body.get_operators_reader().unwrap() {
                    match operator.unwrap() {
                        Operator::StructGet {
                            struct_type_index,
                            field_index,
                        } if struct_type_index == cell => {
                            *reads.fields.entry(field_index).or_default() += 1;
                        }
                        Operator::Call { function_index } => {
                            if let Some(name) = helpers.get(&function_index) {
                                *reads.calls.entry(name.clone()).or_default() += 1;
                            }
                        }
                        _ => {}
                    }
                }
                result.insert(name.clone(), reads);
            }
            index += 1;
        }
    }
    assert_eq!(
        result.len(),
        names.len(),
        "inspect every physical source variant"
    );
    result
}

#[test]
fn proven_module_reads_load_cells_without_reference_or_name_lookup() {
    let program = module(
        r#"
        export function diagnostics() { return arguments[0]; }
        export function owned(cell) { function hold() { return cell; } return cell; }
        export function make(cell) { return function captured() { return cell; }; }
    "#,
    );
    let actual = emit(&program).expect("proven cells emit");
    let mut control = program.clone();
    for function in &mut control.script.as_mut().unwrap().functions {
        if !["owned", "captured"].contains(&function.name.as_str()) {
            continue;
        }
        let returned = function
            .body
            .statements
            .iter_mut()
            .find_map(|statement| match statement {
                StatementIr::Return(value) => Some(value),
                _ => None,
            })
            .expect("simple source return remains explicit");
        // Preserve exact cell allocations, captured metadata and prologues.
        *returned = TypedExpr::from_info(ValueInfo::undefined(), ExprIr::Undefined);
    }
    let control = emit(&control).expect("same prologues without the read emit");
    for name in ["owned", "captured"] {
        let controls = inspect(&control, name);
        for (variant, actual) in inspect(&actual, name) {
            let control = &controls[&variant];
            for (field, count) in &control.fields {
                assert!(
                    actual.fields.get(field).copied().unwrap_or(0) >= *count,
                    "{variant}: retained prologue cell reads changed: {actual:?} / {control:?}"
                );
            }
            let added_fields = actual
                .fields
                .iter()
                .filter(|(field, count)| **count > control.fields.get(*field).copied().unwrap_or(0))
                .count();
            assert!(added_fields >= 2,
                "{variant}: direct value/initialization loads require additional cell fields: {actual:?} / {control:?}");
            assert_eq!(
                actual.calls("helper::lexical_binding_uninitialized"),
                control.calls("helper::lexical_binding_uninitialized") + 1
            );
            assert_eq!(
                actual.calls("helper::pooled_string_resolve"),
                control.calls("helper::pooled_string_resolve"),
                "{variant}: no additional source-name resolution"
            );
            assert!(
                !actual
                    .calls
                    .keys()
                    .any(|name| name.starts_with("helper::environment_identifier_")),
                "{variant}: {actual:?}"
            );
        }
    }
}

#[test]
fn proven_typeof_keeps_tdz_while_non_simple_owner_keeps_named_fallback() {
    let artifact = emit(&module(
        r#"
        export function diagnostics() { return arguments[0]; }
        export function typed(cell) { function hold() { return cell; } return typeof cell; }
        export function fallback(cell = 0) { return cell; }
    "#,
    ))
    .expect("typeof and fallback emit");
    for (variant, reads) in inspect(&artifact, "typed") {
        assert!(reads.fields.len() >= 2, "{variant}: {reads:?}");
        assert_eq!(
            reads.calls("helper::lexical_binding_uninitialized"),
            1,
            "{variant}: {reads:?}"
        );
        assert!(
            !reads
                .calls
                .keys()
                .any(|name| name.starts_with("helper::environment_identifier_")),
            "{variant}: {reads:?}"
        );
    }
    for (variant, reads) in inspect(&artifact, "fallback") {
        assert!(
            reads.calls("helper::environment_identifier_read_strict") > 0,
            "{variant}: original resolver remains: {reads:?}"
        );
    }
}
