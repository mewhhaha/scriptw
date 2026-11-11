use std::fs;
use std::path::Path;

fn count_identifier(source: &str, identifier: &str) -> usize {
    source
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .filter(|token| *token == identifier)
        .count()
}

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
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
            count_identifier(&source, identifier)
        })
        .sum()
}

#[test]
fn obsolete_builtin_emitters_are_absent_from_backend_sources() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for name in [
        "emit_date_time_within_day",
        "emit_throw_if_shared_array_buffer",
        "emit_throw_if_array_buffer_immutable",
        "emit_string_match_all_global_ascii_word_iterator_from_string_locals_from_start",
        "emit_string_match_all_global_ascii_word_iterator_from_string_locals",
    ] {
        assert_eq!(
            count_identifier_in_rust_sources(&source_root, name),
            0,
            "`{name}`"
        );
    }
}

#[test]
fn live_neighboring_emitters_remain_owned_and_reachable() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for (name, expected) in [
        ("emit_date_positive_mod", 8),
        ("emit_date_make_time", 2),
        ("emit_binary_buffer_immutable_i32", 3),
        ("emit_string_match_all_builtin", 2),
        ("emit_regexp_prototype_symbol_match_all_builtin", 2),
    ] {
        assert_eq!(
            count_identifier_in_rust_sources(&source_root, name),
            expected,
            "`{name}`"
        );
    }

    // Date math is shared by the private constructor, component setters and parser.
    // Keep the helper census attached to those actual owners after their extraction.
    for (owner, positive_mod_mentions, make_time_mentions) in [
        ("builtins/date.rs", 6, 2),
        ("builtins/date/local_string.rs", 0, 0),
        ("builtins/date/components.rs", 1, 0),
        ("builtins/date/date_string_parse/components.rs", 1, 0),
    ] {
        let path = source_root.join(owner);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        assert_eq!(
            count_identifier(&source, "emit_date_positive_mod"),
            positive_mod_mentions,
            "PositiveMod owner `{owner}`"
        );
        assert_eq!(
            count_identifier(&source, "emit_date_make_time"),
            make_time_mentions,
            "MakeTime owner `{owner}`"
        );
    }

    let date_source = fs::read_to_string(source_root.join("builtins/date.rs"))
        .expect("failed to read Date math owner");
    assert_eq!(date_source.matches("fn emit_date_make_time(").count(), 1);
    let completed_make_date = date_source
        .split_once("fn emit_date_make_date_from_components_into(")
        .expect("completed MakeDate factory is required")
        .1
        .split_once("fn emit_date_make_full_year(")
        .expect("MakeFullYear follows the completed MakeDate factory")
        .0;
    assert_eq!(
        count_identifier(completed_make_date, "emit_date_make_time"),
        1,
        "the sole MakeTime call belongs to the completed MakeDate factory"
    );
}
