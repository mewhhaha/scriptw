use scriptw_aot_wasm::emit;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{Validator, WasmFeatures};

use crate::linked_bodies;

fn match_caller_bytes(call: &str, repetitions: usize) -> u32 {
    let call = call.to_string();
    std::thread::Builder::new()
        .name(format!("string-match-caller-{repetitions}"))
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let source = format!(
                "function callMatches(receiver, pattern) {{ {} return 1; }} \
                 callMatches('abcdef', /(?<=(?<letter>\\w){{3}})f/u);",
                call.repeat(repetitions)
            );
            let parsed = parse(&source, ParseOptions::script()).expect("match calls parse");
            let lowered = lower(&parsed);
            assert!(lowered.is_wasm_supported(), "{:?}", lowered.diagnostics);
            let artifact = emit(&lowered).expect("match calls emit real Wasm");
            let features = WasmFeatures::default()
                | WasmFeatures::THREADS
                | WasmFeatures::FUNCTION_REFERENCES
                | WasmFeatures::GC
                | WasmFeatures::EXCEPTIONS;
            for bytes in [
                &artifact.runtime().expect("match calls link R").bytes()[..],
                artifact.bytes.as_slice(),
            ] {
                Validator::new_with_features(features)
                    .validate_all(bytes)
                    .expect("shared match calls validate for the native backend");
            }
            // The intrinsic is R's one body; the caller never copies it.
            let bodies = linked_bodies::linked_bodies(&artifact);
            let intrinsic = bodies.unique("builtin::String.prototype.match");
            assert!(
                bodies
                    .runtime
                    .iter()
                    .any(|body| body.index == intrinsic.index),
                "the match intrinsic is owned by the runtime module"
            );
            assert!(
                intrinsic.bytes() > 1024,
                "the callable intrinsic must retain its implementation"
            );
            artifact
                .function_sizes
                .iter()
                .filter(|body| body.name.starts_with("js::callMatches#"))
                .map(|body| body.body_bytes.bytes())
                .max()
                .expect("the match caller must be emitted")
        })
        .expect("compiler worker starts")
        .join()
        .expect("match emission does not panic")
}

#[test]
fn repeated_match_calls_do_not_copy_regexp_fallback_construction() {
    let call = "receiver.match(pattern);\n";
    let single = match_caller_bytes(call, 1);
    let repeated = match_caller_bytes(call, 17);
    let growth = repeated
        .checked_sub(single)
        .expect("calls remain observable");
    assert!(
        growth < 16 * 4096,
        "sixteen method calls added {growth} bytes ({single} -> {repeated})"
    );
}

#[test]
fn repeated_literal_lookbehind_matches_have_bounded_caller_growth() {
    let call = "'abcdef'.match(/(?<=(?<letter>\\w){3})f/u);\n";
    let single = match_caller_bytes(call, 1);
    let repeated = match_caller_bytes(call, 17);
    let growth = repeated
        .checked_sub(single)
        .expect("RegExp allocations remain");
    // Literal allocation remains at each site; matcher and fallback bodies do not.
    assert!(
        growth < 16 * 32768,
        "sixteen literal match calls added {growth} bytes ({single} -> {repeated})"
    );
}
