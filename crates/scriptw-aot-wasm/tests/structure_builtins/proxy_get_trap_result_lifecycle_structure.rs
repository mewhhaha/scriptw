const OBJECTS_SOURCE: &str = include_str!("../../src/objects.rs");
const CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/object.rs");
const CLI_FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_proxy_get_direct_descriptor_invariants.js"
);

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end after {start}: {end}"))
        .0
}

#[test]
fn proxy_get_trap_result_roles_have_no_incidental_capabilities() {
    let declaration = bounded(
        OBJECTS_SOURCE,
        "/// A Proxy Get result whose abrupt completion still needs routing.",
        "/// Where a revoked-Proxy TypeError",
    );
    assert!(!declaration.contains("#[derive"));
    assert!(declaration.contains("struct PendingProxyGetTrapResultLocals<'value>"));
    assert!(
        declaration.contains("struct NormalProxyGetTrapResultLocals<'value>(&'value ValueLocals)")
    );
    assert!(declaration.contains("completion: &'value CompletionLocals"));
    assert!(declaration.contains("value: &'value ValueLocals"));
    for role in [
        "PendingProxyGetTrapResultLocals",
        "NormalProxyGetTrapResultLocals",
    ] {
        assert!(!declaration.contains(&format!("pub(crate) struct {role}")));
        for capability in [
            "Clone",
            "Copy",
            "Debug",
            "Default",
            "PartialEq",
            "Eq",
            "PartialOrd",
            "Ord",
            "Hash",
        ] {
            assert!(!OBJECTS_SOURCE.contains(&format!("impl {capability} for {role}")));
        }
    }
    for message in [
        "a pending Proxy Get trap result must be normalized before inspection",
        "a normal Proxy Get trap result must be consumed by its invariant",
    ] {
        assert!(declaration.contains(&format!("#[must_use = \"{message}\"]")));
    }
}

#[test]
fn one_transition_routes_completion_before_publishing_the_normal_result() {
    assert_eq!(
        OBJECTS_SOURCE
            .matches("PendingProxyGetTrapResultLocals")
            .count(),
        4,
        "declaration, construction, consuming argument and destructure"
    );
    assert_eq!(OBJECTS_SOURCE.matches("NormalProxyGetTrapResultLocals").count(), 5,
        "declaration, borrowed observer impl, transition return/construction and invariant argument");
    let transition = bounded(
        OBJECTS_SOURCE,
        "fn emit_normal_proxy_get_trap_result<'value>(",
        "fn emit_proxy_get_invariant_check(",
    );
    assert!(transition.contains("pending: PendingProxyGetTrapResultLocals<'value>"));
    let route = transition
        .find("self.emit_object_operation_abrupt_exit(completion, result, exit, function);")
        .unwrap();
    let copy = transition
        .find("value.copy_from(completion.value(), function);")
        .unwrap();
    let publish = transition
        .find("NormalProxyGetTrapResultLocals(value)")
        .unwrap();
    assert!(route < copy && copy < publish);
}

#[test]
fn the_normal_result_has_one_consuming_invariant_and_borrowed_observers() {
    let invariant = bounded(
        OBJECTS_SOURCE,
        "fn emit_proxy_get_invariant_check(",
        "pub(crate) fn emit_object_write(",
    );
    assert!(invariant.contains("trap_result: NormalProxyGetTrapResultLocals<'_>"));
    assert_eq!(
        invariant
            .matches("trap_result.value().tag().load(function);")
            .count(),
        1
    );
    let normalized = invariant
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    assert_eq!(normalized.matches("self.emit_tagged_payload_same_value_i32(trap_result.value(),&invariant_value,function)?;").count(), 1);
    assert!(invariant.contains("descriptor.read_getter(&invariant_value, schema, function);"));
    assert!(invariant.contains("descriptor.read_value(&invariant_value, self, function);"));
    let trap = bounded(
        OBJECTS_SOURCE,
        "fn emit_proxy_get(",
        "fn emit_normal_proxy_get_trap_result<'value>(",
    );
    let call = trap
        .find("self.emit_function_handle_call_with_argv_inner(")
        .unwrap();
    let transition = trap
        .find("self.emit_normal_proxy_get_trap_result(")
        .unwrap();
    let check = trap.find("self.emit_proxy_get_invariant_check(").unwrap();
    let close = trap
        .find("self.pop_control(ControlFrameKind::Block);")
        .unwrap();
    let clear = trap.find("trap_result.clear(function);").unwrap();
    assert!(call < transition && transition < check && check < close && close < clear);
    assert_eq!(
        OBJECTS_SOURCE
            .matches("self.emit_proxy_get_invariant_check(")
            .count(),
        1
    );
}

#[test]
fn exact_cli_witness_pins_abrupt_result_identity_and_descriptor_invariants() {
    assert!(CLI_TESTS
        .contains("fn run_wasm_backend_succeeds_for_proxy_get_direct_descriptor_invariants()"));
    assert!(CLI_TESTS.contains("wasm_proxy_get_direct_descriptor_invariants.js"));
    for marker in [
        "direct thrown trap was replaced by invariant error",
        "Reflect thrown trap was replaced by invariant error",
        "callable Proxy getter is not undefined",
        "symbol object-identity SameValue",
    ] {
        assert!(
            CLI_FIXTURE.contains(marker),
            "missing fixture marker: {marker}"
        );
    }
}
