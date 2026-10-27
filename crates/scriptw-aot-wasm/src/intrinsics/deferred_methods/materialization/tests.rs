use super::*;
use std::collections::BTreeMap;
use wasmparser::{KnownCustom, Name, Operator, Parser, Payload, TypeRef};

fn ordinary_meta() -> WasmFunctionMeta {
    WasmFunctionMeta {
        numeric_leaf: None,
        async_generator_resume_environment_plan: None,
        template_source: None,
        name: "abs".into(),
        to_string_value: "function abs() { [native code] }".into(),
        standard_builtin: Some(StandardBuiltinId::MathAbs),
        host_builtin: None,
        length: 1,
        length_name_configurable: true,
        entry: crate::function_entry::PlannedFunctionEntry::standard(
            StandardBuiltinId::MathAbs,
            17,
        ),
        strict: true,
        is_named_expression: false,
        class_element_execution_kind: scriptw_ir::ClassElementExecutionKind::None,
        class_heritage_kind: scriptw_ir::ClassHeritageKind::None,
        is_static_class_member: false,
        is_derived_constructor: false,
        is_synthetic_default_derived_constructor: false,
        class_instance_element_plan: None,
        uses_super: false,
        this_before_super: false,
        captures_private_environment: false,
        needs_active_function_identity: false,
    }
}

#[test]
fn common_tail_rejects_nonuniform_metadata_before_dispatch() {
    let method = DeferredIntrinsicMethod::new(StandardBuiltinId::MathAbs).unwrap();
    assert!(MethodPlan::new(method, ordinary_meta()).is_ok());
    let mutations: &[fn(&mut WasmFunctionMeta)] = &[
        |meta| meta.strict = false,
        |meta| meta.length_name_configurable = false,
        |meta| meta.uses_super = true,
        |meta| meta.captures_private_environment = true,
        |meta| meta.needs_active_function_identity = true,
        |meta| meta.is_derived_constructor = true,
        |meta| {
            meta.entry = crate::function_entry::PlannedFunctionEntry::standard(
                StandardBuiltinId::MathMax,
                17,
            )
        },
        |meta| meta.standard_builtin = Some(StandardBuiltinId::MathMax),
    ];
    for (index, mutate) in mutations.iter().enumerate() {
        let mut meta = ordinary_meta();
        mutate(&mut meta);
        assert!(
            MethodPlan::new(method, meta).is_err(),
            "invalid domain mutation {index}"
        );
    }
}

#[test]
fn deferred_dispatch_has_many_typed_entries_but_one_function_construction_tail() {
    std::thread::Builder::new().stack_size(64 * 1024 * 1024).spawn(|| {
        let parsed = scriptw_front::parse("Math.abs;", scriptw_front::ParseOptions::script()).unwrap();
        let artifact = crate::emit(&scriptw_ir::lower(&parsed)).unwrap();
        let bytes = artifact.runtime().unwrap().bytes();
        wasmparser::Validator::new_with_features(crate::runtime_artifact::program_validation_features())
            .validate_all(bytes).unwrap();
        let mut names = BTreeMap::new();
        let mut bodies = BTreeMap::new();
        let mut next = 0;
        for payload in Parser::new(0).parse_all(bytes) {
            match payload.unwrap() {
                Payload::ImportSection(imports) => for import in imports.into_imports() {
                    if matches!(import.unwrap().ty, TypeRef::Func(_) | TypeRef::FuncExact(_)) { next += 1; }
                },
                Payload::CodeSectionEntry(body) => {
                    bodies.insert(next, body.get_operators_reader().unwrap().into_iter().map(Result::unwrap).collect::<Vec<_>>());
                    next += 1;
                }
                Payload::CustomSection(section) => if let KnownCustom::Name(reader) = section.as_known() {
                    for subsection in reader {
                        if let Name::Function(reader) = subsection.unwrap() {
                            for row in reader { let row = row.unwrap(); names.insert(row.name.to_owned(), row.index); }
                        }
                    }
                },
                _ => {}
            }
        }
        let resolve = &bodies[&names["helper::property_value_resolve"]];
        for helper in ["helper::native_function_allocate", "helper::function_metadata_publish"] {
            let target = names[helper];
            assert_eq!(resolve.iter().filter(|op| matches!(op, Operator::Call { function_index } if *function_index == target)).count(), 1,
                "every valid selection shares one physical construction/publication tail: {helper}");
        }
        assert!(resolve.iter().filter(|op| matches!(op, Operator::RefFunc { .. })).count() > 10,
            "the control must retain many actual typed builtin entries, not erase the dispatch domain");
        assert!(resolve.iter().filter(|op| matches!(op, Operator::I32Ne)).count() > 10,
            "each selected leaf still checks exact builtin identity before reaching the tail");
    }).unwrap().join().unwrap();
}
