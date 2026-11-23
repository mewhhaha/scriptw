const STANDARD_SOURCE: &str = include_str!("../../src/builtins/standard.rs");
const PLANNING_SOURCE: &str = include_str!("../../src/planning.rs");
const CATALOG_SOURCE: &str = include_str!("../../../scriptw-ir/src/builtins/catalog.rs");
const FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_function_prototype_symbol_has_instance.js"
);
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/function-prototype-symbol-has-instance.md"
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
fn intrinsic_is_one_rooted_nonconstructable_catalog_identity() {
    let catalog = bounded(
        CATALOG_SOURCE,
        "    FunctionPrototypeSymbolHasInstance {",
        "    TypedArrayConstructor {",
    );
    assert!(catalog.contains("=> BUILTIN_FUNCTION_PROTOTYPE_SYMBOL_HAS_INSTANCE_FUNCTION_ID"));
    assert!(catalog.contains("debug: \"Function.prototype[Symbol.hasInstance]\""));
    // OrdinaryHasInstance can run user code: Get(C, "prototype") may be a
    // getter, and the receiver's chain may hold a Proxy getPrototypeOf trap.
    assert!(catalog.contains("flags: [SYNCHRONOUS_USER_CODE]"));
    assert!(catalog.contains("installer: None"));
    assert!(catalog.contains("native: \"[Symbol.hasInstance]\""));

    let roots = bounded(
        PLANNING_SOURCE,
        "        if builtin == StandardBuiltinId::FunctionConstructor {",
        "        if builtin == StandardBuiltinId::DisposableStackConstructor {",
    );
    assert_eq!(
        roots
            .matches("StandardBuiltinId::FunctionPrototypeSymbolHasInstance,")
            .count(),
        1
    );
    assert!(roots.contains("self.require_standard_builtin(dependency)"));

    let length = bounded(
        PLANNING_SOURCE,
        "pub(crate) fn standard_builtin_length(builtin: StandardBuiltinId) -> u64 {",
        "pub(crate) fn host_builtin_length(builtin: HostBuiltinId) -> u64 {",
    );
    assert!(length.contains("StandardBuiltinId::FunctionPrototypeSymbolHasInstance"));
    assert!(length.contains("=> 1,"));

    let dispatch = bounded(
        STANDARD_SOURCE,
        "            StandardBuiltinId::FunctionPrototypeSymbolHasInstance => {",
        "            StandardBuiltinId::FunctionPrototypeCall => {",
    );
    assert_eq!(
        dispatch
            .matches("self.emit_function_prototype_symbol_has_instance_builtin(function)?")
            .count(),
        1
    );
}

#[test]
fn consumer_fixture_covers_the_complete_nondynamic_runtime_boundary() {
    for witness in [
        "realm-local intrinsic identity",
        "label + \" writable\"",
        "label + \" enumerable\"",
        "label + \" configurable\"",
        "undefined receiver",
        "number candidate",
        "positive chain",
        "negative chain",
        "bound target recursion",
        "bound custom handler result",
        "call-only function starts without prototype",
        "poisoned prototype abrupt",
        "default prototype stays non-configurable",
        "configurable prototype changes kind",
        "non-object prototype TypeError",
        "Proxy GetPrototypeOf abrupt",
        "Proxy chain abrupt",
    ] {
        assert!(
            FIXTURE.contains(witness),
            "missing fixture witness: {witness}"
        );
    }
    assert!(FIXTURE.contains("__scriptwCreateRealm"));
    assert!(CONTRACT.contains("The focused Test262 directory contains eleven files."));
    assert!(CONTRACT.contains("Dynamic Function source"));
    assert!(CONTRACT.contains("generation remains a non-claim"));
}
