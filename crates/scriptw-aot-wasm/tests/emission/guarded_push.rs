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

fn incoming_empty_argv_allocation(
    body: &[Operator<'_>],
    signature: &wasmparser::FuncType,
    argv: u32,
) -> usize {
    let vectors = signature
        .params()
        .iter()
        .enumerate()
        .filter_map(|(index, ty)| {
            matches!(ty, ValType::Ref(reference) if reference.is_nullable()
            && reference.heap_type() == HeapType::Concrete(UnpackedIndex::Module(argv)))
            .then_some(index as u32)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        vectors.len(),
        1,
        "the source entry owns one nullable incoming argument List"
    );
    let vector = vectors[0];
    let count = vector
        .checked_sub(1)
        .expect("the incoming count precedes its vector");
    assert_eq!(signature.params()[count as usize], ValType::I64);
    let allocations = body
        .windows(14)
        .enumerate()
        .filter_map(|(start, ops)| {
            matches!(ops,
            [Operator::LocalGet { local_index: parameter }, Operator::LocalSet { local_index: saved },
             Operator::LocalGet { local_index: supplied }, Operator::RefIsNull, Operator::If { .. },
             Operator::LocalGet { local_index: supplied_count }, Operator::I64Eqz, Operator::I32Eqz,
             Operator::If { .. }, Operator::Unreachable, Operator::End,
             Operator::ArrayNewFixed { array_type_index, array_size: 0 },
             Operator::LocalSet { local_index: normalized }, Operator::Else]
                if *parameter == vector && saved == supplied && supplied == normalized
                    && *supplied_count == count && *array_type_index == argv)
            .then_some(start + 11)
        })
        .collect::<Vec<_>>();
    assert_eq!(allocations.len(), 1,
        "only a null incoming vector with zero declared arguments constructs the entry's empty List");
    allocations[0]
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
            let storage = runtime.witness_type(GcSnapshotLayout::Array);
            let code = runtime.witness_type(GcSnapshotLayout::Code);
            let context = runtime.witness_type(GcSnapshotLayout::Context);
            let (dispatch, _) = runtime.named("helper::proxy_call");
            // Nominal GC arrays can share an element layout. Bind the actual
            // JS dispatch operand, rather than guessing its type by shape.
            let signature = runtime.function_type(dispatch);
            let ValType::Ref(arguments) = signature.params()[6] else {
                panic!("two value triples precede the actual dispatch argument List");
            };
            assert!(!arguments.is_nullable(), "dispatch requires an actual argument List");
            let HeapType::Concrete(UnpackedIndex::Module(argv)) = arguments.heap_type() else {
                panic!("dispatch arguments retain their nominal GC array type");
            };
            let CompositeInnerType::Array(slots) = &runtime.types[argv as usize].composite_type.inner else {
                panic!("the actual dispatch argument operand is a GC array");
            };
            let StorageType::Val(ValType::Ref(element)) = slots.0.element_type else {
                panic!("the actual argument List contains typed GC references");
            };
            assert!(slots.0.mutable && !element.is_nullable()
                && element.heap_type() == HeapType::Concrete(UnpackedIndex::Module(stored)),
                "the actual dispatch List owns non-null canonical StoredValue entries");
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
                let (source, body) = program.named(&format!("js::{name}#"));
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
                let entry_argv = incoming_empty_argv_allocation(body, program.function_type(source), argv);
                let before_push = body[..direct].iter().enumerate()
                    .filter_map(|(index, op)| argv_allocation(op, argv).then_some(index))
                    .collect::<Vec<_>>();
                assert_eq!(before_push, [entry_argv],
                    "the source entry alone normalizes incoming null argv; successful fixed push creates no argument List");
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
