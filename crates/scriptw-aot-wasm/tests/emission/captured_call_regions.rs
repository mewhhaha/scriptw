//! Real dynamic closures fuse only inside a fully guarded numeric region.
use crate::{
    property_read_plans::Module,
    scalar_masked_rows::{loops, read_only},
};
use scriptw_aot_wasm::emit;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{Operator, Validator, WasmFeatures};

#[test]
fn unchanged_captured_call_programs_have_scalar_loops_and_real_call_fallbacks() {
    std::thread::Builder::new().name("captured-call-regions".into()).stack_size(64 * 1024 * 1024).spawn(|| {
        for source in [include_str!("fixtures/numeric_body_args_unused.mjs"), include_str!("fixtures/numeric_body_frame_compact.mjs")] {
            let ir = lower(&parse(source, ParseOptions::module()).unwrap());
            assert!(ir.is_wasm_supported(), "{:?}", ir.diagnostics);
            let artifact = emit(&ir).unwrap();
            for bytes in [artifact.bytes.as_slice(), artifact.runtime().unwrap().bytes()] {
                Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
            }
            let p = Module::read(&artifact.bytes, true);
            let (_, body) = p.named("js::run#");
            assert!(loops(body).iter().any(|body| read_only(body)
                && body.iter().any(|op| matches!(op, Operator::F64Add))
                && body.iter().any(|op| matches!(op, Operator::ArrayGet { .. }))),
                "an actual captured-call loop must have a projected scalar body with no calls, allocation or heap writes");
            assert!(body.iter().any(|op| matches!(op, Operator::CallRef { .. })), "the original dynamic call still owns every declined entry");
            assert!(body.iter().any(|op| matches!(op, Operator::RefEq)), "actual cell/callable identities guard aliases and code ownership");
        }
    }).unwrap().join().unwrap();
}

#[test]
fn observed_arguments_and_large_forwarding_switches_keep_source_calls() {
    std::thread::Builder::new()
        .name("captured-call-controls".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let original = include_str!("fixtures/numeric_body_args_unused.mjs");
            let observed =
                original.replace("var original = value;", "var original = arguments[0];");
            let frame = include_str!("fixtures/numeric_body_frame_compact.mjs");
            let mut branches = String::from("switch(index & 255) {");
            for index in 0..256 {
                branches.push_str(&format!("case {index}: return steps[{}](row);", index & 1));
            }
            branches.push('}');
            let large = frame.replace("return steps[index & 1](row);", &branches);
            assert_ne!(original, observed);
            assert_ne!(frame, large);
            for source in [observed, large] {
                let ir = lower(&parse(&source, ParseOptions::module()).unwrap());
                let artifact = emit(&ir).unwrap();
                let p = Module::read(&artifact.bytes, true);
                let (_, body) = p.named("js::run#");
                assert!(
                    !loops(body).iter().any(|body| read_only(body)
                        && body.iter().any(|op| matches!(op, Operator::F64Add))
                        && body
                            .iter()
                            .any(|op| matches!(op, Operator::ArrayGet { .. }))),
                    "unsupported observers and source budgets cannot enter the fused region"
                );
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
