//! Strict pair preflight reuses exact equality and one ordered Number backedge.
use crate::{
    linked_bodies::module_bodies,
    property_read_plans::Module,
    scalar_masked_rows::{loops, read_only, type_group_sizes},
};
use scriptw_aot_wasm::{emit_with_rooted_snapshot, GcSnapshotLayout, PromiseRejectionPolicy};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{Operator, Validator, WasmFeatures};

fn check(source: &str, names: &[(&str, bool)]) {
    let ir = lower(&parse(source, ParseOptions::module()).unwrap());
    assert!(ir.is_wasm_supported(), "{:?}", ir.diagnostics);
    let artifact = emit_with_rooted_snapshot(
        &ir,
        PromiseRejectionPolicy::Ignore,
        &scriptw_intl::IntlCompilationProfile::Minimal,
    )
    .unwrap();
    let r = artifact.runtime().unwrap().bytes();
    for bytes in [artifact.bytes.as_slice(), r] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .unwrap();
    }
    let program = Module::read(&artifact.bytes, true);
    let runtime = Module::read(r, false);
    assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
    let pg = type_group_sizes(&artifact.bytes);
    let rg = type_group_sizes(r);
    assert_eq!(
        &pg[..rg.len()],
        &rg,
        "authenticate the actual shared recursive types"
    );
    let numbers = runtime.witness_type(GcSnapshotLayout::ArrayNumbers);
    let values = runtime.witness_type(GcSnapshotLayout::ArrayValues);
    let (bigint_definition, _) = runtime.named("helper::bigint_equality");
    let bigint = program.imported_runtime_function(&runtime, bigint_definition);
    assert_eq!(
        program.function_type(bigint),
        runtime.function_type(bigint_definition)
    );
    let bodies = module_bodies(&artifact.bytes);
    for (name, admitted) in names {
        let variants = bodies
            .iter()
            .filter(|body| body.name.starts_with(&format!("js::{name}#")))
            .collect::<Vec<_>>();
        assert!(!variants.is_empty(), "actual source body {name}");
        for variant in variants {
            let (_, body) = program.named(&variant.name);
            let regions = loops(body);
            let hot = regions.iter().filter(|region| read_only(region)
                && region.iter().any(|op| matches!(op, Operator::F64Add))
                && region.iter().any(|op| matches!(op, Operator::F64Sub))
                && region.iter().filter(|op| matches!(op, Operator::ArrayGet{array_type_index} if *array_type_index == numbers)).count() == 1
            ).collect::<Vec<_>>();
            assert_eq!(
                hot.len(),
                usize::from(*admitted),
                "closed projected strict pair: {}",
                variant.name
            );
            if !admitted {
                assert!(!body.iter().any(|op| matches!(op, Operator::ArrayNew { array_type_index } if *array_type_index == numbers)),
                    "a refused complete proof emits no speculative projection");
                continue;
            }
            let hot = hot[0];
            assert_eq!(
                hot.iter()
                    .filter(|op| matches!(op, Operator::F64Eq))
                    .count(),
                0,
                "the exact comparison is precomputed, never approximated in the hot loop"
            );
            assert!(hot.iter().any(|op| matches!(op, Operator::If { .. })));
            assert!(hot.iter().any(|op| matches!(op, Operator::Else)));
            assert!(hot.iter().any(|op| matches!(op, Operator::F64Add)));
            assert!(hot.iter().any(|op| matches!(op, Operator::F64Sub)));
            assert!(
                !hot.iter().any(|op| matches!(
                    op,
                    Operator::StructGet { .. }
                        | Operator::RefTestNonNull { .. }
                        | Operator::RefCastNonNull { .. }
                )),
                "the body only reads the private predicate projection"
            );
            assert_eq!(body.iter().filter(|op| matches!(op, Operator::ArrayNew{array_type_index} if *array_type_index == numbers)).count(), 1,
                "one existing NumberBits array holds the complete period");
            let preflight = regions
                .iter()
                .filter(|region| {
                    region.iter().any(|op|
                matches!(op, Operator::ArraySet{array_type_index} if *array_type_index == numbers)
            )
                })
                .collect::<Vec<_>>();
            assert_eq!(preflight.len(), 1, "one complete-period preflight");
            let preflight = preflight[0];
            for backing in [numbers, values] {
                assert_eq!(preflight.iter().filter(|op| matches!(op, Operator::ArrayGet { array_type_index } if *array_type_index == backing)).count(), 2,
                    "each input supports its current canonical packed backing");
            }
            assert!(!preflight.iter().any(|op| matches!(op,
                Operator::F64Add | Operator::F64Sub | Operator::F64Mul | Operator::F64Div
                | Operator::StructNew { .. } | Operator::ArrayNew { .. })),
                "preflight compares current Values without source arithmetic or per-element allocation");
            assert_eq!(preflight.iter().filter(|op| matches!(op, Operator::Call { function_index } if *function_index == bigint)).count(), 1,
                "the canonical all-kind kernel retains real BigInt equality");
            assert_eq!(
                preflight
                    .iter()
                    .filter(|op| matches!(op, Operator::F64Eq))
                    .count(),
                1,
                "the same canonical kernel retains exact Number equality"
            );
            assert!(
                preflight.iter().any(|op| matches!(op, Operator::RefEq)),
                "reference identity remains inside the exact kernel"
            );
            assert_eq!(
                hot.iter()
                    .filter(|op| matches!(op, Operator::I32LtU))
                    .count(),
                1
            );
            assert_eq!(
                hot.iter()
                    .filter(|op| matches!(op, Operator::I32Add))
                    .count(),
                2,
                "one phase addition and one canonical counter update; no induction mirror"
            );
            assert!(
                !hot.iter()
                    .any(|op| matches!(op, Operator::F64Lt | Operator::F64Le)),
                "the sealed bounded source counter supplies the one integer loop test"
            );
            assert_eq!(
                hot.iter()
                    .filter(|op| matches!(op, Operator::F64Add))
                    .count(),
                3,
                "both branch offsets plus the true-arm addition remain; no Number counter update"
            );
            assert!(
                hot.iter().any(|op| matches!(op, Operator::I64Eqz)),
                "the projection stores exactly Number zero/one bits"
            );
            assert!(
                regions.iter().any(|region| region
                    .iter()
                    .any(|op| matches!(op, Operator::Call { .. } | Operator::CallRef { .. }))
                    && region.iter().any(|op| matches!(op, Operator::F64Add))),
                "the original whole loop remains available on every entry miss"
            );
        }
    }
}

#[test]
fn unchanged_number_and_mixed_sources_share_one_projected_strict_backedge() {
    std::thread::Builder::new()
        .name("typed-number-inputs".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            check(
                include_str!("fixtures/strict-equality-numbers.mjs"),
                &[("run", true)],
            );
            check(
                include_str!("fixtures/strict-equality-mixed.mjs"),
                &[("run", true)],
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn strict_projection_requires_dominance_complete_period_and_private_effect_proof() {
    std::thread::Builder::new().name("typed-number-refusals".into()).stack_size(64*1024*1024).spawn(|| {
        check(r#"
export function local(left,right,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=(slot&3)+1;else sum-=(slot&1)+1;}return sum+i+slot;}
export function positiveFirst(left,right,phase) {var sum=0;for(var i=5;i<32;i++){var slot=(i+phase)&3;if(left[slot]!==right[slot])sum+=(slot&3)+1;else sum-=(slot&1)+1;}return sum+i+slot;}
export function boundedLast(left,right,phase) {var sum=0;for(var i=1048570;i<1048576;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=(slot&3)+1;else sum-=(slot&1)+1;}return sum+i+slot;}
export function negativeFirst(left,right,phase) {var sum=0;for(var i=-0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1/i;else sum-=1/i;}return sum+i+slot;}
export function loose(left,right,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]==right[slot])sum+=1;else sum-=1;}return sum;}
export function onePeriod(left,right,phase) {var sum=0;for(var i=0;i<4;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;}return sum;}
export function shortPeriod(left,right,phase) {var sum=0;for(var i=30;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;}return sum;}
export function overBound(left,right,phase) {var sum=0;for(var i=1048570;i<1048577;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;}return sum;}
export function numericElements(left,right,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=left[slot];else sum-=right[slot];}return sum;}
export function secondPair(left,right,third,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;if(left[slot]===third[slot])sum+=2;else sum-=2;}return sum;}
export function callback(left,right,phase,observe) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;observe();}return sum;}
export function writeInput(left,right,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;left[slot]=0;}return sum;}
export function movingIndex(left,right,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;slot=0;if(left[slot]===right[slot])sum+=1;else sum-=1;}return sum;}
export function movingCounter(left,right,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;i++;}return sum;}
export function movingPhase(left,right,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;phase++;}return sum;}
export function capturedIndex(left,right,phase) {var sum=0;function read(){return slot;}for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;}return read;}
export function capturedSum(left,right,phase) {var sum=0;function read(){return sum;}for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;}return read;}
export function prefix(left,right,phase) {var sum=0;for(var i=0;i<32;i++){sum++;var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;}return sum;}
export function differentIndex(left,right,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[i&3])sum+=1;else sum-=1;}return sum;}
export function noElse(left,right,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;}return sum;}
export function lexical(left,right,phase) {var sum=0;for(var i=0;i<32;i++){let slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;}return sum;}
export function zero(left,right,phase) {var sum=0;for(var i=0;i<0;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=1;else sum-=1;}return slot;}
"#, &[("local",true),("positiveFirst",true),("boundedLast",true),("negativeFirst",false),("loose",false),
("onePeriod",false),("shortPeriod",false),("overBound",false),("numericElements",false),
("secondPair",false),("callback",false),("writeInput",false),("movingIndex",false),
("movingCounter",false),("movingPhase",false),("capturedIndex",false),("capturedSum",false),
("prefix",false),("differentIndex",false),("noElse",false),("lexical",false),("zero",false)]);
    }).unwrap().join().unwrap();
}
