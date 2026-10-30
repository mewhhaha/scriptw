//! Fixed push entries retain real code/Realm proofs and allocate argv only on
//! the saved-operand fallback, with canonical packed publication in R.
use super::property_read_plans::{calls, Module};
use scriptw_aot_wasm::{
    emit_with_rooted_snapshot, GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy,
};
use scriptw_front::{parse, ParseOptions};
use scriptw_ir::lower;
use wasmparser::{
    CompositeInnerType, HeapType, Operator, StorageType, UnpackedIndex, ValType, Validator,
    WasmFeatures,
};

fn frame_end(body: &[Operator<'_>], start: usize) -> usize {
    let mut depth = 1;
    for (offset, op) in body[start + 1..].iter().enumerate() {
        match op {
            Operator::If { .. }
            | Operator::Block { .. }
            | Operator::Loop { .. }
            | Operator::TryTable { .. }
            | Operator::Try { .. } => depth += 1,
            Operator::End => {
                depth -= 1;
                if depth == 0 {
                    return start + 1 + offset;
                }
            }
            _ => {}
        }
    }
    panic!("encoded control frame is balanced");
}

fn argv_allocation(op: &Operator<'_>, array: u32) -> bool {
    matches!(op, Operator::ArrayNew { array_type_index }
        | Operator::ArrayNewDefault { array_type_index }
        | Operator::ArrayNewFixed { array_type_index, .. }
        if *array_type_index == array)
}

#[test]
fn fixed_push_proves_actual_native_then_keeps_argument_allocations_in_fallback() {
    std::thread::Builder::new()
        .name("guarded-push-emission".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let lowered = lower(&parse(
                "function one(v){var a=[];var result=a.push(v);return result;} \
                 function two(v){var a=[];var result=a.push(v,v);return result;} \
                 function three(v){var a=[];var result=a.push(v,v,v);return result;} \
                 function control(v){var a=[];return a;} \
                 function zero(v){var a=[];var result=a.push();return result;} \
                 function four(v){var a=[];var result=a.push(v,v,v,v);return result;} \
                 [one,two,three,control,zero,four];",
                ParseOptions::script(),
            ).unwrap());
            let artifact = emit_with_rooted_snapshot(
                &lowered,
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            ).unwrap();
            let runtime_bytes = artifact.runtime().unwrap().bytes();
            for bytes in [&artifact.bytes[..], runtime_bytes] {
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(bytes).unwrap();
            }
            let runtime = Module::read(runtime_bytes, true);
            let program = Module::read(&artifact.bytes, true);
            let stored = runtime.witness_type(GcSnapshotLayout::Stored);
            let array = runtime.witness_type(GcSnapshotLayout::Array);
            let storage = runtime.witness_type(GcSnapshotLayout::ArrayStorage);
            let code = runtime.witness_type(GcSnapshotLayout::Code);
            let context = runtime.witness_type(GcSnapshotLayout::Context);
            let argv = runtime.types.iter().enumerate().filter_map(|(index, ty)| {
                let CompositeInnerType::Array(array) = &ty.composite_type.inner else { return None; };
                let StorageType::Val(ValType::Ref(element)) = array.0.element_type else { return None; };
                (!element.is_nullable()
                    && element.heap_type() == HeapType::Concrete(UnpackedIndex::Module(stored)))
                    .then_some(index as u32)
            }).collect::<Vec<_>>();
            assert_eq!(argv.len(), 1, "the canonical non-null StoredValue List has one GC type");
            let argv = argv[0];
            let (dispatch, _) = runtime.named("helper::proxy_call");
            let dispatch = program.imported_runtime_function(&runtime, dispatch);
            let (publish, _) = runtime.named("helper::array_indexed_publish_default_data");
            let (descriptor, _) = runtime.named("helper::ordinary_own_descriptor");
            let (_, control) = program.named("js::control#");
            let source_allocations = |body: &[Operator<'_>]| body.iter().filter(|op| matches!(op,
                Operator::StructNew { struct_type_index } if *struct_type_index == stored)).count();
            let mut kernels = Vec::new();
            for (name, arity) in [("one", 1), ("two", 2), ("three", 3)] {
                let (definition, kernel) = runtime.named(&format!("helper::array_packed_push{arity}"));
                let imported = program.imported_runtime_function(&runtime, definition);
                kernels.push(imported);
                let (_, body) = program.named(&format!("js::{name}#"));
                let direct = body.iter().position(|op| matches!(op,
                    Operator::Call { function_index } if *function_index == imported))
                    .expect("the source call reaches its fixed-arity native entry");
                assert!(body[..direct].iter().filter(|op| matches!(op,
                    Operator::StructGet { struct_type_index, .. } if *struct_type_index == code)).count() >= 2,
                    "actual immutable code kind and builtin identity guard the native entry");
                assert!(body[..direct].iter().any(|op| matches!(op,
                    Operator::StructGet { struct_type_index, field_index }
                        if *struct_type_index == context && *field_index == GcSnapshotField::ContextRealm.index())),
                    "the saved actual callable supplies the execution Realm");
                assert!(body[..direct].iter().any(|op| matches!(op, Operator::RefEq)),
                    "the actual execution Realm is compared with the caller Realm");
                assert_eq!(source_allocations(&body[..direct]), source_allocations(control),
                    "successful fixed push allocates no per-argument StoredValue snapshots");
                assert!(!body[..direct].iter().any(|op| argv_allocation(op, argv)),
                    "successful fixed push creates no argument List");
                let fallback = body.windows(3).enumerate().find_map(|(start, ops)| {
                    let [Operator::LocalGet { .. }, Operator::I32Eqz, Operator::If { .. }] = ops else { return None; };
                    let end = frame_end(body, start + 2);
                    (start > direct && calls(&body[start + 3..end], dispatch))
                        .then_some(&body[start + 3..end])
                }).expect("a failed handled flag owns the ordinary saved-operand fallback");
                assert_eq!(source_allocations(fallback), arity,
                    "only fallback snapshots each once-evaluated argument");
                assert!(fallback.iter().any(|op| argv_allocation(op, argv)),
                    "the fallback alone materializes the ordinary argument List");
                let first_publication = kernel.iter().position(|op| matches!(op,
                    Operator::Call { function_index } if *function_index == publish)).unwrap();
                assert_eq!(kernel.iter().filter(|op| matches!(op,
                    Operator::Call { function_index } if *function_index == publish)).count(), arity,
                    "the native entry publishes each rooted operand through the canonical owner");
                assert!(kernel[..first_publication].iter().any(|op| matches!(op,
                    Operator::RefTestNonNull { hty: HeapType::Concrete(UnpackedIndex::Module(index)) }
                        if *index == array)), "actual concrete Array admission precedes publication");
                for (owner, field) in [(array, GcSnapshotField::ArrayLengthWritable),
                    (storage, GcSnapshotField::ArrayCount)] {
                    assert!(kernel[..first_publication].iter().any(|op| matches!(op,
                        Operator::StructGet { struct_type_index, field_index }
                            if *struct_type_index == owner && *field_index == field.index())),
                        "live {field:?} is checked before publication");
                }
                assert!(calls(&kernel[..first_publication], descriptor),
                    "prototype indexed descriptors remain part of shared admission");
                assert!(!kernel.iter().any(|op| argv_allocation(op, argv)),
                    "the native entry does not rebuild an argument vector internally");
            }
            for name in ["js::zero#", "js::four#"] {
                let (_, body) = program.named(name);
                assert!(kernels.iter().all(|kernel| !calls(body, *kernel)),
                    "unsupported fixed arities retain the general call from the start");
                assert!(calls(body, dispatch));
            }
        }).unwrap().join().unwrap();
}
