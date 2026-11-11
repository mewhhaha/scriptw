//! Original scored computed-property sources, with whitespace-only blank lines normalized.
//! The generated benchmark corpus remains byte-identical and is not edited here.
use crate::{
    property_read_plans::Module,
    scalar_masked_rows::{loops, read_only, type_group_sizes},
};
use scriptw_aot_wasm::{GcSnapshotLayout, PromiseRejectionPolicy, emit_with_rooted_snapshot};
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::lower;
use wasmparser::{HeapType, Operator, UnpackedIndex, Validator, WasmFeatures};

#[test]
fn actual_computed_property_sources_prevalidate_both_tables_before_numeric_loops() {
    std::thread::Builder::new().name("computed-key-projection".into()).stack_size(64*1024*1024).spawn(|| {
        for source in [include_str!("fixtures/property-stable.mjs"),include_str!("fixtures/property-mutated.mjs")] {
            let ir=lower(&parse(source,ParseOptions::module()).unwrap());
            assert!(ir.is_wasm_supported(),"{:?}",ir.diagnostics);
            let artifact=emit_with_rooted_snapshot(&ir,PromiseRejectionPolicy::Ignore,&scriptw_intl::IntlCompilationProfile::Minimal).unwrap();
            let r=artifact.runtime().unwrap().bytes();
            for bytes in [artifact.bytes.as_slice(),r] {Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();}
            let program=Module::read(&artifact.bytes,true);let runtime=Module::read(r,true);
            assert_eq!(&program.types[..runtime.types.len()],&runtime.types);
            let pg=type_group_sizes(&artifact.bytes);let rg=type_group_sizes(r);
            assert_eq!(&pg[..rg.len()],&rg,"authenticate actual R/P recursive GC identities");
            let numbers=runtime.witness_type(GcSnapshotLayout::ArrayNumbers);
            let values=runtime.witness_type(GcSnapshotLayout::ArrayValues);
            let string=runtime.witness_type(GcSnapshotLayout::String);
            let (_,body)=program.named("js::run#");
            assert_eq!(body.iter().filter(|op|matches!(op,Operator::ArrayNew{array_type_index} if *array_type_index==numbers)).count(),1);
            let regions=loops(body);
            let hot=regions.iter().filter(|region|read_only(region)
                && region.iter().any(|op|matches!(op,Operator::F64Add))
                && region.iter().any(|op|matches!(op,Operator::ArrayGet{array_type_index} if *array_type_index==numbers))).collect::<Vec<_>>();
            assert_eq!(hot.len(),1,"one arithmetic backedge for the real source");
            assert_eq!(hot[0].iter().filter(|op|matches!(op,Operator::ArrayGet{array_type_index} if *array_type_index==numbers)).count(),1);
            assert!(!hot[0].iter().any(|op|matches!(op,Operator::StructGet{..}|Operator::RefTestNonNull{..}|Operator::RefCastNonNull{..}|Operator::RefEq)),"no repeated key/row/value guard");
            let preflight=regions.iter().filter(|region|region.iter().any(|op|matches!(op,Operator::ArraySet{array_type_index} if *array_type_index==numbers))).collect::<Vec<_>>();
            assert_eq!(preflight.len(),1);
            assert_eq!(preflight[0].iter().filter(|op|matches!(op,Operator::ArrayGet{array_type_index} if *array_type_index==values)).count(),2,"one original row and one aligned current String key");
            assert!(preflight[0].iter().any(|op|matches!(op,Operator::RefTestNonNull{hty:HeapType::Concrete(UnpackedIndex::Module(ty))} if *ty==string)),"a real String proof precedes the own-data probe");
            assert!(!preflight[0].iter().any(|op|matches!(op,Operator::F64Add|Operator::F64Sub|Operator::F64Mul|Operator::F64Div)),"preflight has no source arithmetic");
            assert!(regions.iter().any(|region|region.iter().any(|op|matches!(op,Operator::Call{..}|Operator::CallRef{..}))
                && region.iter().any(|op|matches!(op,Operator::F64Add))),"the whole original loop remains on miss");
        }
    }).unwrap().join().unwrap();
}

#[test]
fn cyclic_projection_refuses_incomplete_periods_writes_calls_and_captured_accumulators() {
    std::thread::Builder::new().name("computed-key-refusals".into()).stack_size(64*1024*1024).spawn(||{
        let ir=lower(&parse(r#"
export function onePeriod(rows,keys,seed){var sum=seed;for(var i=0;i<4;i++)sum+=rows[i&3][keys[i&1]];return sum;}
export function sparseKeyMask(rows,keys,seed){var sum=seed;for(var i=0;i<32;i++)sum+=rows[i&3][keys[i&5]];return sum;}
export function aliasKey(rows,seed){var row=['x','y'];var sum=seed;for(var i=0;i<16;i++){var row=rows[i&3];sum+=row[row[i&1]];}return sum;}
export function keyWrite(rows,keys,seed){var sum=seed;for(var i=0;i<32;i++){sum+=rows[i&3][keys[i&1]];keys[0]='x';}return sum;}
export function callback(rows,keys,observe){var sum=0;for(var i=0;i<32;i++){sum+=rows[i&3][keys[i&1]];observe();}return sum;}
export function captured(rows,keys,seed){var sum=seed;function read(){return sum;}for(var i=0;i<32;i++)sum+=rows[i&3][keys[i&1]];return read;}
"#,ParseOptions::module()).unwrap());
        assert!(ir.is_wasm_supported(),"{:?}",ir.diagnostics);
        let artifact=emit_with_rooted_snapshot(&ir,PromiseRejectionPolicy::Ignore,&scriptw_intl::IntlCompilationProfile::Minimal).unwrap();
        for bytes in [artifact.bytes.as_slice(), artifact.runtime().unwrap().bytes()] {Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();}
        let runtime=Module::read(artifact.runtime().unwrap().bytes(),true);let program=Module::read(&artifact.bytes,true);
        assert_eq!(&program.types[..runtime.types.len()],&runtime.types);
        let rg=type_group_sizes(artifact.runtime().unwrap().bytes());let pg=type_group_sizes(&artifact.bytes);assert_eq!(&pg[..rg.len()],&rg);
        let numbers=runtime.witness_type(GcSnapshotLayout::ArrayNumbers);
        for name in ["onePeriod","sparseKeyMask","aliasKey","keyWrite","callback","captured"] {
            let (_,body)=program.named(&format!("js::{name}#"));
            assert!(!body.iter().any(|op|matches!(op,Operator::ArrayNew{array_type_index} if *array_type_index==numbers)),"{name} has no speculative numeric projection");
        }
    }).unwrap().join().unwrap();
}
