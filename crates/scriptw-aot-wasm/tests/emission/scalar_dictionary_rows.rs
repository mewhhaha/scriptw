//! The existing complete row proof admits live dictionary data without Get.
use crate::{
    linked_bodies::module_bodies,
    property_read_plans::Module,
    scalar_masked_rows::{loops, read_only, type_group_sizes},
};
use scriptw_aot_wasm::{
    emit_with_rooted_snapshot, GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{HeapType, Operator, UnpackedIndex, Validator, WasmFeatures};

fn checks(source: &str, options: ParseOptions, expected: &[(&str, usize)]) {
    let ir = lower(&parse(source, options).unwrap());
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
    let runtime = Module::read(r, true);
    assert!(program.types.len() >= runtime.types.len());
    assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
    let p_groups = type_group_sizes(&artifact.bytes);
    let r_groups = type_group_sizes(r);
    assert!(p_groups.len() >= r_groups.len());
    assert_eq!(
        &p_groups[..r_groups.len()],
        &r_groups,
        "authenticate R witness indices in P"
    );
    let storage = runtime.witness_type(GcSnapshotLayout::PropertyStorage);
    let table = runtime.witness_type(GcSnapshotLayout::Properties);
    let callable = runtime.witness_type(GcSnapshotLayout::Function);
    let stored = runtime.witness_type(GcSnapshotLayout::Stored);
    let descriptor = runtime.witness_type(GcSnapshotLayout::Descriptor);
    let (find, _) = runtime.named("helper::ordinary_property_find");
    let find = program.imported_runtime_function(&runtime, find);
    for (name, count) in expected {
        let variants = module_bodies(&artifact.bytes)
            .into_iter()
            .filter(|body| body.name.starts_with(&format!("js::{name}#")))
            .collect::<Vec<_>>();
        assert!(!variants.is_empty(), "named source {name} remains encoded");
        for variant in variants {
            let (_, body) = program.named(&variant.name);
            let regions = loops(body);
            let closed=regions.iter().filter(|region|read_only(region)
                && region.iter().any(|op|matches!(op,Operator::StructGet{struct_type_index,..} if *struct_type_index==storage))
                && region.iter().any(|op|matches!(op,Operator::ArrayGet{array_type_index} if *array_type_index==table))
                && region.iter().any(|op|matches!(op,Operator::F64Add))).collect::<Vec<_>>();
            assert_eq!(
                closed.len(),
                *count,
                "complete dictionary proof in {}",
                variant.name
            );
            if *count == 0 {
                continue;
            }
            let region = closed[0];
            for ty in [storage, callable, stored] {
                assert!(region.iter().any(|op|matches!(op,Operator::RefTestNonNull{hty:HeapType::Concrete(UnpackedIndex::Module(actual))} if *actual==ty)),"guard actual dictionary, callable and eager value layouts");
            }
            assert_eq!(
                body.iter()
                    .filter(
                        |op| matches!(op,Operator::Call{function_index} if *function_index==find)
                    )
                    .count(),
                2,
                "only first-row key resolution calls the existing finder"
            );
            let reads = region
                .iter()
                .enumerate()
                .filter_map(|(i, op)| {
                    matches!(op,Operator::ArrayGet{array_type_index} if *array_type_index==table)
                        .then_some(i)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                reads.len(),
                2,
                "two data field hints are revalidated in each row"
            );
            for at in &reads {
                let preceding = &region[at.saturating_sub(16)..*at];
                assert!(preceding.iter().any(|op|matches!(op,Operator::StructGet{struct_type_index,field_index} if *struct_type_index==storage && *field_index==GcSnapshotField::PropertiesLength.index())),"logical insertion extent guards every table slot");
                assert_eq!(
                    preceding
                        .iter()
                        .filter(|op| matches!(op, Operator::I32GeU))
                        .count(),
                    2,
                    "logical LENGTH and physical capacity each decline"
                );
            }
            assert_eq!(region.iter().filter(|op|matches!(op,Operator::StructGet{struct_type_index,field_index} if *struct_type_index==descriptor && *field_index==GcSnapshotField::DescriptorFlags.index())).count(),2,"both entries must be data before primitive reads");
            let last_arithmetic = region
                .iter()
                .rposition(|op| matches!(op, Operator::F64Add))
                .unwrap();
            assert!(
                reads.iter().all(|at| *at > last_arithmetic),
                "the completed body precedes preparation of every next-row field"
            );
            assert!(
                matches!(region.last(), Some(Operator::Br { relative_depth: 0 })),
                "next-row preparation finishes before the backedge can enter another body"
            );
            assert!(
                regions.iter().any(|region| region
                    .iter()
                    .any(|op| matches!(op, Operator::Call { .. } | Operator::CallRef { .. }))),
                "original generic continuation remains encoded"
            );
        }
    }
}

#[test]
fn dictionary_rows_require_the_whole_closed_region_and_live_entry_guards() {
    std::thread::Builder::new().name("scalar-dictionary-rows".into()).stack_size(64*1024*1024).spawn(||{
        // One-period function-row control retains the guarded dictionary path.
        let source=r#"var rows, seed;
export function prepare(input) {
  seed = input;
  var targets = [function Array(a) {}, function Map() {}, function Set() {},
    function Date(a, b, c, d, e, f, g) {}, function Number(a) {},
    function String(a) {}, function Boolean(a) {}, function Uint8Array(a, b, c) {}];
  rows = [];
  for (var i = 0; i < 64; i++) rows.push(targets[(input + i) & 7]);
}
export function run() {
  var sum = seed;
  for (var i = 0; i < 64; i++) {
    var row = rows[i & 63];
    sum += row.length + row.name.length;
  }
  print(sum);
}

export function capturedAlias(array,seed,extra) {
  var sum=seed;function observe(){return row;}
  for(var i=0;i<8;i++){var row=array[i&3];sum+=row.length+row.name.length;}
  return observe;
}
export function callback(array,seed,observe) {
  var sum=seed;for(var i=0;i<8;i++){var row=array[i&3];sum+=row.length+row.name.length;observe();}return sum;
}
export function mutation(array,seed,extra) {
  var sum=seed;for(var i=0;i<8;i++){var row=array[i&3];sum+=row.length+row.name.length;row.name='x';}return sum;
}
"#;
        checks(source,ParseOptions::module(),&[("run",1),("capturedAlias",0),("callback",0),("mutation",0)]);
        checks(r#"
function withScan(array,seed,environment) {
  var sum=seed;with(environment){for(var i=0;i<8;i++){var row=array[i&3];sum+=row.length+row.name.length;}}return sum;
}
[withScan];
"#,ParseOptions::script(),&[("withScan",0)]);
    }).unwrap().join().unwrap();
}
