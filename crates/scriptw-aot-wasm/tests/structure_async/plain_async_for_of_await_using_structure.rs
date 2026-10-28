const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const FOR_OF_LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering/for_of.rs");
const ASYNC_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/async_disposable.rs");
const IR_TEST_SOURCE: &str = include_str!("../../../scriptw-ir/src/tests/resource_disposal.rs");
const CONTROL_FLOW_SOURCE: &str = include_str!("../../src/control_flow.rs");
const DATA_SOURCE: &str = include_str!("../../src/data.rs");
const EMIT_SOURCE: &str = concat!(
    include_str!("../../src/emit.rs"),
    include_str!("../../src/emit/async_generator_admission.rs")
);
const COMPLETE_FOR_OF_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/async_generator_for_of.rs");
const FIXTURE: &str =
    include_str!("../../../scriptw-cli/tests/fixtures/wasm_await_using_for_of_lifecycle.js");
const CLI_TEST_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/resource_management.rs");
const TEST262_RUNNER_SOURCE: &str = include_str!("../../../scriptw-test262/src/lib.rs");
const KNOWN_FAILURES: &str = include_str!("../../../scriptw-cli/tests/known-failures.tsv");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/plain-async-function-for-of-await-using.md"
);

const EXACT_FILES: [(&str, &str); 5] = [
    (
        "language/statements/await-using/initializer-Symbol.asyncDispose-called-at-end-of-each-iteration-of-forofstatement.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/statements/await-using/initializer-Symbol.asyncDispose-called-at-end-of-each-iteration-of-forofstatement.js"
        ),
    ),
    (
        "language/statements/await-using/initializer-Symbol.dispose-called-at-end-of-each-iteration-of-forofstatement.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/statements/await-using/initializer-Symbol.dispose-called-at-end-of-each-iteration-of-forofstatement.js"
        ),
    ),
    (
        "language/statements/for-of/head-await-using-bound-names-fordecl-tdz.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/statements/for-of/head-await-using-bound-names-fordecl-tdz.js"
        ),
    ),
    (
        "language/statements/await-using/syntax/await-using-invalid-assignment-statement-body-for-of.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/statements/await-using/syntax/await-using-invalid-assignment-statement-body-for-of.js"
        ),
    ),
    (
        "language/statements/await-using/syntax/await-using-valid-for-await-using-of-of.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/statements/await-using/syntax/await-using-valid-for-await-using-of-of.js"
        ),
    ),
];

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
            .unwrap_or_else(|| panic!("missing marker: {marker}"));
        cursor += offset + marker.len();
    }
}

#[test]
fn ir_head_makes_the_repeating_async_capability_and_iterator_record_indivisible() {
    assert!(IR_SOURCE.contains(
        "#[must_use = \"a plain-async for-of async DisposeCapability must be attached to its head\"]\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct AsyncFunctionAsyncDisposableForOfCapabilityIr"
    ));
    let capability = bounded(
        IR_SOURCE,
        "pub struct AsyncFunctionAsyncDisposableForOfCapabilityIr {",
        "/// One immutable async-disposable resource binding",
    );
    for marker in [
        "binding_name: String",
        "finalizer: AsyncDisposableFinalizerPlanIr",
        "pub(crate) fn new(",
        "pub fn binding_name(&self) -> &str",
        "pub fn finalizer(&self) -> &AsyncDisposableFinalizerPlanIr",
    ] {
        assert!(capability.contains(marker), "{marker}");
    }
    assert!(!capability.contains("pub binding_name"));
    assert!(!capability.contains("pub finalizer"));
    assert!(!capability.contains("Copy"));

    assert!(IR_SOURCE.contains(
        "#[must_use = \"an async-disposable for-of head must be attached to its iterator loop\"]\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct AsyncDisposableForOfHeadIr"
    ));
    let head = bounded(
        IR_SOURCE,
        "pub struct AsyncDisposableForOfHeadIr {",
        "/// The exhaustive head domain of the generic iterator protocol path.",
    );
    for marker in [
        "binding_name: String",
        "capability: AsyncFunctionAsyncDisposableForOfCapabilityIr",
        "record: IteratorRecordIr",
        "pub(crate) fn new(",
        "pub fn binding_name(&self) -> &str",
        "pub fn capability(&self) -> &AsyncFunctionAsyncDisposableForOfCapabilityIr",
        "pub fn record(&self) -> &IteratorRecordIr",
    ] {
        assert!(head.contains(marker), "{marker}");
    }
    assert!(!head.contains("pub binding_name"));
    assert!(!head.contains("pub capability"));
    assert!(!head.contains("pub record"));
    assert!(!head.contains("Copy"));

    let heads = bounded(
        IR_SOURCE,
        "pub enum ForOfIteratorHeadIr {",
        "/// The runtime Environment Record lifecycle owned by a resumable loop.",
    );
    assert!(heads.contains("AsyncDisposable(AsyncDisposableForOfHeadIr)"));
    assert!(heads.contains("SyncDisposable(SyncDisposableForOfHeadIr)"));
    let async_head = bounded(heads, "AsyncDisposable(AsyncDisposableForOfHeadIr)", "}");
    assert!(!async_head.contains("async_plan"));
    assert!(!async_head.contains("protocol"));

    assert!(!IR_SOURCE.contains("    ForOfArray {"));
    assert!(!IR_SOURCE.contains("    ForOfString {"));
    let iterator = bounded(IR_SOURCE, "    ForOfIterator {", "    ForInArray {");
    assert!(iterator.contains("head: ForOfIteratorHeadIr"));
    assert!(!iterator.contains("async_plan:"));
    assert!(!iterator.contains("protocol:"));
}

#[test]
fn lowering_holds_the_iterator_roles_until_the_body_has_allocated_source_states() {
    assert!(ASYNC_LOWERING_SOURCE.contains(
        "#[must_use = \"a pending async-disposable for-of head must be finalized after its body\"]\npub(super) struct PendingAsyncDisposableForOfHeadIr"
    ));
    let pending = bounded(
        ASYNC_LOWERING_SOURCE,
        "pub(super) struct PendingAsyncDisposableForOfHeadIr {",
        "/// The private pre-finalizer owner proof",
    );
    for marker in [
        "entry_state: u32",
        "binding_name: String",
        "capability_binding_name: String",
        "record: IteratorRecordIr",
    ] {
        assert!(pending.contains(marker), "{marker}");
    }
    assert!(!pending.contains("Clone"));
    assert!(!pending.contains("Copy"));

    let admit = bounded(
        ASYNC_LOWERING_SOURCE,
        "pub(super) fn admit_async_disposable_for_of_head(",
        "pub(super) fn async_disposable_for_head(",
    );
    positions_in_order(
        admit,
        &[
            "if for_of.r#await()",
            "await using declaration in for-await-of",
            "contains(for_of.iterable(), ContainsSymbol::AwaitExpression)",
            "contains(for_of.iterable(), ContainsSymbol::YieldExpression)",
            "contains(for_of.body(), ContainsSymbol::AwaitExpression)",
            "contains(for_of.body(), ContainsSymbol::YieldExpression)",
            "source suspension in await using for-of loop",
            "let Binding::Identifier(identifier) = binding",
            "await using declaration binding pattern in for-of",
        ],
    );

    let begin = bounded(
        ASYNC_LOWERING_SOURCE,
        "pub(super) fn begin_async_disposable_for_of_head(",
        "/// Completes the repeating per-iteration finalizer",
    );
    positions_in_order(
        begin,
        &[
            "match self.async_disposable_scope_owner()",
            "AsyncDisposableScopeOwnerPlan::AsyncFunction => {}",
            "AsyncDisposableScopeOwnerPlan::AsyncGenerator =>",
            "AsyncDisposableScopeOwnerPlan::Generator =>",
            "AsyncDisposableScopeOwnerPlan::Ordinary =>",
            "let entry_state = self",
            "alloc_suspension_owned_binding(",
            "async.function.forof.await.dispose.capability.",
            "let record = IteratorRecordIr::new(",
            "self.alloc_iterator_slot()",
            "self.alloc_next_method_slot()",
            "self.alloc_done_slot()",
            "Some(PendingAsyncDisposableForOfHeadIr",
        ],
    );

    let finish = bounded(
        ASYNC_LOWERING_SOURCE,
        "pub(super) fn finish_async_disposable_for_of_head(",
        "/// Lowers one admitted async-owner `await using` declaration",
    );
    positions_in_order(
        finish,
        &[
            "let finalizer = self.allocate_async_disposable_finalizer(pending.entry_state)",
            "AsyncDisposableForOfHeadIr::new(",
            "pending.binding_name",
            "AsyncFunctionAsyncDisposableForOfCapabilityIr::new(",
            "pending.capability_binding_name",
            "finalizer",
            "pending.record",
        ],
    );

    let exhaustive_begin = bounded(
        ASYNC_LOWERING_SOURCE,
        "pub(super) fn begin_async_disposable_for_of_if_needed(",
        "/// Completes the repeating per-iteration finalizer",
    );
    assert!(exhaustive_begin
        .contains("LoweredForOfHeadKind::Assignment | LoweredForOfHeadKind::SyncDisposable"));
    assert!(exhaustive_begin.contains("LoweredForOfHeadKind::AsyncDisposable => self"));
    assert!(exhaustive_begin.contains("begin_async_disposable_for_of_head"));
    assert!(!exhaustive_begin.contains("_ =>"));

    let statement = bounded(
        ASYNC_LOWERING_SOURCE,
        "pub(super) fn async_disposable_for_of_statement(",
        "/// Lowers one admitted async-owner `await using` declaration",
    );
    positions_in_order(
        statement,
        &[
            "IteratorProtocolWitness::SYNC_ITERATOR_PROTOCOL",
            "StatementIr::ForOfIterator",
            "ForOfIteratorHeadIr::AsyncDisposable(head)",
        ],
    );
    assert!(!statement.contains("ASYNC_ITERATOR_PROTOCOL"));

    let lower = bounded(
        FOR_OF_LOWERING_SOURCE,
        "fn lower_for_of_head(&mut self, for_of: &ForOfLoop) -> ForOfLoweringIr {",
        "        ForOfLoweringIr::new(statement, body_kind, protocol)",
    );
    positions_in_order(
        lower,
        &[
            "IterableLoopInitializer::AwaitUsing(binding)",
            "self.admit_async_disposable_for_of_head(for_of, binding)",
            "LoweredForOfHeadKind::AsyncDisposable",
            "self.lower_for_head_expression_with_tdz(mode, &name, for_of.iterable())",
            "self.begin_async_disposable_for_of_if_needed(head_kind, &storage_name)",
            "self.declare_binding(",
            "let (mut body, body_kind) = self.lower_loop_body(for_of.body())",
            "self.finish_async_disposable_for_of_head(pending)",
            "LoweredForOfHeadKind::AsyncDisposable => Self::async_disposable_for_of_statement(",
        ],
    );
    assert_eq!(
        lower
            .matches("&& head_kind == LoweredForOfHeadKind::Assignment")
            .count(),
        0,
        "ordinary for-of heads must not select an iterator-protocol bypass"
    );

    assert!(IR_TEST_SOURCE
        .contains("fn plain_async_for_of_await_using_owns_repeating_iteration_capability()"));
    let ir_test = bounded(
        IR_TEST_SOURCE,
        "fn plain_async_for_of_await_using_owns_repeating_iteration_capability()",
        "fn synchronous_using_for_of_is_a_closed_generic_iterator_head()",
    );
    for marker in [
        "StatementIr::AsyncGeneratorForOf(plan)",
        "plan.execution(), ResumableRegionProtocolIr::Async",
        "plan.resource().expect(\"per-iteration disposal capability\")",
        "StatementIr::AsyncGeneratorResourceRegistration(head)",
        "head.capability_binding(), resource.capability_binding()",
        "plan.lexical_environment().expect(\"original per-key environment\")",
        "tdz_binding_names",
        "ExprIr::RuntimeThrow",
        "name: NativeErrorKind::ReferenceError",
        "iteration_environment",
        "capture.mode, BindingMode::Const",
        "name: NativeErrorKind::TypeError",
        "capability.binding_name()",
        "plan.head_binding().name.as_str()",
        "plan.incoming_binding().name.as_str()",
        "plan.value_binding().name.as_str()",
        "let owned_names = [",
        "owned_names.iter().copied().collect::<BTreeSet<_>>().len(),",
        "every activation-backed role must own exactly one slot",
        "finalizer.entry_state() < finalizer.dispose_state()",
        "finalizer.dispose_state() < finalizer.resume_state()",
        "finalizer.resume_state() < finalizer.exit_state()",
        "assert!(capability.finalizer().dispose_state() > plan.body().end_state())",
    ] {
        assert!(ir_test.contains(marker), "{marker}");
    }
    assert!(
        FOR_OF_LOWERING_SOURCE.contains("return self.lower_async_generator_for_of(for_of, owner)")
    );
    positions_in_order(
        COMPLETE_FOR_OF_LOWERING_SOURCE,
        &[
            "owner.checked_source(source)",
            "self.lower_complete_resumable_for_of(source)",
            "let states = source.states(entry)?",
            "self.lower_complete_for_of_operand(execution, ast.iterable())",
            "let incoming_binding = owned_pattern_binding(",
            "self.lower_checked_async_generator_for_of_initializer(",
            "let (body, kind) = self.lower_loop_body(ast.body())",
            "let plan = AsyncGeneratorForOfIr::new(",
            "Some((StatementIr::AsyncGeneratorForOf(Box::new(plan)), kind))",
        ],
    );
}

#[test]
fn exact_inventory_is_raw_and_the_consumer_oracle_pins_the_lifecycle_boundary() {
    for (path, source) in EXACT_FILES {
        assert!(source.contains("features: [explicit-resource-management]"));
        assert!(source.contains("await using"));
        assert!(!TEST262_RUNNER_SOURCE.contains(path), "rewritten: {path}");
        assert!(!KNOWN_FAILURES.contains(path), "known failure: {path}");
        assert!(CONTRACT.contains(path), "contract inventory: {path}");
        assert!(CONTRACT.contains(path), "task inventory: {path}");
    }

    for marker in [
        "for (await using shadowed of [shadowed])",
        "for (await using of of [])",
        "acquire-tdz:ReferenceError",
        "binding initialized after acquisition",
        "genericIterable(",
        "normal disposal before next",
        "first fresh captured binding",
        "sync fallback return ignored",
        "continue disposes before next without close",
        "break disposal before close",
        "return disposal before close",
        "throw disposal before close",
        "current = null",
        "immutable assignment disposal before close",
        "later acquisition failure closes after prior disposal",
        "outer head binding after nested finalizer",
        "outer head binding survives nested finalizer",
        "nested LIFO suppression before close",
        "body suppression identity",
        "await-using-for-of:true",
    ] {
        assert!(FIXTURE.contains(marker), "{marker}");
    }
    assert!(!FIXTURE.contains("for await ("));
    assert!(!FIXTURE.contains("eval("));
    assert!(CLI_TEST_SOURCE.contains("fn wasm_await_using_for_of_lifecycle()"));
    assert!(CLI_TEST_SOURCE.contains("wasm_await_using_for_of_lifecycle.js"));
}

#[test]
fn backend_typestate_and_exhaustive_consumers_make_the_async_head_compile_visible() {
    let continuation = bounded(
        CONTROL_FLOW_SOURCE,
        "#[must_use = \"an async DisposeCapability continuation must be consumed by its finalizer\"]",
        "/// The two activation layouts that can own an asynchronous DisposeCapability.",
    );
    for marker in [
        "enum ActivationAsyncDisposeCompletionContinuation",
        "Scope",
        "ClassicFor",
        "enum AsyncDisposableForOfIterationEnvironment",
        "Absent",
        "Active",
        "struct AsyncDisposableForOfCompletionContinuationLocals",
        "iteration_environment: AsyncDisposableForOfIterationEnvironment",
        "continue_target: ControlTarget",
        "loop_target: ControlTarget",
    ] {
        assert!(continuation.contains(marker), "{marker}");
    }
    assert!(!continuation.contains("derive(Clone"));
    assert!(!continuation.contains("derive(Copy"));

    let owner = bounded(
        CONTROL_FLOW_SOURCE,
        "#[must_use = \"an async DisposeCapability owner must reach its consuming finalizer\"]",
        "/// The resumable execution owners that share the activation-backed",
    );
    assert!(owner.contains("AsyncFunctionForOf(&'a AsyncFunctionAsyncDisposableForOfCapabilityIr)"));
    for helper in [
        "fn binding_name(&self)",
        "fn finalizer(&self)",
        "const fn execution_kind(&self)",
    ] {
        let helper = bounded(owner, helper, "    }");
        assert!(helper.contains("Self::AsyncFunctionForOf"), "{helper}");
        assert!(!helper.contains("_ =>"));
    }
    assert!(!owner.contains("derive(Clone"));
    assert!(!owner.contains("derive(Copy"));

    let entry = bounded(
        CONTROL_FLOW_SOURCE,
        "fn async_statement_entry_state(statement: &StatementIr)",
        "fn async_statement_exit_state(statement: &StatementIr)",
    );
    assert!(entry.contains("head: ForOfIteratorHeadIr::AsyncDisposable(head)"));
    assert!(entry.contains("head.capability().finalizer().entry_state()"));
    let exit = bounded(
        CONTROL_FLOW_SOURCE,
        "fn async_statement_exit_state(statement: &StatementIr)",
        "fn labelled_async_disposable_for_finalizer(",
    );
    assert!(exit.contains("head: ForOfIteratorHeadIr::AsyncDisposable(head)"));
    assert!(exit.contains("head.capability().finalizer().exit_state()"));
    let labelled = bounded(
        CONTROL_FLOW_SOURCE,
        "fn labelled_async_disposable_for_finalizer(",
        "fn generator_statement_entry_state(",
    );
    assert!(labelled.contains("StatementIr::Labelled { statement, .. }"));
    assert!(labelled.contains("Self::labelled_async_disposable_for_finalizer(statement)"));
    assert!(labelled.contains("head: ForOfIteratorHeadIr::AsyncDisposable(head)"));
    assert!(labelled.contains("Some(head.capability().finalizer())"));
    assert!(labelled.contains("_ => None"));

    for (start, end) in [
        (
            "pub(crate) fn compile_statement(",
            "pub(crate) fn compile_labelled_statement(",
        ),
        (
            "pub(crate) fn compile_labelled_statement(",
            "pub(crate) fn compile_try_catch(",
        ),
    ] {
        let dispatch = bounded(CONTROL_FLOW_SOURCE, start, end);
        assert!(dispatch.contains("ForOfIteratorHeadIr::AsyncDisposable(head)"));
        assert!(dispatch.contains("self.compile_async_disposable_for_of_iterator("));
    }

    let data = bounded(
        DATA_SOURCE,
        "StatementIr::ForOfIterator {",
        "StatementIr::Switch {",
    );
    assert!(data.contains("ForOfIteratorHeadIr::AsyncDisposable(head)"));
    assert!(data.contains("\"Symbol.asyncDispose\""));
    assert!(data.contains("\"Symbol.dispose\""));
    assert!(data.contains("self.intern_string(head.binding_name())"));
    assert_eq!(
        EMIT_SOURCE
            .matches("head: ForOfIteratorHeadIr::AsyncDisposable(_)")
            .count(),
        2,
        "async-generator suspension and admission walkers must classify the head explicitly"
    );
    assert!(EMIT_SOURCE.contains("await using for-of requires a plain async function"));
    assert!(
        CONTRACT.contains("All closed head and owner domains are handled with exhaustive matches")
    );
}

#[test]
fn backend_disposes_and_awaits_before_choosing_next_or_iterator_close() {
    let compile = bounded(
        CONTROL_FLOW_SOURCE,
        "pub(crate) fn compile_async_disposable_for_of_iterator(",
        "pub(crate) fn compile_for_of_iterator(",
    );
    for marker in [
        "await using for-of head requires a plain async function",
        "ActivationAsyncDisposeOwner::AsyncFunctionForOf(head.capability())",
        "await using for-of Iterator lacks its activation-owned cell",
        "await using for-of NextMethod lacks its activation-owned cell",
        "await using for-of Done lacks its activation-owned cell",
        "await using for-of DisposeCapability lacks its activation-owned cell",
        "AsyncDisposableForOfIterationEnvironment::Active",
        "AsyncDisposableForOfIterationEnvironment::Absent",
    ] {
        assert!(compile.contains(marker), "{marker}");
    }
    positions_in_order(
        compile,
        &[
            "self.emit_get_sync_iterator(&source, SyncIteratorConsumer::ForOf, function)",
            "self.write_binding_from_locals(iterator_storage, &iterator_value, function)",
            "self.write_binding_from_locals(next_storage, &next_method, function)",
            "self.write_binding_from_locals(done_storage, &done_value, function)",
            "let iterator = OwnedSyncIterator",
            "self.emit_sync_iterator_step_value(&iterator, done, &value, function)",
            "self.acquire_async_disposable_resource_from_locals(&acquired, function)",
            "self.append_activation_async_disposable_resource(&capability, &acquired, function)",
            "self.write_binding_from_locals(binding, &acquired.value, function)",
            "self.compile_statement(body, function)",
            "self.begin_async_dispose_pending_completion(function)",
            "self.begin_activation_async_dispose_capability(storage, finalizer, function)",
            "self.consume_activation_async_dispose_capability(",
            "ActivationAsyncDisposeCompletionContinuation::ForOf(",
            "iterator: &iterator",
            "iterator.clear(function)",
        ],
    );
}
