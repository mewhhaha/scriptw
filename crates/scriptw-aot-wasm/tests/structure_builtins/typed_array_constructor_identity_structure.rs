const PLANNING_SOURCE: &str = include_str!("../../src/planning.rs");
const CATALOG_SOURCE: &str = include_str!("../../../scriptw-ir/src/builtins/catalog.rs");
const NAMES_SOURCE: &str = include_str!("../../../scriptw-ir/src/names.rs");
const CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/typed_array.rs");
const CLI_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_typedarray_intrinsic_identity.js");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

#[test]
fn typed_array_intrinsic_has_one_hidden_constructable_builtin_identity() {
    assert_eq!(
        NAMES_SOURCE
            .matches(
                "pub const BUILTIN_TYPED_ARRAY_CONSTRUCTOR_FUNCTION_ID: &str = \"$builtin.TypedArray\";"
            )
            .count(),
        1
    );

    let catalog = bounded(CATALOG_SOURCE, "    TypedArrayConstructor {", "\n    }\n");
    for field in [
        "=> BUILTIN_TYPED_ARRAY_CONSTRUCTOR_FUNCTION_ID,",
        "debug: \"%TypedArray%\",",
        "flags: [CONSTRUCTABLE, ALWAYS_THROWS],",
        "installer: None,",
        "native: TYPED_ARRAY_NAME,",
    ] {
        assert_eq!(catalog.matches(field).count(), 1, "missing `{field}`");
    }
    assert!(!catalog.contains("global:"));
    assert!(!catalog.contains("global_name:"));
}

#[test]
fn planning_requires_hidden_typed_array_intrinsic_and_zero_length() {
    let concrete_constructor_dependencies = bounded(
        PLANNING_SOURCE,
        "        if is_typed_array_constructor(builtin) {",
        "        if builtin == StandardBuiltinId::ObjectConstructor {",
    );
    assert_eq!(
        concrete_constructor_dependencies
            .matches("self.require_standard_builtin(StandardBuiltinId::TypedArrayConstructor);")
            .count(),
        1
    );

    let length_table = bounded(
        PLANNING_SOURCE,
        "pub(crate) fn standard_builtin_length(builtin: StandardBuiltinId) -> u64 {",
        "pub(crate) fn host_builtin_length(builtin: HostBuiltinId) -> u64 {",
    );
    let length_suffix = length_table
        .split_once("StandardBuiltinId::TypedArrayConstructor")
        .expect("TypedArray intrinsic length entry")
        .1;
    let result = length_suffix
        .find("=>")
        .expect("TypedArray intrinsic length match result");
    assert!(
        length_suffix[result..].starts_with("=> 0,"),
        "the TypedArray intrinsic must belong to a zero-length match arm"
    );
}

#[test]
fn focused_cli_fixture_covers_both_realm_identities_and_constructor_protocols() {
    let cli_test = CLI_TESTS
        .split_once("fn run_wasm_backend_preserves_hidden_typedarray_constructor_identity()")
        .expect("focused hidden TypedArray CLI test")
        .1;
    assert!(cli_test.contains("wasm_typedarray_intrinsic_identity.js"));
    assert!(cli_test.contains("boolean(true)"));

    for marker in [
        "entry realm",
        "created realm",
        "dedicated identity",
        "native source",
        "per-realm TypedArray identity",
        "per-realm TypedArray prototype identity",
        "name descriptor",
        "length descriptor",
        "prototype descriptor",
        "constructor descriptor",
        "direct call",
        "direct construct",
        "reflected target",
        "IsConstructor newTarget prototype",
    ] {
        assert!(CLI_FIXTURE.contains(marker), "missing `{marker}` control");
    }
    assert_eq!(
        CLI_FIXTURE
            .matches("Object.getPrototypeOf(constructors[i])")
            .count(),
        1
    );
    assert_eq!(
        CLI_FIXTURE
            .matches("realmGlobal.Reflect.construct(")
            .count(),
        2
    );
    assert_eq!(
        CLI_FIXTURE
            .matches("Object.getOwnPropertyDescriptor(realmGlobal, \"TypedArray\")")
            .count(),
        1
    );
}
