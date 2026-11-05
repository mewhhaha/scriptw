//! The unchanged scored Number comparison loop uses the existing scalar owner.
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
                && region.iter().any(|op| matches!(op, Operator::F64Eq))
                && region.iter().filter(|op| matches!(op, Operator::ArrayGet{array_type_index} if *array_type_index == numbers)).count() == 2
            ).collect::<Vec<_>>();
            assert_eq!(
                hot.len(),
                usize::from(*admitted),
                "closed two-input body: {}",
                variant.name
            );
            if !admitted {
                continue;
            }
            let hot = hot[0];
            assert_eq!(
                hot.iter()
                    .filter(|op| matches!(op, Operator::F64Eq))
                    .count(),
                1,
                "one source comparison per backedge, no unsafe unrolling"
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
                "entry authenticates both current backings; the body only reads Number bits"
            );
            assert!(!body.iter().any(|op| matches!(op, Operator::ArrayNew{array_type_index} if *array_type_index == numbers)),
                "two existing Number backings are borrowed without a projection allocation");
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
fn unchanged_number_comparison_source_has_one_ordered_typed_backedge() {
    std::thread::Builder::new()
        .name("typed-number-inputs".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            check(
                include_str!("fixtures/strict-equality-numbers.mjs"),
                &[("run", true)],
            );
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn numeric_branch_region_requires_dominance_private_writes_and_complete_effect_proof() {
    std::thread::Builder::new().name("typed-number-refusals".into()).stack_size(64*1024*1024).spawn(|| {
        check(r#"
export function local(left,right,phase) {var sum=0;for(var i=0;i<32;i++){var slot=(i+phase)&3;if(left[slot]===right[slot])sum+=(slot&3)+1;else sum-=(slot&1)+1;}return sum+i+slot;}
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
"#, &[("local",true),("callback",false),("writeInput",false),("movingIndex",false),
("movingCounter",false),("movingPhase",false),("capturedIndex",false),("capturedSum",false),
("prefix",false),("differentIndex",false),("noElse",false),("lexical",false),("zero",false)]);
    }).unwrap().join().unwrap();
}
