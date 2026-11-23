fn functions_named<'a>(program: &'a ProgramIr, name: &str) -> Vec<&'a FunctionIr> {
    program
        .script
        .as_ref()
        .expect("Script IR")
        .functions
        .iter()
        .filter(|function| function.name == name && !function.id.contains("$exact_helper_context"))
        .collect()
}

fn guarded_pair<'a>(
    program: &'a ProgramIr,
    name: &str,
) -> (&'a FunctionIr, &'a [BuiltinEntryFactIr], &'a FunctionIr) {
    let bodies = functions_named(program, name);
    let [guarded, fallback] = bodies.as_slice() else {
        panic!("`{name}` must be a guarded pair: {} bodies", bodies.len());
    };
    let (facts, partner) = guarded
        .builtin_entry_guard()
        .unwrap_or_else(|| panic!("`{name}` must carry its entry guard first"));
    assert_eq!(partner.id, fallback.id);
    assert!(matches!(
        &fallback.builtin_entry,
        Some(BuiltinEntryRoleIr::Fallback { guarded: partner }) if partner.id == guarded.id
    ));
    (guarded, facts, fallback)
}

fn returned(function: &FunctionIr) -> &TypedExpr {
    function
        .body
        .statements
        .iter()
        .find_map(|statement| match statement {
            StatementIr::Return(value) => Some(value),
            _ => None,
        })
        .expect("the body returns a value")
}

/// A body whose builtin facts became proofs is emitted twice: the guarded
/// body keeps the proof and names what its entry must check; the fallback
/// assumes nothing; callers read the join of both through the one identity.
#[test]
fn a_body_that_proves_from_a_builtin_fact_is_a_guarded_pair() {
    let program = lower_script("function run() { return Math.imul(2, 3) + 1; } run();");
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let (guarded, facts, fallback) = guarded_pair(&program, "run");
    assert_eq!(
        facts,
        [BuiltinEntryFactIr::Namespace(BuiltinNamespaceIr::Math)]
    );
    assert_eq!(returned(guarded).kind, ValueKind::Number);
    assert_ne!(returned(fallback).kind, ValueKind::Number);
    assert_eq!(guarded.return_kind, fallback.return_kind);
    assert_ne!(guarded.return_kind, ValueKind::Number);
    assert_eq!(guarded.params, fallback.params);
    assert_eq!(guarded.captured_bindings, fallback.captured_bindings);
}

#[test]
fn an_entry_guard_names_every_builtin_global_the_body_consulted() {
    let program = lower_script(
        "function run(text) { return parseInt(text) + String(text).length + Number(text); }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let (_, facts, _) = guarded_pair(&program, "run");
    for (name, function) in [
        ("parseInt", HostBuiltinId::ParseInt.function_id()),
        ("String", StandardBuiltinId::StringConstructor.function_id()),
        ("Number", StandardBuiltinId::NumberConstructor.function_id()),
    ] {
        assert!(
            facts.contains(&BuiltinEntryFactIr::Function {
                name: name.to_string(),
                function,
            }),
            "{name}: {facts:?}"
        );
    }
    let mut sorted = facts.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(facts, sorted);
}

/// No fact, no second body: the guard costs only the bodies it protects.
#[test]
fn a_body_without_builtin_proofs_stays_one_body() {
    let program = lower_script(
        "function add(left, right) { return left + right; } \
         function pick(object) { return object.value; } add(1, 2); pick({ value: 1 });",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    for name in ["add", "pick"] {
        let bodies = functions_named(&program, name);
        assert_eq!(bodies.len(), 1, "{name}");
        assert!(bodies[0].builtin_entry.is_none(), "{name}");
    }
}

/// A resumable body would have to prove its facts again after every
/// suspension, and a class element has no second body: both assume nothing.
#[test]
fn resumable_bodies_and_class_elements_assume_no_builtin_fact() {
    let program = lower_script(
        "function* generator() { return Math.imul(2, 3) + 1; } \
         async function asynchronous() { return Math.imul(2, 3) + 1; } \
         class Holder { method() { return Math.imul(2, 3) + 1; } }",
    );
    assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
    let script = program.script.as_ref().expect("Script IR");
    assert!(script
        .functions
        .iter()
        .all(|function| function.builtin_entry.is_none()));
    let method = script
        .functions
        .iter()
        .find(|function| {
            function.protocol == FunctionProtocolIr::ClassMethod(FunctionExecutionKind::Ordinary)
        })
        .expect("the class method is lowered");
    assert_ne!(returned(method).kind, ValueKind::Number);
}

/// A namespace no entry can prove is never assumed.
#[test]
fn a_body_that_consults_an_unprovable_builtin_is_proof_free() {
    let program = lower_script(
        "function run(value) { return Temporal.Now.instant() + Math.imul(value, 3); }",
    );
    let bodies = functions_named(&program, "run");
    assert_eq!(bodies.len(), 1);
    assert!(bodies[0].builtin_entry.is_none());
}
