//! Internal definitions admit canonical storage without expanding source accessors.
use super::{
    ordinary_receiver_definition::concrete_if_arms,
    property_read_plans::{Module, calls},
    scalar_masked_rows::type_group_sizes,
};
use scriptw_aot_wasm::{
    GcSnapshotField, GcSnapshotLayout, PromiseRejectionPolicy, RUNTIME_IMPORT_NAMESPACE,
    emit_with_rooted_snapshot,
};
use scriptw_front::{ParseOptions, parse};
use scriptw_ir::lower;
use wasmparser::{ExternalKind, Operator, Parser, Payload, TypeRef, Validator, WasmFeatures};

/// A forbidden helper may have no P import at all. Resolve its real exported
/// R identity before looking for calls, rather than inventing a function index.
fn optional_runtime_import(program: &[u8], runtime: &[u8], definition: u32) -> Option<u32> {
    let mut exports = Vec::new();
    for payload in Parser::new(0).parse_all(runtime) {
        if let Payload::ExportSection(section) = payload.expect("R section decodes") {
            for export in section {
                let export = export.expect("R export decodes");
                if export.kind == ExternalKind::Func && export.index == definition {
                    exports.push(export.name);
                }
            }
        }
    }
    if exports.is_empty() {
        return None;
    }
    let mut next_function = 0;
    let mut found = None;
    for payload in Parser::new(0).parse_all(program) {
        if let Payload::ImportSection(section) = payload.expect("P section decodes") {
            for import in section.into_imports() {
                let import = import.expect("P import decodes");
                if matches!(import.ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) {
                    if import.module == RUNTIME_IMPORT_NAMESPACE && exports.contains(&import.name) {
                        assert!(
                            found.replace(next_function).is_none(),
                            "one actual runtime owner import"
                        );
                    }
                    next_function += 1;
                }
            }
        }
    }
    found
}

#[test]
fn internal_definitions_skip_carriers_without_copying_array_code_into_source_accessors() {
    std::thread::Builder::new()
        .name("internal-definition".into())
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            let ir = lower(
                &parse(
                    "export function literal(key, value) { return { [key]: value, \
                     get pair() { return value; }, set pair(next) { value = next; } }; } \
                     export function from(source) { return Array.from(source); }",
                    ParseOptions::module(),
                )
                .unwrap(),
            );
            let artifact = emit_with_rooted_snapshot(
                &ir,
                PromiseRejectionPolicy::Ignore,
                &scriptw_intl::IntlCompilationProfile::Minimal,
            )
            .unwrap();
            let runtime_bytes = artifact.runtime().unwrap().bytes();
            for bytes in [runtime_bytes, artifact.bytes.as_slice()] {
                Validator::new_with_features(WasmFeatures::all())
                    .validate_all(bytes)
                    .expect("internal definition R/P modules validate");
            }
            let runtime = Module::read(runtime_bytes, true);
            let program = Module::read(&artifact.bytes, true);
            assert!(program.types.len() >= runtime.types.len());
            assert_eq!(&program.types[..runtime.types.len()], &runtime.types);
            let runtime_groups = type_group_sizes(runtime_bytes);
            let program_groups = type_group_sizes(&artifact.bytes);
            assert!(program_groups.len() >= runtime_groups.len());
            assert_eq!(&program_groups[..runtime_groups.len()], &runtime_groups);

            let object = runtime.witness_type(GcSnapshotLayout::Object);
            let array = runtime.witness_type(GcSnapshotLayout::Array);
            let descriptor = runtime.witness_type(GcSnapshotLayout::Descriptor);
            let (prepare, _) = runtime.named("helper::ordinary_property_prepare");
            let (allocate, _) = runtime.named("helper::ordinary_object_allocate");
            let (public, _) = runtime.named("builtin::Reflect.defineProperty");
            let (function_call, _) = runtime.named("helper::function_call");
            let (proxy_call, _) = runtime.named("helper::proxy_call");
            let (data, data_body) = runtime.named("helper::object_define_data");
            let (publish, _) = runtime.named("helper::array_indexed_publish");
            let (to_number, _) = runtime.named("helper::value_to_number");
            let (_, array_from) = runtime.named("builtin::Array.from");
            assert_eq!(array_from.iter().filter(|op| matches!(op,
                Operator::Call { function_index } if *function_index == data)).count(), 2,
                "Array.from's iterator and array-like loops use the shared definition owner");
            let (_, literal) = program.named("js::literal#");
            assert!(
                calls(literal, program.imported_runtime_function(&runtime, data)),
                "computed data definitions retain the shared runtime owner"
            );
            for (module, body, expected, expected_arrays, label) in [
                (&runtime, data_body, 1, 1, "runtime data definition"),
                (&program, literal, 2, 0, "source getter and setter definitions"),
            ] {
                let arms = concrete_if_arms(body, object)
                    .into_iter()
                    .filter(|arm| arm.iter().any(|op| matches!(op,
                        Operator::StructGet { struct_type_index, field_index }
                            if *struct_type_index == object && *field_index == GcSnapshotField::ObjectExtensible.index())))
                    .collect::<Vec<_>>();
                assert_eq!(arms.len(), expected, "{label} admits actual ordinary objects");
                let owner = |runtime_function| {
                    if std::ptr::eq(module, &runtime) {
                        return Some(runtime_function);
                    }
                    optional_runtime_import(&artifact.bytes, runtime_bytes, runtime_function)
                };
                let array_arms = concrete_if_arms(body, array);
                assert_eq!(array_arms.len(), expected_arrays,
                    "{label} admits Array data creation without expanding source accessors");
                for arm in &array_arms {
                    assert!(calls(arm, owner(publish).expect("canonical indexed publication is imported")),
                        "{label} retains canonical Array indexed descriptor publication");
                    for field in [GcSnapshotField::ArrayLength, GcSnapshotField::ArrayLengthWritable] {
                        assert!(arm.iter().any(|op| matches!(op,
                            Operator::StructGet { struct_type_index, field_index }
                                if *struct_type_index == array && *field_index == field.index())),
                            "{label} checks live {field:?}");
                        if std::ptr::eq(module, &runtime) {
                            assert!(arm.iter().any(|op| matches!(op,
                                Operator::StructSet { struct_type_index, field_index }
                                    if *struct_type_index == array && *field_index == field.index())),
                                "supplied data fields preserve canonical {field:?} updates");
                        }
                    }
                    if std::ptr::eq(module, &runtime) {
                        let conversions = arm.iter().enumerate().filter_map(|(index, op)| {
                            matches!(op, Operator::Call { function_index }
                                if *function_index == to_number).then_some(index)
                        }).collect::<Vec<_>>();
                        assert_eq!(conversions.len(), 2,
                            "a supplied Array length retains both observable numeric conversions");
                        let current_length = arm.iter().position(|op| matches!(op,
                            Operator::StructGet { struct_type_index, field_index }
                                if *struct_type_index == array && *field_index == GcSnapshotField::ArrayLength.index()))
                            .expect("Array length is read after conversion");
                        assert!(conversions[1] < current_length,
                            "length coercion effects precede acquiring the current Array length");
                    }
                }
                for arm in arms.into_iter().chain(array_arms) {
                    assert!(calls(arm, owner(prepare).expect("canonical preparation is imported")), "{label} retains canonical non-index dictionary preparation");
                    assert!(arm.iter().any(|op| matches!(op,
                        Operator::StructNew { struct_type_index } if *struct_type_index == descriptor)),
                        "{label} merges into the canonical stored descriptor");
                    assert!(!arm.iter().any(|op| matches!(op,
                        Operator::StructNew { struct_type_index } | Operator::StructNewDefault { struct_type_index }
                            if *struct_type_index == object)),
                        "{label} constructs no JavaScript descriptor object");
                    for function in [allocate, public, function_call, proxy_call] {
                        if let Some(imported) = owner(function) {
                            assert!(!calls(arm, imported),
                                "{label} bypasses descriptor allocation and JavaScript dispatch");
                        }
                    }
                    assert!(!arm.iter().any(|op| matches!(op,
                        Operator::CallRef { .. } | Operator::RefFunc { .. })),
                        "{label} creates no callable or indirect JavaScript call");
                }
                assert!(calls(body, owner(allocate).expect("exotic carrier allocation is imported")), "{label} retains exotic descriptor carriers");
                assert!(calls(body, owner(proxy_call).expect("exotic JavaScript dispatch is imported")), "{label} retains exotic JavaScript dispatch");
            }
        })
        .unwrap()
        .join()
        .unwrap();
}
