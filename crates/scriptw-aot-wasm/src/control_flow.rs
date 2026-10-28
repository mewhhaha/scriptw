use super::*;
use crate::emit::{async_generator_for_await_is_transparent_yield, ControlTarget};
use crate::gc_types::*;
use crate::generator_delegation::AsyncGeneratorDelegationKind;
use scriptw_ir::{
    ArrayDestructuringEvaluationIr, AsyncDisposableFinalizerPlanIr, AsyncDisposableForInitIr,
    AsyncDisposableForOfHeadIr, AsyncDisposableResourcesIr, AsyncDisposableScopeExecutionIr,
    AsyncForOfIteratorPlanIr, AsyncFunctionAsyncDisposableCapabilityIr,
    AsyncFunctionAsyncDisposableForOfCapabilityIr, AsyncFunctionForOfIteratorPlanIr,
    AsyncFunctionForOfIteratorValueStorageIr, AsyncFunctionSyncDisposableCapabilityIr,
    AsyncGeneratorAsyncDisposableCapabilityIr, AsyncGeneratorSyncDisposableCapabilityIr,
    AsyncResumeModeIr, AsyncTryPlanIr, ForOfAssignmentIr, ForOfIteratorHeadIr,
    IdentifierWriteErrorIr, IdentifierWriteReferenceIr, ObjectDestructuringPatternIr,
    PlainGeneratorSyncDisposableCapabilityIr, ResumableLoopIterationEnvironmentIr,
    SyncDisposableForOfHeadIr, SyncDisposableResourceIr, SyncDisposableResourcesIr,
    SyncDisposableScopeExecutionIr, SynchronousLoopBodyIr,
};

mod annex_b_function_copy;
mod array_destructuring;
mod array_iterator_helpers;
mod async_function_for_of_iterator;
mod async_function_if;
mod async_function_labelled;
mod async_function_switch;
mod async_function_while;
mod async_suspension;
mod constant_number_condition;
mod for_await_iteration_environment;
mod for_await_iterator_plan;
mod for_await_iterator_symbol;
mod for_in;
mod generator_for_in;
mod generator_for_of;
mod generator_resource_scope;
mod iterator_protocol;
pub(crate) use generator_resource_scope::CheckedAsyncGeneratorResourceOwner;
mod generator_loop;
pub(crate) use generator_loop::CheckedAsyncGeneratorEnvironmentOwner;
mod generator_switch;
mod generator_with;
mod object_destructuring;
mod resumable_array_destructuring;
mod resumable_sequence;
mod resumable_sync_for_of_iterator;
pub(crate) use for_in::FOR_IN_INTRINSICS;
mod repeated_identifier_reads;
mod resumed_identifier_assignment;
mod return_position;
mod scalar_number_loop;
mod statement_completion;
use for_await_iterator_plan::ForAwaitIteratorPlan;
use for_await_iterator_symbol::ForAwaitIteratorSymbol;
pub(crate) use statement_completion::GeneratorStatementListValueContext;

enum BranchCompletionKind {
    Break,
    Continue,
}

#[must_use = "a captured using-scope completion must be restored and dispatched"]
struct PendingSyncDisposeCompletionLocals {
    completion: CompletionLocals,
}

#[must_use = "an acquired using resource must be consumed by reverse disposal"]
struct AcquiredSyncDisposableResourceLocals {
    registered: I32Local,
    value: ValueLocals,
    method: ValueLocals,
}

#[must_use = "an activation-backed DisposeCapability binding must reach its consuming detach path"]
struct ActivationSyncDisposeCapabilityStorage {
    binding: BindingStorage,
}

#[must_use = "an activation-backed DisposeCapability must be published before acquisition"]
struct ActiveActivationSyncDisposeCapabilityLocals {
    record: GcLocal<ActivationSyncDisposeCapability>,
    entries: GcLocal<DisposableResourceTable>,
}

#[must_use = "a detached activation DisposeCapability must be consumed exactly once"]
struct DetachedActivationSyncDisposeCapabilityLocals {
    record: GcLocal<ActivationSyncDisposeCapability>,
    entries: GcLocal<DisposableResourceTable>,
    entry_count: I64Local,
}

#[must_use = "an async DisposeCapability storage proof must reach its consuming finalizer"]
struct ActivationAsyncDisposeCapabilityStorage {
    binding: BindingStorage,
}

#[must_use = "an active async DisposeCapability must be published before acquisition"]
struct ActiveActivationAsyncDisposeCapabilityLocals {
    record: GcLocal<ActivationAsyncDisposeCapability>,
    entries: GcLocal<ActivationAsyncDisposeResourceTable>,
}

#[must_use = "an acquired async resource must be published or released"]
struct AcquiredAsyncDisposableResourceLocals {
    kind: GcI32DomainLocal<ActivationAsyncDisposeEntryKind>,
    value: ValueLocals,
    method: ValueLocals,
}

#[must_use = "a detached async DisposeCapability must finish its parked LIFO walk"]
struct DisposingActivationAsyncDisposeCapability {
    storage: ActivationAsyncDisposeCapabilityStorage,
}

#[must_use = "a parked async-dispose completion must be restored exactly once"]
struct ActiveAsyncDisposePendingCompletion;

/// Empty-entry preludes do not count as awaiting an async method result.
enum ActivationAsyncDisposeAwaitRole {
    Method,
    EmptyPrelude,
}

/// The legal continuations after an asynchronous DisposeCapability
/// has restored its parked completion.
///
/// A scope can dispatch immediately. A classic-for head must first restore its
/// lexical environment, then dispatch abrupt completions, and route Normal to
/// the loop's break target. A for-of head must instead leave its fresh
/// iteration environment and choose between another iterator step and
/// IteratorClose. Consuming this role inside the finalizer prevents a new call
/// site from accidentally dispatching before its required environment leave.
#[must_use = "an async DisposeCapability continuation must be consumed by its finalizer"]
enum ActivationAsyncDisposeCompletionContinuation<'a> {
    Scope,
    CompleteMixedScope,
    ClassicFor {
        lexical_environment: ClassicForAsyncDisposeLexicalEnvironment,
        break_target: ControlTarget,
    },
    ForOf(AsyncDisposableForOfCompletionContinuationLocals<'a>),
}

#[must_use = "a classic-for async-disposal environment role must be consumed at finalization"]
enum ClassicForAsyncDisposeLexicalEnvironment {
    Absent,
    Active,
}

#[must_use = "a for-of async-disposal environment role must be consumed at finalization"]
enum AsyncDisposableForOfIterationEnvironment {
    Absent,
    Active,
}

/// Everything needed to choose the one legal continuation after one
/// async-disposable `for-of` iteration. Keeping the iterator-close locals and
/// loop targets in this consuming carrier prevents the shared disposal walker
/// from dispatching before the iteration environment has been left.
#[must_use = "a for-of async-disposal continuation must be consumed by its finalizer"]
struct AsyncDisposableForOfCompletionContinuationLocals<'a> {
    iteration_environment: AsyncDisposableForOfIterationEnvironment,
    state: I32Local,
    iterator: &'a OwnedSyncIterator,
    continue_target: ControlTarget,
    loop_target: ControlTarget,
}

/// The two activation layouts that can own an asynchronous DisposeCapability.
///
/// Keeping the public capability types distinct in this private consumer makes
/// every owner-dependent offset, Await continuation and completion path an
/// exhaustive decision. A future resumable owner cannot silently inherit the
/// plain-async layout.
#[must_use = "an async DisposeCapability owner must reach its consuming finalizer"]
enum ActivationAsyncDisposeOwner<'a> {
    AsyncFunction(&'a AsyncFunctionAsyncDisposableCapabilityIr),
    AsyncFunctionForOf(&'a AsyncFunctionAsyncDisposableForOfCapabilityIr),
    AsyncCompleteIterator(&'a AsyncGeneratorAsyncDisposableCapabilityIr),
    AsyncGenerator(&'a AsyncGeneratorAsyncDisposableCapabilityIr),
}

impl<'a> ActivationAsyncDisposeOwner<'a> {
    fn from_execution(execution: &'a AsyncDisposableScopeExecutionIr) -> Self {
        match execution {
            AsyncDisposableScopeExecutionIr::AsyncFunction(capability) => {
                Self::AsyncFunction(capability)
            }
            AsyncDisposableScopeExecutionIr::AsyncGenerator(capability) => {
                Self::AsyncGenerator(capability)
            }
        }
    }

    fn binding_name(&self) -> &str {
        match self {
            Self::AsyncFunction(capability) => capability.binding_name(),
            Self::AsyncFunctionForOf(capability) => capability.binding_name(),
            Self::AsyncCompleteIterator(capability) => capability.binding_name(),
            Self::AsyncGenerator(capability) => capability.binding_name(),
        }
    }

    fn finalizer(&self) -> &AsyncDisposableFinalizerPlanIr {
        match self {
            Self::AsyncFunction(capability) => capability.finalizer(),
            Self::AsyncFunctionForOf(capability) => capability.finalizer(),
            Self::AsyncCompleteIterator(capability) => capability.finalizer(),
            Self::AsyncGenerator(capability) => capability.finalizer(),
        }
    }

    const fn execution_kind(&self) -> FunctionExecutionKind {
        match self {
            Self::AsyncFunction(_)
            | Self::AsyncFunctionForOf(_)
            | Self::AsyncCompleteIterator(_) => FunctionExecutionKind::Async,
            Self::AsyncGenerator(_) => FunctionExecutionKind::AsyncGenerator,
        }
    }
}

/// The resumable execution owners that share the activation-backed
/// synchronous DisposeCapability representation.
///
/// Keeping the producer carriers in distinct variants prevents an async
/// capability from selecting generator state offsets (or the reverse) while
/// still giving their identical heap lifecycle one backend consumer.
#[must_use = "an activation-backed using owner must be consumed exhaustively"]
enum ActivationSyncDisposeOwner<'a> {
    PlainGenerator(&'a PlainGeneratorSyncDisposableCapabilityIr),
    AsyncFunction(&'a AsyncFunctionSyncDisposableCapabilityIr),
    AsyncGenerator(&'a AsyncGeneratorSyncDisposableCapabilityIr),
}

impl ActivationSyncDisposeOwner<'_> {
    fn binding_name(&self) -> &str {
        match self {
            Self::PlainGenerator(capability) => capability.binding_name(),
            Self::AsyncFunction(capability) => capability.binding_name(),
            Self::AsyncGenerator(capability) => capability.binding_name(),
        }
    }

    const fn execution_kind(&self) -> FunctionExecutionKind {
        match self {
            Self::PlainGenerator(_) => FunctionExecutionKind::Generator,
            Self::AsyncFunction(_) => FunctionExecutionKind::Async,
            Self::AsyncGenerator(_) => FunctionExecutionKind::AsyncGenerator,
        }
    }

    const fn completion_continuation(&self) -> SyncDisposeCompletionContinuation {
        match self {
            Self::PlainGenerator(_) => SyncDisposeCompletionContinuation::Dispatch,
            Self::AsyncFunction(_) => SyncDisposeCompletionContinuation::DispatchAsyncFunction,
            Self::AsyncGenerator(_) => SyncDisposeCompletionContinuation::DispatchAsyncGenerator,
        }
    }
}

#[must_use = "a synchronous iterator head must consume its iteration lifecycle"]
pub(crate) enum SyncForOfIteratorHead<'a> {
    Assignment(&'a ForOfAssignmentIr),
    SyncDisposable {
        head: &'a SyncDisposableForOfHeadIr,
        body: SynchronousLoopBodyIr<'a>,
    },
}

#[must_use = "a synchronous for-of iteration must finish assignment or disposal"]
enum SyncForOfIterationLifecycleLocals<'a> {
    Assignment(&'a ForOfAssignmentIr),
    SyncDisposable {
        head: &'a SyncDisposableForOfHeadIr,
        acquired: AcquiredSyncDisposableResourceLocals,
    },
}

#[must_use = "a sync disposal continuation must be consumed after completion restoration"]
enum SyncDisposeCompletionContinuation {
    Dispatch,
    DispatchAsyncFunction,
    DispatchAsyncGenerator,
    DeferToIteratorClose,
}

fn innermost_target(left: ControlTarget, right: ControlTarget) -> ControlTarget {
    if left.frame >= right.frame {
        left
    } else {
        right
    }
}

#[cfg(test)]
mod tests;

fn environment_hops(current_depth: u32, target_depth: u32) -> u32 {
    current_depth
        .checked_sub(target_depth)
        .expect("control target environment must enclose the current environment")
}

fn finalizer_crosses_branch(finalizer: ControlTarget, branch_target: ControlTarget) -> bool {
    finalizer.frame > branch_target.frame
}

fn iteration_environment_owns_binding(
    lexical_environment: Option<&ForInOfEnvironmentIr>,
    name: &str,
) -> bool {
    lexical_environment
        .and_then(|environment| environment.iteration_environment.as_ref())
        .is_some_and(|environment| {
            environment
                .bindings
                .iter()
                .any(|binding| binding.name == name)
        })
}

#[must_use = "a completed iterator record must be cleared or captured by its consumer"]
pub(crate) struct OwnedSyncIterator {
    record: GcLocal<IteratorRecord>,
    consumer: SyncIteratorConsumer,
}
impl OwnedSyncIterator {
    /// The helper captures this genuine direct record in its GC state. The
    /// fixed consumer keeps diagnostic policy out of the helper caller.
    pub(crate) fn from_helper_record(record: GcLocal<IteratorRecord>) -> Self {
        Self {
            record,
            consumer: SyncIteratorConsumer::IteratorHelper,
        }
    }
    pub(crate) fn record(&self) -> &GcLocal<IteratorRecord> {
        &self.record
    }
    pub(crate) fn clear(self, function: &mut Function) {
        self.record.clear(function);
    }
}

#[derive(Clone, Copy)]
pub(crate) enum SyncIteratorConsumer {
    ArrayDestructuring,
    ArrayAccumulation,
    ArrayFrom,
    ForOf,
    MathSumPrecise,
    ListFormat,
    AggregateError,
    MapConstructor,
    SetConstructor,
    WeakMapConstructor,
    WeakSetConstructor,
    ObjectFromEntries,
    MapGroupBy,
    ObjectGroupBy,
    SetLike,
    IteratorHelper,
}

enum SyncIteratorProtocolError {
    NotIterable,
    MethodResultNotObject,
    NextNotCallable,
    NextResultNotObject,
}

/// The activation layout shared by the two execution kinds that can own a
/// `for-await-of` suspension.
///
/// Ordinary async functions strictly decode their two-way resume completion.
/// Async generators retain their separate five-way resume-kind domain.
#[must_use = "a for-await activation layout must be consumed by all suspension policies"]
enum AsyncContinuationOwner {
    AsyncFunction,
    AsyncGenerator,
}

/// An identifier PutValue that needs no runtime environment lookup. An
/// environment Reference is resolved before the value is read and prepared as
/// `PreparedDestructuringTarget::EnvironmentIdentifier`, so this domain cannot
/// spell it and the write cannot meet it again.
#[must_use = "a prepared identifier write must be consumed by its write"]
enum PreparedIdentifierWrite<'a> {
    MutableBinding {
        storage_name: &'a str,
    },
    IgnoreImmutableBinding,
    Throw {
        error: IdentifierWriteErrorIr,
    },
    Global {
        referenced_name: &'a str,
        strictness: Strictness,
    },
}

#[must_use = "a prepared destructuring target must be consumed by its write"]
enum PreparedDestructuringTarget<'a> {
    Binding {
        mode: BindingMode,
        name: &'a str,
    },
    AssignmentIdentifier(PreparedIdentifierWrite<'a>),
    EnvironmentIdentifier(
        crate::environments::environment_reference::EnvironmentIdentifierReference,
    ),
    WithObjectIdentifier {
        selected: ValueLocals,
        reference: crate::environments::environment_reference::EnvironmentIdentifierReference,
        fallback_storage_name: &'a str,
    },
    Property {
        target: ValueLocals,
        key: PreparedDestructuringPropertyKey<'a>,
        strictness: Strictness,
    },
    Private {
        target: ValueLocals,
        private_name_id: PrivateNameId,
    },
    /// A SuperProperty target whose Reference `capture` already evaluated.
    Super {
        value_binding: &'a str,
        put: &'a TypedExpr,
    },
    NestedArray(&'a ArrayDestructuringPatternIr),
    NestedObject(&'a ObjectDestructuringPatternIr),
}

#[must_use = "a prepared destructuring property key must be consumed by its write"]
enum PreparedDestructuringPropertyKey<'a> {
    Static(&'a str),
    Computed(ValueLocals),
}

impl<'a> FunctionBuilder<'a> {
    fn pending_completion_frame(&self) -> Result<&GcLocal<InvocationFrame>, EmitError> {
        self.body_entry_locals()
            .and_then(|entry| entry.resume_frame())
            .ok_or_else(|| {
                EmitError::unsupported(
                    "compiler invariant: pending completion has no resumable frame",
                )
            })
    }

    fn emit_push_generator_pending_completion(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_push_pending_completion(function)
    }

    fn emit_push_async_pending_completion(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_push_pending_completion(function)
    }

    fn emit_push_pending_completion(&self, function: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let frame = self.pending_completion_frame()?;
        let previous = schema
            .reserve_gc_local::<CompletionRecord, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<InvocationFrame>()
                    .field(InvocationFrameSchema::PENDING_COMPLETION)
                    .read(frame, schema, function)
                    .reference(),
                function,
            );
        let return_stage =
            GcI32DomainLocal::new(schema, AsyncGeneratorReturnStage::Unawaited, function);
        if self.current_function_meta().is_some_and(|meta| {
            meta.protocol().execution_kind() == FunctionExecutionKind::AsyncGenerator
        }) {
            let activation = self.control_flow_async_generator_activation(function)?;
            schema
                .struct_type::<AsyncGeneratorActivation>()
                .field(AsyncGeneratorActivationSchema::RETURN_STAGE)
                .read(&activation, schema, function)
                .store_domain(&return_stage, function);
            activation.clear(function);
        }
        let record = schema
            .reserve_gc_local::<CompletionRecord, NonNullable>(function)
            .initialize(
                schema.struct_type::<CompletionRecord>().from_completion(
                    self.completion(),
                    &previous,
                    &return_stage,
                    schema,
                    function,
                ),
                function,
            );
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::PENDING_COMPLETION)
            .write(
                frame,
                GcOperand::nullable_reference(&record, schema),
                schema,
                function,
            );
        let depth = schema.reserve_i64_local(function);
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::PENDING_COMPLETION_DEPTH)
            .read(frame, schema, function)
            .store_i64(depth, function);
        depth.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        depth.store(function);
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::PENDING_COMPLETION_DEPTH)
            .write(frame, GcOperand::i64_local(depth), schema, function);
        schema.release_i64_local(depth, function);
        record.clear(function);
        return_stage.clear(schema, function);
        previous.clear(function);
        Ok(())
    }

    fn emit_pop_and_restore_generator_pending_completion(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_pop_and_restore_pending_completion(function)
    }

    fn emit_pop_and_restore_async_pending_completion(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_pop_and_restore_pending_completion(function)
    }

    fn emit_pop_and_restore_pending_completion(
        &self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_pop_pending_completion(true, function)
    }

    fn emit_pop_pending_completion(
        &self,
        restore: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let frame = self.pending_completion_frame()?;
        let record = schema
            .reserve_gc_local::<CompletionRecord, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<InvocationFrame>()
                    .field(InvocationFrameSchema::PENDING_COMPLETION)
                    .read(frame, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            );
        let next = schema
            .reserve_gc_local::<CompletionRecord, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<CompletionRecord>()
                    .field(CompletionRecordSchema::NEXT)
                    .read(&record, schema, function)
                    .reference(),
                function,
            );
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::PENDING_COMPLETION)
            .write(frame, GcOperand::reference(&next, schema), schema, function);
        let depth = schema.reserve_i64_local(function);
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::PENDING_COMPLETION_DEPTH)
            .read(frame, schema, function)
            .store_i64(depth, function);
        depth.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        depth.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        depth.store(function);
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::PENDING_COMPLETION_DEPTH)
            .write(frame, GcOperand::i64_local(depth), schema, function);
        if restore {
            schema.struct_type::<CompletionRecord>().read_into(
                &record,
                self.completion(),
                schema,
                function,
            );
            if self.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() == FunctionExecutionKind::AsyncGenerator
            }) {
                let stage =
                    GcI32DomainLocal::new(schema, AsyncGeneratorReturnStage::Unawaited, function);
                schema
                    .struct_type::<CompletionRecord>()
                    .field(CompletionRecordSchema::RETURN_STAGE)
                    .read(&record, schema, function)
                    .store_domain(&stage, function);
                let activation = self.control_flow_async_generator_activation(function)?;
                schema
                    .struct_type::<AsyncGeneratorActivation>()
                    .field(AsyncGeneratorActivationSchema::RETURN_STAGE)
                    .write(&activation, stage.operand(), schema, function);
                activation.clear(function);
                stage.clear(schema, function);
            }
        }
        schema.release_i64_local(depth, function);
        next.clear(function);
        record.clear(function);
        Ok(())
    }

    fn emit_discard_generator_pending_completion(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_discard_pending_completion(function)
    }

    fn emit_discard_async_pending_completion(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_discard_pending_completion(function)
    }

    fn emit_discard_pending_completion(&self, function: &mut Function) -> Result<(), EmitError> {
        // A finalizer's new abrupt completion also owns its current Return
        // stage; discarding the parked record must preserve both.
        self.emit_pop_pending_completion(false, function)
    }

    fn emit_prepare_fresh_return(&self, function: &mut Function) -> Result<(), EmitError> {
        if self.current_function_meta().is_some_and(|meta| {
            meta.protocol().execution_kind() == FunctionExecutionKind::AsyncGenerator
        }) {
            let schema = self.runtime_schema();
            let activation = self.control_flow_async_generator_activation(function)?;
            schema
                .struct_type::<AsyncGeneratorActivation>()
                .field(AsyncGeneratorActivationSchema::RETURN_STAGE)
                .write(
                    &activation,
                    GcOperand::constant(AsyncGeneratorReturnStage::Unawaited),
                    schema,
                    function,
                );
            activation.clear(function);
        }
        Ok(())
    }

    pub(crate) fn set_completion_kind(&self, kind: CompletionKind, function: &mut Function) {
        if matches!(kind, CompletionKind::Return) {
            self.emit_retire_abandoned_identifier_references(function);
        }
        self.completion().set_kind(kind, function);
        function.instruction(&Instruction::I32Const(0));
        self.completion().target().store(function);
    }

    fn set_branch_completion(
        &self,
        kind: BranchCompletionKind,
        target: ControlTarget,
        function: &mut Function,
    ) {
        self.completion().set_kind(
            match kind {
                BranchCompletionKind::Break => CompletionKind::Break,
                BranchCompletionKind::Continue => CompletionKind::Continue,
            },
            function,
        );
        function.instruction(&Instruction::I32Const(target.frame as i32));
        self.completion().target().store(function);
    }

    pub(crate) fn emit_return_current_completion(&self, function: &mut Function) {
        self.emit_retire_abandoned_identifier_references_if_throw(function);
        if self.statement_list_value_context.is_some() {
            self.completion().kind().load(function);
            function.instruction(&Instruction::I32Const(CompletionKind::Return.code() as i32));
            function.instruction(&Instruction::I32Eq);
            self.completion().kind().load(function);
            function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
            function.instruction(&Instruction::If(BlockType::Empty));
            self.emit_retire_generator_statement_list_values(None, function);
            function.instruction(&Instruction::End);
        }
        if let Some(target) = self.completion_exit.main_job_checkpoint_target() {
            self.emit_branch_to_target(target, function);
            return;
        }
        if let Some(target) = self.completion_exit.source_activation_epilogue_target() {
            // Reference/finally cleanup above remains local to this exit.
            // Environment unwind keeps its actual branch depth; activation
            // properties/current caller/depth are restored once at the sink.
            // Statement-value retirement was already selected by the actual
            // completion kind above. A suspension must preserve parked values.
            self.emit_unwind_environments_to_target(target, function);
            function.branch_to_label(target.label);
            return;
        }
        self.emit_unwind_environment_depth(self.environment_depth, function);
        self.emit_leave_source_activation(function);
        self.completion().emit(function);
        function.instruction(&Instruction::Return);
    }

    pub(crate) fn emit_return_current_completion_if_throw(&mut self, function: &mut Function) {
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_return_current_completion(function);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_propagate_current_completion_if_throw(&mut self, function: &mut Function) {
        self.emit_propagate_current_throw_if_needed(function);
    }

    pub(crate) fn emit_propagate_current_throw(&self, function: &mut Function) {
        if let Some(target) = self.active_throw_target() {
            self.emit_retire_abandoned_identifier_references(function);
            self.emit_branch_to_target(target, function);
        } else {
            self.emit_return_current_completion(function);
        }
    }

    pub(crate) fn emit_break_current_completion_if_throw(
        &self,
        depth: u32,
        function: &mut Function,
    ) {
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Br(depth));
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_throw_from_value(&self, value: &ValueLocals, function: &mut Function) {
        self.completion().set_throw(value, function);
    }

    pub(crate) fn emit_resume_after_finally(
        &mut self,
        saved: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.completion().copy_from(saved, function);
        function.instruction(&Instruction::End);
        self.emit_dispatch_current_completion(function)
    }

    pub(crate) fn emit_dispatch_branch_completion(
        &self,
        targets: &[(u32, ControlTarget)],
        function: &mut Function,
    ) {
        for (target_id, branch_target) in targets {
            self.completion().target().load(function);
            function.instruction(&Instruction::I32Const(*target_id as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            if let Some(finalizer) = self.active_finally_target_for_branch(*branch_target) {
                self.emit_branch_to_target(finalizer, function);
            } else {
                self.set_completion_kind(CompletionKind::Normal, function);
                self.emit_branch_to_target(*branch_target, function);
            }
            function.instruction(&Instruction::End);
        }
        function.instruction(&Instruction::Unreachable);
    }

    /// Every frame a pending Break completion can name once a finalizer has
    /// swallowed the original `br`.
    ///
    /// `breakable_stack` only holds the iteration and switch statements that
    /// an unlabelled `break` can reach. A labelled Block is also a break
    /// target (ECMA-262 14.13: `BreakStatement : break LabelIdentifier ;` names
    /// any enclosing LabelledStatement), and it registers itself on
    /// `label_stack` alone — so a `break label` out of a `try`/`finally` inside
    /// one would otherwise resume into no target at all and fall through to
    /// the dispatcher's trap.
    pub(crate) fn active_break_targets(&self) -> Vec<(u32, ControlTarget)> {
        let mut targets: Vec<(u32, ControlTarget)> = Vec::new();
        let frames = self
            .breakable_stack
            .iter()
            .rev()
            .copied()
            .chain(self.label_stack.iter().rev().map(|label| label.break_frame));
        for target in frames {
            let target_id = target.frame as u32;
            if !targets.iter().any(|(id, _)| *id == target_id) {
                targets.push((target_id, target));
            }
        }
        targets
    }

    pub(crate) fn active_continue_targets(&self) -> Vec<(u32, ControlTarget)> {
        let mut targets = Vec::new();
        for target in self.loop_stack.iter().rev() {
            let target_id = target.continue_frame.frame as u32;
            if !targets.iter().any(|(id, _)| *id == target_id) {
                targets.push((target_id, target.continue_frame));
            }
        }
        targets
    }

    pub(crate) fn active_throw_target(&self) -> Option<ControlTarget> {
        match (self.throw_handler_stack.last(), self.finally_stack.last()) {
            (Some(handler), Some(finalizer)) => Some(innermost_target(*handler, *finalizer)),
            (Some(handler), None) => Some(*handler),
            (None, Some(finalizer)) => Some(*finalizer),
            (None, None) => None,
        }
    }

    fn active_finally_target_for_branch(
        &self,
        branch_target: ControlTarget,
    ) -> Option<ControlTarget> {
        self.finally_stack
            .last()
            .copied()
            .filter(|finalizer| finalizer_crosses_branch(*finalizer, branch_target))
    }

    /// Routes the pending completion — throw, return, break or continue — to
    /// whichever frame owns it.
    ///
    /// The four `1`/`2`/`4`/`5` compensations this used to add on top of
    /// `depth_to` were exactly the count of `If` frames opened by the
    /// `if`/`else` chain below (and, for the two dispatch arms, the extra `If`
    /// that `emit_dispatch_branch_completion` opens per target). They were
    /// right, and they are now double counts: the sink sees those `If`s. There
    /// is no `extra_depth` to forward, so a caller standing inside frames of
    /// its own no longer has to declare them.
    pub(crate) fn emit_dispatch_current_completion(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_THROW as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_propagate_current_throw(function);
        function.instruction(&Instruction::Else);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_RETURN as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        if let Some(target) = self.finally_stack.last().copied() {
            self.emit_branch_to_target(target, function);
        } else {
            self.emit_derived_constructor_body_result(function)?;
            self.emit_return_current_completion(function);
        }
        function.instruction(&Instruction::Else);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_BREAK as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let targets = self.active_break_targets();
        self.emit_dispatch_branch_completion(&targets, function);
        function.instruction(&Instruction::Else);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_CONTINUE as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let targets = self.active_continue_targets();
        self.emit_dispatch_branch_completion(&targets, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_dispatch_async_generator_completion(&self, function: &mut Function) {
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_THROW as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_retire_abandoned_identifier_references(function);
        if let Some(target) = self.active_throw_target() {
            self.emit_branch_to_target(target, function);
        } else {
            self.emit_return_current_completion(function);
        }
        function.instruction(&Instruction::Else);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_RETURN as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_retire_abandoned_identifier_references(function);
        if let Some(target) = self.finally_stack.last().copied() {
            self.emit_branch_to_target(target, function);
        } else {
            self.emit_return_current_completion(function);
        }
        // A Break or Continue that a yielding or awaiting finalizer has held
        // pending resumes toward its target once the finalizer completes
        // normally; dropping it here would silently continue the loop.
        function.instruction(&Instruction::Else);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_BREAK as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let targets = self.active_break_targets();
        self.emit_dispatch_branch_completion(&targets, function);
        function.instruction(&Instruction::Else);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(COMPLETION_KIND_CONTINUE as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let targets = self.active_continue_targets();
        self.emit_dispatch_branch_completion(&targets, function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    fn emit_dispatch_async_completion(&mut self, function: &mut Function) -> Result<(), EmitError> {
        if self.current_function_meta().is_some_and(|meta| {
            meta.protocol().execution_kind() == FunctionExecutionKind::AsyncGenerator
        }) {
            self.emit_dispatch_async_generator_completion(function);
            return Ok(());
        }
        self.emit_dispatch_current_completion(function)
    }

    /// Opens a Wasm control frame *and* records it, in one call.
    ///
    /// The frame instruction is written here rather than by the caller, so the
    /// "emit the frame, then push the entry" ordering that ~190 call sites used
    /// to follow by hand cannot be got backwards — and the label recorded in
    /// the returned [`ControlTarget`] is the sink's depth *after* the frame is
    /// open, which is the label a branch to this frame must name.
    pub(crate) fn open_frame(
        &mut self,
        kind: ControlFrameKind,
        function: &mut Function,
    ) -> ControlTarget {
        function.instruction(&match kind {
            ControlFrameKind::If => Instruction::If(BlockType::Empty),
            ControlFrameKind::Block => Instruction::Block(BlockType::Empty),
            ControlFrameKind::Loop => Instruction::Loop(BlockType::Empty),
        });
        let target = ControlTarget {
            frame: self.control_stack.len(),
            environment_depth: self.environment_depth,
            label: function.label_depth(),
        };
        self.control_stack.push(kind);
        target
    }

    /// Pops the tracked entry `open_frame` pushed.
    ///
    /// The matching `End` is still written by the caller and is still counted
    /// by the sink, so this does not need the body: the tracked stack no longer
    /// carries any branch arithmetic, only the frame *identity* used to order
    /// targets and to name one in the completion dispatcher.
    pub(crate) fn pop_control(&mut self, expected: ControlFrameKind) {
        let actual = self
            .control_stack
            .pop()
            .expect("control stack must not underflow");
        assert!(matches!(
            (actual, expected),
            (ControlFrameKind::If, ControlFrameKind::If)
                | (ControlFrameKind::Block, ControlFrameKind::Block)
                | (ControlFrameKind::Loop, ControlFrameKind::Loop)
        ));
    }

    fn emit_unwind_environment_depth(&self, hops: u32, function: &mut Function) {
        let schema = self.runtime_schema();
        for _ in 0..hops {
            self.replace_current_environment(
                schema
                    .struct_type::<Environment>()
                    .field(EnvironmentSchema::PARENT)
                    .read(self.current_environment(), schema, function)
                    .reference(),
                function,
            );
        }
    }

    fn emit_unwind_environments_to_target(&self, target: ControlTarget, function: &mut Function) {
        self.emit_unwind_environment_depth(
            environment_hops(self.environment_depth, target.environment_depth),
            function,
        );
    }

    pub(crate) fn emit_branch_to_target(&self, target: ControlTarget, function: &mut Function) {
        self.emit_retire_generator_statement_list_values(Some(target), function);
        self.emit_unwind_environments_to_target(target, function);
        function.branch_to_label(target.label);
    }

    pub(crate) fn emit_branch_if_to_target(&self, target: ControlTarget, function: &mut Function) {
        if target.environment_depth == self.environment_depth
            && !self.generator_statement_list_branch_retires(target)
        {
            function.branch_if_to_label(target.label);
            return;
        }

        // The `If` opened here used to need an `extra_depth + 1` on the branch
        // inside it. The sink counts it, so the compensation is gone.
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_branch_to_target(target, function);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn push_labels(
        &mut self,
        labels: &[String],
        break_frame: ControlTarget,
        continue_frame: Option<ControlTarget>,
    ) {
        for label in labels {
            self.label_stack.push(LabelTargets {
                name: label.clone(),
                break_frame,
                continue_frame,
            });
        }
    }

    pub(crate) fn pop_labels(&mut self, count: usize) {
        for _ in 0..count {
            self.label_stack.pop();
        }
    }

    pub(crate) fn compile_block_contents(
        &mut self,
        block: &BlockIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.with_checked_async_generator_source_environment(|builder| {
            builder.compile_block_contents_in_source_environment(block, function)
        })
    }

    fn compile_block_contents_in_source_environment(
        &mut self,
        block: &BlockIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let execution = self
            .current_function_meta()
            .map(|meta| meta.protocol().execution_kind());
        let entry_state = match execution {
            Some(FunctionExecutionKind::Async | FunctionExecutionKind::AsyncGenerator) => block
                .statements
                .iter()
                .find_map(Self::async_statement_entry_state),
            Some(FunctionExecutionKind::Generator) => block
                .statements
                .iter()
                .find_map(Self::generator_statement_entry_state),
            Some(FunctionExecutionKind::Ordinary) | None => None,
        };
        if let Some(entry_state) = entry_state {
            return self.compile_resumable_block_contents(
                block,
                entry_state,
                !std::ptr::eq(block, self.body),
                function,
            );
        }
        if let Some(environment) = &block.lexical_environment {
            self.emit_enter_lexical_environment(environment, function)?;
        }
        let resumable_root_body = std::ptr::eq(block, self.body)
            && matches!(
                execution,
                Some(
                    FunctionExecutionKind::Generator
                        | FunctionExecutionKind::Async
                        | FunctionExecutionKind::AsyncGenerator
                )
            );
        if !resumable_root_body {
            self.initialize_direct_lexical_bindings(&block.statements, function);
        }
        // Ordinary source bodies keep their activation, locals and completion
        // while sharing one code row for long runs of inert numeric arguments.
        // Resumable and runtime-owned bodies retain their existing control path.
        let compact_calls = self.has_source_execution_environment()
            && matches!(execution, Some(FunctionExecutionKind::Ordinary) | None);
        let mut next = 0;
        while next < block.statements.len() {
            if compact_calls {
                if let Some(run) = repeated_identifier_reads::IdentifierReadRun::of_prefix(
                    &block.statements[next..],
                ) {
                    self.compile_identifier_read_run(&run, function)?;
                    next += run.len();
                    continue;
                }
                if let Some(run) =
                    crate::call_rows::NumericCallRun::of_prefix(&block.statements[next..])
                {
                    self.compile_numeric_call_run(&run, function)?;
                    next += run.len();
                    continue;
                }
            }
            self.compile_statement(&block.statements[next], function)?;
            next += 1;
        }
        self.emit_clear_private_argument_lists_in_scope(function)?;
        if block.lexical_environment.is_some() {
            self.emit_leave_lexical_environment(function);
        }
        Ok(())
    }

    fn compile_resumable_block_contents(
        &mut self,
        block: &BlockIr,
        entry_state: u32,
        initialize_bindings: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let generator = self.current_function_meta().is_some_and(|meta| {
            meta.protocol().execution_kind() == FunctionExecutionKind::Generator
        });
        let guarded_environment = block.lexical_environment.is_some();
        if guarded_environment {
            let exit_state = block
                .statements
                .iter()
                .rev()
                .find_map(|statement| {
                    if generator {
                        Self::generator_statement_exit_state(statement)
                    } else {
                        Self::async_statement_exit_state(statement)
                    }
                })
                .unwrap_or(entry_state);
            let point = self.emit_resumable_resume_point(function)?;
            point.load(function);
            function.instruction(&Instruction::I32Const(entry_state as i32));
            function.instruction(&Instruction::I32GeU);
            point.load(function);
            function.instruction(&Instruction::I32Const(exit_state as i32));
            function.instruction(&Instruction::I32LeU);
            function.instruction(&Instruction::I32And);
            schema.release_i32_local(point, function);
            self.open_frame(ControlFrameKind::If, function);
        }
        if let Some(environment) = &block.lexical_environment {
            self.emit_enter_resumable_block_environment(environment, entry_state, function)?;
        }
        if initialize_bindings {
            let point = self.emit_resumable_resume_point(function)?;
            point.load(function);
            function.instruction(&Instruction::I32Const(entry_state as i32));
            function.instruction(&Instruction::I32Eq);
            schema.release_i32_local(point, function);
            self.open_frame(ControlFrameKind::If, function);
            self.initialize_direct_lexical_bindings(&block.statements, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.compile_resumable_statement_sequence(&block.statements, entry_state, function)?;
        self.emit_clear_private_argument_lists_in_scope(function)?;
        if guarded_environment {
            self.emit_leave_lexical_environment(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        Ok(())
    }

    fn is_async_resume_body(&self) -> bool {
        self.current_function_meta().is_some_and(|meta| {
            matches!(
                meta.protocol().execution_kind(),
                FunctionExecutionKind::Async | FunctionExecutionKind::AsyncGenerator
            )
        })
    }

    fn async_statement_entry_state(statement: &StatementIr) -> Option<u32> {
        match statement {
            StatementIr::AsyncGeneratorLoop(plan) => Some(plan.entry_state()),
            StatementIr::AsyncGeneratorIf(plan) => Some(plan.entry_state()),
            StatementIr::AsyncGeneratorWith(plan) => Some(plan.entry_state()),
            StatementIr::AsyncGeneratorSwitch(plan) => Some(plan.entry_state()),
            StatementIr::EmptyStatementCompletion(item) => {
                Self::async_statement_entry_state(item.statement())
            }
            StatementIr::ResumableClassDefinition(plan) => Some(plan.entry_state()),
            StatementIr::AsyncFunctionIf { plan, .. } => Some(plan.entry_state()),
            StatementIr::AsyncFunctionWhile(plan) => Some(plan.entry_state()),
            StatementIr::AsyncFunctionSwitch(plan) => Some(plan.entry_state()),
            StatementIr::AsyncFunctionArrayDestructuring(plan) => Some(plan.entry_state()),
            StatementIr::AsyncGeneratorArrayDestructuring(plan) => Some(plan.entry_state()),
            StatementIr::AsyncFunctionWith(plan) => Some(plan.entry_state()),
            StatementIr::AsyncGeneratorForIn(plan) => Some(plan.entry_state()),
            StatementIr::AsyncGeneratorForOf(plan) => Some(plan.entry_state()),
            StatementIr::AsyncGeneratorResourceScope(plan) => Some(plan.entry_state()),
            StatementIr::AsyncModuleInstantiation => Some(0),
            StatementIr::AsyncAwait { suspend_state, .. } => Some(*suspend_state),
            StatementIr::GeneratorYield { suspend_state, .. } => Some(*suspend_state),
            StatementIr::GeneratorLoop { entry_state, .. }
            | StatementIr::GeneratorIf { entry_state, .. } => Some(*entry_state),
            StatementIr::LexicalBlock(statements) => statements
                .iter()
                .find_map(Self::async_statement_entry_state),
            StatementIr::Block(block) => block
                .statements
                .iter()
                .find_map(Self::async_statement_entry_state),
            // Checked labels own their completion exits. Immediate labels
            // forward the states of a directly labelled complete source owner.
            StatementIr::Labelled {
                async_plan: Some(plan),
                ..
            } => Some(plan.entry_state()),
            StatementIr::Labelled {
                statement,
                async_plan: None,
                ..
            } => Self::labelled_async_function_for_of_plan(statement)
                .map(AsyncFunctionForOfIteratorPlanIr::entry_state)
                .or_else(|| {
                    Self::labelled_complete_iterator_states(statement).map(|states| states.0)
                })
                .or_else(|| {
                    Self::labelled_async_function_while_plan(statement)
                        .map(scriptw_ir::AsyncFunctionWhileConditionIr::entry_state)
                })
                .or_else(|| {
                    Self::labelled_async_disposable_for_finalizer(statement)
                        .map(AsyncDisposableFinalizerPlanIr::entry_state)
                })
                .or_else(|| {
                    Self::labelled_async_linear_loop_states(statement).map(|states| states.0)
                })
                .or_else(|| {
                    Self::labelled_async_generator_loop_plan(statement)
                        .map(scriptw_ir::AsyncGeneratorLoopIr::entry_state)
                })
                .or_else(|| {
                    Self::labelled_async_generator_with_plan(statement)
                        .map(scriptw_ir::AsyncGeneratorWithIr::entry_state)
                })
                .or_else(|| {
                    Self::labelled_complete_resource_switch_plan(statement)
                        .map(scriptw_ir::AsyncGeneratorSwitchIr::entry_state)
                }),
            StatementIr::TryCatch {
                async_plan: Some(plan),
                ..
            }
            | StatementIr::TryFinally {
                async_plan: Some(plan),
                ..
            }
            | StatementIr::TryCatchFinally {
                async_plan: Some(plan),
                ..
            } => Some(plan.entry_state),
            StatementIr::ForOfIterator {
                head:
                    ForOfIteratorHeadIr::Assignment {
                        async_plan: Some(plan),
                        ..
                    },
                ..
            } => Some(plan.entry_state),
            StatementIr::AsyncFunctionForOfIterator { plan, .. } => Some(plan.entry_state()),
            StatementIr::ForOfIterator {
                head: ForOfIteratorHeadIr::AsyncDisposable(head),
                ..
            } => Some(head.capability().finalizer().entry_state()),
            StatementIr::SyncDisposableScope {
                execution:
                    SyncDisposableScopeExecutionIr::AsyncFunction(_)
                    | SyncDisposableScopeExecutionIr::AsyncGenerator(_),
                body,
                ..
            } => body
                .statements
                .iter()
                .find_map(Self::async_statement_entry_state),
            StatementIr::AsyncDisposableScope { execution, .. } => Some(
                ActivationAsyncDisposeOwner::from_execution(execution)
                    .finalizer()
                    .entry_state(),
            ),
            StatementIr::For {
                init: Some(ForInitIr::AsyncDisposable(init)),
                ..
            } => Some(init.capability().finalizer().entry_state()),
            StatementIr::SyncDisposableScope {
                execution:
                    SyncDisposableScopeExecutionIr::Immediate
                    | SyncDisposableScopeExecutionIr::PlainGenerator(_),
                ..
            } => None,
            _ => None,
        }
    }

    pub(crate) fn async_statement_exit_state(statement: &StatementIr) -> Option<u32> {
        match statement {
            StatementIr::AsyncGeneratorLoop(plan) => Some(plan.exit_state()),
            StatementIr::AsyncGeneratorIf(plan) => Some(plan.exit_state()),
            StatementIr::AsyncGeneratorWith(plan) => Some(plan.exit_state()),
            StatementIr::AsyncGeneratorSwitch(plan) => Some(plan.exit_state()),
            StatementIr::EmptyStatementCompletion(item) => {
                Self::async_statement_exit_state(item.statement())
            }
            StatementIr::ResumableClassDefinition(plan) => Some(plan.exit_state()),
            StatementIr::AsyncFunctionIf { plan, .. } => Some(plan.exit_state()),
            StatementIr::AsyncFunctionWhile(plan) => Some(plan.exit_state()),
            StatementIr::AsyncFunctionSwitch(plan) => Some(plan.exit_state()),
            StatementIr::AsyncFunctionArrayDestructuring(plan) => Some(plan.exit_state()),
            StatementIr::AsyncGeneratorArrayDestructuring(plan) => Some(plan.exit_state()),
            StatementIr::AsyncFunctionWith(plan) => Some(plan.exit_state()),
            StatementIr::AsyncGeneratorForIn(plan) => Some(plan.exit_state()),
            StatementIr::AsyncGeneratorForOf(plan) => Some(plan.exit_state()),
            StatementIr::AsyncGeneratorResourceScope(plan) => Some(plan.exit_state()),
            StatementIr::AsyncModuleInstantiation => Some(1),
            StatementIr::AsyncAwait { resume_state, .. } => Some(*resume_state),
            StatementIr::GeneratorYield { resume_state, .. } => Some(*resume_state),
            StatementIr::GeneratorLoop { exit_state, .. }
            | StatementIr::GeneratorIf { exit_state, .. } => Some(*exit_state),
            StatementIr::LexicalBlock(statements) => statements
                .iter()
                .rev()
                .find_map(Self::async_statement_exit_state),
            StatementIr::Block(block) => block
                .statements
                .iter()
                .rev()
                .find_map(Self::async_statement_exit_state),
            StatementIr::Labelled {
                async_plan: Some(plan),
                ..
            } => Some(plan.exit_state()),
            StatementIr::Labelled {
                statement,
                async_plan: None,
                ..
            } => Self::labelled_async_function_for_of_plan(statement)
                .map(AsyncFunctionForOfIteratorPlanIr::exit_state)
                .or_else(|| {
                    Self::labelled_complete_iterator_states(statement).map(|states| states.1)
                })
                .or_else(|| {
                    Self::labelled_async_function_while_plan(statement)
                        .map(scriptw_ir::AsyncFunctionWhileConditionIr::exit_state)
                })
                .or_else(|| {
                    Self::labelled_async_disposable_for_finalizer(statement)
                        .map(AsyncDisposableFinalizerPlanIr::exit_state)
                })
                .or_else(|| {
                    Self::labelled_async_linear_loop_states(statement).map(|states| states.1)
                })
                .or_else(|| {
                    Self::labelled_async_generator_loop_plan(statement)
                        .map(scriptw_ir::AsyncGeneratorLoopIr::exit_state)
                })
                .or_else(|| {
                    Self::labelled_async_generator_with_plan(statement)
                        .map(scriptw_ir::AsyncGeneratorWithIr::exit_state)
                })
                .or_else(|| {
                    Self::labelled_complete_resource_switch_plan(statement)
                        .map(scriptw_ir::AsyncGeneratorSwitchIr::exit_state)
                }),
            StatementIr::TryCatch {
                async_plan: Some(plan),
                ..
            }
            | StatementIr::TryFinally {
                async_plan: Some(plan),
                ..
            }
            | StatementIr::TryCatchFinally {
                async_plan: Some(plan),
                ..
            } => Some(plan.exit_state),
            StatementIr::ForOfIterator {
                head:
                    ForOfIteratorHeadIr::Assignment {
                        async_plan: Some(plan),
                        ..
                    },
                ..
            } => Some(plan.exit_state),
            StatementIr::AsyncFunctionForOfIterator { plan, .. } => Some(plan.exit_state()),
            StatementIr::ForOfIterator {
                head: ForOfIteratorHeadIr::AsyncDisposable(head),
                ..
            } => Some(head.capability().finalizer().exit_state()),
            StatementIr::SyncDisposableScope {
                execution:
                    SyncDisposableScopeExecutionIr::AsyncFunction(_)
                    | SyncDisposableScopeExecutionIr::AsyncGenerator(_),
                body,
                ..
            } => body
                .statements
                .iter()
                .rev()
                .find_map(Self::async_statement_exit_state),
            StatementIr::AsyncDisposableScope { execution, .. } => Some(
                ActivationAsyncDisposeOwner::from_execution(execution)
                    .finalizer()
                    .exit_state(),
            ),
            StatementIr::For {
                init: Some(ForInitIr::AsyncDisposable(init)),
                ..
            } => Some(init.capability().finalizer().exit_state()),
            StatementIr::SyncDisposableScope {
                execution:
                    SyncDisposableScopeExecutionIr::Immediate
                    | SyncDisposableScopeExecutionIr::PlainGenerator(_),
                ..
            } => None,
            _ => None,
        }
    }

    fn labelled_async_generator_loop_plan(
        statement: &StatementIr,
    ) -> Option<&scriptw_ir::AsyncGeneratorLoopIr> {
        match statement {
            StatementIr::AsyncGeneratorLoop(plan) => Some(plan),
            StatementIr::Labelled {
                statement,
                async_plan: None,
                ..
            } => Self::labelled_async_generator_loop_plan(statement),
            _ => None,
        }
    }

    fn labelled_async_generator_with_plan(
        statement: &StatementIr,
    ) -> Option<&scriptw_ir::AsyncGeneratorWithIr> {
        match statement {
            StatementIr::AsyncGeneratorWith(plan) => Some(plan),
            StatementIr::Labelled {
                statement,
                async_plan: None,
                ..
            } => Self::labelled_async_generator_with_plan(statement),
            _ => None,
        }
    }

    fn labelled_complete_resource_switch_plan(
        statement: &StatementIr,
    ) -> Option<&scriptw_ir::AsyncGeneratorSwitchIr> {
        match statement {
            StatementIr::AsyncGeneratorSwitch(plan) => Some(plan),
            StatementIr::Labelled {
                statement,
                async_plan: None,
                ..
            } => Self::labelled_complete_resource_switch_plan(statement),
            _ => None,
        }
    }

    fn labelled_async_linear_loop_states(statement: &StatementIr) -> Option<(u32, u32)> {
        match statement {
            StatementIr::GeneratorLoop {
                entry_state,
                exit_state,
                ..
            } => Some((*entry_state, *exit_state)),
            StatementIr::Labelled {
                statement,
                async_plan: None,
                ..
            } => Self::labelled_async_linear_loop_states(statement),
            _ => None,
        }
    }

    fn labelled_async_disposable_for_finalizer(
        statement: &StatementIr,
    ) -> Option<&AsyncDisposableFinalizerPlanIr> {
        match statement {
            StatementIr::Labelled { statement, .. } => {
                Self::labelled_async_disposable_for_finalizer(statement)
            }
            StatementIr::For {
                init: Some(ForInitIr::AsyncDisposable(init)),
                ..
            } => Some(init.capability().finalizer()),
            StatementIr::ForOfIterator {
                head: ForOfIteratorHeadIr::AsyncDisposable(head),
                ..
            } => Some(head.capability().finalizer()),
            _ => None,
        }
    }

    fn labelled_async_function_while_plan(
        statement: &StatementIr,
    ) -> Option<&scriptw_ir::AsyncFunctionWhileConditionIr> {
        match statement {
            StatementIr::AsyncFunctionWhile(plan) => Some(plan),
            StatementIr::Labelled { statement, .. } => {
                Self::labelled_async_function_while_plan(statement)
            }
            _ => None,
        }
    }

    fn labelled_async_function_for_of_plan(
        statement: &StatementIr,
    ) -> Option<&AsyncFunctionForOfIteratorPlanIr> {
        match statement {
            StatementIr::Labelled { statement, .. } => {
                Self::labelled_async_function_for_of_plan(statement)
            }
            StatementIr::AsyncFunctionForOfIterator { plan, .. } => Some(plan),
            _ => None,
        }
    }

    fn labelled_complete_iterator_states(statement: &StatementIr) -> Option<(u32, u32)> {
        match statement {
            StatementIr::AsyncGeneratorForIn(plan)
                if plan.execution() != scriptw_ir::ResumableRegionProtocolIr::Generator =>
            {
                Some((plan.entry_state(), plan.exit_state()))
            }
            StatementIr::AsyncGeneratorForOf(plan)
                if plan.execution() != scriptw_ir::ResumableRegionProtocolIr::Generator =>
            {
                Some((plan.entry_state(), plan.exit_state()))
            }
            StatementIr::Labelled { statement, .. } => {
                Self::labelled_complete_iterator_states(statement)
            }
            _ => None,
        }
    }

    fn generator_statement_entry_state(statement: &StatementIr) -> Option<u32> {
        match statement {
            StatementIr::AsyncGeneratorLoop(plan) => (plan.execution()
                == scriptw_ir::ResumableRegionProtocolIr::Generator)
                .then_some(plan.entry_state()),
            StatementIr::AsyncGeneratorSwitch(plan) => (plan.execution()
                == scriptw_ir::ResumableRegionProtocolIr::Generator)
                .then_some(plan.entry_state()),
            StatementIr::AsyncGeneratorIf(_) | StatementIr::AsyncGeneratorWith(_) => None,
            StatementIr::AsyncGeneratorForIn(plan) => (plan.execution()
                == scriptw_ir::ResumableRegionProtocolIr::Generator)
                .then_some(plan.entry_state()),
            StatementIr::AsyncGeneratorForOf(plan) => (plan.execution()
                == scriptw_ir::ResumableRegionProtocolIr::Generator)
                .then_some(plan.entry_state()),
            StatementIr::AsyncGeneratorResourceScope(plan) => (plan.execution()
                == scriptw_ir::ResumableRegionProtocolIr::Generator)
                .then_some(plan.entry_state()),
            StatementIr::AsyncGeneratorArrayDestructuring(_) => None,
            StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => Some(plan.entry_state()),
            StatementIr::OrdinaryGeneratorWith(plan) => Some(plan.entry_state()),
            StatementIr::OrdinaryGeneratorSwitch(plan) => Some(plan.entry_state()),
            StatementIr::EmptyStatementCompletion(item) => {
                Self::generator_statement_entry_state(item.statement())
            }
            StatementIr::OrdinaryGeneratorLoop(plan) => Some(plan.entry_state()),
            StatementIr::OrdinaryGeneratorIf(plan) => Some(plan.entry_state()),
            StatementIr::Labelled { statement, .. } => {
                Self::generator_statement_entry_state(statement)
            }
            StatementIr::GeneratorForOfIterator { plan, .. } => Some(plan.entry_state()),
            StatementIr::ResumableClassDefinition(plan) => Some(plan.entry_state()),
            StatementIr::GeneratorYield { suspend_state, .. } => Some(*suspend_state),
            StatementIr::LexicalBlock(statements) => statements
                .iter()
                .find_map(Self::generator_statement_entry_state),
            StatementIr::Block(block) => block
                .statements
                .iter()
                .find_map(Self::generator_statement_entry_state),
            StatementIr::GeneratorLoop { entry_state, .. }
            | StatementIr::GeneratorIf { entry_state, .. } => Some(*entry_state),
            StatementIr::TryCatch {
                generator_plan: Some(plan),
                ..
            }
            | StatementIr::TryFinally {
                generator_plan: Some(plan),
                ..
            }
            | StatementIr::TryCatchFinally {
                generator_plan: Some(plan),
                ..
            } => Some(plan.entry_state),
            StatementIr::SyncDisposableScope {
                execution: SyncDisposableScopeExecutionIr::PlainGenerator(_),
                body,
                ..
            } => body
                .statements
                .iter()
                .find_map(Self::generator_statement_entry_state),
            StatementIr::SyncDisposableScope {
                execution:
                    SyncDisposableScopeExecutionIr::Immediate
                    | SyncDisposableScopeExecutionIr::AsyncFunction(_)
                    | SyncDisposableScopeExecutionIr::AsyncGenerator(_),
                ..
            } => None,
            StatementIr::AsyncDisposableScope { .. } => None,
            _ => None,
        }
    }

    fn generator_statement_exit_state(statement: &StatementIr) -> Option<u32> {
        match statement {
            StatementIr::AsyncGeneratorLoop(plan) => (plan.execution()
                == scriptw_ir::ResumableRegionProtocolIr::Generator)
                .then_some(plan.exit_state()),
            StatementIr::AsyncGeneratorSwitch(plan) => (plan.execution()
                == scriptw_ir::ResumableRegionProtocolIr::Generator)
                .then_some(plan.exit_state()),
            StatementIr::AsyncGeneratorIf(_) | StatementIr::AsyncGeneratorWith(_) => None,
            StatementIr::AsyncGeneratorForIn(plan) => (plan.execution()
                == scriptw_ir::ResumableRegionProtocolIr::Generator)
                .then_some(plan.exit_state()),
            StatementIr::AsyncGeneratorForOf(plan) => (plan.execution()
                == scriptw_ir::ResumableRegionProtocolIr::Generator)
                .then_some(plan.exit_state()),
            StatementIr::AsyncGeneratorResourceScope(plan) => (plan.execution()
                == scriptw_ir::ResumableRegionProtocolIr::Generator)
                .then_some(plan.exit_state()),
            StatementIr::AsyncGeneratorArrayDestructuring(_) => None,
            StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => Some(plan.exit_state()),
            StatementIr::OrdinaryGeneratorWith(plan) => Some(plan.exit_state()),
            StatementIr::OrdinaryGeneratorSwitch(plan) => Some(plan.exit_state()),
            StatementIr::EmptyStatementCompletion(item) => {
                Self::generator_statement_exit_state(item.statement())
            }
            StatementIr::OrdinaryGeneratorLoop(plan) => Some(plan.exit_state()),
            StatementIr::OrdinaryGeneratorIf(plan) => Some(plan.exit_state()),
            StatementIr::Labelled { statement, .. } => {
                Self::generator_statement_exit_state(statement)
            }
            StatementIr::GeneratorForOfIterator { plan, .. } => Some(plan.exit_state()),
            StatementIr::ResumableClassDefinition(plan) => Some(plan.exit_state()),
            StatementIr::GeneratorYield { resume_state, .. } => Some(*resume_state),
            StatementIr::LexicalBlock(statements) => statements
                .iter()
                .rev()
                .find_map(Self::generator_statement_exit_state),
            StatementIr::Block(block) => block
                .statements
                .iter()
                .rev()
                .find_map(Self::generator_statement_exit_state),
            StatementIr::GeneratorLoop { exit_state, .. }
            | StatementIr::GeneratorIf { exit_state, .. } => Some(*exit_state),
            StatementIr::TryCatch {
                generator_plan: Some(plan),
                ..
            }
            | StatementIr::TryFinally {
                generator_plan: Some(plan),
                ..
            }
            | StatementIr::TryCatchFinally {
                generator_plan: Some(plan),
                ..
            } => Some(plan.exit_state),
            StatementIr::SyncDisposableScope {
                execution: SyncDisposableScopeExecutionIr::PlainGenerator(_),
                body,
                ..
            } => body
                .statements
                .iter()
                .rev()
                .find_map(Self::generator_statement_exit_state),
            StatementIr::SyncDisposableScope {
                execution:
                    SyncDisposableScopeExecutionIr::Immediate
                    | SyncDisposableScopeExecutionIr::AsyncFunction(_)
                    | SyncDisposableScopeExecutionIr::AsyncGenerator(_),
                ..
            } => None,
            StatementIr::AsyncDisposableScope { .. } => None,
            _ => None,
        }
    }

    pub(crate) fn initialize_direct_lexical_bindings(
        &mut self,
        statements: &[StatementIr],
        function: &mut Function,
    ) {
        for statement in statements {
            match statement {
                // The original head TDZ and each per-key binding are owned by
                // the complete ForIn's own phase, never the enclosing block.
                StatementIr::AsyncGeneratorForIn(_) | StatementIr::AsyncGeneratorForOf(_) => {}
                StatementIr::AsyncGeneratorResourceScope(_) => {}
                StatementIr::AsyncGeneratorResourceRegistration(registration) => {
                    let binding = self
                        .lookup_current_scope_binding(registration.binding_name())
                        .or_else(|| self.lookup_binding(registration.binding_name()))
                        .unwrap_or_else(|| {
                            // This also publishes an existing activation cell in
                            // the compiler scope. Resolving its raw storage alone
                            // leaves later identifier reads without an alias.
                            self.allocate_binding(
                                registration.binding_name().to_string(),
                                BindingMode::Const,
                                registration.initializer().kind,
                                function,
                            )
                        });
                    self.initialize_binding_uninitialized(binding, function);
                }
                // Every case TDZ/function belongs to the single CaseBlock
                // entered by its complete Switch, never this enclosing list.
                StatementIr::OrdinaryGeneratorSwitch(_) | StatementIr::AsyncGeneratorSwitch(_) => {}
                StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => {
                    self.initialize_direct_lexical_bindings(
                        &plan.body().block().statements,
                        function,
                    );
                }
                StatementIr::AsyncFunctionArrayDestructuring(plan) => {
                    self.initialize_direct_lexical_bindings(&plan.body().statements, function);
                }
                StatementIr::AsyncGeneratorArrayDestructuring(plan) => {
                    self.initialize_direct_lexical_bindings(
                        &plan.body().block().statements,
                        function,
                    );
                }
                StatementIr::OrdinaryGeneratorWith(plan) => {
                    self.initialize_direct_lexical_bindings(
                        &plan.body().block().statements,
                        function,
                    );
                }
                StatementIr::AsyncFunctionWith(plan) => {
                    self.initialize_direct_lexical_bindings(&plan.body().statements, function);
                }
                StatementIr::AsyncGeneratorWith(plan) => {
                    self.initialize_direct_lexical_bindings(
                        &plan.body().block().statements,
                        function,
                    );
                }
                StatementIr::EmptyStatementCompletion(item) => {
                    self.initialize_direct_lexical_bindings(
                        std::slice::from_ref(item.statement()),
                        function,
                    );
                }
                StatementIr::Lexical { mode, name, init } => {
                    let storage = self
                        .lookup_current_scope_binding(name)
                        .or_else(|| self.lookup_binding(name))
                        .unwrap_or_else(|| {
                            self.allocate_binding(name.clone(), *mode, init.kind, function)
                        });
                    self.initialize_binding_uninitialized(storage, function);
                }
                StatementIr::LexicalBlock(statements) => {
                    self.initialize_direct_lexical_bindings(statements, function);
                }
                StatementIr::SyncDisposableScope {
                    resources, body, ..
                } => {
                    self.initialize_sync_disposable_resource_bindings(resources, function);
                    self.initialize_direct_lexical_bindings(&body.statements, function);
                }
                StatementIr::AsyncDisposableScope {
                    resources, body, ..
                } => {
                    self.initialize_async_disposable_resource_bindings(resources, function);
                    self.initialize_direct_lexical_bindings(&body.statements, function);
                }
                StatementIr::DeclarationEvaluation(TypedExpr {
                    expr:
                        ExprIr::ArrayDestructure {
                            pattern,
                            evaluation,
                            ..
                        },
                    ..
                }) => match *evaluation {
                    ArrayDestructuringEvaluationIr::BindingInitialization => {
                        pattern.visit_bindings(&mut |mode, name| {
                            if mode == BindingMode::Var {
                                return;
                            }
                            let storage = self
                                .lookup_current_scope_binding(name)
                                .or_else(|| self.lookup_binding(name))
                                .unwrap_or_else(|| {
                                    self.allocate_binding(
                                        name.to_string(),
                                        mode,
                                        ValueKind::Dynamic,
                                        function,
                                    )
                                });
                            self.initialize_binding_uninitialized(storage, function);
                        });
                    }
                    ArrayDestructuringEvaluationIr::AssignmentEvaluation => {}
                },
                StatementIr::DeclarationEvaluation(TypedExpr {
                    expr: ExprIr::ObjectDestructure { pattern, .. },
                    ..
                }) => {
                    pattern.visit_bindings(&mut |mode, name| {
                        if mode == BindingMode::Var {
                            return;
                        }
                        let storage = self
                            .lookup_current_scope_binding(name)
                            .or_else(|| self.lookup_binding(name))
                            .unwrap_or_else(|| {
                                self.allocate_binding(
                                    name.to_string(),
                                    mode,
                                    ValueKind::Dynamic,
                                    function,
                                )
                            });
                        self.initialize_binding_uninitialized(storage, function);
                    });
                }
                StatementIr::DeclarationEvaluation(TypedExpr {
                    expr: ExprIr::ObjectDestructuringOperation(operation),
                    ..
                }) => {
                    operation.visit_bindings(&mut |mode, name| {
                        if mode == BindingMode::Var {
                            return;
                        }
                        let storage = self
                            .lookup_current_scope_binding(name)
                            .or_else(|| self.lookup_binding(name))
                            .unwrap_or_else(|| {
                                self.allocate_binding(
                                    name.to_string(),
                                    mode,
                                    ValueKind::Dynamic,
                                    function,
                                )
                            });
                        self.initialize_binding_uninitialized(storage, function);
                    });
                }
                _ => {}
            }
        }
    }

    fn compile_resumable_async_loop(
        &mut self,
        statement: &StatementIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let StatementIr::GeneratorLoop {
            init,
            test,
            update,
            iteration_environment,
            before_suspension,
            suspension_statement,
            after_suspension,
            entry_state,
            resume_state,
            exit_state,
        } = statement
        else {
            return Err(EmitError::unsupported(
                "compiler invariant: async loop requires its checked resumable plan",
            ));
        };
        let schema = self.runtime_schema();
        let point = self.emit_resumable_resume_point(function)?;
        let frame = schema.reserve_gc_local(function).initialize(
            self.pending_completion_frame()?.load(schema, function),
            function,
        );
        let fresh = match iteration_environment {
            ResumableLoopIterationEnvironmentIr::StorageOnly => None,
            ResumableLoopIterationEnvironmentIr::FreshPerIteration(environment) => {
                Some(environment)
            }
        };
        point.load(function);
        function.instruction(&Instruction::I32Const(*entry_state as i32));
        function.instruction(&Instruction::I32GeU);
        point.load(function);
        function.instruction(&Instruction::I32Const(*resume_state as i32));
        function.instruction(&Instruction::I32LeU);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        point.load(function);
        function.instruction(&Instruction::I32Const(*entry_state as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        if fresh.is_none() {
            self.initialize_direct_lexical_bindings(before_suspension, function);
            self.initialize_direct_lexical_bindings(after_suspension, function);
        }
        if let Some(init) = init {
            self.compile_for_init(init, function)?;
            self.emit_dispatch_async_completion(function)?;
        }
        function.instruction(&Instruction::Else);
        let resume_cleanup = fresh.map(|_| self.open_frame(ControlFrameKind::Block, function));
        if let Some(environment) = fresh {
            self.push_scope();
            let saved = schema
                .struct_type::<InvocationFrame>()
                .field(InvocationFrameSchema::LEXICAL_ENVIRONMENT)
                .read(&frame, schema, function)
                .reference();
            self.replace_current_environment(saved, function);
            self.begin_existing_lexical_environment_scope(environment);
            self.finally_stack.push(ControlTarget {
                environment_depth: self.environment_depth,
                ..resume_cleanup.expect("fresh resumed iteration owns cleanup")
            });
        }
        self.compile_statement(suspension_statement, function)?;
        let first_resume =
            Self::async_statement_exit_state(suspension_statement).ok_or_else(|| {
                EmitError::unsupported("compiler invariant: async loop has no suspension exit")
            })?;
        self.compile_resumable_statement_sequence(after_suspension, first_resume, function)?;
        if fresh.is_some() {
            self.finally_stack.pop();
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
            self.emit_leave_lexical_environment(function);
            self.pop_scope();
            self.emit_save_resumable_environment(function)?;
            self.emit_dispatch_async_completion(function)?;
        }
        if let Some(update) = update {
            let value = schema.reserve_value_local(function);
            self.compile_expr_to_value(update, &value, function)?;
            value.clear(function);
            self.emit_dispatch_async_completion(function)?;
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        if let Some(test) = test {
            self.compile_truthy_i32(test, function)?;
            self.emit_dispatch_async_completion(function)?;
        } else {
            function.instruction(&Instruction::I32Const(1));
        }
        self.open_frame(ControlFrameKind::If, function);
        self.emit_set_resumable_resume_point(*entry_state, function)?;
        let entry_cleanup = fresh.map(|_| self.open_frame(ControlFrameKind::Block, function));
        if let Some(environment) = fresh {
            self.push_scope();
            self.emit_enter_lexical_environment(environment, function)?;
            self.finally_stack.push(ControlTarget {
                environment_depth: self.environment_depth,
                ..entry_cleanup.expect("fresh entered iteration owns cleanup")
            });
            self.emit_save_resumable_environment(function)?;
        }
        self.initialize_direct_lexical_bindings(before_suspension, function);
        self.initialize_direct_lexical_bindings(after_suspension, function);
        for statement in before_suspension {
            self.compile_statement(statement, function)?;
        }
        self.compile_statement(suspension_statement, function)?;
        if fresh.is_some() {
            self.finally_stack.pop();
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
            self.emit_leave_lexical_environment(function);
            self.pop_scope();
            self.emit_save_resumable_environment(function)?;
            self.emit_dispatch_async_completion(function)?;
        }
        function.instruction(&Instruction::Else);
        self.emit_set_resumable_resume_point(*exit_state, function)?;
        self.emit_statement_result(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        frame.clear(function);
        schema.release_i32_local(point, function);
        Ok(())
    }

    pub(crate) fn control_flow_async_generator_activation(
        &self,
        function: &mut Function,
    ) -> Result<GcLocal<AsyncGeneratorActivation>, EmitError> {
        let schema = self.runtime_schema();
        match self
            .body_entry_locals()
            .and_then(|entry| entry.resume_activation())
        {
            Some(crate::function_entry::ResumableEntryLocals::AsyncGenerator(activation)) => {
                Ok(schema
                    .reserve_gc_local(function)
                    .initialize(activation.load(schema, function), function))
            }
            Some(
                crate::function_entry::ResumableEntryLocals::Async(_)
                | crate::function_entry::ResumableEntryLocals::Generator(_),
            )
            | None => Err(EmitError::unsupported(
                "compiler invariant: async-generator statement has no matching activation",
            )),
        }
    }

    pub(crate) fn control_flow_generator_activation(
        &self,
        function: &mut Function,
    ) -> Result<GcLocal<GeneratorActivation>, EmitError> {
        let schema = self.runtime_schema();
        match self
            .body_entry_locals()
            .and_then(|entry| entry.resume_activation())
        {
            Some(crate::function_entry::ResumableEntryLocals::Generator(activation)) => Ok(schema
                .reserve_gc_local(function)
                .initialize(activation.load(schema, function), function)),
            Some(
                crate::function_entry::ResumableEntryLocals::Async(_)
                | crate::function_entry::ResumableEntryLocals::AsyncGenerator(_),
            )
            | None => Err(EmitError::unsupported(
                "compiler invariant: generator statement has no matching activation",
            )),
        }
    }

    fn control_flow_async_activation(
        &self,
        function: &mut Function,
    ) -> Result<GcLocal<AsyncActivation>, EmitError> {
        let schema = self.runtime_schema();
        match self
            .body_entry_locals()
            .and_then(|entry| entry.resume_activation())
        {
            Some(crate::function_entry::ResumableEntryLocals::Async(activation)) => Ok(schema
                .reserve_gc_local(function)
                .initialize(activation.load(schema, function), function)),
            Some(
                crate::function_entry::ResumableEntryLocals::Generator(_)
                | crate::function_entry::ResumableEntryLocals::AsyncGenerator(_),
            )
            | None => Err(EmitError::unsupported(
                "compiler invariant: async statement has no matching activation",
            )),
        }
    }

    fn emit_resumable_state_equals(
        &self,
        state: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let point = self.emit_resumable_resume_point(function)?;
        point.load(function);
        function.instruction(&Instruction::I32Const(state as i32));
        function.instruction(&Instruction::I32Eq);
        schema.release_i32_local(point, function);
        Ok(())
    }

    pub(crate) fn emit_set_resumable_resume_point(
        &self,
        state: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let point = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(state as i32));
        point.store(function);
        self.emit_set_resumable_resume_point_local(point, function)?;
        schema.release_i32_local(point, function);
        Ok(())
    }
    fn emit_set_resumable_resume_point_local(
        &self,
        point: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let activation = self
            .body_entry_locals()
            .and_then(|entry| entry.resume_activation())
            .ok_or_else(|| {
                EmitError::unsupported(
                    "compiler invariant: state publication has no resumable activation",
                )
            })?;
        match activation {
            crate::function_entry::ResumableEntryLocals::Generator(activation) => schema
                .struct_type::<GeneratorActivation>()
                .field(GeneratorActivationSchema::RESUME_POINT)
                .write(activation, GcOperand::i32_local(point), schema, function),
            crate::function_entry::ResumableEntryLocals::Async(activation) => schema
                .struct_type::<AsyncActivation>()
                .field(AsyncActivationSchema::RESUME_POINT)
                .write(activation, GcOperand::i32_local(point), schema, function),
            crate::function_entry::ResumableEntryLocals::AsyncGenerator(activation) => schema
                .struct_type::<AsyncGeneratorActivation>()
                .field(AsyncGeneratorActivationSchema::RESUME_POINT)
                .write(activation, GcOperand::i32_local(point), schema, function),
        }
        Ok(())
    }

    pub(crate) fn emit_save_resumable_environment(
        &self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let frame = self.pending_completion_frame()?;
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::LEXICAL_ENVIRONMENT)
            .write(
                frame,
                GcOperand::reference(self.current_environment(), schema),
                schema,
                function,
            );
        Ok(())
    }

    fn compile_resumable_generator_if(
        &mut self,
        statement: &StatementIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let StatementIr::GeneratorIf {
            condition,
            then_before_yield,
            then_yield_statement,
            then_after_yield,
            else_before_yield,
            else_yield_statement,
            else_after_yield,
            entry_state,
            then_resume_state,
            else_resume_state,
            exit_state,
        } = statement
        else {
            return Err(EmitError::unsupported(
                "compiler invariant: resumable branch requires its checked plan",
            ));
        };
        let schema = self.runtime_schema();
        let point = self.emit_resumable_resume_point(function)?;
        point.load(function);
        function.instruction(&Instruction::I32Const(*entry_state as i32));
        function.instruction(&Instruction::I32Eq);
        for state in [then_resume_state, else_resume_state].into_iter().flatten() {
            point.load(function);
            function.instruction(&Instruction::I32Const(*state as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
        }
        self.open_frame(ControlFrameKind::If, function);
        point.load(function);
        function.instruction(&Instruction::I32Const(*entry_state as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.compile_truthy_i32(condition, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.open_frame(ControlFrameKind::If, function);
        self.compile_resumable_generator_if_initial_branch(
            then_before_yield,
            then_after_yield,
            then_yield_statement.as_deref(),
            *exit_state,
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.compile_resumable_generator_if_initial_branch(
            else_before_yield,
            else_after_yield,
            else_yield_statement.as_deref(),
            *exit_state,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        for (resume_state, yielded, after) in [
            (
                *then_resume_state,
                then_yield_statement.as_deref(),
                then_after_yield.as_slice(),
            ),
            (
                *else_resume_state,
                else_yield_statement.as_deref(),
                else_after_yield.as_slice(),
            ),
        ] {
            if let Some(resume_state) = resume_state {
                point.load(function);
                function.instruction(&Instruction::I32Const(resume_state as i32));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                if let Some(yielded) = yielded {
                    self.compile_statement(yielded, function)?;
                }
                self.compile_resumable_statement_sequence(after, resume_state, function)?;
                self.emit_set_resumable_resume_point(*exit_state, function)?;
                self.emit_statement_result(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(point, function);
        Ok(())
    }

    fn compile_resumable_generator_if_initial_branch(
        &mut self,
        before: &[StatementIr],
        after: &[StatementIr],
        yielded: Option<&StatementIr>,
        exit_state: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.initialize_direct_lexical_bindings(before, function);
        self.initialize_direct_lexical_bindings(after, function);
        for statement in before {
            self.compile_statement(statement, function)?;
        }
        if let Some(yielded) = yielded {
            let StatementIr::GeneratorYield { suspend_state, .. } = yielded else {
                return Err(EmitError::unsupported(
                    "compiler invariant: resumable branch must own a direct yield",
                ));
            };
            self.emit_set_resumable_resume_point(*suspend_state, function)?;
            self.compile_statement(yielded, function)?;
        } else {
            self.emit_set_resumable_resume_point(exit_state, function)?;
            self.emit_statement_result(function);
        }
        Ok(())
    }

    fn compile_async_generator_yield(
        &mut self,
        value: &TypedExpr,
        form: &YieldForm,
        suspend_state: u32,
        resume_state: u32,
        resume_mode: &GeneratorResumeModeIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match form {
            YieldForm::Plain => {}
            YieldForm::Delegate(_) => {
                if self.has_generator_statement_list_value() {
                    self.emit_resumable_state_equals(suspend_state, function)?;
                    self.open_frame(ControlFrameKind::If, function);
                    self.emit_checkpoint_generator_statement_list_value(function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                return self.compile_async_generator_delegation(
                    value,
                    suspend_state,
                    resume_state,
                    resume_mode,
                    AsyncGeneratorDelegationKind::YieldStar,
                    function,
                );
            }
        }
        let schema = self.runtime_schema();
        let activation = self.control_flow_async_generator_activation(function)?;
        let yielded = schema.reserve_value_local(function);
        let resumed = schema.reserve_value_local(function);
        let kind = schema.reserve_i32_local(function);
        let row = schema.struct_type::<AsyncGeneratorActivation>();
        self.emit_resumable_state_equals(suspend_state, function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_checkpoint_generator_statement_list_value(function);
        if let GeneratorResumeModeIr::AssignProperty(reference) = resume_mode {
            self.prepare_suspended_property_reference(reference, function)?;
        }
        self.compile_expr_to_value(value, &yielded, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_set_resumable_resume_point(resume_state, function)?;
        self.emit_save_resumable_environment(function)?;
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&yielded, function),
            function,
        );
        row.field(AsyncGeneratorActivationSchema::BODY_RESULT)
            .write(
                &activation,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
        stored.clear(function);
        row.field(AsyncGeneratorActivationSchema::BODY_STATUS)
            .write(
                &activation,
                GcOperand::constant(AsyncGeneratorBodyStatus::Yield),
                schema,
                function,
            );
        // Plain Yield first awaits its value. The body scheduler keeps this
        // activation Executing until that Await settles; only completing the
        // yield may publish SuspendedYield and admit another queued request.
        self.completion().set_normal(&yielded, function);
        self.emit_return_current_completion(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.emit_resumable_state_equals(resume_state, function)?;
        self.open_frame(ControlFrameKind::If, function);
        row.field(AsyncGeneratorActivationSchema::RESUME_KIND)
            .read(&activation, schema, function)
            .store(kind, function);
        let stored = schema.reserve_gc_local(function).initialize(
            row.field(AsyncGeneratorActivationSchema::RESUME_VALUE)
                .read(&activation, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &resumed, schema, function);
        stored.clear(function);
        self.completion().set_normal(&resumed, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorResumeKind::Return,
        )));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.set_completion_kind(CompletionKind::Return, function);
        self.emit_mark_async_generator_return_awaited(&activation, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorResumeKind::Throw,
        )));
        function.instruction(&Instruction::I32Eq);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorResumeKind::Reject,
        )));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.set_completion_kind(CompletionKind::Throw, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        if let GeneratorResumeModeIr::AssignProperty(_) = resume_mode {
            self.completion().kind().load(function);
            function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
            function.instruction(&Instruction::I32Ne);
            self.open_frame(ControlFrameKind::If, function);
            self.clear_suspended_property_reference(function)?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.emit_dispatch_async_generator_completion(function);
        match resume_mode {
            GeneratorResumeModeIr::Ignore => {
                if !self.has_generator_statement_list_value() {
                    self.emit_statement_result(function);
                }
            }
            GeneratorResumeModeIr::Return => {
                self.set_completion_kind(CompletionKind::Return, function);
                self.emit_dispatch_async_generator_completion(function);
            }
            GeneratorResumeModeIr::AssignIdentifier(name) => {
                self.emit_resumed_binding_assignment(name, &resumed, function)?;
                self.emit_statement_result(function);
            }
            GeneratorResumeModeIr::AssignGlobal { name, strictness } => {
                self.emit_resumed_global_assignment(name, &resumed, *strictness, function)?;
                self.emit_statement_result(function);
            }
            GeneratorResumeModeIr::AssignProperty(reference) => {
                self.write_suspended_property_reference(reference, &resumed, function)?;
                self.emit_statement_result(function);
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind, function);
        resumed.clear(function);
        yielded.clear(function);
        activation.clear(function);
        Ok(())
    }

    fn compile_async_generator_await(
        &mut self,
        value: &TypedExpr,
        suspend_state: u32,
        resume_state: u32,
        resume_mode: &AsyncResumeModeIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let activation = self.control_flow_async_generator_activation(function)?;
        let awaited = schema.reserve_value_local(function);
        let resumed = schema.reserve_value_local(function);
        let kind = schema.reserve_i32_local(function);
        let row = schema.struct_type::<AsyncGeneratorActivation>();
        self.emit_resumable_state_equals(suspend_state, function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_checkpoint_generator_statement_list_value(function);
        self.compile_expr_to_value(value, &awaited, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_set_resumable_resume_point(resume_state, function)?;
        self.emit_save_resumable_environment(function)?;
        self.emit_async_generator_await_reactions(&activation, &awaited, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        row.field(AsyncGeneratorActivationSchema::BODY_STATUS)
            .write(
                &activation,
                GcOperand::constant(AsyncGeneratorBodyStatus::Await),
                schema,
                function,
            );
        row.field(AsyncGeneratorActivationSchema::EXECUTION_STATE)
            .write(
                &activation,
                GcOperand::constant(AsyncGeneratorExecutionState::Executing),
                schema,
                function,
            );
        self.completion().set_normal(&awaited, function);
        self.emit_return_current_completion(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.emit_resumable_state_equals(resume_state, function)?;
        self.open_frame(ControlFrameKind::If, function);
        row.field(AsyncGeneratorActivationSchema::RESUME_KIND)
            .read(&activation, schema, function)
            .store(kind, function);
        let stored = schema.reserve_gc_local(function).initialize(
            row.field(AsyncGeneratorActivationSchema::RESUME_VALUE)
                .read(&activation, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &resumed, schema, function);
        stored.clear(function);
        self.completion().set_normal(&resumed, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorResumeKind::Reject,
        )));
        function.instruction(&Instruction::I32Eq);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorResumeKind::Throw,
        )));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.set_completion_kind(CompletionKind::Throw, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorResumeKind::Return,
        )));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.set_completion_kind(CompletionKind::Return, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_dispatch_async_generator_completion(function);
        match resume_mode {
            AsyncResumeModeIr::Ignore => {
                if !self.has_generator_statement_list_value() {
                    self.emit_statement_result(function);
                }
            }
            AsyncResumeModeIr::Return => {
                self.emit_mark_async_generator_return_awaited(&activation, function);
                self.set_completion_kind(CompletionKind::Return, function);
                self.emit_dispatch_async_generator_completion(function);
            }
            AsyncResumeModeIr::AssignIdentifier(name) => {
                self.emit_resumed_binding_assignment(name, &resumed, function)?;
                self.emit_statement_result(function);
            }
            AsyncResumeModeIr::AssignGlobal { name, strictness } => {
                self.emit_resumed_global_assignment(name, &resumed, *strictness, function)?;
                self.emit_statement_result(function);
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind, function);
        resumed.clear(function);
        awaited.clear(function);
        activation.clear(function);
        Ok(())
    }

    fn compile_plain_generator_yield(
        &mut self,
        value: &TypedExpr,
        form: &YieldForm,
        suspend_state: u32,
        resume_state: u32,
        resume_mode: &GeneratorResumeModeIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match form {
            YieldForm::Plain => {}
            YieldForm::Delegate(_) => {
                if self.has_generator_statement_list_value() {
                    self.emit_resumable_state_equals(suspend_state, function)?;
                    self.open_frame(ControlFrameKind::If, function);
                    self.emit_checkpoint_generator_statement_list_value(function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                return self.compile_generator_delegation(
                    value,
                    suspend_state,
                    resume_state,
                    resume_mode,
                    function,
                );
            }
        }
        let schema = self.runtime_schema();
        let activation = self.control_flow_generator_activation(function)?;
        let yielded = schema.reserve_value_local(function);
        let resumed = schema.reserve_value_local(function);
        let kind = schema.reserve_i32_local(function);
        let row = schema.struct_type::<GeneratorActivation>();
        self.emit_resumable_state_equals(suspend_state, function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_checkpoint_generator_statement_list_value(function);
        if let GeneratorResumeModeIr::AssignProperty(reference) = resume_mode {
            self.prepare_suspended_property_reference(reference, function)?;
        }
        self.compile_expr_to_value(value, &yielded, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_set_resumable_resume_point(resume_state, function)?;
        self.emit_save_resumable_environment(function)?;
        row.field(GeneratorActivationSchema::STATUS).write(
            &activation,
            GcOperand::constant(GeneratorState::SuspendedYield),
            schema,
            function,
        );
        self.completion().set_normal(&yielded, function);
        self.emit_return_current_completion(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_resumable_state_equals(resume_state, function)?;
        self.open_frame(ControlFrameKind::If, function);
        row.field(GeneratorActivationSchema::RESUME_KIND)
            .read(&activation, schema, function)
            .store(kind, function);
        let stored = schema.reserve_gc_local(function).initialize(
            row.field(GeneratorActivationSchema::RESUME_VALUE)
                .read(&activation, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &resumed, schema, function);
        stored.clear(function);
        self.completion().set_normal(&resumed, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            GeneratorResumeKind::Return,
        )));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.set_completion_kind(CompletionKind::Return, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            GeneratorResumeKind::Throw,
        )));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.set_completion_kind(CompletionKind::Throw, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        if let GeneratorResumeModeIr::AssignProperty(_) = resume_mode {
            self.completion().kind().load(function);
            function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
            function.instruction(&Instruction::I32Ne);
            self.open_frame(ControlFrameKind::If, function);
            self.clear_suspended_property_reference(function)?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.emit_dispatch_current_completion(function)?;
        match resume_mode {
            GeneratorResumeModeIr::Ignore => {
                if !self.has_generator_statement_list_value() {
                    self.emit_statement_result(function);
                }
            }
            GeneratorResumeModeIr::Return => {
                self.set_completion_kind(CompletionKind::Return, function);
                self.emit_dispatch_current_completion(function)?;
            }
            GeneratorResumeModeIr::AssignIdentifier(name) => {
                self.emit_resumed_binding_assignment(name, &resumed, function)?;
                self.emit_statement_result(function);
            }
            GeneratorResumeModeIr::AssignGlobal { name, strictness } => {
                self.emit_resumed_global_assignment(name, &resumed, *strictness, function)?;
                self.emit_statement_result(function);
            }
            GeneratorResumeModeIr::AssignProperty(reference) => {
                self.write_suspended_property_reference(reference, &resumed, function)?;
                self.emit_statement_result(function);
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind, function);
        resumed.clear(function);
        yielded.clear(function);
        activation.clear(function);
        Ok(())
    }

    fn compile_plain_async_await(
        &mut self,
        value: &TypedExpr,
        suspend_state: u32,
        resume_state: u32,
        resume_mode: &AsyncResumeModeIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let activation = self.control_flow_async_activation(function)?;
        let awaited = schema.reserve_value_local(function);
        let resumed = schema.reserve_value_local(function);
        let kind = schema.reserve_i32_local(function);
        let row = schema.struct_type::<AsyncActivation>();
        self.emit_resumable_state_equals(suspend_state, function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_checkpoint_generator_statement_list_value(function);
        self.compile_expr_to_value(value, &awaited, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_set_resumable_resume_point(resume_state, function)?;
        if matches!(
            value.expr,
            ExprIr::ModuleEvaluate(_) | ExprIr::ModuleDeferredImportEvaluate(_)
        ) {
            let promise = schema.reserve_gc_local(function).initialize(
                awaited.cast_reference::<PromiseObject>(schema, function),
                function,
            );
            self.emit_module_import_await_reactions(&activation, &promise, function)?;
            promise.clear(function);
        } else {
            self.emit_async_await_reactions(&activation, &awaited, function)?;
        }
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_return_async_suspension(function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_resumable_state_equals(resume_state, function)?;
        self.open_frame(ControlFrameKind::If, function);
        row.field(AsyncActivationSchema::RESUME_COMPLETION)
            .read(&activation, schema, function)
            .store(kind, function);
        let stored = schema.reserve_gc_local(function).initialize(
            row.field(AsyncActivationSchema::RESUME_VALUE)
                .read(&activation, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &resumed, schema, function);
        stored.clear(function);
        self.completion().set_normal(&resumed, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AwaitCompletionKind::Throw,
        )));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.set_completion_kind(CompletionKind::Throw, function);
        self.emit_dispatch_current_completion(function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        match resume_mode {
            AsyncResumeModeIr::Ignore => {
                if !self.has_generator_statement_list_value() {
                    self.emit_statement_result(function);
                }
            }
            AsyncResumeModeIr::Return => {
                self.set_completion_kind(CompletionKind::Return, function);
                self.emit_dispatch_current_completion(function)?;
            }
            AsyncResumeModeIr::AssignIdentifier(name) => {
                self.emit_resumed_binding_assignment(name, &resumed, function)?;
                self.emit_statement_result(function);
            }
            AsyncResumeModeIr::AssignGlobal { name, strictness } => {
                self.emit_resumed_global_assignment(name, &resumed, *strictness, function)?;
                self.emit_statement_result(function);
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind, function);
        resumed.clear(function);
        awaited.clear(function);
        activation.clear(function);
        Ok(())
    }

    pub(crate) fn compile_statement(
        &mut self,
        statement: &StatementIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if self.current_function_meta().is_some_and(|meta| {
            meta.protocol().execution_kind() == FunctionExecutionKind::AsyncGenerator
        }) {
            match statement {
                StatementIr::GeneratorYield {
                    value,
                    form,
                    suspend_state,
                    resume_state,
                    resume_mode,
                } => {
                    return self.compile_async_generator_yield(
                        value,
                        form,
                        *suspend_state,
                        *resume_state,
                        resume_mode,
                        function,
                    );
                }
                StatementIr::AsyncAwait {
                    value,
                    suspend_state,
                    resume_state,
                    resume_mode,
                } => {
                    return self.compile_async_generator_await(
                        value,
                        *suspend_state,
                        *resume_state,
                        resume_mode,
                        function,
                    );
                }
                StatementIr::GeneratorLoop { .. } => {
                    return self.compile_resumable_async_loop(statement, function);
                }
                StatementIr::GeneratorIf { .. } => {
                    return self.compile_resumable_generator_if(statement, function);
                }
                _ => {}
            }
        }

        // A plain async function reaches its loop bodies through the same
        // one-iteration-per-invocation state machine; only the activation slot
        // holding the resume state differs.
        if matches!(statement, StatementIr::GeneratorLoop { .. })
            && self.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() == FunctionExecutionKind::Async
            })
        {
            return self.compile_resumable_async_loop(statement, function);
        }

        match statement {
            StatementIr::ResumableClassDefinition(plan) => {
                self.compile_resumable_class_definition(plan, function)?
            }
            StatementIr::ModuleImportBinding(import) => {
                self.allocate_binding(
                    import.name.clone(),
                    BindingMode::Const,
                    ValueKind::Dynamic,
                    function,
                );
                self.emit_module_import_binding(import, function)?;
            }
            StatementIr::ModuleUnitOnce { module, block } => {
                self.emit_module_unit_once(*module, block, function)?;
            }
            StatementIr::Empty => {}
            StatementIr::Lexical { mode, name, init } => {
                if let ExprIr::CaptureArgumentList(capture) = &init.expr {
                    let schema = self.runtime_schema();
                    let saved = schema.reserve_completion(function);
                    saved.copy_from(self.completion(), function);
                    let storage = self
                        .lookup_current_scope_binding(name)
                        .or_else(|| self.lookup_binding(name))
                        .unwrap_or_else(|| {
                            self.allocate_binding(name.clone(), *mode, ValueKind::Dynamic, function)
                        });
                    let arguments = self.emit_argument_list_capture(capture, function)?;
                    self.emit_store_captured_argument_list(storage, &arguments, function)?;
                    arguments.clear(function);
                    self.completion().copy_from(&saved, function);
                    saved.clear(function);
                    return Ok(());
                }
                let saved = self.save_statement_list_value(function);
                let storage = self
                    .lookup_current_scope_binding(name)
                    .or_else(|| self.lookup_binding(name))
                    .unwrap_or_else(|| {
                        self.allocate_binding(name.clone(), *mode, init.kind, function)
                    });
                self.initialize_binding_uninitialized(storage, function);
                let value = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(init, &value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.write_binding_from_locals(storage, &value, function);
                value.clear(function);
                self.restore_statement_list_value(saved, function)?;
            }
            StatementIr::SyncDisposableScope {
                execution,
                resources,
                body,
            } => {
                self.compile_sync_disposable_scope(execution, resources, body, function)?;
            }
            StatementIr::AsyncDisposableScope {
                execution,
                resources,
                body,
            } => {
                self.compile_async_disposable_scope(execution, resources, body, function)?;
            }
            StatementIr::AnnexBFunctionCopy {
                source_name,
                block_storage_name,
                target,
                admission,
            } => {
                self.compile_annex_b_function_copy(
                    source_name,
                    block_storage_name,
                    target,
                    admission,
                    function,
                )?;
            }
            StatementIr::DeclarationEvaluation(expression) => {
                let saved = self.save_statement_list_value(function);
                let value = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(expression, &value, function)?;
                value.clear(function);
                self.restore_statement_list_value(saved, function)?;
            }
            StatementIr::Expression(expr) => {
                let value = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(expr, &value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.completion().value().copy_from(&value, function);
                self.set_completion_kind(CompletionKind::Normal, function);
                value.clear(function);
            }
            StatementIr::GeneratorYield {
                value,
                form,
                suspend_state,
                resume_state,
                resume_mode,
            } => {
                self.compile_plain_generator_yield(
                    value,
                    form,
                    *suspend_state,
                    *resume_state,
                    resume_mode,
                    function,
                )?;
            }
            StatementIr::AsyncModuleInstantiation => {
                if !self.current_function_meta().is_some_and(|meta| {
                    meta.protocol() == FunctionProtocolIr::AsyncModuleActivation
                }) {
                    return Err(EmitError::unsupported(
                        "compiler invariant: module instantiation has no async module entry",
                    ));
                }
                let schema = self.runtime_schema();
                let activation = self.control_flow_async_activation(function)?;
                self.emit_resumable_state_equals(0, function)?;
                self.open_frame(ControlFrameKind::If, function);
                schema
                    .struct_type::<AsyncActivation>()
                    .field(AsyncActivationSchema::MODULE_ENTRY_MODE)
                    .read(&activation, schema, function);
                function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                    AsyncModuleEntryMode::Instantiate,
                )));
                function.instruction(&Instruction::I32Ne);
                self.open_frame(ControlFrameKind::If, function);
                function.instruction(&Instruction::Unreachable);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.emit_set_resumable_resume_point(1, function)?;
                self.emit_save_resumable_environment(function)?;
                schema
                    .struct_type::<AsyncActivation>()
                    .field(AsyncActivationSchema::MODULE_ENTRY_MODE)
                    .write(
                        &activation,
                        GcOperand::constant(AsyncModuleEntryMode::Execute),
                        schema,
                        function,
                    );
                self.emit_return_async_suspension(function)?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                activation.clear(function);
            }
            StatementIr::AsyncAwait {
                value,
                suspend_state,
                resume_state,
                resume_mode,
            } => {
                self.compile_plain_async_await(
                    value,
                    *suspend_state,
                    *resume_state,
                    resume_mode,
                    function,
                )?;
            }
            StatementIr::OrdinaryGeneratorLoop(plan) => {
                self.compile_ordinary_generator_loop(plan, &[], function)?;
            }
            StatementIr::AsyncGeneratorLoop(plan) => {
                self.compile_async_generator_loop(plan, &[], function)?;
            }
            StatementIr::AsyncGeneratorIf(plan) => {
                self.compile_async_generator_if(plan, function)?;
            }
            StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => {
                self.compile_ordinary_generator_array_destructuring(plan, function)?;
            }
            StatementIr::AsyncFunctionArrayDestructuring(plan) => {
                self.compile_async_function_array_destructuring(plan, function)?;
            }
            StatementIr::AsyncGeneratorArrayDestructuring(plan) => {
                self.compile_async_generator_array_destructuring(plan, function)?;
            }
            StatementIr::OrdinaryGeneratorWith(plan) => {
                self.compile_ordinary_generator_with(plan, function)?;
            }
            StatementIr::AsyncFunctionWith(plan) => {
                self.compile_async_function_with(plan, function)?;
            }
            StatementIr::AsyncGeneratorWith(plan) => {
                self.compile_async_generator_with(plan, function)?;
            }
            StatementIr::AsyncGeneratorForIn(plan) => {
                self.compile_async_generator_for_in(plan, &[], function)?;
            }
            StatementIr::AsyncGeneratorForOf(plan) => {
                self.compile_async_generator_for_of(plan, &[], function)?;
            }
            StatementIr::AsyncGeneratorResourceScope(plan) => {
                self.compile_async_generator_resource_scope(plan, function)?;
            }
            StatementIr::AsyncGeneratorResourceRegistration(registration) => {
                self.compile_async_generator_resource_registration(registration, function)?;
            }
            StatementIr::ArrayDestructuringOperation(operation) => {
                self.compile_array_destructuring_operation_statement(operation, function)?;
            }
            StatementIr::OrdinaryGeneratorSwitch(plan) => {
                self.compile_ordinary_generator_switch(plan, &[], function)?;
            }
            StatementIr::AsyncGeneratorSwitch(plan) => {
                self.compile_async_generator_switch(plan, &[], function)?;
            }
            StatementIr::EmptyStatementCompletion(item) => {
                self.compile_empty_statement_completion(item.statement(), function)?;
            }
            StatementIr::OrdinaryGeneratorIf(plan) => {
                self.compile_ordinary_generator_if(plan, function)?;
            }
            StatementIr::GeneratorLoop { .. } => {
                return Err(EmitError::unsupported(
                    "compiler invariant: ordinary generator loop requires its checked phase graph",
                ));
            }
            StatementIr::GeneratorIf { .. } => {
                self.compile_resumable_generator_if(statement, function)?;
            }
            StatementIr::Var(declarators) => {
                let saved = self.save_statement_list_value(function);
                self.compile_var_declarators(declarators, function)?;
                self.restore_statement_list_value(saved, function)?;
            }
            StatementIr::ParameterInitialization { .. } => {}
            StatementIr::LexicalBlock(statements) => {
                let entry = match self
                    .current_function_meta()
                    .map(|meta| meta.protocol().execution_kind())
                {
                    Some(FunctionExecutionKind::Generator) => statements
                        .iter()
                        .find_map(Self::generator_statement_entry_state),
                    Some(FunctionExecutionKind::Async | FunctionExecutionKind::AsyncGenerator) => {
                        statements
                            .iter()
                            .find_map(Self::async_statement_entry_state)
                    }
                    Some(FunctionExecutionKind::Ordinary) | None => None,
                };
                if let Some(entry) = entry {
                    self.emit_resumable_state_equals(entry, function)?;
                    self.open_frame(ControlFrameKind::If, function);
                    self.initialize_direct_lexical_bindings(statements, function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                    self.compile_resumable_statement_sequence(statements, entry, function)?;
                } else {
                    self.initialize_direct_lexical_bindings(statements, function);
                    for statement in statements {
                        self.compile_statement(statement, function)?;
                    }
                }
            }
            StatementIr::Block(block) => {
                self.push_scope();
                self.compile_block_contents(block, function)?;
                self.pop_scope();
            }
            StatementIr::Labelled {
                labels,
                statement,
                async_plan,
            } => match async_plan {
                Some(plan) => {
                    self.compile_async_function_labelled(labels, statement, *plan, function)?
                }
                None => self.compile_labelled_statement(labels, statement, function)?,
            },
            StatementIr::Throw(value) => {
                let thrown = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(value, &thrown, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.emit_clear_throw_diagnostic(
                    crate::module::ThrowDiagnosticRole::Name,
                    function,
                );
                self.emit_clear_throw_diagnostic(
                    crate::module::ThrowDiagnosticRole::Message,
                    function,
                );
                self.emit_capture_throw_error_name(&thrown, function)?;
                self.completion().set_throw(&thrown, function);
                thrown.clear(function);
                self.emit_propagate_current_throw(function);
            }
            StatementIr::TryCatch {
                try_block,
                catch_name,
                catch_source_name,
                catch_parameter_environment,
                catch_block,
                generator_plan,
                async_plan,
            } => {
                if let (Some(async_plan), true) = (async_plan, self.is_async_resume_body()) {
                    self.compile_async_try_catch(
                        try_block,
                        catch_name,
                        catch_source_name,
                        catch_parameter_environment.as_ref(),
                        catch_block,
                        *async_plan,
                        function,
                    )?;
                } else if let Some(generator_plan) = generator_plan {
                    self.compile_generator_try_catch(
                        try_block,
                        catch_name,
                        catch_source_name,
                        catch_parameter_environment.as_ref(),
                        catch_block,
                        *generator_plan,
                        function,
                    )?;
                } else {
                    self.compile_try_catch(
                        try_block,
                        catch_name,
                        catch_source_name,
                        catch_parameter_environment.as_ref(),
                        catch_block,
                        function,
                    )?;
                }
            }
            StatementIr::TryFinally {
                try_block,
                finally_block,
                generator_plan,
                async_plan,
            } => {
                if let (Some(async_plan), true) = (async_plan, self.is_async_resume_body()) {
                    self.compile_async_try_finally(
                        try_block,
                        finally_block,
                        *async_plan,
                        function,
                    )?;
                } else if let Some(generator_plan) = generator_plan {
                    self.compile_generator_try_finally(
                        try_block,
                        finally_block,
                        *generator_plan,
                        function,
                    )?;
                } else {
                    self.compile_try_finally(try_block, finally_block, function)?;
                }
            }
            StatementIr::TryCatchFinally {
                try_block,
                catch_name,
                catch_source_name,
                catch_parameter_environment,
                catch_block,
                finally_block,
                generator_plan,
                async_plan,
            } => {
                if let (Some(async_plan), true) = (async_plan, self.is_async_resume_body()) {
                    self.compile_async_try_catch_finally(
                        try_block,
                        catch_name,
                        catch_source_name,
                        catch_parameter_environment.as_ref(),
                        catch_block,
                        finally_block,
                        *async_plan,
                        function,
                    )?;
                } else if let Some(generator_plan) = generator_plan {
                    self.compile_generator_try_catch_finally(
                        try_block,
                        catch_name,
                        catch_source_name,
                        catch_parameter_environment.as_ref(),
                        catch_block,
                        finally_block,
                        *generator_plan,
                        function,
                    )?;
                } else {
                    self.compile_try_catch_finally(
                        try_block,
                        catch_name,
                        catch_source_name,
                        catch_parameter_environment.as_ref(),
                        catch_block,
                        finally_block,
                        function,
                    )?;
                }
            }
            StatementIr::AsyncFunctionWhile(plan) => {
                self.compile_async_function_while_condition(plan, &[], function)?;
            }
            StatementIr::AsyncFunctionIf {
                condition,
                then_branch,
                else_branch,
                plan,
            } => {
                self.compile_async_function_if(
                    condition,
                    then_branch,
                    else_branch.as_deref(),
                    *plan,
                    function,
                )?;
            }
            StatementIr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                if let Some(taken) = constant_number_condition::truthiness(condition) {
                    // Lowering and planning have already visited both branches.
                    // Preserve If's UpdateEmpty(undefined) before the selected
                    // statement, including an empty branch or no else branch.
                    self.emit_statement_result(function);
                    if taken {
                        self.compile_statement(then_branch, function)?;
                    } else if let Some(else_branch) = else_branch {
                        self.compile_statement(else_branch, function)?;
                    }
                    return Ok(());
                }
                self.compile_truthy_i32(condition, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.emit_statement_result(function);
                self.open_frame(ControlFrameKind::If, function);
                self.compile_statement(then_branch, function)?;
                if let Some(else_branch) = else_branch {
                    function.instruction(&Instruction::Else);
                    self.compile_statement(else_branch, function)?;
                }
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            StatementIr::While { condition, body } => {
                self.compile_while(condition, body, &[], function)?;
            }
            StatementIr::DoWhile { body, condition } => {
                self.compile_do_while(body, condition, &[], function)?;
            }
            StatementIr::For {
                init,
                test,
                update,
                body,
                lexical_environment,
            } => {
                self.compile_for(
                    init.as_ref(),
                    test.as_ref(),
                    update.as_ref(),
                    body,
                    lexical_environment.as_ref(),
                    &[],
                    function,
                )?;
            }
            StatementIr::AsyncFunctionForOfIterator { iterable, plan } => {
                self.compile_async_function_for_of_iterator(iterable, plan, function)?;
            }
            StatementIr::GeneratorForOfIterator { iterable, plan } => {
                self.compile_generator_for_of_iterator(iterable, plan, function)?;
            }
            StatementIr::ForOfIterator {
                head,
                iterable,
                body,
                lexical_environment,
            } => match head {
                ForOfIteratorHeadIr::Assignment {
                    binding,
                    async_plan: Some(async_plan),
                    ..
                } => {
                    if self.current_function_meta().is_some_and(|meta| {
                        meta.protocol().execution_kind() == FunctionExecutionKind::AsyncGenerator
                    }) && async_generator_for_await_is_transparent_yield(&binding.name, body)
                    {
                        self.compile_async_generator_delegation(
                            iterable,
                            async_plan.entry_state,
                            async_plan.exit_state,
                            &GeneratorResumeModeIr::Ignore,
                            AsyncGeneratorDelegationKind::ForAwaitYield,
                            function,
                        )?;
                        return Ok(());
                    }
                    self.compile_async_for_of_iterator(
                        iterable,
                        ForAwaitIteratorPlan::existing(
                            binding,
                            body,
                            lexical_environment.as_ref(),
                            async_plan,
                        ),
                        &[],
                        function,
                    )?;
                }
                ForOfIteratorHeadIr::Assignment {
                    binding,
                    async_plan: None,
                    ..
                } => {
                    self.compile_for_of_iterator(
                        SyncForOfIteratorHead::Assignment(binding),
                        iterable,
                        body,
                        lexical_environment.as_ref(),
                        &[],
                        function,
                    )?;
                }
                ForOfIteratorHeadIr::SyncDisposable(head) => {
                    self.compile_for_of_iterator(
                        SyncForOfIteratorHead::SyncDisposable {
                            head,
                            body: SynchronousLoopBodyIr::new(body).map_err(|error| {
                                EmitError::unsupported(format!(
                                    "invalid synchronous resource loop body: {error:?}"
                                ))
                            })?,
                        },
                        iterable,
                        body,
                        lexical_environment.as_ref(),
                        &[],
                        function,
                    )?;
                }
                ForOfIteratorHeadIr::AsyncDisposable(head) => {
                    self.compile_async_disposable_for_of_iterator(
                        head,
                        iterable,
                        body,
                        lexical_environment.as_ref(),
                        &[],
                        function,
                    )?;
                }
            },
            StatementIr::ForInArray {
                mode,
                name,
                target,
                body,
                lexical_environment,
            } => self.compile_for_in(
                *mode,
                name,
                target,
                body,
                lexical_environment.as_ref(),
                &[],
                function,
            )?,
            StatementIr::ForInString {
                mode,
                name,
                target,
                body,
                lexical_environment,
            } => self.compile_for_in(
                *mode,
                name,
                target,
                body,
                lexical_environment.as_ref(),
                &[],
                function,
            )?,
            StatementIr::ForInObject {
                mode,
                name,
                target,
                body,
                lexical_environment,
            } => self.compile_for_in(
                *mode,
                name,
                target,
                body,
                lexical_environment.as_ref(),
                &[],
                function,
            )?,
            StatementIr::AsyncFunctionSwitch(plan) => {
                self.compile_async_function_switch(plan, &[], function)?;
            }
            StatementIr::Switch {
                discriminant,
                lexical_environment,
                lexical_declarations,
                cases,
            } => {
                self.compile_switch(
                    discriminant,
                    lexical_environment.as_ref(),
                    lexical_declarations,
                    cases,
                    &[],
                    function,
                )?;
            }
            StatementIr::Debugger => {}
            StatementIr::Return(expression) => {
                if self.throw_handler_stack.is_empty() && self.finally_stack.is_empty() {
                    self.compile_return_position_expr(expression, function)?;
                    return Ok(());
                }
                let value = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(expression, &value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.completion().value().copy_from(&value, function);
                value.clear(function);
                self.emit_prepare_fresh_return(function)?;
                self.set_completion_kind(CompletionKind::Return, function);
                if let Some(target) = self.finally_stack.last().copied() {
                    self.emit_branch_to_target(target, function);
                } else {
                    self.emit_derived_constructor_body_result(function)?;
                    self.emit_return_current_completion(function);
                }
            }
            StatementIr::Break { label } => self.compile_break(label.as_deref(), function)?,
            StatementIr::Continue { label } => self.compile_continue(label.as_deref(), function)?,
        }
        Ok(())
    }

    pub(crate) fn compile_labelled_statement(
        &mut self,
        labels: &[String],
        statement: &StatementIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        // Non-loop async bodies need their checked region owner. Direct loop
        // forms retain their established emitter and completion destinations.
        // An omitted annotation must refuse emission rather than skip a body.
        if self
            .current_function_meta()
            .is_some_and(|meta| meta.protocol().execution_kind() == FunctionExecutionKind::Async)
            && Self::async_statement_entry_state(statement).is_some()
            && Self::labelled_async_linear_loop_states(statement).is_none()
            && Self::labelled_async_function_while_plan(statement).is_none()
            && !Self::labelled_async_generator_loop_plan(statement).is_some_and(|plan| {
                plan.execution() == scriptw_ir::ResumableRegionProtocolIr::Async
            })
            && !Self::labelled_complete_resource_switch_plan(statement).is_some_and(|plan| {
                plan.execution() == scriptw_ir::ResumableRegionProtocolIr::Async
            })
            && Self::labelled_async_function_for_of_plan(statement).is_none()
            && Self::labelled_complete_iterator_states(statement).is_none()
            && Self::labelled_async_disposable_for_finalizer(statement).is_none()
        {
            return Err(EmitError::unsupported(
                "labelled async region is missing its continuation owner",
            ));
        }
        match statement {
            StatementIr::OrdinaryGeneratorLoop(plan) => {
                self.compile_ordinary_generator_loop(plan, labels, function)?;
            }
            StatementIr::AsyncGeneratorLoop(plan) => {
                self.compile_async_generator_loop(plan, labels, function)?;
            }
            StatementIr::AsyncGeneratorForIn(plan) => {
                self.compile_async_generator_for_in(plan, labels, function)?;
            }
            StatementIr::AsyncGeneratorForOf(plan) => {
                self.compile_async_generator_for_of(plan, labels, function)?;
            }
            StatementIr::OrdinaryGeneratorSwitch(plan) => {
                self.compile_ordinary_generator_switch(plan, labels, function)?;
            }
            StatementIr::AsyncGeneratorSwitch(plan) => {
                self.compile_async_generator_switch(plan, labels, function)?;
            }
            StatementIr::Block(block) => {
                let break_frame = self.open_frame(ControlFrameKind::Block, function);
                self.push_labels(labels, break_frame, None);
                self.push_scope();
                self.compile_block_contents(block, function)?;
                self.pop_scope();
                self.pop_labels(labels.len());
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
            }
            StatementIr::AsyncFunctionWhile(plan) => {
                self.compile_async_function_while_condition(plan, labels, function)?;
            }
            StatementIr::While { condition, body } => {
                self.compile_while(condition, body, labels, function)?;
            }
            StatementIr::DoWhile { body, condition } => {
                self.compile_do_while(body, condition, labels, function)?;
            }
            StatementIr::For {
                init,
                test,
                update,
                body,
                lexical_environment,
            } => {
                self.compile_for(
                    init.as_ref(),
                    test.as_ref(),
                    update.as_ref(),
                    body,
                    lexical_environment.as_ref(),
                    labels,
                    function,
                )?;
            }
            StatementIr::ForOfIterator {
                head,
                iterable,
                body,
                lexical_environment,
            } => match head {
                ForOfIteratorHeadIr::Assignment {
                    binding,
                    async_plan: Some(async_plan),
                    ..
                } => {
                    self.compile_async_for_of_iterator(
                        iterable,
                        ForAwaitIteratorPlan::existing(
                            binding,
                            body,
                            lexical_environment.as_ref(),
                            async_plan,
                        ),
                        labels,
                        function,
                    )?;
                }
                ForOfIteratorHeadIr::Assignment {
                    binding,
                    async_plan: None,
                    ..
                } => {
                    self.compile_for_of_iterator(
                        SyncForOfIteratorHead::Assignment(binding),
                        iterable,
                        body,
                        lexical_environment.as_ref(),
                        labels,
                        function,
                    )?;
                }
                ForOfIteratorHeadIr::SyncDisposable(head) => {
                    self.compile_for_of_iterator(
                        SyncForOfIteratorHead::SyncDisposable {
                            head,
                            body: SynchronousLoopBodyIr::new(body).map_err(|error| {
                                EmitError::unsupported(format!(
                                    "invalid synchronous resource loop body: {error:?}"
                                ))
                            })?,
                        },
                        iterable,
                        body,
                        lexical_environment.as_ref(),
                        labels,
                        function,
                    )?;
                }
                ForOfIteratorHeadIr::AsyncDisposable(head) => {
                    self.compile_async_disposable_for_of_iterator(
                        head,
                        iterable,
                        body,
                        lexical_environment.as_ref(),
                        labels,
                        function,
                    )?;
                }
            },
            StatementIr::ForInArray {
                mode,
                name,
                target,
                body,
                lexical_environment,
            } => self.compile_for_in(
                *mode,
                name,
                target,
                body,
                lexical_environment.as_ref(),
                labels,
                function,
            )?,
            StatementIr::ForInString {
                mode,
                name,
                target,
                body,
                lexical_environment,
            } => self.compile_for_in(
                *mode,
                name,
                target,
                body,
                lexical_environment.as_ref(),
                labels,
                function,
            )?,
            StatementIr::ForInObject {
                mode,
                name,
                target,
                body,
                lexical_environment,
            } => self.compile_for_in(
                *mode,
                name,
                target,
                body,
                lexical_environment.as_ref(),
                labels,
                function,
            )?,
            StatementIr::AsyncFunctionSwitch(plan) => {
                self.compile_async_function_switch(plan, labels, function)?;
            }
            StatementIr::Switch {
                discriminant,
                lexical_environment,
                lexical_declarations,
                cases,
            } => {
                self.compile_switch(
                    discriminant,
                    lexical_environment.as_ref(),
                    lexical_declarations,
                    cases,
                    labels,
                    function,
                )?;
            }
            _ => {
                let break_frame = self.open_frame(ControlFrameKind::Block, function);
                self.push_labels(labels, break_frame, None);
                self.compile_statement(statement, function)?;
                self.pop_labels(labels.len());
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
            }
        }
        Ok(())
    }

    pub(crate) fn compile_try_catch(
        &mut self,
        try_block: &BlockIr,
        catch_name: &str,
        catch_source_name: &str,
        catch_parameter_environment: Option<&LexicalEnvironmentIr>,
        catch_block: &BlockIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_statement_result(function);
        let outer_frame = self.open_frame(ControlFrameKind::Block, function);
        let catch_frame = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(catch_frame);
        self.push_scope();
        self.compile_block_contents(try_block, function)?;
        self.pop_scope();
        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        self.emit_branch_to_target(outer_frame, function);
        function.instruction(&Instruction::End);

        self.push_scope();
        if let Some(environment) = catch_parameter_environment {
            self.emit_enter_lexical_environment(environment, function)?;
        }
        let catch_storage = self
            .lookup_current_scope_binding(catch_name)
            .unwrap_or_else(|| self.allocate_dynamic_binding_storage(catch_name, function));
        self.binding_scopes
            .last_mut()
            .expect("binding scope stack must exist")
            .insert(catch_name.to_string(), catch_storage);
        if catch_source_name != catch_name {
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(catch_source_name.to_string(), catch_storage);
        }
        let caught = self.runtime_schema().reserve_value_local(function);
        caught.copy_from(self.completion().value(), function);
        self.write_binding_from_locals(catch_storage, &caught, function);
        caught.clear(function);
        self.emit_statement_result(function);
        self.push_scope();
        self.compile_block_contents(catch_block, function)?;
        self.pop_scope();
        if catch_parameter_environment.is_some() {
            self.emit_leave_lexical_environment(function);
        }
        self.pop_scope();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_resumable_state_in_range(
        &self,
        start_state: u32,
        end_state: u32,
        inclusive_end: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let point = self.emit_resumable_resume_point(function)?;
        point.load(function);
        function.instruction(&Instruction::I32Const(start_state as i32));
        function.instruction(&Instruction::I32GeU);
        point.load(function);
        function.instruction(&Instruction::I32Const(end_state as i32));
        function.instruction(&if inclusive_end {
            Instruction::I32LeU
        } else {
            Instruction::I32LtU
        });
        function.instruction(&Instruction::I32And);
        schema.release_i32_local(point, function);
        Ok(())
    }

    fn emit_generator_state_in_range(
        &self,
        start_state: u32,
        end_state: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_resumable_state_in_range(start_state, end_state, false, function)
    }

    fn emit_async_state_in_range(
        &self,
        start_state: u32,
        end_state: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_resumable_state_in_range(start_state, end_state, false, function)
    }

    fn emit_async_try_state_in_range(
        &self,
        start_state: u32,
        end_state: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let inclusive = self.current_function_meta().is_some_and(|meta| {
            meta.protocol().execution_kind() == FunctionExecutionKind::AsyncGenerator
        });
        self.emit_resumable_state_in_range(start_state, end_state, inclusive, function)
    }

    fn emit_resumable_finalizer_needs_pending_completion(
        &self,
        finally_entry_state: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let point = self.emit_resumable_resume_point(function)?;
        point.load(function);
        function.instruction(&Instruction::I32Const(finally_entry_state as i32));
        let inclusive = self.current_function_meta().is_some_and(|meta| {
            meta.protocol().execution_kind() == FunctionExecutionKind::AsyncGenerator
        });
        function.instruction(&if inclusive {
            Instruction::I32LeU
        } else {
            Instruction::I32LtU
        });
        schema.release_i32_local(point, function);
        Ok(())
    }

    fn compile_generator_try_catch(
        &mut self,
        try_block: &BlockIr,
        catch_name: &str,
        catch_source_name: &str,
        catch_parameter_environment: Option<&LexicalEnvironmentIr>,
        catch_block: &BlockIr,
        generator_plan: GeneratorTryPlanIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let catch_entry_state = generator_plan.catch_entry_state.ok_or_else(|| {
            EmitError::unsupported("generator try/catch is missing its catch resume state")
        })?;
        let catch_exit_state = generator_plan.catch_exit_state.ok_or_else(|| {
            EmitError::unsupported("generator try/catch is missing its catch exit state")
        })?;

        self.emit_generator_state_in_range(
            generator_plan.entry_state,
            generator_plan.exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        let outer_frame = self.open_frame(ControlFrameKind::Block, function);
        let catch_frame = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(catch_frame);

        self.emit_generator_state_in_range(
            generator_plan.entry_state,
            generator_plan.try_exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.push_scope();
        self.compile_resumable_block_contents(
            try_block,
            generator_plan.entry_state,
            true,
            function,
        )?;
        self.pop_scope();
        self.emit_set_resumable_resume_point(generator_plan.exit_state, function)?;
        self.emit_branch_to_target(outer_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.push_scope();
        let thrown = self.runtime_schema().reserve_value_local(function);
        thrown.copy_from(self.completion().value(), function);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_set_resumable_resume_point(catch_entry_state, function)?;
        function.instruction(&Instruction::End);
        if let Some(environment) = catch_parameter_environment {
            self.emit_enter_resumable_lexical_environment(
                environment,
                catch_entry_state,
                function,
            )?;
        }
        let catch_storage = self
            .lookup_current_scope_binding(catch_name)
            .unwrap_or_else(|| self.allocate_dynamic_binding_storage(catch_name, function));
        self.binding_scopes
            .last_mut()
            .expect("binding scope stack must exist")
            .insert(catch_name.to_string(), catch_storage);
        if catch_source_name != catch_name {
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(catch_source_name.to_string(), catch_storage);
        }

        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.write_binding_from_locals(catch_storage, &thrown, function);
        self.emit_statement_result(function);
        function.instruction(&Instruction::End);
        thrown.clear(function);

        self.push_scope();
        self.compile_resumable_block_contents(catch_block, catch_entry_state, true, function)?;
        self.pop_scope();
        if catch_parameter_environment.is_some() {
            self.emit_leave_lexical_environment(function);
        }
        self.pop_scope();
        self.emit_set_resumable_resume_point(generator_plan.exit_state, function)?;

        debug_assert_eq!(catch_exit_state, generator_plan.exit_state);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_generator_try_finally(
        &mut self,
        try_block: &BlockIr,
        finally_block: &BlockIr,
        generator_plan: GeneratorTryPlanIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let finally_entry_state = generator_plan.finally_entry_state.ok_or_else(|| {
            EmitError::unsupported("generator try/finally is missing its finalizer resume state")
        })?;
        let finally_exit_state = generator_plan.finally_exit_state.ok_or_else(|| {
            EmitError::unsupported("generator try/finally is missing its finalizer exit state")
        })?;

        self.emit_generator_state_in_range(
            generator_plan.entry_state,
            generator_plan.exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.open_frame(ControlFrameKind::Block, function);
        let finally_entry_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(finally_entry_frame);

        self.emit_generator_state_in_range(
            generator_plan.entry_state,
            generator_plan.try_exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.push_scope();
        self.compile_resumable_block_contents(
            try_block,
            generator_plan.entry_state,
            true,
            function,
        )?;
        self.pop_scope();
        self.emit_branch_to_target(finally_entry_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.emit_resumable_finalizer_needs_pending_completion(finally_entry_state, function)?;
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_push_generator_pending_completion(function)?;
        self.emit_statement_result(function);
        self.emit_set_resumable_resume_point(finally_entry_state, function)?;
        function.instruction(&Instruction::End);

        let finalizer_epilogue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(finalizer_epilogue_frame);
        self.generator_finalizer_depth += 1;
        self.push_scope();
        self.compile_resumable_block_contents(finally_block, finally_entry_state, true, function)?;
        self.pop_scope();
        self.generator_finalizer_depth -= 1;
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_pop_and_restore_generator_pending_completion(function)?;
        function.instruction(&Instruction::Else);
        self.emit_discard_generator_pending_completion(function)?;
        function.instruction(&Instruction::End);
        self.emit_set_resumable_resume_point(generator_plan.exit_state, function)?;
        self.emit_dispatch_current_completion(function)?;

        debug_assert_eq!(finally_exit_state, generator_plan.exit_state);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_generator_try_catch_finally(
        &mut self,
        try_block: &BlockIr,
        catch_name: &str,
        catch_source_name: &str,
        catch_parameter_environment: Option<&LexicalEnvironmentIr>,
        catch_block: &BlockIr,
        finally_block: &BlockIr,
        generator_plan: GeneratorTryPlanIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let catch_entry_state = generator_plan.catch_entry_state.ok_or_else(|| {
            EmitError::unsupported("generator try/catch is missing its catch resume state")
        })?;
        let catch_exit_state = generator_plan.catch_exit_state.ok_or_else(|| {
            EmitError::unsupported("generator try/catch is missing its catch exit state")
        })?;
        let finally_entry_state = generator_plan.finally_entry_state.ok_or_else(|| {
            EmitError::unsupported("generator try/finally is missing its finalizer resume state")
        })?;
        let finally_exit_state = generator_plan.finally_exit_state.ok_or_else(|| {
            EmitError::unsupported("generator try/finally is missing its finalizer exit state")
        })?;

        self.emit_generator_state_in_range(
            generator_plan.entry_state,
            generator_plan.exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Block, function);
        let catch_skip_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(catch_skip_frame);
        let catch_frame = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(catch_frame);

        self.emit_generator_state_in_range(
            generator_plan.entry_state,
            generator_plan.try_exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.push_scope();
        self.compile_resumable_block_contents(
            try_block,
            generator_plan.entry_state,
            true,
            function,
        )?;
        self.pop_scope();
        self.emit_branch_to_target(catch_skip_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_generator_state_in_range(catch_entry_state, catch_exit_state, function)?;
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.push_scope();
        let thrown = self.runtime_schema().reserve_value_local(function);
        thrown.copy_from(self.completion().value(), function);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_set_resumable_resume_point(catch_entry_state, function)?;
        function.instruction(&Instruction::End);
        if let Some(environment) = catch_parameter_environment {
            self.emit_enter_resumable_lexical_environment(
                environment,
                catch_entry_state,
                function,
            )?;
        }
        let catch_storage = self
            .lookup_current_scope_binding(catch_name)
            .unwrap_or_else(|| self.allocate_dynamic_binding_storage(catch_name, function));
        self.binding_scopes
            .last_mut()
            .expect("binding scope stack must exist")
            .insert(catch_name.to_string(), catch_storage);
        if catch_source_name != catch_name {
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(catch_source_name.to_string(), catch_storage);
        }

        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.write_binding_from_locals(catch_storage, &thrown, function);
        self.emit_statement_result(function);
        function.instruction(&Instruction::End);
        thrown.clear(function);

        self.push_scope();
        self.compile_resumable_block_contents(catch_block, catch_entry_state, true, function)?;
        self.pop_scope();
        if catch_parameter_environment.is_some() {
            self.emit_leave_lexical_environment(function);
        }
        self.pop_scope();
        self.emit_branch_to_target(catch_skip_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.emit_resumable_finalizer_needs_pending_completion(finally_entry_state, function)?;
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_push_generator_pending_completion(function)?;
        self.emit_statement_result(function);
        self.emit_set_resumable_resume_point(finally_entry_state, function)?;
        function.instruction(&Instruction::End);

        let finalizer_epilogue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(finalizer_epilogue_frame);
        self.generator_finalizer_depth += 1;
        self.push_scope();
        self.compile_resumable_block_contents(finally_block, finally_entry_state, true, function)?;
        self.pop_scope();
        self.generator_finalizer_depth -= 1;
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_pop_and_restore_generator_pending_completion(function)?;
        function.instruction(&Instruction::Else);
        self.emit_discard_generator_pending_completion(function)?;
        function.instruction(&Instruction::End);
        self.emit_set_resumable_resume_point(generator_plan.exit_state, function)?;
        self.emit_dispatch_current_completion(function)?;

        debug_assert_eq!(catch_exit_state, finally_entry_state);
        debug_assert_eq!(finally_exit_state, generator_plan.exit_state);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_async_try_catch(
        &mut self,
        try_block: &BlockIr,
        catch_name: &str,
        catch_source_name: &str,
        catch_parameter_environment: Option<&LexicalEnvironmentIr>,
        catch_block: &BlockIr,
        async_plan: AsyncTryPlanIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.with_checked_async_generator_source_environment(|builder| {
            builder.compile_async_try_catch_in_source_environment(
                try_block,
                catch_name,
                catch_source_name,
                catch_parameter_environment,
                catch_block,
                async_plan,
                function,
            )
        })
    }

    fn compile_async_try_catch_in_source_environment(
        &mut self,
        try_block: &BlockIr,
        catch_name: &str,
        catch_source_name: &str,
        catch_parameter_environment: Option<&LexicalEnvironmentIr>,
        catch_block: &BlockIr,
        async_plan: AsyncTryPlanIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let catch_entry_state = async_plan.catch_entry_state.ok_or_else(|| {
            EmitError::unsupported("async try/catch is missing its catch resume state")
        })?;
        let catch_exit_state = async_plan.catch_exit_state.ok_or_else(|| {
            EmitError::unsupported("async try/catch is missing its catch exit state")
        })?;

        self.emit_async_try_state_in_range(
            async_plan.entry_state,
            async_plan.exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        let outer_frame = self.open_frame(ControlFrameKind::Block, function);
        let catch_frame = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(catch_frame);

        self.emit_async_try_state_in_range(
            async_plan.entry_state,
            async_plan.try_exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.push_scope();
        self.compile_resumable_block_contents(try_block, async_plan.entry_state, true, function)?;
        self.pop_scope();
        self.emit_set_resumable_resume_point(async_plan.exit_state, function)?;
        self.emit_branch_to_target(outer_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.push_scope();
        let thrown = self.runtime_schema().reserve_value_local(function);
        thrown.copy_from(self.completion().value(), function);
        let restores_checked_async_generator_catch =
            self.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() == FunctionExecutionKind::AsyncGenerator
            }) && self.checked_async_generator_environment_owner.is_some()
                && self
                    .async_generator_resume_environment_plan
                    .as_ref()
                    .is_some_and(|plan| {
                        !plan.invocation_resume_states().is_empty()
                            || !plan.enclosing_scope_resume_states().is_empty()
                    });
        let restores_catch_environment = restores_checked_async_generator_catch
            || self.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() == FunctionExecutionKind::Async
            });
        if restores_catch_environment {
            self.completion().kind().load(function);
            function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            self.emit_set_resumable_resume_point(catch_entry_state, function)?;
            function.instruction(&Instruction::End);
            if let Some(environment) = catch_parameter_environment {
                if restores_checked_async_generator_catch {
                    self.emit_enter_checked_async_generator_catch_environment(
                        environment,
                        catch_entry_state,
                        function,
                    )?;
                } else {
                    self.emit_enter_resumable_lexical_environment(
                        environment,
                        catch_entry_state,
                        function,
                    )?;
                }
            }
        }
        let catch_storage = self
            .lookup_current_scope_binding(catch_name)
            .unwrap_or_else(|| self.allocate_dynamic_binding_storage(catch_name, function));
        self.binding_scopes
            .last_mut()
            .expect("binding scope stack must exist")
            .insert(catch_name.to_string(), catch_storage);
        if catch_source_name != catch_name {
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(catch_source_name.to_string(), catch_storage);
        }

        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        if !restores_catch_environment {
            if let Some(environment) = catch_parameter_environment {
                self.emit_enter_lexical_environment(environment, function)?;
            }
        }
        self.write_binding_from_locals(catch_storage, &thrown, function);
        self.emit_statement_result(function);
        self.emit_set_resumable_resume_point(catch_entry_state, function)?;
        function.instruction(&Instruction::End);
        thrown.clear(function);

        self.push_scope();
        self.compile_resumable_block_contents(catch_block, catch_entry_state, true, function)?;
        self.pop_scope();
        if catch_parameter_environment.is_some() {
            self.emit_leave_lexical_environment(function);
        }
        self.pop_scope();
        self.emit_set_resumable_resume_point(async_plan.exit_state, function)?;

        debug_assert_eq!(catch_exit_state, async_plan.exit_state);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_async_try_catch_finally(
        &mut self,
        try_block: &BlockIr,
        catch_name: &str,
        catch_source_name: &str,
        catch_parameter_environment: Option<&LexicalEnvironmentIr>,
        catch_block: &BlockIr,
        finally_block: &BlockIr,
        async_plan: AsyncTryPlanIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.with_checked_async_generator_source_environment(|builder| {
            builder.compile_async_try_catch_finally_in_source_environment(
                try_block,
                catch_name,
                catch_source_name,
                catch_parameter_environment,
                catch_block,
                finally_block,
                async_plan,
                function,
            )
        })
    }

    fn compile_async_try_catch_finally_in_source_environment(
        &mut self,
        try_block: &BlockIr,
        catch_name: &str,
        catch_source_name: &str,
        catch_parameter_environment: Option<&LexicalEnvironmentIr>,
        catch_block: &BlockIr,
        finally_block: &BlockIr,
        async_plan: AsyncTryPlanIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let catch_entry_state = async_plan.catch_entry_state.ok_or_else(|| {
            EmitError::unsupported("async try/catch is missing its catch resume state")
        })?;
        let catch_exit_state = async_plan.catch_exit_state.ok_or_else(|| {
            EmitError::unsupported("async try/catch is missing its catch exit state")
        })?;
        let finally_entry_state = async_plan.finally_entry_state.ok_or_else(|| {
            EmitError::unsupported("async try/finally is missing its finalizer resume state")
        })?;
        let finally_exit_state = async_plan.finally_exit_state.ok_or_else(|| {
            EmitError::unsupported("async try/finally is missing its finalizer exit state")
        })?;

        self.emit_async_try_state_in_range(
            async_plan.entry_state,
            async_plan.exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Block, function);
        let catch_skip_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(catch_skip_frame);
        let catch_frame = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(catch_frame);

        self.emit_async_try_state_in_range(
            async_plan.entry_state,
            async_plan.try_exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.push_scope();
        self.compile_resumable_block_contents(try_block, async_plan.entry_state, true, function)?;
        self.pop_scope();
        self.emit_branch_to_target(catch_skip_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_async_try_state_in_range(catch_entry_state, catch_exit_state, function)?;
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.push_scope();
        let thrown = self.runtime_schema().reserve_value_local(function);
        thrown.copy_from(self.completion().value(), function);
        let restores_checked_async_generator_catch =
            self.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() == FunctionExecutionKind::AsyncGenerator
            }) && self.checked_async_generator_environment_owner.is_some()
                && self
                    .async_generator_resume_environment_plan
                    .as_ref()
                    .is_some_and(|plan| {
                        !plan.invocation_resume_states().is_empty()
                            || !plan.enclosing_scope_resume_states().is_empty()
                    });
        let restores_catch_environment = restores_checked_async_generator_catch
            || self.current_function_meta().is_some_and(|meta| {
                meta.protocol().execution_kind() == FunctionExecutionKind::Async
            });
        if restores_catch_environment {
            self.completion().kind().load(function);
            function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            self.emit_set_resumable_resume_point(catch_entry_state, function)?;
            function.instruction(&Instruction::End);
            if let Some(environment) = catch_parameter_environment {
                if restores_checked_async_generator_catch {
                    self.emit_enter_checked_async_generator_catch_environment(
                        environment,
                        catch_entry_state,
                        function,
                    )?;
                } else {
                    self.emit_enter_resumable_lexical_environment(
                        environment,
                        catch_entry_state,
                        function,
                    )?;
                }
            }
        }
        let catch_storage = self
            .lookup_current_scope_binding(catch_name)
            .unwrap_or_else(|| self.allocate_dynamic_binding_storage(catch_name, function));
        self.binding_scopes
            .last_mut()
            .expect("binding scope stack must exist")
            .insert(catch_name.to_string(), catch_storage);
        if catch_source_name != catch_name {
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(catch_source_name.to_string(), catch_storage);
        }

        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        if !restores_catch_environment {
            if let Some(environment) = catch_parameter_environment {
                self.emit_enter_lexical_environment(environment, function)?;
            }
        }
        self.write_binding_from_locals(catch_storage, &thrown, function);
        self.emit_statement_result(function);
        self.emit_set_resumable_resume_point(catch_entry_state, function)?;
        function.instruction(&Instruction::End);
        thrown.clear(function);

        self.push_scope();
        self.compile_resumable_block_contents(catch_block, catch_entry_state, true, function)?;
        self.pop_scope();
        if catch_parameter_environment.is_some() {
            self.emit_leave_lexical_environment(function);
        }
        self.pop_scope();
        self.emit_branch_to_target(catch_skip_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.emit_resumable_finalizer_needs_pending_completion(finally_entry_state, function)?;
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_push_async_pending_completion(function)?;
        self.emit_statement_result(function);
        self.emit_set_resumable_resume_point(finally_entry_state, function)?;
        function.instruction(&Instruction::End);

        let finalizer_epilogue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(finalizer_epilogue_frame);
        self.push_scope();
        self.compile_resumable_block_contents(finally_block, finally_entry_state, true, function)?;
        self.pop_scope();
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_pop_and_restore_async_pending_completion(function)?;
        function.instruction(&Instruction::Else);
        self.emit_discard_async_pending_completion(function)?;
        function.instruction(&Instruction::End);
        self.emit_set_resumable_resume_point(async_plan.exit_state, function)?;
        self.emit_dispatch_async_completion(function)?;

        debug_assert_eq!(catch_exit_state, finally_entry_state);
        debug_assert_eq!(finally_exit_state, async_plan.exit_state);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_async_try_finally(
        &mut self,
        try_block: &BlockIr,
        finally_block: &BlockIr,
        async_plan: AsyncTryPlanIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.with_checked_async_generator_source_environment(|builder| {
            builder.compile_async_try_finally_in_source_environment(
                try_block,
                finally_block,
                async_plan,
                function,
            )
        })
    }

    fn compile_async_try_finally_in_source_environment(
        &mut self,
        try_block: &BlockIr,
        finally_block: &BlockIr,
        async_plan: AsyncTryPlanIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let finally_entry_state = async_plan.finally_entry_state.ok_or_else(|| {
            EmitError::unsupported("async try/finally is missing its finalizer resume state")
        })?;
        let finally_exit_state = async_plan.finally_exit_state.ok_or_else(|| {
            EmitError::unsupported("async try/finally is missing its finalizer exit state")
        })?;

        self.emit_async_try_state_in_range(
            async_plan.entry_state,
            async_plan.exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.open_frame(ControlFrameKind::Block, function);
        let finally_entry_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(finally_entry_frame);

        self.emit_async_try_state_in_range(
            async_plan.entry_state,
            async_plan.try_exit_state,
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.push_scope();
        self.compile_resumable_block_contents(try_block, async_plan.entry_state, true, function)?;
        self.pop_scope();
        self.emit_branch_to_target(finally_entry_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.emit_resumable_finalizer_needs_pending_completion(finally_entry_state, function)?;
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_push_async_pending_completion(function)?;
        self.emit_statement_result(function);
        self.emit_set_resumable_resume_point(finally_entry_state, function)?;
        function.instruction(&Instruction::End);

        let finalizer_epilogue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(finalizer_epilogue_frame);
        self.push_scope();
        self.compile_resumable_block_contents(finally_block, finally_entry_state, true, function)?;
        self.pop_scope();
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_pop_and_restore_async_pending_completion(function)?;
        function.instruction(&Instruction::Else);
        self.emit_discard_async_pending_completion(function)?;
        function.instruction(&Instruction::End);
        self.emit_set_resumable_resume_point(async_plan.exit_state, function)?;
        self.emit_dispatch_async_completion(function)?;

        debug_assert_eq!(finally_exit_state, async_plan.exit_state);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_try_finally(
        &mut self,
        try_block: &BlockIr,
        finally_block: &BlockIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_statement_result(function);
        let saved = self.runtime_schema().reserve_completion(function);

        let _outer_frame = self.open_frame(ControlFrameKind::Block, function);
        let finally_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(finally_frame);
        self.push_scope();
        self.compile_block_contents(try_block, function)?;
        self.pop_scope();
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.save_current_completion(&saved, function);
        self.emit_statement_result(function);
        self.push_scope();
        self.compile_block_contents(finally_block, function)?;
        self.pop_scope();
        self.emit_resume_after_finally(&saved, function)?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        saved.clear(function);
        Ok(())
    }

    fn compile_async_disposable_scope(
        &mut self,
        execution: &AsyncDisposableScopeExecutionIr,
        resources: &AsyncDisposableResourcesIr,
        body: &BlockIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        debug_assert!(!resources.is_empty());
        let owner = ActivationAsyncDisposeOwner::from_execution(execution);
        if !self
            .current_function_meta()
            .is_some_and(|meta| meta.protocol().execution_kind() == owner.execution_kind())
        {
            return Err(EmitError::unsupported(
                "async DisposeCapability has the wrong execution owner",
            ));
        }
        let binding = self
            .activation_owned_binding_storage(owner.binding_name())
            .ok_or_else(|| {
                EmitError::unsupported(
                    "async DisposeCapability is missing its activation-owned binding",
                )
            })?;
        let storage = ActivationAsyncDisposeCapabilityStorage { binding };
        let finalizer = owner.finalizer();

        self.emit_async_state_in_range(finalizer.entry_state(), finalizer.exit_state(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let _outer_frame = self.open_frame(ControlFrameKind::Block, function);
        let disposal_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(disposal_frame);

        self.emit_async_state_in_range(
            finalizer.entry_state(),
            finalizer.dispose_state(),
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_resumable_state_equals(finalizer.entry_state(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.initialize_activation_async_dispose_capability(&storage, resources, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.push_scope();
        self.compile_resumable_block_contents(body, finalizer.entry_state(), true, function)?;
        self.pop_scope();
        self.emit_branch_to_target(disposal_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.emit_resumable_finalizer_needs_pending_completion(
            finalizer.dispose_state(),
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        let pending = self.begin_async_dispose_pending_completion(function)?;
        self.set_completion_kind(CompletionKind::Normal, function);
        let disposing =
            self.begin_activation_async_dispose_capability(storage, finalizer, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.consume_activation_async_dispose_capability(
            &owner,
            disposing,
            pending,
            finalizer,
            ActivationAsyncDisposeCompletionContinuation::Scope,
            function,
        )?;

        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// Resolve an activation binding from the environment that
    /// is current at this source point.
    ///
    /// `owned_env_slot` is relative to the activation root. A materialized
    /// source environment may sit above that root, so spelling `hops: 0` at a
    /// consumer can silently redirect the slot into an unrelated source
    /// binding with the same slot index.
    pub(crate) fn activation_owned_binding_storage(&self, name: &str) -> Option<BindingStorage> {
        self.owned_env_slot(name)
            .map(|slot| BindingStorage::EnvSlot {
                slot,
                hops: self.environment_depth,
            })
    }

    fn initialize_async_disposable_resource_bindings(
        &mut self,
        resources: &AsyncDisposableResourcesIr,
        function: &mut Function,
    ) {
        for resource in resources.iter() {
            let storage = self
                .lookup_current_scope_binding(resource.binding_name())
                .or_else(|| self.lookup_binding(resource.binding_name()))
                .unwrap_or_else(|| {
                    self.allocate_binding(
                        resource.binding_name().to_string(),
                        BindingMode::Const,
                        resource.initializer().kind,
                        function,
                    )
                });
            self.initialize_binding_uninitialized(storage, function);
        }
    }

    fn initialize_activation_async_dispose_capability(
        &mut self,
        storage: &ActivationAsyncDisposeCapabilityStorage,
        resources: &AsyncDisposableResourcesIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let capability = self.initialize_empty_activation_async_dispose_capability(
            storage,
            resources.len(),
            function,
        )?;
        for resource in resources.iter() {
            let acquired = self.reserve_async_disposable_resource_locals(function);
            self.compile_expr_to_value(resource.initializer(), &acquired.value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            self.acquire_async_disposable_resource_from_locals(&acquired, function)?;
            self.append_activation_async_disposable_resource(&capability, &acquired, function);
            let binding = self
                .lookup_current_scope_binding(resource.binding_name())
                .or_else(|| self.lookup_binding(resource.binding_name()))
                .unwrap_or_else(|| {
                    self.allocate_binding(
                        resource.binding_name().to_string(),
                        BindingMode::Const,
                        resource.initializer().kind,
                        function,
                    )
                });
            self.write_binding_from_locals(binding, &acquired.value, function);
            self.release_async_disposable_resource_locals(acquired, function);
        }
        self.release_active_activation_async_dispose_capability(capability, function);
        Ok(())
    }

    fn initialize_empty_activation_async_dispose_capability(
        &mut self,
        storage: &ActivationAsyncDisposeCapabilityStorage,
        capacity: usize,
        function: &mut Function,
    ) -> Result<ActiveActivationAsyncDisposeCapabilityLocals, EmitError> {
        let schema = self.runtime_schema();
        let count = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(i32::try_from(capacity).map_err(
            |_| EmitError::unsupported("too many asynchronous disposal resources"),
        )?));
        count.store(function);
        let entries = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ActivationAsyncDisposeResourceTable>()
                .filled(GcOperand::null(schema), count, function),
            function,
        );
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ActivationAsyncDisposeCapability>()
                .construct(
                    (
                        GcOperand::constant(ActivationAsyncDisposeCapabilityState::Pending),
                        GcOperand::reference(&entries, schema),
                        GcOperand::i64(0),
                        GcOperand::i64(0),
                        GcOperand::boolean(false),
                        GcOperand::boolean(false),
                    ),
                    function,
                ),
            function,
        );
        let cell = self.control_flow_owned_binding_cell(storage.binding, function)?;
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::ASYNC_DISPOSE_CAPABILITY)
            .write(
                &cell,
                GcOperand::nullable_reference(&record, schema),
                schema,
                function,
            );
        cell.clear(function);
        schema.release_i32_local(count, function);
        Ok(ActiveActivationAsyncDisposeCapabilityLocals { record, entries })
    }

    fn load_activation_async_dispose_capability(
        &mut self,
        storage: &ActivationAsyncDisposeCapabilityStorage,
        function: &mut Function,
    ) -> Result<ActiveActivationAsyncDisposeCapabilityLocals, EmitError> {
        let schema = self.runtime_schema();
        let cell = self.control_flow_owned_binding_cell(storage.binding, function)?;
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<BindingCell>()
                .field(BindingCellSchema::ASYNC_DISPOSE_CAPABILITY)
                .read(&cell, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let entries = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ActivationAsyncDisposeCapability>()
                .field(ActivationAsyncDisposeCapabilitySchema::RESOURCES)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        cell.clear(function);
        Ok(ActiveActivationAsyncDisposeCapabilityLocals { record, entries })
    }

    fn release_active_activation_async_dispose_capability(
        &mut self,
        capability: ActiveActivationAsyncDisposeCapabilityLocals,
        function: &mut Function,
    ) {
        capability.entries.clear(function);
        capability.record.clear(function);
    }

    fn reserve_async_disposable_resource_locals(
        &mut self,
        function: &mut Function,
    ) -> AcquiredAsyncDisposableResourceLocals {
        let schema = self.runtime_schema();
        let resource = AcquiredAsyncDisposableResourceLocals {
            kind: GcI32DomainLocal::new(schema, ActivationAsyncDisposeEntryKind::Empty, function),
            value: schema.reserve_value_local(function),
            method: schema.reserve_value_local(function),
        };
        resource.value.set_undefined(function);
        resource.method.set_undefined(function);
        resource
    }

    fn release_async_disposable_resource_locals(
        &mut self,
        resource: AcquiredAsyncDisposableResourceLocals,
        function: &mut Function,
    ) {
        resource.method.clear(function);
        resource.value.clear(function);
        resource.kind.clear(self.runtime_schema(), function);
    }

    fn acquire_async_disposable_resource_from_locals(
        &mut self,
        resource: &AcquiredAsyncDisposableResourceLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.compile_nullish_tagged_i32(resource.value.tag(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_heap_object_like_tag_i32(resource.value.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_control_flow_type_error(
            RuntimeErrorMessage::AWAIT_USING_DECLARATION_RESOURCE_IS_NOT_AN_OBJECT,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(
                scriptw_ir::WellKnownSymbol::AsyncDispose,
                function,
            )?,
            function,
        );
        let key = crate::operations::PropertyKeyLocals::from_symbol(schema, &symbol, function);
        let pending = schema.reserve_completion(function);
        self.emit_object_read(&resource.value, &resource.value, &key, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        resource.method.copy_from(pending.value(), function);
        key.clear(function);
        symbol.clear(function);
        self.compile_nullish_tagged_i32(resource.method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(scriptw_ir::WellKnownSymbol::Dispose, function)?,
            function,
        );
        let key = crate::operations::PropertyKeyLocals::from_symbol(schema, &symbol, function);
        self.emit_object_read(&resource.value, &resource.value, &key, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        resource.method.copy_from(pending.value(), function);
        key.clear(function);
        symbol.clear(function);
        self.compile_nullish_tagged_i32(resource.method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_control_flow_type_error(
            RuntimeErrorMessage::AWAIT_USING_DECLARATION_RESOURCE_HAS_NO_DISPOSAL_METHOD,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_callable_i32(&resource.method, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_control_flow_type_error(
            RuntimeErrorMessage::AWAIT_USING_DECLARATION_SYMBOL_DISPOSE_METHOD_IS_NOT_CALLABLE,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        resource.kind.set_constant(
            ActivationAsyncDisposeEntryKind::SyncFallbackMethod,
            function,
        );
        function.instruction(&Instruction::Else);
        self.emit_is_callable_i32(&resource.method, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_control_flow_type_error(
            RuntimeErrorMessage::AWAIT_USING_DECLARATION_SYMBOL_ASYNCDISPOSE_METHOD_IS_NOT_CALLABLE,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        resource
            .kind
            .set_constant(ActivationAsyncDisposeEntryKind::AsyncMethod, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn append_activation_async_disposable_resource(
        &mut self,
        capability: &ActiveActivationAsyncDisposeCapabilityLocals,
        resource: &AcquiredAsyncDisposableResourceLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let count = schema.reserve_i64_local(function);
        let index = schema.reserve_i32_local(function);
        schema
            .struct_type::<ActivationAsyncDisposeCapability>()
            .field(ActivationAsyncDisposeCapabilitySchema::ENTRY_COUNT)
            .read(&capability.record, schema, function)
            .store_i64(count, function);
        count.load(function);
        function.instruction(&Instruction::I32WrapI64);
        index.store(function);
        let value = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&resource.value, function),
            function,
        );
        let method = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&resource.method, function),
            function,
        );
        let entry = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ActivationAsyncDisposeResource>()
                .construct(
                    (
                        resource.kind.operand(),
                        GcOperand::reference(&value, schema),
                        GcOperand::reference(&method, schema),
                    ),
                    function,
                ),
            function,
        );
        schema
            .array_type::<ActivationAsyncDisposeResourceTable>()
            .write(
                &capability.entries,
                index,
                GcOperand::nullable_reference(&entry, schema),
                schema,
                function,
            );
        count.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        count.store(function);
        schema
            .struct_type::<ActivationAsyncDisposeCapability>()
            .field(ActivationAsyncDisposeCapabilitySchema::ENTRY_COUNT)
            .write(
                &capability.record,
                GcOperand::i64_local(count),
                schema,
                function,
            );
        entry.clear(function);
        method.clear(function);
        value.clear(function);
        schema.release_i32_local(index, function);
        schema.release_i64_local(count, function);
    }

    fn begin_async_dispose_pending_completion(
        &mut self,
        function: &mut Function,
    ) -> Result<ActiveAsyncDisposePendingCompletion, EmitError> {
        self.emit_push_async_pending_completion(function)?;
        Ok(ActiveAsyncDisposePendingCompletion)
    }

    fn begin_activation_async_dispose_capability(
        &mut self,
        storage: ActivationAsyncDisposeCapabilityStorage,
        finalizer: &AsyncDisposableFinalizerPlanIr,
        function: &mut Function,
    ) -> Result<DisposingActivationAsyncDisposeCapability, EmitError> {
        let schema = self.runtime_schema();
        let capability = self.load_activation_async_dispose_capability(&storage, function)?;
        let count = schema.reserve_i64_local(function);
        let row = schema.struct_type::<ActivationAsyncDisposeCapability>();
        row.field(ActivationAsyncDisposeCapabilitySchema::STATE)
            .write(
                &capability.record,
                GcOperand::constant(ActivationAsyncDisposeCapabilityState::Disposing),
                schema,
                function,
            );
        row.field(ActivationAsyncDisposeCapabilitySchema::ENTRY_COUNT)
            .read(&capability.record, schema, function)
            .store_i64(count, function);
        row.field(ActivationAsyncDisposeCapabilitySchema::NEXT_RESOURCE_INDEX)
            .write(
                &capability.record,
                GcOperand::i64_local(count),
                schema,
                function,
            );
        row.field(ActivationAsyncDisposeCapabilitySchema::ENTRY_COUNT)
            .write(&capability.record, GcOperand::i64(0), schema, function);
        row.field(ActivationAsyncDisposeCapabilitySchema::NEEDS_AWAIT)
            .write(
                &capability.record,
                GcOperand::boolean(false),
                schema,
                function,
            );
        row.field(ActivationAsyncDisposeCapabilitySchema::HAS_AWAITED)
            .write(
                &capability.record,
                GcOperand::boolean(false),
                schema,
                function,
            );
        self.emit_set_resumable_resume_point(finalizer.dispose_state(), function)?;
        schema.release_i64_local(count, function);
        self.release_active_activation_async_dispose_capability(capability, function);
        Ok(DisposingActivationAsyncDisposeCapability { storage })
    }

    fn fold_error_into_async_dispose_pending_completion(
        &mut self,
        new_error: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let frame = schema.reserve_gc_local(function).initialize(
            self.pending_completion_frame()?.load(schema, function),
            function,
        );
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<InvocationFrame>()
                .field(InvocationFrameSchema::PENDING_COMPLETION)
                .read(&frame, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let next = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<CompletionRecord>()
                .field(CompletionRecordSchema::NEXT)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        let pending = schema.reserve_completion(function);
        let combined = schema.reserve_completion(function);
        schema
            .struct_type::<CompletionRecord>()
            .read_into(&record, &pending, schema, function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let realm = self.load_current_realm(function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            crate::functions::NonArrayRealmIntrinsicSlot::SuppressedErrorPrototype,
            &prototype,
            function,
        );
        self.emit_alloc_suppressed_error_instance(
            None,
            new_error,
            pending.value(),
            &prototype,
            &combined,
            function,
        )?;
        pending.set_throw(combined.value(), function);
        prototype.clear(function);
        realm.clear(function);
        function.instruction(&Instruction::Else);
        pending.set_throw(new_error, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let return_stage =
            GcI32DomainLocal::new(schema, AsyncGeneratorReturnStage::Unawaited, function);
        let replacement = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<CompletionRecord>().from_completion(
                &pending,
                &next,
                &return_stage,
                schema,
                function,
            ),
            function,
        );
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::PENDING_COMPLETION)
            .write(
                &frame,
                GcOperand::nullable_reference(&replacement, schema),
                schema,
                function,
            );
        replacement.clear(function);
        return_stage.clear(schema, function);
        combined.clear(function);
        pending.clear(function);
        next.clear(function);
        record.clear(function);
        frame.clear(function);
        Ok(())
    }

    fn emit_sync_fallback_disposal_promise(
        &mut self,
        method: &ValueLocals,
        receiver: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        // GetDisposeMethod's async wrapper creates its capability before calling
        // the saved synchronous method and ignores that method's normal value.
        let realm = self.load_current_realm(function);
        let promise = self.emit_alloc_promise_in_realm(&realm, function)?;
        let called = schema.reserve_completion(function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        self.emit_function_or_proxy_call_with_argv(
            method, receiver, &arguments, &called, function,
        )?;
        called.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_settle_promise_record(
            &promise,
            PromiseSettlement::Reject,
            called.value(),
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_settle_promise_record(
            &promise,
            PromiseSettlement::Fulfill,
            &undefined,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        output.set_reference(&promise, schema, function);
        undefined.clear(function);
        arguments.clear(function);
        called.clear(function);
        promise.clear(function);
        realm.clear(function);
        Ok(())
    }

    fn emit_load_activation_async_dispose_resume(
        &mut self,
        owner: &ActivationAsyncDisposeOwner<'_>,
        value: &ValueLocals,
        is_throw: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let owner = match owner {
            ActivationAsyncDisposeOwner::AsyncFunction(_)
            | ActivationAsyncDisposeOwner::AsyncFunctionForOf(_)
            | ActivationAsyncDisposeOwner::AsyncCompleteIterator(_) => {
                AsyncContinuationOwner::AsyncFunction
            }
            ActivationAsyncDisposeOwner::AsyncGenerator(_) => {
                AsyncContinuationOwner::AsyncGenerator
            }
        };
        self.emit_load_async_continuation_resume(&owner, value, is_throw, function)
    }

    fn emit_load_async_continuation_resume(
        &mut self,
        owner: &AsyncContinuationOwner,
        value: &ValueLocals,
        is_throw: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let kind = schema.reserve_i32_local(function);
        match owner {
            AsyncContinuationOwner::AsyncFunction => {
                let activation = self.control_flow_async_activation(function)?;
                let row = schema.struct_type::<AsyncActivation>();
                row.field(AsyncActivationSchema::RESUME_COMPLETION)
                    .read(&activation, schema, function)
                    .store(kind, function);
                let stored = schema.reserve_gc_local(function).initialize(
                    row.field(AsyncActivationSchema::RESUME_VALUE)
                        .read(&activation, schema, function)
                        .reference(),
                    function,
                );
                schema
                    .struct_type::<StoredValue>()
                    .read_into(&stored, value, schema, function);
                kind.load(function);
                function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                    AwaitCompletionKind::Throw,
                )));
                function.instruction(&Instruction::I32Eq);
                is_throw.store(function);
                kind.load(function);
                function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                    AwaitCompletionKind::Normal,
                )));
                function.instruction(&Instruction::I32Eq);
                is_throw.load(function);
                function.instruction(&Instruction::I32Or);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                function.instruction(&Instruction::Unreachable);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                stored.clear(function);
                activation.clear(function);
            }
            AsyncContinuationOwner::AsyncGenerator => {
                let activation = self.control_flow_async_generator_activation(function)?;
                let row = schema.struct_type::<AsyncGeneratorActivation>();
                row.field(AsyncGeneratorActivationSchema::RESUME_KIND)
                    .read(&activation, schema, function)
                    .store(kind, function);
                let stored = schema.reserve_gc_local(function).initialize(
                    row.field(AsyncGeneratorActivationSchema::RESUME_VALUE)
                        .read(&activation, schema, function)
                        .reference(),
                    function,
                );
                schema
                    .struct_type::<StoredValue>()
                    .read_into(&stored, value, schema, function);
                kind.load(function);
                function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                    AsyncGeneratorResumeKind::Reject,
                )));
                function.instruction(&Instruction::I32Eq);
                is_throw.store(function);
                kind.load(function);
                function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                    AsyncGeneratorResumeKind::Fulfill,
                )));
                function.instruction(&Instruction::I32Eq);
                is_throw.load(function);
                function.instruction(&Instruction::I32Or);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                function.instruction(&Instruction::Unreachable);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                stored.clear(function);
                activation.clear(function);
            }
        }
        schema.release_i32_local(kind, function);
        Ok(())
    }

    fn emit_activation_async_dispose_await_reactions(
        &mut self,
        owner: &ActivationAsyncDisposeOwner<'_>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let owner = match owner {
            ActivationAsyncDisposeOwner::AsyncFunction(_)
            | ActivationAsyncDisposeOwner::AsyncFunctionForOf(_)
            | ActivationAsyncDisposeOwner::AsyncCompleteIterator(_) => {
                AsyncContinuationOwner::AsyncFunction
            }
            ActivationAsyncDisposeOwner::AsyncGenerator(_) => {
                AsyncContinuationOwner::AsyncGenerator
            }
        };
        self.emit_async_continuation_await(&owner, value, function)
    }

    fn emit_async_continuation_await(
        &mut self,
        owner: &AsyncContinuationOwner,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_save_resumable_environment(function)?;
        match owner {
            AsyncContinuationOwner::AsyncFunction => {
                let activation = self.control_flow_async_activation(function)?;
                self.emit_async_await_reactions(&activation, value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                activation.clear(function);
            }
            AsyncContinuationOwner::AsyncGenerator => {
                let activation = self.control_flow_async_generator_activation(function)?;
                self.emit_async_generator_await_reactions(&activation, value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                let row = schema.struct_type::<AsyncGeneratorActivation>();
                row.field(AsyncGeneratorActivationSchema::BODY_STATUS)
                    .write(
                        &activation,
                        GcOperand::constant(AsyncGeneratorBodyStatus::Await),
                        schema,
                        function,
                    );
                row.field(AsyncGeneratorActivationSchema::EXECUTION_STATE)
                    .write(
                        &activation,
                        GcOperand::constant(AsyncGeneratorExecutionState::Executing),
                        schema,
                        function,
                    );
                activation.clear(function);
            }
        }
        Ok(())
    }

    fn emit_dispatch_activation_async_dispose_completion(
        &mut self,
        owner: &ActivationAsyncDisposeOwner<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match owner {
            ActivationAsyncDisposeOwner::AsyncFunction(_)
            | ActivationAsyncDisposeOwner::AsyncFunctionForOf(_)
            | ActivationAsyncDisposeOwner::AsyncCompleteIterator(_) => {
                self.emit_dispatch_current_completion(function)
            }
            ActivationAsyncDisposeOwner::AsyncGenerator(_) => {
                self.emit_dispatch_async_generator_completion(function);
                Ok(())
            }
        }
    }

    fn emit_activation_async_dispose_await(
        &mut self,
        owner: &ActivationAsyncDisposeOwner<'_>,
        capability: &ActiveActivationAsyncDisposeCapabilityLocals,
        value: &ValueLocals,
        finalizer: &AsyncDisposableFinalizerPlanIr,
        role: ActivationAsyncDisposeAwaitRole,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        match role {
            ActivationAsyncDisposeAwaitRole::Method => {
                schema
                    .struct_type::<ActivationAsyncDisposeCapability>()
                    .field(ActivationAsyncDisposeCapabilitySchema::HAS_AWAITED)
                    .write(
                        &capability.record,
                        GcOperand::boolean(true),
                        schema,
                        function,
                    );
            }
            ActivationAsyncDisposeAwaitRole::EmptyPrelude => {}
        }
        self.emit_set_resumable_resume_point(finalizer.resume_state(), function)?;
        let await_failure = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(await_failure);
        self.emit_activation_async_dispose_await_reactions(owner, value, function)?;
        self.emit_return_async_suspension(function)?;
        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        // PromiseResolve can fail synchronously before registration. That error
        // joins the parked completion, just as a resumed Await rejection does.
        let error = schema.reserve_value_local(function);
        error.copy_from(self.completion().value(), function);
        self.fold_error_into_async_dispose_pending_completion(&error, function)?;
        error.clear(function);
        self.emit_statement_result(function);
        self.emit_set_resumable_resume_point(finalizer.dispose_state(), function)?;
        Ok(())
    }

    fn consume_activation_async_dispose_capability(
        &mut self,
        owner: &ActivationAsyncDisposeOwner<'_>,
        disposing: DisposingActivationAsyncDisposeCapability,
        pending: ActiveAsyncDisposePendingCompletion,
        finalizer: &AsyncDisposableFinalizerPlanIr,
        continuation: ActivationAsyncDisposeCompletionContinuation<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let capability =
            self.load_activation_async_dispose_capability(&disposing.storage, function)?;
        let resumed = schema.reserve_value_local(function);
        let resume_throw = schema.reserve_i32_local(function);
        self.emit_resumable_state_equals(finalizer.resume_state(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_load_activation_async_dispose_resume(owner, &resumed, resume_throw, function)?;
        resume_throw.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.fold_error_into_async_dispose_pending_completion(&resumed, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_statement_result(function);
        self.emit_set_resumable_resume_point(finalizer.dispose_state(), function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.emit_resumable_state_equals(finalizer.dispose_state(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let finish = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        let cursor = schema.reserve_i64_local(function);
        let index = schema.reserve_i32_local(function);
        let kind = schema.reserve_i32_local(function);
        let should_await = schema.reserve_i32_local(function);
        let value = schema.reserve_value_local(function);
        let method = schema.reserve_value_local(function);
        let awaited = schema.reserve_value_local(function);
        let called = schema.reserve_completion(function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
        let row = schema.struct_type::<ActivationAsyncDisposeCapability>();
        row.field(ActivationAsyncDisposeCapabilitySchema::NEXT_RESOURCE_INDEX)
            .read(&capability.record, schema, function)
            .store_i64(cursor, function);
        cursor.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(finish, function);
        cursor.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        cursor.store(function);
        cursor.load(function);
        function.instruction(&Instruction::I32WrapI64);
        index.store(function);
        let entry = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ActivationAsyncDisposeResourceTable>()
                .read(&capability.entries, index, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let entry_row = schema.struct_type::<ActivationAsyncDisposeResource>();
        entry_row
            .field(ActivationAsyncDisposeResourceSchema::KIND)
            .read(&entry, schema, function)
            .store(kind, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            ActivationAsyncDisposeEntryKind::SyncMethod,
        )));
        function.instruction(&Instruction::I32Eq);
        row.field(ActivationAsyncDisposeCapabilitySchema::NEEDS_AWAIT)
            .read(&capability.record, schema, function);
        function.instruction(&Instruction::I32And);
        row.field(ActivationAsyncDisposeCapabilitySchema::HAS_AWAITED)
            .read(&capability.record, schema, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        // Await the preceding empty async entries before this sync call. Keep
        // its cursor and row untouched, so the reaction consumes it exactly
        // once. This prelude does not set the spec's hasAwaited method flag.
        row.field(ActivationAsyncDisposeCapabilitySchema::NEEDS_AWAIT)
            .write(
                &capability.record,
                GcOperand::boolean(false),
                schema,
                function,
            );
        awaited.set_undefined(function);
        self.emit_activation_async_dispose_await(
            owner,
            &capability,
            &awaited,
            finalizer,
            ActivationAsyncDisposeAwaitRole::EmptyPrelude,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        row.field(ActivationAsyncDisposeCapabilitySchema::NEXT_RESOURCE_INDEX)
            .write(
                &capability.record,
                GcOperand::i64_local(cursor),
                schema,
                function,
            );
        let stored_value = schema.reserve_gc_local(function).initialize(
            entry_row
                .field(ActivationAsyncDisposeResourceSchema::VALUE)
                .read(&entry, schema, function)
                .reference(),
            function,
        );
        let stored_method = schema.reserve_gc_local(function).initialize(
            entry_row
                .field(ActivationAsyncDisposeResourceSchema::METHOD)
                .read(&entry, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored_value, &value, schema, function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored_method, &method, schema, function);
        // The cursor is committed before callback/Await and the entry is released
        // from the capability before user code can resume or reenter it.
        schema
            .array_type::<ActivationAsyncDisposeResourceTable>()
            .write(
                &capability.entries,
                index,
                GcOperand::null(schema),
                schema,
                function,
            );
        awaited.set_undefined(function);
        function.instruction(&Instruction::I32Const(1));
        should_await.store(function);
        let mut arms = 0;
        for entry_kind in ActivationAsyncDisposeEntryKind::ALL {
            kind.load(function);
            function.instruction(&Instruction::I32Const(GcI32Constant::encode(entry_kind)));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            match entry_kind {
                ActivationAsyncDisposeEntryKind::Empty => {
                    row.field(ActivationAsyncDisposeCapabilitySchema::NEEDS_AWAIT)
                        .write(
                            &capability.record,
                            GcOperand::boolean(true),
                            schema,
                            function,
                        );
                    function.instruction(&Instruction::I32Const(0));
                    should_await.store(function);
                }
                ActivationAsyncDisposeEntryKind::AsyncMethod => {
                    self.emit_function_or_proxy_call_with_argv(
                        &method, &value, &arguments, &called, function,
                    )?;
                    called.kind().load(function);
                    function
                        .instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
                    function.instruction(&Instruction::I32Eq);
                    self.open_frame(ControlFrameKind::If, function);
                    self.fold_error_into_async_dispose_pending_completion(
                        called.value(),
                        function,
                    )?;
                    function.instruction(&Instruction::I32Const(0));
                    should_await.store(function);
                    function.instruction(&Instruction::Else);
                    awaited.copy_from(called.value(), function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                ActivationAsyncDisposeEntryKind::SyncFallbackMethod => {
                    self.emit_sync_fallback_disposal_promise(&method, &value, &awaited, function)?;
                }
                ActivationAsyncDisposeEntryKind::SyncMethod => {
                    self.emit_function_or_proxy_call_with_argv(
                        &method, &value, &arguments, &called, function,
                    )?;
                    called.kind().load(function);
                    function
                        .instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
                    function.instruction(&Instruction::I32Eq);
                    self.open_frame(ControlFrameKind::If, function);
                    self.fold_error_into_async_dispose_pending_completion(
                        called.value(),
                        function,
                    )?;
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                    function.instruction(&Instruction::I32Const(0));
                    should_await.store(function);
                }
            }
            function.instruction(&Instruction::Else);
            arms += 1;
        }
        function.instruction(&Instruction::Unreachable);
        for _ in 0..arms {
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        stored_method.clear(function);
        stored_value.clear(function);
        entry.clear(function);
        should_await.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_activation_async_dispose_await(
            owner,
            &capability,
            &awaited,
            finalizer,
            ActivationAsyncDisposeAwaitRole::Method,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        row.field(ActivationAsyncDisposeCapabilitySchema::NEEDS_AWAIT)
            .read(&capability.record, schema, function);
        row.field(ActivationAsyncDisposeCapabilitySchema::HAS_AWAITED)
            .read(&capability.record, schema, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        // Clear the pending empty-resource await before suspension. A resume at
        // cursor zero must finish this pass instead of awaiting a second time.
        row.field(ActivationAsyncDisposeCapabilitySchema::NEEDS_AWAIT)
            .write(
                &capability.record,
                GcOperand::boolean(false),
                schema,
                function,
            );
        awaited.set_undefined(function);
        self.emit_activation_async_dispose_await(
            owner,
            &capability,
            &awaited,
            finalizer,
            ActivationAsyncDisposeAwaitRole::Method,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        arguments.clear(function);
        called.clear(function);
        awaited.clear(function);
        method.clear(function);
        value.clear(function);
        schema.release_i32_local(should_await, function);
        schema.release_i32_local(kind, function);
        schema.release_i32_local(index, function);
        schema.release_i64_local(cursor, function);
        row.field(ActivationAsyncDisposeCapabilitySchema::STATE)
            .write(
                &capability.record,
                GcOperand::constant(ActivationAsyncDisposeCapabilityState::Disposed),
                schema,
                function,
            );
        let cell = self.control_flow_owned_binding_cell(disposing.storage.binding, function)?;
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::ASYNC_DISPOSE_CAPABILITY)
            .write(&cell, GcOperand::null(schema), schema, function);
        cell.clear(function);
        self.finish_async_dispose_pending_completion(pending, function)?;
        match continuation {
            ActivationAsyncDisposeCompletionContinuation::Scope => {
                self.emit_set_resumable_resume_point(finalizer.exit_state(), function)?;
                self.emit_dispatch_activation_async_dispose_completion(owner, function)?;
            }
            ActivationAsyncDisposeCompletionContinuation::CompleteMixedScope => {
                self.emit_set_resumable_resume_point(finalizer.exit_state(), function)?;
                self.emit_save_resumable_environment(function)?;
                self.emit_dispatch_current_completion(function)?;
            }
            ActivationAsyncDisposeCompletionContinuation::ClassicFor {
                lexical_environment,
                break_target,
            } => {
                self.emit_set_resumable_resume_point(finalizer.exit_state(), function)?;
                if let ClassicForAsyncDisposeLexicalEnvironment::Active = lexical_environment {
                    self.emit_leave_lexical_environment(function);
                }
                self.emit_dispatch_activation_async_dispose_completion(owner, function)?;
                self.emit_branch_to_target(break_target, function);
            }
            ActivationAsyncDisposeCompletionContinuation::ForOf(continuation) => self
                .finish_async_disposable_for_of_iteration(
                    owner,
                    finalizer,
                    continuation,
                    function,
                )?,
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(resume_throw, function);
        resumed.clear(function);
        self.release_active_activation_async_dispose_capability(capability, function);
        Ok(())
    }

    fn finish_async_disposable_for_of_iteration(
        &mut self,
        owner: &ActivationAsyncDisposeOwner<'_>,
        finalizer: &AsyncDisposableFinalizerPlanIr,
        continuation: AsyncDisposableForOfCompletionContinuationLocals<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        if let AsyncDisposableForOfIterationEnvironment::Active = continuation.iteration_environment
        {
            self.emit_leave_lexical_environment(function);
            self.emit_save_resumable_environment(function)?;
        }
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(
            CompletionKind::Continue.code() as i32
        ));
        function.instruction(&Instruction::I32Eq);
        self.completion().target().load(function);
        function.instruction(&Instruction::I32Const(
            continuation.continue_target.frame as i32,
        ));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.set_completion_kind(CompletionKind::Normal, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_set_resumable_resume_point(finalizer.entry_state(), function)?;
        function.instruction(&Instruction::I32Const(finalizer.entry_state() as i32));
        continuation.state.store(function);
        self.emit_branch_to_target(continuation.loop_target, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_set_resumable_resume_point(finalizer.exit_state(), function)?;
        let pending = schema.reserve_completion(function);
        let closed = schema.reserve_completion(function);
        pending.copy_from(self.completion(), function);
        self.emit_sync_iterator_close(continuation.iterator, &pending, &closed, function)?;
        self.completion().copy_from(&closed, function);
        closed.clear(function);
        pending.clear(function);
        self.emit_dispatch_activation_async_dispose_completion(owner, function)
    }

    fn finish_async_dispose_pending_completion(
        &mut self,
        _pending: ActiveAsyncDisposePendingCompletion,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_pop_and_restore_async_pending_completion(function)
    }

    fn compile_sync_disposable_scope(
        &mut self,
        execution: &SyncDisposableScopeExecutionIr,
        resources: &SyncDisposableResourcesIr,
        body: &BlockIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match execution {
            SyncDisposableScopeExecutionIr::Immediate => {
                self.compile_immediate_sync_disposable_scope(resources, body, function)
            }
            SyncDisposableScopeExecutionIr::PlainGenerator(capability) => self
                .compile_activation_sync_disposable_scope(
                    ActivationSyncDisposeOwner::PlainGenerator(capability),
                    resources,
                    body,
                    function,
                ),
            SyncDisposableScopeExecutionIr::AsyncFunction(capability) => self
                .compile_activation_sync_disposable_scope(
                    ActivationSyncDisposeOwner::AsyncFunction(capability),
                    resources,
                    body,
                    function,
                ),
            SyncDisposableScopeExecutionIr::AsyncGenerator(capability) => self
                .compile_activation_sync_disposable_scope(
                    ActivationSyncDisposeOwner::AsyncGenerator(capability),
                    resources,
                    body,
                    function,
                ),
        }
    }

    fn compile_immediate_sync_disposable_scope(
        &mut self,
        resources: &SyncDisposableResourcesIr,
        body: &BlockIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        debug_assert!(!resources.is_empty());
        let acquired = resources
            .iter()
            .map(|_| self.reserve_sync_disposable_resource_locals(function))
            .collect::<Vec<_>>();

        let _outer_frame = self.open_frame(ControlFrameKind::Block, function);
        let disposal_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(disposal_frame);
        for (resource, locals) in resources.iter().zip(&acquired) {
            self.compile_sync_disposable_resource(resource, locals, function)?;
        }
        self.push_scope();
        self.compile_block_contents(body, function)?;
        self.pop_scope();
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        let pending = self.capture_pending_sync_dispose_completion(function);
        self.set_completion_kind(CompletionKind::Normal, function);
        self.consume_sync_disposable_resources(
            pending,
            acquired,
            SyncDisposeCompletionContinuation::Dispatch,
            function,
        )?;

        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_activation_sync_disposable_scope(
        &mut self,
        owner: ActivationSyncDisposeOwner<'_>,
        resources: &SyncDisposableResourcesIr,
        body: &BlockIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        debug_assert!(!resources.is_empty());
        let execution_kind = owner.execution_kind();
        if !self
            .current_function_meta()
            .is_some_and(|meta| meta.protocol().execution_kind() == execution_kind)
        {
            return Err(EmitError::unsupported(
                "activation-backed synchronous DisposeCapability has the wrong execution owner",
            ));
        }
        let binding = self
            .activation_owned_binding_storage(owner.binding_name())
            .ok_or_else(|| {
                EmitError::unsupported(
                    "activation-backed synchronous DisposeCapability is missing its owned binding",
                )
            })?;
        let capability_storage = ActivationSyncDisposeCapabilityStorage { binding };
        let (entry_state_of, exit_state_of): (
            fn(&StatementIr) -> Option<u32>,
            fn(&StatementIr) -> Option<u32>,
        ) = match &owner {
            ActivationSyncDisposeOwner::PlainGenerator(_) => (
                Self::generator_statement_entry_state,
                Self::generator_statement_exit_state,
            ),
            ActivationSyncDisposeOwner::AsyncFunction(_) => (
                Self::async_statement_entry_state,
                Self::async_statement_exit_state,
            ),
            ActivationSyncDisposeOwner::AsyncGenerator(_) => (
                Self::async_statement_entry_state,
                Self::async_statement_exit_state,
            ),
        };
        let entry_state = body.statements.iter().find_map(entry_state_of);
        let exit_state = body.statements.iter().rev().find_map(exit_state_of);
        let suspension_span = match (entry_state, exit_state) {
            (None, None) => None,
            (Some(entry_state), Some(exit_state)) => Some((entry_state, exit_state)),
            _ => {
                return Err(EmitError::unsupported(
                    "activation-backed using body has an incomplete suspension-state span",
                ));
            }
        };

        if let Some((entry_state, exit_state)) = suspension_span {
            self.emit_resumable_state_in_range(entry_state, exit_state, true, function)?;
            self.open_frame(ControlFrameKind::If, function);
        }

        let _outer_frame = self.open_frame(ControlFrameKind::Block, function);
        let disposal_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(disposal_frame);

        if let Some((entry_state, _)) = suspension_span {
            self.emit_resumable_state_equals(entry_state, function)?;
            self.open_frame(ControlFrameKind::If, function);
        }
        self.initialize_activation_sync_dispose_capability(
            &capability_storage,
            resources,
            function,
        )?;
        if suspension_span.is_some() {
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }

        self.push_scope();
        if let Some((entry_state, _)) = suspension_span {
            self.compile_resumable_block_contents(body, entry_state, true, function)?;
        } else {
            self.compile_block_contents(body, function)?;
        }
        self.pop_scope();
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        let detached =
            self.detach_activation_sync_dispose_capability(capability_storage, function)?;
        let acquired = self.load_detached_activation_sync_disposable_resources(
            &detached,
            resources.len(),
            function,
        );
        self.release_detached_activation_sync_dispose_capability(detached, function);
        let pending = self.capture_pending_sync_dispose_completion(function);
        self.set_completion_kind(CompletionKind::Normal, function);
        self.consume_sync_disposable_resources(
            pending,
            acquired,
            owner.completion_continuation(),
            function,
        )?;

        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if suspension_span.is_some() {
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        Ok(())
    }

    fn initialize_sync_disposable_resource_bindings(
        &mut self,
        resources: &SyncDisposableResourcesIr,
        function: &mut Function,
    ) {
        for resource in resources.iter() {
            let storage = self
                .lookup_current_scope_binding(&resource.binding_name)
                .or_else(|| self.lookup_binding(&resource.binding_name))
                .unwrap_or_else(|| {
                    self.allocate_binding(
                        resource.binding_name.clone(),
                        BindingMode::Const,
                        resource.initializer.kind,
                        function,
                    )
                });
            self.initialize_binding_uninitialized(storage, function);
        }
    }

    fn control_flow_owned_binding_cell(
        &mut self,
        binding: BindingStorage,
        function: &mut Function,
    ) -> Result<GcLocal<BindingCell>, EmitError> {
        let BindingStorage::EnvSlot { slot, hops } = binding else {
            return Err(EmitError::unsupported(
                "compiler-private resumable capability requires an owned environment cell",
            ));
        };
        let environment = self.resolve_env_handle_local(hops, function);
        let cell = self.emit_environment_cell_local(&environment, slot, function);
        environment.clear(function);
        Ok(cell)
    }

    fn initialize_activation_sync_dispose_capability(
        &mut self,
        storage: &ActivationSyncDisposeCapabilityStorage,
        resources: &SyncDisposableResourcesIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let capability = self.initialize_empty_activation_sync_dispose_capability(
            storage,
            resources.len(),
            function,
        )?;
        // Publish the private capability before any initializer/GetMethod may throw.
        for resource in resources.iter() {
            let acquired = self.reserve_sync_disposable_resource_locals(function);
            self.compile_expr_to_value(&resource.initializer, &acquired.value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            self.acquire_sync_disposable_resource_from_locals(&acquired, function)?;
            self.append_activation_sync_disposable_resource(&capability, &acquired, function);
            let binding = self
                .lookup_current_scope_binding(&resource.binding_name)
                .or_else(|| self.lookup_binding(&resource.binding_name))
                .unwrap_or_else(|| {
                    self.allocate_binding(
                        resource.binding_name.clone(),
                        BindingMode::Const,
                        resource.initializer.kind,
                        function,
                    )
                });
            self.write_binding_from_locals(binding, &acquired.value, function);
            self.release_sync_disposable_resource_locals(acquired, function);
        }
        capability.entries.clear(function);
        capability.record.clear(function);
        Ok(())
    }

    fn initialize_empty_activation_sync_dispose_capability(
        &mut self,
        storage: &ActivationSyncDisposeCapabilityStorage,
        resource_capacity: usize,
        function: &mut Function,
    ) -> Result<ActiveActivationSyncDisposeCapabilityLocals, EmitError> {
        let schema = self.runtime_schema();
        let capacity = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(
            i32::try_from(resource_capacity)
                .map_err(|_| EmitError::unsupported("too many synchronous disposal resources"))?,
        ));
        capacity.store(function);
        let entries = schema
            .reserve_gc_local::<DisposableResourceTable, NonNullable>(function)
            .initialize(
                schema.array_type::<DisposableResourceTable>().filled(
                    GcOperand::null(schema),
                    capacity,
                    function,
                ),
                function,
            );
        let record = schema
            .reserve_gc_local::<ActivationSyncDisposeCapability, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<ActivationSyncDisposeCapability>()
                    .construct(
                        (
                            GcOperand::constant(DisposableStackState::Pending),
                            GcOperand::reference(&entries, schema),
                            GcOperand::i64(0),
                        ),
                        function,
                    ),
                function,
            );
        let capability = ActiveActivationSyncDisposeCapabilityLocals { record, entries };
        let cell = self.control_flow_owned_binding_cell(storage.binding, function)?;
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::SYNC_DISPOSE_CAPABILITY)
            .write(
                &cell,
                GcOperand::nullable_reference(&capability.record, schema),
                schema,
                function,
            );
        cell.clear(function);
        schema.release_i32_local(capacity, function);
        Ok(capability)
    }

    fn load_activation_sync_dispose_capability(
        &mut self,
        storage: &ActivationSyncDisposeCapabilityStorage,
        function: &mut Function,
    ) -> Result<ActiveActivationSyncDisposeCapabilityLocals, EmitError> {
        let schema = self.runtime_schema();
        let cell = self.control_flow_owned_binding_cell(storage.binding, function)?;
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<BindingCell>()
                .field(BindingCellSchema::SYNC_DISPOSE_CAPABILITY)
                .read(&cell, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let entries = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ActivationSyncDisposeCapability>()
                .field(ActivationSyncDisposeCapabilitySchema::RESOURCES)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        cell.clear(function);
        Ok(ActiveActivationSyncDisposeCapabilityLocals { record, entries })
    }

    fn append_activation_sync_disposable_resource(
        &mut self,
        capability: &ActiveActivationSyncDisposeCapabilityLocals,
        resource: &AcquiredSyncDisposableResourceLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        resource.registered.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let count = schema.reserve_i64_local(function);
        let index = schema.reserve_i32_local(function);
        schema
            .struct_type::<ActivationSyncDisposeCapability>()
            .field(ActivationSyncDisposeCapabilitySchema::ENTRY_COUNT)
            .read(&capability.record, schema, function)
            .store_i64(count, function);
        count.load(function);
        function.instruction(&Instruction::I32WrapI64);
        index.store(function);
        let value = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&resource.value, function),
            function,
        );
        let method = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&resource.method, function),
            function,
        );
        let entry = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<DisposableResource>().construct(
                (
                    GcOperand::constant(DisposableStackEntryKind::Use),
                    GcOperand::reference(&value, schema),
                    GcOperand::reference(&method, schema),
                ),
                function,
            ),
            function,
        );
        schema.array_type::<DisposableResourceTable>().write(
            &capability.entries,
            index,
            GcOperand::nullable_reference(&entry, schema),
            schema,
            function,
        );
        count.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        count.store(function);
        schema
            .struct_type::<ActivationSyncDisposeCapability>()
            .field(ActivationSyncDisposeCapabilitySchema::ENTRY_COUNT)
            .write(
                &capability.record,
                GcOperand::i64_local(count),
                schema,
                function,
            );
        entry.clear(function);
        method.clear(function);
        value.clear(function);
        schema.release_i32_local(index, function);
        schema.release_i64_local(count, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    fn detach_activation_sync_dispose_capability(
        &mut self,
        storage: ActivationSyncDisposeCapabilityStorage,
        function: &mut Function,
    ) -> Result<DetachedActivationSyncDisposeCapabilityLocals, EmitError> {
        let schema = self.runtime_schema();
        let cell = self.control_flow_owned_binding_cell(storage.binding, function)?;
        let record = schema
            .reserve_gc_local::<ActivationSyncDisposeCapability, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<BindingCell>()
                    .field(BindingCellSchema::SYNC_DISPOSE_CAPABILITY)
                    .read(&cell, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            );
        schema
            .struct_type::<BindingCell>()
            .field(BindingCellSchema::SYNC_DISPOSE_CAPABILITY)
            .write(&cell, GcOperand::null(schema), schema, function);
        cell.clear(function);
        let entries = schema
            .reserve_gc_local::<DisposableResourceTable, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<ActivationSyncDisposeCapability>()
                    .field(ActivationSyncDisposeCapabilitySchema::RESOURCES)
                    .read(&record, schema, function)
                    .reference(),
                function,
            );
        let entry_count = schema.reserve_i64_local(function);
        schema
            .struct_type::<ActivationSyncDisposeCapability>()
            .field(ActivationSyncDisposeCapabilitySchema::ENTRY_COUNT)
            .read(&record, schema, function)
            .store_i64(entry_count, function);
        schema
            .struct_type::<ActivationSyncDisposeCapability>()
            .field(ActivationSyncDisposeCapabilitySchema::STATE)
            .write(
                &record,
                GcOperand::constant(DisposableStackState::Disposed),
                schema,
                function,
            );
        schema
            .struct_type::<ActivationSyncDisposeCapability>()
            .field(ActivationSyncDisposeCapabilitySchema::ENTRY_COUNT)
            .write(&record, GcOperand::i64(0), schema, function);
        Ok(DetachedActivationSyncDisposeCapabilityLocals {
            record,
            entries,
            entry_count,
        })
    }

    fn load_detached_activation_sync_disposable_resources(
        &mut self,
        detached: &DetachedActivationSyncDisposeCapabilityLocals,
        resource_count: usize,
        function: &mut Function,
    ) -> Vec<AcquiredSyncDisposableResourceLocals> {
        let schema = self.runtime_schema();
        (0..resource_count)
            .map(|ordinal| {
                let resource = self.reserve_sync_disposable_resource_locals(function);
                function.instruction(&Instruction::I64Const(ordinal as i64));
                detached.entry_count.load(function);
                function.instruction(&Instruction::I64LtU);
                resource.registered.store(function);
                resource.registered.load(function);
                self.open_frame(ControlFrameKind::If, function);
                let index = schema.reserve_i32_local(function);
                function.instruction(&Instruction::I32Const(ordinal as i32));
                index.store(function);
                let entry = schema
                    .reserve_gc_local::<DisposableResource, NonNullable>(function)
                    .initialize(
                        schema
                            .array_type::<DisposableResourceTable>()
                            .read(&detached.entries, index, schema, function)
                            .reference()
                            .require_non_null(function),
                        function,
                    );
                let value = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<DisposableResource>()
                        .field(DisposableResourceSchema::VALUE)
                        .read(&entry, schema, function)
                        .reference(),
                    function,
                );
                let method = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<DisposableResource>()
                        .field(DisposableResourceSchema::METHOD)
                        .read(&entry, schema, function)
                        .reference(),
                    function,
                );
                schema.struct_type::<StoredValue>().read_into(
                    &value,
                    &resource.value,
                    schema,
                    function,
                );
                schema.struct_type::<StoredValue>().read_into(
                    &method,
                    &resource.method,
                    schema,
                    function,
                );
                method.clear(function);
                value.clear(function);
                entry.clear(function);
                schema.release_i32_local(index, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                resource
            })
            .collect()
    }

    fn release_detached_activation_sync_dispose_capability(
        &mut self,
        detached: DetachedActivationSyncDisposeCapabilityLocals,
        function: &mut Function,
    ) {
        self.runtime_schema()
            .release_i64_local(detached.entry_count, function);
        detached.entries.clear(function);
        detached.record.clear(function);
    }

    fn reserve_sync_disposable_resource_locals(
        &mut self,
        function: &mut Function,
    ) -> AcquiredSyncDisposableResourceLocals {
        let schema = self.runtime_schema();
        let resource = AcquiredSyncDisposableResourceLocals {
            registered: schema.reserve_i32_local(function),
            value: schema.reserve_value_local(function),
            method: schema.reserve_value_local(function),
        };
        self.reset_sync_disposable_resource_locals(&resource, function);
        resource
    }
    fn reset_sync_disposable_resource_locals(
        &mut self,
        resource: &AcquiredSyncDisposableResourceLocals,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I32Const(0));
        resource.registered.store(function);
        resource.value.set_undefined(function);
        resource.method.set_undefined(function);
    }
    fn release_sync_disposable_resource_locals(
        &mut self,
        resource: AcquiredSyncDisposableResourceLocals,
        function: &mut Function,
    ) {
        resource.method.clear(function);
        resource.value.clear(function);
        self.runtime_schema()
            .release_i32_local(resource.registered, function);
    }
    fn compile_sync_disposable_resource(
        &mut self,
        resource: &SyncDisposableResourceIr,
        locals: &AcquiredSyncDisposableResourceLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_expr_to_value(&resource.initializer, &locals.value, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        let binding = self
            .lookup_current_scope_binding(&resource.binding_name)
            .or_else(|| self.lookup_binding(&resource.binding_name))
            .unwrap_or_else(|| {
                self.allocate_binding(
                    resource.binding_name.clone(),
                    BindingMode::Const,
                    resource.initializer.kind,
                    function,
                )
            });
        self.compile_sync_disposable_resource_from_locals(binding, locals, function)
    }

    fn emit_control_flow_type_error(
        &mut self,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let pending = self.runtime_schema().reserve_completion(function);
        self.emit_throw_runtime_error_to_active_handler(
            NativeErrorKind::TypeError,
            message,
            &pending,
            function,
        )?;
        pending.clear(function);
        Ok(())
    }
    fn acquire_sync_disposable_resource_from_locals(
        &mut self,
        resource: &AcquiredSyncDisposableResourceLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.compile_nullish_tagged_i32(resource.value.tag(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_heap_object_like_tag_i32(resource.value.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_control_flow_type_error(
            RuntimeErrorMessage::USING_DECLARATION_RESOURCE_IS_NOT_AN_OBJECT,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(scriptw_ir::WellKnownSymbol::Dispose, function)?,
            function,
        );
        let key = crate::operations::PropertyKeyLocals::from_symbol(schema, &symbol, function);
        let pending = schema.reserve_completion(function);
        self.emit_object_read(&resource.value, &resource.value, &key, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        resource.method.copy_from(pending.value(), function);
        pending.clear(function);
        key.clear(function);
        symbol.clear(function);
        self.compile_nullish_tagged_i32(resource.method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_control_flow_type_error(
            RuntimeErrorMessage::USING_DECLARATION_RESOURCE_HAS_NO_SYMBOL_DISPOSE_METHOD,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_callable_i32(&resource.method, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_control_flow_type_error(
            RuntimeErrorMessage::USING_DECLARATION_SYMBOL_DISPOSE_METHOD_IS_NOT_CALLABLE,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // Publication follows every GetMethod and callability observation.
        function.instruction(&Instruction::I32Const(1));
        resource.registered.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }
    fn compile_sync_disposable_resource_from_locals(
        &mut self,
        binding: BindingStorage,
        resource: &AcquiredSyncDisposableResourceLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.acquire_sync_disposable_resource_from_locals(resource, function)?;
        self.write_binding_from_locals(binding, &resource.value, function);
        Ok(())
    }
    fn capture_pending_sync_dispose_completion(
        &mut self,
        function: &mut Function,
    ) -> PendingSyncDisposeCompletionLocals {
        let completion = self.runtime_schema().reserve_completion(function);
        completion.copy_from(self.completion(), function);
        PendingSyncDisposeCompletionLocals { completion }
    }
    fn consume_sync_disposable_resources(
        &mut self,
        pending: PendingSyncDisposeCompletionLocals,
        resources: Vec<AcquiredSyncDisposableResourceLocals>,
        continuation: SyncDisposeCompletionContinuation,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let called = schema.reserve_completion(function);
        let combined = schema.reserve_completion(function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
        for resource in resources.iter().rev() {
            resource.registered.load(function);
            self.open_frame(ControlFrameKind::If, function);
            // Detach before callback invocation: reentry cannot dispose this entry twice.
            function.instruction(&Instruction::I32Const(0));
            resource.registered.store(function);
            self.emit_function_or_proxy_call_with_argv(
                &resource.method,
                &resource.value,
                &arguments,
                &called,
                function,
            )?;
            called.kind().load(function);
            function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            pending.completion.kind().load(function);
            function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            let realm = self.load_current_realm(function);
            let prototype = schema.reserve_value_local(function);
            self.emit_load_non_array_realm_intrinsic(
                &realm,
                crate::functions::NonArrayRealmIntrinsicSlot::SuppressedErrorPrototype,
                &prototype,
                function,
            );
            self.emit_alloc_suppressed_error_instance(
                None,
                called.value(),
                pending.completion.value(),
                &prototype,
                &combined,
                function,
            )?;
            pending.completion.set_throw(combined.value(), function);
            prototype.clear(function);
            realm.clear(function);
            function.instruction(&Instruction::Else);
            pending.completion.copy_from(&called, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.completion().copy_from(&pending.completion, function);
        arguments.clear(function);
        combined.clear(function);
        called.clear(function);
        pending.completion.clear(function);
        for resource in resources.into_iter().rev() {
            self.release_sync_disposable_resource_locals(resource, function);
        }
        match continuation {
            SyncDisposeCompletionContinuation::Dispatch => {
                self.emit_dispatch_current_completion(function)?
            }
            SyncDisposeCompletionContinuation::DispatchAsyncFunction => {
                self.emit_dispatch_async_completion(function)?
            }
            SyncDisposeCompletionContinuation::DispatchAsyncGenerator => {
                self.emit_dispatch_async_generator_completion(function)
            }
            SyncDisposeCompletionContinuation::DeferToIteratorClose => {}
        }
        Ok(())
    }

    pub(crate) fn compile_try_catch_finally(
        &mut self,
        try_block: &BlockIr,
        catch_name: &str,
        catch_source_name: &str,
        catch_parameter_environment: Option<&LexicalEnvironmentIr>,
        catch_block: &BlockIr,
        finally_block: &BlockIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_statement_result(function);
        let saved = self.runtime_schema().reserve_completion(function);

        let _outer_frame = self.open_frame(ControlFrameKind::Block, function);
        let _finally_frame = self.open_frame(ControlFrameKind::Block, function);
        let catch_skip_frame = self.open_frame(ControlFrameKind::Block, function);
        let catch_frame = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(catch_frame);
        // `br` targets exit the selected block. In this layout, branching to
        // `finally_frame` would therefore skip the finalizer itself. The
        // catch-skip block instead ends immediately before the finalizer, so
        // it is the continuation target for abrupt completions from either
        // the try or catch body.
        self.finally_stack.push(catch_skip_frame);
        self.push_scope();
        self.compile_block_contents(try_block, function)?;
        self.pop_scope();
        self.throw_handler_stack.pop();
        self.emit_branch_to_target(catch_skip_frame, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.push_scope();
        if let Some(environment) = catch_parameter_environment {
            self.emit_enter_lexical_environment(environment, function)?;
        }
        let catch_storage = self
            .lookup_current_scope_binding(catch_name)
            .unwrap_or_else(|| self.allocate_dynamic_binding_storage(catch_name, function));
        self.binding_scopes
            .last_mut()
            .expect("binding scope stack must exist")
            .insert(catch_name.to_string(), catch_storage);
        if catch_source_name != catch_name {
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(catch_source_name.to_string(), catch_storage);
        }
        let caught = self.runtime_schema().reserve_value_local(function);
        caught.copy_from(self.completion().value(), function);
        self.write_binding_from_locals(catch_storage, &caught, function);
        caught.clear(function);
        self.emit_statement_result(function);
        self.push_scope();
        self.compile_block_contents(catch_block, function)?;
        self.pop_scope();
        if catch_parameter_environment.is_some() {
            self.emit_leave_lexical_environment(function);
        }
        self.pop_scope();
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.save_current_completion(&saved, function);
        self.emit_statement_result(function);
        self.push_scope();
        self.compile_block_contents(finally_block, function)?;
        self.pop_scope();
        self.emit_resume_after_finally(&saved, function)?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        saved.clear(function);
        Ok(())
    }

    pub(crate) fn compile_while(
        &mut self,
        condition: &TypedExpr,
        body: &StatementIr,
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_statement_result(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);
        let continue_frame = self.open_frame(ControlFrameKind::Loop, function);
        self.loop_stack.push(LoopTargets { continue_frame });
        self.push_labels(labels, break_frame, Some(continue_frame));
        self.compile_iteration_condition(condition, function)?;
        function.instruction(&Instruction::I32Eqz);
        function.branch_if_to_label(break_frame.label);
        self.compile_statement(body, function)?;
        function.branch_to_label(continue_frame.label);
        self.pop_labels(labels.len());
        self.loop_stack.pop();
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_do_while(
        &mut self,
        body: &StatementIr,
        condition: &TypedExpr,
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_statement_result(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);
        let loop_frame = self.open_frame(ControlFrameKind::Loop, function);
        let continue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.loop_stack.push(LoopTargets { continue_frame });
        self.push_labels(labels, break_frame, Some(continue_frame));
        self.compile_statement(body, function)?;
        self.pop_labels(labels.len());
        self.loop_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.compile_iteration_condition(condition, function)?;
        function.branch_if_to_label(loop_frame.label);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_for(
        &mut self,
        init: Option<&ForInitIr>,
        test: Option<&TypedExpr>,
        update: Option<&TypedExpr>,
        body: &StatementIr,
        lexical_environment: Option<&ForLexicalEnvironmentIr>,
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if let Some(ForInitIr::SyncDisposable(resources)) = init {
            return self.compile_sync_disposable_for(
                resources,
                test,
                update,
                SynchronousLoopBodyIr::new(body).map_err(|error| {
                    EmitError::unsupported(format!(
                        "invalid synchronous resource loop body: {error:?}"
                    ))
                })?,
                lexical_environment,
                labels,
                function,
            );
        }
        if let Some(ForInitIr::AsyncDisposable(init)) = init {
            return self.compile_async_disposable_for(
                init,
                test,
                update,
                body,
                lexical_environment,
                labels,
                function,
            );
        }

        self.push_scope();
        self.emit_statement_result(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);
        let runtime_environment = lexical_environment.map(|environment| LexicalEnvironmentIr {
            initialization: scriptw_ir::LexicalEnvironmentInitializationIr::Uninitialized,
            eval_environment: environment.eval_environment.clone(),
            bindings: environment.bindings.clone(),
        });
        if let Some(environment) = &runtime_environment {
            self.emit_enter_lexical_environment(environment, function)?;
        }
        if let Some(init) = init {
            let saved = self.save_statement_list_value(function);
            self.compile_for_init(init, function)?;
            self.restore_statement_list_value(saved, function)?;
        }
        if let Some(environment) = lexical_environment {
            self.emit_replace_lexical_environment(environment, function)?;
        }
        let scalar_plan = lexical_environment
            .is_none()
            .then(|| scalar_number_loop::ScalarNumberLoop::new(self, test, update, body))
            .flatten();
        if let Some(plan) = scalar_plan {
            plan.emit_entry_guard(self, function)?;
            self.open_frame(ControlFrameKind::If, function);
            self.compile_scalar_number_loop(&plan, function);
            function.instruction(&Instruction::Else);
            self.compile_classic_for_iterations(
                test,
                update,
                body,
                lexical_environment,
                labels,
                break_frame,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        } else {
            self.compile_classic_for_iterations(
                test,
                update,
                body,
                lexical_environment,
                labels,
                break_frame,
                function,
            )?;
        }
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if runtime_environment.is_some() {
            self.end_lexical_environment_scope();
        }
        self.pop_scope();
        Ok(())
    }

    fn compile_classic_for_iterations(
        &mut self,
        test: Option<&TypedExpr>,
        update: Option<&TypedExpr>,
        body: &StatementIr,
        lexical_environment: Option<&ForLexicalEnvironmentIr>,
        labels: &[String],
        break_frame: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let loop_frame = self.open_frame(ControlFrameKind::Loop, function);
        if let Some(test) = test {
            self.compile_classic_for_test(test, break_frame, function)?;
        }
        let continue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.loop_stack.push(LoopTargets { continue_frame });
        self.push_labels(labels, break_frame, Some(continue_frame));
        self.compile_statement(body, function)?;
        self.pop_labels(labels.len());
        self.loop_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if let Some(environment) = lexical_environment {
            self.emit_replace_lexical_environment(environment, function)?;
        }
        if let Some(update) = update {
            self.compile_classic_for_update(update, function)?;
        }
        function.branch_to_label(loop_frame.label);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn compile_classic_for_test(
        &mut self,
        test: &TypedExpr,
        false_target: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_iteration_condition(test, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(false_target, function);
        Ok(())
    }

    fn compile_classic_for_update(
        &mut self,
        update: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let saved = self.save_statement_list_value(function);
        let value = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(update, &value, function)?;
        value.clear(function);
        self.restore_statement_list_value(saved, function)
    }

    fn compile_async_disposable_for(
        &mut self,
        init: &AsyncDisposableForInitIr,
        test: Option<&TypedExpr>,
        update: Option<&TypedExpr>,
        body: &StatementIr,
        lexical_environment: Option<&ForLexicalEnvironmentIr>,
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !self
            .current_function_meta()
            .is_some_and(|meta| meta.protocol().execution_kind() == FunctionExecutionKind::Async)
        {
            return Err(EmitError::unsupported(
                "await using for head requires a plain async function",
            ));
        }
        if lexical_environment
            .is_some_and(|environment| !environment.per_iteration_slots.is_empty())
        {
            return Err(EmitError::unsupported(
                "await using for head cannot own per-iteration bindings",
            ));
        }

        let resources = init.resources();
        debug_assert!(!resources.is_empty());
        let owner = ActivationAsyncDisposeOwner::AsyncFunction(init.capability());
        let finalizer = owner.finalizer();

        self.push_scope();
        self.emit_statement_result(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);

        self.emit_async_state_in_range(finalizer.entry_state(), finalizer.exit_state(), function)?;
        self.open_frame(ControlFrameKind::If, function);

        let runtime_environment = lexical_environment.map(|environment| LexicalEnvironmentIr {
            initialization: scriptw_ir::LexicalEnvironmentInitializationIr::Uninitialized,
            eval_environment: environment.eval_environment.clone(),
            bindings: environment.bindings.clone(),
        });
        if let Some(environment) = &runtime_environment {
            // Async body re-entry reconstructs lexical environments from the
            // activation root. The original loop record remains reachable by
            // closures created before disposal; this fresh record is used only
            // to restore the binding layout and parent chain while finalizing.
            self.emit_enter_lexical_environment(environment, function)?;
        }

        // Resolve after entering or reattaching the loop environment. Its
        // hidden activation-owned binding therefore carries the adjusted hop
        // count instead of aliasing a loop-head slot with the same raw index.
        let binding = self
            .activation_owned_binding_storage(owner.binding_name())
            .ok_or_else(|| {
                EmitError::unsupported(
                    "async DisposeCapability is missing its activation-owned binding",
                )
            })?;
        let storage = ActivationAsyncDisposeCapabilityStorage { binding };

        let _outer_frame = self.open_frame(ControlFrameKind::Block, function);
        let disposal_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(disposal_frame);

        self.emit_async_state_in_range(
            finalizer.entry_state(),
            finalizer.dispose_state(),
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_resumable_state_equals(finalizer.entry_state(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.initialize_async_disposable_resource_bindings(resources, function);
        self.initialize_activation_async_dispose_capability(&storage, resources, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let loop_frame = self.open_frame(ControlFrameKind::Loop, function);
        if let Some(test) = test {
            self.compile_classic_for_test(test, disposal_frame, function)?;
        }
        let continue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.loop_stack.push(LoopTargets { continue_frame });
        self.push_labels(labels, break_frame, Some(continue_frame));
        self.compile_statement(body, function)?;
        self.pop_labels(labels.len());
        self.loop_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if let Some(update) = update {
            self.compile_classic_for_update(update, function)?;
        }
        function.branch_to_label(loop_frame.label);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        self.emit_resumable_finalizer_needs_pending_completion(
            finalizer.dispose_state(),
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        let pending = self.begin_async_dispose_pending_completion(function)?;
        self.set_completion_kind(CompletionKind::Normal, function);
        let disposing =
            self.begin_activation_async_dispose_capability(storage, finalizer, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.consume_activation_async_dispose_capability(
            &owner,
            disposing,
            pending,
            finalizer,
            ActivationAsyncDisposeCompletionContinuation::ClassicFor {
                lexical_environment: if runtime_environment.is_some() {
                    ClassicForAsyncDisposeLexicalEnvironment::Active
                } else {
                    ClassicForAsyncDisposeLexicalEnvironment::Absent
                },
                break_target: break_frame,
            },
            function,
        )?;

        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_scope();
        Ok(())
    }

    fn compile_sync_disposable_for(
        &mut self,
        resources: &SyncDisposableResourcesIr,
        test: Option<&TypedExpr>,
        update: Option<&TypedExpr>,
        body: SynchronousLoopBodyIr<'_>,
        lexical_environment: Option<&ForLexicalEnvironmentIr>,
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let body = body.statement();
        if lexical_environment
            .is_some_and(|environment| !environment.per_iteration_slots.is_empty())
        {
            return Err(EmitError::unsupported(
                "synchronous using for head cannot own per-iteration bindings",
            ));
        }
        debug_assert!(!resources.is_empty());

        self.push_scope();
        self.emit_statement_result(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);
        let runtime_environment = lexical_environment.map(|environment| LexicalEnvironmentIr {
            initialization: scriptw_ir::LexicalEnvironmentInitializationIr::Uninitialized,
            eval_environment: environment.eval_environment.clone(),
            bindings: environment.bindings.clone(),
        });
        if let Some(environment) = &runtime_environment {
            self.emit_enter_lexical_environment(environment, function)?;
        }
        self.initialize_sync_disposable_resource_bindings(resources, function);

        let acquired = resources
            .iter()
            .map(|_| self.reserve_sync_disposable_resource_locals(function))
            .collect::<Vec<_>>();
        let _outer_frame = self.open_frame(ControlFrameKind::Block, function);
        let disposal_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(disposal_frame);
        for (resource, locals) in resources.iter().zip(&acquired) {
            self.compile_sync_disposable_resource(resource, locals, function)?;
        }

        if let Some(environment) = lexical_environment {
            self.emit_replace_lexical_environment(environment, function)?;
        }
        let loop_frame = self.open_frame(ControlFrameKind::Loop, function);
        if let Some(test) = test {
            self.compile_classic_for_test(test, disposal_frame, function)?;
        }
        let continue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.loop_stack.push(LoopTargets { continue_frame });
        self.push_labels(labels, break_frame, Some(continue_frame));
        self.compile_statement(body, function)?;
        self.pop_labels(labels.len());
        self.loop_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if let Some(environment) = lexical_environment {
            self.emit_replace_lexical_environment(environment, function)?;
        }
        if let Some(update) = update {
            self.compile_classic_for_update(update, function)?;
        }
        function.branch_to_label(loop_frame.label);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);

        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let pending = self.capture_pending_sync_dispose_completion(function);
        self.set_completion_kind(CompletionKind::Normal, function);
        self.consume_sync_disposable_resources(
            pending,
            acquired,
            SyncDisposeCompletionContinuation::Dispatch,
            function,
        )?;

        // Normal loop exhaustion has no completion to dispatch. Route it
        // through the loop's break target so the lexical environment is
        // restored by the same control edge as an explicit break.
        self.emit_branch_to_target(break_frame, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if runtime_environment.is_some() {
            self.end_lexical_environment_scope();
        }
        self.pop_scope();
        Ok(())
    }

    pub(crate) fn compile_switch(
        &mut self,
        expression: &TypedExpr,
        lexical_environment: Option<&LexicalEnvironmentIr>,
        lexical_declarations: &[StatementIr],
        cases: &[SwitchCaseIr],
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let discriminant = schema.reserve_value_local(function);
        let selected = schema.reserve_i32_local(function);
        let active = schema.reserve_i32_local(function);
        let default = cases.iter().position(|case| case.condition.is_none());
        self.emit_statement_result(function);
        self.compile_expr_to_value(expression, &discriminant, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.push_scope();
        if let Some(environment) = lexical_environment {
            self.emit_enter_lexical_environment(environment, function)?;
        }
        for case in cases {
            self.initialize_direct_lexical_bindings(&case.body.statements, function);
        }
        for declaration in lexical_declarations {
            self.compile_statement(declaration, function)?;
        }
        function.instruction(&Instruction::I32Const(-1));
        selected.store(function);
        for (index, case) in cases.iter().enumerate() {
            let Some(condition) = &case.condition else {
                continue;
            };
            let index = i32::try_from(index).map_err(|_| {
                EmitError::unsupported("switch clause index exceeds Wasm I32 domain")
            })?;
            selected.load(function);
            function.instruction(&Instruction::I32Const(-1));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            self.compile_switch_case_match(&discriminant, condition, function)?;
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::I32Const(index));
            selected.store(function);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        if let Some(index) = default {
            let index = i32::try_from(index).map_err(|_| {
                EmitError::unsupported("switch clause index exceeds Wasm I32 domain")
            })?;
            selected.load(function);
            function.instruction(&Instruction::I32Const(-1));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::I32Const(index));
            selected.store(function);
            function.instruction(&Instruction::End);
        }
        self.emit_statement_result(function);
        function.instruction(&Instruction::I32Const(0));
        active.store(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);
        self.push_labels(labels, break_frame, None);
        for (index, case) in cases.iter().enumerate() {
            let index = i32::try_from(index).map_err(|_| {
                EmitError::unsupported("switch clause index exceeds Wasm I32 domain")
            })?;
            active.load(function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
            selected.load(function);
            function.instruction(&Instruction::I32Const(index));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::I32Const(1));
            active.store(function);
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::End);
            active.load(function);
            self.open_frame(ControlFrameKind::If, function);
            self.compile_switch_case_body(&case.body, function)?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_labels(labels.len());
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if lexical_environment.is_some() {
            self.emit_leave_lexical_environment(function);
        }
        self.pop_scope();
        schema.release_i32_local(active, function);
        schema.release_i32_local(selected, function);
        discriminant.clear(function);
        Ok(())
    }

    pub(crate) fn compile_switch_case_body(
        &mut self,
        block: &BlockIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        for statement in &block.statements {
            self.compile_statement(statement, function)?;
        }
        Ok(())
    }

    pub(crate) fn compile_switch_case_match(
        &mut self,
        discriminant: &ValueLocals,
        condition: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let selected = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(condition, &selected, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_tagged_payload_equality_i32(discriminant, &selected, function)?;
        selected.clear(function);
        Ok(())
    }

    pub(crate) fn compile_break(
        &mut self,
        label: Option<&str>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let break_frame = if let Some(label) = label {
            self.label_stack
                .iter()
                .rev()
                .find(|targets| targets.name == label)
                .map(|targets| targets.break_frame)
                .ok_or_else(|| {
                    EmitError::unsupported(format!(
                        "unsupported in scriptw wasm-aot first slice: unknown label `{label}`"
                    ))
                })?
        } else {
            *self.breakable_stack.last().ok_or_else(|| {
                EmitError::unsupported(
                    "unsupported in scriptw wasm-aot first slice: break outside loop or switch",
                )
            })?
        };
        if let Some(target) = self.active_finally_target_for_branch(break_frame) {
            self.set_branch_completion(BranchCompletionKind::Break, break_frame, function);
            self.emit_branch_to_target(target, function);
            return Ok(());
        }
        self.emit_branch_to_target(break_frame, function);
        Ok(())
    }

    pub(crate) fn compile_continue(
        &mut self,
        label: Option<&str>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let continue_frame = if let Some(label) = label {
            self.label_stack
                .iter()
                .rev()
                .find(|targets| targets.name == label)
                .and_then(|targets| targets.continue_frame)
                .ok_or_else(|| {
                    EmitError::unsupported(format!(
                        "unsupported in scriptw wasm-aot first slice: continue to non-loop label `{label}`"
                    ))
                })?
        } else {
            self.loop_stack
                .last()
                .copied()
                .ok_or_else(|| {
                    EmitError::unsupported(
                        "unsupported in scriptw wasm-aot first slice: continue outside loop",
                    )
                })?
                .continue_frame
        };
        if let Some(target) = self.active_finally_target_for_branch(continue_frame) {
            self.set_branch_completion(BranchCompletionKind::Continue, continue_frame, function);
            self.emit_branch_to_target(target, function);
            return Ok(());
        }
        self.emit_branch_to_target(continue_frame, function);
        Ok(())
    }

    pub(crate) fn compile_for_init(
        &mut self,
        init: &ForInitIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match init {
            ForInitIr::Lexical { mode, name, init } => {
                let storage = self.lookup_current_scope_binding(name).unwrap_or_else(|| {
                    self.allocate_binding(name.clone(), *mode, init.kind, function)
                });
                self.initialize_binding_uninitialized(storage, function);
                self.compile_expr_to_binding(init, storage, function)?;
            }
            ForInitIr::LexicalBlock(bindings) => {
                for binding in bindings {
                    let storage = self
                        .lookup_current_scope_binding(&binding.name)
                        .unwrap_or_else(|| {
                            self.allocate_binding(
                                binding.name.clone(),
                                binding.mode,
                                binding.init.kind,
                                function,
                            )
                        });
                    self.initialize_binding_uninitialized(storage, function);
                }
                for binding in bindings {
                    let storage = self
                        .lookup_current_scope_binding(&binding.name)
                        .expect("for lexical binding should be allocated");
                    self.compile_expr_to_binding(&binding.init, storage, function)?;
                }
            }
            ForInitIr::Var(declarators) => {
                self.compile_var_declarators(declarators, function)?;
            }
            ForInitIr::Expression(expr) => {
                let value = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(expr, &value, function)?;
                value.clear(function);
                self.emit_propagate_current_throw_if_needed(function);
            }
            ForInitIr::Statements(statements) => {
                // Compiled directly in the loop's scope - `compile_for` has
                // already pushed it - so a pattern head's bindings stay visible
                // to the test, update and body.
                for statement in statements {
                    self.compile_statement(statement, function)?;
                }
            }
            ForInitIr::SyncDisposable(_) => {
                return Err(EmitError::unsupported(
                    "synchronous using for head requires the classic loop disposal lifecycle",
                ));
            }
            ForInitIr::AsyncDisposable(_) => {
                return Err(EmitError::unsupported(
                    "await using for head requires the async classic loop disposal lifecycle",
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn compile_var_declarators(
        &mut self,
        declarators: &[VarDeclaratorIr],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        for declarator in declarators {
            let Some(init) = &declarator.init else {
                continue;
            };
            let storage = self.lookup_binding(&declarator.name);
            if self.is_script_global_binding(&declarator.name) && storage.is_none() {
                let value = self.runtime_schema().reserve_value_local(function);
                self.compile_expr_to_value(init, &value, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                self.emit_global_property_write(&declarator.name, &value, function)?;
                value.clear(function);
            } else {
                let storage = storage.ok_or_else(|| {
                    EmitError::unsupported(format!(
                        "unsupported in scriptw wasm-aot first slice: unbound identifier `{}`",
                        declarator.name
                    ))
                })?;
                self.compile_expr_to_binding(init, storage, function)?;
                self.mirror_binding_to_global_object(&declarator.name, storage, function)?;
            }
        }
        Ok(())
    }

    pub(crate) fn emit_to_integer_clamped_to_string_len(
        &mut self,
        number_payload_local: I64Local,
        string_len_local: I64Local,
        out_local: I64Local,
        function: &mut Function,
    ) {
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        out_local.store(function);
        function.instruction(&Instruction::Else);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        string_len_local.load(function);
        out_local.store(function);
        function.instruction(&Instruction::Else);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::NEG_INFINITY)));
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        out_local.store(function);
        function.instruction(&Instruction::Else);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::I64TruncSatF64S);
        out_local.store(function);
        out_local.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        out_local.store(function);
        function.instruction(&Instruction::Else);
        out_local.load(function);
        string_len_local.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        string_len_local.load(function);
        out_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_to_slice_index_clamped_to_string_len(
        &mut self,
        number_payload_local: I64Local,
        string_len_local: I64Local,
        out_local: I64Local,
        function: &mut Function,
    ) {
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        out_local.store(function);
        function.instruction(&Instruction::Else);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        string_len_local.load(function);
        out_local.store(function);
        function.instruction(&Instruction::Else);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Const(Ieee64::from(f64::NEG_INFINITY)));
        function.instruction(&Instruction::F64Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        out_local.store(function);
        function.instruction(&Instruction::Else);
        number_payload_local.load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Trunc);
        function.instruction(&Instruction::I64TruncSatF64S);
        out_local.store(function);
        out_local.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        string_len_local.load(function);
        out_local.load(function);
        function.instruction(&Instruction::I64Add);
        out_local.store(function);
        out_local.load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I64Const(0));
        out_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        out_local.load(function);
        string_len_local.load(function);
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        string_len_local.load(function);
        out_local.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
    }

    /// Leave `low <= state <= high` (unsigned) on the stack as an i32 boolean.
    ///
    /// The for-await emitter tests its plan states as spans rather than as
    /// individual equalities, because the states a suspension in the loop body
    /// resumes into are allocated *inside* the loop's span and are not known
    /// to the loop itself — only their bounds are.
    fn emit_state_in_inclusive_range_i32(
        state_local: I32Local,
        low: u32,
        high: u32,
        function: &mut Function,
    ) {
        state_local.load(function);
        function.instruction(&Instruction::I32Const(low as i32));
        function.instruction(&Instruction::I32GeU);
        state_local.load(function);
        function.instruction(&Instruction::I32Const(high as i32));
        function.instruction(&Instruction::I32LeU);
        function.instruction(&Instruction::I32And);
    }

    fn emit_for_await_state_table(
        &mut self,
        frame: &GcLocal<InvocationFrame>,
        entry: u32,
        function: &mut Function,
    ) -> Result<GcLocal<ForAwaitIteratorTable>, EmitError> {
        let schema = self.runtime_schema();
        let selected = schema
            .reserve_gc_local::<ForAwaitIteratorTable, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<InvocationFrame>()
                    .field(InvocationFrameSchema::FOR_AWAIT_ITERATORS)
                    .read(frame, schema, function)
                    .reference(),
                function,
            );
        let required = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(
            i32::try_from(
                entry
                    .checked_add(1)
                    .ok_or_else(|| EmitError::unsupported("for-await state index overflow"))?,
            )
            .map_err(|_| EmitError::unsupported("for-await state index exceeds array limit"))?,
        ));
        required.store(function);
        let old_length = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        old_length.store(function);
        selected.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let old = schema.reserve_gc_local(function).initialize(
            selected.load(schema, function).require_non_null(function),
            function,
        );
        schema
            .array_type::<ForAwaitIteratorTable>()
            .length(&old, schema, function);
        old_length.store(function);
        old.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        old_length.load(function);
        required.load(function);
        function.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, function);
        let grown = schema.reserve_gc_local(function).initialize(
            schema.array_type::<ForAwaitIteratorTable>().filled(
                GcOperand::null(schema),
                required,
                function,
            ),
            function,
        );
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let finish = self.open_frame(ControlFrameKind::Block, function);
        let copy = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        old_length.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(finish, function);
        let old = schema.reserve_gc_local(function).initialize(
            selected.load(schema, function).require_non_null(function),
            function,
        );
        let item = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ForAwaitIteratorTable>()
                .read(&old, index, schema, function)
                .reference(),
            function,
        );
        schema.array_type::<ForAwaitIteratorTable>().write(
            &grown,
            index,
            GcOperand::reference(&item, schema),
            schema,
            function,
        );
        item.clear(function);
        old.clear(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(copy, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        selected.replace(grown.load(schema, function).nullable(), function);
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::FOR_AWAIT_ITERATORS)
            .write(
                frame,
                GcOperand::nullable_reference(&grown, schema),
                schema,
                function,
            );
        schema.release_i32_local(index, function);
        grown.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let table = schema.reserve_gc_local(function).initialize(
            selected.load(schema, function).require_non_null(function),
            function,
        );
        schema.release_i32_local(old_length, function);
        schema.release_i32_local(required, function);
        selected.clear(function);
        Ok(table)
    }

    fn emit_finish_for_await_close(
        &mut self,
        outcome: &CompletionLocals,
        require_object: bool,
        exit: u32,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_restore_for_await_close(outcome, require_object, function)?;
        self.emit_set_resumable_resume_point(exit, function)?;
        self.emit_dispatch_current_completion(function)
    }

    fn emit_restore_for_await_close(
        &mut self,
        outcome: &CompletionLocals,
        require_object: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_pop_and_restore_async_pending_completion(function)?;
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        outcome.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(outcome, function);
        if require_object {
            function.instruction(&Instruction::Else);
            self.emit_is_heap_object_like_tag_i32(outcome.value().tag(), function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            let rejected = self.runtime_schema().reserve_completion(function);
            self.emit_throw_runtime_error(
                NativeErrorKind::TypeError,
                RuntimeErrorMessage::FOR_AWAIT_OF_ASYNC_ITERATOR_RETURN_RESULT_MUST_BE_OBJECT,
                &rejected,
                function,
            )?;
            self.completion().copy_from(&rejected, function);
            rejected.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_async_for_of_iterator(
        &mut self,
        iterable: &TypedExpr,
        plan: ForAwaitIteratorPlan<'_>,
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let owner = match self
            .current_function_meta()
            .map(|meta| meta.protocol().execution_kind())
        {
            Some(FunctionExecutionKind::Async) => AsyncContinuationOwner::AsyncFunction,
            Some(FunctionExecutionKind::AsyncGenerator)
                if !plan.requires_plain_async_activation() =>
            {
                AsyncContinuationOwner::AsyncGenerator
            }
            Some(
                FunctionExecutionKind::Ordinary
                | FunctionExecutionKind::Generator
                | FunctionExecutionKind::AsyncGenerator,
            )
            | None => {
                return Err(EmitError::unsupported(
                    "for-await requires its checked async activation owner",
                ));
            }
        };
        if !(plan.entry_state() < plan.value_resume_state()
            && plan.value_resume_state() < plan.close_resume_state()
            && plan.close_resume_state() < plan.exit_state())
        {
            return Err(EmitError::unsupported(
                "for-await resume states are not strictly ordered",
            ));
        }
        let body_suspends = plan.body_suspends();
        if body_suspends && plan.has_unowned_body_environment() {
            return Err(EmitError::unsupported(
                "for-await body suspension has an unowned lexical environment",
            ));
        }
        let point = self.emit_resumable_resume_point(function)?;
        Self::emit_state_in_inclusive_range_i32(
            point,
            plan.entry_state(),
            plan.close_resume_state(),
            function,
        );
        self.open_frame(ControlFrameKind::If, function);
        let iteration_environment = plan
            .head_environment()
            .and_then(|environment| environment.iteration_environment.as_ref());
        if body_suspends && iteration_environment.is_some() {
            Self::emit_state_in_inclusive_range_i32(
                point,
                plan.value_resume_state() + 1,
                plan.close_resume_state() - 1,
                function,
            );
            self.open_frame(ControlFrameKind::If, function);
            self.emit_reattach_checked_async_generator_foreign_environment(function)?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        let saved_environment = if body_suspends && iteration_environment.is_some() {
            Some(self.detach_suspended_for_await_iteration_environment(point, plan, function))
        } else {
            None
        };
        self.push_scope();
        let mode = plan.value_mode();
        let name = plan.value_name();
        let fallback =
            if mode == BindingMode::Var {
                Some(self.lookup_binding(name).ok_or_else(|| {
                    EmitError::unsupported(format!("unbound for-await var `{name}`"))
                })?)
            } else if !iteration_environment_owns_binding(plan.head_environment(), name) {
                Some(self.allocate_binding(name.to_string(), mode, ValueKind::Dynamic, function))
            } else {
                None
            };
        if body_suspends
            && iteration_environment.is_none()
            && !matches!(fallback, Some(BindingStorage::EnvSlot { .. }))
        {
            return Err(EmitError::unsupported(
                "for-await source binding does not survive body suspension",
            ));
        }
        if mode == BindingMode::Var {
            self.binding_scopes
                .last_mut()
                .expect("scope exists")
                .insert(name.to_string(), fallback.expect("var storage exists"));
        }
        let frame = schema.reserve_gc_local(function).initialize(
            self.pending_completion_frame()?.load(schema, function),
            function,
        );
        let table = self.emit_for_await_state_table(&frame, plan.entry_state(), function)?;
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(plan.entry_state() as i32));
        index.store(function);
        let source = schema.reserve_value_local(function);
        let iterator_value = schema.reserve_value_local(function);
        let next_method = schema.reserve_value_local(function);
        let awaited = schema.reserve_value_local(function);
        let resumed = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let method = schema.reserve_completion(function);
        let called = schema.reserve_completion(function);
        let pending = schema.reserve_completion(function);
        let closed = schema.reserve_completion(function);
        let is_async = schema.reserve_i32_local(function);
        let rejected = schema.reserve_i32_local(function);
        let done = schema.reserve_i32_local(function);
        let state_root = schema
            .reserve_gc_local::<ForAwaitIteratorState, Nullable>(function)
            .initialize_null(schema, function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
        let terminal = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(terminal);
        self.finally_stack.push(terminal);
        point.load(function);
        function.instruction(&Instruction::I32Const(plan.entry_state() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        if let Some(environment) = plan.head_environment() {
            self.emit_enter_for_in_of_tdz_scope(mode, environment, function)?;
        }
        self.compile_expr_to_value(iterable, &source, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        if let Some(environment) = plan.head_environment() {
            self.emit_leave_for_in_of_tdz_scope(environment, function);
        }
        let state = self.emit_acquire_for_await_iterator_state(&source, function)?;
        schema.array_type::<ForAwaitIteratorTable>().write(
            &table,
            index,
            GcOperand::nullable_reference(&state, schema),
            schema,
            function,
        );
        state_root.replace(state.load(schema, function).nullable(), function);
        state.clear(function);
        function.instruction(&Instruction::Else);
        state_root.replace(
            schema
                .array_type::<ForAwaitIteratorTable>()
                .read(&table, index, schema, function)
                .reference(),
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let state = schema.reserve_gc_local(function).initialize(
            state_root.load(schema, function).require_non_null(function),
            function,
        );
        state_root.clear(function);
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ForAwaitIteratorState>()
                .field(ForAwaitIteratorStateSchema::RECORD)
                .read(&state, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<ForAwaitIteratorState>()
            .field(ForAwaitIteratorStateSchema::ASYNC_ITERATOR)
            .read(&state, schema, function)
            .store(is_async, function);
        let stored_iterator = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::ITERATOR)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        let stored_next = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::NEXT_METHOD)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        schema.struct_type::<StoredValue>().read_into(
            &stored_iterator,
            &iterator_value,
            schema,
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored_next, &next_method, schema, function);
        stored_next.clear(function);
        stored_iterator.clear(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);
        // A close reaction restores the original five-part completion before
        // deciding whether the inner error or object-result check can replace it.
        point.load(function);
        function.instruction(&Instruction::I32Const(plan.close_resume_state() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_load_async_continuation_resume(&owner, &resumed, rejected, function)?;
        closed.set_normal(&resumed, function);
        rejected.load(function);
        self.open_frame(ControlFrameKind::If, function);
        closed.set_throw(&resumed, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_finish_for_await_close(&closed, true, plan.exit_state(), function)?;
        self.emit_branch_to_target(break_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // Only next's own reaction unpacks the IteratorResult; body resumes
        // retain the already initialized source cell and exact child environment.
        Self::emit_state_in_inclusive_range_i32(
            point,
            plan.value_resume_state(),
            plan.close_resume_state() - 1,
            function,
        );
        self.open_frame(ControlFrameKind::If, function);
        point.load(function);
        function.instruction(&Instruction::I32Const(plan.value_resume_state() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_load_async_continuation_resume(&owner, &resumed, rejected, function)?;
        rejected.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().set_throw(&resumed, function);
        self.emit_set_resumable_resume_point(plan.exit_state(), function)?;
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_for_await_result_value(&record, &resumed, done, &value, function)?;
        done.load(function);
        self.emit_branch_if_to_target(break_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let continue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.loop_stack.push(LoopTargets { continue_frame });
        self.push_labels(labels, break_frame, Some(continue_frame));
        let close_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(close_frame);
        let active_environment = if let Some(saved) = saved_environment {
            Some(self.enter_suspended_for_await_iteration_environment(
                saved,
                iteration_environment.expect("saved layout exists"),
                function,
            )?)
        } else {
            if let Some(environment) = iteration_environment {
                self.emit_enter_lexical_environment(environment, function)?;
            }
            None
        };
        let binding = self
            .lookup_current_scope_binding(name)
            .or(fallback)
            .ok_or_else(|| EmitError::unsupported("for-await source binding has no storage"))?;
        point.load(function);
        function.instruction(&Instruction::I32Const(plan.value_resume_state() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        if mode != BindingMode::Var {
            self.initialize_binding_uninitialized(binding, function);
        }
        self.write_binding_from_locals(binding, &value, function);
        self.mirror_binding_to_global_object(name, binding, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        plan.compile_body(self, function)?;
        if let Some(active) = active_environment {
            self.leave_suspended_for_await_iteration_environment(active, function)?;
        }
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_labels(labels.len());
        self.loop_stack.pop();
        pending.copy_from(self.completion(), function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(
            CompletionKind::Continue.code() as i32
        ));
        function.instruction(&Instruction::I32Eq);
        pending.target().load(function);
        function.instruction(&Instruction::I32Const(continue_frame.frame as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        pending.set_normal(pending.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if iteration_environment.is_some() && !body_suspends {
            self.emit_leave_lexical_environment(function);
            self.emit_save_resumable_environment(function)?;
        }
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        self.emit_push_async_pending_completion(function)?;
        self.emit_statement_result(function);
        schema
            .struct_type::<IteratorRecord>()
            .field(IteratorRecordSchema::DONE)
            .write(&record, GcOperand::boolean(true), schema, function);
        self.emit_prepare_for_await_close(
            &record,
            is_async,
            &iterator_value,
            &awaited,
            function,
            |builder, outcome, function| {
                builder.emit_finish_for_await_close(outcome, false, plan.exit_state(), function)?;
                builder.emit_branch_to_target(break_frame, function);
                Ok(())
            },
        )?;
        self.emit_set_resumable_resume_point(plan.close_resume_state(), function)?;
        let await_failure = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(await_failure);
        self.emit_async_continuation_await(&owner, &awaited, function)?;
        self.emit_return_async_suspension(function)?;
        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        closed.copy_from(self.completion(), function);
        self.emit_finish_for_await_close(&closed, false, plan.exit_state(), function)?;
        self.emit_branch_to_target(break_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&pending, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_cached_for_await_next(
            &record,
            is_async,
            &iterator_value,
            &next_method,
            &awaited,
            function,
        )?;
        self.emit_set_resumable_resume_point(plan.value_resume_state(), function)?;
        self.emit_async_continuation_await(&owner, &awaited, function)?;
        self.emit_return_async_suspension(function)?;
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_statement_result(function);
        self.finally_stack.pop();
        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.array_type::<ForAwaitIteratorTable>().write(
            &table,
            index,
            GcOperand::null(schema),
            schema,
            function,
        );
        self.emit_set_resumable_resume_point(plan.exit_state(), function)?;
        self.emit_dispatch_current_completion(function)?;
        self.pop_scope();
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        arguments.clear(function);
        record.clear(function);
        state.clear(function);
        table.clear(function);
        frame.clear(function);
        schema.release_i32_local(done, function);
        schema.release_i32_local(rejected, function);
        schema.release_i32_local(is_async, function);
        closed.clear(function);
        pending.clear(function);
        called.clear(function);
        method.clear(function);
        value.clear(function);
        resumed.clear(function);
        awaited.clear(function);
        next_method.clear(function);
        iterator_value.clear(function);
        source.clear(function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(point, function);
        Ok(())
    }

    pub(crate) fn compile_async_disposable_for_of_iterator(
        &mut self,
        head: &AsyncDisposableForOfHeadIr,
        iterable: &TypedExpr,
        body: &StatementIr,
        lexical_environment: Option<&ForInOfEnvironmentIr>,
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !self
            .current_function_meta()
            .is_some_and(|meta| meta.protocol().execution_kind() == FunctionExecutionKind::Async)
        {
            return Err(EmitError::unsupported(
                "await using for-of head requires a plain async function",
            ));
        }
        let schema = self.runtime_schema();
        let owner = ActivationAsyncDisposeOwner::AsyncFunctionForOf(head.capability());
        let finalizer = owner.finalizer();
        let state = self.emit_resumable_resume_point(function)?;
        self.emit_async_state_in_range(finalizer.entry_state(), finalizer.exit_state(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.push_scope();
        let name = head.binding_name();
        let fallback_binding = if !iteration_environment_owns_binding(lexical_environment, name) {
            Some(self.allocate_binding(
                name.to_string(),
                BindingMode::Const,
                ValueKind::Dynamic,
                function,
            ))
        } else {
            None
        };
        let iterator_storage = self
            .activation_owned_binding_storage(head.record().iterator().as_str())
            .ok_or_else(|| {
                EmitError::unsupported(
                    "await using for-of Iterator lacks its activation-owned cell",
                )
            })?;
        let next_storage = self
            .activation_owned_binding_storage(head.record().next_method().as_str())
            .ok_or_else(|| {
                EmitError::unsupported(
                    "await using for-of NextMethod lacks its activation-owned cell",
                )
            })?;
        let done_storage = self
            .activation_owned_binding_storage(head.record().done().as_str())
            .ok_or_else(|| {
                EmitError::unsupported("await using for-of Done lacks its activation-owned cell")
            })?;
        let source = schema.reserve_value_local(function);
        let iterator_value = schema.reserve_value_local(function);
        let next_method = schema.reserve_value_local(function);
        let done_value = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let done = schema.reserve_i32_local(function);
        let retained = schema
            .reserve_gc_local::<IteratorRecord, Nullable>(function)
            .initialize_null(schema, function);
        state.load(function);
        function.instruction(&Instruction::I32Const(finalizer.entry_state() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        if let Some(environment) = lexical_environment {
            self.emit_enter_for_in_of_tdz_scope(BindingMode::Const, environment, function)?;
        }
        self.compile_expr_to_value(iterable, &source, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        if let Some(environment) = lexical_environment {
            self.emit_leave_for_in_of_tdz_scope(environment, function);
        }
        let acquired =
            self.emit_get_sync_iterator(&source, SyncIteratorConsumer::ForOf, function)?;
        let stored_iterator = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::ITERATOR)
                .read(acquired.record(), schema, function)
                .reference(),
            function,
        );
        let stored_next = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::NEXT_METHOD)
                .read(acquired.record(), schema, function)
                .reference(),
            function,
        );
        schema.struct_type::<StoredValue>().read_into(
            &stored_iterator,
            &iterator_value,
            schema,
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored_next, &next_method, schema, function);
        self.write_binding_from_locals(iterator_storage, &iterator_value, function);
        self.write_binding_from_locals(next_storage, &next_method, function);
        done_value.set_scalar(ScalarValue::Boolean(false), function);
        self.write_binding_from_locals(done_storage, &done_value, function);
        retained.replace(
            acquired.record().load(schema, function).nullable(),
            function,
        );
        stored_next.clear(function);
        stored_iterator.clear(function);
        acquired.clear(function);
        function.instruction(&Instruction::Else);
        self.read_binding_to_locals(iterator_storage, &iterator_value, function)?;
        self.read_binding_to_locals(next_storage, &next_method, function)?;
        self.read_binding_to_locals(done_storage, &done_value, function)?;
        self.compile_truthy_tagged_i32(&done_value, function)?;
        done.store(function);
        let stored_iterator = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&iterator_value, function),
            function,
        );
        let stored_next = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&next_method, function),
            function,
        );
        let restored = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<IteratorRecord>().construct(
                (
                    GcOperand::reference(&stored_iterator, schema),
                    GcOperand::reference(&stored_next, schema),
                    GcOperand::boolean_local(done),
                ),
                function,
            ),
            function,
        );
        retained.replace(restored.load(schema, function).nullable(), function);
        restored.clear(function);
        stored_next.clear(function);
        stored_iterator.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let iterator = OwnedSyncIterator {
            record: schema.reserve_gc_local(function).initialize(
                retained.load(schema, function).require_non_null(function),
                function,
            ),
            consumer: SyncIteratorConsumer::ForOf,
        };
        retained.clear(function);
        self.emit_statement_result(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);
        let loop_frame = self.open_frame(ControlFrameKind::Loop, function);
        state.load(function);
        function.instruction(&Instruction::I32Const(finalizer.entry_state() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_sync_iterator_step_value(&iterator, done, &value, function)?;
        done_value.set_boolean(done, function);
        self.write_binding_from_locals(done_storage, &done_value, function);
        done.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_set_resumable_resume_point(finalizer.exit_state(), function)?;
        self.emit_branch_to_target(break_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let iteration_environment =
            lexical_environment.and_then(|environment| environment.iteration_environment.as_ref());
        if let Some(environment) = iteration_environment {
            self.emit_enter_resumable_lexical_environment(
                environment,
                finalizer.entry_state(),
                function,
            )?;
        }
        let binding = self
            .lookup_current_scope_binding(name)
            .or(fallback_binding)
            .ok_or_else(|| {
                EmitError::unsupported("await using for-of source binding lacks storage")
            })?;
        let storage = ActivationAsyncDisposeCapabilityStorage {
            binding: self
                .activation_owned_binding_storage(owner.binding_name())
                .ok_or_else(|| {
                    EmitError::unsupported(
                        "await using for-of DisposeCapability lacks its activation-owned cell",
                    )
                })?,
        };
        let acquired = self.reserve_async_disposable_resource_locals(function);
        let continue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.loop_stack.push(LoopTargets { continue_frame });
        let _disposal_outer = self.open_frame(ControlFrameKind::Block, function);
        let disposal_frame = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(disposal_frame);
        state.load(function);
        function.instruction(&Instruction::I32Const(finalizer.entry_state() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.initialize_binding_uninitialized(binding, function);
        let capability =
            self.initialize_empty_activation_async_dispose_capability(&storage, 1, function)?;
        acquired.value.copy_from(&value, function);
        self.acquire_async_disposable_resource_from_locals(&acquired, function)?;
        self.append_activation_async_disposable_resource(&capability, &acquired, function);
        self.write_binding_from_locals(binding, &acquired.value, function);
        self.release_active_activation_async_dispose_capability(capability, function);
        self.emit_save_resumable_environment(function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_async_state_in_range(
            finalizer.entry_state(),
            finalizer.dispose_state(),
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        self.push_labels(labels, break_frame, Some(continue_frame));
        self.compile_statement(body, function)?;
        self.pop_labels(labels.len());
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.loop_stack.pop();
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_resumable_finalizer_needs_pending_completion(
            finalizer.dispose_state(),
            function,
        )?;
        self.open_frame(ControlFrameKind::If, function);
        let pending = self.begin_async_dispose_pending_completion(function)?;
        self.emit_statement_result(function);
        let disposing =
            self.begin_activation_async_dispose_capability(storage, finalizer, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.consume_activation_async_dispose_capability(
            &owner,
            disposing,
            pending,
            finalizer,
            ActivationAsyncDisposeCompletionContinuation::ForOf(
                AsyncDisposableForOfCompletionContinuationLocals {
                    iteration_environment: if iteration_environment.is_some() {
                        AsyncDisposableForOfIterationEnvironment::Active
                    } else {
                        AsyncDisposableForOfIterationEnvironment::Absent
                    },
                    state,
                    iterator: &iterator,
                    continue_target: continue_frame,
                    loop_target: loop_frame,
                },
            ),
            function,
        )?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_set_resumable_resume_point(finalizer.exit_state(), function)?;
        self.pop_scope();
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.release_async_disposable_resource_locals(acquired, function);
        iterator.clear(function);
        schema.release_i32_local(done, function);
        value.clear(function);
        done_value.clear(function);
        next_method.clear(function);
        iterator_value.clear(function);
        source.clear(function);
        schema.release_i32_local(state, function);
        Ok(())
    }

    pub(crate) fn compile_for_of_iterator(
        &mut self,
        head: SyncForOfIteratorHead<'_>,
        iterable: &TypedExpr,
        body: &StatementIr,
        lexical_environment: Option<&ForInOfEnvironmentIr>,
        labels: &[String],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let body = match &head {
            SyncForOfIteratorHead::Assignment(_) => body,
            SyncForOfIteratorHead::SyncDisposable { body, .. } => body.statement(),
        };
        let lifecycle = match head {
            SyncForOfIteratorHead::Assignment(binding) => {
                SyncForOfIterationLifecycleLocals::Assignment(binding)
            }
            SyncForOfIteratorHead::SyncDisposable { head, .. } => {
                SyncForOfIterationLifecycleLocals::SyncDisposable {
                    head,
                    acquired: self.reserve_sync_disposable_resource_locals(function),
                }
            }
        };
        let (mode, name) = match &lifecycle {
            SyncForOfIterationLifecycleLocals::Assignment(binding) => {
                (binding.mode, binding.name.as_str())
            }
            SyncForOfIterationLifecycleLocals::SyncDisposable { head, .. } => {
                (BindingMode::Const, head.binding_name())
            }
        };
        let source = schema.reserve_value_local(function);
        if let Some(environment) = lexical_environment {
            self.emit_enter_for_in_of_tdz_scope(mode, environment, function)?;
        }
        self.compile_expr_to_value(iterable, &source, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        if let Some(environment) = lexical_environment {
            self.emit_leave_for_in_of_tdz_scope(environment, function);
        }
        self.push_scope();
        let storage_without_environment =
            if mode == BindingMode::Var {
                Some(self.lookup_binding(name).ok_or_else(|| {
                    EmitError::unsupported(format!("unbound for-of var `{name}`"))
                })?)
            } else if !iteration_environment_owns_binding(lexical_environment, name) {
                Some(self.allocate_binding(name.to_string(), mode, ValueKind::Dynamic, function))
            } else {
                None
            };
        if mode == BindingMode::Var {
            self.binding_scopes
                .last_mut()
                .expect("binding scope stack must exist")
                .insert(
                    name.to_string(),
                    storage_without_environment.expect("for-of var storage must exist"),
                );
        }
        let iterator =
            self.emit_get_sync_iterator(&source, SyncIteratorConsumer::ForOf, function)?;
        source.clear(function);
        let value = schema.reserve_value_local(function);
        let loop_value = schema.reserve_value_local(function);
        loop_value.set_undefined(function);
        let done = schema.reserve_i32_local(function);
        let pending = schema.reserve_completion(function);
        let closed = schema.reserve_completion(function);
        self.emit_statement_result(function);
        let break_frame = self.open_frame(ControlFrameKind::Block, function);
        self.breakable_stack.push(break_frame);
        let loop_frame = self.open_frame(ControlFrameKind::Loop, function);
        self.emit_sync_iterator_step_value(&iterator, done, &value, function)?;
        done.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().set_normal(&loop_value, function);
        self.emit_branch_to_target(break_frame, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        if let Some(environment) =
            lexical_environment.and_then(|environment| environment.iteration_environment.as_ref())
        {
            self.emit_enter_lexical_environment(environment, function)?;
        }
        let binding = self
            .lookup_current_scope_binding(name)
            .or(storage_without_environment)
            .expect("for-of lexical storage must be allocated before assignment");
        let continue_frame = self.open_frame(ControlFrameKind::Block, function);
        self.loop_stack.push(LoopTargets { continue_frame });
        let finalizer = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(finalizer);
        self.completion().set_normal(&loop_value, function);
        match &lifecycle {
            SyncForOfIterationLifecycleLocals::Assignment(_) => {
                self.write_binding_from_locals(binding, &value, function);
                self.mirror_binding_to_global_object(name, binding, function)?;
            }
            SyncForOfIterationLifecycleLocals::SyncDisposable { acquired, .. } => {
                self.initialize_binding_uninitialized(binding, function);
                self.reset_sync_disposable_resource_locals(acquired, function);
                acquired.value.copy_from(&value, function);
                self.open_frame(ControlFrameKind::Block, function);
                let disposal = self.open_frame(ControlFrameKind::Block, function);
                self.finally_stack.push(disposal);
                self.compile_sync_disposable_resource_from_locals(binding, acquired, function)?;
            }
        }
        self.push_labels(labels, break_frame, Some(continue_frame));
        self.compile_statement(body, function)?;
        self.pop_labels(labels.len());
        match lifecycle {
            SyncForOfIterationLifecycleLocals::Assignment(_) => {}
            SyncForOfIterationLifecycleLocals::SyncDisposable { acquired, .. } => {
                self.finally_stack.pop();
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                let completion = self.capture_pending_sync_dispose_completion(function);
                self.set_completion_kind(CompletionKind::Normal, function);
                self.consume_sync_disposable_resources(
                    completion,
                    vec![acquired],
                    SyncDisposeCompletionContinuation::DeferToIteratorClose,
                    function,
                )?;
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
            }
        }
        self.finally_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.loop_stack.pop();
        pending.copy_from(self.completion(), function);
        if lexical_environment
            .and_then(|environment| environment.iteration_environment.as_ref())
            .is_some()
        {
            self.emit_leave_lexical_environment(function);
        }
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(
            CompletionKind::Continue.code() as i32
        ));
        function.instruction(&Instruction::I32Eq);
        pending.target().load(function);
        function.instruction(&Instruction::I32Const(continue_frame.frame as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        pending.set_normal(pending.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_sync_iterator_close(&iterator, &pending, &closed, function)?;
        self.completion().copy_from(&closed, function);
        self.emit_dispatch_current_completion(function)?;
        function.instruction(&Instruction::Else);
        loop_value.copy_from(pending.value(), function);
        self.completion().copy_from(&pending, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.emit_branch_to_target(loop_frame, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.breakable_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_scope();
        closed.clear(function);
        pending.clear(function);
        schema.release_i32_local(done, function);
        loop_value.clear(function);
        value.clear(function);
        iterator.clear(function);
        Ok(())
    }

    pub(crate) fn emit_copy_data_properties_into(
        &mut self,
        source: &ValueLocals,
        excluded_keys: &[&crate::operations::PropertyKeyLocals],
        target: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        result.initialize(function);
        self.completion().initialize(function);
        let boxed = schema.reserve_completion(function);
        let get = schema.reserve_completion(function);
        let define = schema.reserve_completion(function);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let flags = schema.reserve_i32_local(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(exit);
        self.compile_nullish_tagged_i32(source.tag(), function)?;
        self.emit_branch_if_to_target(exit, function);
        self.emit_value_to_object_locals(source, &boxed, function)?;
        boxed.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&boxed, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let keys = self.emit_object_own_property_keys(boxed.value(), function)?;
        keys.length(count, schema, function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let loop_target = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, function);
        let key = keys.read_key(index, self, function)?;
        let skip = self.open_frame(ControlFrameKind::Block, function);
        self.throw_handler_stack.push(skip);
        for excluded in excluded_keys {
            self.emit_tagged_payload_same_value_i32(key.value(), excluded.value(), function)?;
            self.emit_branch_if_to_target(skip, function);
        }
        let descriptor = self.emit_direct_own_descriptor_fact(boxed.value(), &key, function)?;
        descriptor.load(schema, function).is_null(function);
        self.emit_branch_if_to_target(skip, function);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, schema, function)
            .store(flags, function);
        flags.load(function);
        function.instruction(&Instruction::I32Const(
            crate::heap::DescriptorBit::Enumerable.word() as i32,
        ));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(skip, function);
        // CopyDataProperties performs Get on the ToObject result, rather than
        // GetV's original primitive receiver.
        self.emit_object_read(boxed.value(), boxed.value(), &key, &get, function)?;
        get.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&get, function);
        self.emit_branch_to_target(skip, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_create_data_property_or_throw(target, &key, get.value(), &define, function)?;
        define.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&define, function);
        self.emit_branch_to_target(skip, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        key.clear(function);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(self.completion(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(exit, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(loop_target, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.throw_handler_stack.pop();
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(self.completion(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        keys.clear(function);
        schema.release_i32_local(flags, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        define.clear(function);
        get.clear(function);
        boxed.clear(function);
        Ok(())
    }

    /// Reserves the common GetIterator/IteratorStep/IteratorValue working set.
    /// The matching release method owns the reverse-order discipline so a new
    /// iterator consumer cannot silently corrupt the temp-local stack.
    fn emit_control_flow_string_key(
        &mut self,
        name: &str,
        function: &mut Function,
    ) -> Result<crate::operations::PropertyKeyLocals, EmitError> {
        let schema = self.runtime_schema();
        let string = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
        let key = crate::operations::PropertyKeyLocals::from_string(schema, &string, function);
        string.clear(function);
        Ok(key)
    }

    fn emit_sync_iterator_close_inline(
        &mut self,
        iterator: &OwnedSyncIterator,
        pending: &CompletionLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let done = schema.reserve_i32_local(function);
        result.copy_from(pending, function);
        schema
            .struct_type::<IteratorRecord>()
            .field(IteratorRecordSchema::DONE)
            .read(iterator.record(), schema, function)
            .store(done, function);
        done.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<IteratorRecord>()
                    .field(IteratorRecordSchema::ITERATOR)
                    .read(iterator.record(), schema, function)
                    .reference(),
                function,
            );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &receiver, schema, function);
        self.emit_iterator_close_with_completion(&receiver, pending, result, function)?;
        schema
            .struct_type::<IteratorRecord>()
            .field(IteratorRecordSchema::DONE)
            .write(
                iterator.record(),
                GcOperand::boolean(true),
                schema,
                function,
            );
        stored.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(done, function);
        receiver.clear(function);
        Ok(())
    }

    pub(crate) fn emit_get_sync_iterator(
        &mut self,
        source: &ValueLocals,
        consumer: SyncIteratorConsumer,
        function: &mut Function,
    ) -> Result<OwnedSyncIterator, EmitError> {
        let schema = self.runtime_schema();
        let boxed = schema.reserve_completion(function);
        self.emit_value_to_object_locals(source, &boxed, function)?;
        self.completion().copy_from(&boxed, function);
        self.emit_propagate_current_throw_if_needed(function);
        let symbol = schema
            .reserve_gc_local::<SymbolValue, NonNullable>(function)
            .initialize(
                self.emit_well_known_symbol_reference(
                    scriptw_ir::WellKnownSymbol::Iterator,
                    function,
                )?,
                function,
            );
        let key = crate::operations::PropertyKeyLocals::from_symbol(schema, &symbol, function);
        let method = schema.reserve_completion(function);
        self.emit_object_read(boxed.value(), source, &key, &method, function)?;
        self.completion().copy_from(&method, function);
        self.emit_propagate_current_throw_if_needed(function);
        self.emit_is_callable_i32(method.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_sync_iterator_protocol_type_error(
            &consumer,
            SyncIteratorProtocolError::NotIterable,
            &method,
            function,
        )?;
        self.completion().copy_from(&method, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
        let iterator = schema.reserve_completion(function);
        self.emit_function_or_proxy_call_with_argv(
            method.value(),
            source,
            &arguments,
            &iterator,
            function,
        )?;
        self.completion().copy_from(&iterator, function);
        self.emit_propagate_current_throw_if_needed(function);
        let record = self.emit_get_sync_iterator_direct(iterator.value(), consumer, function)?;
        iterator.clear(function);
        arguments.clear(function);
        method.clear(function);
        key.clear(function);
        symbol.clear(function);
        boxed.clear(function);
        Ok(record)
    }

    /// GetIteratorDirect after a keys()/iterator method Call: preserve the
    /// original result receiver and cache its next property exactly once.
    pub(crate) fn emit_get_sync_iterator_direct(
        &mut self,
        iterator: &ValueLocals,
        consumer: SyncIteratorConsumer,
        function: &mut Function,
    ) -> Result<OwnedSyncIterator, EmitError> {
        let schema = self.runtime_schema();
        let invalid = schema.reserve_completion(function);
        self.emit_is_heap_object_like_tag_i32(iterator.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_sync_iterator_protocol_type_error(
            &consumer,
            SyncIteratorProtocolError::MethodResultNotObject,
            &invalid,
            function,
        )?;
        self.completion().copy_from(&invalid, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let next_key = self.emit_control_flow_string_key("next", function)?;
        let next = schema.reserve_completion(function);
        self.emit_object_read(iterator, iterator, &next_key, &next, function)?;
        self.completion().copy_from(&next, function);
        self.emit_propagate_current_throw_if_needed(function);
        // GetIterator caches next without an eager callability observation.
        let iterator_value = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(iterator, function),
                function,
            );
        let next_value = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(next.value(), function),
                function,
            );
        let record = schema
            .reserve_gc_local::<IteratorRecord, NonNullable>(function)
            .initialize(
                schema.struct_type::<IteratorRecord>().construct(
                    (
                        GcOperand::reference(&iterator_value, schema),
                        GcOperand::reference(&next_value, schema),
                        GcOperand::boolean(false),
                    ),
                    function,
                ),
                function,
            );
        next_value.clear(function);
        iterator_value.clear(function);
        next.clear(function);
        next_key.clear(function);
        invalid.clear(function);
        Ok(OwnedSyncIterator { record, consumer })
    }

    fn emit_sync_iterator_protocol_type_error(
        &mut self,
        consumer: &SyncIteratorConsumer,
        error: SyncIteratorProtocolError,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let message = match (consumer, error) {
            (SyncIteratorConsumer::ArrayDestructuring, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::DESTRUCTURING_VALUE_IS_NOT_ITERABLE
            }
            (
                SyncIteratorConsumer::ArrayDestructuring,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::DESTRUCTURING_ITERATOR_METHOD_MUST_RETURN_OBJECT,
            (
                SyncIteratorConsumer::ArrayDestructuring,
                SyncIteratorProtocolError::NextNotCallable,
            ) => RuntimeErrorMessage::DESTRUCTURING_ITERATOR_NEXT_MUST_BE_CALLABLE,
            (
                SyncIteratorConsumer::ArrayDestructuring,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::DESTRUCTURING_ITERATOR_NEXT_RESULT_MUST_BE_OBJECT,
            (SyncIteratorConsumer::ArrayAccumulation, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::ARRAY_SPREAD_VALUE_IS_NOT_ITERABLE
            }
            (
                SyncIteratorConsumer::ArrayAccumulation,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::ARRAY_SPREAD_ITERATOR_METHOD_MUST_RETURN_OBJECT,
            (
                SyncIteratorConsumer::ArrayAccumulation,
                SyncIteratorProtocolError::NextNotCallable,
            ) => RuntimeErrorMessage::ARRAY_SPREAD_ITERATOR_NEXT_MUST_BE_CALLABLE,
            (
                SyncIteratorConsumer::ArrayAccumulation,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::ARRAY_SPREAD_ITERATOR_NEXT_RESULT_MUST_BE_OBJECT,
            (SyncIteratorConsumer::ArrayFrom, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::ARRAY_FROM_ITERATOR_METHOD_MUST_BE_CALLABLE
            }
            (SyncIteratorConsumer::ArrayFrom, SyncIteratorProtocolError::MethodResultNotObject) => {
                RuntimeErrorMessage::ARRAY_FROM_ITERATOR_METHOD_MUST_RETURN_OBJECT
            }
            (SyncIteratorConsumer::ArrayFrom, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::ARRAY_FROM_ITERATOR_NEXT_MUST_BE_CALLABLE
            }
            (SyncIteratorConsumer::ArrayFrom, SyncIteratorProtocolError::NextResultNotObject) => {
                RuntimeErrorMessage::ARRAY_FROM_ITERATOR_NEXT_RESULT_MUST_BE_OBJECT
            }
            (SyncIteratorConsumer::ForOf, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::FOR_OF_TARGET_IS_NOT_ITERABLE
            }
            (SyncIteratorConsumer::ForOf, SyncIteratorProtocolError::MethodResultNotObject) => {
                RuntimeErrorMessage::FOR_OF_ITERATOR_METHOD_MUST_RETURN_OBJECT
            }
            (SyncIteratorConsumer::ForOf, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::FOR_OF_ITERATOR_NEXT_MUST_BE_CALLABLE
            }
            (SyncIteratorConsumer::ForOf, SyncIteratorProtocolError::NextResultNotObject) => {
                RuntimeErrorMessage::FOR_OF_ITERATOR_NEXT_RESULT_MUST_BE_OBJECT
            }
            (SyncIteratorConsumer::MathSumPrecise, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::MATH_SUMPRECISE_INPUT_IS_NOT_ITERABLE
            }
            (
                SyncIteratorConsumer::MathSumPrecise,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::MATH_SUMPRECISE_ITERATOR_METHOD_MUST_RETURN_AN_OBJECT,
            (SyncIteratorConsumer::MathSumPrecise, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::MATH_SUMPRECISE_ITERATOR_NEXT_METHOD_IS_NOT_CALLABLE
            }
            (
                SyncIteratorConsumer::MathSumPrecise,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::MATH_SUMPRECISE_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT,
            (SyncIteratorConsumer::ListFormat, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::INTL_LISTFORMAT_INPUT_IS_NOT_ITERABLE
            }
            (
                SyncIteratorConsumer::ListFormat,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::INTL_LISTFORMAT_ITERATOR_METHOD_RESULT_MUST_BE_OBJECT,
            (SyncIteratorConsumer::ListFormat, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::INTL_LISTFORMAT_ITERATOR_NEXT_MUST_BE_CALLABLE
            }
            (SyncIteratorConsumer::ListFormat, SyncIteratorProtocolError::NextResultNotObject) => {
                RuntimeErrorMessage::INTL_LISTFORMAT_ITERATOR_NEXT_RESULT_MUST_BE_OBJECT
            }
            (SyncIteratorConsumer::AggregateError, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::AGGREGATEERROR_ERRORS_INPUT_MUST_BE_ITERABLE
            }
            (
                SyncIteratorConsumer::AggregateError,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::AGGREGATEERROR_ITERATOR_METHOD_MUST_RETURN_OBJECT,
            (SyncIteratorConsumer::AggregateError, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::AGGREGATEERROR_ITERATOR_NEXT_MUST_BE_CALLABLE
            }
            (
                SyncIteratorConsumer::AggregateError,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::AGGREGATEERROR_ITERATOR_NEXT_RESULT_MUST_BE_OBJECT,
            (SyncIteratorConsumer::MapConstructor, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::MAP_CONSTRUCTOR_ITERATOR_METHOD_IS_NOT_CALLABLE
            }
            (
                SyncIteratorConsumer::MapConstructor,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::MAP_CONSTRUCTOR_ITERATOR_METHOD_MUST_RETURN_AN_OBJECT,
            (SyncIteratorConsumer::MapConstructor, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::MAP_CONSTRUCTOR_ITERATOR_NEXT_METHOD_IS_NOT_CALLABLE
            }
            (
                SyncIteratorConsumer::MapConstructor,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::MAP_CONSTRUCTOR_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT,
            (SyncIteratorConsumer::SetConstructor, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::SET_CONSTRUCTOR_ITERATOR_METHOD_IS_NOT_CALLABLE
            }
            (
                SyncIteratorConsumer::SetConstructor,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::SET_CONSTRUCTOR_ITERATOR_METHOD_MUST_RETURN_AN_OBJECT,
            (SyncIteratorConsumer::SetConstructor, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::SET_CONSTRUCTOR_ITERATOR_NEXT_METHOD_IS_NOT_CALLABLE
            }
            (
                SyncIteratorConsumer::SetConstructor,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::SET_CONSTRUCTOR_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT,
            (SyncIteratorConsumer::WeakMapConstructor, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::WEAKMAP_CONSTRUCTOR_ITERATOR_METHOD_IS_NOT_CALLABLE
            }
            (
                SyncIteratorConsumer::WeakMapConstructor,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::WEAKMAP_CONSTRUCTOR_ITERATOR_METHOD_MUST_RETURN_AN_OBJECT,
            (
                SyncIteratorConsumer::WeakMapConstructor,
                SyncIteratorProtocolError::NextNotCallable,
            ) => RuntimeErrorMessage::WEAKMAP_CONSTRUCTOR_ITERATOR_NEXT_METHOD_IS_NOT_CALLABLE,
            (
                SyncIteratorConsumer::WeakMapConstructor,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::WEAKMAP_CONSTRUCTOR_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT,
            (SyncIteratorConsumer::WeakSetConstructor, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::WEAKSET_CONSTRUCTOR_ITERATOR_METHOD_IS_NOT_CALLABLE
            }
            (
                SyncIteratorConsumer::WeakSetConstructor,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::WEAKSET_CONSTRUCTOR_ITERATOR_METHOD_MUST_RETURN_AN_OBJECT,
            (
                SyncIteratorConsumer::WeakSetConstructor,
                SyncIteratorProtocolError::NextNotCallable,
            ) => RuntimeErrorMessage::WEAKSET_CONSTRUCTOR_ITERATOR_NEXT_METHOD_IS_NOT_CALLABLE,
            (
                SyncIteratorConsumer::WeakSetConstructor,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::WEAKSET_CONSTRUCTOR_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT,
            (SyncIteratorConsumer::ObjectFromEntries, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::OBJECT_FROMENTRIES_ITERATOR_METHOD_IS_NOT_CALLABLE
            }
            (
                SyncIteratorConsumer::ObjectFromEntries,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::OBJECT_FROMENTRIES_ITERATOR_METHOD_MUST_RETURN_AN_OBJECT,
            (
                SyncIteratorConsumer::ObjectFromEntries,
                SyncIteratorProtocolError::NextNotCallable,
            ) => RuntimeErrorMessage::OBJECT_FROMENTRIES_ITERATOR_NEXT_METHOD_IS_NOT_CALLABLE,
            (
                SyncIteratorConsumer::ObjectFromEntries,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::OBJECT_FROMENTRIES_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT,
            (SyncIteratorConsumer::MapGroupBy, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::MAP_GROUPBY_ITERATOR_METHOD_MUST_BE_CALLABLE
            }
            (
                SyncIteratorConsumer::MapGroupBy,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::MAP_GROUPBY_ITERATOR_METHOD_MUST_RETURN_AN_OBJECT,
            (SyncIteratorConsumer::MapGroupBy, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::MAP_GROUPBY_ITERATOR_NEXT_METHOD_MUST_BE_CALLABLE
            }
            (SyncIteratorConsumer::MapGroupBy, SyncIteratorProtocolError::NextResultNotObject) => {
                RuntimeErrorMessage::MAP_GROUPBY_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT
            }
            (SyncIteratorConsumer::ObjectGroupBy, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::OBJECT_GROUPBY_ITERATOR_METHOD_MUST_BE_CALLABLE
            }
            (
                SyncIteratorConsumer::ObjectGroupBy,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::OBJECT_GROUPBY_ITERATOR_METHOD_MUST_RETURN_AN_OBJECT,
            (SyncIteratorConsumer::ObjectGroupBy, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::OBJECT_GROUPBY_ITERATOR_NEXT_METHOD_MUST_BE_CALLABLE
            }
            (
                SyncIteratorConsumer::ObjectGroupBy,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::OBJECT_GROUPBY_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT,
            (SyncIteratorConsumer::IteratorHelper, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::ITERATOR_FROM_ITERATOR_METHOD_MUST_BE_CALLABLE
            }
            (
                SyncIteratorConsumer::IteratorHelper,
                SyncIteratorProtocolError::MethodResultNotObject,
            ) => RuntimeErrorMessage::ITERATOR_FROM_ITERATOR_METHOD_MUST_RETURN_OBJECT,
            (SyncIteratorConsumer::IteratorHelper, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::ITERATOR_PROTOTYPE_MAP_NEXT_METHOD_MUST_BE_CALLABLE
            }
            (
                SyncIteratorConsumer::IteratorHelper,
                SyncIteratorProtocolError::NextResultNotObject,
            ) => RuntimeErrorMessage::ITERATOR_MAP_HELPER_NEXT_RESULT_MUST_BE_OBJECT,
            (SyncIteratorConsumer::SetLike, SyncIteratorProtocolError::NotIterable) => {
                RuntimeErrorMessage::SET_LIKE_KEYS_METHOD_IS_NOT_CALLABLE
            }
            (SyncIteratorConsumer::SetLike, SyncIteratorProtocolError::MethodResultNotObject) => {
                RuntimeErrorMessage::SET_LIKE_KEYS_METHOD_MUST_RETURN_AN_OBJECT
            }
            (SyncIteratorConsumer::SetLike, SyncIteratorProtocolError::NextNotCallable) => {
                RuntimeErrorMessage::SET_LIKE_ITERATOR_NEXT_METHOD_IS_NOT_CALLABLE
            }
            (SyncIteratorConsumer::SetLike, SyncIteratorProtocolError::NextResultNotObject) => {
                RuntimeErrorMessage::SET_LIKE_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT
            }
        };
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            message,
            result,
            function,
        )
    }

    /// Runs one sync IteratorStep/IteratorValue pair without introducing an
    /// IteratorClose path. This is the exact control shape required by
    /// ArrayAccumulation: any abrupt completion propagates directly, `done` is
    /// set for a completed iterator, and `value` is read only on the false arm.
    pub(crate) fn emit_sync_iterator_step_value(
        &mut self,
        iterator: &OwnedSyncIterator,
        done: I32Local,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        value.set_undefined(function);
        self.emit_sync_iterator_step(iterator, done, Some(value), function)
    }

    pub(crate) fn emit_sync_iterator_step_without_value(
        &mut self,
        iterator: &OwnedSyncIterator,
        done: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_sync_iterator_step(iterator, done, None, function)
    }

    /// Native helpers must finalize their own state before publishing a Throw.
    pub(crate) fn emit_sync_iterator_step_value_into(
        &mut self,
        iterator: &OwnedSyncIterator,
        done: I32Local,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        value.set_undefined(function);
        self.emit_sync_iterator_step_into(iterator, done, Some(value), result, function)
    }

    pub(crate) fn emit_sync_iterator_step_without_value_into(
        &mut self,
        iterator: &OwnedSyncIterator,
        done: I32Local,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_sync_iterator_step_into(iterator, done, None, result, function)
    }

    fn emit_sync_iterator_step(
        &mut self,
        iterator: &OwnedSyncIterator,
        done: I32Local,
        value: Option<&ValueLocals>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(function);
        result.initialize(function);
        self.emit_sync_iterator_step_into(iterator, done, value, &result, function)?;
        self.completion().copy_from(&result, function);
        result.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    fn emit_sync_iterator_step_inline_into(
        &mut self,
        iterator: &OwnedSyncIterator,
        done: I32Local,
        value: Option<&ValueLocals>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let row = schema.struct_type::<IteratorRecord>();
        let receiver = schema.reserve_value_local(function);
        let next = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        let next_result = schema.reserve_value_local(function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[], function);
        let done_key = self.emit_control_flow_string_key("done", function)?;
        let value_key = self.emit_control_flow_string_key("value", function)?;
        let finish = self.open_frame(ControlFrameKind::Block, function);
        row.field(IteratorRecordSchema::DONE)
            .read(iterator.record(), schema, function)
            .store(done, function);
        done.load(function);
        self.emit_branch_if_to_target(finish, function);
        let stored_receiver = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                row.field(IteratorRecordSchema::ITERATOR)
                    .read(iterator.record(), schema, function)
                    .reference(),
                function,
            );
        let stored_next = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                row.field(IteratorRecordSchema::NEXT_METHOD)
                    .read(iterator.record(), schema, function)
                    .reference(),
                function,
            );
        schema.struct_type::<StoredValue>().read_into(
            &stored_receiver,
            &receiver,
            schema,
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored_next, &next, schema, function);
        self.emit_is_callable_i32(&next, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_sync_iterator_protocol_type_error(
            &iterator.consumer,
            SyncIteratorProtocolError::NextNotCallable,
            &pending,
            function,
        )?;
        self.emit_branch_to_target(finish, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_function_or_proxy_call_with_argv(
            &next, &receiver, &arguments, &pending, function,
        )?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(finish, function);
        self.emit_is_heap_object_like_tag_i32(pending.value().tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_sync_iterator_protocol_type_error(
            &iterator.consumer,
            SyncIteratorProtocolError::NextResultNotObject,
            &pending,
            function,
        )?;
        self.emit_branch_to_target(finish, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        next_result.copy_from(pending.value(), function);
        self.emit_object_read(&next_result, &next_result, &done_key, &pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(finish, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        done.store(function);
        done.load(function);
        self.emit_branch_if_to_target(finish, function);
        if let Some(output) = value {
            self.emit_object_read(&next_result, &next_result, &value_key, &pending, function)?;
            pending.kind().load(function);
            function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
            function.instruction(&Instruction::I32Eq);
            self.emit_branch_if_to_target(finish, function);
            output.copy_from(pending.value(), function);
        }
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        // IteratorStepValue marks the record done on every step/value abrupt;
        // its caller therefore never closes on a failed protocol observation.
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(1));
        done.store(function);
        function.instruction(&Instruction::End);
        row.field(IteratorRecordSchema::DONE).write(
            iterator.record(),
            GcOperand::boolean_local(done),
            schema,
            function,
        );
        result.copy_from(&pending, function);
        stored_next.clear(function);
        stored_receiver.clear(function);
        value_key.clear(function);
        done_key.clear(function);
        arguments.clear(function);
        next_result.clear(function);
        pending.clear(function);
        next.clear(function);
        receiver.clear(function);
        Ok(())
    }

    fn prepare_destructuring_target<'b>(
        &mut self,
        target: &'b DestructuringTargetIr,
        function: &mut Function,
    ) -> Result<PreparedDestructuringTarget<'b>, EmitError> {
        let schema = self.runtime_schema();
        match target {
            DestructuringTargetIr::Binding { mode, name } => {
                Ok(PreparedDestructuringTarget::Binding { mode: *mode, name })
            }
            DestructuringTargetIr::ResolvedVarBinding { reference, .. }
            | DestructuringTargetIr::AssignmentIdentifier(reference) => {
                self.prepare_destructuring_identifier_reference(reference, function)
            }
            DestructuringTargetIr::AssignmentProperty {
                target,
                key,
                strictness,
            } => {
                let receiver = schema.reserve_value_local(function);
                self.compile_expr_to_value(target, &receiver, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                let key = match key {
                    DestructuringPropertyKeyIr::Static(name) => {
                        PreparedDestructuringPropertyKey::Static(name)
                    }
                    DestructuringPropertyKeyIr::Computed(expression) => {
                        let value = schema.reserve_value_local(function);
                        self.compile_expr_to_value(expression, &value, function)?;
                        self.emit_propagate_current_throw_if_needed(function);
                        PreparedDestructuringPropertyKey::Computed(value)
                    }
                };
                Ok(PreparedDestructuringTarget::Property {
                    target: receiver,
                    key,
                    strictness: *strictness,
                })
            }
            DestructuringTargetIr::AssignmentPrivate {
                target,
                private_name_id,
            } => {
                let receiver = schema.reserve_value_local(function);
                self.compile_expr_to_value(target, &receiver, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                Ok(PreparedDestructuringTarget::Private {
                    target: receiver,
                    private_name_id: *private_name_id,
                })
            }
            DestructuringTargetIr::AssignmentSuper {
                capture,
                value_binding,
                put,
            } => {
                let captured = schema.reserve_value_local(function);
                self.compile_expr_to_value(capture, &captured, function)?;
                captured.clear(function);
                self.emit_propagate_current_throw_if_needed(function);
                Ok(PreparedDestructuringTarget::Super { value_binding, put })
            }
            DestructuringTargetIr::NestedArray(pattern) => {
                Ok(PreparedDestructuringTarget::NestedArray(pattern))
            }
            DestructuringTargetIr::NestedObject(pattern) => {
                Ok(PreparedDestructuringTarget::NestedObject(pattern))
            }
        }
    }

    fn prepare_destructuring_identifier_reference<'b>(
        &mut self,
        reference: &'b IdentifierWriteReferenceIr,
        function: &mut Function,
    ) -> Result<PreparedDestructuringTarget<'b>, EmitError> {
        let schema = self.runtime_schema();
        let write = match reference.write_disposition() {
            IdentifierWriteDisposition::WithObject {
                referenced_name,
                selection,
                fallback_storage_name,
                strictness,
            } => {
                let key = schema
                    .reserve_gc_local::<StringValue, NonNullable>(function)
                    .initialize(
                        self.emit_interned_string_reference(referenced_name, function)?,
                        function,
                    );
                let selected = schema.reserve_value_local(function);
                self.compile_expr_to_value(selection, &selected, function)?;
                self.emit_propagate_current_throw_if_needed(function);
                let reference = self.emit_selected_with_object_identifier_reference(
                    &key, &selected, strictness, function,
                );
                key.clear(function);
                return Ok(PreparedDestructuringTarget::WithObjectIdentifier {
                    selected,
                    reference,
                    fallback_storage_name,
                });
            }
            IdentifierWriteDisposition::Environment {
                referenced_name,
                strictness,
            } => {
                let key = schema
                    .reserve_gc_local::<StringValue, NonNullable>(function)
                    .initialize(
                        self.emit_interned_string_reference(referenced_name, function)?,
                        function,
                    );
                let reference =
                    self.emit_resolve_environment_identifier(&key, strictness, function)?;
                key.clear(function);
                return Ok(PreparedDestructuringTarget::EnvironmentIdentifier(
                    reference,
                ));
            }
            IdentifierWriteDisposition::MutableBinding { storage_name } => {
                PreparedIdentifierWrite::MutableBinding { storage_name }
            }
            IdentifierWriteDisposition::IgnoreImmutableBinding => {
                PreparedIdentifierWrite::IgnoreImmutableBinding
            }
            IdentifierWriteDisposition::Throw { error } => PreparedIdentifierWrite::Throw { error },
            IdentifierWriteDisposition::Global {
                referenced_name,
                strictness,
            } => PreparedIdentifierWrite::Global {
                referenced_name,
                strictness,
            },
        };
        Ok(PreparedDestructuringTarget::AssignmentIdentifier(write))
    }

    fn put_destructuring_target(
        &mut self,
        prepared: PreparedDestructuringTarget<'_>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        match prepared {
            PreparedDestructuringTarget::Binding { mode, name } => {
                let storage = self
                    .lookup_current_scope_binding(name)
                    .or_else(|| self.lookup_binding(name))
                    .unwrap_or_else(|| {
                        self.allocate_binding(name.to_string(), mode, ValueKind::Dynamic, function)
                    });
                self.write_binding_from_locals(storage, value, function);
                self.mirror_binding_to_global_object(name, storage, function)?;
            }
            PreparedDestructuringTarget::EnvironmentIdentifier(reference) => {
                self.emit_environment_identifier_put(&reference, value, function)?;
                self.release_environment_identifier_reference(reference, function);
            }
            PreparedDestructuringTarget::WithObjectIdentifier {
                selected,
                reference,
                fallback_storage_name,
            } => {
                selected.tag().load(function);
                function.instruction(&Instruction::I32Const(
                    WasmRuntimeValueTag::Undefined as i32,
                ));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                let storage = self.lookup_binding(fallback_storage_name).ok_or_else(|| {
                    EmitError::unsupported(format!(
                        "unbound var destructuring fallback `{fallback_storage_name}`"
                    ))
                })?;
                self.write_binding_from_locals_checked(storage, value, function)?;
                self.mirror_binding_to_global_object(fallback_storage_name, storage, function)?;
                function.instruction(&Instruction::Else);
                self.emit_environment_identifier_put(&reference, value, function)?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.release_environment_identifier_reference(reference, function);
                selected.clear(function);
            }
            PreparedDestructuringTarget::AssignmentIdentifier(write) => match write {
                PreparedIdentifierWrite::MutableBinding { storage_name } => {
                    let storage = self.lookup_binding(storage_name).ok_or_else(|| {
                        EmitError::unsupported(format!(
                            "unbound destructuring assignment `{storage_name}`"
                        ))
                    })?;
                    self.write_binding_from_locals_checked(storage, value, function)?;
                    self.mirror_binding_to_global_object(storage_name, storage, function)?;
                }
                PreparedIdentifierWrite::IgnoreImmutableBinding => {}
                PreparedIdentifierWrite::Throw { error } => {
                    // Target preparation and the complete value/default precede PutValue.
                    let pending = schema.reserve_completion(function);
                    self.emit_throw_runtime_error(
                        error.kind(),
                        self.strings.source_runtime_error_message(
                            SourceRuntimeErrorMessage::IdentifierWrite(error),
                        )?,
                        &pending,
                        function,
                    )?;
                    self.completion().copy_from(&pending, function);
                    pending.clear(function);
                    self.emit_propagate_current_throw(function);
                }
                PreparedIdentifierWrite::Global {
                    referenced_name,
                    strictness,
                } => {
                    self.with_reference_strictness(strictness, function, |emitter, function| {
                        emitter.emit_global_property_write_checked(
                            referenced_name,
                            value,
                            strictness,
                            function,
                        )
                    })?;
                }
            },
            PreparedDestructuringTarget::Property {
                target,
                key,
                strictness,
            } => {
                // Nullish rejection belongs to PutValue and precedes ToPropertyKey.
                self.compile_nullish_tagged_i32(target.tag(), function)?;
                self.open_frame(ControlFrameKind::If, function);
                let pending = schema.reserve_completion(function);
                self.emit_throw_runtime_error(
                    NativeErrorKind::TypeError,
                    RuntimeErrorMessage::CANNOT_CONVERT_UNDEFINED_OR_NULL_TO_OBJECT,
                    &pending,
                    function,
                )?;
                self.completion().copy_from(&pending, function);
                pending.clear(function);
                self.emit_propagate_current_throw(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                let key = match key {
                    PreparedDestructuringPropertyKey::Static(name) => {
                        self.emit_control_flow_string_key(name, function)?
                    }
                    PreparedDestructuringPropertyKey::Computed(raw) => {
                        let key = self.emit_value_to_property_key_locals(&raw, function)?;
                        raw.clear(function);
                        key
                    }
                };
                let strict = schema.reserve_i32_local(function);
                function.instruction(&Instruction::I32Const(i32::from(
                    strictness.throws_on_failed_set(),
                )));
                strict.store(function);
                let pending = schema.reserve_completion(function);
                self.emit_object_write(&target, &key, value, strict, &pending, function)?;
                self.completion().copy_from(&pending, function);
                pending.clear(function);
                schema.release_i32_local(strict, function);
                key.clear(function);
                target.clear(function);
            }
            PreparedDestructuringTarget::Private {
                target,
                private_name_id,
            } => {
                let pending = schema.reserve_completion(function);
                self.emit_private_write_from_locals(
                    &target,
                    private_name_id,
                    value,
                    &pending,
                    function,
                )?;
                self.completion().copy_from(&pending, function);
                pending.clear(function);
                target.clear(function);
            }
            PreparedDestructuringTarget::Super { value_binding, put } => {
                let storage = self.lookup_binding(value_binding).ok_or_else(|| {
                    EmitError::unsupported(format!(
                        "unbound super destructuring value `{value_binding}`"
                    ))
                })?;
                self.write_binding_from_locals(storage, value, function);
                let written = schema.reserve_value_local(function);
                self.compile_expr_to_value(put, &written, function)?;
                written.clear(function);
            }
            PreparedDestructuringTarget::NestedArray(pattern) => {
                self.compile_array_destructure_from_value_locals(value, pattern, function)?
            }
            PreparedDestructuringTarget::NestedObject(pattern) => {
                let ignored = schema.reserve_value_local(function);
                self.compile_object_destructure_from_value_locals(
                    value, pattern, &ignored, function,
                )?;
                ignored.clear(function);
            }
        }
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }
}
