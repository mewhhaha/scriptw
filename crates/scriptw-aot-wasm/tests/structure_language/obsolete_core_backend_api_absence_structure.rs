use std::fs;
use std::path::Path;

fn count_identifier_in_rust_sources(dir: &Path, identifier: &str) -> usize {
    fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", dir.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path.is_dir() {
                return count_identifier_in_rust_sources(&path, identifier);
            }
            if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                return 0;
            }
            fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
                .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
                .filter(|token| *token == identifier)
                .count()
        })
        .sum()
}

#[test]
fn obsolete_core_backend_apis_are_absent() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for name in [
        "static_number_expr_value",
        "buffer_memarg32",
        "buffer_memarg16",
        "emit_store_realm_type_error_prototype",
        "standard_builtin_prototype_global_index",
        "emit_string_substring_method_call",
        "emit_string_char_code_at_from_locals",
        "buffer_memarg64",
        "buffer_memarg8",
        "standard_builtin_function_global_index",
        "standard_builtin_constructor_global_index",
    ] {
        assert_eq!(
            count_identifier_in_rust_sources(&source_root, name),
            0,
            "`{name}`"
        );
    }
}

#[test]
fn live_neighboring_apis_remain_reachable() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // A live neighboring API needs its definition and at least one other
    // source occurrence. Optimizations may remove callers without retiring the
    // API; frozen occurrence totals do not express that reachability contract.
    for name in [
        "compile_string_substring_range_builtin",
        "emit_gc_string_slice",
        "emit_gc_string_code_unit_i32",
        "emit_intrinsic_callable",
        "standard_builtin_function_realm_slot",
    ] {
        assert!(
            count_identifier_in_rust_sources(&source_root, name) >= 2,
            "`{name}` must retain its definition and a caller"
        );
    }
}
