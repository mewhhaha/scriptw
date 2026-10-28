const IR_SOURCE: &str = include_str!("../../../scriptw-ir/src/ir.rs");
const ANALYSIS_SOURCE: &str = include_str!("../../../scriptw-ir/src/analysis.rs");
const LOWERING_SOURCE: &str = include_str!("../../../scriptw-ir/src/lowering.rs");
const ASYNC_LOWERING_SOURCE: &str =
    include_str!("../../../scriptw-ir/src/lowering/async_disposable.rs");
const IR_TEST_SOURCE: &str = include_str!("../../../scriptw-ir/src/tests/resource_disposal.rs");
const CONTROL_FLOW_SOURCE: &str = include_str!("../../src/control_flow.rs");
const HEAP_SOURCE: &str = include_str!("../../src/heap.rs");
const FIXTURE: &str = include_str!(
    "../../../scriptw-cli/tests/fixtures/wasm_await_using_plain_async_function_lifecycle.js"
);
const CLI_TEST_SOURCE: &str = include_str!("../../../scriptw-cli/tests/cli/resource_management.rs");
const TEST262_RUNNER_SOURCE: &str = include_str!("../../../scriptw-test262/src/lib.rs");
const KNOWN_FAILURES: &str = include_str!("../../../scriptw-cli/tests/known-failures.tsv");
const CONTRACT: &str = include_str!(
    "../../../../docs/rust-rewrite/contracts/plain-async-function-await-using-scope.md"
);

const EXACT_PATHS: [&str; 2] = [
    "language/statements/await-using/initializer-Symbol.asyncDispose-called-at-end-of-asyncfunctionbody.js",
    "language/statements/await-using/initializer-Symbol.dispose-called-at-end-of-asyncfunctionbody.js",
];

macro_rules! plain_async_statement_list_inventory {
    ($($file:literal),+ $(,)?) => {
        const PLAIN_ASYNC_STATEMENT_LIST_FILES: [(&str, &str); 49] = [$(
            (
                $file,
                include_str!(concat!(
                    "../../../../test262/vendor/test262/test/language/statements/await-using/",
                    $file
                )),
            ),
        )+];
    };
}

plain_async_statement_list_inventory!(
    "Symbol.asyncDispose-getter.js",
    "Symbol.asyncDispose-method-called-with-correct-this.js",
    "Symbol.asyncDispose-method-not-async.js",
    "Symbol.dispose-getter.js",
    "Symbol.dispose-method-called-with-correct-this.js",
    "await-using-Symbol.asyncDispose-allows-non-promise-return-value.js",
    "await-using-Symbol.asyncDispose-allows-promiselike-return-value.js",
    "await-using-allows-null-initializer.js",
    "await-using-allows-undefined-initializer.js",
    "await-using-does-not-imply-await-if-not-evaluated.js",
    "await-using-implies-await-if-evaluated.js",
    "block-local-closure-get-before-initialization.js",
    "block-local-use-before-initialization-in-declaration-statement.js",
    "block-local-use-before-initialization-in-prior-statement.js",
    "fn-name-arrow.js",
    "fn-name-class.js",
    "fn-name-cover.js",
    "fn-name-fn.js",
    "fn-name-gen.js",
    "function-local-closure-get-before-initialization.js",
    "function-local-use-before-initialization-in-declaration-statement.js",
    "function-local-use-before-initialization-in-prior-statement.js",
    "gets-initializer-Symbol.asyncDispose-property-once.js",
    "gets-initializer-Symbol.dispose-after-Symbol.asyncDispose-is-null.js",
    "gets-initializer-Symbol.dispose-after-Symbol.asyncDispose-is-undefined.js",
    "gets-initializer-Symbol.dispose-property-once.js",
    "gets-initializer-does-not-read-Symbol.dispose-if-Symbol.asyncDispose-exists.js",
    "global-closure-get-before-initialization.js",
    "global-use-before-initialization-in-declaration-statement.js",
    "global-use-before-initialization-in-prior-statement.js",
    "initializer-Symbol.asyncDispose-called-at-end-of-asyncfunctionbody.js",
    "initializer-Symbol.asyncDispose-called-at-end-of-block.js",
    "initializer-Symbol.asyncDispose-called-if-subsequent-initializer-throws.js",
    "initializer-Symbol.dispose-called-at-end-of-asyncfunctionbody.js",
    "initializer-Symbol.dispose-called-at-end-of-block.js",
    "initializer-Symbol.dispose-called-if-subsequent-initializer-throws.js",
    "multiple-resources-disposed-in-reverse-order.js",
    "puts-initializer-on-top-of-disposableresourcestack-multiple-bindings.js",
    "puts-initializer-on-top-of-disposableresourcestack-subsequent-usings.js",
    "throws-error-as-is-if-only-one-error-during-disposal.js",
    "throws-if-initializer-Symbol.asyncDispose-property-is-null.js",
    "throws-if-initializer-Symbol.asyncDispose-property-is-undefined.js",
    "throws-if-initializer-Symbol.asyncDispose-property-not-callable.js",
    "throws-if-initializer-Symbol.dispose-property-is-null.js",
    "throws-if-initializer-Symbol.dispose-property-is-undefined.js",
    "throws-if-initializer-Symbol.dispose-property-not-callable.js",
    "throws-if-initializer-missing-both-Symbol.asyncDispose-and-Symbol.dispose.js",
    "throws-if-initializer-not-object.js",
    "throws-suppressederror-if-multiple-errors-during-disposal.js",
);

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
fn ir_owns_a_nonempty_async_resource_domain_and_closed_finalizer_states() {
    let statement = bounded(
        IR_SOURCE,
        "AsyncDisposableScope {",
        "ParameterInitialization {",
    );
    assert!(statement.contains("execution: AsyncDisposableScopeExecutionIr"));
    assert!(statement.contains("resources: AsyncDisposableResourcesIr"));
    assert!(statement.contains("body: BlockIr"));

    let resource = bounded(
        IR_SOURCE,
        "pub struct AsyncDisposableResourceIr {",
        "pub struct AsyncDisposableResourcesIr {",
    );
    assert!(resource.contains("binding_name: String"));
    assert!(resource.contains("initializer: TypedExpr"));
    assert!(resource.contains("pub(crate) fn new("));
    assert!(resource.contains("pub fn binding_name(&self) -> &str"));
    assert!(resource.contains("pub fn initializer(&self) -> &TypedExpr"));
    assert!(!resource.contains("pub binding_name"));
    assert!(!resource.contains("SyncDisposableResourceIr"));

    let resources = bounded(
        IR_SOURCE,
        "pub struct AsyncDisposableResourcesIr {",
        "pub struct AsyncDisposableFinalizerPlanIr {",
    );
    assert!(resources.contains("first: AsyncDisposableResourceIr"));
    assert!(resources.contains("rest: Vec<AsyncDisposableResourceIr>"));
    assert!(resources.contains("pub(crate) fn new("));
    assert!(resources.contains("DoubleEndedIterator<Item = &AsyncDisposableResourceIr>"));
    assert!(resources.contains("pub fn is_empty(&self) -> bool {\n        false"));

    assert!(IR_SOURCE.contains(
        "#[must_use = \"an async-dispose finalizer plan must be attached to its capability\"]\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct AsyncDisposableFinalizerPlanIr"
    ));
    let finalizer = bounded(
        IR_SOURCE,
        "pub struct AsyncDisposableFinalizerPlanIr {",
        "pub struct AsyncFunctionAsyncDisposableCapabilityIr {",
    );
    for role in ["entry_state", "dispose_state", "resume_state", "exit_state"] {
        assert!(finalizer.contains(&format!("{role}: u32")));
        assert!(finalizer.contains(&format!("pub fn {role}(&self) -> u32")));
    }
    assert!(finalizer.contains(
        "entry_state < dispose_state\n                && dispose_state < resume_state\n                && resume_state < exit_state"
    ));
    assert!(!finalizer.contains("Copy"));

    assert!(IR_SOURCE.contains(
        "#[must_use = \"a plain-async-function async DisposeCapability must be attached to its scope\"]\n#[derive(Debug, Clone, PartialEq, Eq)]\npub struct AsyncFunctionAsyncDisposableCapabilityIr"
    ));
    let capability = bounded(
        IR_SOURCE,
        "pub struct AsyncFunctionAsyncDisposableCapabilityIr {",
        "impl SyncDisposableResourcesIr",
    );
    assert!(capability.contains("binding_name: String"));
    assert!(capability.contains("finalizer: AsyncDisposableFinalizerPlanIr"));
    assert!(capability.contains("pub(crate) fn new("));
    assert!(capability.contains("pub fn binding_name(&self) -> &str"));
    assert!(capability.contains("pub fn finalizer(&self) -> &AsyncDisposableFinalizerPlanIr"));
    assert!(!capability.contains("Copy"));
}

#[test]
fn lowering_selects_the_plain_async_owner_before_minting_one_finalizer() {
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
            "match owner",
            "AsyncDisposableScopeOwnerPlan::AsyncFunction\n            | AsyncDisposableScopeOwnerPlan::AsyncGenerator => {}",
            "let entry_state = self",
            "let execution = match owner",
            "AsyncDisposableScopeOwnerPlan::AsyncFunction =>",
            "alloc_suspension_owned_binding(",
            "async.function.async.dispose.capability.",
            "let init = self.lower_expression(initializer)",
            "into_async_disposable_resource(self)",
            "AsyncDisposableResourcesIr::new(first, resources.collect())",
        ],
    );
    assert!(lower.contains("AsyncDisposableScopeOwnerPlan::Ordinary =>"));
    assert!(lower.contains("AsyncDisposableScopeOwnerPlan::Generator =>"));
    assert!(lower.contains("PendingAsyncDisposableScopeExecutionIr::AsyncGenerator"));
    assert!(!lower.contains("SyncDisposableResourcesIr::new"));

    let finish = bounded(
        ASYNC_LOWERING_SOURCE,
        "pub(super) fn finish_disposable_scopes(",
        "fn allocate_async_disposable_finalizer(",
    );
    positions_in_order(
        finish,
        &[
            "for (mut prefix, scope) in segments.into_iter().rev()",
            "LoweredDisposableScopeIr::Async(scope) =>",
            "self.allocate_async_disposable_finalizer(scope.execution.entry_state())",
            "StatementIr::AsyncDisposableScope",
            "execution: scope.execution.finalize(finalizer)",
        ],
    );
    assert_eq!(
        finish
            .matches("allocate_async_disposable_finalizer(")
            .count(),
        1
    );
    let allocate_finalizer = bounded(
        ASYNC_LOWERING_SOURCE,
        "fn allocate_async_disposable_finalizer(",
        "pub(super) fn lower_async_disposable_for_init(",
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
    let checked_states = bounded(
        IR_SOURCE,
        "pub(crate) fn after_source_suffix(",
        "pub(crate) fn new(",
    );
    positions_in_order(
        checked_states,
        &[
            "if entry_state > suffix_end",
            "return None",
            "let dispose_state = suffix_end.checked_add(1)?",
            "let resume_state = dispose_state.checked_add(1)?",
            "let exit_state = suffix_end.checked_add(Self::IMPLICIT_STATE_COUNT)?",
            "Some(Self::new(",
        ],
    );
    assert!(IR_SOURCE.contains("pub(crate) const IMPLICIT_STATE_COUNT: u32 = 3"));
    assert_eq!(checked_states.matches("checked_add(").count(), 3);
    assert!(IR_TEST_SOURCE
        .contains("fn plain_async_function_await_using_owns_closed_finalizer_states()"));
    assert!(LOWERING_SOURCE.contains("mod async_disposable;"));
}

#[test]
fn backend_typestates_and_closed_entry_kinds_own_the_async_lifecycle() {
    for declaration in [
        "#[must_use = \"an async DisposeCapability storage proof must reach its consuming finalizer\"]\nstruct ActivationAsyncDisposeCapabilityStorage",
        "#[must_use = \"an active async DisposeCapability must be published before acquisition\"]\nstruct ActiveActivationAsyncDisposeCapabilityLocals",
        "#[must_use = \"an acquired async resource must be published or released\"]\nstruct AcquiredAsyncDisposableResourceLocals",
        "#[must_use = \"a parked async-dispose completion must be restored exactly once\"]\nstruct ActiveAsyncDisposePendingCompletion",
        "#[must_use = \"an async DisposeCapability owner must reach its consuming finalizer\"]\nenum ActivationAsyncDisposeOwner<'a>",
    ] {
        assert!(CONTROL_FLOW_SOURCE.contains(declaration), "{declaration}");
    }
    let typestates = bounded(
        CONTROL_FLOW_SOURCE,
        "struct ActivationAsyncDisposeCapabilityStorage",
        "enum ActivationSyncDisposeOwner",
    );
    assert!(!typestates.contains("Clone"));
    assert!(!typestates.contains("Copy"));

    let state = bounded(
        HEAP_SOURCE,
        "pub(crate) enum ActivationAsyncDisposeCapabilityState",
        "pub(crate) enum ActivationAsyncDisposeEntryKind",
    );
    for variant in ["Pending", "Disposing", "Disposed"] {
        assert!(state.contains(variant));
    }
    assert!(!state.contains("_ =>"));

    let kinds = bounded(
        HEAP_SOURCE,
        "pub(crate) enum ActivationAsyncDisposeEntryKind",
        "impl AsyncDisposableStackEntryKind",
    );
    for variant in ["Empty", "AsyncMethod", "SyncFallbackMethod", "SyncMethod"] {
        assert!(kinds.contains(variant));
    }
    assert!(kinds.contains("pub(crate) const ALL: [Self; 4]"));
    assert!(!kinds.contains("_ =>"));

    let compile = bounded(
        CONTROL_FLOW_SOURCE,
        "fn compile_async_disposable_scope(",
        "fn initialize_async_disposable_resource_bindings(",
    );
    assert!(compile.contains("ActivationAsyncDisposeOwner::from_execution(execution)"));
    assert!(compile.contains("meta.protocol().execution_kind() == owner.execution_kind()"));
    assert!(compile.contains("activation_owned_binding_storage(owner.binding_name())"));
    assert!(!compile.contains("allocate_binding(owner.binding_name"));
    positions_in_order(
        compile,
        &[
            "finalizer.entry_state()",
            "finalizer.exit_state()",
            "self.finally_stack.push(disposal_frame)",
            "finalizer.entry_state()",
            "finalizer.dispose_state()",
            "initialize_activation_async_dispose_capability",
            "begin_async_dispose_pending_completion",
            "begin_activation_async_dispose_capability",
            "consume_activation_async_dispose_capability",
        ],
    );
}

#[test]
fn exact_inventory_and_durable_fixture_bound_the_claim() {
    assert_eq!(PLAIN_ASYNC_STATEMENT_LIST_FILES.len(), 49);
    for (file, source) in PLAIN_ASYNC_STATEMENT_LIST_FILES {
        assert!(
            source.contains("explicit-resource-management"),
            "unexpected inventory file {file}"
        );
    }

    for path in EXACT_PATHS {
        assert!(PLAIN_ASYNC_STATEMENT_LIST_FILES
            .iter()
            .any(|(file, _)| path.ends_with(file)));
        assert!(!TEST262_RUNNER_SOURCE.contains(path));
        assert!(!KNOWN_FAILURES.contains(path));
        assert!(CONTRACT.contains(path));
        assert!(CONTRACT.contains(path));
    }

    for marker in [
        "same(fallback.thenReads(), 0, \"fallback thenable ignored\")",
        "acquisition:tdz:ReferenceError",
        "empty:after:false",
        "unreachable:after:true",
        "first waits for second",
        "body error identity",
        "disposer rejection identity",
        "outer suppressed error",
        "await-using-plain-async:true",
    ] {
        assert!(FIXTURE.contains(marker), "missing fixture marker {marker}");
    }
    assert!(CLI_TEST_SOURCE.contains("fn wasm_await_using_plain_async_function_lifecycle()"));
    assert!(CONTRACT.contains("all 49 plain-async statement-list files"));
    assert!(CONTRACT.contains("complete `await using` directory"));
}
