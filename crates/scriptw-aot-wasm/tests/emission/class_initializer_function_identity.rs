use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use std::collections::BTreeSet;
use wasmparser::{Validator, WasmFeatures};

// Exact JavaScript witnesses from aot_class_initializer_grammar_context;
// this layer catches identity/index defects without native compilation.
const SOURCES: [&str; 2] = [
    r#"
var await = 7;
async function make() {
  class C {
    value = await;
    #private = await;
    static value = await;
    static #staticPrivate = await;
    accessor item = await;
    accessor #item = await;
    static accessor other = await;
    static accessor #other = await;
    read() { return this.#private + ':' + this.#item; }
    static read() { return this.#staticPrivate + ':' + this.#other; }
  }
  return await Promise.resolve(C);
}
var ready = make();
await = 9;
ready.then(C => {
  var value = new C();
  print(C.value + ':' + C.other + ':' + C.read());
  print(value.value + ':' + value.item + ':' + value.read());
});
"#,
    r#"
var await = 11;
function* make() {
  return class {
    [yield 'key'] = await;
    nested = async () => await Promise.resolve(13);
    generator = function* () { yield 17; };
  };
}
var iterator = make();
print(iterator.next().value);
var C = iterator.next('value').value;
var value = new C();
print(value.value + ':' + value.generator().next().value);
async function run() {
  class D { [await Promise.resolve('value')] = 19; }
  print((await value.nested()) + ':' + new D().value);
}
run();
"#,
];

#[test]
fn class_initializers_in_resumable_owners_emit_one_body_per_function_id() {
    std::thread::Builder::new()
        .name("class-initializer-function-identity".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            for source in SOURCES {
                let parsed = parse(source, ParseOptions::script()).expect("native witness parses");
                let program = lower(&parsed);
                assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
                let script = program.script.as_ref().expect("script IR");
                let mut ids = BTreeSet::new();
                for function in &script.functions {
                    assert!(
                        ids.insert(&function.id),
                        "one body per source callable: {}",
                        function.id
                    );
                }
                let artifact = scriptw_aot_wasm::emit(&program)
                    .expect("class execution identities retain their planned entries");
                Validator::new_with_features(
                    WasmFeatures::default()
                        | WasmFeatures::THREADS
                        | WasmFeatures::MULTI_MEMORY
                        | WasmFeatures::FUNCTION_REFERENCES
                        | WasmFeatures::GC
                        | WasmFeatures::EXCEPTIONS
                        | WasmFeatures::TAIL_CALL,
                )
                .validate_all(&artifact.bytes)
                .expect("every body uses its declared entry signature and index");
                let emitted = artifact
                    .function_sizes
                    .iter()
                    .filter(|body| body.category == "script")
                    .collect::<Vec<_>>();
                // Emission also appends the empty Generator, Async and
                // AsyncGenerator constructor bodies to the Script category.
                // Every artifact entry still owns a distinct index, and each
                // original source identity below must have exactly one body.
                let indices = artifact
                    .function_sizes
                    .iter()
                    .map(|body| body.wasm_index)
                    .collect::<BTreeSet<_>>();
                assert_eq!(indices.len(), artifact.function_sizes.len());
                for function in &script.functions {
                    let name = if function.name.is_empty() {
                        format!("js::{}", function.id)
                    } else {
                        format!("js::{}#{}", function.name, function.id)
                    };
                    assert_eq!(emitted.iter().filter(|body| body.name == name).count(), 1);
                }
            }
        })
        .expect("bounded compiler worker starts")
        .join()
        .expect("class initializer emission completes");
}
