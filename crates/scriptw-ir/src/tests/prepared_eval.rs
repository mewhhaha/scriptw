#[test]
fn no_source_eval_works_through_alias_optional_and_safe_multi_target_calls() {
    for source in [
        "let indirect = eval; indirect(1);",
        "eval?.();",
        "eval?.(1);",
        "(eval?.(function marker() { return 1; }))();",
        "function plusOne(value) { return value + 1; } let f = unknown ? eval : plusOne; f(1);",
    ] {
        let program = lower_script(source);
        assert!(
            !program.diagnostics.iter().any(|diagnostic| matches!(
                diagnostic.unsupported_feature(),
                Some(UnsupportedFeature::DynamicSource(_))
            )),
            "{source}: {:?}",
            program.diagnostics
        );
    }
}

#[test]
fn multi_target_eval_keeps_genuine_undefined_as_a_result_alternative() {
    for source in [
            "function returnsNumber() { return 1; } let f = unknown ? eval : returnsNumber; f();",
            "function returnsNumber() { return 1; } let f = unknown ? eval : returnsNumber; f(undefined);",
        ] {
            let program = lower_script(source);
            assert!(
                !program.diagnostics.iter().any(|diagnostic| matches!(
                    diagnostic.unsupported_feature(),
                    Some(UnsupportedFeature::DynamicSource(_))
                )),
                "{source}: {:?}",
                program.diagnostics
            );
            let script = program.script.as_ref().expect("script IR should exist");
            let call = script
                .body
                .statements
                .iter()
                .rev()
                .find_map(|statement| match statement {
                    StatementIr::Expression(expression) => indirect_call_body(expression),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{source}: missing multi-target call"));
            assert_eq!(
                call.possible_kinds,
                KindSet::from_kind(ValueKind::Undefined)
                    .union(KindSet::from_kind(ValueKind::Number)),
                "{source}: target alternatives"
            );
        }
}

#[test]
fn eval_spread_and_unknown_source_keep_the_runtime_invocation() {
    for source in [
        "eval(...[1]);",
        "let source = unknown ? 1 : 'source'; eval(source);",
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
    }
    let mixed_eval = lower_script(
            "function plusOne(value) { return value + 1; } let f = unknown ? eval : plusOne; f('source');",
        );
    assert_prepared_script(&mixed_eval, PreparedScriptKind::IndirectEval);
    let mixed_function = lower_script("let f = unknown ? eval : Function; f(1);");
    assert!(
        mixed_function.is_wasm_supported(),
        "{:?}",
        mixed_function.diagnostics
    );
    assert_eq!(
        mixed_function
            .script
            .expect("Script IR")
            .prepared_dynamic_functions[0]
            .arguments,
        ["1"]
    );
}

#[test]
fn syntax_proven_function_arguments_register_compiled_sources() {
    for source in [
        "Function('return 1');",
        "Function('value', `return value`);",
        "let C = Object.getPrototypeOf(function*() {}).constructor; C('yield 1');",
        "new (Object.getPrototypeOf(async function*() {}).constructor)('yield 1');",
        "Function?.('return 1');",
        "let C = Object.getPrototypeOf(function*() {}).constructor; C?.('yield 1');",
    ] {
        let program = lower_script(source);
        assert!(
            program.diagnostics.is_empty(),
            "{source}: {:?}",
            program.diagnostics
        );
        let prepared = program.script.unwrap().prepared_dynamic_functions;
        assert_eq!(prepared.len(), 1);
        assert!(matches!(
            prepared[0].outcome,
            PreparedDynamicFunctionOutcome::Compiled { .. }
        ));
    }
}

#[test]
fn direct_eval_sources_keep_runtime_dispatch_and_compile_known_candidates() {
    for source in [
        "eval('source');",
        "eval(`source`);",
        "eval('sou' + ('rce'));",
        "eval(source);",
        "eval(String('source'));",
        "eval(true ? 'source' : 'other');",
    ] {
        let program = lower_script(source);
        assert!(
            program.is_wasm_supported(),
            "{source}: {:?}",
            program.diagnostics
        );
    }
    for source in [
        "eval('source');",
        "eval(`source`);",
        "eval('sou' + ('rce'));",
    ] {
        let program = lower_script(source);
        assert!(program
            .script
            .unwrap()
            .prepared_scripts
            .iter()
            .any(|source| matches!(source.kind, PreparedScriptKind::DirectEval(_))));
    }
}

#[test]
fn direct_eval_after_a_user_constructor_prepares_caller_source() {
    let program = lower_script(
        r#"
function Factory() {
  this.toString = function () { return "wizard"; };
}
Factory.prototype.charAt = String.prototype.charAt;
let instance = new Factory();
instance.charAt(eval("1"));
"#,
    );

    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    assert!(program
        .script
        .unwrap()
        .prepared_scripts
        .iter()
        .any(|source| matches!(source.kind, PreparedScriptKind::DirectEval(_))));
}

#[test]
fn direct_eval_callee_is_captured_before_an_argument_replaces_the_global() {
    let program = lower_script(
        r#"
function Factory() {}
function replacement(source) { return source; }
new Factory();
eval((eval = replacement, "source"));
"#,
    );

    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
}

#[test]
fn conditionally_deleted_eval_retains_its_intrinsic_possibility() {
    let program = lower_script(
        r#"
if (unknown) delete eval;
eval("source");
"#,
    );

    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    assert!(program
        .script
        .unwrap()
        .prepared_scripts
        .iter()
        .any(|source| matches!(source.kind, PreparedScriptKind::DirectEval(_))));
}

#[test]
fn definitely_deleted_eval_is_not_dynamic_source() {
    let program = lower_script(
        r#"
delete eval;
eval("source");
"#,
    );

    assert!(!program.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.unsupported_feature(),
            Some(UnsupportedFeature::DynamicSource(_))
        )
    }));
}

#[test]
fn a_definitely_replaced_eval_binding_is_not_dynamic_source() {
    let program = lower_script(
        r#"
function replacement(source) { return source; }
eval = replacement;
eval("source");
"#,
    );

    assert!(!program.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.unsupported_feature(),
            Some(UnsupportedFeature::DynamicSource(_))
        )
    }));
}

#[test]
fn non_string_eval_after_a_user_constructor_needs_no_dynamic_source() {
    let program = lower_script(
        r#"
function Factory() {}
new Factory();
eval(1);
"#,
    );

    assert!(!program.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.unsupported_feature(),
            Some(UnsupportedFeature::DynamicSource(_))
        )
    }));
    let script = program.script.as_ref().expect("script IR should exist");
    let call = script
        .body
        .statements
        .iter()
        .rev()
        .find_map(|statement| match statement {
            StatementIr::Expression(TypedExpr {
                expr: ExprIr::EnvironmentIdentifier(identifier),
                ..
            }) if identifier.name == "eval" => Some(identifier),
            _ => None,
        })
        .expect("the no-source eval Reference remains in IR");
    let EnvironmentIdentifierOperationIr::Call {
        args,
        direct_eval: Some(_),
        ..
    } = &call.operation
    else {
        panic!("bare call keeps context");
    };
    assert!(
        matches!(args.as_slice(), [TypedExpr { expr: ExprIr::Number(value), .. }] if *value == 1.0f64.to_bits())
    );
}

#[test]
fn a_parenthesized_eval_identifier_keeps_direct_eval_authority() {
    let program = lower_script("(eval)(\"source\");");

    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    assert!(program
        .script
        .unwrap()
        .prepared_scripts
        .iter()
        .any(|source| matches!(source.kind, PreparedScriptKind::DirectEval(_))));
}

#[test]
fn an_eval_property_call_has_indirect_eval_authority() {
    let program = lower_script("(globalThis.eval)(\"source\");");
    assert_prepared_script(&program, PreparedScriptKind::IndirectEval);
}

#[test]
fn grouped_optional_dynamic_source_prefix_is_accounted_once() {
    let program = lower_script("(Function?.('return 1'))();");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    assert_eq!(
        program
            .script
            .expect("Script IR")
            .prepared_dynamic_functions
            .len(),
        1
    );
}

#[test]
fn multi_target_construct_ignores_non_constructable_dynamic_source_identities() {
    let program = lower_script("let C = unknown ? eval : Array; new C('source');");
    assert!(
        !program.diagnostics.iter().any(|diagnostic| matches!(
            diagnostic.unsupported_feature(),
            Some(UnsupportedFeature::DynamicSource(_))
        )),
        "{:?}",
        program.diagnostics
    );
}

#[test]
fn realm_eval_script_is_a_test262_only_typed_host_capability() {
    let name = HostBuiltinId::RealmEvalScript
        .global_name()
        .expect("realm eval must have a harness global name");
    let source = format!("let realmEval = {name}; realmEval('source');");

    let test262 = lower_test262_script(&source);
    assert_prepared_script(&test262, PreparedScriptKind::RealmScript);
    assert!(test262
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));

    let optional_test262 = lower_test262_script(&format!("{name}?.('source');"));
    assert_prepared_script(&optional_test262, PreparedScriptKind::RealmScript);
    let optional_converted_test262 = lower_test262_script(&format!("{name}?.(String('source'));"));
    assert!(
        optional_converted_test262.is_wasm_supported(),
        "{:?}",
        optional_converted_test262.diagnostics
    );
    assert!(
        optional_converted_test262
            .script
            .as_ref()
            .expect("Script IR")
            .prepared_scripts
            .iter()
            .any(|script| script.kind == PreparedScriptKind::RealmScript
                && script.source == "source"
                && script.admission == PreparedScriptAdmission::RuntimeCandidate
                && matches!(script.outcome, PreparedScriptOutcome::Executable(_))),
        "converted source must retain runtime callable and source guards"
    );
    let optional_runtime_test262 =
        lower_test262_script(&format!("{name}?.(String(unknownSource));"));
    assert!(
        optional_runtime_test262.is_wasm_supported(),
        "{:?}",
        optional_runtime_test262.diagnostics
    );
    let runtime_script = optional_runtime_test262.script.as_ref().expect("Script IR");
    assert!(runtime_script.prepared_scripts.is_empty());
    assert!(runtime_script
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));

    let product = lower_script(&source);
    assert!(!product.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.unsupported_feature(),
            Some(UnsupportedFeature::DynamicSource(gap))
                if gap.kind() == DynamicSourceKind::RealmEvalScript
        )
    }));
    assert!(!product
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn realm_eval_script_symbol_conversion_keeps_host_call_without_prepared_source() {
    let name = HostBuiltinId::RealmEvalScript.global_name().unwrap();
    for invocation in [
        format!("{name}(Symbol('source'));"),
        format!("{name}?.(Symbol('source'));"),
        format!("let captured = {name}; captured(Symbol('source'));"),
        format!(
            "const source = Symbol('source'); globalThis.Symbol = () => 'source'; {name}(source);"
        ),
    ] {
        let program = lower_test262_script(&invocation);
        assert!(
            program.is_wasm_supported(),
            "{invocation}: {:?}",
            program.diagnostics
        );
        let script = program.script.as_ref().expect("Script IR");
        assert!(
            script.prepared_scripts.is_empty(),
            "conversion must precede source dispatch"
        );
        assert!(script
            .host_builtins
            .contains(&HostBuiltinId::RealmEvalScript));
    }
}

#[test]
fn created_realm_eval_script_preserves_typed_dynamic_source_identity() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let $262 = {{ createRealm: function () {{ return {create_realm}(); }} }}; \
             let other = $262.createRealm(); other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert_prepared_script(&program, PreparedScriptKind::RealmScript);
    let script = program.script.as_ref().expect("script ir should exist");
    assert!(script.host_builtins.contains(&HostBuiltinId::CreateRealm));
    assert!(script
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}

#[test]
fn created_realm_shape_survives_an_earlier_heap_effect() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    // Missing descriptor fields are absent rather than inherited getters.
    let source = format!(
        "Object.defineProperty({{}}, 'x', {{ __proto__: null, value: 0 }}); \
             let $262 = {{ createRealm: function () {{ return {create_realm}(); }} }}; \
             let other = $262.createRealm(); other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert_prepared_script(&program, PreparedScriptKind::RealmScript);
}

#[test]
fn created_realm_shape_survives_an_earlier_callback_effect() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "function createRealm(callback) {{ callback(); return {create_realm}(); }} \
             createRealm(function () {{}}).evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert_prepared_script(&program, PreparedScriptKind::RealmScript);
}

#[test]
fn unaccounted_proxy_trap_invalidates_a_created_realm_shape() {
    let create_realm = HostBuiltinId::CreateRealm
        .global_name()
        .expect("create realm must have a harness global name");
    let source = format!(
        "let other = {create_realm}(); \
             let proxy = new Proxy({{}}, {{ getPrototypeOf: function () {{ \
                 delete other.evalScript; return null; \
             }} }}); \
             Object.getPrototypeOf(proxy); \
             other.evalScript('source');"
    );

    let program = lower_test262_script(&source);
    assert!(!program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|prepared| prepared.kind == PreparedScriptKind::RealmScript
            && prepared.admission == PreparedScriptAdmission::ResolvedIntrinsic));
    assert!(!program
        .script
        .as_ref()
        .expect("script ir should exist")
        .host_builtins
        .contains(&HostBuiltinId::RealmEvalScript));
}
#[test]
fn shadow_realm_evaluate_prepares_independent_eval_units_through_aliases_and_nested_sources() {
    for source in [
        "new ShadowRealm().evaluate('let local = 1; local');",
        "const realm = new ShadowRealm(); const evaluate = realm.evaluate; evaluate.call(realm, 'let local = 1; local');",
        "const realm = new ShadowRealm(); realm.evaluate?.('let local = 1; local');",
        "const holder = { run: new ShadowRealm().evaluate }; holder.run.call(new ShadowRealm(), 'let local = 1; local');",
    ] {
        let program = lower_script(source);
        assert!(program.is_wasm_supported(), "{source}: {:?}", program.diagnostics);
        let prepared = &program.script.as_ref().expect("Script IR").prepared_scripts;
        assert!(prepared.iter().any(|entry| {
            entry.kind == PreparedScriptKind::ShadowRealmEvaluate
                && entry.source == "let local = 1; local"
                && matches!(entry.outcome, PreparedScriptOutcome::Executable(_))
        }), "{source}: {prepared:?}");
        assert!(!prepared.iter().any(|entry| entry.kind == PreparedScriptKind::RealmScript));
    }

    let program =
        lower_script(r#"new ShadowRealm().evaluate("new ShadowRealm().evaluate('41 + 1')");"#);
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    assert!(program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_scripts
        .iter()
        .any(|entry| {
            entry.kind == PreparedScriptKind::ShadowRealmEvaluate && entry.source == "41 + 1"
        }));
}

#[test]
fn shadow_realm_evaluate_preparation_separates_strict_lifetime_and_deferred_syntax_errors() {
    let program = lower_script(
        r#"
        const realm = new ShadowRealm();
        realm.evaluate('let local = 1; var shared = 2; shared');
        realm.evaluate("'use strict'; let local = 1; var privateVar = 2; privateVar");
        realm.evaluate('new.target');
        realm.evaluate('super.value');
        realm.evaluate('return 1');
        "#,
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let prepared = &program.script.as_ref().expect("Script IR").prepared_scripts;
    for entry in prepared
        .iter()
        .filter(|entry| entry.kind == PreparedScriptKind::ShadowRealmEvaluate)
    {
        match &entry.outcome {
            PreparedScriptOutcome::Executable(unit) => {
                assert_eq!(unit.has_global_variable_environment(), !unit.strict);
            }
            PreparedScriptOutcome::DeferredSyntaxError { .. } => {
                assert!(matches!(
                    entry.source.as_str(),
                    "new.target" | "super.value" | "return 1"
                ));
            }
        }
    }
    for source in ["new.target", "super.value", "return 1"] {
        assert!(
            prepared.iter().any(|entry| entry.source == source
                && matches!(
                    entry.outcome,
                    PreparedScriptOutcome::DeferredSyntaxError { .. }
                )),
            "missing deferred syntax error for {source}"
        );
    }
}

#[test]
fn repeated_template_cell_hints_prepare_captured_eval_and_function_sources() {
    let program = lower_script(
        r#"
function direct(text) {
    function captured() { return text; }
    return eval(`'${text}${text}${text}${text}${text}${text}${text}${text}${text}';`);
}
function constructor(text) {
    function captured() { return text; }
    return Function(`return '${text}${text}${text}${text}${text}${text}${text}${text}${text}';`);
}
direct('a'); direct('b'); direct('c');
constructor('a'); constructor('b'); constructor('c');
"#,
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("Script IR");
    for text in ["a", "b", "c"] {
        let repeated = text.repeat(9);
        assert!(
            script.prepared_scripts.iter().any(|entry| {
                matches!(entry.kind, PreparedScriptKind::DirectEval(_))
                    && entry.source == format!("'{repeated}';")
                    && entry.admission == PreparedScriptAdmission::RuntimeCandidate
                    && matches!(entry.outcome, PreparedScriptOutcome::Executable(_))
            }),
            "missing captured direct source for {text}"
        );
        assert!(
            script.prepared_dynamic_functions.iter().any(|entry| {
                entry.arguments == [format!("return '{repeated}';")]
                    && matches!(
                        entry.outcome,
                        PreparedDynamicFunctionOutcome::Compiled { .. }
                    )
            }),
            "missing captured Function source for {text}"
        );
    }
}

#[test]
fn repeated_template_hints_keep_independent_cells_and_the_256_choice_limit() {
    let values = (0..16)
        .map(|index| format!("'{index}'"))
        .collect::<Vec<_>>()
        .join(",");
    let program = lower_script(&format!(
        "for (const left of [{values}]) for (const right of [{values}]) \
         Function(`return '${{left}}:${{right}}:${{left}}:${{right}}';`);"
    ));
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let functions = &program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_dynamic_functions;
    for left in 0..16 {
        for right in 0..16 {
            let body = format!("return '{left}:{right}:{left}:{right}';");
            assert!(
                functions
                    .iter()
                    .any(|entry| entry.arguments == [body.clone()]
                        && matches!(
                            entry.outcome,
                            PreparedDynamicFunctionOutcome::Compiled { .. }
                        )),
                "independent cells lost {left}/{right}"
            );
        }
    }
    assert_eq!(functions.len(), 256);

    let overflowing = lower_script(&format!(
        "Function('return 997;'); \
         for (const left of [{values},'16']) for (const right of [{values}]) \
         Function(`return '${{left}}:${{right}}:${{left}}:${{right}}';`);"
    ));
    assert!(
        overflowing.is_wasm_supported(),
        "{:?}",
        overflowing.diagnostics
    );
    let functions = &overflowing
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_dynamic_functions;
    assert!(functions
        .iter()
        .any(|entry| entry.arguments == ["return 997;"]));
    assert!(functions
        .iter()
        .all(|entry| !entry.arguments.iter().any(|source| source.contains(':'))));
}

#[test]
fn fitting_template_cartesian_sources_keep_reassignment_alternatives() {
    let program = lower_script(
        r#"
for (let text of ['a', 'b']) {
    Function(`return '${text}${(text = text === 'a' ? 'b' : 'a', '')}${text}';`);
}
"#,
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let functions = &program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_dynamic_functions;
    for text in ["aa", "ab", "ba", "bb"] {
        let body = format!("return '{text}';");
        assert!(
            functions
                .iter()
                .any(|entry| entry.arguments == [body.clone()]
                    && matches!(
                        entry.outcome,
                        PreparedDynamicFunctionOutcome::Compiled { .. }
                    )),
            "a fitting pre-existing Cartesian source was lost: {text}"
        );
    }
}

#[test]
fn repeated_template_hints_do_not_cross_shadowed_owner_cells() {
    let program = lower_script(
        r#"
function outer(text) {
    Function(`return 'outer:${text}${text}${text}${text}${text}${text}${text}${text}${text}';`);
    function inner(text) {
        return Function(`return 'inner:${text}${text}${text}${text}${text}${text}${text}${text}${text}';`);
    }
    inner('x'); inner('y'); inner('z');
}
outer('a'); outer('b'); outer('c');
"#,
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let functions = &program
        .script
        .as_ref()
        .expect("Script IR")
        .prepared_dynamic_functions;
    for (owner, values, foreign) in [
        ("outer", ["a", "b", "c"], ["x", "y", "z"]),
        ("inner", ["x", "y", "z"], ["a", "b", "c"]),
    ] {
        for text in values {
            let body = format!("return '{owner}:{}';", text.repeat(9));
            assert!(
                functions
                    .iter()
                    .any(|entry| entry.arguments == [body.clone()]),
                "{body}"
            );
        }
        for text in foreign {
            let body = format!("return '{owner}:{}';", text.repeat(9));
            assert!(
                functions
                    .iter()
                    .all(|entry| entry.arguments != [body.clone()]),
                "borrowed {body}"
            );
        }
    }
}
