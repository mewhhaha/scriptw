use scriptw_ir::{
    AsyncDisposableScopeExecutionIr, AwaitSequenceError, DerivedConstructorActivationIr,
    FunctionExecutionKind, HostBuiltinId, ProgramIr, ScriptIr, StandardBuiltinId,
    SyncDisposableScopeExecutionIr, ValueKind, direct_await_sequence_resume_state,
};
// `CodeSection` is deliberately absent from this list. Every code-section entry
// now goes through `ModuleCode::push(EmittedFunction)`, which cannot be called
// without an identity and records the body's measured size; keeping the raw
// section unnameable here is what makes "emit a body without attributing it"
// a compile error rather than a review finding. See `emitted_function.rs`.
// `Function` is deliberately absent from this list too: it names
// `code_sink::Function`, reached through the glob below. Importing the encoder
// type here would shadow the sink in this file alone, which is precisely the
// hole the sink exists to close.
use crate::runtime_artifact::{EmittedModule, ModuleKind};
use wasm_encoder::{Instruction, ValType};

use super::*;
use crate::gc_types::*;

mod async_generator_admission;
mod body_compilation;
mod body_entry;
mod completion_exit;
mod module_assembly;
mod runtime_requirement;
pub(crate) use body_compilation::{CompletedProgramInitializeBody, ProgramInitializeCompilation};
pub(crate) use completion_exit::CompletionExit;
pub(crate) use module_assembly::{
    emit_runtime_direct_eval_module, emit_runtime_function_module,
    emit_runtime_global_script_module,
};
#[cfg(test)]
mod async_generator_dispatcher_tests;

#[derive(Debug, Clone, Copy)]
pub(crate) enum ControlFrameKind {
    If,
    Block,
    Loop,
}

/// What a call site wants done with a throw the callee left behind.
///
/// This was spelled `Option<u32>`: `Some(n)` meant "route it to the active
/// handler, and by the way the caller has `n` raw Wasm frames open that the
/// branch arithmetic cannot see", `None` meant "leave it in the completion
/// tuple". The depth half is gone — branch immediates come from the real label
/// depth now — but the choice half is a real, closed decision, so it is an enum
/// rather than a `bool`: `Some(0)` and `Some(1)` read like tuning and were the
/// same decision spelled two ways.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PropagateCallThrow {
    /// Emit the propagation, so the throw reaches the enclosing `try`/`finally`
    /// (or returns the completion when there is none).
    ToActiveHandler,
    /// Leave `completion_local == THROW` for the caller to inspect.
    LeaveInCompletion,
}

/// Whether the completed ordinary property read routes a getter's whole Throw
/// to the active handler or leaves it for the caller to inspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AccessorThrowRouting {
    /// Publish the whole operation completion and route its Throw.
    BreakToOrdinaryReadExit,
    /// Leave `completion_local == THROW` in place.
    LeaveInCompletion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ControlTarget {
    /// Index into `FunctionBuilder::control_stack`. This is an *identity* —
    /// it orders two targets (`innermost_target`, `finalizer_crosses_branch`)
    /// and names one in the completion dispatcher's `target_id`. It is no
    /// longer used to compute a branch immediate; `label` is.
    pub(crate) frame: usize,
    pub(crate) environment_depth: u32,
    /// The real Wasm label this frame opened, recorded by
    /// `ControlFlow::open_frame` from the sink immediately after the frame
    /// instruction was written. Subtracting it from the sink's current depth
    /// is the whole branch arithmetic; see `code_sink.rs`.
    pub(crate) label: LabelDepth,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LoopTargets {
    pub(crate) continue_frame: ControlTarget,
}

#[derive(Debug, Clone)]
pub(crate) struct LabelTargets {
    pub(crate) name: String,
    pub(crate) break_frame: ControlTarget,
    pub(crate) continue_frame: Option<ControlTarget>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LocalBindingId {
    ordinal: usize,
}

pub(crate) struct LocalBindingLocals {
    value: ValueLocals,
    initialization: LocalBindingInitialization,
    role: LocalBindingRole,
}
/// Only the constructors that publish an initialized value can omit a TDZ
/// flag. Declaration spelling alone cannot prove a captured snapshot ready.
#[derive(Clone, Copy)]
enum LocalBindingInitialization {
    Checked(I32Local),
    HoistedVar,
    CompilerTemporary,
}
enum LocalBindingRole {
    Source {
        private_argument_list: Option<GcLocal<ValueArray, Nullable>>,
    },
    CompilerTemporary,
}
impl LocalBindingLocals {
    pub(crate) fn value(&self) -> &ValueLocals {
        &self.value
    }
    pub(crate) fn initialization_check(&self) -> Option<I32Local> {
        match self.initialization {
            LocalBindingInitialization::Checked(initialized) => Some(initialized),
            LocalBindingInitialization::HoistedVar
            | LocalBindingInitialization::CompilerTemporary => None,
        }
    }
    pub(crate) fn mark_initialized(&self, function: &mut Function) {
        if let Some(initialized) = self.initialization_check() {
            function.instruction(&Instruction::I32Const(1));
            initialized.store(function);
        }
    }
    fn private_argument_list(&self) -> Option<&GcLocal<ValueArray, Nullable>> {
        match &self.role {
            LocalBindingRole::Source {
                private_argument_list,
            } => private_argument_list.as_ref(),
            LocalBindingRole::CompilerTemporary => None,
        }
    }
}

/// Scope maps may copy a private identity; the builder retains the only rooted
/// value/initialization owner. Environment storage names the real slot/hops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingStorage {
    Local(LocalBindingId),
    EnvSlot { slot: u32, hops: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReturnAbi {
    MainExport,
    MultiValue,
    ProgramInstance,
}

/// Construction role of one emitted function.
///
/// The main role necessarily borrows the exact sealed global package that the
/// module later encodes. `Self::new` derives the return ABI from this value, so
/// a caller cannot construct a main body without its rooted section or extract
/// a copyable schema to pair with another section.
enum FunctionModuleState<'a> {
    ProgramInitialize,
    Main(&'a FinalizedModuleGlobals, PromiseRejectionPolicy),
    PreparedScript(
        &'a PreparedScriptUnit,
        &'a crate::function_entry::PlannedFunctionEntry,
    ),
    Internal(&'a crate::function_entry::PlannedFunctionEntry),
    RuntimeOperation(RuntimeHelperId),
}

impl<'a> FunctionModuleState<'a> {
    fn parameter_count(&self) -> usize {
        match self {
            Self::Main(_, _) => StaticSignature::Main.parameter_count(),
            Self::ProgramInitialize => StaticSignature::ProgramInitialize.parameter_count(),
            Self::Internal(entry) | Self::PreparedScript(_, entry) => {
                entry.signature().parameter_count()
            }
            Self::RuntimeOperation(helper) => helper.parameter_types().len(),
        }
    }

    const fn return_abi(&self) -> ReturnAbi {
        match self {
            Self::Main(_, _) => ReturnAbi::MainExport,
            Self::ProgramInitialize => ReturnAbi::ProgramInstance,
            Self::Internal(_) | Self::PreparedScript(_, _) | Self::RuntimeOperation(_) => {
                ReturnAbi::MultiValue
            }
        }
    }
    fn planned_body_entry(&self) -> Option<crate::function_entry::PlannedBodyEntry<'a>> {
        match self {
            Self::Internal(entry) | Self::PreparedScript(_, entry) => Some(entry.body()),
            Self::Main(_, _) | Self::RuntimeOperation(_) | Self::ProgramInitialize => None,
        }
    }
}

/// Only a builder owning the planned entry can finish this body. ModuleCode
/// consumes the entry's declaration and verifies its index at publication.
pub(crate) struct CompletedCallableBody {
    entry: crate::function_entry::PlannedFunctionEntry,
    body: Function,
}
impl CompletedCallableBody {
    pub(crate) fn into_parts(self) -> (crate::function_entry::PlannedFunctionEntry, Function) {
        (self.entry, self.body)
    }
}

/// Closed inputs for compiling the one exported main body.
///
/// Fields and construction stay in this module. The finalized module package
/// consumes the plan and supplies its own private globals to `compile`, so
/// module assembly cannot pass an arbitrary callback that ignores package A
/// while compiling against package B.
pub(crate) struct MainFunctionCompilation<'a> {
    script: &'a ScriptIr,
    promise_rejection_policy: PromiseRejectionPolicy,
    strings: &'a StringPool,
    functions: &'a FunctionMetaRegistry,
    uses_heap: bool,
    runtime_bootstrap_plan: RuntimeBootstrapPlan,
    runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
}

impl<'a> MainFunctionCompilation<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        script: &'a ScriptIr,
        promise_rejection_policy: PromiseRejectionPolicy,
        strings: &'a StringPool,
        functions: &'a FunctionMetaRegistry,
        uses_heap: bool,
        runtime_bootstrap_plan: RuntimeBootstrapPlan,
        runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
    ) -> Self {
        Self {
            script,
            promise_rejection_policy,
            strings,
            functions,
            uses_heap,
            runtime_bootstrap_plan,
            runtime_helper_base,
        }
    }

    pub(crate) fn compile(
        self,
        module_globals: &FinalizedModuleGlobals,
        program_initialize: Option<crate::function_layout::ProgramInitializeIndex>,
    ) -> Result<(EmittedFunction, u32), EmitError> {
        let mut builder = FunctionBuilder::new_main(
            self.script,
            self.strings,
            self.functions,
            self.uses_heap,
            self.runtime_bootstrap_plan,
            self.runtime_helper_base,
            module_globals,
            self.promise_rejection_policy,
        );
        let main = builder.compile_main_with_program_initialize(program_initialize)?;
        let emitted_local_count = builder.emitted_local_count();
        Ok((EmittedFunction::main(main), emitted_local_count))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionKind {
    Normal,
    Throw,
    Return,
    Break,
    Continue,
}

impl CompletionKind {
    pub(crate) const fn code(self) -> i64 {
        match self {
            Self::Normal => COMPLETION_KIND_NORMAL,
            Self::Throw => COMPLETION_KIND_THROW,
            Self::Return => COMPLETION_KIND_RETURN,
            Self::Break => COMPLETION_KIND_BREAK,
            Self::Continue => COMPLETION_KIND_CONTINUE,
        }
    }
}

mod runtime_operations;

mod template_objects;
pub(crate) use template_objects::TemplateSourceExecution;

/// Dense cache-table index minted by the shared registry of one emitted P.
pub(crate) enum ProgramSiteKind {
    CompactShape,
    OwnDataRead,
}

#[derive(Clone, Copy)]
pub(crate) struct ProgramSiteId(u32);
impl ProgramSiteId {
    pub(crate) fn emit(self, output: crate::gc_types::I32Local, function: &mut Function) {
        function.instruction(&Instruction::I32Const(self.0 as i32));
        output.store(function);
    }
}

pub(crate) struct FunctionBuilder<'a> {
    schema: &'a RuntimeSchema,
    owned_body: Option<Function>,
    current_environment: GcLocal<Environment, Nullable>,
    current_private_environment: GcLocal<PrivateEnvironment, Nullable>,
    direct_eval_context: Option<GcLocal<DirectEvalExecutionContext, Nullable>>,
    completion: CompletionLocals,
    local_bindings: Vec<Option<LocalBindingLocals>>,
    body_entry_locals: Option<crate::function_entry::BodyEntryLocals>,
    // Main binds the P published by its initializer; registered helpers bind
    // their required P operand. Callable bodies own theirs in BodyEntryLocals.
    entry_program: Option<crate::gc_types::ProgramInstanceLocals>,
    pub(crate) source_activation: Option<crate::functions::SourceActivation>,
    // Only a declared typed helper parameter can retain an actual callable's
    // immutable context. It never manufactures a callable body entry.
    helper_function_context: Option<GcLocal<FunctionContext>>,
    // Complete source operations can transport their caller's already selected
    // Realm without claiming a callable context or a source Environment chain.
    helper_execution_realm: Option<GcLocal<RealmRecord>>,
    pub(crate) bootstrap_realm: Option<crate::builtins::BootstrapRealm>,
    pub(crate) body: &'a BlockIr,
    template_source_body: Option<scriptw_ir::TemplateSourceIr>,
    pub(crate) template_source_execution: Option<TemplateSourceExecution>,
    module_prelude_id: Option<scriptw_ir::StaticScriptId>,
    script_entry_after_prelude_id: Option<scriptw_ir::StaticScriptId>,
    global_prelude_phase: Option<crate::gc_types::GlobalScriptPreludePhase>,
    pub(crate) params: &'a [FunctionParamIr],
    pub(crate) owned_env_bindings: &'a [OwnedEnvBindingIr],
    eval_environment: Option<&'a scriptw_ir::EvalEnvironmentRoleIr>,
    pub(crate) captured_bindings: &'a [scriptw_ir::CapturedBindingIr],
    pub(crate) strings: &'a StringPool,
    pub(crate) functions: &'a FunctionMetaRegistry,
    pub(crate) function_id: Option<FunctionId>,
    pub(crate) function_flavor: FunctionFlavor,
    pub(crate) function_arguments_protocol: FunctionArgumentsProtocol,
    pub(crate) lexical_derived_activation: Option<&'a DerivedConstructorActivationIr>,
    pub(crate) is_derived_constructor: bool,
    pub(crate) strict: bool,
    pub(crate) self_binding_name: Option<String>,
    pub(crate) script_global_bindings: Option<&'a GlobalBindingPlan>,
    pub(crate) uses_heap: bool,
    pub(crate) completion_exit: CompletionExit,
    module_state: FunctionModuleState<'a>,
    pub(crate) binding_scopes: Vec<BTreeMap<String, BindingStorage>>,
    pub(crate) hoisted_vars: Vec<String>,
    emitted_local_count: u32,
    pub(crate) environment_depth: u32,
    pub(crate) control_stack: Vec<ControlFrameKind>,
    pub(crate) breakable_stack: Vec<ControlTarget>,
    pub(crate) loop_stack: Vec<LoopTargets>,
    pub(crate) label_stack: Vec<LabelTargets>,
    pub(crate) throw_handler_stack: Vec<ControlTarget>,
    pub(crate) finally_stack: Vec<ControlTarget>,
    pub(crate) generator_finalizer_depth: u32,
    pub(crate) statement_list_value_context:
        Option<crate::control_flow::GeneratorStatementListValueContext>,
    pub(crate) checked_async_generator_environment_owner:
        Option<crate::control_flow::CheckedAsyncGeneratorEnvironmentOwner>,
    pub(crate) checked_async_generator_resource_owner:
        Option<crate::control_flow::CheckedAsyncGeneratorResourceOwner>,
    pub(crate) async_generator_resume_environment_plan:
        Option<scriptw_ir::AsyncGeneratorResumeEnvironmentPlanIr>,
    pub(crate) runtime_bootstrap_plan: RuntimeBootstrapPlan,
    pub(crate) runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
    /// When false, `emit_string_payload_equality_i32` inlines its byte-compare
    /// loop instead of calling the shared string-equality helper. Set false only
    /// while compiling that helper itself. Builtin bodies compare interned
    /// string payloads at thousands of sites (property-name matching, key
    /// switches), and the inline loop is ~65 instructions per site, so outlining
    /// it keeps the largest builtin bodies under Cranelift's per-function
    /// virtual-register limit.
    pub(crate) outline_string_equality: bool,
    /// When false, `emit_number_to_string_payload` inlines its digit-emission
    /// state machine instead of calling the shared helper. Set false only while
    /// compiling that helper itself. Number formatting appears in nearly every
    /// builtin (ToString of numeric results, join/serialize paths), and the
    /// inline expansion is several KB per site.
    pub(crate) outline_number_to_string: bool,
    /// When false, `emit_string_to_number_payload` inlines its parse state
    /// machine instead of calling the shared helper. Set false only while
    /// compiling that helper itself. String-to-number parsing (ToNumber of
    /// string operands) is similarly several KB per inline site.
    pub(crate) outline_string_to_number: bool,
    /// When false, `emit_value_to_string_payload` inlines the full dynamic
    /// ToString composite (per-kind dispatch, ToPrimitive on objects, array
    /// join, function source text) instead of calling the shared helper. Set
    /// false only while compiling that helper itself. Every dynamic string
    /// concatenation and ToString site otherwise pays tens of KB inline.
    pub(crate) outline_value_to_string: bool,
    /// When false, `emit_value_to_number_payload` inlines the full ToNumber
    /// composite (per-kind dispatch, ToPrimitive on objects, array→string,
    /// BigInt/Symbol throw sites) instead of calling the shared helper. Set
    /// false only while compiling that helper itself. ToNumber appears at ~130
    /// builtin sites, each otherwise several KB inline.
    pub(crate) outline_value_to_number: bool,
    /// When false, `emit_value_to_numeric_locals` inlines the full dynamic
    /// ToNumeric composite instead of calling the shared helper. Set false only
    /// while compiling that helper itself. Object coercion dominates repeated
    /// arithmetic expressions, so outlining it keeps user functions below
    /// Cranelift's per-function virtual-register limit.
    pub(crate) outline_value_to_numeric: bool,
    /// When `Some(local)`, `emit_object_write` is being emitted as the shared
    /// outlined write helper and must decide sloppy/strict `[[Set]]` failure
    /// behavior from the runtime value of `local` (a helper parameter carrying
    /// the calling function's strictness) rather than from the compile-time
    /// `is_current_function_strict()` of the helper body itself (which is a
    /// fixed, mode-less runtime helper).
    ///
    /// `None` only where **no Reference is in play** — property installation,
    /// class field definition, internal helper writes — and there
    /// `ambient_object_write_strict_flag_word` is authoritative. A Reference
    /// write installs its own carried `[[Strict]]` here through
    /// `expressions.rs`'s `with_reference_strictness`, inline emission
    /// included, because PutValue 3.d asks about `V.[[Strict]]` and not about
    /// the mode of the code being emitted. The two differ whenever lowering
    /// hoists a write into a generated function.
    pub(crate) object_write_strict_flag_local: Option<I32Local>,
}

pub fn emit(program: &ProgramIr) -> Result<WasmArtifact, EmitError> {
    emit_with_intl_profile_and_runtime_inputs(
        program,
        PromiseRejectionPolicy::default(),
        &scriptw_intl::IntlCompilationProfile::default(),
        crate::RuntimeArtifactInputs::new(None),
    )
}

pub fn emit_with_promise_rejection_policy(
    program: &ProgramIr,
    promise_rejection_policy: PromiseRejectionPolicy,
) -> Result<WasmArtifact, EmitError> {
    emit_with_intl_profile_and_runtime_inputs(
        program,
        promise_rejection_policy,
        &scriptw_intl::IntlCompilationProfile::default(),
        crate::RuntimeArtifactInputs::new(None),
    )
}

/// Compile with one immutable Intl data selection for all emission retries.
/// Programs without Intl or system-zone consumers leave its images unresolved.
pub fn emit_with_intl_profile(
    program: &ProgramIr,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_profile: &scriptw_intl::IntlCompilationProfile,
) -> Result<WasmArtifact, EmitError> {
    emit_with_intl_profile_and_runtime_inputs(
        program,
        promise_rejection_policy,
        intl_profile,
        crate::RuntimeArtifactInputs::new(None),
    )
}

/// Compile with optional bounded persistence of the program-independent runtime.
pub fn emit_with_intl_profile_and_runtime_cache(
    program: &ProgramIr,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_profile: &scriptw_intl::IntlCompilationProfile,
    cache: Option<&dyn crate::RuntimeArtifactCache>,
) -> Result<WasmArtifact, EmitError> {
    emit_with_intl_profile_and_runtime_inputs(
        program,
        promise_rejection_policy,
        intl_profile,
        crate::RuntimeArtifactInputs::new(cache),
    )
}

/// Compile with checked raw runtime inputs, resolved only for a linked program.
pub fn emit_with_intl_profile_and_runtime_inputs(
    program: &ProgramIr,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_profile: &scriptw_intl::IntlCompilationProfile,
    inputs: crate::RuntimeArtifactInputs<'_>,
) -> Result<WasmArtifact, EmitError> {
    emit_with_observation_mode(
        program,
        promise_rejection_policy,
        intl_profile,
        false,
        inputs,
    )
}

/// Emits the canonical schema witnesses and rooted Realm inventory consumed
/// solely by the native host graph adapter.
pub fn emit_with_rooted_snapshot(
    program: &ProgramIr,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_profile: &scriptw_intl::IntlCompilationProfile,
) -> Result<WasmArtifact, EmitError> {
    emit_with_rooted_snapshot_and_runtime_inputs(
        program,
        promise_rejection_policy,
        intl_profile,
        crate::RuntimeArtifactInputs::new(None),
    )
}

/// Snapshot emission uses the same checked runtime cache as ordinary emission.
pub fn emit_with_rooted_snapshot_and_runtime_cache(
    program: &ProgramIr,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_profile: &scriptw_intl::IntlCompilationProfile,
    cache: Option<&dyn crate::RuntimeArtifactCache>,
) -> Result<WasmArtifact, EmitError> {
    emit_with_rooted_snapshot_and_runtime_inputs(
        program,
        promise_rejection_policy,
        intl_profile,
        crate::RuntimeArtifactInputs::new(cache),
    )
}

/// Rooted observation shares the same checked build-package/runtime cache owner.
pub fn emit_with_rooted_snapshot_and_runtime_inputs(
    program: &ProgramIr,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_profile: &scriptw_intl::IntlCompilationProfile,
    inputs: crate::RuntimeArtifactInputs<'_>,
) -> Result<WasmArtifact, EmitError> {
    emit_with_observation_mode(
        program,
        promise_rejection_policy,
        intl_profile,
        true,
        inputs,
    )
}

fn emit_with_observation_mode(
    program: &ProgramIr,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_profile: &scriptw_intl::IntlCompilationProfile,
    snapshot: bool,
    inputs: crate::RuntimeArtifactInputs<'_>,
) -> Result<WasmArtifact, EmitError> {
    // Diagnostics are scanned *before* `program.script`: a stage that reports a
    // reason for failing also declines to produce a script, so checking the
    // script first replaces every honest diagnostic with the generic "no
    // lowered script ir". Module linking is the case that made this visible —
    // an unresolved specifier is a `LinkError`, not `Unsupported`, and used to
    // reach the backend as nothing at all.
    if let Some(diagnostic) = program.wasm_blocking_diagnostic() {
        return Err(EmitError::unsupported(diagnostic.message.clone()));
    }
    let script = program.script.as_ref().ok_or_else(|| {
        EmitError::unsupported("unsupported in scriptw wasm-aot first slice: no lowered script ir")
    })?;
    let is_module_entry = program
        .modules
        .as_ref()
        .is_some_and(|graph| !graph.entry_is_script);
    if is_module_entry != script.module_entry_evaluation().is_some() {
        return Err(EmitError::unsupported(
            "compiler invariant violated: Module entry and trusted evaluation-completion operation must have the same owner",
        ));
    }
    let intl_selection = scriptw_intl::IntlDataSelection::new(intl_profile.clone());
    emit_script(
        script,
        promise_rejection_policy,
        &intl_selection,
        snapshot,
        inputs,
    )
}

fn emit_script(
    script: &ScriptIr,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_selection: &scriptw_intl::IntlDataSelection,
    snapshot: bool,
    inputs: crate::RuntimeArtifactInputs<'_>,
) -> Result<WasmArtifact, EmitError> {
    let uses_heap = snapshot || runtime_requirement::requires_runtime(script);
    let mut prepared_script = script.clone();
    if uses_heap {
        super::builtins::append_empty_dynamic_function_bodies(&mut prepared_script);
    }
    let script = &prepared_script;
    for function in script.functions.iter().filter(|function| {
        function.protocol.execution_kind() == FunctionExecutionKind::AsyncGenerator
    }) {
        if let Some(feature) = function
            .body
            .statements
            .iter()
            .find_map(async_generator_dispatcher_unsupported_feature)
        {
            return Err(EmitError::unsupported(format!(
                "unsupported in scriptw wasm-aot first slice: async-generator body dispatcher for `{}` does not yet support {feature}",
                function.name
            )));
        }
    }

    let runtime = uses_heap
        .then(|| crate::runtime_artifact_with_inputs(intl_selection, inputs))
        .transpose()?;
    let kind = match &runtime {
        Some(runtime) => ModuleKind::Program(runtime),
        None => ModuleKind::Standalone,
    };
    Ok(
        module_assembly::emit_script_module(
            script,
            kind,
            promise_rejection_policy,
            intl_selection,
        )?
        .artifact,
    )
}

/// The runtime module, emitted from the empty script every program is a
/// continuation of.
pub(crate) fn emit_runtime_module(
    script: &ScriptIr,
    intl_selection: &scriptw_intl::IntlDataSelection,
    source_compilation_profile: crate::RuntimeSourceCompilationProfile,
) -> Result<EmittedModule, EmitError> {
    let mut script = script.clone();
    super::builtins::append_empty_dynamic_function_bodies(&mut script);
    module_assembly::emit_script_module(
        &script,
        ModuleKind::Runtime(source_compilation_profile),
        PromiseRejectionPolicy::default(),
        intl_selection,
    )
}

#[derive(Clone, Copy)]
enum AsyncGeneratorSuspension {
    Await,
    Yield,
}

fn async_generator_contains_suspension(
    statement: &StatementIr,
    suspension: AsyncGeneratorSuspension,
) -> bool {
    match statement {
        StatementIr::EmptyStatementCompletion(item) => {
            async_generator_contains_suspension(item.statement(), suspension)
        }
        StatementIr::ResumableClassDefinition(plan) => plan
            .prefixes()
            .flat_map(|prefix| prefix.statements())
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::AsyncAwait { .. } => matches!(suspension, AsyncGeneratorSuspension::Await),
        StatementIr::AsyncModuleInstantiation => false,
        StatementIr::ArrayDestructuringOperation(_) => false,
        StatementIr::GeneratorYield { .. } => {
            matches!(suspension, AsyncGeneratorSuspension::Yield)
        }
        StatementIr::AsyncGeneratorLoop(plan) => {
            (matches!(suspension, AsyncGeneratorSuspension::Await)
                && plan.resource().is_some_and(|resource| {
                    matches!(
                        resource.capability(),
                        scriptw_ir::AsyncGeneratorResourceCapabilityIr::Async(_)
                    )
                }))
                || plan
                    .regions()
                    .flat_map(|region| &region.block().statements)
                    .any(|statement| async_generator_contains_suspension(statement, suspension))
        }
        StatementIr::AsyncGeneratorIf(plan) => plan
            .regions()
            .flat_map(|region| &region.block().statements)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::AsyncGeneratorWith(plan) => plan
            .regions()
            .flat_map(|region| &region.block().statements)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::AsyncGeneratorSwitch(plan) => {
            (matches!(suspension, AsyncGeneratorSuspension::Await)
                && plan.resource().is_some_and(|resource| {
                    matches!(
                        resource.capability(),
                        scriptw_ir::AsyncGeneratorResourceCapabilityIr::Async(_)
                    )
                }))
                || plan
                    .regions()
                    .flat_map(|region| &region.block().statements)
                    .any(|statement| async_generator_contains_suspension(statement, suspension))
        }
        StatementIr::OrdinaryGeneratorLoop(plan) => plan
            .regions()
            .flat_map(|region| &region.block().statements)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::OrdinaryGeneratorIf(plan) => [plan.then_branch(), plan.else_branch()]
            .into_iter()
            .flat_map(|region| &region.block().statements)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::OrdinaryGeneratorArrayDestructuring(plan) => plan
            .body()
            .block()
            .statements
            .iter()
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::AsyncGeneratorResourceScope(plan) => {
            (matches!(suspension, AsyncGeneratorSuspension::Await)
                && matches!(
                    plan.capability(),
                    scriptw_ir::AsyncGeneratorResourceCapabilityIr::Async(_)
                ))
                || plan
                    .body()
                    .block()
                    .statements
                    .iter()
                    .any(|statement| async_generator_contains_suspension(statement, suspension))
        }
        StatementIr::AsyncGeneratorResourceRegistration(_) => false,
        StatementIr::AsyncGeneratorArrayDestructuring(plan) => plan
            .body()
            .block()
            .statements
            .iter()
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::AsyncFunctionArrayDestructuring(plan) => plan
            .body()
            .statements
            .iter()
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::OrdinaryGeneratorWith(plan) => [plan.head().region(), plan.body()]
            .into_iter()
            .flat_map(|region| &region.block().statements)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::AsyncFunctionWith(plan) => [plan.head(), plan.body()]
            .into_iter()
            .flat_map(|block| &block.statements)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::AsyncGeneratorForIn(plan) => [
            plan.head().region().block(),
            plan.initialization(),
            plan.body().block(),
        ]
        .into_iter()
        .flat_map(|block| &block.statements)
        .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::AsyncGeneratorForOf(plan) => {
            (matches!(suspension, AsyncGeneratorSuspension::Await)
                && (matches!(
                    plan.protocol(),
                    scriptw_ir::AsyncGeneratorIteratorProtocolIr::Awaited { .. }
                ) || plan.resource().is_some_and(|resource| {
                    matches!(
                        resource.capability(),
                        scriptw_ir::AsyncGeneratorResourceCapabilityIr::Async(_)
                    )
                })))
                || [
                    plan.head().region().block(),
                    plan.initialization().block(),
                    plan.body().block(),
                ]
                .into_iter()
                .flat_map(|block| &block.statements)
                .any(|statement| async_generator_contains_suspension(statement, suspension))
        }
        StatementIr::OrdinaryGeneratorSwitch(plan) => plan
            .regions()
            .flat_map(|region| &region.block().statements)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::GeneratorLoop {
            before_suspension,
            suspension_statement,
            after_suspension,
            ..
        } => before_suspension
            .iter()
            .chain(std::iter::once(suspension_statement.as_ref()))
            .chain(after_suspension)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::AsyncFunctionForOfIterator { .. } => {
            matches!(suspension, AsyncGeneratorSuspension::Await)
        }
        StatementIr::GeneratorForOfIterator { .. } => {
            matches!(suspension, AsyncGeneratorSuspension::Yield)
        }
        StatementIr::AsyncFunctionIf { .. }
        | StatementIr::AsyncFunctionWhile(_)
        | StatementIr::AsyncFunctionSwitch(_) => {
            matches!(suspension, AsyncGeneratorSuspension::Await)
        }
        StatementIr::GeneratorIf {
            then_before_yield,
            then_yield_statement,
            then_after_yield,
            else_before_yield,
            else_yield_statement,
            else_after_yield,
            ..
        } => then_before_yield
            .iter()
            .chain(then_yield_statement.as_deref())
            .chain(then_after_yield)
            .chain(else_before_yield)
            .chain(else_yield_statement.as_deref())
            .chain(else_after_yield)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::LexicalBlock(statements)
        | StatementIr::ParameterInitialization { statements, .. } => statements
            .iter()
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::Block(block) => block
            .statements
            .iter()
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::If {
            then_branch,
            else_branch,
            ..
        } => {
            async_generator_contains_suspension(then_branch, suspension)
                || else_branch.as_ref().is_some_and(|else_branch| {
                    async_generator_contains_suspension(else_branch, suspension)
                })
        }
        // A `for await` loop is itself a suspension: it awaits `next()` once per
        // iteration and awaits the iterator close on exit. Only recursing into
        // the body would report a nested for-await as suspension-free and let it
        // through a guard that exists precisely to keep second suspensions out.
        StatementIr::ForOfIterator {
            head:
                ForOfIteratorHeadIr::Assignment {
                    async_plan: Some(_),
                    ..
                },
            ..
        } => matches!(suspension, AsyncGeneratorSuspension::Await),
        StatementIr::ForOfIterator {
            head: ForOfIteratorHeadIr::AsyncDisposable(_),
            ..
        } => matches!(suspension, AsyncGeneratorSuspension::Await),
        StatementIr::While { body, .. }
        | StatementIr::DoWhile { body, .. }
        | StatementIr::For { body, .. }
        | StatementIr::ForOfIterator { body, .. }
        | StatementIr::ForInArray { body, .. }
        | StatementIr::ForInString { body, .. }
        | StatementIr::ForInObject { body, .. }
        | StatementIr::Labelled {
            statement: body, ..
        } => async_generator_contains_suspension(body, suspension),
        StatementIr::Switch {
            lexical_declarations,
            cases,
            ..
        } => lexical_declarations
            .iter()
            .chain(cases.iter().flat_map(|case| case.body.statements.iter()))
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::TryCatch {
            try_block,
            catch_block,
            ..
        } => try_block
            .statements
            .iter()
            .chain(&catch_block.statements)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::TryFinally {
            try_block,
            finally_block,
            ..
        } => try_block
            .statements
            .iter()
            .chain(&finally_block.statements)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::TryCatchFinally {
            try_block,
            catch_block,
            finally_block,
            ..
        } => try_block
            .statements
            .iter()
            .chain(&catch_block.statements)
            .chain(&finally_block.statements)
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::SyncDisposableScope {
            execution: SyncDisposableScopeExecutionIr::AsyncGenerator(_),
            body,
            ..
        } => body
            .statements
            .iter()
            .any(|statement| async_generator_contains_suspension(statement, suspension)),
        StatementIr::SyncDisposableScope {
            execution:
                SyncDisposableScopeExecutionIr::Immediate
                | SyncDisposableScopeExecutionIr::PlainGenerator(_)
                | SyncDisposableScopeExecutionIr::AsyncFunction(_),
            ..
        } => false,
        StatementIr::AsyncDisposableScope {
            execution: AsyncDisposableScopeExecutionIr::AsyncGenerator(_),
            body,
            ..
        } => match suspension {
            AsyncGeneratorSuspension::Await => true,
            AsyncGeneratorSuspension::Yield => body
                .statements
                .iter()
                .any(|statement| async_generator_contains_suspension(statement, suspension)),
        },
        StatementIr::AsyncDisposableScope {
            execution: AsyncDisposableScopeExecutionIr::AsyncFunction(_),
            ..
        } => false,
        _ => false,
    }
}

fn async_generator_dispatcher_unsupported_feature(statement: &StatementIr) -> Option<&'static str> {
    async_generator_admission::check(statement)
}

pub(crate) fn async_generator_for_await_is_transparent_yield(
    binding: &str,
    body: &StatementIr,
) -> bool {
    match body {
        StatementIr::GeneratorYield {
            value:
                TypedExpr {
                    expr: ExprIr::Identifier(yielded_binding),
                    ..
                },
            form,
            resume_mode: GeneratorResumeModeIr::Ignore,
            ..
        } => match form {
            YieldForm::Plain => yielded_binding == binding,
            YieldForm::Delegate(_) => false,
        },
        StatementIr::LexicalBlock(statements) => {
            matches!(statements.as_slice(), [statement]
                if async_generator_for_await_is_transparent_yield(binding, statement))
        }
        StatementIr::Block(block) => {
            matches!(block.statements.as_slice(), [statement]
                if async_generator_for_await_is_transparent_yield(binding, statement))
        }
        StatementIr::SyncDisposableScope { .. } | StatementIr::AsyncDisposableScope { .. } => false,
        _ => false,
    }
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn allocate_local_binding(
        &mut self,
        _mode: scriptw_ir::BindingMode,
        function: &mut Function,
    ) -> BindingStorage {
        let value = self.schema.reserve_value_local(function);
        value.set_undefined(function);
        let initialized = self.schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        initialized.store(function);
        let id = LocalBindingId {
            ordinal: self.local_bindings.len(),
        };
        self.local_bindings.push(Some(LocalBindingLocals {
            value,
            initialization: LocalBindingInitialization::Checked(initialized),
            role: LocalBindingRole::Source {
                private_argument_list: None,
            },
        }));
        BindingStorage::Local(id)
    }
    /// FunctionDeclarationInstantiation initializes every new local var to
    /// undefined before its name becomes visible to source body emission.
    pub(crate) fn allocate_initialized_var_binding(
        &mut self,
        function: &mut Function,
    ) -> BindingStorage {
        let value = self.schema.reserve_value_local(function);
        value.set_undefined(function);
        let id = LocalBindingId {
            ordinal: self.local_bindings.len(),
        };
        self.local_bindings.push(Some(LocalBindingLocals {
            value,
            initialization: LocalBindingInitialization::HoistedVar,
            role: LocalBindingRole::Source {
                private_argument_list: None,
            },
        }));
        BindingStorage::Local(id)
    }
    /// An eager inferred-name binding is compiler storage, with no source
    /// declaration mode or TDZ. Its whole value is initialized before use.
    pub(crate) fn allocate_compiler_temporary_binding(
        &mut self,
        source: &ValueLocals,
        function: &mut Function,
    ) -> BindingStorage {
        let value = self.schema.reserve_value_local(function);
        value.copy_from(source, function);
        let id = LocalBindingId {
            ordinal: self.local_bindings.len(),
        };
        self.local_bindings.push(Some(LocalBindingLocals {
            value,
            initialization: LocalBindingInitialization::CompilerTemporary,
            role: LocalBindingRole::CompilerTemporary,
        }));
        BindingStorage::Local(id)
    }
    pub(crate) fn local_binding(&self, id: LocalBindingId) -> &LocalBindingLocals {
        self.local_bindings[id.ordinal]
            .as_ref()
            .expect("live local binding owner")
    }
    pub(crate) fn release_local_binding(&mut self, id: LocalBindingId, function: &mut Function) {
        let binding = self.local_bindings[id.ordinal]
            .take()
            .expect("local binding released once");
        let initialized = binding.initialization_check();
        if let LocalBindingRole::Source {
            private_argument_list: Some(list),
        } = binding.role
        {
            list.clear(function);
        }
        if let Some(initialized) = initialized {
            self.schema.release_i32_local(initialized, function);
        }
        binding.value.clear(function);
    }
    /// The paired IR capture has compiler-private storage. Its ValueArray is
    /// never encoded as a JavaScript Array or a StoredValue reference.
    pub(crate) fn emit_store_captured_argument_list(
        &mut self,
        storage: BindingStorage,
        arguments: &GcLocal<ValueArray>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.schema;
        match storage {
            BindingStorage::EnvSlot { slot, hops } => {
                let environment = self.resolve_env_handle_local(hops, function);
                let cell = self.emit_environment_cell_local(&environment, slot, function);
                schema
                    .struct_type::<BindingCell>()
                    .field(BindingCellSchema::ARGUMENT_LIST)
                    .write(
                        &cell,
                        GcOperand::nullable_reference(arguments, schema),
                        schema,
                        function,
                    );
                schema
                    .struct_type::<BindingCell>()
                    .field(BindingCellSchema::INITIALIZED)
                    .write(&cell, GcOperand::boolean(true), schema, function);
                cell.clear(function);
                environment.clear(function);
            }
            BindingStorage::Local(id) => {
                let binding = self.local_bindings[id.ordinal]
                    .as_mut()
                    .expect("paired private capture owns a live binding");
                let LocalBindingRole::Source {
                    private_argument_list,
                } = &mut binding.role
                else {
                    return Err(EmitError::unsupported(
                        "compiler temporary cannot own a captured argument list",
                    ));
                };
                let list = private_argument_list.get_or_insert_with(|| {
                    schema
                        .reserve_gc_local::<ValueArray, Nullable>(function)
                        .initialize_null(schema, function)
                });
                list.replace(arguments.load(schema, function).nullable(), function);
                binding.mark_initialized(function);
                if let Some(frame) = self
                    .body_entry_locals
                    .as_ref()
                    .and_then(|entry| entry.resume_frame())
                {
                    Self::emit_store_frame_argument_list(
                        schema,
                        frame,
                        id.ordinal,
                        Some(arguments),
                        function,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn emit_load_captured_argument_list(
        &mut self,
        list: &scriptw_ir::CapturedArgumentListIr,
        function: &mut Function,
    ) -> Result<GcLocal<ValueArray>, EmitError> {
        let schema = self.schema;
        let storage = self.lookup_binding(list.storage_name()).ok_or_else(|| {
            EmitError::unsupported(
                "compiler invariant: captured argument list has no paired storage",
            )
        })?;
        let output = schema.reserve_gc_local::<ValueArray, NonNullable>(function);
        match storage {
            BindingStorage::EnvSlot { slot, hops } => {
                let environment = self.resolve_env_handle_local(hops, function);
                let cell = self.emit_environment_cell_local(&environment, slot, function);
                let result = output.initialize(
                    schema
                        .struct_type::<BindingCell>()
                        .field(BindingCellSchema::ARGUMENT_LIST)
                        .read(&cell, schema, function)
                        .reference()
                        .require_non_null(function),
                    function,
                );
                cell.clear(function);
                environment.clear(function);
                Ok(result)
            }
            BindingStorage::Local(id) => {
                if let Some(frame) = self
                    .body_entry_locals
                    .as_ref()
                    .and_then(|entry| entry.resume_frame())
                {
                    let table = schema
                        .reserve_gc_local::<PrivateArgumentListTable, NonNullable>(function)
                        .initialize(
                            schema
                                .struct_type::<InvocationFrame>()
                                .field(InvocationFrameSchema::PRIVATE_ARGUMENT_LISTS)
                                .read(frame, schema, function)
                                .reference(),
                            function,
                        );
                    let index = schema.reserve_i32_local(function);
                    function.instruction(&Instruction::I32Const(
                        i32::try_from(id.ordinal).map_err(|_| {
                            EmitError::unsupported(
                                "private argument-list slot exceeds Wasm array bounds",
                            )
                        })?,
                    ));
                    index.store(function);
                    let result = output.initialize(
                        schema
                            .array_type::<PrivateArgumentListTable>()
                            .read(&table, index, schema, function)
                            .reference()
                            .require_non_null(function),
                        function,
                    );
                    schema.release_i32_local(index, function);
                    table.clear(function);
                    Ok(result)
                } else {
                    let binding = self.local_binding(id);
                    let list = binding.private_argument_list().ok_or_else(|| {
                        EmitError::unsupported(
                            "compiler invariant: argument-list storage was not initialized",
                        )
                    })?;
                    Ok(output.initialize(
                        list.load(schema, function).require_non_null(function),
                        function,
                    ))
                }
            }
        }
    }

    /// Stable binding ordinals retain lists across suspension independently of
    /// JavaScript locals. Growing the private table preserves existing edges.
    fn emit_store_frame_argument_list(
        schema: &RuntimeSchema,
        frame: &GcLocal<InvocationFrame>,
        ordinal: usize,
        arguments: Option<&GcLocal<ValueArray>>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let ordinal = i32::try_from(ordinal).map_err(|_| {
            EmitError::unsupported("private argument-list slot exceeds Wasm array bounds")
        })?;
        let required = ordinal.checked_add(1).ok_or_else(|| {
            EmitError::unsupported("private argument-list table exceeds Wasm array bounds")
        })?;
        let table = schema
            .reserve_gc_local::<PrivateArgumentListTable, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<InvocationFrame>()
                    .field(InvocationFrameSchema::PRIVATE_ARGUMENT_LISTS)
                    .read(frame, schema, function)
                    .reference(),
                function,
            );
        let length = schema.reserve_i32_local(function);
        schema
            .array_type::<PrivateArgumentListTable>()
            .length(&table, schema, function);
        length.store(function);
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(ordinal));
        index.store(function);
        length.load(function);
        index.load(function);
        function.instruction(&Instruction::I32LeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        let new_length = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(required));
        new_length.store(function);
        let grown = schema
            .reserve_gc_local::<PrivateArgumentListTable, NonNullable>(function)
            .initialize(
                schema.array_type::<PrivateArgumentListTable>().filled(
                    GcOperand::null(schema),
                    new_length,
                    function,
                ),
                function,
            );
        let cursor = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        cursor.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        cursor.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::BrIf(1));
        let old = schema
            .reserve_gc_local::<ValueArray, Nullable>(function)
            .initialize(
                schema
                    .array_type::<PrivateArgumentListTable>()
                    .read(&table, cursor, schema, function)
                    .reference(),
                function,
            );
        schema.array_type::<PrivateArgumentListTable>().write(
            &grown,
            cursor,
            GcOperand::reference(&old, schema),
            schema,
            function,
        );
        old.clear(function);
        cursor.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        cursor.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::PRIVATE_ARGUMENT_LISTS)
            .write(
                frame,
                GcOperand::reference(&grown, schema),
                schema,
                function,
            );
        table.replace(grown.load(schema, function), function);
        schema.release_i32_local(cursor, function);
        grown.clear(function);
        schema.release_i32_local(new_length, function);
        function.instruction(&Instruction::End);
        match arguments {
            Some(arguments) => schema.array_type::<PrivateArgumentListTable>().write(
                &table,
                index,
                GcOperand::nullable_reference(arguments, schema),
                schema,
                function,
            ),
            None => schema.array_type::<PrivateArgumentListTable>().write(
                &table,
                index,
                GcOperand::null(schema),
                schema,
                function,
            ),
        }
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        table.clear(function);
        Ok(())
    }

    pub(crate) fn emit_clear_private_argument_lists_in_scope(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.schema;
        let storages = self
            .binding_scopes
            .last()
            .map(|scope| scope.values().copied().collect::<Vec<_>>())
            .unwrap_or_default();
        for storage in storages {
            match storage {
                BindingStorage::Local(id) => {
                    if let Some(list) = self.local_binding(id).private_argument_list() {
                        list.set_null(schema, function);
                        if let Some(frame) = self
                            .body_entry_locals
                            .as_ref()
                            .and_then(|entry| entry.resume_frame())
                        {
                            Self::emit_store_frame_argument_list(
                                schema, frame, id.ordinal, None, function,
                            )?;
                        }
                    }
                }
                BindingStorage::EnvSlot { slot, hops } => {
                    let environment = self.resolve_env_handle_local(hops, function);
                    let cell = self.emit_environment_cell_local(&environment, slot, function);
                    schema
                        .struct_type::<BindingCell>()
                        .field(BindingCellSchema::ARGUMENT_LIST)
                        .write(&cell, GcOperand::null(schema), schema, function);
                    cell.clear(function);
                    environment.clear(function);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn runtime_schema(&self) -> &'a RuntimeSchema {
        self.schema
    }
    pub(crate) fn emit_interned_string_reference(
        &self,
        value: &str,
        function: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        let ordinal = self.strings.pooled_string_index(value)?;
        if self.uses_heap {
            return self.emit_pooled_string_reference(ordinal, function);
        }
        // Runtime-free scripts do not initialize the module's pooled table.
        // Their literal primitives still own real GC Strings and UTF-16 units.
        let schema = self.schema;
        let units = StringPool::runtime_code_units_for_string(value);
        let length = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let unit = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(
            i32::try_from(units.len()).expect("literal UTF-16 length"),
        ));
        length.store(function);
        let units_slot = schema.reserve_gc_local::<CodeUnitArray, NonNullable>(function);
        let construction = StringConstruction::allocate(schema, units_slot, length, function);
        for (ordinal, code_unit) in units.into_iter().enumerate() {
            function.instruction(&Instruction::I32Const(
                i32::try_from(ordinal).expect("literal UTF-16 index"),
            ));
            index.store(function);
            function.instruction(&Instruction::I32Const(i32::from(code_unit)));
            unit.store(function);
            construction.write(index, unit, schema, function);
        }
        let string = construction.publish(schema, function);
        schema.release_i32_local(unit, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        Ok(string)
    }
    pub(crate) fn emit_runtime_error_message_reference(
        &self,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        self.emit_pooled_string_reference(
            self.strings.runtime_error_string_index(message)?,
            function,
        )
    }
    pub(crate) fn emit_well_known_symbol_reference(
        &self,
        symbol: scriptw_ir::WellKnownSymbol,
        function: &mut Function,
    ) -> Result<GcStackReference<SymbolValue>, EmitError> {
        Ok(self.schema.well_known_symbol_reference(symbol, function))
    }
    pub(crate) fn current_private_environment(&self) -> &GcLocal<PrivateEnvironment, Nullable> {
        &self.current_private_environment
    }
    pub(crate) fn replace_current_private_environment(
        &self,
        value: GcStackReference<PrivateEnvironment, Nullable>,
        function: &mut Function,
    ) {
        self.current_private_environment.replace(value, function);
    }
    pub(crate) fn load_current_realm(&self, function: &mut Function) -> GcLocal<RealmRecord> {
        let slot = self
            .schema
            .reserve_gc_local::<RealmRecord, NonNullable>(function);
        slot.initialize(self.schema.load_current_realm(function), function)
    }
    pub(crate) fn replace_current_realm(
        &self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) {
        self.schema.replace_current_realm(realm, function);
    }
    pub(crate) fn emit_propagate_current_throw_if_needed(&mut self, function: &mut Function) {
        self.completion.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }
    pub(crate) fn emit_clear_throw_diagnostic(
        &self,
        role: ThrowDiagnosticRole,
        function: &mut Function,
    ) {
        self.schema.clear_throw_diagnostic(role, function);
    }
    pub(crate) fn emit_store_throw_diagnostic(
        &self,
        role: ThrowDiagnosticRole,
        value: &GcLocal<StringValue>,
        function: &mut Function,
    ) {
        self.schema.store_throw_diagnostic(role, value, function);
    }
    pub(crate) fn current_environment(&self) -> &GcLocal<Environment, Nullable> {
        &self.current_environment
    }
    pub(crate) fn replace_current_environment(
        &self,
        value: GcStackReference<Environment, Nullable>,
        function: &mut Function,
    ) {
        self.current_environment.replace(value, function);
    }
    pub(crate) fn completion(&self) -> &CompletionLocals {
        &self.completion
    }
    pub(crate) fn runtime_helper_base(
        &self,
    ) -> Result<crate::runtime_helpers::RuntimeHelperFunctionBase, EmitError> {
        self.runtime_helper_base.ok_or_else(|| {
            EmitError::unsupported(
                "compiler invariant: runtime helper call has no registered helper plan",
            )
        })
    }
    pub(crate) fn body_entry_locals(&self) -> Option<&crate::function_entry::BodyEntryLocals> {
        self.body_entry_locals.as_ref()
    }
    pub(crate) fn current_program(&self) -> &crate::gc_types::ProgramInstanceLocals {
        self.body_entry_locals()
            .map(crate::function_entry::BodyEntryLocals::program)
            .or(self.entry_program.as_ref())
            .expect("program-consuming entry requires its explicit frame-local owner")
    }
    /// Main and registered helper entries bind one actual owner. Callable
    /// entries already selected theirs from captured defining/calling P.
    pub(crate) fn bind_entry_program(
        &mut self,
        program: crate::gc_types::ProgramInstanceLocals,
        function: &mut Function,
    ) {
        assert!(
            self.body_entry_locals.is_none() && self.entry_program.is_none(),
            "one explicit program binding per main/helper entry"
        );
        assert!(
            matches!(
                self.module_state,
                FunctionModuleState::Main(_, _) | FunctionModuleState::RuntimeOperation(_)
            ),
            "only main and registered helpers bind program operands"
        );
        program.mark_native_context(self.schema, function);
        self.entry_program = Some(program);
    }
    pub(crate) fn capture_current_program(
        &self,
        function: &mut Function,
    ) -> crate::gc_types::ProgramInstanceLocals {
        crate::gc_types::ProgramInstanceLocals::bind(
            self.schema,
            self.current_program().reference(self.schema, function),
            function,
        )
    }
    pub(crate) fn clear_entry_program(&mut self, function: &mut Function) {
        if let Some(program) = self.entry_program.take() {
            program.clear(function);
        }
    }
    /// Root's P initializer emitter declares this body as ProgramInitialize,
    /// publishes a P result, and never enters compile() or its JS prologue.
    pub(super) fn begin_program_initialize_body(&mut self) -> Function {
        assert!(
            self.is_program_initialize()
                && self.body_entry_locals.is_none()
                && self.entry_program.is_none()
                && StaticSignature::ProgramInitialize
                    .parameter_types()
                    .is_empty()
                && self
                    .owned_body
                    .as_ref()
                    .is_some_and(|body| body.parameter_types().is_empty()),
            "P initialization starts from its unexecuted zero-argument bootstrap builder"
        );
        self.take_owned_body()
    }
    pub(crate) fn current_function_context(&self) -> Option<&GcLocal<FunctionContext>> {
        self.body_entry_locals()
            .and_then(|entry| entry.function_context())
            .or(self.helper_function_context.as_ref())
    }
    pub(crate) fn clear_helper_function_context(&mut self, function: &mut Function) {
        if let Some(context) = self.helper_function_context.take() {
            context.clear(function);
        }
    }
    pub(crate) fn helper_execution_realm(&self) -> Option<&GcLocal<RealmRecord>> {
        self.helper_execution_realm.as_ref()
    }
    pub(crate) fn clear_helper_execution_realm(&mut self, function: &mut Function) {
        if let Some(realm) = self.helper_execution_realm.take() {
            realm.clear(function);
        }
    }
    fn take_owned_body(&mut self) -> Function {
        self.owned_body
            .take()
            .expect("one registered body per FunctionBuilder")
    }
    pub(crate) fn helper_parameters<P: crate::runtime_helpers::HelperParameters>(
        &mut self,
        function: &mut Function,
    ) -> P {
        assert!(
            matches!(self.module_state, FunctionModuleState::RuntimeOperation(actual) if actual == P::ID),
            "helper parameter roles must match this declaration"
        );
        let parameters = self.schema.helper_parameters::<P>(function);
        if let Some(program) = parameters.caller_program() {
            let owner = crate::gc_types::ProgramInstanceLocals::bind(
                self.schema,
                program.load(self.schema, function),
                function,
            );
            self.bind_entry_program(owner, function);
        }
        if let Some(environment) = parameters.caller_environment() {
            self.current_environment
                .replace(environment.load(self.schema, function), function);
        }
        if let Some(context) = parameters.caller_function_context() {
            assert!(
                self.helper_function_context.is_none(),
                "one callable context per helper body"
            );
            self.helper_function_context = Some(
                self.schema
                    .reserve_gc_local(function)
                    .initialize(context.load(self.schema, function), function),
            );
        }
        if let Some(realm) = parameters.caller_execution_realm() {
            assert!(
                self.helper_execution_realm.is_none() && self.current_function_context().is_none(),
                "one execution Realm authority per helper body"
            );
            self.helper_execution_realm = Some(
                self.schema
                    .reserve_gc_local(function)
                    .initialize(realm.load(self.schema, function), function),
            );
        }
        parameters
    }

    fn new_main(
        script: &'a ScriptIr,
        strings: &'a StringPool,
        functions: &'a FunctionMetaRegistry,
        uses_heap: bool,
        runtime_bootstrap_plan: RuntimeBootstrapPlan,
        runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
        module_globals: &'a FinalizedModuleGlobals,
        promise_rejection_policy: PromiseRejectionPolicy,
    ) -> Self {
        let mut builder = Self::new(
            module_globals.schema(),
            &script.body,
            script.template_source,
            &[],
            script.owned_env_bindings.as_slice(),
            script.eval_environment.as_ref(),
            &[],
            strings,
            functions,
            None,
            FunctionFlavor::Ordinary,
            FunctionArgumentsProtocol::script_main(),
            None,
            script.strict,
            None,
            Some(&script.global_bindings),
            uses_heap,
            FunctionModuleState::Main(module_globals, promise_rejection_policy),
            false,
            runtime_bootstrap_plan,
            runtime_helper_base,
        );
        builder.module_prelude_id = script.module_prelude.as_ref().map(|unit| unit.id);
        builder.script_entry_after_prelude_id = script
            .script_entry_after_prelude
            .as_ref()
            .map(|entry| entry.unit().id);
        builder.global_prelude_phase = module_globals.global_prelude_phase();
        builder
    }

    fn new_prepared_script(
        schema: &'a RuntimeSchema,
        unit: &'a PreparedScriptUnit,
        strings: &'a StringPool,
        functions: &'a FunctionMetaRegistry,
        uses_heap: bool,
        runtime_bootstrap_plan: RuntimeBootstrapPlan,
        runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
    ) -> Self {
        Self::new(
            schema,
            &unit.body,
            unit.template_source,
            &[],
            &unit.owned_env_bindings,
            unit.eval_environment.as_ref(),
            &[],
            strings,
            functions,
            Some(unit.id.function_id()),
            FunctionFlavor::Ordinary,
            FunctionArgumentsProtocol::script_main(),
            None,
            unit.strict,
            None,
            Some(&unit.global_bindings),
            uses_heap,
            FunctionModuleState::PreparedScript(
                unit,
                &functions
                    .get(&unit.id.function_id())
                    .expect("prepared Script has its planned entry")
                    .entry,
            ),
            false,
            runtime_bootstrap_plan,
            runtime_helper_base,
        )
    }

    fn new_function(
        schema: &'a RuntimeSchema,
        function: &'a FunctionIr,
        global_bindings: &'a GlobalBindingPlan,
        strings: &'a StringPool,
        functions: &'a FunctionMetaRegistry,
        uses_heap: bool,
        runtime_bootstrap_plan: RuntimeBootstrapPlan,
        runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
    ) -> Result<Self, EmitError> {
        let (function, arguments_protocol, owned_env_bindings) =
            UserFunctionActivationPlan::for_function(function)?.into_parts();
        Ok(Self::new(
            schema,
            &function.body,
            function.template_source,
            function.params.as_slice(),
            owned_env_bindings,
            function.eval_environment.as_ref(),
            function.captured_bindings.as_slice(),
            strings,
            functions,
            Some(function.id.clone()),
            function.protocol.flavor(),
            arguments_protocol,
            function.lexical_derived_activation.as_ref(),
            function.strict,
            function.is_named_expression.then(|| function.name.clone()),
            Some(global_bindings),
            uses_heap,
            FunctionModuleState::Internal(
                &functions
                    .get(&function.id)
                    .expect("user function has its planned entry")
                    .entry,
            ),
            function.is_derived_constructor,
            runtime_bootstrap_plan,
            runtime_helper_base,
        ))
    }

    fn new_host_builtin(
        schema: &'a RuntimeSchema,
        builtin: HostBuiltinId,
        strings: &'a StringPool,
        functions: &'a FunctionMetaRegistry,
        uses_heap: bool,
        runtime_bootstrap_plan: RuntimeBootstrapPlan,
        runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
    ) -> Self {
        let function_id = builtin.function_id();
        Self::new(
            schema,
            &EMPTY_BLOCK,
            None,
            &[],
            &[],
            None,
            &[],
            strings,
            functions,
            Some(function_id),
            FunctionFlavor::Ordinary,
            FunctionArgumentsProtocol::strict_internal_callable(),
            None,
            true,
            None,
            None,
            uses_heap,
            FunctionModuleState::Internal(
                &functions
                    .get(&builtin.function_id())
                    .expect("host builtin has its planned entry")
                    .entry,
            ),
            false,
            runtime_bootstrap_plan,
            runtime_helper_base,
        )
    }

    /// No Script, Realm, environment or source entry is attached to this body.
    /// Only the dedicated typed module owner may publish its ProgramInstance result.
    pub(super) fn new_program_initialize(
        schema: &'a RuntimeSchema,
        strings: &'a StringPool,
        functions: &'a FunctionMetaRegistry,
        runtime_bootstrap_plan: RuntimeBootstrapPlan,
        runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
    ) -> Self {
        Self::new(
            schema,
            &EMPTY_BLOCK,
            None,
            &[],
            &[],
            None,
            &[],
            strings,
            functions,
            None,
            FunctionFlavor::Ordinary,
            FunctionArgumentsProtocol::strict_internal_callable(),
            None,
            true,
            None,
            None,
            true,
            FunctionModuleState::ProgramInitialize,
            false,
            runtime_bootstrap_plan,
            runtime_helper_base,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_runtime_operation_helper(
        schema: &'a RuntimeSchema,
        helper: RuntimeHelperId,
        strings: &'a StringPool,
        functions: &'a FunctionMetaRegistry,
        uses_heap: bool,
        runtime_bootstrap_plan: RuntimeBootstrapPlan,
        runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
    ) -> Self {
        Self::new(
            schema,
            &EMPTY_BLOCK,
            None,
            &[],
            &[],
            None,
            &[],
            strings,
            functions,
            None,
            FunctionFlavor::Ordinary,
            FunctionArgumentsProtocol::strict_internal_callable(),
            None,
            true,
            None,
            None,
            uses_heap,
            FunctionModuleState::RuntimeOperation(helper),
            false,
            runtime_bootstrap_plan,
            runtime_helper_base,
        )
    }

    fn new_standard_builtin(
        schema: &'a RuntimeSchema,
        builtin: StandardBuiltinId,
        entry: &'a crate::function_entry::PlannedFunctionEntry,
        strings: &'a StringPool,
        functions: &'a FunctionMetaRegistry,
        uses_heap: bool,
        runtime_bootstrap_plan: RuntimeBootstrapPlan,
        runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
    ) -> Self {
        Self::new(
            schema,
            &EMPTY_BLOCK,
            None,
            &[],
            &[],
            None,
            &[],
            strings,
            functions,
            Some(builtin.function_id()),
            FunctionFlavor::Ordinary,
            FunctionArgumentsProtocol::strict_internal_callable(),
            None,
            true,
            None,
            None,
            uses_heap,
            FunctionModuleState::Internal(entry),
            false,
            runtime_bootstrap_plan,
            runtime_helper_base,
        )
    }

    fn new(
        schema: &'a RuntimeSchema,
        body: &'a BlockIr,
        template_source: Option<scriptw_ir::TemplateSourceIr>,
        params: &'a [FunctionParamIr],
        owned_env_bindings: &'a [OwnedEnvBindingIr],
        eval_environment: Option<&'a scriptw_ir::EvalEnvironmentRoleIr>,
        captured_bindings: &'a [scriptw_ir::CapturedBindingIr],
        strings: &'a StringPool,
        functions: &'a FunctionMetaRegistry,
        function_id: Option<FunctionId>,
        function_flavor: FunctionFlavor,
        function_arguments_protocol: FunctionArgumentsProtocol,
        lexical_derived_activation: Option<&'a DerivedConstructorActivationIr>,
        strict: bool,
        self_binding_name: Option<String>,
        script_global_bindings: Option<&'a GlobalBindingPlan>,
        uses_heap: bool,
        module_state: FunctionModuleState<'a>,
        is_derived_constructor: bool,
        runtime_bootstrap_plan: RuntimeBootstrapPlan,
        runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
    ) -> Self {
        let parameter_types = match &module_state {
            FunctionModuleState::Main(_, _) => schema.signature(StaticSignature::Main),
            FunctionModuleState::ProgramInitialize => {
                schema.signature(StaticSignature::ProgramInitialize)
            }
            FunctionModuleState::PreparedScript(_, entry)
            | FunctionModuleState::Internal(entry) => schema.signature(entry.signature()),
            FunctionModuleState::RuntimeOperation(helper) => helper.definition(schema.layouts()),
        }
        .parameters()
        .to_vec();
        let return_abi = module_state.return_abi();
        let hoisted_vars = match &module_state {
            FunctionModuleState::ProgramInitialize => Vec::new(),
            FunctionModuleState::Main(_, _) => script_global_bindings
                .expect("main builder must carry the global binding plan")
                .main_frame_write_bindings()
                .map(|binding| binding.name.clone())
                .collect(),
            FunctionModuleState::PreparedScript(unit, _)
                if unit.has_global_variable_environment() =>
            {
                unit.global_bindings
                    .main_frame_write_bindings()
                    .map(|binding| binding.name.clone())
                    .collect()
            }
            FunctionModuleState::Internal(_)
            | FunctionModuleState::RuntimeOperation(_)
            | FunctionModuleState::PreparedScript(_, _) => collect_hoisted_vars_block_root(body),
        };
        let template_source_body = template_source;
        let async_generator_resume_environment_plan = function_id
            .as_deref()
            .and_then(|id| functions.get(id))
            .and_then(|meta| meta.async_generator_resume_environment_plan.clone());
        let mut owned_body = Function::with_parameters(
            LocalDeclarations::from_types(std::iter::empty()),
            parameter_types,
        );
        let current_environment = schema
            .reserve_gc_local::<Environment, Nullable>(&mut owned_body)
            .initialize_null(schema, &mut owned_body);
        let current_private_environment = schema
            .reserve_gc_local::<PrivateEnvironment, Nullable>(&mut owned_body)
            .initialize_null(schema, &mut owned_body);
        let completion = schema.reserve_completion(&mut owned_body);
        completion.initialize(&mut owned_body);
        let body_entry_locals = module_state
            .planned_body_entry()
            .map(|entry| entry.materialize(schema, &mut owned_body));
        if let Some(entry) = &body_entry_locals {
            current_environment.replace(
                entry.lexical_environment().load(schema, &mut owned_body),
                &mut owned_body,
            );
        }
        let direct_eval_context = if let Some(entry) = &body_entry_locals {
            current_private_environment.replace(
                entry.private_environment().load(schema, &mut owned_body),
                &mut owned_body,
            );
            entry.direct_eval_context().map(|context| {
                let slot = schema
                    .reserve_gc_local::<DirectEvalExecutionContext, Nullable>(&mut owned_body);
                slot.initialize(context.load(schema, &mut owned_body), &mut owned_body)
            })
        } else {
            None
        };
        let direct_eval_context = direct_eval_context.or_else(|| {
            captured_bindings
                .iter()
                .any(|binding| binding.name == scriptw_ir::DIRECT_EVAL_EXECUTION_CONTEXT_NAME)
                .then(|| {
                    schema
                        .reserve_gc_local::<DirectEvalExecutionContext, Nullable>(&mut owned_body)
                        .initialize_null(schema, &mut owned_body)
                })
        });
        Self {
            schema,
            owned_body: Some(owned_body),
            current_environment,
            current_private_environment,
            direct_eval_context,
            completion,
            local_bindings: Vec::new(),
            body_entry_locals,
            entry_program: None,
            source_activation: None,
            helper_function_context: None,
            helper_execution_realm: None,
            bootstrap_realm: None,
            body,
            template_source_body,
            template_source_execution: None,
            module_prelude_id: None,
            script_entry_after_prelude_id: None,
            global_prelude_phase: None,
            params,
            owned_env_bindings,
            eval_environment,
            captured_bindings,
            strings,
            functions,
            function_id,
            function_flavor,
            function_arguments_protocol,
            lexical_derived_activation,
            is_derived_constructor,
            strict,
            self_binding_name,
            script_global_bindings,
            uses_heap,
            completion_exit: CompletionExit::for_return_abi(return_abi),
            module_state,
            hoisted_vars,
            binding_scopes: Vec::new(),
            emitted_local_count: 0,
            environment_depth: 0,
            control_stack: Vec::new(),
            breakable_stack: Vec::new(),
            loop_stack: Vec::new(),
            label_stack: Vec::new(),
            throw_handler_stack: Vec::new(),
            finally_stack: Vec::new(),
            generator_finalizer_depth: 0,
            statement_list_value_context: None,
            checked_async_generator_environment_owner: None,
            checked_async_generator_resource_owner: None,
            async_generator_resume_environment_plan,
            runtime_bootstrap_plan,
            runtime_helper_base,
            outline_string_equality: true,
            outline_number_to_string: true,
            outline_string_to_number: true,
            outline_value_to_string: true,
            outline_value_to_number: true,
            outline_value_to_numeric: true,
            object_write_strict_flag_local: None,
        }
    }

    pub(crate) fn emitted_local_count(&self) -> u32 {
        self.emitted_local_count
    }

    pub(crate) const fn is_main(&self) -> bool {
        matches!(self.return_abi(), ReturnAbi::MainExport)
    }

    pub(crate) const fn is_program_initialize(&self) -> bool {
        matches!(self.module_state, FunctionModuleState::ProgramInitialize)
    }

    pub(crate) fn next_program_site(
        &mut self,
        kind: ProgramSiteKind,
    ) -> Result<ProgramSiteId, EmitError> {
        match &self.module_state {
            FunctionModuleState::Main(_, _) => {}
            FunctionModuleState::PreparedScript(_, entry)
            | FunctionModuleState::Internal(entry)
                if self.has_source_execution_environment() =>
            {
                // Retain the planned-source proof even though table slots no
                // longer encode the callable's Wasm index.
                entry.source_site_owner()?;
            }
            FunctionModuleState::PreparedScript(_, _)
            | FunctionModuleState::Internal(_)
            | FunctionModuleState::RuntimeOperation(_)
            | FunctionModuleState::ProgramInitialize => {
                return Err(EmitError::unsupported(
                    "program caches require a genuine source site",
                ));
            }
        }
        Ok(ProgramSiteId(self.functions.next_program_site_index(kind)?))
    }

    pub(crate) fn has_source_execution_environment(&self) -> bool {
        match self.module_state {
            FunctionModuleState::Main(_, _) | FunctionModuleState::PreparedScript(_, _) => true,
            FunctionModuleState::Internal(_) => self
                .current_function_meta()
                .is_some_and(|meta| meta.standard_builtin.is_none() && meta.host_builtin.is_none()),
            FunctionModuleState::RuntimeOperation(_) | FunctionModuleState::ProgramInitialize => {
                false
            }
        }
    }

    pub(crate) fn has_global_script_bindings(&self) -> bool {
        match self.module_state {
            FunctionModuleState::Main(_, _) => true,
            FunctionModuleState::PreparedScript(unit, _) => unit.has_global_variable_environment(),
            FunctionModuleState::Internal(_)
            | FunctionModuleState::RuntimeOperation(_)
            | FunctionModuleState::ProgramInitialize => false,
        }
    }

    pub(crate) fn direct_eval_derived_owner(&self) -> Option<&scriptw_ir::DirectEvalDerivedOwner> {
        match self.module_state {
            FunctionModuleState::ProgramInitialize => None,
            FunctionModuleState::PreparedScript(unit, _) => match &unit.kind {
                PreparedScriptKind::DirectEval(context) => context.derived_owner(),
                PreparedScriptKind::RealmScript
                | PreparedScriptKind::IndirectEval
                | PreparedScriptKind::ShadowRealmEvaluate => None,
            },
            FunctionModuleState::Main(_, _)
            | FunctionModuleState::Internal(_)
            | FunctionModuleState::RuntimeOperation(_) => {
                self.direct_eval_context.as_ref()?;
                let function_id = self
                    .function_id
                    .as_ref()
                    .expect("capturing arrow has an id");
                let unit = self
                    .functions
                    .prepared_scripts()
                    .iter()
                    .find_map(|entry| match &entry.outcome {
                        PreparedScriptOutcome::Executable(unit)
                            if unit.function_ids.contains(function_id) =>
                        {
                            Some(unit)
                        }
                        PreparedScriptOutcome::Executable(_)
                        | PreparedScriptOutcome::DeferredSyntaxError { .. } => None,
                    })
                    .expect("capturing arrow belongs to an independently prepared Script");
                let PreparedScriptKind::DirectEval(context) = &unit.kind else {
                    panic!("only direct eval supplies a caller execution context");
                };
                context.derived_owner()
            }
        }
    }

    pub(crate) fn direct_eval_execution_context_local(
        &self,
    ) -> Option<&GcLocal<DirectEvalExecutionContext, Nullable>> {
        self.direct_eval_context.as_ref()
    }

    pub(crate) fn planned_entry(&self) -> Option<&crate::function_entry::PlannedFunctionEntry> {
        match self.module_state {
            FunctionModuleState::Internal(entry)
            | FunctionModuleState::PreparedScript(_, entry) => Some(entry),
            FunctionModuleState::Main(_, _)
            | FunctionModuleState::RuntimeOperation(_)
            | FunctionModuleState::ProgramInitialize => None,
        }
    }

    pub(crate) const fn return_abi(&self) -> ReturnAbi {
        self.completion_exit.return_abi()
    }

    pub(crate) fn is_script_global_binding(&self, name: &str) -> bool {
        self.script_global_bindings
            .and_then(|bindings| bindings.get(name))
            .is_some_and(|binding| binding.declarations.is_declared())
    }

    pub(crate) fn compiled_host_builtin_by_name(&self, name: &str) -> Option<HostBuiltinId> {
        host_builtin_by_name(name).filter(|builtin| self.functions.host_surface_contains(*builtin))
    }

    pub(crate) fn should_read_script_global_property(&self, name: &str) -> bool {
        // An owned cell without its compiler alias is an emission invariant
        // failure, never an unresolved source global.
        !self.is_main()
            && name != LEXICAL_THIS_NAME
            && name != LEXICAL_ARGUMENTS_NAME
            && self.owned_env_slot(name).is_none()
            && self.lookup_binding(name).is_none()
    }

    pub(crate) fn emit_increment_local(
        &self,
        local: I64Local,
        delta: i64,
        function: &mut Function,
    ) {
        local.load(function);
        function.instruction(&Instruction::I64Const(delta));
        function.instruction(&Instruction::I64Add);
        local.store(function);
    }

    /// Every body retains its exact owned mixed-type declaration. The reported
    /// count is measured from that same body, never a source estimate or a
    /// homogeneous scalar prefix that can discard reference locals.
    pub(crate) fn finish_function(&mut self, function: Function) -> Function {
        self.emitted_local_count = function.allocated_local_count();
        function
    }

    pub(crate) const fn memarg32(offset: u64) -> MemArg {
        Self::memarg32_in(0, offset)
    }

    pub(crate) const fn memarg32_in(memory_index: u32, offset: u64) -> MemArg {
        MemArg {
            offset,
            align: 2,
            memory_index,
        }
    }

    pub(crate) const fn memarg16(offset: u64) -> MemArg {
        Self::memarg16_in(0, offset)
    }

    pub(crate) const fn memarg16_in(memory_index: u32, offset: u64) -> MemArg {
        MemArg {
            offset,
            align: 1,
            memory_index,
        }
    }

    pub(crate) const fn memarg8(offset: u64) -> MemArg {
        Self::memarg8_in(0, offset)
    }

    pub(crate) const fn memarg8_in(memory_index: u32, offset: u64) -> MemArg {
        MemArg {
            offset,
            align: 0,
            memory_index,
        }
    }

    pub(crate) const fn shared_memarg64(offset: u64) -> MemArg {
        MemArg {
            offset,
            align: 3,
            memory_index: 1,
        }
    }

    pub(crate) const fn shared_memarg32(offset: u64) -> MemArg {
        MemArg {
            offset,
            align: 2,
            memory_index: 1,
        }
    }

    pub(crate) const fn shared_memarg16(offset: u64) -> MemArg {
        MemArg {
            offset,
            align: 1,
            memory_index: 1,
        }
    }

    pub(crate) const fn shared_memarg8(offset: u64) -> MemArg {
        MemArg {
            offset,
            align: 0,
            memory_index: 1,
        }
    }
}
