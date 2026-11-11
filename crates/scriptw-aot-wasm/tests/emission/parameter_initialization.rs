//! Completed ordinary parameters omit body TDZ branches; default failures have execution coverage.
use crate::property_read_plans::Module;
use scriptw_aot_wasm::emit;
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::lower;
use wasmparser::{Operator, Validator, WasmFeatures};

#[test]
fn completed_parameter_locals_omit_body_checks() {
    let artifact = std::thread::Builder::new()
        .name("parameter-initialization-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let parsed = parse(
                r#"
export function plain(a,b,c,d,e) { return [a,a,b,b,c,c,d,d,e,e]; }
export function earlier(a=1,b=a) { return [a,a,b,b]; }
export function captured(a) { return function readCaptured() { return a; }; }
export function lexical(a) { let hidden=a; return hidden; }
export function* resumed(a) { yield a; return a; }
"#,
                ParseOptions::module(),
            )
            .unwrap();
            let program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            emit(&program).unwrap()
        })
        .unwrap()
        .join()
        .unwrap();
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(&artifact.bytes)
        .unwrap();
    let runtime = Module::read(artifact.runtime().unwrap().bytes(), false);
    let program = Module::read(&artifact.bytes, true);
    let (definition, _) = runtime.named("helper::lexical_binding_uninitialized");
    let failure = program.imported_runtime_function(&runtime, definition);
    let calls = |name, callee| {
        program
            .named(name)
            .1
            .iter()
            .filter(
                |op| matches!(op, Operator::Call { function_index } if *function_index == callee),
            )
            .count()
    };
    assert_eq!(
        calls("js::plain#", failure),
        0,
        "initialized parameters have no body TDZ branch"
    );
    assert_eq!(
        calls("js::earlier#", failure),
        0,
        "the frontend proves the earlier default ready; body parameters are sealed"
    );
}
