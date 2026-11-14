//! Proven declarative Calls read live cells; unresolved References keep their owner.
use crate::property_read_plans::Module;
use scriptw_aot_wasm::{GcHostField, emit};
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::{
    EnvironmentIdentifierOperationIr as Operation, EnvironmentIdentifierResolutionStart as Start,
    ExprIr, StatementIr, Strictness, lower,
};
use wasmparser::{
    CompositeInnerType, HeapType, Operator, StorageType, UnpackedIndex, ValType, Validator,
    WasmFeatures,
};

fn concrete(value: ValType) -> u32 {
    let ValType::Ref(reference) = value else {
        panic!("typed reference")
    };
    let HeapType::Concrete(UnpackedIndex::Module(index)) = reference.heap_type() else {
        panic!("nominal type")
    };
    index
}
fn calls(body: &[Operator<'_>], target: u32) -> usize {
    body.iter()
        .filter(|op| matches!(op,Operator::Call{function_index} if *function_index==target))
        .count()
}

#[test]
fn strict_proven_cell_calls_and_numeric_rows_keep_live_target_tdz_and_typed_dispatch() {
    std::thread::Builder::new().stack_size(64*1024*1024).spawn(|| {
        let rows=(0..64).map(|index|format!("sink({index});")).collect::<String>();
        let source=format!("export function diagnostics(){{return arguments[0];}} let sink; export function set(next){{sink=next;}} export function root(value){{return sink(value);}} export function owned(step,value){{function hold(){{return step;}}return step(value);}} export function make(step){{return function captured(value){{return step(value);}};}} export function rows(){{{rows}return 0;}} export function fallback(step=0){{return step(1);}}");
        let ir=lower(&parse(&source,ParseOptions::module()).unwrap());
        for name in ["root","owned","captured"] {
            let source=ir.script.as_ref().unwrap().functions.iter().find(|source|source.name==name).unwrap();
            let call=source.body.statements.iter().find_map(|statement|if let StatementIr::Return(value)=statement{Some(value)}else{None}).unwrap();
            let ExprIr::EnvironmentIdentifier(identifier)=&call.expr else{panic!("{name}: actual identifier Call")};
            assert!(matches!(identifier.strictness,Strictness::Strict));
            assert!(matches!(identifier.operation,Operation::Call{direct_eval:None,..}));
            assert!(matches!(identifier.resolution_start(),Start::Cell(_)),"{name}: admission uses actual analysis proof");
        }
        let artifact=emit(&ir).unwrap();
        for bytes in [artifact.bytes.as_slice(),artifact.runtime().unwrap().bytes()] {
            Validator::new_with_features(WasmFeatures::all()).validate_all(bytes).unwrap();
        }
        let p=Module::read(&artifact.bytes,true);
        let r=Module::read(artifact.runtime().unwrap().bytes(),true);
        let (get,_) =r.named("helper::environment_identifier_get_strict");
        let generic=p.imported_runtime_function(&r,get);
        let cell=concrete(r.function_type(get).params()[4]);
        let CompositeInnerType::Struct(layout)=&r.types[cell as usize].composite_type.inner else{panic!("binding cell")};
        let tag=GcHostField::BindingCellTag.definition().index();
        let initialized=layout.fields.iter().enumerate().filter_map(|(index,field)|
            (field.mutable && field.element_type==StorageType::Val(ValType::I32) && index as u32!=tag).then_some(index as u32)
        ).collect::<Vec<_>>();
        assert_eq!(initialized.len(),1,"actual mutable initialization flag is distinct from tag");
        let target=layout.fields.iter().enumerate().filter_map(|(index,field)| match field.element_type {
            StorageType::Val(ValType::Ref(reference)) if field.mutable && reference.is_nullable() && reference.heap_type()==HeapType::Concrete(UnpackedIndex::Module(cell)) => Some(index as u32),
            _=>None,
        }).collect::<Vec<_>>();
        assert_eq!(target.len(),1,"actual sole import-target cell edge");
        let (tdz,_)=r.named("helper::lexical_binding_uninitialized");
        let tdz=p.imported_runtime_function(&r,tdz);
        for name in ["root","owned","captured","rows"] {
            let (_,body)=p.named(&format!("js::{name}#"));
            assert_eq!(calls(body,generic),0,"{name}: no generic Reference GetValue");
            for field in [initialized[0],GcHostField::BindingCellScalar.definition().index(),tag,GcHostField::BindingCellReference.definition().index()] {
                assert!(body.iter().any(|op|matches!(op,Operator::StructGet{struct_type_index,field_index} if *struct_type_index==cell && *field_index==field)),"{name}: actual live field{field}");
            }
            // This module imports nothing, so no cell's TARGET is ever linked:
            // the read follows no import edge.
            assert!(!body.iter().any(|op|matches!(op,Operator::StructGet{struct_type_index,field_index} if *struct_type_index==cell && *field_index==target[0])),"{name}: no import TARGET in a program without imports");
            assert!(calls(body,tdz)>0,"{name}: TDZ failure remains a whole abrupt completion");
        }
        let (_,fallback)=p.named("js::fallback#");
        assert!(calls(fallback,generic)>0,"complex parameter owner retains generic Reference dispatch");
        let (_,rows)=p.named("js::rows#");
        let (call,_)=r.named("helper::function_call");
        let argv=concrete(r.function_type(call).params()[6]);
        let allocation=rows.iter().position(|op|matches!(op,Operator::ArrayNew{array_type_index}|Operator::ArrayNewFixed{array_type_index,..} if *array_type_index==argv)).expect("numeric row constructs its existing argv");
        let read=rows.iter().position(|op|matches!(op,Operator::StructGet{struct_type_index,field_index} if *struct_type_index==cell && *field_index==initialized[0])).unwrap();
        let tdz_branch=rows.iter().position(|op|matches!(op,Operator::Call{function_index} if *function_index==tdz)).unwrap();
        assert!(read<tdz_branch && tdz_branch<allocation,"live callee and TDZ precede every argument vector");
    }).unwrap().join().unwrap();
}
