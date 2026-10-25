//! Inspect actual GC allocations and the immutable snapshot operands they use.
use super::*;
use crate::gc_types::{
    FunctionContext, FunctionObject, GcHeapType, GcNullability, GcStructHeapType, GlobalLedger,
    PropertyDescriptor, RuntimeModuleTypes, RuntimeSchema, StoredValue,
};

fn type_index<T: GcHeapType>(schema: &RuntimeSchema) -> u32 {
    let wasm_encoder::HeapType::Concrete(index) = schema
        .reference_type::<T>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("registered GC type must have a concrete index");
    };
    index
}

fn named_body<'a>(bytes: &'a [u8], name: &str) -> Vec<Operator<'a>> {
    let index = function_names_in_bytes(bytes)
        .into_iter()
        .find_map(|(index, actual)| (actual == name).then_some(index))
        .expect("actual runtime helper must have its name");
    let mut next = imported_function_count_in_bytes(bytes);
    for payload in Parser::new(0).parse_all(bytes) {
        if let Payload::CodeSectionEntry(body) = payload.unwrap() {
            if next == index {
                return body
                    .get_operators_reader()
                    .unwrap()
                    .into_iter()
                    .map(Result::unwrap)
                    .collect();
            }
            next += 1;
        }
    }
    panic!("named helper must have a body");
}

#[test]
fn runtime_flag_data_descriptors_allocate_values_without_absent_accessor_snapshots() {
    let artifact = emit_script("Array;").unwrap();
    let runtime = artifact.runtime().unwrap();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let stored = type_index::<StoredValue>(schema);
    let descriptor = type_index::<PropertyDescriptor>(schema);
    let operators = named_body(runtime.bytes(), "helper::function_metadata_publish");
    let allocations = |ty| {
        operators
            .iter()
            .filter(|operator| {
                matches!(operator,
        Operator::StructNew { struct_type_index } if *struct_type_index == ty)
            })
            .count()
    };
    // name/length, optional legacy caller/arguments, optional prototype and its
    // constructor back reference are six distinct mutable descriptor sites.
    assert_eq!(allocations(descriptor), 6);
    assert_eq!(
        allocations(stored), allocations(descriptor) + 1,
        "each value owns its snapshot; only the new public prototype needs a second cached snapshot"
    );
    expect_valid_module(&artifact, 0);
}

#[test]
fn native_bootstrap_uses_immutable_undefined_for_home_and_initial_prototype() {
    let artifact = emit_script("Array;").unwrap();
    let runtime = artifact.runtime().unwrap();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let stored = type_index::<StoredValue>(schema);
    let context = type_index::<FunctionContext>(schema);
    let callable = type_index::<FunctionObject>(schema);
    let mut next_global = 0;
    let mut undefined = Vec::new();
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        match payload.unwrap() {
            Payload::ImportSection(imports) => {
                next_global += imports
                    .into_imports()
                    .map(Result::unwrap)
                    .filter(|import| matches!(import.ty, wasmparser::TypeRef::Global(_)))
                    .count() as u32;
            }
            Payload::GlobalSection(globals) => {
                for global in globals {
                    let global = global.unwrap();
                    let operators = global
                        .init_expr
                        .get_operators_reader()
                        .into_iter()
                        .map(Result::unwrap)
                        .collect::<Vec<_>>();
                    if let [Operator::I32Const { value }, Operator::I64Const { value: 0 }, Operator::RefNull { .. }, Operator::StructNew { struct_type_index }, Operator::End] =
                        &operators[..]
                    {
                        if *value == crate::WasmRuntimeValueTag::Undefined as i32
                            && *struct_type_index == stored
                        {
                            assert!(!global.ty.mutable);
                            undefined.push(next_global);
                        }
                    }
                    next_global += 1;
                }
            }
            _ => {}
        }
    }
    assert_eq!(
        undefined.len(),
        1,
        "one immutable runtime-owned undefined snapshot"
    );
    let operators = named_body(runtime.bytes(), "helper::realm_initialize_intrinsics");
    let mut local_is_undefined = BTreeMap::new();
    let mut contexts = 0;
    let mut callables = 0;
    for (position, operator) in operators.iter().enumerate() {
        if let Operator::LocalSet { local_index } = operator {
            local_is_undefined.insert(
                *local_index,
                position > 0
                    && matches!(operators[position - 1],
                Operator::GlobalGet { global_index } if global_index == undefined[0]),
            );
        }
        let Operator::StructNew { struct_type_index } = operator else {
            continue;
        };
        let fields = if *struct_type_index == context {
            contexts += 1;
            FunctionContext::FIELD_COUNT
        } else if *struct_type_index == callable {
            callables += 1;
            FunctionObject::FIELD_COUNT
        } else {
            continue;
        };
        // Both layouts' fifth field is the snapshot under test. Constructor
        // operands are scalar constants/local references; nullable refinements
        // are stack-neutral and do not add an operand.
        let operand = operators[..position]
            .iter()
            .rev()
            .filter(|operator| !matches!(operator, Operator::RefAsNonNull))
            .nth((fields - 1 - 4) as usize)
            .unwrap();
        let Operator::LocalGet { local_index } = operand else {
            panic!("snapshot operand must be rooted")
        };
        assert_eq!(
            local_is_undefined.get(local_index),
            Some(&true),
            "native snapshot must load the immutable global"
        );
    }
    assert!(contexts > 0);
    assert_eq!(
        contexts, callables,
        "mutable contexts/callables remain independently allocated"
    );
}
