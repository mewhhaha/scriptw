use std::{fs, path::Path};

const FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_regexp_unicode_sets_class_strings.js");
const CLI_TEST_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/regexp.rs");
const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/regexp.rs");
const MATCHER_SOURCE: &str = include_str!("../../src/builtins/regexp.rs");
const CHOICE_SOURCE: &str =
    include_str!("../../src/builtins/regexp/matcher_workspace/choice_entries.rs");
const PROGRAM_SOURCE: &str = include_str!("../../../scriptw-ir/src/regexp/program.rs");
const TEST262_RUNNER_SOURCE: &str = include_str!("../../../scriptw-test262/src/lib.rs");
const SHORTCUT_ALLOWLIST: &str = include_str!("../../../../test262/backlog/shortcut-allowlist.tsv");
const KNOWN_FAILURES: &str = include_str!("../../../scriptw-cli/tests/known-failures.tsv");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/regexp-unicode-set-finite-string-algebra.md"
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

fn positions_in_order(source: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let offset = source[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing marker after byte {cursor}: {marker}"));
        cursor += offset + marker.len();
    }
}

const EXACT_TEST262: [(&str, &str); 27] = [
    (
        "character-class-escape-union-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-class-escape-union-string-literal.js"),
    ),
    (
        "character-class-union-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-class-union-string-literal.js"),
    ),
    (
        "character-property-escape-union-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-property-escape-union-string-literal.js"),
    ),
    (
        "character-union-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-union-string-literal.js"),
    ),
    (
        "string-literal-union-character-class-escape.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-union-character-class-escape.js"),
    ),
    (
        "string-literal-union-character-class.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-union-character-class.js"),
    ),
    (
        "string-literal-union-character-property-escape.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-union-character-property-escape.js"),
    ),
    (
        "string-literal-union-character.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-union-character.js"),
    ),
    (
        "string-literal-union-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-union-string-literal.js"),
    ),
    (
        "character-class-escape-intersection-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-class-escape-intersection-string-literal.js"),
    ),
    (
        "character-class-intersection-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-class-intersection-string-literal.js"),
    ),
    (
        "character-property-escape-intersection-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-property-escape-intersection-string-literal.js"),
    ),
    (
        "character-intersection-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-intersection-string-literal.js"),
    ),
    (
        "string-literal-intersection-character-class-escape.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-intersection-character-class-escape.js"),
    ),
    (
        "string-literal-intersection-character-class.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-intersection-character-class.js"),
    ),
    (
        "string-literal-intersection-character-property-escape.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-intersection-character-property-escape.js"),
    ),
    (
        "string-literal-intersection-character.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-intersection-character.js"),
    ),
    (
        "string-literal-intersection-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-intersection-string-literal.js"),
    ),
    (
        "character-class-escape-difference-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-class-escape-difference-string-literal.js"),
    ),
    (
        "character-class-difference-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-class-difference-string-literal.js"),
    ),
    (
        "character-property-escape-difference-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-property-escape-difference-string-literal.js"),
    ),
    (
        "character-difference-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/character-difference-string-literal.js"),
    ),
    (
        "string-literal-difference-character-class-escape.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-difference-character-class-escape.js"),
    ),
    (
        "string-literal-difference-character-class.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-difference-character-class.js"),
    ),
    (
        "string-literal-difference-character-property-escape.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-difference-character-property-escape.js"),
    ),
    (
        "string-literal-difference-character.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-difference-character.js"),
    ),
    (
        "string-literal-difference-string-literal.js",
        include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/unicodeSets/generated/string-literal-difference-string-literal.js"),
    ),
];

const KEYCAP_NEGATIVE_PATHS: [&str; 3] = [
    "built-ins/RegExp/property-escapes/generated/strings/Emoji_Keycap_Sequence-negative-CharacterClass.js",
    "built-ins/RegExp/property-escapes/generated/strings/Emoji_Keycap_Sequence-negative-P.js",
    "built-ins/RegExp/property-escapes/generated/strings/Emoji_Keycap_Sequence-negative-u.js",
];

fn current_pin_keycap_inventory() -> Vec<(String, String)> {
    let test_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test262/vendor/test262/test");
    let mut inventory = Vec::new();
    for relative_dir in [
        "built-ins/RegExp/property-escapes/generated/strings",
        "built-ins/RegExp/unicodeSets/generated",
    ] {
        let directory = test_root.join(relative_dir);
        for entry in fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()))
        {
            let path = entry
                .expect("Test262 directory entry should be readable")
                .path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("js") {
                continue;
            }
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
            if !source.contains("Emoji_Keycap_Sequence") {
                continue;
            }
            let relative_path = path
                .strip_prefix(&test_root)
                .expect("keycap inventory must stay inside the pinned Test262 root")
                .to_string_lossy()
                .into_owned();
            inventory.push((relative_path, source));
        }
    }
    inventory.sort_by(|left, right| left.0.cmp(&right.0));
    inventory
}

#[test]
fn finite_class_set_is_the_only_canonical_string_algebra() {
    for domain in [
        "struct FiniteClassSet {\n    ranges: Vec<(u32, u32)>,\n    strings: BTreeSet<Vec<u32>>,\n}",
        "struct FiniteClassSetAtom {\n    multi_code_point_strings: Vec<Vec<RegExpInstruction>>,\n    singleton: RegExpInstruction,\n    contains_empty: bool,\n}",
    ] {
        assert!(IR_SOURCE.contains(domain), "missing closed domain: {domain}");
    }
    assert!(!IR_SOURCE.contains("RequiresClassStringSemantics"));

    let algebra = bounded(
        IR_SOURCE,
        "impl FiniteClassSet {",
        "\nstruct ClassSetValue {",
    );
    positions_in_order(
        algebra,
        &[
            "fn class_strings(alternatives: BTreeSet<Vec<u32>>, folding: CaseFolding) -> Self",
            "[code_point] => ranges.push((*code_point, *code_point))",
            "[] | [_, _, ..]",
            "strings.insert(alternative);",
            "ranges: case_close_ranges(&normalize_ranges(ranges), folding)",
            "fn union(self, right: Self) -> Self",
            "strings.extend(right.strings);",
            "fn intersection(self, right: Self) -> Self",
            "self.strings.intersection(&right.strings)",
            "fn subtraction(self, right: Self) -> Self",
            "self.strings.difference(&right.strings)",
        ],
    );

    let atom = bounded(
        IR_SOURCE,
        "impl FiniteClassSetAtom {",
        "/// Parses a `v`-mode `ClassSetExpression`",
    );
    positions_in_order(
        atom,
        &[
            "let FiniteClassSet { ranges, strings }",
            "let singleton = finish_range_set",
            "let contains_empty = strings.iter().any(Vec::is_empty);",
            ".filter(|string| !string.is_empty())",
            "apply_modifiers(",
            "multi_code_point_strings.sort_by_key(|string| std::cmp::Reverse(string.len()));",
            "multi_code_point_strings,",
            "singleton,",
            "contains_empty,",
        ],
    );
    assert!(IR_SOURCE.contains("ParsedAtom::FiniteClassSet(atom) => atom.contains_empty,"));
}

#[test]
fn one_lowerer_emits_longest_singleton_empty_priority_in_both_directions() {
    assert!(IR_SOURCE.contains("enum RegExpMatchDirection {\n    Forward,\n    Reverse,\n}"));
    assert!(IR_SOURCE.contains(
        "ParsedAtom::FiniteClassSet(atom) => {\n                self.finite_class_set_atom(atom, RegExpMatchDirection::Forward)"
    ));
    assert!(IR_SOURCE.contains(
        "ParsedAtom::FiniteClassSet(atom) => {\n                self.finite_class_set_atom(atom, RegExpMatchDirection::Reverse)"
    ));

    let producer = bounded(
        IR_SOURCE,
        "    fn finite_class_set_atom(\n",
        "    fn reverse_alternatives(\n",
    );
    positions_in_order(
        producer,
        &[
            "atom.multi_code_point_strings.len() + 1 + usize::from(atom.contains_empty)",
            "for index in 0..alternative_count",
            "self.push(RegExpInstruction::split(0, 0))?;",
            "self.finite_class_set_alternative(atom, index, direction)?;",
            "self.push(RegExpInstruction::jump(0))?;",
            "self.instructions[split] = RegExpInstruction::split(primary, fallback);",
            "fn finite_class_set_alternative(",
            "atom.multi_code_point_strings.get(index)",
            "RegExpMatchDirection::Forward",
            "for instruction in string",
            "RegExpMatchDirection::Reverse",
            "for instruction in string.iter().rev()",
            "index == atom.multi_code_point_strings.len()",
            "self.push(atom.singleton)",
            "debug_assert!(atom.contains_empty);",
        ],
    );
    assert!(!producer.contains("progress_split"));
    assert!(IR_SOURCE.contains("fn unicode_sets_finite_string_algebra()"));
}

#[test]
fn existing_aot_choices_and_shared_range_matcher_cover_both_directions() {
    let compact = |source: &str| source.split_whitespace().collect::<String>();
    let split = compact(bounded(
        MATCHER_SOURCE,
        "// `Split` records the fallback before taking the primary arm.",
        "// Nullable attempts retain",
    ));
    positions_in_order(
        &split,
        &[
            "REGEXP_OPCODE_SPLITasi64",
            "workspace.choices().push_snapshot(",
            "SnapshotChoice::Ordinary",
            "fallback:operand1",
            "origin:pc",
            "operand0.load(&mutfunction)",
            "pc.store(&mutfunction)",
        ],
    );
    let frame = compact(bounded(
        CHOICE_SOURCE,
        "fn push_snapshot(",
        "fn top_cursor(",
    ));
    for marker in [
        "cursor.byte",
        "cursor.utf16",
        "cursor.on_low_surrogate",
        "self.workspace.live_bytes.load(function)",
        "Instruction::MemoryCopy",
    ] {
        assert!(frame.contains(marker), "choice frame lost {marker}");
    }
    assert!(PROGRAM_SOURCE.contains("is_some_and(RegExpOpcode::is_choice)"));
    assert_eq!(
        MATCHER_SOURCE
            .matches("self.emit_regexp_unicode_property_mismatch(")
            .count(),
        2
    );
    let reverse=compact(bounded(MATCHER_SOURCE,
        concat!("        reverse_mode.load(&mut function);\n","        function.instruction(&Instruction::I32WrapI64);\n","        function.instruction(&Instruction::If(BlockType::Empty));\n","        opcode.load(&mut function);\n"),
        concat!("        function.instruction(&Instruction::Br(1));\n","        function.instruction(&Instruction::End);\n","        opcode.load(&mut function);\n","        function.instruction(&Instruction::I64Const(REGEXP_OPCODE_LITERAL_ASCII as i64));")));
    for marker in [
        "REGEXP_OPCODE_LITERAL_CODE_POINTasi64",
        "REGEXP_OPCODE_UNICODE_PROPERTYasi64",
        "Reversematchingusesthesamecanonicalrangeslice",
        "self.emit_regexp_unicode_property_mismatch(",
        "utf16_advance.load(&mutfunction)",
        "I64Sub",
    ] {
        assert!(reverse.contains(marker), "reverse matcher lost {marker}");
    }
}

#[test]
fn exact_inventory_is_nine_union_nine_intersection_and_nine_difference_files() {
    assert_eq!(EXACT_TEST262.len(), 27);
    assert_eq!(
        EXACT_TEST262
            .iter()
            .filter(|(path, _)| path.contains("-union-"))
            .count(),
        9
    );
    assert_eq!(
        EXACT_TEST262
            .iter()
            .filter(|(path, _)| path.contains("-intersection-"))
            .count(),
        9
    );
    assert_eq!(
        EXACT_TEST262
            .iter()
            .filter(|(path, _)| path.contains("-difference-"))
            .count(),
        9
    );

    for (path, source) in EXACT_TEST262 {
        assert!(
            path.contains("string-literal"),
            "inventory lost q operand: {path}"
        );
        assert!(
            !path.contains("property-of-strings-escape"),
            "inventory crossed the property-of-strings boundary: {path}"
        );
        for marker in [
            "regexp-v-flag",
            "testExtendedCharacterClass({",
            "\\q{0|2|4|9\\uFE0F\\u20E3}",
        ] {
            assert!(source.contains(marker), "{path} lost {marker}");
        }
        assert!(!source.contains("flags:"), "{path} stopped being unflagged");
    }

    for marker in [
        "That is 27 physical files and 54 strict/non-strict executions.",
        "The six adjacent generated files whose name contains",
        "Backward/lookbehind lowering preserves the same alternative priority",
        "The current static producer implements the finite tables for",
        "`Emoji_Keycap_Sequence`, `Basic_Emoji`, `RGI_Emoji_Flag_Sequence`",
        "`RGI_Emoji_Modifier_Sequence`, `RGI_Emoji_Tag_Sequence`, `RGI_Emoji_ZWJ_Sequence`",
        "and `RGI_Emoji`.",
        "does not add a new Wasm matcher opcode or data",
        "finite source and property strings lower to the existing ordered matcher",
    ] {
        assert!(CONTRACT.contains(marker), "contract lost {marker}");
    }
}

#[test]
fn exact_inventory_has_no_rewrite_materializer_or_known_failure_mask() {
    for (path, _) in EXACT_TEST262 {
        for source in [TEST262_RUNNER_SOURCE, SHORTCUT_ALLOWLIST, KNOWN_FAILURES] {
            assert!(!source.contains(path), "{path} gained an exact mask");
        }
    }

    let keycaps = current_pin_keycap_inventory();
    assert_eq!(keycaps.len(), 37, "current-pin keycap inventory drifted");
    assert_eq!(
        keycaps
            .iter()
            .filter(|(path, _)| path.contains("/property-escapes/"))
            .count(),
        4
    );
    assert_eq!(
        keycaps
            .iter()
            .filter(|(path, _)| path.contains("/unicodeSets/"))
            .count(),
        33
    );
    for operator in ["-union-", "-intersection-", "-difference-"] {
        assert_eq!(
            keycaps
                .iter()
                .filter(|(path, _)| path.contains("/unicodeSets/") && path.contains(operator))
                .count(),
            11,
            "current-pin keycap {operator} inventory drifted"
        );
    }
    assert_eq!(
        keycaps
            .iter()
            .filter(|(path, _)| {
                path.rsplit('/')
                    .next()
                    .is_some_and(|name| name.starts_with("property-of-strings-escape-"))
            })
            .count(),
        18
    );
    assert_eq!(
        keycaps
            .iter()
            .filter(|(path, _)| path.ends_with("-property-of-strings-escape.js"))
            .count(),
        18
    );

    let mut negative_count = 0;
    for (path, source) in &keycaps {
        assert!(!source.contains("flags:"), "{path} stopped being unflagged");
        if KEYCAP_NEGATIVE_PATHS.contains(&path.as_str()) {
            negative_count += 1;
            for marker in ["negative:", "phase: parse", "type: SyntaxError"] {
                assert!(source.contains(marker), "{path} lost {marker}");
            }
        } else {
            assert!(
                !source.contains("negative:"),
                "{path} became a negative test"
            );
        }
        for mask in [TEST262_RUNNER_SOURCE, SHORTCUT_ALLOWLIST, KNOWN_FAILURES] {
            assert!(!mask.contains(path), "{path} gained an exact mask");
        }
    }
    assert_eq!(negative_count, 3);
    assert_eq!(keycaps.len() - negative_count, 34);
}

#[test]
fn durable_fixture_exercises_complete_string_members_and_set_algebra() {
    for marker in [
        "longest class string first",
        "shorter class string restores capture",
        "multi-character member is indivisible",
        "first class-string capture",
        "string-left union",
        "string-right union",
        "direct keycap property",
        "keycap property identity case folding",
        "keycap property is indivisible",
        "keycap property union",
        "keycap property intersection",
        "keycap property intersection removes other members",
        "keycap property subtraction",
        "keycap property subtraction removes member",
        "string intersection",
        "intersection removes multi-code-point member",
        "string subtraction",
        "string-left subtraction retains keycap",
        "string-right subtraction removes scalar",
        "empty class string",
        "global empty progress",
        "lookbehind class-string capture",
    ] {
        assert!(FIXTURE.contains(marker), "fixture lost {marker}");
    }
    assert!(CLI_TEST_SOURCE.contains("fn run_wasm_backend_matches_unicode_sets_class_strings()"));
    assert!(CLI_TEST_SOURCE.contains("wasm_regexp_unicode_sets_class_strings.js"));
}
