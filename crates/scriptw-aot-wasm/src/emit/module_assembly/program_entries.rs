//! Typed export plans and native/ordinary module entry points.
use super::*;
use crate::function_layout::ProgramInitializeIndex;
use crate::gc_types::RuntimeSchema;

/// Emits one module. A heap-backed script is split at the runtime/program
/// boundary of the function space (see [`FunctionIndexLayout`]): the runtime
/// module compiles every standard and host builtin and runtime helper, and a
/// program module compiles only what the script owns, importing the rest.
pub(crate) fn emit_script_module(
    script: &ScriptIr,
    kind: ModuleKind<'_>,
    promise_rejection_policy: PromiseRejectionPolicy,
    intl_selection: &IntlDataSelection,
) -> Result<EmittedModule, EmitError> {
    emit_script_module_with_factory(
        script,
        kind,
        promise_rejection_policy,
        intl_selection,
        None,
        None,
    )
}

/// Force a fresh P against the supplied R; ordinary emit() is never consulted.
pub(crate) fn emit_runtime_function_module(
    program: &scriptw_ir::RuntimeFunctionProgram,
    validated: &crate::ValidatedRuntimeArtifact,
) -> Result<EmittedModule, EmitError> {
    let script = super::native_program::prepare_native_program_script(program.program())?;
    emit_script_module_with_factory(
        &script,
        ModuleKind::Program(validated.runtime()),
        PromiseRejectionPolicy::default(),
        validated.selection(),
        Some(NativeProgramFactory::Function(program)),
        Some(validated),
    )
}

/// Only the actual layout and validated native source witness mint this plan.
/// Callers cannot choose an independent initializer index or factory slot.
pub(super) struct ProgramExports {
    main: Option<u32>,
    initialize: Option<ProgramInitializeIndex>,
    factory: Option<(u32, &'static str)>,
    owns_globals: bool,
}

impl ProgramExports {
    pub(super) fn debug_entry(&self) -> &'static str {
        if self.factory.is_some() {
            "export funcs: program initializer and native source factory"
        } else if self.main.is_some() {
            "export func: main -> tag/scalar/reference/kind/target"
        } else {
            "export funcs: shared runtime"
        }
    }

    pub(super) fn new(
        kind: ModuleKind<'_>,
        layout: &FunctionIndexLayout,
        factory: Option<NativeProgramFactory<'_>>,
        functions: &FunctionMetaRegistry,
    ) -> Result<Self, EmitError> {
        if matches!(kind, ModuleKind::Program(_)) != layout.program_initialize_index().is_some() {
            return Err(EmitError::unsupported(
                "program exports require their actual typed initializer layout",
            ));
        }
        let factory = if let Some(program) = factory {
            if !matches!(kind, ModuleKind::Program(_)) {
                return Err(EmitError::unsupported(
                    "native source factory requires its actual linked P",
                ));
            }
            Some(program.resolve(functions, layout)?)
        } else {
            None
        };
        Ok(Self {
            main: layout.main_index(),
            initialize: layout.program_initialize_index(),
            factory,
            owns_globals: kind.owns_globals(),
        })
    }

    pub(super) fn append(
        &self,
        schema: &RuntimeSchema,
        global_prelude: Option<crate::gc_types::GlobalScriptPreludePhase>,
        has_module_entry: bool,
        helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
        exports: &mut ExportSection,
    ) {
        if let Some(main) = self.main {
            if let Some(phase) = global_prelude {
                phase.export(exports);
            }
            exports.export("main", ExportKind::Func, main);
            if has_module_entry {
                exports.export(
                    MODULE_EVALUATION_STATUS_EXPORT,
                    ExportKind::Global,
                    MODULE_EVALUATION_STATUS_GLOBAL_INDEX,
                );
            }
        }
        if let Some(index) = self.initialize {
            exports.export(
                crate::runtime_function::PROGRAM_INITIALIZE_EXPORT,
                ExportKind::Func,
                index.raw(),
            );
        }
        if let Some((index, name)) = self.factory {
            exports.export(name, ExportKind::Func, index);
        }
        if let Some(base) = helper_base {
            use crate::embedding_bridge::*;
            // These names expose checked semantic operations, never raw
            // source function indices or mutable compiler ownership state.
            for (name, helper) in [
                (EMBEDDING_CALL, RuntimeHelperId::EmbeddingCall),
                (EMBEDDING_READ, RuntimeHelperId::EmbeddingRead),
                (EMBEDDING_STRING, RuntimeHelperId::EmbeddingString),
                (EMBEDDING_BYTES, RuntimeHelperId::EmbeddingBytes),
                (EMBEDDING_BYTES_READ, RuntimeHelperId::EmbeddingBytesRead),
                (EMBEDDING_PROMISE, RuntimeHelperId::EmbeddingPromise),
                (EMBEDDING_CANCEL, RuntimeHelperId::EmbeddingCancel),
            ] {
                exports.export(name, ExportKind::Func, helper.index(base));
            }
            if self.main.is_some() {
                exports.export(
                    EMBEDDING_CHECKPOINT,
                    ExportKind::Func,
                    RuntimeHelperId::EmbeddingCheckpoint.index(base),
                );
                if has_module_entry {
                    exports.export(
                        EMBEDDING_EXPORT,
                        ExportKind::Func,
                        RuntimeHelperId::EmbeddingExport.index(base),
                    );
                }
            }
        }
        // Diagnostics/snapshot roots remain in the instance owning the globals.
        if self.owns_globals {
            schema.export_throw_diagnostics(exports);
            schema.export_snapshot_roots(exports);
        }
    }
}
