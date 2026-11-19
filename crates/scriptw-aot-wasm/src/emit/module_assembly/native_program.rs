//! Native source witnesses select an actual owned P factory, never a raw slot.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum NativeProgramFactory<'a> {
    Function(&'a scriptw_ir::RuntimeFunctionProgram),
    GlobalScript(&'a scriptw_ir::RuntimeGlobalScriptProgram),
    DirectEval(&'a scriptw_ir::RuntimeDirectEvalProgram),
}

impl NativeProgramFactory<'_> {
    pub(super) fn resolve(
        self,
        functions: &FunctionMetaRegistry,
        layout: &FunctionIndexLayout,
    ) -> Result<(u32, &'static str), EmitError> {
        let (helper, name) = match self {
            Self::Function(program) => {
                crate::runtime_function::require_factory_plan(program, functions)?;
                (
                    RuntimeHelperId::PreparedDynamicFunction,
                    crate::runtime_function::RUNTIME_FUNCTION_FACTORY_EXPORT,
                )
            }
            Self::GlobalScript(program) => {
                crate::runtime_global_script::require_factory_plan(program, functions)?;
                (
                    RuntimeHelperId::PreparedScript,
                    crate::runtime_global_script::RUNTIME_GLOBAL_SCRIPT_FACTORY_EXPORT,
                )
            }
            Self::DirectEval(program) => {
                crate::runtime_direct_eval::require_factory_plan(program, functions)?;
                (
                    RuntimeHelperId::NativeDirectEvalFactory,
                    crate::runtime_direct_eval::RUNTIME_DIRECT_EVAL_FACTORY_EXPORT,
                )
            }
        };
        Ok((helper.index(layout.helper_base()), name))
    }
}

pub(super) fn prepare_native_program_script(
    program: &scriptw_ir::ProgramIr,
) -> Result<ScriptIr, EmitError> {
    let mut script = program
        .script
        .as_ref()
        .ok_or_else(|| EmitError::unsupported("native source has no lowered Script IR"))?
        .clone();
    crate::builtins::append_empty_dynamic_function_bodies(&mut script);
    for function in script.functions.iter().filter(|function| {
        function.protocol.execution_kind() == FunctionExecutionKind::AsyncGenerator
    }) {
        if let Some(feature) = function
            .body
            .statements
            .iter()
            .find_map(async_generator_dispatcher_unsupported_feature)
        {
            return Err(EmitError::unsupported(format!("unsupported in scriptw wasm-aot first slice: async-generator body dispatcher for `{}` does not yet support {feature}", function.name)));
        }
    }
    Ok(script)
}

/// No ordinary emitter fallback, new Realm, R rebuild or JavaScript main entry.
pub(crate) fn emit_runtime_global_script_module(
    program: &scriptw_ir::RuntimeGlobalScriptProgram,
    validated: &crate::ValidatedRuntimeArtifact,
) -> Result<EmittedModule, EmitError> {
    let script = prepare_native_program_script(program.program())?;
    emit_script_module_with_factory(
        &script,
        ModuleKind::Program(validated.runtime()),
        PromiseRejectionPolicy::default(),
        validated.selection(),
        Some(NativeProgramFactory::GlobalScript(program)),
        Some(validated),
        crate::profile::ProfileUse::None,
    )
}

/// Caller-context source has its own witness, typed factory and fresh P.
pub(crate) fn emit_runtime_direct_eval_module(
    program: &scriptw_ir::RuntimeDirectEvalProgram,
    validated: &crate::ValidatedRuntimeArtifact,
) -> Result<EmittedModule, EmitError> {
    let script = prepare_native_program_script(program.program())?;
    emit_script_module_with_factory(
        &script,
        ModuleKind::Program(validated.runtime()),
        PromiseRejectionPolicy::default(),
        validated.selection(),
        Some(NativeProgramFactory::DirectEval(program)),
        Some(validated),
        crate::profile::ProfileUse::None,
    )
}
