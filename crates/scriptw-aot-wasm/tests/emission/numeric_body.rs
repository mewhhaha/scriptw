//! Actual assigned-callable coverage, without a speculative call-site target.
use crate::{property_read_plans::Module, scalar_masked_rows::type_group_sizes};
use scriptw_aot_wasm::{emit, GcHostField};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower, FunctionIr};
use wasmparser::{
    CompositeInnerType, HeapType, Operator, UnpackedIndex, ValType, Validator, WasmFeatures,
};

fn name(source: &FunctionIr) -> String {
    if source.name.is_empty() {
        return format!("js::{}", source.id);
    }
    format!("js::{}#{}", source.name, source.id)
}
fn concrete(value: ValType) -> u32 {
    let ValType::Ref(value) = value else {
        panic!("actual typed reference");
    };
    let HeapType::Concrete(UnpackedIndex::Module(index)) = value.heap_type() else {
        panic!("canonical module type");
    };
    index
}
fn writes(body: &[Operator<'_>], cell: u32) -> Vec<usize> {
    let field = GcHostField::BindingCellScalar.definition().index();
    body.iter().enumerate().filter_map(|(at,op)| matches!(op,Operator::StructSet {struct_type_index,field_index} if *struct_type_index==cell && *field_index==field).then_some(at)).collect()
}
#[test]
fn unchanged_assigned_step_uses_its_existing_entry_and_one_immediate_cell_write() {
    std::thread::Builder::new().name("numeric-body-emission".into()).stack_size(64*1024*1024).spawn(|| {
        for (source, expected) in [
            (include_str!("fixtures/numeric_body_args_unused.mjs"),1),
            (include_str!("fixtures/numeric_body_frame_compact.mjs"),2),
        ] {
            let ir=lower(&parse(source,ParseOptions::module()).unwrap());
            assert!(ir.is_wasm_supported(),"{:?}",ir.diagnostics);
            let functions:Vec<_>=ir.script.as_ref().unwrap().functions.iter().filter(|function|
                function.captured_bindings.iter().any(|binding|binding.source_name=="tick")
                && function.params.len()<=2).collect();
            assert_eq!(functions.len(),expected,"actual scored closure identities");
            let artifact=emit(&ir).unwrap();
            for bytes in [artifact.bytes.as_slice(),artifact.runtime().unwrap().bytes()] {
                Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
            }
            let p=Module::read(&artifact.bytes,true);
            let r=Module::read(artifact.runtime().unwrap().bytes(),true);
            assert_eq!(&p.types[..r.types.len()],&r.types,"canonical full type prefix");
            let pg=type_group_sizes(&artifact.bytes);let rg=type_group_sizes(artifact.runtime().unwrap().bytes());
            assert_eq!(&pg[..rg.len()],&rg,"canonical recursive groups");
            let (get,_)=r.named("helper::environment_identifier_get_strict");
            let cell=concrete(r.function_type(get).params()[4]);
            let (cached,_)=r.named("helper::cached_property_read");
            let cached=p.imported_runtime_function(&r,cached);
            for source in functions {
                assert!(source.strict);
                let (index,body)=p.named(&name(source));
                assert_eq!(p.function_type(index).params().len(),15,"the original local entry remains canonical");
                let actual=writes(body,cell);
                if expected==1 {
                    assert_eq!(actual.len(),1,"one Number prefix writes through the existing canonical cell owner");
                    assert!(body[..actual[0]].iter().any(|op|matches!(op,Operator::F64Add)));
                    assert!(body[..actual[0]].iter().any(|op|matches!(op,Operator::BrIf{..})),"preflight can decline before the write");
                    assert_eq!(body.iter().filter(|op|matches!(op,Operator::Call{function_index}if *function_index==cached)).count(),2,"both unchanged generic field reads remain in fallback");
                    let (_,run)=p.named("js::run#");
                    assert!(run.iter().any(|op| match op {
                        Operator::CallRef{type_index} => match &p.types[*type_index as usize].composite_type.inner {
                            CompositeInnerType::Func(signature)=>signature==p.function_type(index), _=>false,
                        }, _=>false,
                    }),"the unchanged dynamic source call actually has a local-entry route");
                    let (_,adapter)=p.named(&format!("js-arguments-adapter::{}#{}",source.name,source.id));
                    assert_eq!(adapter.iter().filter(|op|matches!(op,Operator::ReturnCall{function_index}if *function_index==index)).count(),1,"vector callers enter the same body exactly once");
                } else {
                    assert!(actual.is_empty(),"branching frame closures retain the original body");
                }
            }
        }
    }).unwrap().join().unwrap();
}
