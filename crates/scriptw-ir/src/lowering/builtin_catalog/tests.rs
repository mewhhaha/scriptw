use super::*;

#[test]
fn canonical_bootstrap_fill_preserves_source_function_var_and_lexical_owners() {
    let mut plan = GlobalBindingPlan::from_parts(
        [],
        [("lexical".to_owned(), GlobalLexicalBindingModeIr::Mutable)],
    );
    plan.record_var("ordinary".to_owned());
    let id = "source:array".to_owned();
    assert_eq!(
        plan.record_function("Array".to_owned(), id.clone()),
        GlobalFunctionDeclarationDispositionIr::Installed
    );
    let before = plan.clone();
    retain_canonical_standard_globals(&mut plan);
    for binding in &before {
        assert_eq!(plan.get(&binding.name), Some(binding));
    }
    assert_eq!(plan.lexical_bindings(), before.lexical_bindings());
    assert_eq!(
        plan.get("Array").unwrap().initializer,
        GlobalPropertyInitializerIr::SourceFunction(id)
    );
    assert_eq!(
        plan.get("ordinary").unwrap().declarations,
        GlobalDeclarationSetIr::Var
    );
    let count = plan.len();
    retain_canonical_standard_globals(&mut plan);
    assert_eq!(plan.len(), count);
}

#[test]
fn immutable_catalogs_are_reused_with_bounded_complete_entries() {
    for policy in [HostSurfacePolicy::Product, HostSurfacePolicy::Test262] {
        assert!(std::ptr::eq(
            global_property_baseline(policy),
            global_property_baseline(policy)
        ));
        let expected =
            10 + StandardBuiltinId::all_globals().len() + policy.global_builtins().count();
        assert_eq!(global_property_baseline(policy).len(), expected);
    }
    assert_eq!(
        global_property_baseline(HostSurfacePolicy::Product).len(),
        82
    );
    assert_eq!(
        global_property_baseline(HostSurfacePolicy::Test262).len(),
        97
    );
    assert!(std::ptr::eq(
        function_signature_baseline(),
        function_signature_baseline()
    ));
    assert_eq!(function_signature_baseline().len(), 997);
    for builtin in StandardBuiltinId::all_functions() {
        assert!(function_signature_baseline().contains_key(&builtin.function_id()));
    }
}

#[test]
fn product_and_test262_global_catalogs_preserve_separate_authority() {
    let product = global_property_baseline(HostSurfacePolicy::Product);
    let test262 = global_property_baseline(HostSurfacePolicy::Test262);
    assert!(!std::ptr::eq(product, test262));
    for builtin in HostBuiltinId::global_builtins() {
        let name = builtin.global_name().unwrap();
        assert_eq!(
            product.contains_key(name),
            HostSurfacePolicy::Product.allows(builtin)
        );
        assert_eq!(
            test262.contains_key(name),
            HostSurfacePolicy::Test262.allows(builtin)
        );
    }
}

#[test]
fn mutable_global_shapes_cannot_escape_one_compilation() {
    for policy in [HostSurfacePolicy::Product, HostSurfacePolicy::Test262] {
        let baseline = global_property_baseline(policy);
        let mut first = global_properties(policy);
        let reflect = first.get_mut(REFLECT_NAME).unwrap();
        let Some(HeapShape::Object(shape)) = reflect.value_info.heap_shape.as_deref_mut() else {
            panic!("Reflect must retain its builtin object recipe");
        };
        assert!(!shape.properties.is_empty());
        shape.properties.clear();
        reflect.configurable = false;
        first.remove("Infinity");
        assert!(global_properties(policy).iter().eq(baseline.iter()));
        assert!(!first.iter().eq(baseline.iter()));
    }
}

#[test]
fn mutable_signature_facts_cannot_escape_one_compilation() {
    let baseline = function_signature_baseline();
    let mut first = function_signatures();
    let signature = first
        .get_mut(&StandardBuiltinId::FunctionConstructor.function_id())
        .unwrap();
    signature.return_kind = ValueKind::Undefined;
    signature.return_shape = FunctionReturnShape::Absent;
    signature.this_observed = true;
    let Some(HeapShape::Object(shape)) = signature.constructor_instance.heap_shape.as_deref_mut()
    else {
        panic!("Function constructor must retain its instance recipe");
    };
    shape.properties.clear();
    first.remove(&HostBuiltinId::CreateRealm.function_id());
    assert!(function_signatures().iter().eq(baseline.iter()));
    assert!(!first.iter().eq(baseline.iter()));
}

#[test]
fn warm_catalogs_do_not_reuse_a_source_or_caller_parse_context() {
    use scriptw_front::EvalInvocationContext;
    let _ = global_property_baseline(HostSurfacePolicy::Product);
    let _ = global_property_baseline(HostSurfacePolicy::Test262);
    let _ = function_signature_baseline();
    for strictness in [Strictness::Sloppy, Strictness::Strict, Strictness::Sloppy] {
        let context = DirectEvalContextIr::new(
            strictness,
            EvalInvocationContext::Function,
            BTreeMap::new(),
            None,
        );
        let caller =
            crate::RuntimeDirectEvalCaller::validate(context.runtime_descriptor(), []).unwrap();
        let source = "010;".encode_utf16().collect::<Vec<_>>();
        let result = crate::lower_runtime_direct_eval(&caller, &source, HostSurfacePolicy::Product);
        if strictness == Strictness::Strict {
            assert!(matches!(
                result,
                Err(crate::RuntimeDirectEvalCompileError::Parse(_))
            ));
        } else {
            let program = result.unwrap();
            assert_eq!(program.caller(), &caller);
            assert_eq!(program.program().source_len, source.len());
        }
    }
}
