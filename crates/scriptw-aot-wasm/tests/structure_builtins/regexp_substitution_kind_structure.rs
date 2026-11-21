use std::fs;
use std::path::Path;

const STRING_PARENT: &str = include_str!("../../src/builtins/string.rs");
const REGEXP_SUBSTITUTION: &str = include_str!("../../src/builtins/string/regexp_substitution.rs");
const REGEXP_PROTOCOL: &str = include_str!("../../src/builtins/string/regexp_protocol.rs");
const LITERAL_REPLACEMENT: &str =
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

fn assert_before(source: &str, earlier: &str, later: &str) {
    assert!(source.find(earlier).expect(earlier) < source.find(later).expect(later));
}

#[test]
fn substitution_owns_four_literal_rows_and_completed_capture_domains() {
    let domain = bounded(REGEXP_SUBSTITUTION, "enum LiteralSubstitution {", "}");
    assert_eq!(
        domain
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>(),
        ["Dollar,", "Matched,", "Prefix,", "Suffix,"]
    );
    assert!(!REGEXP_SUBSTITUTION.contains("pub enum LiteralSubstitution"));
    assert!(!REGEXP_SUBSTITUTION.contains("pub(crate) enum LiteralSubstitution"));
    assert!(!REGEXP_SUBSTITUTION.contains("pub(super) enum LiteralSubstitution"));
    assert!(!REGEXP_SUBSTITUTION.contains("#[derive"));
    assert_eq!(STRING_PARENT.matches("mod regexp_substitution;").count(), 1);
    let captures = bounded(
        REGEXP_SUBSTITUTION,
        "pub(super) struct SubstitutionCaptureList(",
        "impl FunctionBuilder<'_>",
    );
    for marker in [
        "ArgumentListConstruction)",
        "pub(super) struct SubstitutionCaptures(GcLocal<ValueArray>)",
        "text: &GcLocal<StringValue>",
        "pub(super) fn append_undefined(&self",
        "SubstitutionCaptures(self.0.finish(b, f))",
        "pub(super) fn values(&self) -> &GcLocal<ValueArray>",
        "pub(super) fn clear(self, f: &mut Function)",
        "pub(super) struct NamedSubstitutionCaptures(ValueLocals)",
    ] {
        assert!(captures.contains(marker), "{marker}");
    }
    assert!(!captures.contains("pub(super) fn append("));
    let named = bounded(
        REGEXP_SUBSTITUTION,
        "pub(super) fn emit_complete_named_substitution_captures(",
        "pub(super) fn emit_regexp_get_substitution(",
    );
    assert_before(
        named,
        "WasmRuntimeValueTag::Undefined.tag()",
        "self.emit_value_to_current_function_realm_object_locals(input, &pending, f)",
    );
    assert_before(
        named,
        "self.emit_value_to_current_function_realm_object_locals(input, &pending, f)",
        "self.emit_native_string_abrupt_exit(&pending, output, exit, f)",
    );
    assert_before(
        named,
        "self.emit_native_string_abrupt_exit(&pending, output, exit, f)",
        "Ok(NamedSubstitutionCaptures(value))",
    );
    let emitter = REGEXP_SUBSTITUTION
        .split_once("pub(super) fn emit_regexp_get_substitution(")
        .unwrap()
        .1;
    assert!(emitter.contains("captures: &SubstitutionCaptures"));
    assert!(emitter.contains("named: &NamedSubstitutionCaptures"));
    assert_eq!(
        REGEXP_PROTOCOL
            .matches("b.emit_regexp_get_substitution(")
            .count(),
        1
    );
    assert_eq!(
        LITERAL_REPLACEMENT
            .matches("self.emit_regexp_get_substitution(")
            .count(),
        1
    );
}

#[test]
fn literal_recognizers_are_typed_and_exhaustively_select_their_meaning() {
    let handler = bounded(
        REGEXP_SUBSTITUTION,
        "for (code, kind) in [",
        "        next_unit.load(f);\n        f.instruction(&Instruction::I32Const(48))",
    );
    let code = without_whitespace(handler);
    for row in [
        "(36,LiteralSubstitution::Dollar)",
        "(38,LiteralSubstitution::Matched)",
        "(96,LiteralSubstitution::Prefix)",
        "(39,LiteralSubstitution::Suffix)",
    ] {
        assert_eq!(code.matches(row).count(), 1, "{row}");
    }
    assert_eq!(handler.matches("match kind {").count(), 1);
    for arm in [
        "LiteralSubstitution::Dollar=>{piece.replace(self.emit_native_string_static(\"$\",f),f)}",
        "LiteralSubstitution::Matched=>piece.replace(matched.load(s,f),f)",
        "LiteralSubstitution::Prefix=>{piece.replace(self.emit_gc_string_slice(input,zero,position,f),f)}",
        "LiteralSubstitution::Suffix=>{piece.replace(self.emit_gc_string_slice(input,end,input_length,f),f)}",
    ] { assert!(code.contains(arm), "{arm}"); }
    assert!(!handler.contains("_ =>"));
    assert!(!handler.contains("unreachable!"));
    assert_before(handler, "Instruction::I64Const(2)", "consumed.store(f)");
    assert_before(handler, "consumed.store(f)", "match kind");
}

#[test]
fn numbered_and_named_captures_require_valid_indices_and_observable_gets() {
    let code = without_whitespace(REGEXP_SUBSTITUTION);
    for proof in [
        "capture.load(f);f.instruction(&Instruction::I64Eqz);f.instruction(&Instruction::I32Eqz);capture.load(f);capture_count.load(f);f.instruction(&Instruction::I64LeU)",
        "candidate.load(f);f.instruction(&Instruction::I64Eqz);f.instruction(&Instruction::I32Eqz);candidate.load(f);capture_count.load(f);f.instruction(&Instruction::I64LeU)",
        "capture.load(f);f.instruction(&Instruction::I64Const(10));f.instruction(&Instruction::I64Mul)",
        "consumed.load(f);f.instruction(&Instruction::I64Const(1));f.instruction(&Instruction::I64GtU)",
        "capture.load(f);f.instruction(&Instruction::I64Const(1));f.instruction(&Instruction::I64Sub)",
        "self.emit_argument_vector_entry_to_value(captures.values(),ordinal,&value,f)",
    ] { assert!(code.contains(proof), "{proof}"); }
    let named = bounded(
        REGEXP_SUBSTITUTION,
        "        next_unit.load(f);\n        f.instruction(&Instruction::I32Const(60))",
        "        self.emit_native_gc_string_length(&piece, piece_length, f);",
    );
    for marker in [
        "named.0.tag().load(f)",
        "WasmRuntimeValueTag::Undefined.tag()",
        "Instruction::I32Const(62)",
        "found.load(f)",
        "PropertyKeyLocals::from_string(s, &name, f)",
        "self.emit_object_read(&named.0, &named.0, &key, &pending, f)",
        "self.emit_value_to_string_payload(&value, &pending, f)",
    ] {
        assert!(named.contains(marker), "{marker}");
    }
    assert_eq!(named.matches("self.emit_object_read(").count(), 1);
    assert_eq!(
        named
            .matches("self.emit_native_string_abrupt_exit(")
            .count(),
        2
    );
    assert_before(named, "found.load(f)", "self.emit_object_read(");
    assert_before(
        named,
        "self.emit_object_read(",
        "self.emit_native_string_abrupt_exit(",
    );
    assert_before(
        named,
        "WasmRuntimeValueTag::Undefined.tag()",
        "self.emit_value_to_string_payload(",
    );
    assert_before(
        named,
        "self.emit_value_to_string_payload(",
        "consumed.store(f)",
    );
}

#[test]
fn consumed_and_source_updates_follow_the_completed_replacement_piece() {
    let code = without_whitespace(REGEXP_SUBSTITUTION);
    for width in [1, 2, 3] {
        assert!(code.contains(&format!(
            "Instruction::I64Const({width}));consumed.store(f)"
        )));
    }
    assert!(code.contains("scan.load(f);index.load(f);f.instruction(&Instruction::I64Sub);f.instruction(&Instruction::I64Const(1));f.instruction(&Instruction::I64Add);consumed.store(f)"));
    let completed = bounded(
        REGEXP_SUBSTITUTION,
        "        self.emit_native_gc_string_length(&piece, piece_length, f);",
        "        pending.clear(f);",
    );
    // The accumulated length, with this piece added, is checked against the
    // String length limit before the piece is appended.
    assert!(completed.contains("RuntimeErrorMessage::STRING_LENGTH_EXCEEDS_IMPLEMENTATION_LIMIT"));
    assert_before(
        completed,
        "total_length.store(f)",
        "Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH)",
    );
    assert_before(
        completed,
        "Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH)",
        "self.emit_native_string_error_if(",
    );
    assert_before(
        completed,
        "self.emit_native_string_error_if(",
        "pieces.append(&piece, s, f)",
    );
    assert!(without_whitespace(completed).contains("pieces.append(&piece,s,f);piece.clear(f);index.load(f);consumed.load(f);f.instruction(&Instruction::I64Add);index.store(f)"));
    assert_before(
        completed,
        "self.pop_control(ControlFrameKind::Block)",
        "pieces.publish(self, total_length, f)",
    );
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(
        count_in_rust_sources(&source_root, "LiteralSubstitution"),
        9
    );
    assert_eq!(
        count_in_rust_sources(&source_root, "emit_regexp_get_substitution("),
        3
    );
    assert_eq!(
        count_in_rust_sources(&source_root, "RegExpSubstitutionKind"),
        0
    );
    assert!(!REGEXP_SUBSTITUTION.contains("substitution_kind_local"));
}
