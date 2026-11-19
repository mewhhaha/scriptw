use scriptw_aot_wasm::emit;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower, StatementIr};

#[test]
fn complete_async_iterator_labels_use_their_original_continuation_owner() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let source = r#"
                async function keys(input) {
                    outer: inner: for (const key in await input) {
                        await 0; break outer;
                    }
                }
                async function values(input) {
                    outer: inner: for (const value of await input) {
                        await 0; continue outer;
                    }
                }
                async function awaited(input) {
                    outer: inner: for await (const value of input) {
                        await 0; break outer;
                    }
                }
                keys({a: 1}); values([1]); awaited([1]);
            "#;
            let parsed = parse(source, ParseOptions::script()).expect("labelled iterator source");
            let program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let artifact =
                emit(&program).expect("complete iterator owns its labelled continuation");
            wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
                .validate_all(&artifact.bytes)
                .expect("labelled break and continue keep valid iterator destinations");
        })
        .expect("compiler worker")
        .join()
        .expect("checked iterator labels emit without a compiler panic");
}

#[test]
fn a_missing_label_owner_cannot_silently_emit_bypassed_await_states() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            for source in [
                "async function run() { outer: { break outer; while (await true) {} } await 0; } run();",
                "async function run() { outer: { break outer; await 0; } await 0; } run();",
            ] {
            let parsed = parse(source, ParseOptions::script())
            .expect("valid early labelled break source");
            let mut program = lower(&parsed);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let function = program.script.as_mut().expect("script").functions.iter_mut()
                .find(|function| function.name == "run").expect("run function");
            let async_plan = function.body.statements.iter_mut().find_map(|statement| match statement {
                StatementIr::Labelled { async_plan, .. } => Some(async_plan),
                _ => None,
            }).expect("labelled region");
            assert!(async_plan.take().is_some(), "lowering must publish the owner");
            let error = emit(&program).expect_err("missing owner must refuse emission");
            assert!(error.to_string().contains("labelled async region is missing its continuation owner"), "{error}");
            }
        })
        .expect("compiler worker")
        .join()
        .expect("invalid owner refuses without a compiler panic");
}
