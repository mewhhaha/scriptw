//! Real dynamic closures fuse only inside a fully guarded numeric region.
use crate::{
    property_read_plans::Module,
    scalar_masked_rows::{loops, read_only},
};
use scriptw_aot_wasm::emit;
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::lower;
use wasmparser::{Operator, Validator, WasmFeatures};

#[test]
fn unchanged_captured_call_programs_have_scalar_loops_and_real_call_fallbacks() {
    std::thread::Builder::new().name("captured-call-regions".into()).stack_size(64 * 1024 * 1024).spawn(|| {
        for (source, alias_comparisons) in [(include_str!("fixtures/numeric_body_args_unused.mjs"), 0), (include_str!("fixtures/numeric_body_frame_compact.mjs"), 8)] {
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
            let region = loops(body).into_iter().find(|body| read_only(body)
                && body.iter().any(|op| matches!(op, Operator::ArrayGet { .. }))
                && body.iter().any(|op| matches!(op, Operator::F64Add))).unwrap();
            assert_eq!(region.windows(3).filter(|ops| matches!(ops,
                [Operator::LocalGet { .. }, Operator::LocalGet { .. }, Operator::I32Eq])).count(), alias_comparisons,
                "only cross-site captures need a per-write alias comparison; inactive source alternatives cannot alias");
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

#[test]
fn call_domains_remove_only_redundant_number_modulo_and_singleton_dispatch() {
    std::thread::Builder::new()
        .name("captured-call-domains".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            for (step, modulo, branches) in [
                (
                    "tick=row.x|0;var value=tick;value=(value+3)|0;return (value+tick)|0;",
                    1,
                    0,
                ),
                (
                    "var value=row.x|0;if(row.x<0)value=row.x>>>0;return (value+1)|0;",
                    3,
                    1,
                ),
                (
                    "tick=row.x|0;if(row.y<0)tick=row.y;return (tick+1)|0;",
                    2,
                    1,
                ),
            ] {
                let source = format!(
                    "var rows,step;export function prepare(input){{rows=[{{x:input,y:input}},{{x:2,y:3}},{{x:4,y:5}},{{x:6,y:7}}];var tick=input;step=function(row){{{step}}};}}export function run(){{var sum=0;for(var i=0;i<32;i++)sum+=step(rows[i&3]);return sum;}}"
                );
                let ir = lower(&parse(&source, ParseOptions::module()).unwrap());
                let artifact = emit(&ir).unwrap();
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(&artifact.bytes)
                    .unwrap();
                let p = Module::read(&artifact.bytes, true);
                let (_, body) = p.named("js::run#");
                let regions: Vec<_> = loops(body)
                    .into_iter()
                    .filter(|body| {
                        read_only(body)
                            && body.iter().any(|op| matches!(op, Operator::ArrayGet { .. }))
                            && body.iter().any(|op| matches!(op, Operator::I32Or))
                    })
                    .collect();
                assert_eq!(regions.len(), 1, "one admitted source loop: {step}");
                let region = regions[0];
                assert_eq!(
                    region.iter().filter(|op| matches!(op, Operator::F64Floor)).count(),
                    modulo,
                    "only arbitrary Number operands retain full ToUint32: {step}"
                );
                // Each retained full ToUint32 first takes its saturating
                // truncation below 2^63, one branch of its own.
                assert_eq!(
                    region.iter().filter(|op| matches!(op, Operator::If { .. })).count(),
                    branches + modulo,
                    "only actual source branches and full ToUint32 fast paths remain after singleton admission: {step}"
                );
                assert!(body.iter().any(|op| matches!(op, Operator::CallRef { .. })));
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
