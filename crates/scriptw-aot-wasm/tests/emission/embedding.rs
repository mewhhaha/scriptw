//! Validate the real Portable bridge contract against its canonical GC types.
use scriptw_aot_wasm::{
    emit, EMBEDDING_BYTES, EMBEDDING_BYTES_READ, EMBEDDING_CALL, EMBEDDING_CANCEL,
    EMBEDDING_CHECKPOINT, EMBEDDING_EXPORT, EMBEDDING_PROMISE, EMBEDDING_READ, EMBEDDING_STRING,
};
use scriptw_ir::{lower_module_graph, ModuleGraphSources, ModuleKey, ModuleSourceIr};
use std::collections::BTreeMap;
use wasmparser::{ExternalKind, Parser, Payload, TypeRef, Validator, WasmFeatures};

fn function_signatures(bytes: &[u8]) -> BTreeMap<String, (usize, usize)> {
    let mut types = Vec::new();
    let mut functions = Vec::new();
    let mut exports = BTreeMap::new();
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::TypeSection(reader) => {
                for group in reader {
                    types.extend(group.unwrap().into_types().map(|ty| {
                        if let wasmparser::CompositeInnerType::Func(function) =
                            ty.composite_type.inner
                        {
                            Some((function.params().len(), function.results().len()))
                        } else {
                            None
                        }
                    }));
                }
            }
            Payload::ImportSection(reader) => {
                for import in reader.into_imports() {
                    if let TypeRef::Func(index) | TypeRef::FuncExact(index) = import.unwrap().ty {
                        functions.push(index);
                    }
                }
            }
            Payload::FunctionSection(reader) => {
                functions.extend(reader.into_iter().map(Result::unwrap))
            }
            Payload::ExportSection(reader) => {
                for export in reader {
                    let export = export.unwrap();
                    if export.kind == ExternalKind::Func {
                        exports.insert(export.name.to_owned(), export.index);
                    }
                }
            }
            _ => {}
        }
    }
    exports
        .into_iter()
        .map(|(name, function)| {
            (
                name,
                types[functions[function as usize] as usize].expect("function signature"),
            )
        })
        .collect()
}

#[test]
fn portable_retained_embedding_bridges_use_real_gc_calls_and_complete_results() {
    std::thread::Builder::new().name("embedding-emission".into())
        .stack_size(64 * 1024 * 1024).spawn(|| {
            let sources = ModuleGraphSources {
                realm_requests: Default::default(), entry: 0,
                modules: vec![ModuleSourceIr::new(
                    ModuleKey::from_host("entry"),
                    "export let count = 0; export const callable = new Proxy(function(n) { return count += n; }, {}); export default { fetch() { return count; } };".into(),
                    "entry".into(),
                )], resolutions: Vec::new(),
            };
            let program = lower_module_graph(&sources);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let artifact = emit(&program).expect("retained Module entry emits");
            let runtime = artifact.runtime().expect("GC runtime");
            for bytes in [artifact.bytes.as_slice(), runtime.bytes()] {
                Validator::new_with_features(WasmFeatures::all()).validate_all(bytes)
                    .expect("every emitted bridge validates against its declared result type");
            }
            let p = function_signatures(&artifact.bytes);
            let r = function_signatures(runtime.bytes());
            assert_eq!(p.get(EMBEDDING_EXPORT), Some(&(3, 5)));
            assert_eq!(p.get(EMBEDDING_CHECKPOINT), Some(&(2, 5)));
            for (name, signature) in [
                (EMBEDDING_CALL, (10, 5)), (EMBEDDING_READ, (7, 5)),
                (EMBEDDING_STRING, (1, 5)), (EMBEDDING_BYTES, (2, 5)),
                (EMBEDDING_BYTES_READ, (3, 1)), (EMBEDDING_PROMISE, (3, 6)),
                (EMBEDDING_CANCEL, (2, 5)),
            ] {
                assert_eq!(p.get(name), Some(&signature), "P {name}");
                assert_eq!(r.get(name), Some(&signature), "R {name}");
            }
        }).unwrap().join().unwrap();
}
