//! Closed masked row loops reuse scalar storage and retain the generic continuation.
use crate::{linked_bodies::module_bodies, property_read_plans::Module};
use scriptw_aot_wasm::{
    emit_with_rooted_snapshot, GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{HeapType, Operator, Parser, Payload, UnpackedIndex, Validator, WasmFeatures};

pub(super) fn type_group_sizes(bytes: &[u8]) -> Vec<usize> {
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::TypeSection(groups) = payload.expect("module decodes") {
            return groups
                .into_iter()
                .map(|group| group.expect("recursive type group decodes").types().len())
                .collect();
        }
    }
    panic!("encoded module has a type section");
}

pub(super) fn loops<'a>(body: &'a [Operator<'a>]) -> Vec<&'a [Operator<'a>]> {
    let mut frames = Vec::new();
    let mut loops = Vec::new();
    for (index, op) in body.iter().enumerate() {
        match op {
            Operator::Loop { .. } => frames.push(Some(index)),
            Operator::Block { .. } | Operator::If { .. } | Operator::TryTable { .. } => {
                frames.push(None)
            }
            Operator::End => {
                if let Some(Some(start)) = frames.pop() {
                    loops.push(&body[start + 1..index]);
                }
            }
            _ => {}
        }
    }
    loops
}

pub(super) fn read_only(body: &[Operator<'_>]) -> bool {
    body.iter().all(|op| {
        !matches!(
            op,
            Operator::Call { .. }
                | Operator::CallRef { .. }
                | Operator::CallIndirect { .. }
                | Operator::ReturnCall { .. }
                | Operator::ReturnCallRef { .. }
                | Operator::ReturnCallIndirect { .. }
                | Operator::StructSet { .. }
                | Operator::ArraySet { .. }
                | Operator::GlobalSet { .. }
                | Operator::StructNew { .. }
                | Operator::StructNewDefault { .. }
                | Operator::ArrayNew { .. }
                | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. }
                | Operator::ArrayNewData { .. }
                | Operator::ArrayNewElem { .. }
        )
    })
}

#[test]
fn masked_rows_require_complete_private_effect_free_proofs_and_keep_real_fallbacks() {
    std::thread::Builder::new().name("scalar-masked-rows".into()).stack_size(64 * 1024 * 1024).spawn(|| {
        // One-period controls retain the live guarded-row path; the exact
        // repeated metadata source is covered by scalar_row_projection.
        let source = r#"
var rows, seed;
export function prepare(input) {
  seed = input;
  var targets = [{length: 1, name: 'Array'}, {length: 0, name: 'Map'},
    {length: 0, name: 'Set'}, {length: 7, name: 'Date'},
    {length: 1, name: 'Number'}, {length: 1, name: 'String'},
    {length: 1, name: 'Boolean'}, {length: 3, name: 'Uint8Array'}];
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
export function setRows(value) { rows=value; }
export function captured(seed, unused, extra) {
    var sum=seed; for(var i=0;i<4;i++){var row=rows[i&3];sum+=row.width+row.label.length;} return [sum,i,row];
}
export function local(array, seed, extra) {
    var sum=seed; for(var i=2;i<4;i++){var row=array[i&3];sum+=row.width+row.label.length;} return [sum,i,row];
}
export function split(seed, unused, extra) {
    var sum=seed, row='before';
    try { for(var i=0;i<4;i++){var row=rows[i&3];sum+=row.width;sum+=row.label.length;} }
    catch(error) { return [sum,i,row,error]; }
    return [sum,i,row];
}
export function callback(array, seed, observe) {
    var sum=seed; for(var i=0;i<32;i++){var row=array[i&3];sum+=row.width+row.label.length;observe();} return sum;
}
export function mutation(array, seed, extra) {
    var sum=seed; for(var i=0;i<32;i++){var row=array[i&3];sum+=row.width+row.label.length;row.width=4;} return sum;
}
export function aliasEscape(array, seed, extra) {
    var sum=seed; for(var i=0;i<32;i++){var row=array[i&3];sum+=row.width+row.label.length;extra=row;} return extra;
}
export function capturedAlias(array, seed, extra) {
    var sum=seed; function read(){return row;} for(var i=0;i<32;i++){var row=array[i&3];sum+=row.width+row.label.length;} return read;
}
export function movingCounter(array, seed, extra) {
    var sum=seed; for(var i=0;i<32;i++){var row=array[i&3];sum+=row.width+row.label.length;i++;} return sum;
}
export function dynamicMask(array, seed, mask) {
    var sum=seed; for(var i=0;i<32;i++){var row=array[i&mask];sum+=row.width+row.label.length;} return sum;
}
export function reversedIndex(array, seed, extra) {
    var sum=seed; for(var i=0;i<32;i++){var row=array[3&i];sum+=row.width+row.label.length;} return sum;
}
export function zeroTrip(array, seed, extra) {
    var sum=seed; for(var i=32;i<32;i++){var row=array[i&3];sum+=row.width+row.label.length;} return sum;
}
export function lexicalRow(array, seed, extra) {
    var sum=seed; for(var i=0;i<32;i++){let row=array[i&3];sum+=row.width+row.label.length;} return sum;
}
export function prefixEffect(array, seed, extra) {
    var sum=seed; for(var i=0;i<32;i++){sum++;var row=array[i&3];sum+=row.width+row.label.length;} return sum;
}
export function dynamicTrip(array, seed, limit) {
    var sum=seed; for(var i=0;i<limit;i++){var row=array[i&3];sum+=row.width+row.label.length;} return sum;
}
"#;
        let parsed=parse(source, ParseOptions::module()).unwrap();
        let ir=lower(&parsed);
        assert!(ir.is_wasm_supported(), "{:?}",ir.diagnostics);
        let artifact=emit_with_rooted_snapshot(&ir,PromiseRejectionPolicy::Ignore,&scriptw_intl::IntlCompilationProfile::Minimal).unwrap();
        for bytes in [artifact.bytes.as_slice(), artifact.runtime().unwrap().bytes()] {
            Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
        }
        let module=Module::read(&artifact.bytes,true);
        let runtime_bytes=artifact.runtime().unwrap().bytes();
        let runtime=Module::read(runtime_bytes,false);
        // Snapshot globals belong to R. Authenticate the complete encoded
        // shared type prefix and recursion-group boundaries before using its
        // concrete witness indices to classify P's instructions.
        assert!(module.types.len()>=runtime.types.len());
        assert_eq!(&module.types[..runtime.types.len()],&runtime.types,
            "P and R retain the same core types at the same indices");
        let program_groups=type_group_sizes(&artifact.bytes);
        let runtime_groups=type_group_sizes(runtime_bytes);
        assert!(program_groups.len()>=runtime_groups.len());
        assert_eq!(&program_groups[..runtime_groups.len()],&runtime_groups,
            "P's appended signatures preserve every shared recursive group");
        let inline=runtime.witness_type(GcSnapshotLayout::InlineCompactStorage);
        let values=runtime.witness_type(GcSnapshotLayout::ArrayValues);
        let string=runtime.witness_type(GcSnapshotLayout::String);
        let bodies=module_bodies(&artifact.bytes);
        for (name, expected, mask) in [
            ("run",1,63),("captured",1,3),("local",1,3),("split",1,3),("callback",0,3),("mutation",0,3),
            ("aliasEscape",0,3),("capturedAlias",0,3),("movingCounter",0,3),
            ("dynamicMask",0,3),("reversedIndex",0,3),("zeroTrip",0,3),("lexicalRow",0,3),
            ("prefixEffect",0,3),("dynamicTrip",0,3),
        ] {
            let variants=bodies.iter().filter(|body| body.name.starts_with(&format!("js::{name}#"))).collect::<Vec<_>>();
            assert!(!variants.is_empty(),"source body {name} is present");
            for variant in variants {
                let (_,body)=module.named(&variant.name);
                let regions=loops(body);
                let closed=regions.iter().filter(|region| read_only(region)
                    && region.iter().any(|op| matches!(op,Operator::StructGet{struct_type_index,..} if *struct_type_index==inline))
                    && region.iter().any(|op| matches!(op,Operator::ArrayGet{array_type_index} if *array_type_index==values))
                    && region.iter().any(|op| matches!(op,Operator::RefEq))
                    && region.iter().any(|op| matches!(op,Operator::F64Add))).collect::<Vec<_>>();
                assert_eq!(closed.len(),expected,"complete proof for {}",variant.name);
                if expected==0 { continue; }
                let region=closed[0];
                assert!(region.windows(2).any(|ops| matches!(ops,[Operator::I32Const{value},Operator::I32And] if *value==mask)),"masked physical row slot");
                assert!(region.iter().any(|op| matches!(op,Operator::RefTestNonNull{hty:HeapType::Concrete(UnpackedIndex::Module(ty))} if *ty==string)),"own primitive String is guarded");
                assert!(region.iter().any(|op| matches!(op,Operator::StructGet{struct_type_index,field_index} if *struct_type_index==string && *field_index==GcSnapshotField::StringUnits.index())),"String length uses its immutable UTF-16 payload");
                assert_eq!(region.windows(2).filter(|ops| matches!(ops,[Operator::ArrayLen,Operator::F64ConvertI32U])).count(),1,"one String length Number conversion");
                assert!(regions.iter().any(|region| region.iter().any(|op| matches!(op,Operator::Call{..}|Operator::CallRef{..}))),"the complete original loop remains for every guard miss");
            }
        }
    }).unwrap().join().unwrap();
}
