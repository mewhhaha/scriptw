//! A repeated complete row domain becomes a private numeric projection.
use crate::{
    property_read_plans::Module,
    scalar_masked_rows::{loops, read_only, type_group_sizes},
};
use scriptw_aot_wasm::{emit_with_rooted_snapshot, GcSnapshotLayout, PromiseRejectionPolicy};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{HeapType, Operator, UnpackedIndex, Validator, WasmFeatures};

#[test]
fn repeated_rows_validate_before_source_work_and_keep_only_numeric_loads_on_the_backedge() {
    std::thread::Builder::new()
        .name("scalar-row-projection".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let source = format!(
                "{}{}",
                include_str!("fixtures/compact_inline_metadata.mjs"),
                r#"
export function onePeriod(array,seed,extra) {
  var sum=seed;for(var i=0;i<4;i++){var row=array[i&3];sum+=row.length+row.name.length;}return sum;
}
export function sparseMask(array,seed,extra) {
  var sum=seed;for(var i=0;i<32;i++){var row=array[i&5];sum+=row.length+row.name.length;}return sum;
}
export function callback(array,seed,observe) {
  var sum=seed;for(var i=0;i<32;i++){var row=array[i&3];sum+=row.length+row.name.length;observe();}return sum;
}
export function nestedArray(array,seed,column) {
  var sum=seed;for(var i=0;i<32;i++){var row=array[i&3];sum+=row.values[column];}return sum;
}
export function escapedAlias(array,seed,extra) {
  var sum=seed;function read(){return row;}for(var i=0;i<32;i++){var row=array[i&3];sum+=row.length+row.name.length;}return read;
}
"#
            );
            let ir = lower(&parse(&source, ParseOptions::module()).unwrap());
            assert!(ir.is_wasm_supported(), "{:?}", ir.diagnostics);
            let artifact = emit_with_rooted_snapshot(
                &ir,
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            ).unwrap();
            let r = artifact.runtime().unwrap().bytes();
            for bytes in [artifact.bytes.as_slice(), r] {
                Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
            }
            let program = Module::read(&artifact.bytes, true);
            let runtime = Module::read(r, true);
            assert!(program.types.len() >= runtime.types.len());
            assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
            let pg = type_group_sizes(&artifact.bytes);
            let rg = type_group_sizes(r);
            assert_eq!(&pg[..rg.len()], &rg, "authenticate R's concrete GC witnesses in P");
            let numbers = runtime.witness_type(GcSnapshotLayout::ArrayNumbers);
            let originals = runtime.witness_type(GcSnapshotLayout::ArrayValues);
            let (_, body) = program.named("js::run#");
            assert_eq!(body.iter().filter(|op| matches!(op, Operator::ArrayNew { array_type_index } if *array_type_index == numbers)).count(), 1,
                "one temporary existing numeric array; no per-row allocations");
            let regions = loops(body);
            let hot = regions.iter().filter(|region| read_only(region)
                && region.iter().any(|op| matches!(op, Operator::F64Add))
                && region.iter().any(|op| matches!(op, Operator::ArrayGet {array_type_index} if *array_type_index == numbers)))
                .collect::<Vec<_>>();
            assert_eq!(hot.len(), 1, "one source arithmetic loop after complete prevalidation");
            assert_eq!(hot[0].iter().filter(|op| matches!(op, Operator::ArrayGet {array_type_index} if *array_type_index == numbers)).count(), 2);
            assert_eq!(hot[0].iter().filter(|op| matches!(op, Operator::F64Add)).count(), 3,
                "retain both source Number additions and the original induction update");
            assert!(!hot[0].iter().any(|op| matches!(op,
                Operator::StructGet {..} | Operator::StructGetS {..} | Operator::StructGetU {..}
                | Operator::RefTestNonNull {..} | Operator::RefTestNullable {..}
                | Operator::RefCastNonNull {..} | Operator::RefCastNullable {..} | Operator::RefEq)),
                "the repeated backedge has no receiver, shape, descriptor, key or value-tag guards");
            let preflight = regions.iter().filter(|region| region.iter().any(|op|
                matches!(op, Operator::ArraySet {array_type_index} if *array_type_index == numbers))).collect::<Vec<_>>();
            assert_eq!(preflight.len(), 1);
            assert_eq!(preflight[0].iter().filter(|op| matches!(op, Operator::ArraySet {array_type_index} if *array_type_index == numbers)).count(), 2);
            assert!(preflight[0].iter().any(|op| matches!(op, Operator::RefTestNonNull {..})), "reuse the actual live own-data guard");
            // The first-row gate must use this same decoder: an earlier inline
            // or dictionary-only guard would silently exclude intrinsic rows.
            let allocation = body.iter().position(|op| matches!(op,
                Operator::ArrayNew { array_type_index } if *array_type_index == numbers)).unwrap();
            for helper in ["helper::intrinsic_property_find", "helper::intrinsic_property_value"] {
                let (definition, _) = runtime.named(helper);
                let imported = program.imported_runtime_function(&runtime, definition);
                assert_eq!(program.function_type(imported), runtime.function_type(definition));
                for phase in [&body[..allocation], preflight[0].as_ref()] {
                    assert_eq!(phase.iter().filter(|op| matches!(op,
                        Operator::Call { function_index } if *function_index == imported)).count(), 2,
                        "both own fields use canonical intrinsic decoding before allocation and during preflight");
                }
            }
            let (resolve, _) = runtime.named("helper::property_value_resolve");
            let resolve = program.imported_runtime_function(&runtime, resolve);
            assert!(preflight[0].iter().any(|op| matches!(op,
                Operator::Call { function_index } if *function_index == resolve)),
                "deferred DATA values retain the canonical defining-Realm resolver");
            let storage = runtime.witness_type(GcSnapshotLayout::CompactStorage);
            let concrete = runtime.types.iter().enumerate().filter_map(|(index, ty)|
                (ty.supertype_idx.map(|parent| parent.unpack()) == Some(UnpackedIndex::Module(storage)))
                    .then_some(index as u32)).collect::<Vec<_>>();
            assert_eq!(concrete.len(), 2, "canonical inline and array compact storage");
            for ty in concrete {
                assert!(preflight[0].iter().any(|op| matches!(op,
                    Operator::RefCastNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) }
                        if *index == ty)), "preflight uses both existing compact value decoders");
            }
            assert!(!preflight[0].iter().any(|op| matches!(op, Operator::F64Add | Operator::F64Sub | Operator::F64Mul | Operator::F64Div)),
                "no source arithmetic executes while the projection remains incomplete");
            assert!(body.iter().any(|op| matches!(op, Operator::ArrayGet {array_type_index} if *array_type_index == originals)),
                "the final row alias comes from the original rooted table");
            assert!(regions.iter().any(|region| region.iter().any(|op| matches!(op, Operator::Call {..} | Operator::CallRef {..}))
                && region.iter().any(|op| matches!(op, Operator::F64Add))), "whole original loop remains on a preflight miss");
            for name in ["onePeriod", "sparseMask", "callback", "nestedArray", "escapedAlias"] {
                let (_, body) = program.named(&format!("js::{name}#"));
                assert!(!body.iter().any(|op| matches!(op, Operator::ArrayNew { array_type_index } if *array_type_index == numbers)),
                    "projection refuses {name} without an allocation");
            }
        }).unwrap().join().unwrap();
}
