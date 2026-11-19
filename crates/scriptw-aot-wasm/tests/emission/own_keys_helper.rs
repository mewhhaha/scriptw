//! Real R/P signatures and call sites keep internal Lists out of public Arrays.
use crate::{linked_bodies::module_bodies, property_read_plans::Module};
use scriptw_aot_wasm::WasmArtifact;
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
use wasmparser::{
    CompositeInnerType, HeapType, Operator, StorageType, UnpackedIndex, ValType, Validator,
    WasmFeatures,
};

fn emit_script(source: &'static str) -> WasmArtifact {
    std::thread::Builder::new()
        .name("own-keys-artifact".to_owned())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let parsed = parse(source, ParseOptions::script()).expect("fixture parses");
            scriptw_aot_wasm::emit(&lower_with_host_surface_policy(
                &parsed,
                HostSurfacePolicy::Test262,
            ))
            .expect("fixture emits")
        })
        .unwrap()
        .join()
        .unwrap()
}

fn validate(artifact: &WasmArtifact) {
    for bytes in [
        artifact.runtime().unwrap().bytes(),
        artifact.bytes.as_slice(),
    ] {
        Validator::new_with_features(WasmFeatures::all())
            .validate_all(bytes)
            .expect("whole Throws and the typed key-list result validate");
    }
}

fn call_count(operators: &[Operator<'_>], callee: u32) -> usize {
    operators
        .iter()
        .filter(|operator| {
            matches!(operator, Operator::Call { function_index } if *function_index == callee)
        })
        .count()
}

#[test]
fn internal_own_keys_share_one_recursive_list_helper_with_whole_throw_results() {
    let artifact = emit_script(
        "function collect(source) { var out=[]; for (var key in source) out.push(key); return {...source}; } collect({a:1});",
    );
    validate(&artifact);
    let bytes = artifact.runtime().unwrap().bytes();
    let runtime = Module::read(bytes, true);
    let (helper, helper_body) = runtime.named("helper::object_own_keys");
    let (advance, advance_body) = runtime.named("helper::for_in_advance");
    let (reflect, public_body) = runtime.named("builtin::Reflect.ownKeys");
    let (define, _) = runtime.named("helper::object_define_data");
    assert!(
        call_count(helper_body, helper) >= 2,
        "missing-trap fallback and target validation recurse through the typed boundary"
    );
    assert_eq!(call_count(advance_body, helper), 1);
    assert_eq!(call_count(public_body, helper), 1);
    assert!(call_count(public_body, define) > 0);
    for operators in [helper_body, advance_body] {
        assert_eq!(
            call_count(operators, define),
            0,
            "private key Lists do not materialize public Array properties"
        );
        assert!(
            !operators.iter().any(|operator| matches!(operator,
            Operator::Call {function_index} | Operator::RefFunc {function_index}
                if *function_index == reflect)),
            "internal enumeration never materializes or invokes the public OwnKeys callable"
        );
    }

    // Discover the private table through its real cursor consumer, rather than
    // relying on a private schema API or a hardcoded runtime type index.
    let ValType::Ref(cursor) = runtime.function_type(advance).params()[0] else {
        panic!("ForInAdvance takes its actual cursor reference");
    };
    let HeapType::Concrete(UnpackedIndex::Module(cursor)) = cursor.heap_type() else {
        panic!("the cursor has a concrete GC type");
    };
    let CompositeInnerType::Struct(cursor) = &runtime.types[cursor as usize].composite_type.inner
    else {
        panic!("the cursor is a GC record");
    };
    let tables = cursor
        .fields
        .iter()
        .filter_map(|field| {
            let StorageType::Val(ValType::Ref(reference)) = field.element_type else {
                return None;
            };
            if !reference.is_nullable() {
                return None;
            }
            let HeapType::Concrete(UnpackedIndex::Module(index)) = reference.heap_type() else {
                return None;
            };
            if matches!(
                runtime.types[index as usize].composite_type.inner,
                CompositeInnerType::Array(_)
            ) {
                return Some(ValType::Ref(reference));
            }
            None
        })
        .collect::<Vec<_>>();
    assert_eq!(
        tables.len(),
        1,
        "one nullable accepted-key snapshot in the cursor"
    );
    let expected = tables[0];
    let signature = runtime.function_type(helper);
    assert_eq!(
        signature.params().len(),
        6,
        "target triple, caller Realm, Environment and Program"
    );
    assert_eq!(
        signature.results(),
        [
            wasmparser::ValType::I32,
            wasmparser::ValType::I64,
            wasmparser::ValType::Ref(wasmparser::RefType::EQREF),
            wasmparser::ValType::I32,
            wasmparser::ValType::I32,
            expected,
        ],
        "the whole Completion precedes a separately typed nullable List"
    );
}

#[test]
fn copy_data_properties_imports_the_same_typed_own_keys_export() {
    let artifact = emit_script("function copy(source) { return {...source}; } copy({a:1});");
    validate(&artifact);
    let runtime = Module::read(artifact.runtime().unwrap().bytes(), true);
    let program = Module::read(&artifact.bytes, true);
    let (helper, _) = runtime.named("helper::object_own_keys");
    let imported = program.imported_runtime_function(&runtime, helper);
    assert_eq!(
        program.function_type(imported),
        runtime.function_type(helper)
    );
    let copies = module_bodies(&artifact.bytes)
        .into_iter()
        .filter(|body| body.name.starts_with("js::copy#"))
        .collect::<Vec<_>>();
    assert!(!copies.is_empty());
    for copy in copies {
        let (_, code) = program.named(&copy.name);
        assert_eq!(call_count(code, imported), 1);
    }
}
