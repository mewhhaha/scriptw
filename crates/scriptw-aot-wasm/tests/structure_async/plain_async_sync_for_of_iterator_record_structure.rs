use std::fs;
use std::path::Path;

const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const ASYNC_FOR_OF_PLAN_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/ir/async_for_of_iterator.rs");
const ASYNC_FOR_OF_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/for_of/async_function.rs");
const ASYNC_FOR_OF_BODY_SOURCE: &str = include_str!("../../../scriptw-ir/src/async_for_of_body.rs");

const RESUMABLE_SYNC_FOR_OF_BINDING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/resumable_sync_for_of_binding.rs");
const GENERATOR_FOR_OF_BODY_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/generator_for_of_body.rs");
const GENERATOR_FOR_OF_PLAN_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/generator_for_of_iterator.rs");
const GENERATOR_FOR_OF_HEAD_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/generator_for_of_iterator/head.rs");
const GENERATOR_FOR_OF_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/for_of/generator.rs");
const GENERATOR_SOURCE_PLAN_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/generator_source_plan.rs");
const RESUMABLE_SYNC_FOR_OF_CONTROL_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/resumable_for_of_control.rs");

const ANALYSIS_SOURCE: &str = include_str!("../../../scriptw-ir/src/analysis.rs");
const LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering/for_of.rs");
const OBLIGATIONS_SOURCE: &str = include_str!("../../../scriptw-ir/src/iterator_obligations.rs");
const CONTROL_FLOW_SOURCE: &str = include_str!("../../src/control_flow.rs");
const ASYNC_FUNCTION_FOR_OF_ITERATOR_SOURCE: &str =
    include_str!("../../src/control_flow/async_function_for_of_iterator.rs");
const FOR_AWAIT_PLAN_SOURCE: &str =
    include_str!("../../src/control_flow/for_await_iterator_plan.rs");
const RESUMABLE_SYNC_FOR_OF_ITERATOR_SOURCE: &str =
    include_str!("../../src/control_flow/resumable_sync_for_of_iterator.rs");
const RESUMABLE_SYNC_FOR_OF_PLAN_SOURCE: &str =
    include_str!("../../src/control_flow/resumable_sync_for_of_iterator/plan.rs");
const DATA_SOURCE: &str = include_str!("../../src/data.rs");
const EMIT_SOURCE: &str = include_str!("../../src/emit.rs");
const EMISSION_SITES_SOURCE: &str = include_str!("../../src/emission_sites.rs");
const PLANNING_SOURCE: &str = include_str!("../../src/planning.rs");
const PROTOCOL_FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_plain_async_sync_for_of_iterator_protocol.js"
);
const CLOSE_FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_plain_async_sync_for_of_iterator_close.js"
);
const ERROR_FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_plain_async_sync_for_of_iterator_errors.js"
);
const MEMBER_FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_plain_async_sync_for_of_member_heads.js"
);
const PATTERN_FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_plain_async_sync_for_of_nonlexical_pattern_heads.js"
);
const LEXICAL_PATTERN_FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_plain_async_sync_for_of_lexical_pattern_heads.js"
);
const CAPTURE_FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_async_for_of_closure_capture.js");
const ITERATOR_CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/iterator.rs");
const FUNCTION_CLI_TESTS: &str = include_str!("../../../scriptw-cli/tests/cli/functions.rs");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/synchronous-array-for-of-iterator-protocol.md"
);
const MEMBER_CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/plain-async-synchronous-for-of-member-heads.md"
);
const PATTERN_CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/plain-async-synchronous-for-of-nonlexical-pattern-heads.md"
);
const LEXICAL_PATTERN_CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/plain-async-synchronous-for-of-lexical-pattern-heads.md"
);

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    source
        .split_once(start)
        .unwrap_or_else(|| panic!("missing start marker: {start}"))
        .1
        .split_once(end)
        .unwrap_or_else(|| panic!("missing end marker after: {start}"))
        .0
}

fn positions_in_order(source: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let offset = source[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("missing marker: {marker}"));
        cursor += offset + marker.len();
    }
}

fn compact(source: &str) -> String {
    source
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn rust_source_occurrences(root: &Path, needle: &str) -> usize {
    fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
        .map(|entry| entry.expect("failed to read Rust source entry").path())
        .map(|path| {
            if path.is_dir() {
                return rust_source_occurrences(&path, needle);
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
fn resumable_sync_for_of_emitter_has_one_private_child_owner_and_two_typed_routes() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for module in [
        "async_function_for_of_iterator",
        "resumable_sync_for_of_iterator",
        "for_await_iterator_plan",
    ] {
        let declaration = format!("mod {module};");
        assert_eq!(CONTROL_FLOW_SOURCE.matches(&declaration).count(), 1);
        assert!(!CONTROL_FLOW_SOURCE.contains(&format!("pub mod {module};")));
        assert!(!CONTROL_FLOW_SOURCE.contains(&format!("pub(crate) mod {module};")));
        assert_eq!(rust_source_occurrences(&source_root, &declaration), 1);
    }
    for source in [
        CONTROL_FLOW_SOURCE,
        ASYNC_FUNCTION_FOR_OF_ITERATOR_SOURCE,
        RESUMABLE_SYNC_FOR_OF_ITERATOR_SOURCE,
        RESUMABLE_SYNC_FOR_OF_PLAN_SOURCE,
        FOR_AWAIT_PLAN_SOURCE,
    ] {
        assert!(!source.contains("include!("));
        assert!(!source.contains("#[path"));
    }

    assert_eq!(
        RESUMABLE_SYNC_FOR_OF_ITERATOR_SOURCE
            .matches("mod plan;")
            .count(),
        1
    );
    assert!(!RESUMABLE_SYNC_FOR_OF_ITERATOR_SOURCE.contains("pub mod plan;"));
    assert!(!RESUMABLE_SYNC_FOR_OF_ITERATOR_SOURCE.contains("pub(crate) mod plan;"));

    let owner = "compile_resumable_sync_for_of_iterator";
    assert_eq!(
        RESUMABLE_SYNC_FOR_OF_ITERATOR_SOURCE
            .matches(&format!("    pub(crate) fn {owner}("))
            .count(),
        1
    );
    assert!(!CONTROL_FLOW_SOURCE.contains(&format!("fn {owner}(")));
    assert!(!ASYNC_FUNCTION_FOR_OF_ITERATOR_SOURCE.contains(&format!("fn {owner}(")));
    assert_eq!(
        rust_source_occurrences(&source_root, &format!("fn {owner}(")),
        1
    );
    assert_eq!(rust_source_occurrences(&source_root, owner), 4);
    assert_eq!(
        EMISSION_SITES_SOURCE
            .matches(&format!("FunctionBuilder::{owner}"))
            .count(),
        1
    );

    for (route, variant, plan_type) in [
        (
            "compile_async_function_for_of_iterator",
            "Async",
            "AsyncFunctionForOfIteratorPlanIr",
        ),
        (
            "compile_generator_for_of_iterator",
            "Generator",
            "scriptw_ir::GeneratorForOfIteratorPlanIr",
        ),
    ] {
        let wrapper = bounded(
            ASYNC_FUNCTION_FOR_OF_ITERATOR_SOURCE,
            &format!("    pub(crate) fn {route}("),
            "\n    }",
        );
        assert!(wrapper.contains(&format!("plan: &{plan_type}")));
        assert_eq!(
            compact(wrapper)
                .matches("self.compile_resumable_sync_for_of_iterator(")
                .count(),
            1
        );
        assert!(wrapper.contains(&format!("ResumableSyncForOfPlan::{variant}(plan)")));
        for inline_body in [
            "reserve_temp_local",
            "emit_get_iterator",
            "emit_sync_iterator_step_value",
            "emit_iterator_close",
            "compile_async_statement_sequence",
            "compile_generator_statement_sequence",
            "finally_stack",
        ] {
            assert!(
                !wrapper.contains(inline_body),
                "wrapper duplicates {inline_body}"
            );
        }
        assert!(!CONTROL_FLOW_SOURCE.contains(&format!("fn {route}(")));
        assert_eq!(
            CONTROL_FLOW_SOURCE
                .matches(&format!("self.{route}(iterable, plan, function)?;"))
                .count(),
            1
        );
        assert_eq!(
            EMISSION_SITES_SOURCE
                .matches(&format!("FunctionBuilder::{route}"))
                .count(),
            1
        );
        assert_eq!(
            rust_source_occurrences(&source_root, &format!("fn {route}(")),
            1
        );
        assert_eq!(rust_source_occurrences(&source_root, route), 3);
    }
}

#[test]
fn closed_resumable_owner_selects_each_activation_body_and_completion_protocol() {
    let adapter = bounded(
        RESUMABLE_SYNC_FOR_OF_PLAN_SOURCE,
        "pub(crate) enum ResumableSyncForOfPlan<'a> {",
        "#[derive(Clone, Copy)]",
    );
    assert!(adapter.contains("Async(scriptw_ir::AsyncFunctionForOfSynchronousIteratorIr<'a>)"));
    assert!(!adapter.contains("Async(&'a AsyncFunctionForOfIteratorPlanIr)"));
    assert!(adapter.contains("Generator(&'a GeneratorForOfIteratorPlanIr)"));
    assert_eq!(adapter.matches("(&'a ").count(), 1);

    let storage = bounded(
        RESUMABLE_SYNC_FOR_OF_PLAN_SOURCE,
        "    pub(super) fn value_storage(self)",
        "    pub(super) fn entry_state(self)",
    );
    let async_storage = bounded(
        storage,
        "Self::Async(plan) => match plan.plan().value_storage() {",
        "Self::Generator(plan)",
    );
    let generator_storage = storage
        .split_once("Self::Generator(plan) => match plan.value_storage() {")
        .unwrap()
        .1;
    for variant in ["Activation(binding)", "IterationEnvironment(binding)"] {
        assert_eq!(
            async_storage
                .matches(&format!(
                    "AsyncFunctionForOfIteratorValueStorageIr::{variant}"
                ))
                .count(),
            1
        );
        assert_eq!(
            generator_storage
                .matches(&format!("GeneratorForOfIteratorValueStorageIr::{variant}"))
                .count(),
            1
        );
    }
    assert!(async_storage.contains("AsyncFunctionForOfIteratorValueStorageIr::EntryLocal { name }"));
    assert!(generator_storage.contains("GeneratorForOfIteratorValueStorageIr::EntryLocal { name }"));
    assert!(!storage.contains("_ =>"));
}

#[test]
fn closed_plan_couples_the_iterator_record_checked_body_states_and_environments() {
    assert!(ASYNC_FOR_OF_PLAN_SOURCE
        .contains("#[must_use = \"a plain-async for-of plan must be attached to its statement\"]"));
    let head = bounded(
        IR_SOURCE,
        "pub(crate) enum AsyncFunctionForOfIteratorHeadIr {",
        "/// Where one resumable synchronous `for-of` stores IteratorValue.",
    );
    for marker in [
        "Binding {",
        "source_name: String",
        "binding: ForOfAssignmentIr",
        "PreparedAssignment { value_name: String }",
        "LexicalPattern {",
        "mode: BindingMode",
        "value_name: String",
        "iteration_storage_names: Vec<String>",
        "tdz_placeholder_names: Vec<String>",
        "initialization: Vec<StatementIr>",
    ] {
        assert!(head.contains(marker), "closed head input: {marker}");
    }

    let value_storage = bounded(
        IR_SOURCE,
        "pub enum AsyncFunctionForOfIteratorValueStorageIr {",
        "pub(crate) enum AsyncFunctionForOfIteratorEnvironmentError {",
    );
    for variant in [
        "Activation(ForOfAssignmentIr)",
        "IterationEnvironment(ForOfAssignmentIr)",
        "EntryLocal { name: String }",
    ] {
        assert!(value_storage.contains(variant), "value storage: {variant}");
    }

    let plan = ASYNC_FOR_OF_PLAN_SOURCE;
    for field in [
        "value_storage: AsyncFunctionForOfIteratorValueStorageIr",
        "value_mode: BindingMode",
        "record: IteratorRecordIr",
        "head_environment: Option<ForInOfEnvironmentIr>",
        "iteration_environment: ResumableLoopIterationEnvironmentIr",
        "body: AsyncFunctionForOfBodyIr",
        "exit_state: u32",
    ] {
        assert!(plan.contains(field), "{field}");
        assert!(!plan.contains(&format!("pub {field}")), "public {field}");
    }
    assert!(!plan.contains("binding: ForOfAssignmentIr"));
    assert!(plan.contains("pub(crate) fn new("));
    assert!(!plan.contains("pub fn new("));
    assert!(plan.contains("head: AsyncFunctionForOfIteratorHeadIr"));
    let head_derivation = bounded(
        plan,
        "        let (",
        "        initialization.append(&mut statements);",
    );
    assert!(compact(head_derivation).starts_with(
        "value_storage,value_mode,iteration_environment,mutinitialization,head_environment,)=matchhead{"
    ));
    for variant in [
        "AsyncFunctionForOfIteratorHeadIr::Binding {",
        "AsyncFunctionForOfIteratorHeadIr::PreparedAssignment { value_name }",
        "AsyncFunctionForOfIteratorHeadIr::LexicalPattern {",
    ] {
        assert!(
            head_derivation.contains(variant),
            "head derivation: {variant}"
        );
    }
    assert!(!head_derivation.contains("_ =>"));
    positions_in_order(
        &compact(plan),
        &[
            "let(value_storage,value_mode,iteration_environment,mutinitialization,head_environment,)=matchhead",
            "AsyncFunctionForOfIteratorHeadIr::PreparedAssignment",
            "AsyncFunctionForOfIteratorHeadIr::LexicalPattern",
            "AsyncFunctionForOfIteratorPlanError::CapturedTdzEnvironment",
            "initialization.append(&mutstatements)",
            "AsyncFunctionForOfBodyIr::new(",
            ".map_err(AsyncFunctionForOfIteratorPlanError::InvalidBody)",
            "letbody_exit_state=body.exit_state()",
            "letsuccessor=body_exit_state",
            ".checked_add(1)",
            "Ok(Self",
        ],
    );
    assert!(plan.contains("execution: AsyncFunctionForOfIteratorProtocolIr"));
    assert!(plan.contains("pub(crate) fn new_for_await("));
    assert!(plan.contains("AsyncFunctionForOfBodyIr::new_for_await("));
    assert!(plan.contains("AsyncFunctionForOfIteratorExecutionIr::Synchronous"));
    assert!(plan.contains("AsyncFunctionForOfIteratorExecutionIr::Awaited"));
    assert!(plan.contains("AwaitedProtocolStorageAlias"));
    let constructor_input = bounded(
        plan,
        "enum AsyncFunctionForOfIteratorInputIr {",
        "/// The private constructors bind each borrowed execution view",
    );
    assert!(!constructor_input.contains("close_resume_state"));
    let completed_body = plan
        .split_once("let body_exit_state = body.exit_state();")
        .unwrap()
        .1;
    positions_in_order(
        completed_body,
        &[
            "let successor = body_exit_state",
            "AwaitedProtocolStorageAlias",
            "let close_resume_state = successor",
            "let exit_state = close_resume_state.checked_add(1)",
            "AsyncFunctionForOfIteratorProtocolIr::Awaited {",
            "Ok(Self",
        ],
    );
    assert!(FOR_AWAIT_PLAN_SOURCE
        .contains("Awaited(scriptw_ir::AsyncFunctionForAwaitOfIteratorIr<'a>)"));
    assert!(FOR_AWAIT_PLAN_SOURCE.contains("pub(crate) struct ForAwaitIteratorPlan<'a>"));
    assert!(FOR_AWAIT_PLAN_SOURCE.contains("execution: ForAwaitIteratorExecution<'a>"));
    assert!(!FOR_AWAIT_PLAN_SOURCE.contains("pub execution:"));
    assert!(FOR_AWAIT_PLAN_SOURCE.contains("enum ForAwaitIteratorExecution<'a>"));
    assert!(!FOR_AWAIT_PLAN_SOURCE.contains("pub(crate) enum ForAwaitIteratorExecution"));
    assert!(FOR_AWAIT_PLAN_SOURCE.contains("pub(super) fn existing("));
    assert!(FOR_AWAIT_PLAN_SOURCE.contains("pub(super) fn awaited("));
    let activation = bounded(
        CONTROL_FLOW_SOURCE,
        "pub(crate) fn compile_async_for_of_iterator(",
        "let body_suspends = plan.body_suspends()",
    );
    assert!(activation
        .contains("Some(FunctionExecutionKind::Async) => AsyncContinuationOwner::AsyncFunction"));
    assert!(compact(activation).contains(
        "Some(FunctionExecutionKind::AsyncGenerator)if!plan.requires_plain_async_activation()=>"
    ));
    assert!(activation.contains("for-await requires its checked async activation owner"));
    let policy = bounded(
        FOR_AWAIT_PLAN_SOURCE,
        "pub(super) fn requires_plain_async_activation(",
        "pub(super) fn entry_state(",
    );
    assert!(policy.contains("ForAwaitIteratorExecution::Existing { .. } => false"));
    assert!(policy.contains("ForAwaitIteratorExecution::Awaited(_) => true"));
    assert!(!policy.contains("_ =>"));
    assert!(ASYNC_FUNCTION_FOR_OF_ITERATOR_SOURCE.contains("ForAwaitIteratorPlan::awaited(plan)"));
    assert!(FOR_AWAIT_PLAN_SOURCE.contains("plan.plan().body().statements()"));
    assert!(FOR_AWAIT_PLAN_SOURCE.contains("plan.plan().body().entry_state()"));
    assert!(!FOR_AWAIT_PLAN_SOURCE.contains("body: AsyncFunctionForOfBodyIr"));

    let sequence_errors = bounded(
        IR_SOURCE,
        "pub enum AwaitSequenceError {",
        "/// Validate a nonempty sequence of direct awaits",
    );
    assert_eq!(
        compact(sequence_errors),
        "FirstAwaitRequired,NestedSuspension,StateMismatch{\
         expected_suspend_state:u32,suspend_state:u32,resume_state:u32,},}"
    );
    let sequence = bounded(
        IR_SOURCE,
        "pub fn direct_await_sequence_resume_state(",
        "fn duplicate_async_function_for_of_name(",
    );
    positions_in_order(
        sequence,
        &[
            "if !matches!(first, StatementIr::AsyncAwait { .. })",
            "AwaitSequenceError::FirstAwaitRequired",
            "let mut state = entry_state",
            "for statement in std::iter::once(first).chain(after)",
            "*suspend_state != state || state.checked_add(1) != Some(*resume_state)",
            "AwaitSequenceError::StateMismatch",
            "state = *resume_state",
            "statement if statement_contains_suspension(statement)",
            "AwaitSequenceError::NestedSuspension",
            "Ok(state)",
        ],
    );
    for accessor in [
        "pub fn value_storage(&self) -> &AsyncFunctionForOfIteratorValueStorageIr",
        "pub fn value_name(&self) -> &str",
        "pub fn value_mode(&self) -> BindingMode",
    ] {
        assert!(plan.contains(accessor), "plan accessor: {accessor}");
    }

    let statement = bounded(
        IR_SOURCE,
        "    AsyncFunctionForOfIterator {",
        "    ForInArray {",
    );
    assert!(statement.contains("iterable: TypedExpr"));
    assert!(statement.contains("plan: AsyncFunctionForOfIteratorPlanIr"));
    assert!(!statement.contains("body:"));
    assert!(!statement.contains("lexical_environment:"));
}

#[test]
fn shared_identifier_head_and_generator_plan_are_validated_before_backend_use() {
    let proof = bounded(
        RESUMABLE_SYNC_FOR_OF_BINDING_SOURCE,
        "pub(crate) struct ValidatedResumableSyncForOfBindingIr {",
        "impl From<ResumableSyncForOfBindingError>",
    );
    for field in [
        "storage: ResumableSyncForOfBindingStorageIr",
        "head_environment: Option<ForInOfEnvironmentIr>",
        "iteration_environment: ResumableLoopIterationEnvironmentIr",
    ] {
        assert!(proof.contains(field), "head proof field: {field}");
        assert!(!proof.contains(&format!("pub {field}")));
    }
    for invariant in [
        "BindingHeadEnvironmentRequired",
        "VarBindingHasHeadEnvironment",
        "SingleBindingTdzNameCount",
        "validate_async_function_for_of_environment",
        "SingleBindingIterationNamesMismatch",
        "CapturedTdzEnvironment",
        "ResumableLoopIterationEnvironmentIr::FreshPerIteration",
    ] {
        assert!(
            proof.contains(invariant),
            "shared head invariant: {invariant}"
        );
    }
    let async_binding = bounded(
        ASYNC_FOR_OF_PLAN_SOURCE,
        "AsyncFunctionForOfIteratorHeadIr::Binding {",
        "AsyncFunctionForOfIteratorHeadIr::PreparedAssignment",
    );
    positions_in_order(
        async_binding,
        &[
            "ValidatedResumableSyncForOfBindingIr::new(",
            "&source_name,",
            ".map_err(AsyncFunctionForOfIteratorPlanError::from)?",
            ".into_async_parts()",
        ],
    );

    let lexical_proof = RESUMABLE_SYNC_FOR_OF_BINDING_SOURCE
        .split_once("pub(crate) struct ValidatedResumableSyncForOfLexicalPatternIr {")
        .unwrap()
        .1;
    for field in [
        "mode: BindingMode",
        "value_name: String",
        "head_environment: ForInOfEnvironmentIr",
        "iteration_environment: ResumableLoopIterationEnvironmentIr",
        "initialization: Vec<StatementIr>",
    ] {
        assert!(
            lexical_proof.contains(field),
            "shared lexical proof: {field}"
        );
        assert!(!lexical_proof.contains(&format!("pub {field}")));
    }
    for invariant in [
        "BindingMode::Let | BindingMode::Const",
        "LexicalPatternValueNameCollision",
        "LexicalPatternTdzNamesMismatch",
        "LexicalPatternIterationNamesMismatch",
        "validate_async_function_for_of_initialization(",
        "ResumableLoopIterationEnvironmentIr::FreshPerIteration",
    ] {
        assert!(
            lexical_proof.contains(invariant),
            "shared lexical proof: {invariant}"
        );
    }
    let async_pattern = bounded(
        ASYNC_FOR_OF_PLAN_SOURCE,
        "AsyncFunctionForOfIteratorHeadIr::LexicalPattern {",
        "        if matches!(",
    );
    assert!(async_pattern.contains("ValidatedResumableSyncForOfLexicalPatternIr::new("));
    assert!(async_pattern.contains(".into_async_parts()"));
    assert!(LOWERING_SOURCE.contains("ValidatedResumableSyncForOfLexicalPatternIr::new("));
    for marker in [
        "pattern: ValidatedResumableSyncForOfLexicalPatternIr",
        "prefix != self.pattern.initialization()",
        "entry_local_storage_has_only_dynamic_reads(",
    ] {
        assert!(
            GENERATOR_FOR_OF_HEAD_SOURCE.contains(marker),
            "lexical prefix: {marker}"
        );
    }
    assert!(RESUMABLE_SYNC_FOR_OF_PLAN_SOURCE
        .contains("Self::Generator(plan) => plan.head_binding_environment()"));

    for field in [
        "head: GeneratorForOfIteratorHeadIr",
        "record: IteratorRecordIr",
        "body: GeneratorForOfBodyIr",
        "exit_state: u32",
    ] {
        assert!(GENERATOR_FOR_OF_PLAN_SOURCE.contains(field));
        assert!(!GENERATOR_FOR_OF_PLAN_SOURCE.contains(&format!("pub {field}")));
    }
    assert!(!GENERATOR_FOR_OF_PLAN_SOURCE.contains("pub fn new("));
    positions_in_order(
        GENERATOR_FOR_OF_PLAN_SOURCE,
        &[
            "pub(crate) fn new(",
            "head: GeneratorForOfIteratorHeadInputIr",
            "body: GeneratorForOfBodyIr",
            "complete(&body)",
            "GeneratorForOfIteratorPlanError::InvalidAssignment",
            "let body_exit_state = body.exit_state()",
            ".checked_add(1)",
            "GeneratorForOfIteratorPlanError::ExitStateOverflow",
            "Ok(Self",
        ],
    );
    for invariant in [
        "pub(crate) struct GeneratorForOfAssignmentIr",
        "fn ordinary_property_write(",
        "ExprIr::OrdinaryPropertyAssignment(assignment)",
        "sink(assignment.rhs(), value)",
        "assignment.base_and_receiver()",
        "assignment.referenced_name()",
        "statements_reference_storage(&reference_operands, value)",
        "SpellableSink",
        "HeadEnvironment",
        "PersistentSink",
        "InvalidPrefix",
        "PrefixMismatch",
        "SinkEscapesBody",
        "body.statements().split_first()",
        "statements_reference_storage(rest, &self.value_name)",
        "ResumableLoopIterationEnvironmentIr::StorageOnly",
    ] {
        assert!(
            GENERATOR_FOR_OF_HEAD_SOURCE.contains(invariant),
            "assignment proof: {invariant}"
        );
    }
    for invariant in [
        "YieldRequired",
        "require_state(state, *suspend_state)",
        "require_state(successor(state)?, *resume_state)",
        "GeneratorTryPlanIr",
        "GeneratorForOfBodyError::TryClauseLayout",
        "StatementIr::AsyncFunctionForOfIterator",
        "StatementIr::GeneratorForOfIterator",
        "StatementIr::Break",
        "StatementIr::Continue",
    ] {
        assert!(GENERATOR_FOR_OF_BODY_SOURCE.contains(invariant));
    }
    for lowering_step in [
        "FunctionExecutionKind::Generator",
        "ValidatedResumableSyncForOfBindingIr::new",
        "GeneratorForOfAssignmentIr::identifier",
        "GeneratorForOfAssignmentIr::ordinary_property",
        "GeneratorForOfLexicalPatternIr::new",
        "GeneratorForOfBodyIr::new",
        "self.current_generator_resume_state != Some(body.exit_state())",
        "IteratorRecordIr::new",
        "GeneratorForOfIteratorPlanIr::new",
        "self.current_generator_resume_state = Some(plan.exit_state())",
        "ForOfLoweringIr::generator_iterator",
    ] {
        assert!(GENERATOR_FOR_OF_LOWERING_SOURCE.contains(lowering_step));
    }
    for source_boundary in [
        "IterableLoopInitializer::Var(variable)",
        "IterableLoopInitializer::Let(_)",
        "IterableLoopInitializer::Const(_)",
        "IterableLoopInitializer::Identifier(_)",
        "IterableLoopInitializer::Access(PropertyAccess::Simple(_))",
        "!for_of.r#await()",
        "!contains(for_of.initializer(), ContainsSymbol::YieldExpression)",
        "!contains(for_of.initializer(), ContainsSymbol::AwaitExpression)",
        "resumable_sync_for_of_body_has_local_control_owners(for_of.body())",
        "current_state.checked_add(1)?",
    ] {
        assert!(
            GENERATOR_SOURCE_PLAN_SOURCE.contains(source_boundary),
            "{source_boundary}"
        );
    }
    let nested_iterator = bounded(
        GENERATOR_SOURCE_PLAN_SOURCE,
        "if let Statement::ForOfLoop(for_of) = statement.as_ref()",
        "if let Statement::Try(try_statement)",
    );
    assert!(compact(nested_iterator).contains("GeneratorSuspensionRegion::IteratorBodyifcontains(for_of,ContainsSymbol::YieldExpression)=>{returnNone}"));
    assert!(nested_iterator.contains("GeneratorSuspensionRegion::IteratorBody => {}"));
    assert!(GENERATOR_SOURCE_PLAN_SOURCE.contains(
        "GeneratorSuspensionRegion::IteratorBody => GeneratorValueBranchAdmission::LinearOnly"
    ));
    assert!(!nested_iterator.contains("_ =>"));
    assert!(LOWERING_SOURCE
        .contains("!resumable_sync_for_of_body_has_local_control_owners(for_of.body())"));
    for owner_boundary in [
        "pub(crate) enum ResumableSyncForOfBranchOwner",
        "pub(crate) fn resumable_sync_for_of_body_has_local_control_owners",
        "(ResumableSyncForOfBranchOwner::CurrentLoop, None) => ControlFlow::Continue(())",
        "(ResumableSyncForOfBranchOwner::CurrentLoop, Some(_))",
        "(ResumableSyncForOfBranchOwner::NestedStatement, _) => ControlFlow::Break(())",
    ] {
        assert!(RESUMABLE_SYNC_FOR_OF_CONTROL_SOURCE.contains(owner_boundary));
    }
    positions_in_order(
        ASYNC_FOR_OF_BODY_SOURCE,
        &[
            "pub(crate) fn new(",
            "validate_branch_ownership(&statements, ResumableSyncForOfBranchOwner::CurrentLoop)?",
            "let mut validation = BodyValidation",
            "validation.sequence(&statements, entry_state)?",
            "Ok(Self",
        ],
    );
}

#[test]
fn lowering_allocates_typed_record_slots_and_never_synthesizes_an_array_walk() {
    for retired in [
        "AsyncForOfArrayWalkForm",
        "lower_async_for_of_array_with_body_await",
        "ARRAY_INDEX_WALK_RESUMABLE",
    ] {
        assert!(!IR_SOURCE.contains(retired), "IR still contains {retired}");
        assert!(
            !LOWERING_SOURCE.contains(retired),
            "lowering still contains {retired}"
        );
        assert!(
            !OBLIGATIONS_SOURCE.contains(retired),
            "obligations still contain {retired}"
        );
        assert!(
            !CONTROL_FLOW_SOURCE.contains(retired),
            "backend still contains {retired}"
        );
        assert!(
            !RESUMABLE_SYNC_FOR_OF_ITERATOR_SOURCE.contains(retired),
            "resumable backend still contains {retired}"
        );
        assert!(
            !PLANNING_SOURCE.contains(retired),
            "planner still contains {retired}"
        );
    }

    let lowerer = ASYNC_FOR_OF_LOWERING_SOURCE;
    positions_in_order(
        lowerer,
        &[
            "let statements = match body",
            "flatten_suspending_lexical_blocks(statements)",
            "IteratorRecordIr::new(",
            "self.alloc_iterator_slot()",
            "self.alloc_next_method_slot()",
            "self.alloc_done_slot()",
            "AsyncFunctionForOfIteratorPlanIr::new(",
            "self.current_async_resume_state = Some(plan.exit_state())",
            "ForOfLoweringIr::async_function_iterator(iterable, plan, body_kind)",
        ],
    );
    let publication = bounded(
        lowerer,
        "        let body_exit_matches = match (plan.execution(), lowered_body_exit) {",
        "        match plan.value_storage() {",
    );
    for proof in [
        "(AsyncFunctionForOfIteratorExecutionIr::Synchronous(_), None) => true",
        "(AsyncFunctionForOfIteratorExecutionIr::Awaited(_), Some(actual))",
        "actual == plan.body().exit_state()",
        "(AsyncFunctionForOfIteratorExecutionIr::Awaited(_), None) => false",
        "if !body_exit_matches",
    ] {
        assert!(publication.contains(proof), "publication proof: {proof}");
    }
    assert!(!publication.contains("_ =>"));
    assert!(!lowerer.contains("PropertyKeyIr::ArrayLength"));
    assert!(!lowerer.contains("PropertyKeyIr::ArrayIndex"));
    assert!(lowerer.contains("head: AsyncFunctionForOfIteratorHeadIr"));
    let activation_ownership = bounded(
        lowerer,
        "        match plan.value_storage() {",
        "        self.current_async_resume_state = Some(plan.exit_state());",
    );
    for variant in [
        "AsyncFunctionForOfIteratorValueStorageIr::Activation(binding)",
        "AsyncFunctionForOfIteratorValueStorageIr::IterationEnvironment(_)",
        "AsyncFunctionForOfIteratorValueStorageIr::EntryLocal { .. }",
    ] {
        assert!(
            activation_ownership.contains(variant),
            "activation ownership: {variant}"
        );
    }
    assert!(!activation_ownership.contains("_ =>"));

    assert!(!LOWERING_SOURCE.contains("does not support let or const pattern heads"));
    let lexical_pattern_classification = bounded(
        LOWERING_SOURCE,
        "        let lexical_pattern_bindings = match pattern_initializer.as_ref() {",
        "        let resumable_sync_head_is_assignment = match head_kind {",
    );
    for marker in [
        "Some((BindingMode::Let | BindingMode::Const, pattern))",
        "supported_bound_names(self.interner, &binding)",
        "LexicalForOfPatternBinding",
        "for_of_loop_binding_storage_name(",
        "Some((BindingMode::Var, _)) | None => None",
    ] {
        assert!(
            lexical_pattern_classification.contains(marker),
            "lexical pattern classification: {marker}"
        );
    }

    let bare_assignment_prefix = bounded(
        LOWERING_SOURCE,
        "        } else if let ForOfBareIdentifierHead::AssignmentTarget",
        "        } else if let Some(access) = access_initializer.as_ref() {",
    );
    positions_in_order(
        bare_assignment_prefix,
        &[
            "source_name",
            "ExprIr::Identifier(storage_name.clone())",
            "self.lower_bare_iteration_head_write(source_name.clone(), value)",
            "vec![StatementIr::DeclarationEvaluation(assignment)]",
        ],
    );

    let access_assignment_prefix = bounded(
        LOWERING_SOURCE,
        "        } else if let Some(access) = access_initializer.as_ref() {",
        "        } else if let Some(pattern) = assignment_pattern_initializer.as_ref() {",
    );
    positions_in_order(
        access_assignment_prefix,
        &[
            "ExprIr::Identifier(storage_name.clone())",
            "let assignment = match (generator_entry_state, access)",
            "(Some(_), PropertyAccess::Simple(access))",
            "self.lower_ordinary_property_reference_plan(access)",
            "self.complete_ordinary_property_plain_assignment(",
            "_ => self.lower_property_assign_value(access, value)",
        ],
    );

    let assignment_pattern_prefix = bounded(
        LOWERING_SOURCE,
        "        } else if let Some(pattern) = assignment_pattern_initializer.as_ref() {",
        "        } else if let Some((pattern_mode, pattern)) = pattern_initializer.as_ref() {",
    );
    positions_in_order(
        assignment_pattern_prefix,
        &[
            "ExprIr::Identifier(storage_name.clone())",
            "self.lower_pattern_assign_value(pattern, value)",
            "vec![StatementIr::DeclarationEvaluation(assign)]",
        ],
    );

    let declaration_pattern_prefix = bounded(
        LOWERING_SOURCE,
        "        } else if let Some((pattern_mode, pattern)) = pattern_initializer.as_ref() {",
        "        } else {\n            Vec::new()",
    );
    assert!(declaration_pattern_prefix.contains("*pattern_mode == BindingMode::Var"));
    assert!(declaration_pattern_prefix
        .contains("self.lower_pattern_var_binding_from_value(pattern, init)"));
    positions_in_order(
        declaration_pattern_prefix,
        &[
            "let bindings = lexical_pattern_bindings",
            "let storage_names = bindings",
            "Initialization::Uninitialized(",
            "UninitializedStorage::Allocated",
            ".lower_pattern_lexical_binding_from_value_with_storage_names(",
        ],
    );

    let resumable_head = bounded(
        LOWERING_SOURCE,
        "        if plain_async_await_body {\n            let head = if let (",
        "            return self.lower_async_function_for_of_iterator_with_body_await(",
    );
    positions_in_order(
        resumable_head,
        &[
            "AsyncFunctionForOfIteratorHeadIr::LexicalPattern",
            "iteration_storage_names: bindings",
            "tdz_placeholder_names: bindings",
            "initialization: lexical_pattern_initialization",
            "AsyncFunctionForOfIteratorHeadIr::PreparedAssignment",
            "AsyncFunctionForOfIteratorHeadIr::Binding {",
        ],
    );

    let statement_capture_scan = bounded(
        ANALYSIS_SOURCE,
        "    fn scan_statement(",
        "    fn scan_array_pattern_expressions(",
    );
    let for_of_capture_scan = bounded(
        statement_capture_scan,
        "            Statement::ForOfLoop(for_of) => {",
        "            Statement::ForInLoop(for_in) => {",
    );
    let access_capture_scan = bounded(
        for_of_capture_scan,
        "IterableLoopInitializer::Access(access) => {",
        "_ => {}",
    );
    assert!(access_capture_scan.contains("self.scan_property_access("));
    assert!(access_capture_scan.contains("&body_aliases"));
    assert!(ANALYSIS_SOURCE.contains("fn scan_object_assignment_pattern_expressions("));
    let assignment_pattern_scan = bounded(
        ANALYSIS_SOURCE,
        "    fn scan_assignment_pattern_expressions(",
        "    fn scan_expression(",
    );
    assert!(assignment_pattern_scan.contains("Pattern::Array(pattern)"));
    assert!(assignment_pattern_scan.contains("Pattern::Object(pattern)"));
    assert!(assignment_pattern_scan.contains("self.scan_object_assignment_pattern_expressions("));

    let generic_value = LOWERING_SOURCE
        .split_once("// A generic iterator can yield values unrelated to the iterable's")
        .expect("generic iterator value boundary")
        .1
        .split_once("        };")
        .expect("generic iterator value boundary end")
        .0;
    assert!(generic_value.contains("kind: ValueKind::Dynamic"));
    assert!(generic_value.contains("possible_kinds: KindSet::all_runtime_tags()"));
    assert!(generic_value.contains("heap_shape: None"));
    assert!(generic_value.contains("function_targets: FunctionTargetKnowledge::unknown()"));
    assert!(!LOWERING_SOURCE.contains("let iterable_is_array ="));

    assert!(OBLIGATIONS_SOURCE
        .contains("RESUMABLE_SYNC_ITERATOR_PROTOCOL => IteratorProtocolWitness::emitted_by("));
    assert!(OBLIGATIONS_SOURCE.contains("EmissionSite::ResumableSyncForOfIterator"));
    assert!(EMISSION_SITES_SOURCE.contains(
        "EmissionSite::ResumableSyncForOfIterator => {\n            let _ = FunctionBuilder::compile_resumable_sync_for_of_iterator;"
    ));
}

#[test]
fn checked_body_preserves_structure_and_rejects_unowned_continuations() {
    let body = bounded(
        ASYNC_FOR_OF_BODY_SOURCE,
        "pub struct AsyncFunctionForOfBodyIr {",
        "#[derive(Debug, Clone, PartialEq, Eq)]",
    );
    for field in [
        "statements: Vec<StatementIr>",
        "entry_state: u32",
        "exit_state: u32",
    ] {
        assert!(body.contains(field));
        assert!(!body.contains(&format!("pub {field}")));
    }
    assert!(ASYNC_FOR_OF_BODY_SOURCE.contains("pub(crate) fn new("));
    assert!(!ASYNC_FOR_OF_BODY_SOURCE.contains("pub fn new("));
    for owner in [
        "StatementIr::AsyncAwait",
        "StatementIr::Block",
        "StatementIr::LexicalBlock",
        "StatementIr::AsyncFunctionIf",
        "StatementIr::TryCatch",
        "StatementIr::TryFinally",
        "StatementIr::TryCatchFinally",
    ] {
        assert!(
            ASYNC_FOR_OF_BODY_SOURCE.contains(owner),
            "continuation owner: {owner}"
        );
    }
    for proof in [
        "require_state(state, *suspend_state)?",
        "require_state(successor(state)?, *resume_state)?",
        "require_state(state, plan.entry_state)?",
        "require_state(successor(try_end)?, plan.try_exit_state)?",
        "AsyncFunctionForOfBodyError::TryClauseLayout",
        "AsyncFunctionForOfBodyError::UnsupportedContinuation",
        "AsyncFunctionForOfBodyError::AwaitRequired",
        "BodyEnvironmentPolicy::ForAwaitHeadOnly",
        "AsyncFunctionForOfBodyError::MaterializedBodyEnvironment",
        "self.environment(block.lexical_environment.is_some())?",
    ] {
        assert!(
            ASYNC_FOR_OF_BODY_SOURCE.contains(proof),
            "body proof: {proof}"
        );
    }
    let statement_dispatch = ASYNC_FOR_OF_BODY_SOURCE
        .split_once("    fn statement(")
        .expect("checked statement dispatcher")
        .1;
    assert!(!statement_dispatch.contains("_ =>"));
}

#[test]
fn planner_adds_every_persistent_record_local_to_the_deepest_child() {
    for consumer in [DATA_SOURCE, EMIT_SOURCE, PLANNING_SOURCE] {
        for variant in [
            "StatementIr::AsyncFunctionForOfIterator",
            "StatementIr::GeneratorForOfIterator",
        ] {
            assert!(
                consumer.contains(variant),
                "statement traversal omitted {variant}"
            );
        }
    }
}

#[test]
fn runtime_oracles_cover_acquisition_close_errors_strings_and_fresh_bindings() {
    for marker in [
        "get next()",
        "arrayIteratorMethodReads, 1",
        "arrayNextReads, 1",
        "arrayReturnCalls, 0",
        "return { value: \"4\", done: false }",
        "for (assignedValue of customIterable)",
        "assignedValue, \"assigned-2\"",
        "assignmentIteratorCalls, 1",
        "assignmentNextCalls, 3",
        "for (const value of \"native\")",
        "plain-async-sync-for-of:protocol=ok",
    ] {
        assert!(
            PROTOCOL_FIXTURE.contains(marker),
            "protocol fixture: {marker}"
        );
    }
    for marker in [
        "await Promise.reject(bodyError)",
        "throw bodyCloseError",
        "bodyError,\n      \"body rejection identity\"",
        "same(bodyCloseCalls, 1",
        "return \"unobservable return\"",
        "throw returnCloseError",
        "returnCloseError,\n      \"return close error identity\"",
        "same(returnCloseCalls, 1",
        "plain-async-sync-for-of:close=ok",
    ] {
        assert!(CLOSE_FIXTURE.contains(marker), "close fixture: {marker}");
    }
    for marker in [
        "throw nextError",
        "get done()",
        "get value()",
        "same(nextCloseCalls, 0",
        "same(doneCloseCalls, 0",
        "same(valueCloseCalls, 0",
        "plain-async-sync-for-of:protocol-errors=ok",
    ] {
        assert!(ERROR_FIXTURE.contains(marker), "error fixture: {marker}");
    }
    for marker in [
        "for (staticTarget.value of [3, 5])",
        "for (memberBase()[memberKey()] of memberIterable)",
        "for (this.#value of [7, 9])",
        "for (throwingTarget.value of closingIterable)",
        "for (wrong.#value of privateClosingIterable)",
        "plain-async-sync-for-of:member-heads=ok",
    ] {
        assert!(MEMBER_FIXTURE.contains(marker), "member fixture: {marker}");
    }
    for marker in [
        "for (var [selected = arrayDefault(), ...remaining] of [",
        "for (var { value: objectValue = objectDefault(), ...objectRest } of [",
        "[assignmentSourceKey()]: assignmentTargetBase()[assignmentTargetKey()]",
        "...assignmentRestBase().rest",
        "for ([abruptTarget.value = throwPatternError()] of failingOuterIterable)",
        "plain-async-sync-for-of:nonlexical-pattern-heads=ok",
    ] {
        assert!(
            PATTERN_FIXTURE.contains(marker),
            "pattern fixture: {marker}"
        );
    }
    for marker in [
        "carried = captured + 1",
        "...remaining",
        "beforeClosures.push(function ()",
        "afterClosures.push(function ()",
        "for (let [first = later, later = outerLater] of tdzOuterIterable)",
        "tdzError instanceof ReferenceError",
        "capturedHeadReader = function ()",
        "capturedHeadError instanceof ReferenceError",
        "for (const { locked } of constOuterIterable)",
        "locked = 8",
        "constWriteError instanceof TypeError",
        "[selectObjectPatternKey()]: selected, ...remaining",
        "same(objectRestCloseCalls, 1",
        "for (const [value = throwPatternError()] of abruptOuterIterable)",
        "same(abruptInnerCloseCalls, 1",
        "same(abruptOuterCloseCalls, 1",
        "for (const [] of [emptyArrayInnerIterable])",
        "for (const {} of emptyObjectOuterIterable)",
        "plain-async-sync-for-of:lexical-pattern-heads=ok",
    ] {
        assert!(
            LEXICAL_PATTERN_FIXTURE.contains(marker),
            "lexical pattern fixture: {marker}"
        );
    }
    for marker in [
        "closures.push(() => v)",
        "asyncValues = closures.map((f) => f())",
        "asyncValues.join(\",\") !== \"1,2,3,4,5,6\"",
    ] {
        assert!(
            CAPTURE_FIXTURE.contains(marker),
            "capture fixture: {marker}"
        );
    }

    for fixture in [
        "wasm_plain_async_sync_for_of_iterator_protocol.js",
        "wasm_plain_async_sync_for_of_iterator_close.js",
        "wasm_plain_async_sync_for_of_iterator_errors.js",
        "wasm_plain_async_sync_for_of_member_heads.js",
        "wasm_plain_async_sync_for_of_nonlexical_pattern_heads.js",
        "wasm_plain_async_sync_for_of_lexical_pattern_heads.js",
    ] {
        assert!(ITERATOR_CLI_TESTS.contains(fixture), "CLI test: {fixture}");
    }
    assert!(FUNCTION_CLI_TESTS.contains("wasm_async_for_of_closure_capture.js"));
    assert!(CONTRACT.contains("AsyncFunctionForOfIteratorPlanIr"));
    assert!(MEMBER_CONTRACT.contains("member-reference heads"));
    assert!(MEMBER_CONTRACT.contains("wasm_plain_async_sync_for_of_member_heads.js"));
    assert!(PATTERN_CONTRACT.contains("assignment patterns and `var` binding patterns"));
    assert!(PATTERN_CONTRACT.contains("wasm_plain_async_sync_for_of_nonlexical_pattern_heads.js"));
    assert!(LEXICAL_PATTERN_CONTRACT.contains("public storage enum has exactly those three cases"));
    assert!(LEXICAL_PATTERN_CONTRACT.contains("wasm_plain_async_sync_for_of_lexical_pattern_heads.js"));
}

#[test]
fn backend_exhaustively_uses_each_resumable_value_storage_lifetime() {
    let emitter = bounded(
        RESUMABLE_SYNC_FOR_OF_ITERATOR_SOURCE,
        "    pub(crate) fn compile_resumable_sync_for_of_iterator(",
        "\n    }\n}",
    );
    let allocation = bounded(
        emitter,
        "        let entry_local = match plan.value_storage() {",
        "        let iterator_storage = self.allocate_binding(",
    );
    for variant in [
        "ResumableSyncForOfValueStorage::Activation(binding)",
        "ResumableSyncForOfValueStorage::IterationEnvironment(binding)",
        "ResumableSyncForOfValueStorage::EntryLocal(name)",
    ] {
        assert_eq!(
            allocation.matches(variant).count(),
            1,
            "allocation: {variant}"
        );
    }
    assert!(!allocation.contains("_ =>"));
    assert!(allocation.contains("self.lookup_binding(&binding.name)"));
    assert!(allocation.contains("BindingStorage::EnvSlot"));
    assert!(allocation
        .contains("iteration_environment_owns_binding(plan.head_environment(), &binding.name)"));
    assert!(!allocation.contains("storage_without_iteration_environment"));
    let resolution = bounded(
        emitter,
        "        let (value_storage, temporary) = match plan.value_storage() {",
        "        self.breakable_stack.push(break_frame);",
    );
    for variant in [
        "ResumableSyncForOfValueStorage::Activation(binding)",
        "ResumableSyncForOfValueStorage::IterationEnvironment(binding)",
        "ResumableSyncForOfValueStorage::EntryLocal(_)",
    ] {
        assert_eq!(
            resolution.matches(variant).count(),
            1,
            "resolution: {variant}"
        );
    }
    assert!(!resolution.contains("_ =>"));
    positions_in_order(
        resolution,
        &[
            "ResumableSyncForOfValueStorage::Activation(binding)",
            "self.lookup_binding(&binding.name)",
            "ResumableSyncForOfValueStorage::IterationEnvironment(binding)",
            "self.lookup_current_scope_binding(&binding.name)",
            "ResumableSyncForOfValueStorage::EntryLocal(_)",
            "entry_local.expect(",
        ],
    );
}
