//! Inspect the shared physical R allocation owner and its actual bootstrap callers.
use super::*;
use crate::gc_types::{
    ExecutableCode, FunctionContext, FunctionObject, GcHeapType, GcNullability, GlobalLedger,
    OrdinaryObject, RealmRecord, RuntimeModuleTypes, RuntimeSchema,
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

fn body(bytes: &[u8], index: u32) -> Vec<Operator<'_>> {
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
    panic!("named R helper has a real defined body");
}

#[test]
fn eager_accessor_allocation_shares_the_actual_typed_context_and_function_tail() {
    let artifact =
        emit_script("Object.getOwnPropertyDescriptor(ArrayBuffer.prototype,'byteLength').get;")
            .unwrap();
    expect_valid_module(&artifact, 1);
    let runtime = artifact.runtime().unwrap();
    let bytes = runtime.bytes();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let names = function_names_in_bytes(bytes);
    let named = |requested: &str| {
        let found = names
            .iter()
            .filter_map(|(&index, name)| (name == requested).then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(found.len(), 1, "one physical R body for {requested}");
        found[0]
    };
    let allocate = named("helper::intrinsic_accessor_allocate");
    let resolve = named("helper::pooled_string_resolve");
    let metadata = named("helper::function_metadata_publish");
    let header = named("helper::native_function_allocate");
    let bootstrap = body(bytes, named("helper::realm_initialize_intrinsics"));
    let helper = body(bytes, allocate);
    let allocations = |index| {
        helper.iter().enumerate().filter_map(|(offset, op)| {
        matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == index).then_some(offset)
    }).collect::<Vec<_>>()
    };
    let context = allocations(type_index::<FunctionContext>(schema));
    let callable = allocations(type_index::<FunctionObject>(schema));
    assert_eq!(
        context.len(),
        1,
        "one original context per allocation invocation"
    );
    assert_eq!(
        callable.len(),
        1,
        "one original callable per allocation invocation"
    );
    let source = helper
        .iter()
        .enumerate()
        .filter_map(|(offset, op)| {
            matches!(op, Operator::Call { function_index } if *function_index == resolve)
                .then_some(offset)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        source.len(),
        1,
        "source stays with the checked pooled-string resolver"
    );
    let close = helper
        .iter()
        .enumerate()
        .filter_map(|(offset, op)| {
            matches!(op, Operator::StructSet { struct_type_index, field_index }
            if *struct_type_index == type_index::<FunctionContext>(schema)
                && *field_index == crate::GcHostField::ContextActiveFunction.definition().index())
            .then_some(offset)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        close.len(),
        1,
        "close the actual context/function cycle once"
    );
    assert!(
        context[0] < source[0] && source[0] < callable[0] && callable[0] < close[0],
        "retain context, source resolution, callable, ACTIVE_FUNCTION publication order"
    );
    assert!(
        !helper
            .iter()
            .any(|op| matches!(op, Operator::Call { function_index }
        if *function_index == header || *function_index == metadata)),
        "native headers and any nonempty post-cycle metadata remain caller-owned"
    );
    let calls = |target| {
        bootstrap
            .iter()
            .filter(
                |op| matches!(op, Operator::Call { function_index } if *function_index == target),
            )
            .count()
    };
    assert!(
        calls(allocate) > 1,
        "actual Realm initialization shares this physical R helper"
    );
    assert!(
        calls(header) >= calls(allocate),
        "eager callers still construct independent native headers"
    );
    assert_eq!(
        calls(metadata),
        1,
        "only the thrower's non-extensibility still needs metadata publication"
    );

    // Validate the physical helper's declared ABI using actual R exports and
    // registered GC identities, rather than assuming compact P import indices.
    let mut signatures = Vec::new();
    let mut declarations = Vec::new();
    let mut exported = false;
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.unwrap() {
            Payload::TypeSection(groups) => {
                for group in groups {
                    signatures.extend(group.unwrap().into_types());
                }
            }
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    if let wasmparser::TypeRef::Func(index)
                    | wasmparser::TypeRef::FuncExact(index) = import.unwrap().ty
                    {
                        declarations.push(index);
                    }
                }
            }
            Payload::FunctionSection(functions) => {
                declarations.extend(functions.into_iter().map(Result::unwrap))
            }
            Payload::ExportSection(exports) => {
                for export in exports {
                    let export = export.unwrap();
                    exported |=
                        export.kind == wasmparser::ExternalKind::Func && export.index == allocate;
                }
            }
            _ => {}
        }
    }
    assert!(
        exported,
        "the declared runtime helper is exported by actual R"
    );
    let wasmparser::CompositeInnerType::Func(signature) = &signatures
        [declarations[allocate as usize] as usize]
        .composite_type
        .inner
    else {
        panic!("allocator must have its registered function signature");
    };
    let reference = |index| {
        wasmparser::ValType::Ref(
            wasmparser::RefType::new(
                false,
                wasmparser::HeapType::Concrete(wasmparser::UnpackedIndex::Module(index)),
            )
            .unwrap(),
        )
    };
    assert_eq!(
        signature.params(),
        [
            reference(type_index::<OrdinaryObject>(schema)),
            reference(type_index::<ExecutableCode>(schema)),
            reference(type_index::<RealmRecord>(schema)),
            wasmparser::ValType::I32
        ]
    );
    assert_eq!(
        signature.results(),
        [reference(type_index::<FunctionObject>(schema))]
    );
}
