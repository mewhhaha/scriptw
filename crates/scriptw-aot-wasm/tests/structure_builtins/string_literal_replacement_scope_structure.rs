use std::fs;
use std::path::Path;

const PROTOCOL: &str = include_str!("../../src/builtins/string/symbol_method.rs");
const STRING: &str = include_str!("../../src/builtins/string.rs");
const STRING_LITERAL_REPLACEMENT_SCOPE: &str =
    include_str!("../../src/builtins/string/string_literal_replacement_scope.rs");

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
fn string_literal_replacement_scope_is_a_private_non_copyable_two_variant_domain() {
    let domain = bounded(
        STRING_LITERAL_REPLACEMENT_SCOPE,
        "enum StringLiteralReplacementScope {",
        "\n\nimpl FunctionBuilder<'_> {",
    );
    let variants = domain
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && *line != "}")
        .collect::<Vec<_>>();

    assert_eq!(variants, ["FirstOccurrence,", "AllOccurrences,"]);
    let declaration_start = STRING_LITERAL_REPLACEMENT_SCOPE
        .find("enum StringLiteralReplacementScope {")
        .expect("missing literal-replacement scope");
    let preceding_declaration = STRING_LITERAL_REPLACEMENT_SCOPE[..declaration_start].trim();
    assert!(!preceding_declaration.contains("#[derive"));
    assert!(preceding_declaration.ends_with("use crate::functions::ArgumentListConstruction;"));
    for capability in ["Clone", "Copy", "Debug", "PartialEq", "Eq", "Default"] {
        assert!(!domain.contains(capability));
        assert!(!STRING_LITERAL_REPLACEMENT_SCOPE.contains(&format!(
            "impl {capability} for StringLiteralReplacementScope"
        )));
    }
    assert!(!STRING_LITERAL_REPLACEMENT_SCOPE.contains("pub enum StringLiteralReplacementScope"));
    assert!(
        !STRING_LITERAL_REPLACEMENT_SCOPE.contains("pub(crate) enum StringLiteralReplacementScope")
    );
    assert!(
        !STRING_LITERAL_REPLACEMENT_SCOPE.contains("pub(super) enum StringLiteralReplacementScope")
    );
    assert_eq!(
        STRING
            .matches("mod string_literal_replacement_scope;")
            .count(),
        1
    );
    assert!(!STRING.contains("mod string_literal_replacement_scope {"));
    assert!(!STRING.contains("string_literal_replacement_scope::"));
    assert!(!STRING.contains("StringLiteralReplacementScope"));
    assert!(!STRING.contains("emit_string_replace_literal_from_string_locals"));
    assert_eq!(
        STRING_LITERAL_REPLACEMENT_SCOPE
            .matches("StringLiteralReplacementScope")
            .count(),
        9
    );
    assert_eq!(
        STRING_LITERAL_REPLACEMENT_SCOPE
            .matches("FirstOccurrence")
            .count(),
        4
    );
    assert_eq!(
        STRING_LITERAL_REPLACEMENT_SCOPE
            .matches("AllOccurrences")
            .count(),
        4
    );
}

#[test]
fn literal_replace_helper_projects_break_or_continuation_once_in_instruction_order() {
    let emitter = bounded(
        STRING_LITERAL_REPLACEMENT_SCOPE,
        "    fn emit_native_string_replace_literal(",
        "    pub(super) fn emit_native_string_split_literal(",
    );
    assert!(emitter.contains("scope: StringLiteralReplacementScope,"));
    assert_eq!(emitter.matches("match scope {").count(), 1);
    let projection = without_whitespace(bounded(
        emitter,
        "match scope {",
        "self.pop_control(ControlFrameKind::Loop);",
    ));
    assert_eq!(projection, concat!(
        "StringLiteralReplacementScope::FirstOccurrence=>{self.emit_branch_to_target(positions_done,f)}",
        "StringLiteralReplacementScope::AllOccurrences=>{position.load(f);advance.load(f);",
        "f.instruction(&Instruction::I64Add);cursor.store(f);self.emit_branch_to_target(position_next,f);}}"
    ));
    for forbidden in [
        "scope ==",
        "scope !=",
        "matches!(scope",
        "_ =>",
        "unreachable!",
        "Default::default",
        "StandardBuiltinId",
    ] {
        assert!(!emitter.contains(forbidden));
    }
    let advance = without_whitespace(bounded(
        emitter,
        "needle_length.load(f);",
        "// Complete the position List before the first replacement callback.",
    ));
    assert!(advance.contains("advance.store(f);advance.load(f);f.instruction(&Instruction::I64Eqz);self.open_frame(ControlFrameKind::If,f);f.instruction(&Instruction::I64Const(1));advance.store(f);"));
    let append = emitter.find("positions.append(&value, s, f);").unwrap();
    let select = emitter.find("match scope {").unwrap();
    let finished = emitter
        .find("let positions = positions.finish(self, f);")
        .unwrap();
    let callback = emitter
        .find("self.emit_function_or_proxy_call_with_argv(")
        .unwrap();
    assert!(append < select && select < finished && finished < callback);
}

#[test]
fn replace_and_replace_all_fallbacks_choose_their_exact_scopes() {
    let protocol = without_whitespace(PROTOCOL);
    for (variant, wrapper) in [
        ("Replace", "first_occurrence"),
        ("ReplaceAll", "all_occurrences"),
    ] {
        assert_eq!(protocol.matches(&format!("NativeStringProtocol::{variant}=>b.emit_string_replace_literal_{wrapper}_from_string_locals(&input,&pattern,&second,&output,exit,f,)?")).count(), 1);
    }
    let owner = without_whitespace(STRING_LITERAL_REPLACEMENT_SCOPE);
    for variant in ["FirstOccurrence", "AllOccurrences"] {
        assert_eq!(owner.matches(&format!("self.emit_native_string_replace_literal(StringLiteralReplacementScope::{variant},input,search,replacement,output,exit,f,)" )).count(), 1);
    }
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_in_rust_sources(&source_root, "emit_native_string_replace_literal("),
        3
    );
    for wrapper in ["first_occurrence", "all_occurrences"] {
        assert_eq!(
            count_in_rust_sources(
                &source_root,
                &format!("emit_string_replace_literal_{wrapper}_from_string_locals(")
            ),
            2
        );
    }
}
