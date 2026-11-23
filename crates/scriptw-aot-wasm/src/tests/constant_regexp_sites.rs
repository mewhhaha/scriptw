//! Constant RegExp construction sites and module `Math.imul` reach their
//! guarded compile-time routes in real emitted bodies, and keep the original
//! dispatch for every other shape.
use super::*;

fn runtime_helper_imports(artifact: &WasmArtifact) -> BTreeMap<u32, String> {
    let runtime = artifact.runtime().expect("heap program links R");
    let names = function_names_in_bytes(runtime.bytes());
    let mut exports = BTreeMap::new();
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ExportSection(section) = payload.expect("R decodes") {
            for export in section {
                let export = export.expect("R export decodes");
                if export.kind == wasmparser::ExternalKind::Func {
                    if let Some(name) = names.get(&export.index) {
                        exports.insert(export.name.to_owned(), name.clone());
                    }
                }
            }
        }
    }
    let mut helpers = BTreeMap::new();
    let mut index = 0;
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::ImportSection(section) = payload.expect("P decodes") {
            for import in section.into_imports() {
                let import = import.expect("P import decodes");
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

/// Every physical body emitted for source function `requested`.
fn source_bodies<'a>(artifact: &'a WasmArtifact, requested: &str) -> Vec<Vec<Operator<'a>>> {
    let names = function_names(artifact)
        .into_iter()
        .filter(|(_, name)| name.starts_with(&format!("js::{requested}#")))
        .collect::<BTreeMap<_, _>>();
    assert!(!names.is_empty(), "js::{requested} must be emitted");
    let mut index = imported_function_count(artifact);
    let mut bodies = Vec::new();
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        if let Payload::CodeSectionEntry(body) = payload.expect("P decodes") {
            if names.contains_key(&index) {
                bodies.push(
                    body.get_operators_reader()
                        .expect("body opens")
                        .into_iter()
                        .map(|operator| operator.expect("operator decodes"))
                        .collect(),
                );
            }
            index += 1;
        }
    }
    bodies
}

fn helper_calls(artifact: &WasmArtifact, requested: &str, helper: &str) -> usize {
    let helpers = runtime_helper_imports(artifact);
    source_bodies(artifact, requested)
        .iter()
        .flatten()
        .filter(|operator| {
            matches!(operator, Operator::Call { function_index }
                if helpers.get(function_index).map(String::as_str) == Some(helper))
        })
        .count()
}

#[test]
fn constant_regexp_call_and_construct_sites_bind_the_compiled_program() {
    const ALLOCATE: &str = "helper::regexp_literal_allocate";
    for (source, bound) in [
        (
            r#"function make() { return RegExp("^[a-z]+$", "g"); } make;"#,
            true,
        ),
        (
            r#"function make() { return new RegExp("[0-9]+"); } make;"#,
            true,
        ),
        (r#"function make() { return RegExp(); } make;"#, true),
        // Not constant: the native constructor and its candidate table decide.
        (
            r#"function make(source) { return RegExp(source); } make;"#,
            false,
        ),
        // A third operand is an arbitrary expression with its own effects.
        (
            r#"function make() { return RegExp("a", "g", 0); } make;"#,
            false,
        ),
        // Illegal source: the SyntaxError stays with the native constructor.
        (
            r#"function make() { return new RegExp("("); } make;"#,
            false,
        ),
        // Illegal flags likewise.
        (
            r#"function make() { return RegExp("a", "gg"); } make;"#,
            false,
        ),
    ] {
        let artifact = emit_script(source).expect("RegExp site emits");
        expect_valid_module(&artifact, 1);
        assert_eq!(
            helper_calls(&artifact, "make", ALLOCATE) > 0,
            bound,
            "{source}"
        );
    }
}

#[test]
fn module_math_imul_reaches_the_guarded_numeric_route() {
    let program = lower(
        &parse(
            // Not in return position: strict module code makes a returned
            // call a tail call, which the numeric route (Continue only)
            // never takes. This is the retained `integer.js` shape.
            "export function run(a, b) { return (Math.imul(a, b) + 1) >>> 0; }\n\
             export function other(a, b) { return (Math.max(a, b) + 1) >>> 0; }",
            ParseOptions::module(),
        )
        .expect("module parses"),
    );
    let artifact = emit(&program).expect("module emits");
    expect_valid_module(&artifact, 1);
    let multiplies = |name| {
        source_bodies(&artifact, name).iter().any(|body| {
            body.windows(7).any(|ops| {
                matches!(
                    ops,
                    [
                        Operator::LocalGet { .. },
                        Operator::I32WrapI64,
                        Operator::LocalGet { .. },
                        Operator::I32WrapI64,
                        Operator::I32Mul,
                        Operator::F64ConvertI32S,
                        Operator::I64ReinterpretF64
                    ]
                )
            })
        })
    };
    assert!(
        multiplies("run"),
        "module Math.imul keeps its guarded route"
    );
    assert!(!multiplies("other"), "only an `imul` call nominates it");
}
