use std::collections::BTreeMap;

use scriptw_aot_wasm::{emit, RUNTIME_IMPORT_NAMESPACE};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{
    ExternalKind, KnownCustom, Name, Operator, Parser, Payload, TypeRef, Validator, WasmFeatures,
};

#[derive(Default)]
struct Calls {
    normal: Vec<u32>,
    tail: Vec<u32>,
    tail_references: Vec<u32>,
}

#[derive(Default)]
struct Module {
    imports: BTreeMap<u32, (String, String)>,
    exports: BTreeMap<String, u32>,
    names: BTreeMap<u32, String>,
    bodies: BTreeMap<u32, Calls>,
}

impl Module {
    fn read(bytes: &[u8]) -> Self {
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
            .validate_all(bytes)
            .expect("complete module validates");
        let mut module = Self::default();
        let mut next_function = 0;
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.expect("module decodes") {
                Payload::ImportSection(reader) => {
                    for import in reader.into_imports() {
                        let import = import.expect("import decodes");
                        if matches!(import.ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                            module.imports.insert(
                                next_function,
                                (import.module.to_owned(), import.name.to_owned()),
                            );
                            next_function += 1;
                        }
                    }
                }
                Payload::ExportSection(reader) => {
                    for export in reader {
                        let export = export.expect("export decodes");
                        if export.kind == ExternalKind::Func {
                            module.exports.insert(export.name.to_owned(), export.index);
                        }
                    }
                }
                Payload::CodeSectionEntry(body) => {
                    let mut calls = Calls::default();
                    for operator in body.get_operators_reader().expect("body opens") {
                        match operator.expect("operator decodes") {
                            Operator::Call { function_index } => calls.normal.push(function_index),
                            Operator::ReturnCall { function_index } => {
                                calls.tail.push(function_index)
                            }
                            Operator::ReturnCallRef { type_index } => {
                                calls.tail_references.push(type_index)
                            }
                            _ => {}
                        }
                    }
                    module.bodies.insert(next_function, calls);
                    next_function += 1;
                }
                Payload::CustomSection(section) => {
                    if let KnownCustom::Name(subsections) = section.as_known() {
                        for subsection in subsections {
                            if let Name::Function(map) = subsection.expect("name section decodes") {
                                for naming in map {
                                    let naming = naming.expect("function name decodes");
                                    assert!(module
                                        .names
                                        .insert(naming.index, naming.name.to_owned())
                                        .is_none());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        module
    }

    fn named_body(&self, prefix: &str) -> (u32, &Calls) {
        let indices = self
            .names
            .iter()
            .filter_map(|(&index, name)| {
                let matched = if prefix.ends_with('#') {
                    assert!(
                        prefix.starts_with("js::"),
                        "only an explicit source function prefix may end in '#': {prefix}"
                    );
                    name.starts_with(prefix)
                } else {
                    name.as_str() == prefix
                };
                matched.then_some(index)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            indices.len(),
            1,
            "one encoded function matches {prefix}; matching names: {:?}",
            indices
                .iter()
                .map(|index| (index, &self.names[index]))
                .collect::<Vec<_>>()
        );
        (
            indices[0],
            self.bodies
                .get(&indices[0])
                .expect("name identifies an actual code body"),
        )
    }

    fn imported_runtime_function(&self, runtime: &Self, index: u32) -> u32 {
        let imports = self
            .imports
            .iter()
            .filter_map(|(&import_index, (namespace, name))| {
                (namespace == RUNTIME_IMPORT_NAMESPACE && runtime.exports.get(name) == Some(&index))
                    .then_some(import_index)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            imports.len(),
            1,
            "one actual R export supplies this P helper import"
        );
        imports[0]
    }
}

#[test]
fn proper_tail_calls_forward_actual_completion_bodies_and_retain_required_continuations() {
    std::thread::Builder::new().name("proper-tail-call-emission".to_owned())
        .stack_size(64 * 1024 * 1024).spawn(|| {
            // Unobserved parameters retain one unspecialized body per source function.
            let source = r#"
function tail(target, value) { "use strict"; return target(value); }
function method(receiver, value) { "use strict"; return receiver.step(value); }
function optional(receiver, value) { "use strict"; return receiver.step?.(value); }
function conditional(target, yes, value) { "use strict"; return yes ? target(value) : value; }
function logical(target, yes, value) { "use strict"; return yes && target(value); }
function comma(target, value) { "use strict"; return (value, target(value)); }
function sloppy(target, value) { return target(value); }
function caught(target, value) { "use strict"; try { return target(value); } catch (error) { return error; } }
function finalized(target, value) { "use strict"; try { return target(value); } finally { value++; } }
function disposed(target, resource, value) { "use strict"; using item = resource; return target(value); }
function* generator(target, value) { "use strict"; return target(value); }
async function asynchronous(target, value) { "use strict"; return target(value); }
[tail, method, optional, conditional, logical, comma, sloppy, caught, finalized, disposed, generator, asynchronous];
"#;
            let parsed = parse(source, ParseOptions::script()).expect("tail fixture parses");
            let artifact = emit(&lower(&parsed)).expect("tail fixture emits actual Wasm");
            let program = Module::read(&artifact.bytes);
            let runtime = Module::read(artifact.runtime().expect("tail calls link the runtime").bytes());
            let (proxy_index, proxy) = runtime.named_body("helper::proxy_call");
            let (function_index, function) = runtime.named_body("helper::function_call");
            let imported_proxy = program.imported_runtime_function(&runtime, proxy_index);
            for name in ["tail", "method", "optional", "conditional", "logical", "comma"] {
                let (_, body) = program.named_body(&format!("js::{name}#"));
                assert_eq!(body.tail, [imported_proxy], "{name} must tail-call the linked whole-Completion dispatcher exactly once");
            }
            for name in ["sloppy", "caught", "finalized", "disposed", "generator", "asynchronous"] {
                let (_, body) = program.named_body(&format!("js::{name}#"));
                assert!(body.tail.is_empty() && body.tail_references.is_empty(), "{name} must retain its continuation");
                assert!(body.normal.contains(&imported_proxy), "{name} still performs the real Call");
            }
            assert!(proxy.tail.contains(&function_index), "ordinary targets use raw FunctionCall tail forwarding");
            assert!(proxy.tail.contains(&proxy_index), "a Proxy apply trap tail-forwards its actual Call");
            assert!(function.tail.contains(&proxy_index), "bound Proxy targets tail-forward through ProxyCall");
            assert_eq!(function.tail_references.len(), 1, "the ordinary body is entered by typed ReturnCallRef");
            for name in ["builtin::Function.prototype.call", "builtin::Function.prototype.apply", "builtin::Reflect.apply"] {
                let (_, body) = runtime.named_body(name);
                assert!(body.tail.contains(&proxy_index), "{name} implements PrepareForTailCall with the real dispatcher");
            }
        }).expect("compiler worker starts").join().expect("tail emission does not panic");
}
