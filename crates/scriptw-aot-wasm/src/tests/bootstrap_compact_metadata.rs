//! Inspect the actual native bootstrap path, including its shared allocator.
use super::*;
use crate::gc_types::{
    ArrayCompactDataStorage, CompactDataStorage, FunctionObject, GcHeapType, GcNullability,
    GcSnapshotField, GlobalLedger, InlineCompactDataStorage, OrdinaryPropertyStorage,
    PropertyDescriptor, PropertyEntry, RuntimeModuleTypes, StoredValue,
};

fn type_index<T: GcHeapType>(schema: &crate::gc_types::RuntimeSchema) -> u32 {
    let wasm_encoder::HeapType::Concrete(index) = schema
        .reference_type::<T>(GcNullability::NonNullable)
        .heap_type
    else {
        panic!("registered GC type has a concrete index");
    };
    index
}

fn named_body<'a>(bytes: &'a [u8], index: u32) -> Vec<Operator<'a>> {
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
    panic!("registered helper has a body");
}

#[test]
fn native_bootstrap_allocates_compact_metadata_without_eager_property_records() {
    let artifact = emit_script("Math.max.name;").unwrap();
    let runtime = artifact.runtime().unwrap();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let names = function_names_in_bytes(runtime.bytes());
    let index = |name| {
        names
            .iter()
            .find_map(|(index, actual)| (actual == name).then_some(*index))
            .expect("actual runtime helper is registered")
    };
    let allocate = index("helper::native_function_allocate");
    let accessor_allocate = index("helper::intrinsic_accessor_allocate");
    let constructor_allocate = index("helper::intrinsic_constructor_allocate");
    let compact = index("helper::compact_literal_allocate");
    let bootstrap = named_body(
        runtime.bytes(),
        index("helper::realm_initialize_intrinsics"),
    );
    let inline_callables = bootstrap
        .iter()
        .filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == type_index::<FunctionObject>(schema)))
        .count();
    let shared_accessors = bootstrap
        .iter()
        .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == accessor_allocate))
        .count();
    let shared_constructors = bootstrap
        .iter()
        .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == constructor_allocate))
        .count();
    let accessor_allocator = named_body(runtime.bytes(), accessor_allocate);
    assert_eq!(
        accessor_allocator
            .iter()
            .filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == type_index::<FunctionObject>(schema)))
            .count(),
        1,
        "each shared accessor invocation constructs one fresh callable"
    );
    let native_headers = bootstrap
        .iter()
        .filter(|op| matches!(op, Operator::Call { function_index } if *function_index == allocate))
        .count();
    assert!(
        inline_callables > 0 && shared_accessors > 0 && shared_constructors > 0,
        "inspect the inline, accessor and constructor bootstrap paths"
    );
    assert_eq!(
        native_headers,
        inline_callables + shared_accessors + shared_constructors,
        "each native callable gets its own compact header"
    );
    let allocator = named_body(runtime.bytes(), allocate);
    assert_eq!(
        allocator
            .iter()
            .filter(
                |op| matches!(op, Operator::Call { function_index } if *function_index == compact)
            )
            .count(),
        1
    );
    let compact_allocator = named_body(runtime.bytes(), compact);
    for concrete in [
        type_index::<ArrayCompactDataStorage>(schema),
        type_index::<InlineCompactDataStorage>(schema),
    ] {
        assert_eq!(compact_allocator.iter().filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == concrete)).count(), 1, "each physical variant has one allocation site");
    }
    assert!(!compact_allocator.iter().any(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == type_index::<CompactDataStorage>(schema))), "the SHAPE-only sum base is never allocated");
    for operators in [&allocator, &compact_allocator] {
        for dictionary_type in [
            type_index::<OrdinaryPropertyStorage>(schema),
            type_index::<PropertyDescriptor>(schema),
            type_index::<PropertyEntry>(schema),
        ] {
            assert!(!operators.iter().any(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == dictionary_type)), "native metadata allocation must not eagerly create dictionary records");
        }
    }
    let stored_values = |operators: &[Operator<'_>]| {
        operators.iter().filter(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == type_index::<StoredValue>(schema))).count()
    };
    assert_eq!(
        stored_values(&allocator),
        0,
        "native name/length values remain unboxed"
    );
    assert_eq!(
        stored_values(&compact_allocator),
        0,
        "typed prototype references and compact metadata allocate no value boxes"
    );
    expect_valid_module(&artifact, 0);
}

#[test]
fn native_bootstrap_omits_empty_metadata_but_retains_thrower_and_source_publication() {
    let artifact = emit_script(
        "'use strict'; function Source(a) {} const arrow = (a, b) => a + b; print(Source.prototype.constructor === Source, arrow.name);",
    )
    .unwrap();
    let runtime = artifact.runtime().unwrap();
    let names = function_names_in_bytes(runtime.bytes());
    let index = |wanted| {
        names
            .iter()
            .find_map(|(index, name)| (name == wanted).then_some(*index))
            .expect("actual R helper is registered")
    };
    let metadata = index("helper::function_metadata_publish");
    let bootstrap = named_body(
        runtime.bytes(),
        index("helper::realm_initialize_intrinsics"),
    );
    assert_eq!(
        bootstrap
            .iter()
            .filter(
                |op| matches!(op, Operator::Call { function_index } if *function_index == metadata)
            )
            .count(),
        1,
        "ThrowTypeError retains publication; ordinary native headers are already complete"
    );

    // Resolve the actual R export and P import; their function ordinals are
    // separate domains and must never be equated by a test fixture.
    let mut exported_name = None;
    for payload in Parser::new(0).parse_all(runtime.bytes()) {
        if let Payload::ExportSection(exports) = payload.unwrap() {
            for export in exports {
                let export = export.unwrap();
                if export.kind == wasmparser::ExternalKind::Func && export.index == metadata {
                    assert!(exported_name.replace(export.name.to_owned()).is_none());
                }
            }
        }
    }
    let exported_name = exported_name.unwrap();
    let mut imported_functions = 0;
    let mut program_metadata = None;
    let mut source_calls = Vec::new();
    for payload in Parser::new(0).parse_all(&artifact.bytes) {
        match payload.unwrap() {
            Payload::ImportSection(imports) => {
                for import in imports.into_imports() {
                    let import = import.unwrap();
                    if matches!(
                        import.ty,
                        wasmparser::TypeRef::Func(_) | wasmparser::TypeRef::FuncExact(_)
                    ) {
                        if import.module == crate::RUNTIME_IMPORT_NAMESPACE
                            && import.name == exported_name
                        {
                            assert!(program_metadata.replace(imported_functions).is_none());
                        }
                        imported_functions += 1;
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                for op in body.get_operators_reader().unwrap() {
                    if let Operator::Call { function_index } = op.unwrap() {
                        source_calls.push(function_index);
                    }
                }
            }
            _ => {}
        }
    }
    let program_metadata = program_metadata.expect("P imports the real metadata helper");
    assert_eq!(
        source_calls
            .iter()
            .filter(|index| **index == program_metadata)
            .count(),
        2,
        "both the source constructor and strict nonconstructor arrow retain metadata publication"
    );
    expect_valid_module(&artifact, 0);
}

#[test]
fn inline_compact_reads_and_stores_reject_indices_outside_shape_before_field_access() {
    let artifact = emit_script("var record={left:1,right:2}; record.left=3; Object.getOwnPropertyDescriptor(record,'right');").unwrap();
    let runtime = artifact.runtime().unwrap();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let inline = type_index::<InlineCompactDataStorage>(schema);
    let names = function_names_in_bytes(runtime.bytes());
    let mut store = None;
    for name in [
        "helper::compact_data_store",
        "helper::ordinary_own_descriptor",
    ] {
        let index = names
            .iter()
            .find_map(|(index, actual)| (actual == name).then_some(*index))
            .expect("actual registered inline path");
        let body = named_body(runtime.bytes(), index);
        let first_access = body.iter().position(|op| matches!(op, Operator::StructGet { struct_type_index, field_index } | Operator::StructSet { struct_type_index, field_index } if *struct_type_index == inline && *field_index != GcSnapshotField::InlineCompactShape.index())).expect("inline slot fields are used");
        // Null refinement is unrelated to the integer bounds proof. Keep the
        // actual slot local and both unsigned comparisons in the checked span.
        let prefix = body[..first_access]
            .iter()
            .filter(|op| !matches!(op, Operator::RefAsNonNull))
            .collect::<Vec<_>>();
        assert!(prefix.windows(11).any(|ops| matches!(ops,
            [Operator::LocalGet { local_index: slot }, Operator::LocalGet { .. }, Operator::ArrayLen,
             Operator::I32GeU, Operator::LocalGet { local_index: same_slot }, Operator::I32Const { value: 2 },
             Operator::I32GeU, Operator::I32Or, Operator::If { .. }, Operator::Unreachable, Operator::End]
             if slot == same_slot)), "{name} must reject slot >= shape length or slot >= two before accessing fields");
        if name == "helper::compact_data_store" {
            store = Some(body);
        }
    }
    let store = store.unwrap();
    for (field, incoming_parameter) in [
        (GcSnapshotField::InlineScalar0.index(), 3),
        (GcSnapshotField::InlineScalar1.index(), 3),
        (GcSnapshotField::InlineReference0.index(), 4),
        (GcSnapshotField::InlineReference1.index(), 4),
    ] {
        let writes = store.iter().enumerate().filter_map(|(position, op)| matches!(op, Operator::StructSet { struct_type_index, field_index } if *struct_type_index == inline && *field_index == field).then_some(position)).collect::<Vec<_>>();
        assert_eq!(
            writes.len(),
            1,
            "each selected slot has one complete tuple write"
        );
        // ABI: storage, slot, then Value's i32 tag/i64 scalar/nullable eqref.
        // The typed parameter acquisition roots the incoming tuple in locals.
        let copied = store[..writes[0]]
            .windows(2)
            .find_map(|ops| match ops {
                [Operator::LocalGet {
                    local_index: source,
                }, Operator::LocalSet {
                    local_index: target,
                }] if *source == incoming_parameter => Some(*target),
                _ => None,
            })
            .expect("typed helper acquisition copies the incoming payload");
        assert_eq!(
            store[..writes[0]]
                .iter()
                .filter(
                    |op| matches!(op, Operator::LocalSet { local_index } if *local_index == copied)
                )
                .count(),
            1,
            "incoming payload remains intact until its field write"
        );
        // A nullable reference is written even for scalar tags, so an object
        // overwrite cannot retain its previous strong edge in either slot.
        assert!(
            matches!(store[writes[0] - 1], Operator::LocalGet { local_index } if local_index == copied),
            "field {field} is overwritten from its complete incoming payload"
        );
    }
    expect_valid_module(&artifact, 0);
}

#[test]
fn compact_allocator_selects_inline_only_for_one_or_two_keys_without_backing_arrays() {
    let artifact = emit_script("({left:1,right:2});").unwrap();
    let runtime = artifact.runtime().unwrap();
    let registered = RuntimeModuleTypes::register(crate::runtime_artifact::ModuleKind::Runtime(
        crate::RuntimeSourceCompilationProfile::Portable,
    ))
    .finalize_globals(GlobalLedger::new(), false, 0);
    let schema = registered.globals().schema();
    let allocate = function_names_in_bytes(runtime.bytes())
        .iter()
        .find_map(|(index, name)| (name == "helper::compact_literal_allocate").then_some(*index))
        .unwrap();
    let body = named_body(runtime.bytes(), allocate);
    let (start, length_local, lower, width) = body.windows(6).enumerate().find_map(|(start, ops)| match ops {
        [Operator::LocalGet { local_index }, Operator::I32Const { value: lower }, Operator::I32Sub,
         Operator::I32Const { value: width }, Operator::I32LtU, Operator::If { .. }] => Some((start, *local_index, *lower as u32, *width as u32)),
        _ => None,
    }).expect("actual allocator has an unsigned contiguous inline range predicate");
    let length_assignment = body[..start]
        .iter()
        .rposition(
            |op| matches!(op, Operator::LocalSet { local_index } if *local_index == length_local),
        )
        .unwrap();
    assert!(
        matches!(body[length_assignment - 1], Operator::ArrayLen),
        "selector uses the actual shape's key extent"
    );
    for length in (0u32..=16).chain([u32::MAX]) {
        assert_eq!(
            length.wrapping_sub(lower) < width,
            length == 1 || length == 2,
            "emitted predicate for length {length}"
        );
    }
    // Follow structured control rather than comparing instruction offsets or
    // physical local numbers. Inspect both actual allocation branches.
    let mut depth = 0;
    let mut otherwise = None;
    let mut end = None;
    for (position, op) in body.iter().enumerate().skip(start + 6) {
        match op {
            Operator::Block { .. } | Operator::Loop { .. } | Operator::If { .. } => depth += 1,
            Operator::Else if depth == 0 => otherwise = Some(position),
            Operator::End if depth == 0 => {
                end = Some(position);
                break;
            }
            Operator::End => depth -= 1,
            _ => {}
        }
    }
    let otherwise = otherwise.unwrap();
    let inline = &body[start + 6..otherwise];
    let array = &body[otherwise + 1..end.unwrap()];
    let allocations = |ops: &[Operator<'_>]| {
        ops.iter()
            .filter_map(|op| match op {
                Operator::StructNew { struct_type_index } => Some(*struct_type_index),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        allocations(inline),
        [type_index::<InlineCompactDataStorage>(schema)]
    );
    assert_eq!(
        allocations(array),
        [type_index::<ArrayCompactDataStorage>(schema)]
    );
    assert!(
        !inline.iter().any(|op| matches!(
            op,
            Operator::ArrayNew { .. }
                | Operator::ArrayNewDefault { .. }
                | Operator::ArrayNewFixed { .. }
        )),
        "inline objects allocate no tag, scalar or reference arrays"
    );
    assert_eq!(
        array
            .iter()
            .filter(|op| matches!(op, Operator::ArrayNew { .. }))
            .count(),
        1,
        "other extents retain one tag backing and lazy scalar/reference backings"
    );
    assert!(!body.iter().any(|op| matches!(op, Operator::StructNew { struct_type_index } if *struct_type_index == type_index::<CompactDataStorage>(schema))), "no path can allocate the common base");
    expect_valid_module(&artifact, 0);
}
