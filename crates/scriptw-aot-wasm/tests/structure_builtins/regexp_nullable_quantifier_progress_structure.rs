const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/regexp.rs");
const IR_PUBLIC_SOURCE: &str = include_str!("../../../scriptw-ir/src/lib.rs");
const MATCHER_SOURCE: &str = include_str!("../../src/builtins/regexp.rs");
const BACKTRACK_SOURCE: &str = include_str!("../../src/builtins/regexp/backtracking.rs");
const DATA_SOURCE: &str = include_str!("../../src/data.rs");
const PROGRAM_SOURCE: &str = include_str!("../../../scriptw-ir/src/regexp/program.rs");
const OPCODE_SOURCE: &str = include_str!("../../../scriptw-ir/src/regexp/opcode.rs");
const FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_regexp_nullable_quantifier_progress.js");
const CLI_TEST_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/regexp.rs");
const TEST262_RUNNER_SOURCE: &str = include_str!("../../../scriptw-test262/src/lib.rs");
const KNOWN_FAILURES: &str = include_str!("../../../scriptw-cli/tests/known-failures.tsv");
const EXACT_TEST262: &str =
    include_str!("../../../../test262/vendor/test262/test/built-ins/RegExp/nullable-quantifier.js");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/regexp-nullable-quantifier-progress.md");
const COUNTED_PROGRAM_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/regexp/program/counted.rs");
const LOWERER_SOURCE: &str = include_str!("../../src/builtins/regexp/compiler/lowerer.rs");
const LOWER_COUNTED_SOURCE: &str =
    include_str!("../../src/builtins/regexp/compiler/lowerer/counted.rs");
const LOWER_WIDTH_SOURCE: &str =
    include_str!("../../src/builtins/regexp/compiler/lowerer/width.rs");
const LOWER_GRAPH_SOURCE: &str =
    include_str!("../../src/builtins/regexp/compiler/lowerer/graph.rs");
const COUNTED_MATCHER_SOURCE: &str = include_str!("../../src/builtins/regexp/counted.rs");
const WORKSPACE_SOURCE: &str = include_str!("../../src/builtins/regexp/matcher_workspace.rs");
const CHOICE_SOURCE: &str =
    include_str!("../../src/builtins/regexp/matcher_workspace/choice_entries.rs");
const LOGICAL_RUN_SOURCE: &str = include_str!(
    "../../src/builtins/regexp/matcher_workspace/choice_entries/required_run/logical.rs"
);
const RUN_SOURCE: &str =
    include_str!("../../src/builtins/regexp/matcher_workspace/choice_entries/required_run.rs");
const RUN_EXHAUSTION_SOURCE: &str = include_str!(
    "../../src/builtins/regexp/matcher_workspace/choice_entries/required_run/exhaustion.rs"
);
const RUN_TREE_SOURCE: &str =
    include_str!("../../src/builtins/regexp/matcher_workspace/choice_entries/required_run/tree.rs");
const RUN_PATH_SOURCE: &str =
    include_str!("../../src/builtins/regexp/matcher_workspace/choice_entries/required_run/path.rs");
const TRANSIENT_SOURCE: &str = include_str!("../../src/builtins/regexp/transient.rs");
const EXEC_SOURCE: &str = include_str!("../../src/builtins/string/regexp_exec.rs");
const HELPER_SOURCE: &str = include_str!("../../src/runtime_helpers.rs");

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end after {start}: {end}"))
        .0
}

fn quoted_literal_end(source: &str, quote_start: usize, quote: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut offset = quote_start + 1;
    let mut escaped = false;
    while offset < bytes.len() {
        let byte = bytes[offset];
        if escaped {
            escaped = false;
        } else if byte == b'\\' {
            escaped = true;
        } else if byte == quote {
            return Some(offset + 1);
        }
        offset += 1;
    }
    None
}

fn character_literal_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let value_start = start + 1;
    if value_start >= bytes.len() {
        return None;
    }
    let value_end = if bytes[value_start] == b'\\' {
        let mut offset = value_start + 1;
        if offset >= bytes.len() {
            return None;
        }
        if bytes[offset] == b'u' && bytes.get(offset + 1) == Some(&b'{') {
            offset += 2;
            while bytes.get(offset).is_some_and(|byte| *byte != b'}') {
                offset += 1;
            }
            if bytes.get(offset) != Some(&b'}') {
                return None;
            }
            offset + 1
        } else if bytes[offset] == b'x'
            && bytes
                .get(offset + 1..offset + 3)
                .is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit))
        {
            offset + 3
        } else {
            offset + 1
        }
    } else {
        value_start + source[value_start..].chars().next()?.len_utf8()
    };
    (bytes.get(value_end) == Some(&b'\'')).then_some(value_end + 1)
}

fn raw_literal_end(source: &str, start: usize, prefix_len: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut quote_start = start + prefix_len;
    while bytes.get(quote_start) == Some(&b'#') {
        quote_start += 1;
    }
    if bytes.get(quote_start) != Some(&b'"') {
        return None;
    }
    let hashes = quote_start - start - prefix_len;
    let mut offset = quote_start + 1;
    while offset < bytes.len() {
        if bytes[offset] == b'"'
            && bytes
                .get(offset + 1..offset + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            return Some(offset + 1 + hashes);
        }
        offset += 1;
    }
    None
}

fn literal_end(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    match bytes.get(start).copied()? {
        b'"' => quoted_literal_end(source, start, b'"'),
        b'\'' => character_literal_end(source, start),
        b'b' if bytes.get(start + 1) == Some(&b'\'') => character_literal_end(source, start + 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'"') => {
            quoted_literal_end(source, start + 1, b'"')
        }
        b'r' => raw_literal_end(source, start, 1),
        b'b' | b'c' if bytes.get(start + 1) == Some(&b'r') => raw_literal_end(source, start, 2),
        _ => None,
    }
}

struct NormalizedRust {
    code: String,
    identifiers: String,
    routes: String,
}

fn normalize_rust(source: &str) -> NormalizedRust {
    let bytes = source.as_bytes();
    let mut code = String::new();
    let mut identifiers = String::new();
    let mut routes = String::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some(end) = literal_end(source, offset) {
            code.push_str(&source[offset..end]);
            identifiers.push(' ');
            routes.push('L');
            offset = end;
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"//") {
            identifiers.push(' ');
            offset += 2;
            while bytes.get(offset).is_some_and(|byte| *byte != b'\n') {
                offset += 1;
            }
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"/*") {
            identifiers.push(' ');
            offset += 2;
            let mut depth = 1;
            while offset < bytes.len() && depth != 0 {
                if bytes.get(offset..offset + 2) == Some(b"/*") {
                    depth += 1;
                    offset += 2;
                } else if bytes.get(offset..offset + 2) == Some(b"*/") {
                    depth -= 1;
                    offset += 2;
                } else {
                    offset += 1;
                }
            }
            assert_eq!(depth, 0, "unterminated block comment in Rust source");
            continue;
        }
        if bytes.get(offset..offset + 2) == Some(b"r#")
            && source[offset + 2..]
                .chars()
                .next()
                .is_some_and(|character| character == '_' || character.is_alphabetic())
        {
            offset += 2;
            continue;
        }
        let character = source[offset..].chars().next().unwrap();
        if !character.is_whitespace() {
            code.push(character);
            routes.push(character);
        }
        if character.is_whitespace() {
            identifiers.push(' ');
        } else {
            identifiers.push(character);
        }
        offset += character.len_utf8();
    }
    NormalizedRust {
        code,
        identifiers,
        routes,
    }
}

fn exact_identifier_count(source: &str, identifier: &str) -> usize {
    source
        .match_indices(identifier)
        .filter(|(offset, _)| {
            let before = source[..*offset].chars().next_back();
            let after = source[*offset + identifier.len()..].chars().next();
            [before, after].into_iter().all(|edge| {
                edge.map(|character| !character.is_alphanumeric() && character != '_')
                    .unwrap_or(true)
            })
        })
        .count()
}

fn exact_route_count(source: &str, route: &str) -> usize {
    exact_identifier_count(source, route)
}

fn positions_in_order(source: &str, markers: &[&str]) {
    let source = source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let mut cursor = 0;
    for marker in markers {
        let marker = marker
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        let offset = source[cursor..]
            .find(&marker)
            .unwrap_or_else(|| panic!("missing marker after byte {cursor}: {marker}"));
        cursor += offset + marker.len();
    }
}

#[test]
fn quantifier_lowering_owns_a_closed_optional_progress_lifecycle() {
    use scriptw_ir::{RegExpOpcode, RegExpProgram, RegExpRepeatMaximum, ValidatedRegExpProgram};
    for pattern in [
        "()?",
        "()??",
        "()*",
        "()*?",
        "()+",
        "()+?",
        "(){2,3}",
        "(){2,18446744073709551615}",
        "(){2,18446744073709551616}",
        "(){0002,9999999999999999999999999999999999999}",
        "(){2,}",
        "(?<=(){2,18446744073709551616})a",
    ] {
        let program = RegExpProgram::compile(pattern, "d").expect(pattern);
        let descriptor = ValidatedRegExpProgram::from_program(&program).expect(pattern);
        assert_eq!(
            ValidatedRegExpProgram::from_bytes(descriptor.bytes().to_vec()).unwrap(),
            descriptor
        );
        for (pc, instruction) in program.instructions.iter().enumerate() {
            if RegExpOpcode::from_word(instruction.opcode) == Some(RegExpOpcode::ProgressCheck) {
                let mut damaged = program.clone();
                damaged.instructions[pc].operand0 = u64::MAX;
                assert!(
                    ValidatedRegExpProgram::from_program(&damaged).is_err(),
                    "unowned optional progress: {pattern}"
                );
            }
        }
    }
    let small = RegExpProgram::compile("(){2,3}", "").unwrap();
    for maximum in [
        "18446744073709551615",
        "18446744073709551616",
        "9999999999999999999999999999999999999",
    ] {
        let program = RegExpProgram::compile(&format!("(){{2,{maximum}}}"), "").unwrap();
        assert_eq!(
            program.instructions, small.instructions,
            "the bound cannot copy its atom"
        );
        assert_eq!(program.repeat_bounds.len(), 1);
        assert_eq!(program.repeat_bounds[0].minimum().digits(), b"2");
        let RegExpRepeatMaximum::Finite(bound) = program.repeat_bounds[0].maximum() else {
            panic!("finite remains finite")
        };
        assert_eq!(bound.digits(), maximum.as_bytes());
    }
    let unbounded = RegExpProgram::compile("(){2,}", "").unwrap();
    assert_eq!(
        unbounded.repeat_bounds[0].maximum(),
        &RegExpRepeatMaximum::Unbounded
    );
}

#[test]
fn pending_types_force_split_check_and_fallback_registration_order() {
    assert!(IR_SOURCE.contains(
        "#[must_use = \"a nullable optional quantifier attempt must emit its paired progress check\"]\nstruct PendingNullableQuantifierProgress"
    ));
    assert!(IR_SOURCE.contains(
        "#[must_use = \"a completed nullable quantifier attempt must receive its quantifier fallback\"]\nstruct PendingNullableQuantifierFallback"
    ));
    let pending_types = bounded(
        IR_SOURCE,
        "struct PendingNullableQuantifierProgress {",
        "struct ProgramLowerer<'a> {",
    );
    assert!(!pending_types.contains("#[derive"));
    assert!(!pending_types.contains("impl Clone"));
    assert!(!pending_types.contains("impl Copy"));

    let lifecycle = bounded(
        IR_SOURCE,
        "    fn begin_nullable_optional(\n",
        "    fn atom(&mut self, atom: &ParsedAtom)",
    );
    positions_in_order(
        lifecycle,
        &[
            "RegExpInstruction::progress_split(0, 0, preference)",
            "PendingNullableQuantifierProgress {",
            "fn complete_nullable_optional(",
            "pending: PendingNullableQuantifierProgress",
            "RegExpInstruction::progress_check(",
            "PendingNullableQuantifierFallback {",
            "fn finish_nullable_optional(",
            "pending: PendingNullableQuantifierFallback",
            "RegExpInstruction::progress_split(pending.attempt_pc, fallback_pc, pending.preference)",
        ],
    );

    let builders = bounded(
        IR_SOURCE,
        "    fn progress_split(\n",
        "    pub const fn jump(target_pc: usize)",
    );
    for marker in [
        "opcode: REGEXP_OPCODE_PROGRESS_SPLIT",
        "operand0: attempt_pc as u64",
        "operand1: ((fallback_pc as u64) << 1) | preference.word()",
        "opcode: REGEXP_OPCODE_PROGRESS_CHECK",
        "operand0: progress_split_pc as u64",
        "operand1: continuation_pc as u64",
    ] {
        assert!(
            builders.contains(marker),
            "fixed-width builder lost {marker}"
        );
    }
    for opcode in [
        "REGEXP_OPCODE_PROGRESS_SPLIT",
        "REGEXP_OPCODE_PROGRESS_CHECK",
    ] {
        assert!(IR_PUBLIC_SOURCE.contains(opcode));
    }
}

#[test]
fn matcher_frames_preserve_ordered_backtracking_and_exact_progress_identity() {
    let variants = bounded(
        CHOICE_SOURCE,
        "enum ChoiceEntryKind {",
        "impl ChoiceEntryKind {",
    )
    .lines()
    .map(str::trim)
    .filter(|line| !line.is_empty() && *line != "}")
    .collect::<Vec<_>>();
    assert_eq!(
        variants,
        [
            "Ordinary,",
            "GreedyProgress,",
            "LazyProgressChoice,",
            "LazyProgressAttempt,",
            "RequiredRun,"
        ]
    );
    let words = bounded(
        CHOICE_SOURCE,
        "impl ChoiceEntryKind {",
        "enum SnapshotChoice",
    );
    for variant in variants.iter().map(|variant| variant.trim_end_matches(',')) {
        assert!(words.contains(&format!("Self::{variant} =>")));
    }
    assert!(!words.contains("_ =>"));
    let compact = |source: &str| source.split_whitespace().collect::<String>();
    let dispatch = compact(bounded(
        MATCHER_SOURCE,
        "// `Split` records the fallback before taking the primary arm.",
        "        reverse_mode.load(&mut function);",
    ));
    positions_in_order(
        &dispatch,
        &[
            "SnapshotChoice::Ordinary",
            "fallback:operand1",
            "origin:pc",
            "REGEXP_OPCODE_PROGRESS_SPLITasi64",
            "I64Const(0x3fff_ffff)",
            "choice_lazy.store(&mutfunction)",
            "choice_header.store(&mutfunction)",
            "SnapshotChoice::Progress",
            "fallback:choice_header",
            "origin:pc",
            "lazy:choice_lazy",
            "REGEXP_OPCODE_PROGRESS_CHECKasi64",
            "find_progress(self,operand0",
            "entry.load_utf16(function)",
            "match_utf16.load(function)",
            "I64Eq",
            "progress_no_advance.store(function)",
            "progress_no_advance.load(&mutfunction)",
            "Instruction::I32WrapI64",
            "self.emit_regexp_backtrack_request_if(backtrack.request(),&mutfunction)",
            "operand1.load(&mutfunction)",
            "pc.store(&mutfunction)",
        ],
    );
    assert_eq!(
        dispatch
            .matches("workspace.choices().push_snapshot(")
            .count(),
        2
    );
    let push = compact(bounded(
        CHOICE_SOURCE,
        "fn push_snapshot(",
        "fn top_cursor(",
    ));
    positions_in_order(
        &push,
        &[
            "ensure_choice_bytes(builder,required,function)?",
            "Self::store(address,0,",
            "Self::store(address,8,",
            "SnapshotChoice::Ordinary",
            "SnapshotChoice::Progress",
            "LazyProgressChoice.word()",
            "GreedyProgress.word()",
            "Self::store(address,24,fallback,function)",
            "I64Store(FunctionBuilder::memarg8(32))",
            "cursor.byte",
            "cursor.utf16",
            "cursor.on_low_surrogate",
            "Instruction::MemoryCopy",
            "choice_top.store(function)",
            "choice_used.store(function)",
        ],
    );
    let search = compact(bounded(CHOICE_SOURCE, "fn find_progress(", "    fn find("));
    assert!(search.contains("ChoiceSearchPredicate::Progress(origin)"));
    let predicate = compact(bounded(
        CHOICE_SOURCE,
        "impl ChoiceSearchPredicate {",
        "impl<'workspace> ChoiceStack",
    ));
    positions_in_order(
        &predicate,
        &[
            "GreedyProgress",
            "LazyProgressAttempt",
            "ChoiceStack::load(address,32,function)",
            "origin.load(function)",
            "I64Eq",
            "I32And",
        ],
    );
    let logical_search = compact(bounded(CHOICE_SOURCE, "    fn find(", "    fn address("));
    positions_in_order(
        &logical_search,
        &[
            "self.check_entry",
            "ChoiceEntryKind::RequiredRun",
            "entry.find_required_run",
            "predicate.emit(address,None,function)",
            "entry.with_snapshot",
            "found.load(function)",
            "Self::load(address,8,function)",
        ],
    );
    let run_search = compact(bounded(
        LOGICAL_RUN_SOURCE,
        "fn find_required_run(",
        "fn restore_required_run(",
    ));
    positions_in_order(
        &run_search,
        &[
            "PathWord::Index,index",
            "node.selected_kind",
            "predicate.emit(template,Some(kind),f)",
            "LogicalChoiceEntry::Run",
            "node.has_older_group",
            "I64Const(1));older.store(f)",
            "node.count.load(f)",
        ],
    );
    let run_pop = compact(
        LOGICAL_RUN_SOURCE
            .split_once("fn restore_required_run(")
            .unwrap()
            .1,
    );
    positions_in_order(
        &run_pop,
        &[
            "LazyProgressAttempt.word()",
            "node.consume(&root,RunEntryConsumption::FailedBacktrack",
            "logical.restore_cursor",
            "logical.restore_state",
            "LazyProgressChoice.word()",
            "node.current_kind_override.store(f)",
            "node.consume(&root,RunEntryConsumption::FailedBacktrack",
            "I32Const(1)",
            "selected.store(f)",
        ],
    );
    let request = compact(bounded(
        BACKTRACK_SOURCE,
        "fn emit_regexp_backtrack_request_if(",
        "pub(super) fn emit_regexp_backtrack_recovery(",
    ));
    assert!(request.contains("request:&BacktrackRequest"));
    assert!(request.contains("emit_branch_if_to_target(request.target,f)"));
    assert!(
        !request.contains("restore_"),
        "mismatch sites only request recovery"
    );
    assert!(compact(COUNTED_MATCHER_SOURCE).contains("backtrack:&BacktrackRequest"));
    assert!(!BACKTRACK_SOURCE.contains("caller_block_depth"));
    assert!(!BACKTRACK_SOURCE.contains("Instruction::Br("));
    assert!(!BACKTRACK_SOURCE.contains("Instruction::BrIf("));
    let matcher = compact(MATCHER_SOURCE);
    positions_in_order(
        &matcher,
        &[
            "letbacktrack=BacktrackRequestScope::open(self,&mutfunction)",
            "letrecovery=backtrack.close(self,&mutfunction)",
            "self.emit_regexp_backtrack_recovery(",
            "byte:match_byte",
            "utf16:match_utf16",
            "on_low_surrogate:match_on_low_surrogate",
            "pc,recovery,&mutfunction",
        ],
    );
    assert_eq!(
        matcher
            .matches("self.emit_regexp_backtrack_recovery(")
            .count(),
        1
    );
    let close = compact(bounded(
        BACKTRACK_SOURCE,
        "pub(super) fn close(",
        "impl FunctionBuilder<'_>",
    ));
    positions_in_order(
        &close,
        &[
            "self,",
            "->BacktrackRecoveryScope",
            "builder.emit_branch_to_target(self.exhausted,f)",
            "builder.pop_control(ControlFrameKind::Block)",
            "Instruction::End",
            "BacktrackRecoveryScope{",
            "retry:self.retry",
            "exhausted:self.exhausted",
        ],
    );
    let backtrack = compact(
        BACKTRACK_SOURCE
            .split_once("fn emit_regexp_backtrack_recovery(")
            .unwrap()
            .1,
    );
    positions_in_order(
        &backtrack,
        &[
            "scope:BacktrackRecoveryScope",
            "letchoose=self.open_frame(ControlFrameKind::Loop,f)",
            "workspace.choices().is_empty(f)",
            "self.emit_branch_if_to_target(scope.exhausted,f)",
            "with_top(self,f",
            "ChoiceEntryKind::RequiredRun",
            "restore_required_run(builder,pc,cursor,selected,f)",
            "selected.load(f)",
            "Instruction::I32Eqz",
            "builder.emit_branch_if_to_target(choose,f)",
            "ChoiceEntryKind::LazyProgressAttempt",
            "entry.discard_through(f)",
            "builder.emit_branch_to_target(choose,f)",
            "ChoiceEntryKind::LazyProgressChoice",
            "activate_lazy_attempt",
            "entry.restore_fallback(pc,f)",
            "entry.restore_cursor(cursor,f)",
            "entry.restore_state(f)",
            "builder.emit_branch_to_target(scope.retry,f)",
            "Instruction::Unreachable",
        ],
    );
}

#[test]
fn required_run_failed_groups_need_owned_counter_observation_and_actual_exhaustion() {
    let compact = |source: &str| source.split_whitespace().collect::<String>();
    assert!(WORKSPACE_SOURCE.contains("active_run_offset: I64Local"));
    assert!(WORKSPACE_SOURCE.contains("active_run_row: I64Local"));
    assert!(WORKSPACE_SOURCE.contains("active_run_node: I64Local"));
    let observer = compact(bounded(
        RUN_EXHAUSTION_SOURCE,
        "fn observe_repeat_end(",
        "impl ChoiceEntry",
    ));
    positions_in_order(
        &observer,
        &[
            "self.visit_active_ancestry",
            "self.workspace.base.load(f)",
            "ChoiceStack::load(saved,40,f)",
            "state.load(f)",
            "I64Eq",
            "[(104,1),(112,0)]",
        ],
    );
    let end = compact(bounded(
        COUNTED_MATCHER_SOURCE,
        "CountedOperation::End => {",
        "let accelerated",
    ));
    positions_in_order(
        &end,
        &[
            "workspace.choices().observe_repeat_end(self,p.state,f)",
            "RegExpRepeatStateWord::Active",
            "RegExpRepeatStateWord::Stage",
            "RegExpRepeatStateWord::PreUtf16",
        ],
    );
    let append = compact(bounded(RUN_SOURCE, "fn append_required_run(", "fn reject("));
    positions_in_order(
        &append,
        &[
            "self.invalidate_active_run(builder,f)",
            "layout.observed_group,layout.pending_complete",
            "layout.derive_addresses",
            "Instruction::MemoryCopy",
        ],
    );
    let assertion = compact(bounded(
        LOGICAL_RUN_SOURCE,
        "fn discard_through(",
        "impl ChoiceEntry",
    ));
    positions_in_order(
        &assertion,
        &[
            "invalidate_active_run",
            "I64Const(1)",
            "node.observed_group.store(f)",
            "I64Const(0)",
            "node.pending_complete.store(f)",
            "RunEntryConsumption::AssertionTruncate",
        ],
    );
    let consume = compact(bounded(
        LOGICAL_RUN_SOURCE,
        "fn consume(",
        "fn restore_affine_row(",
    ));
    positions_in_order(
        &consume,
        &[
            "self.index.load(f)",
            "RunEntryConsumption::FailedBacktrack",
            "self.observed_group.load(f)",
            "I64Eqz",
            "self.pending_complete.store(f)",
            "self.observed_group.store(f)",
            "self.advance_group",
        ],
    );
    let select = compact(
        RUN_EXHAUSTION_SOURCE
            .split_once("fn begin_backtracked_node(")
            .unwrap()
            .1,
    );
    positions_in_order(
        &select,
        &[
            "workspace.active_run_offset.load(f)",
            "self.offset.load(f)",
            "I64Eq",
            "layout.pending_complete.load(f)",
            "layout.observed_group.load(f)",
            "I64Eqz",
            "self.stack.invalidate_active_run",
            "workspace.active_run_row.store(f)",
        ],
    );
    let pop = compact(
        LOGICAL_RUN_SOURCE
            .split_once("fn restore_required_run(")
            .unwrap()
            .1,
    );
    positions_in_order(
        &pop,
        &[
            "self.begin_backtracked_node",
            "selected.load(f)",
            "node.remaining_used.store(f)",
            "node.publish_used(f)",
            "I32Const(0)",
            "selected.store(f)",
            "self.discard_through(f)",
            "node.selected_kind",
            "logical.restore_state",
        ],
    );
    let discard = compact(bounded(
        CHOICE_SOURCE,
        "fn discard_through(&self, function:",
        "fn restore_fallback(",
    ));
    positions_in_order(
        &discard,
        &[
            "clear_removed_active_run(self.offset,function)",
            "self.load(8,function)",
            "choice_top.store(function)",
            "choice_used.store(function)",
        ],
    );
}

#[test]
fn recursive_required_runs_keep_current_and_original_reset_domains_separate() {
    let compact = |source: &str| source.split_whitespace().collect::<String>();
    let append = compact(bounded(RUN_SOURCE, "fn append_required_run(", "fn reject("));
    positions_in_order(
        &append,
        &[
            "proof.template_bytes()",
            "layout.measure_template",
            "ensure_choice_bytes",
            "layout.template_bytes.load(f)",
            "Instruction::MemoryCopy",
            "layout.initialize_tree",
        ],
    );
    let initialize = compact(bounded(
        RUN_TREE_SOURCE,
        "fn initialize_tree(",
        "fn reset_children(",
    ));
    positions_in_order(
        &initialize,
        &[
            "copy_bytes(state,cursor,child.prefix_bytes,f)",
            "taint(state,f)",
            "copy_bytes(output,child.table,child.table_bytes,f)",
            "foroffsetin[8,16]",
            "relative.load(f)",
            "taint(state,f)",
        ],
    );
    let reset = compact(bounded(
        RUN_TREE_SOURCE,
        "fn reset_children(",
        "impl ChoiceEntry",
    ));
    positions_in_order(
        &reset,
        &[
            "root.state_for",
            "copy_bytes(state,child_address,child.prefix_bytes,f)",
            "taint(state,f)",
            "child.table.load(f)",
            "copy_bytes(state,source,target_relative,f)",
            "taint(state,f)",
        ],
    );
    let search = compact(bounded(
        LOGICAL_RUN_SOURCE,
        "fn find_required_run(",
        "fn restore_required_run(",
    ));
    positions_in_order(
        &search,
        &[
            "PathWord::Context,context",
            "older.load(f)",
            "definition.load(f);context.store(f)",
            "RunLayout::read(context",
            "baseline.state_for",
            "path.initialize",
            "predicate.emit",
        ],
    );
    let patch = compact(bounded(
        LOGICAL_RUN_SOURCE,
        "fn patch_ancestors(",
        "fn restore_repeats(",
    ));
    positions_in_order(
        &patch,
        &[
            "self.depth.load(f)",
            "PathWord::Definition",
            "PathWord::State",
            "PathWord::Older",
            "node.restore_affine_row",
            "I64Sub",
            "level.store(f)",
        ],
    );
    assert!(RUN_PATH_SOURCE.contains("enum PathWord"));
    assert!(compact(RUN_TREE_SOURCE).contains("fnwith_template_snapshots(&self"));
    assert!(compact(RUN_TREE_SOURCE).contains("fncompare_template_tree(&self,other:&ChoiceEntry"));
    assert!(RUN_TREE_SOURCE.contains("fn check_distinct_ancestry"));
    assert!(compact(RUN_EXHAUSTION_SOURCE).contains("workspace.active_run_node.load(f)"));
}

#[test]
fn static_data_validation_counts_progress_choices_and_terminates_checks() {
    assert!(DATA_SOURCE.contains("ValidatedRegExpProgram::from_program(program)"));
    assert!(PROGRAM_SOURCE.contains("is_some_and(RegExpOpcode::is_choice)"));
    assert!(PROGRAM_SOURCE.contains("repeatable_split_count(program)"));

    let repeatable = bounded(
        PROGRAM_SOURCE,
        "fn repeatable_split_count(program: &RegExpProgram) -> u32 {",
        "fn has_non_consuming_cycle(program: &RegExpProgram) -> bool {",
    );
    for marker in [
        "RegExpOpcode::from_word(instructions[pc].opcode)",
        ".successors(instructions[pc], pc, instructions.len())",
        "visited[next] = true;",
        "stack.push(next);",
    ] {
        assert!(repeatable.contains(marker), "accounting lost {marker}");
    }

    let cycle = bounded(
        PROGRAM_SOURCE,
        "fn has_non_consuming_cycle(program: &RegExpProgram) -> bool {",
        "#[cfg(test)]\nmod tests {",
    );
    for marker in [
        "while let Some((pc, edge)) = stack.last_mut()",
        "opcode.stops_non_consuming_walk(instruction.operand1)",
        "let successors = if opcode == RegExpOpcode::RepeatEnd {",
        "[Some(guard.operand0 as usize + 1), None]",
        "opcode.successors(instruction, *pc, instructions.len())",
    ] {
        assert!(cycle.contains(marker), "cycle validation lost {marker}");
    }
    let facts = normalize_rust(OPCODE_SOURCE);
    for marker in [
        "Self::ProgressCheck=>Flow::Operand1",
        "Self::ProgressSplit=>Flow::ProgressSplit",
        "RegExpControlFlow::ProgressSplit=>[valid(a),valid(b>>1)]",
        "RegExpInputProgress::Consumes|RegExpInputProgress::CheckedOptional=>true",
    ] {
        assert!(
            facts.code.contains(marker),
            "shared opcode facts lost {marker}"
        );
    }
}

#[test]
fn exact_inventory_fixture_and_progress_contract_remain_bounded() {
    for marker in [
        "esid: sec-runtime-semantics-repeatmatcher-abstract-operation",
        "let regex = /(a?b??)*/;",
        "assert.sameValue(match[0], expected",
    ] {
        assert!(EXACT_TEST262.contains(marker));
    }
    assert!(!EXACT_TEST262.contains("flags:"));
    assert!(!TEST262_RUNNER_SOURCE.contains("built-ins/RegExp/nullable-quantifier.js"));
    assert!(!KNOWN_FAILURES.contains("nullable-quantifier.js"));

    for marker in [
        "exec(/(a?b??)*/, \"ab\", \"ab\", [\"b\"]",
        "exec(/(a?b??)*b/, \"ab\", \"ab\", [\"a\"]",
        "exec(/(a?)*a/, \"aa\", \"aa\"",
        "exec(/(a?)*?a/, \"aa\", \"a\"",
        "exec(/(a?){2,}b/, \"b\", \"b\"",
        "exec(/^(a?){2,4}b$/",
        "exec(/((a?)*)*b/",
        "exec(/(?<=(a?)*)b/",
        "match(/(a?)*/g)",
        "match(/(a?)*?/g)",
    ] {
        assert!(FIXTURE.contains(marker), "fixture lost {marker}");
    }
    assert!(CLI_TEST_SOURCE
        .contains("fn run_wasm_backend_rejects_empty_optional_nullable_quantifier_iterations()"));
    assert!(CLI_TEST_SOURCE.contains("wasm_regexp_nullable_quantifier_progress.js"));

    for marker in [
        "REGEXP_OPCODE_PROGRESS_SPLIT",
        "REGEXP_OPCODE_PROGRESS_CHECK",
        "PendingNullableQuantifierProgress",
        "Forward and reverse matching use the same paired representation",
        "does not claim all RegExp Test262 coverage",
        "--snapshot-name regexp-nullable-quantifier",
    ] {
        assert!(CONTRACT.contains(marker), "contract lost {marker}");
    }
}

#[test]
fn static_and_emitted_counted_bodies_consume_one_paired_owner() {
    use scriptw_front::{parse, ParseOptions};
    use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
    use wasmparser::{Validator, WasmFeatures};
    // The character loop prevents the constructor's pattern from becoming a
    // static literal. Both actual producers must reach the native module path.
    let parsed = parse(
        r#"
        var source = "";
        var units = [40, 41, 123, 50, 44, 49, 56, 52, 52, 54, 55, 52, 52, 48,
            55, 51, 55, 48, 57, 53, 53, 49, 54, 49, 54, 125];
        for (var i = 0; i < units.length; i++) source += String.fromCharCode(units[i]);
        var dynamic = new RegExp(source, "d");
        var literal = /(){2,18446744073709551616}/d;
        dynamic.exec("");
        literal.exec("");
    "#,
        ParseOptions::script(),
    )
    .unwrap();
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    let artifact = scriptw_aot_wasm::emit(&program)
        .expect("both descriptor producers reach real Wasm codegen");
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(&artifact.bytes)
        .expect("v3 counted descriptor, compiler and matcher emit valid Wasm");
}

#[test]
fn counted_matcher_preserves_optional_fallback_and_required_empty_iterations() {
    use scriptw_front::{parse, ParseOptions};
    use scriptw_ir::{lower_with_host_surface_policy, HostSurfacePolicy};
    use wasmparser::{Validator, WasmFeatures};
    // Keep the retained greedy/lazy, captures, nested choices, assertions and
    // reverse cohort on the actual parser -> IR -> native matcher codegen path.
    let parsed = parse(FIXTURE, ParseOptions::script()).unwrap();
    let program = lower_with_host_surface_policy(&parsed, HostSurfacePolicy::Test262);
    let artifact =
        scriptw_aot_wasm::emit(&program).expect("retained nullable cohort reaches native codegen");
    Validator::new_with_features(WasmFeatures::all())
        .validate_all(&artifact.bytes)
        .expect("optional fallback and exact required counters have balanced native control flow");
}

#[test]
fn one_bounded_tail_workspace_owns_live_state_and_demand_growth() {
    assert!(
        HELPER_SOURCE.contains("const REGEXP_MATCHER_SCRATCH_MAX_BYTES: i64 = 512 * 1024 * 1024;")
    );
    positions_in_order(
        MATCHER_SOURCE,
        &[
            "All immutable program/text transients are complete",
            "let workspace = MatcherWorkspace::allocate(",
            "workspace.reset_repeats(&mut function);",
        ],
    );
    let allocate = bounded(
        WORKSPACE_SOURCE,
        "    pub(super) fn allocate(
",
        "    pub(super) fn fail(
",
    );
    positions_in_order(
        allocate,
        &[
            "workspace.capture_bytes.store(function)",
            "workspace.repeat_bytes.store(function)",
            "workspace.live_bytes.store(function)",
            "workspace.snapshot_bytes.store(function)",
            "REGEXP_MATCHER_SCRATCH_MAX_BYTES",
            "workspace.maximum_capacity.store(function)",
            "workspace.capacity.store(function)",
            "builder.emit_regexp_transient_allocation(",
            "workspace.choices().reset(function)",
        ],
    );
    let growth = bounded(
        WORKSPACE_SOURCE,
        "    fn ensure_choice_bytes(
",
        "    pub(super) fn choices(",
    );
    positions_in_order(
        growth,
        &[
            "Instruction::GlobalGet(PRIVATE_BYTE_CURSOR_GLOBAL_INDEX)",
            "self.allocated_bytes.load(function)",
            "RegExpMatcherFailure::CorruptProgram",
            "self.maximum_capacity.load(function)",
            "RegExpMatcherFailure::ResourceExhausted",
            "self.capacity.load(function)",
            "Instruction::I64Const(2)",
            "builder.emit_regexp_transient_allocation(",
            "extension.load(function)",
            "RegExpMatcherFailure::CorruptProgram",
            "self.allocated_bytes.store(function)",
        ],
    );
    assert!(!growth.contains("input_len"));
    assert!(!growth.contains("minimum"));
    assert!(!growth.contains("maximum.load"));
    let compact = |source: &str| source.split_whitespace().collect::<String>();
    let restore = compact(bounded(
        CHOICE_SOURCE,
        "fn restore_slice(",
        "fn restore_state(",
    ));
    assert!(restore.contains("Instruction::MemoryCopy"));
    let full_restore = compact(bounded(
        CHOICE_SOURCE,
        "fn restore_state(",
        "fn restore_captures(",
    ));
    assert!(
        full_restore.contains("self.restore_slice(None,self.stack.workspace.live_bytes,function)")
    );
    let repeat_restore = compact(bounded(
        CHOICE_SOURCE,
        "fn restore_repeats(",
        "impl ChoiceSnapshot",
    ));
    assert!(repeat_restore.contains("self.stack.workspace.repeat_bytes"));
    assert!(repeat_restore.contains("self.stack.workspace.capture_bytes"));
    let caller = bounded(
        EXEC_SOURCE,
        "    fn emit_native_regexp_scratch(\n",
        "    fn emit_native_regexp_builtin_exec(\n",
    );
    assert!(caller.contains("layout.capture_count().load(f)"));
    assert!(caller
        .contains("self.emit_regexp_capture_output_allocation(size, base, result, exit, f)?;"));
    assert!(!caller.contains("repeat_slot_count"));
    assert!(!caller.contains("input_length"));
    assert!(!caller.contains("split_count"));
    let allocator = bounded(
        TRANSIENT_SOURCE,
        "    pub(super) fn emit_regexp_transient_allocation(\n",
        "    pub(in crate::builtins) fn emit_regexp_capture_output_allocation(\n",
    );
    positions_in_order(
        allocator,
        &[
            "u32::MAX",
            "failure.emit(self, function)?",
            "Instruction::MemoryGrow(0)",
            "failure.emit(self, function)?",
            "TransientByteAllocArguments::new(size)",
        ],
    );
    let output_failure = bounded(
        TRANSIENT_SOURCE,
        "            Self::CaptureOutput { completion, exit } => {",
        "        Ok(())",
    );
    positions_in_order(
        output_failure,
        &[
            "NativeErrorKind::RangeError",
            "completion",
            "builder.emit_branch_to_target(*exit, function)",
        ],
    );
}
