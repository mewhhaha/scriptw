const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const ANALYSIS_SOURCE: &str = include_str!("../../../scriptw-ir/src/analysis.rs");
const ASYNC_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/async_disposable.rs");
const ALLOCATOR_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/async_generator_source/allocator.rs");
const STATEMENT_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/async_generator_source/statement.rs");
const IR_TEST_SOURCE: &str = include_str!("../../../scriptw-ir/src/tests/resource_disposal.rs");
const CONTROL_FLOW_SOURCE: &str = include_str!("../../src/control_flow.rs");
const HEAP_SOURCE: &str = include_str!("../../src/heap.rs");
const EMIT_SOURCE: &str = include_str!("../../src/emit.rs");
const ADMISSION_SOURCE: &str = include_str!("../../src/emit/async_generator_admission.rs");
const FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_await_using_async_generator_lifecycle.js"
);
const CLI_TEST_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/resource_management.rs");
const TEST262_RUNNER_SOURCE: &str = include_str!("../../../scriptw-test262/src/lib.rs");
const KNOWN_FAILURES: &str = include_str!("../../../scriptw-cli/tests/known-failures.tsv");
const CONTRACT: &str =
    include_str!("../../../../docs/rust-rewrite/contracts/async-generator-await-using-scope.md");

const EXACT_FILES: [(&str, &str); 2] = [
    (
        "language/statements/await-using/initializer-Symbol.asyncDispose-called-at-end-of-asyncgeneratorbody.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/statements/await-using/initializer-Symbol.asyncDispose-called-at-end-of-asyncgeneratorbody.js"
        ),
    ),
    (
        "language/statements/await-using/initializer-Symbol.dispose-called-at-end-of-asyncgeneratorbody.js",
        include_str!(
            "../../../../test262/vendor/test262/test/language/statements/await-using/initializer-Symbol.dispose-called-at-end-of-asyncgeneratorbody.js"
        ),
    ),
];

fn bounded<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
    let start = source.find(start).expect("start marker");
    let tail = &source[start..];
    let end = tail.find(end).expect("end marker");
    &tail[..end]
}

fn positions_in_order(source: &str, markers: &[&str]) {
    let mut cursor = 0;
    for marker in markers {
        let offset = source[cursor..].find(marker).expect(marker);
        cursor += offset + marker.len();
    }
}

#[test]
fn ir_makes_the_async_dispose_execution_owner_closed_and_distinct() {
    let statement = bounded(
        IR_SOURCE,
        "AsyncDisposableScope {",
        "ParameterInitialization {",
    );
    assert!(statement.contains("execution: AsyncDisposableScopeExecutionIr"));
    assert!(statement.contains("resources: AsyncDisposableResourcesIr"));
    assert!(statement.contains("body: BlockIr"));
    assert!(!statement.contains("Option<AsyncDisposableScopeExecutionIr>"));

    let execution = bounded(
        IR_SOURCE,
        "pub enum AsyncDisposableScopeExecutionIr {",
        "/// The activation-backed capability for one async-generator",
    );
    assert!(execution.contains("AsyncFunction(AsyncFunctionAsyncDisposableCapabilityIr)"));
    assert!(execution.contains("AsyncGenerator(AsyncGeneratorAsyncDisposableCapabilityIr)"));
    assert!(!execution.contains("_ =>"));
    assert!(!execution.contains("Default"));

    assert!(IR_SOURCE.contains(
        "#[must_use = \"an async-generator async DisposeCapability must be attached to its scope\"]\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct AsyncGeneratorAsyncDisposableCapabilityIr"
    ));
    assert!(IR_SOURCE.contains("pub(crate) const IMPLICIT_STATE_COUNT: u32 = 3;"));
    let capability = bounded(
        IR_SOURCE,
        "pub struct AsyncGeneratorAsyncDisposableCapabilityIr {",
        "impl AsyncFunctionAsyncDisposableCapabilityIr",
    );
    assert!(capability.contains("binding_name: String"));
    assert!(capability.contains("finalizer: AsyncDisposableFinalizerPlanIr"));
    assert!(capability.contains("pub(crate) fn new("));
    assert!(capability.contains("pub fn binding_name(&self) -> &str"));
    assert!(capability.contains("pub fn finalizer(&self) -> &AsyncDisposableFinalizerPlanIr"));
    assert!(!capability.contains("pub binding_name"));
    assert!(!capability.contains("Copy"));
}

#[test]
fn lowering_mints_the_async_generator_owner_before_initializers_and_finalizes_once() {
    let owner = bounded(
        ANALYSIS_SOURCE,
        "pub(crate) enum AsyncDisposableScopeOwnerPlan {",
        "#[derive(Debug, Clone)]",
    );
    for variant in ["Ordinary", "Generator", "AsyncFunction", "AsyncGenerator"] {
        assert!(owner.contains(variant));
    }
    for mapping in [
        "FunctionExecutionKind::Ordinary => AsyncDisposableScopeOwnerPlan::Ordinary",
        "FunctionExecutionKind::Generator => AsyncDisposableScopeOwnerPlan::Generator",
        "FunctionExecutionKind::Async => AsyncDisposableScopeOwnerPlan::AsyncFunction",
        "FunctionExecutionKind::AsyncGenerator => AsyncDisposableScopeOwnerPlan::AsyncGenerator",
    ] {
        assert!(owner.contains(mapping));
    }

    assert!(ASYNC_LOWERING_SOURCE.contains(
        "#[must_use = \"an async-dispose execution owner must be finalized into public IR\"]\nenum PendingAsyncDisposableScopeExecutionIr"
    ));
    let pending = bounded(
        ASYNC_LOWERING_SOURCE,
        "enum PendingAsyncDisposableScopeExecutionIr {",
        "impl ScriptLowerer<'_>",
    );
    for variant in ["AsyncFunction {", "AsyncGenerator {"] {
        assert!(pending.contains(variant));
    }
    assert!(pending.contains("fn finalize("));
    assert!(pending.contains("AsyncDisposableScopeExecutionIr::AsyncFunction("));
    assert!(pending.contains("AsyncDisposableScopeExecutionIr::AsyncGenerator("));
    assert!(!pending.contains("_ =>"));
    assert!(!pending.contains("Copy"));

    let lower = bounded(
        ASYNC_LOWERING_SOURCE,
        "pub(super) fn lower_await_using_declaration(",
        "fn async_disposable_scope_owner(&self)",
    );
    positions_in_order(
        lower,
        &[
            "suspension inside an await using initializer",
            "let owner = self.async_disposable_scope_owner()",
            "AsyncDisposableScopeOwnerPlan::AsyncFunction\n            | AsyncDisposableScopeOwnerPlan::AsyncGenerator => {}",
            "let entry_state = self",
            "let execution = match owner",
            "AsyncDisposableScopeOwnerPlan::AsyncGenerator =>",
            "alloc_suspension_owned_binding(",
            "async.generator.await.dispose.capability.",
            "let init = self.lower_expression(initializer)",
            "into_async_disposable_resource(self)",
        ],
    );
    assert!(lower.contains("PendingAsyncDisposableScopeExecutionIr::AsyncGenerator"));
    assert!(lower.contains("AsyncDisposableResourcesIr::new(first, resources.collect())"));

    let finish = bounded(
        ASYNC_LOWERING_SOURCE,
        "pub(super) fn finish_disposable_scopes(",
        "pub(super) fn lower_await_using_declaration(",
    );
    positions_in_order(
        finish,
        &[
            "for (mut prefix, scope) in segments.into_iter().rev()",
            "LoweredDisposableScopeIr::Async(scope) =>",
            "let finalizer =",
            "self.allocate_async_disposable_finalizer(scope.execution.entry_state())",
            "scope.execution.finalize(finalizer)",
        ],
    );
    let allocate_finalizer = bounded(
        ASYNC_LOWERING_SOURCE,
        "fn allocate_async_disposable_finalizer(",
        "/// Lowers the resource side of an admitted classic-for `await using` head.",
    );
    positions_in_order(
        allocate_finalizer,
        &[
            "let suffix_end = self",
            "AsyncDisposableFinalizerPlanIr::after_source_suffix(entry_state, suffix_end)",
            "self.current_async_resume_state = Some(finalizer.exit_state())",
            "self.current_generator_resume_state = Some(finalizer.exit_state())",
        ],
    );
    assert!(IR_TEST_SOURCE
        .contains("fn async_generator_await_using_owns_distinct_closed_finalizer_states()"));
    assert!(IR_TEST_SOURCE.contains(
        "fn async_generator_await_using_reserves_nested_finalizer_before_following_yield()"
    ));
    positions_in_order(
        IR_TEST_SOURCE,
        &[
            "fn async_generator_await_using_reserves_nested_finalizer_before_following_yield()",
            "suspend_state: 0",
            "resume_state: 1",
            "suspend_state: 4",
            "resume_state: 5",
            "assert_eq!(inner.finalizer().entry_state(), 1)",
            "assert_eq!(inner.finalizer().dispose_state(), 2)",
            "assert_eq!(inner.finalizer().resume_state(), 3)",
            "assert_eq!(inner.finalizer().exit_state(), 4)",
        ],
    );
    assert!(IR_TEST_SOURCE.contains(".filter(|binding| binding.name == capability.binding_name())"));
    assert!(IR_TEST_SOURCE
        .contains("each async-generator async-dispose capability must own one activation slot"));

    positions_in_order(
        ALLOCATOR_SOURCE,
        &[
            "fn reserve_async_disposable_finalizer(",
            "self.reserve_resource_finalizer(self.current_state)",
            "self.finish_resource_finalizer(entry, self.current_state)",
            "AsyncDisposableFinalizerPlanIr::after_source_suffix(entry, end)",
            "self.enclosing_scope_resume_states.push(plan.resume_state())",
            "self.current_state = plan.exit_state()",
        ],
    );
    let collector = bounded(
        STATEMENT_SOURCE,
        "pub(super) fn append_items(",
        "pub(super) fn append_item(",
    );
    positions_in_order(
        collector,
        &[
            "let mut async_disposable_scope_count = 0",
            "visitor.visit_statement_list_item(item)",
            "0..async_disposable_scope_count",
            "states.reserve_async_disposable_finalizer()?",
        ],
    );
}

#[test]
fn backend_owner_selects_layout_and_scope_compilation_exhaustively() {
    assert!(CONTROL_FLOW_SOURCE.contains(
        "#[must_use = \"an async DisposeCapability owner must reach its consuming finalizer\"]\nenum ActivationAsyncDisposeOwner<'a>"
    ));
    let owner = bounded(
        CONTROL_FLOW_SOURCE,
        "enum ActivationAsyncDisposeOwner<'a> {",
        "/// The resumable execution owners that share",
    );
    for marker in [
        "AsyncFunction(&'a AsyncFunctionAsyncDisposableCapabilityIr)",
        "AsyncFunctionForOf(&'a AsyncFunctionAsyncDisposableForOfCapabilityIr)",
        "AsyncCompleteIterator(&'a AsyncGeneratorAsyncDisposableCapabilityIr)",
        "AsyncGenerator(&'a AsyncGeneratorAsyncDisposableCapabilityIr)",
        "fn from_execution(execution: &'a AsyncDisposableScopeExecutionIr)",
        "AsyncDisposableScopeExecutionIr::AsyncFunction(capability)",
        "AsyncDisposableScopeExecutionIr::AsyncGenerator(capability)",
        "Self::AsyncFunction(capability) => capability.binding_name()",
        "Self::AsyncFunctionForOf(capability) => capability.binding_name()",
        "Self::AsyncCompleteIterator(capability) => capability.binding_name()",
        "Self::AsyncGenerator(capability) => capability.binding_name()",
        "Self::AsyncFunction(capability) => capability.finalizer()",
        "Self::AsyncFunctionForOf(capability) => capability.finalizer()",
        "Self::AsyncCompleteIterator(capability) => capability.finalizer()",
        "Self::AsyncGenerator(capability) => capability.finalizer()",
        "Self::AsyncFunction(_)\n            | Self::AsyncFunctionForOf(_)\n            | Self::AsyncCompleteIterator(_) => FunctionExecutionKind::Async",
        "Self::AsyncGenerator(_) => FunctionExecutionKind::AsyncGenerator",
    ] {
        assert!(owner.contains(marker), "{marker}");
    }
    assert!(!owner.contains("Clone"));
    assert!(!owner.contains("Copy"));
    assert!(!owner.contains("_ =>"));

    let compile = bounded(
        CONTROL_FLOW_SOURCE,
        "fn compile_async_disposable_scope(",
        "fn initialize_async_disposable_resource_bindings(",
    );
    positions_in_order(
        compile,
        &[
            "ActivationAsyncDisposeOwner::from_execution(execution)",
            "meta.protocol().execution_kind() == owner.execution_kind()",
            "activation_owned_binding_storage(owner.binding_name())",
            "let finalizer = owner.finalizer()",
            "finalizer.entry_state()",
            "finalizer.exit_state()",
            "initialize_activation_async_dispose_capability",
            "begin_async_dispose_pending_completion",
            "begin_activation_async_dispose_capability",
            "consume_activation_async_dispose_capability(\n            &owner",
        ],
    );
    assert!(compile.contains("async DisposeCapability has the wrong execution owner"));
    assert!(compile.contains("async DisposeCapability is missing its activation-owned binding"));
    assert!(!compile.contains("allocate_binding(owner.binding_name"));
}

#[test]
fn backend_walker_awaits_and_dispatches_through_the_selected_owner() {
    let suspension = bounded(
        EMIT_SOURCE,
        "fn async_generator_contains_suspension(",
        "fn async_generator_dispatcher_unsupported_feature(",
    );
    positions_in_order(
        suspension,
        &[
            "AsyncDisposableScopeExecutionIr::AsyncGenerator(_)",
            "AsyncGeneratorSuspension::Await => true",
            "AsyncGeneratorSuspension::Yield => body",
            "async_generator_contains_suspension(statement, suspension)",
            "AsyncDisposableScopeExecutionIr::AsyncFunction(_)",
            "=> false",
        ],
    );
    let preflight = ADMISSION_SOURCE;
    positions_in_order(
        preflight,
        &[
            "AsyncDisposableScopeExecutionIr::AsyncGenerator(_)",
            "find_map(visit)",
            "AsyncDisposableScopeExecutionIr::AsyncFunction(_)",
            "await using scope with a mismatched execution owner",
        ],
    );

    let state = bounded(
        HEAP_SOURCE,
        "pub(crate) enum ActivationAsyncDisposeCapabilityState",
        "pub(crate) enum ActivationAsyncDisposeEntryKind",
    );
    for variant in ["Pending", "Disposing", "Disposed"] {
        assert!(state.contains(variant));
    }
}

#[test]
fn exact_inventory_and_durable_fixture_bound_the_claim() {
    for (path, source) in EXACT_FILES {
        assert!(source.contains("features: [explicit-resource-management]"));
        assert!(source.contains("flags: [async]"));
        assert!(!TEST262_RUNNER_SOURCE.contains(path));
        assert!(!KNOWN_FAILURES.contains(path));
        assert!(CONTRACT.contains(path));
        assert!(CONTRACT.contains(path));
    }

    for marker in [
        "await using direct = normalDirect.value",
        "await using fallback = normalFallback.value",
        "normal while yielded",
        "normal while awaiting",
        "external return disposal",
        "external throw disposal",
        "rejected await disposal",
        "acquisition failure disposal",
        "nested inner disposal",
        "suppressed disposal LIFO",
        "disposal before settlement and drain",
        "reentrant disposal and drain",
        "fallback thenable ignored",
        "await-using-async-generator:true",
    ] {
        assert!(FIXTURE.contains(marker), "{marker}");
    }
    assert!(FIXTURE.matches("await using ").count() >= 10);
    assert!(!FIXTURE.contains("for (await using"));
    assert!(!FIXTURE.contains("for await ("));
    assert!(!FIXTURE.contains("eval("));
    assert!(CLI_TEST_SOURCE.contains("fn wasm_await_using_async_generator_lifecycle()"));
    assert!(CLI_TEST_SOURCE.contains("wasm_await_using_async_generator_lifecycle.js"));

    let reentrant = bounded(
        FIXTURE,
        "sameTrace(\n    reentrantTrace,",
        "same(reentrantResource.count(), 1, \"reentrant once\")",
    );
    positions_in_order(
        reentrant,
        &[
            "reentrant:dispose",
            "reentrant:enqueue",
            "reentrant:current-settled",
            "reentrant:queued-settled",
        ],
    );
}
