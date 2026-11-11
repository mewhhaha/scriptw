use std::fs;
use std::path::Path;

const STANDARD: &str = include_str!("../../src/builtins/standard.rs");
const STRING: &str = include_str!("../../src/builtins/string.rs");
const SYMBOL_METHOD: &str = include_str!("../../src/builtins/string/symbol_method.rs");
fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker `{start}`"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker `{end}` after `{start}`"))
        .0
}

fn without_whitespace(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
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
fn string_symbol_hook_operation_is_the_six_row_non_copyable_shared_domain() {
    let domain = bounded(
        SYMBOL_METHOD,
        "enum NativeStringProtocol {",
        "impl NativeStringProtocol {",
    );
    assert_eq!(
        without_whitespace(domain),
        "Match,MatchAll,Replace,ReplaceAll,Search,Split,}"
    );
    assert!(!SYMBOL_METHOD.contains("#[derive(Clone, Copy)]\nenum NativeStringProtocol"));
    for capability in ["Clone", "Copy", "Debug", "PartialEq", "Eq", "Default"] {
        assert!(!SYMBOL_METHOD.contains(&format!("impl {capability} for NativeStringProtocol")));
    }
    assert!(!SYMBOL_METHOD.contains("pub(super) enum NativeStringProtocol"));
    let key = without_whitespace(bounded(
        SYMBOL_METHOD,
        "impl NativeStringProtocol {",
        "impl FunctionBuilder<'_> {",
    ));
    assert!(key.contains("fnkey(&self)->StringSymbolMethodKey"));
    for mapping in [
        "Self::Match=>StringSymbolMethodKey::Match",
        "Self::MatchAll=>StringSymbolMethodKey::MatchAll",
        "Self::Replace|Self::ReplaceAll=>StringSymbolMethodKey::Replace",
        "Self::Search=>StringSymbolMethodKey::Search",
        "Self::Split=>StringSymbolMethodKey::Split",
    ] {
        assert!(key.contains(mapping));
    }
    assert!(!key.contains("_=>"));
    assert!(!STRING.contains("NativeStringProtocol"));
}

#[test]
fn symbol_hook_emitter_keeps_closed_policies_and_consumes_one_optional_method() {
    let emitter = bounded(SYMBOL_METHOD, "fn emit_native_string_protocol(", "\n}");
    assert!(emitter.contains("operation: NativeStringProtocol,"));
    for forbidden in [
        "builtin: StandardBuiltinId",
        "passes_second_arg",
        ": bool",
        "matches!(operation",
        "operation ==",
        "operation !=",
        "_ =>",
        "unreachable!",
    ] {
        assert!(!emitter.contains(forbidden), "forbidden `{forbidden}`");
    }
    let normalized = without_whitespace(emitter);
    assert_eq!(
        normalized.matches("match&operation{").count(),
        3,
        "global requirement, argument shape, and fallback flags are exhaustive borrowed policies"
    );
    let require = normalized
        .find("self.emit_native_string_require_coercible(&receiver,")
        .unwrap();
    let flags = normalized
        .find("self.emit_native_regexp_get(&pattern,\"flags\",")
        .unwrap();
    let method = normalized
        .find("letmethod=NullableStringSymbolMethod::get_method(")
        .unwrap();
    let dispatch = normalized.find("method.call_or_fallback(").unwrap();
    assert!(require < flags && flags < method && method < dispatch);
    let optional = without_whitespace(bounded(
        SYMBOL_METHOD,
        "pub(super) struct NullableStringSymbolMethod {",
        "/// Required Invoke consumes",
    ));
    assert!(optional.contains("receiver:ValueLocals,method:ValueLocals,"));
    assert!(optional.contains("fncall_or_fallback(self,"));
    assert_eq!(
        optional
            .matches("b.emit_object_read(receiver,receiver,&property,&pending,f)?;")
            .count(),
        1
    );
    assert!(optional.contains(
        "b.emit_function_or_proxy_call_with_argv(&self.method,&self.receiver,&vector,output,f)?;"
    ));
    assert!(!optional.contains("#[derive"));
    let required = without_whitespace(bounded(
        SYMBOL_METHOD,
        "struct RequiredStringSymbolMethod(",
        "enum NativeStringProtocol {",
    ));
    assert!(required.contains("fninvoke(self,"));
    let signature = required
        .split_once("fninvoke(")
        .unwrap()
        .1
        .split_once(")->Result")
        .unwrap()
        .0;
    assert!(!signature.contains("fallback"));
    assert!(required.contains("STRING_PROTOTYPE_SYMBOL_HOOK_IS_NOT_CALLABLE"));
    assert!(!required.contains("#[derive"));
}

#[test]
fn private_fallback_matches_all_six_operations_to_their_exact_algorithms() {
    let emitter = without_whitespace(bounded(
        SYMBOL_METHOD,
        "fn emit_native_string_protocol(",
        "\n}",
    ));
    assert_eq!(emitter.matches("matchoperation{").count(), 1);
    for (variant, wrapper) in [
        ("Replace", "first_occurrence"),
        ("ReplaceAll", "all_occurrences"),
    ] {
        assert!(emitter.contains(&format!("NativeStringProtocol::{variant}=>b.emit_string_replace_literal_{wrapper}_from_string_locals(")));
    }
    assert!(emitter.contains("NativeStringProtocol::Split=>{b.emit_native_string_split_literal("));
    assert!(emitter.contains("NativeStringProtocol::Match|NativeStringProtocol::MatchAll|NativeStringProtocol::Search=>{"));
    let create = emitter.find("b.emit_native_regexp_create(").unwrap();
    let required = emitter
        .find("RequiredStringSymbolMethod::get_method(")
        .unwrap();
    let invoke = emitter.find("required.invoke(").unwrap();
    assert!(create < required && required < invoke);
}

#[test]
fn standard_dispatch_names_all_six_fixed_entries() {
    let standard = without_whitespace(STANDARD);
    let owner = without_whitespace(SYMBOL_METHOD);
    for (variant, entry, protocol) in [
        ("Match", "match", "Match"),
        ("MatchAll", "match_all", "MatchAll"),
        ("Replace", "replace", "Replace"),
        ("ReplaceAll", "replace_all", "ReplaceAll"),
        ("Search", "search", "Search"),
        ("Split", "split", "Split"),
    ] {
        assert!(standard.contains(&format!("StandardBuiltinId::StringPrototype{variant}=>{{self.emit_string_{entry}_builtin(function)?;}}")));
        assert_eq!(
            owner
                .matches(&format!(
                    "self.emit_native_string_protocol(NativeStringProtocol::{protocol},f)"
                ))
                .count(),
            1
        );
    }
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_in_rust_sources(&source_root, "emit_native_string_protocol("),
        7
    );
}
