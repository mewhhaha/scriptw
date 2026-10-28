use std::fs;
use std::path::Path;

const ARRAY_SOURCE: &str = include_str!("../../src/builtins/array.rs");
const CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/array.rs");
const CORE_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_array_to_locale_string_core.js");
const INVOCATION_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_array_to_locale_string_invocation.js");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/native-array-gc-methods.md");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn normalized(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn assert_before(source: &str, earlier: &str, later: &str) {
    let earlier_offset = source
        .find(earlier)
        .unwrap_or_else(|| panic!("missing earlier operation `{earlier}`"));
    let later_offset = source
        .find(later)
        .unwrap_or_else(|| panic!("missing later operation `{later}`"));
    assert!(
        earlier_offset < later_offset,
        "`{earlier}` must precede `{later}`"
    );
}

fn count_in_rust_sources(dir: &Path, needle: &str) -> usize {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path.is_dir() {
                return count_in_rust_sources(&path, needle);
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return 0;
            }
            fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
                .matches(needle)
                .count()
        })
        .sum()
}

#[test]
fn receiver_kind_is_an_exact_private_non_derived_domain() {
    let declaration = bounded(
        ARRAY_SOURCE,
        "\nenum ToLocaleStringReceiverKind {",
        "\n}\n#[derive(Clone, Copy)]\nenum ArrayCallbackReceiverKind",
    );
    assert_eq!(normalized(declaration), "ArrayLike,TypedArray,");
    assert!(!ARRAY_SOURCE.contains("#[derive(Clone, Copy)]\nenum ToLocaleStringReceiverKind"));
    assert!(!ARRAY_SOURCE.contains("pub enum ToLocaleStringReceiverKind"));
    assert!(!ARRAY_SOURCE.contains("pub(crate) enum ToLocaleStringReceiverKind"));
    assert!(!ARRAY_SOURCE.contains("impl ToLocaleStringReceiverKind"));
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_in_rust_sources(&source_root, "ToLocaleStringReceiverKind"),
        12,
        "declaration, four fixed Join/Locale producers, owned parameter and six exhaustive consumer arms"
    );
    for variant in ["ArrayLike", "TypedArray"] {
        assert_eq!(
            count_in_rust_sources(
                &source_root,
                &format!("ToLocaleStringReceiverKind::{variant}")
            ),
            5
        );
    }
    for capability in ["Clone", "Copy", "Debug", "Default", "PartialEq", "Eq"] {
        assert!(!ARRAY_SOURCE.contains(&format!("{capability} for ToLocaleStringReceiverKind")));
    }
}

#[test]
fn exactly_four_string_entry_producers_choose_their_receiver_and_operation() {
    for (name, next, receiver, operation) in [
        (
            "compile_array_prototype_join_builtin",
            "compile_typed_array_prototype_join_builtin",
            "ArrayLike",
            "Join",
        ),
        (
            "compile_typed_array_prototype_join_builtin",
            "compile_array_prototype_to_locale_string_builtin",
            "TypedArray",
            "Join",
        ),
        (
            "compile_array_prototype_to_locale_string_builtin",
            "compile_typed_array_prototype_to_locale_string_builtin",
            "ArrayLike",
            "Locale",
        ),
        (
            "compile_typed_array_prototype_to_locale_string_builtin",
            "compile_array_string_operation",
            "TypedArray",
            "Locale",
        ),
    ] {
        let entry = normalized(bounded(
            ARRAY_SOURCE,
            &format!("fn {name}("),
            &format!("fn {next}("),
        ));
        assert_eq!(entry.matches(&format!("self.compile_array_string_operation(ToLocaleStringReceiverKind::{receiver},ArrayStringOperation::{operation},f,)" )).count(), 1);
    }
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_in_rust_sources(&source_root, "compile_array_string_operation("),
        5
    );
}

#[test]
fn receiver_validation_precedes_exhaustive_locale_diagnostics_and_invocation() {
    let shared = normalized(bounded(
        ARRAY_SOURCE,
        "fn compile_array_string_operation(",
        "pub(crate) fn compile_typed_array_prototype_to_string_builtin(",
    ));
    assert!(shared.contains("receiver_kind:ToLocaleStringReceiverKind,"));
    assert_eq!(shared.matches("match&receiver_kind{").count(), 2);
    assert_eq!(shared.matches("matchreceiver_kind{").count(), 1);
    assert!(shared.contains("ToLocaleStringReceiverKind::ArrayLike=>{self.emit_array_like_length_snapshot(&receiver,length,&pending,f)?}"));
    assert!(shared.contains(
        "ToLocaleStringReceiverKind::TypedArray=>{letarray=self.emit_array_native_typed_receiver("
    ));
    for (variant, error) in [("ArrayLike", "ARRAY"), ("TypedArray", "TYPEDARRAY")] {
        assert!(shared.contains(&format!("ToLocaleStringReceiverKind::{variant}=>RuntimeErrorMessage::{error}_PROTOTYPE_TOLOCALESTRING_ELEMENT_METHOD_IS_NOT_CALLABLE")));
    }
    for forbidden in [
        "matches!(receiver_kind",
        "receiver_kind==",
        "receiver_kind!=",
        "_=>",
        "unreachable!",
    ] {
        assert!(!shared.contains(forbidden));
    }
    assert_before(
        &shared,
        "match&receiver_kind{",
        "self.emit_array_native_get(&element,\"toLocaleString\",&pending,f)?;",
    );
    assert_before(
        &shared,
        "self.emit_is_callable_i32(&method,f)?;",
        "matchreceiver_kind{",
    );
    assert_before(
        &shared,
        "matchreceiver_kind{",
        "self.emit_function_or_proxy_call_with_argv(&method,&element,&argv,&pending,f)?;",
    );
    assert!(shared.contains("self.emit_pre_evaluated_arg_vector(&[&argument,&options],f)"));
}

#[test]
fn contract_and_existing_product_witnesses_pin_both_entries() {
    assert!(CONTRACT.contains("ToLocaleStringReceiverKind"));
    assert!(CONTRACT
        .contains("cargo test -p scriptw-aot-wasm --test structure_builtins -- to_locale_string_receiver_kind_structure::"));
    assert!(CONTRACT.contains("ToLocaleStringReceiverKind"));
    for registration in [
        "fn run_wasm_backend_succeeds_for_supported_array_to_locale_string_fixture()",
        "fn run_wasm_backend_succeeds_for_array_to_locale_string_invocation_fixture()",
    ] {
        assert!(CLI_TESTS.contains(registration), "missing `{registration}`");
    }
    for marker in [
        "Array.prototype.toLocaleString.call(tracking)",
        "tracking.toLocaleString()",
        "fixed.toLocaleString()",
    ] {
        assert!(
            CORE_FIXTURE.contains(marker),
            "missing core marker `{marker}`"
        );
    }
    for marker in [
        "otherArrayToLocaleString.call([{ toLocaleString: 0 }])",
        "otherTypedArrayToLocaleString.call(new other.Uint8Array([1]))",
        "Array.prototype.toLocaleString.call([proxyElement])",
    ] {
        assert!(
            INVOCATION_FIXTURE.contains(marker),
            "missing invocation marker `{marker}`"
        );
    }
}
