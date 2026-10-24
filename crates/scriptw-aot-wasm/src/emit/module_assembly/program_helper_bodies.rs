//! Body selection is independent of registration order; assembly still emits
//! the one RuntimeHelperId::ALL order for stable physical indices.
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn compile_program_helper_bodies(
    schema: &RuntimeSchema,
    string_pool: &StringPool,
    function_metas: &FunctionMetaRegistry,
    uses_heap: bool,
    runtime_bootstrap_plan: &RuntimeBootstrapPlan,
    runtime_helper_base: Option<crate::runtime_helpers::RuntimeHelperFunctionBase>,
    provided_hooks: ProvidedHooks,
    promise_rejection_policy: PromiseRejectionPolicy,
    helper_bodies: &mut BTreeMap<RuntimeHelperId, Function>,
) -> Result<(), EmitError> {
    let export = if function_metas
        .module_graph()
        .and_then(|graph| graph.module_entry())
        .is_some()
    {
        let mut builder = FunctionBuilder::new_runtime_operation_helper(
            schema,
            RuntimeHelperId::EmbeddingExport,
            string_pool,
            function_metas,
            uses_heap,
            runtime_bootstrap_plan.clone(),
            runtime_helper_base,
        );
        builder.compile_embedding_export()?
    } else {
        let mut body = Function::new(LocalDeclarations::from_types([]));
        body.instruction(&Instruction::Unreachable);
        body.instruction(&Instruction::End);
        body
    };
    helper_bodies.insert(RuntimeHelperId::EmbeddingExport, export);
    let mut checkpoint = FunctionBuilder::new_runtime_operation_helper(
        schema,
        RuntimeHelperId::EmbeddingCheckpoint,
        string_pool,
        function_metas,
        uses_heap,
        runtime_bootstrap_plan.clone(),
        runtime_helper_base,
    );
    helper_bodies.insert(
        RuntimeHelperId::EmbeddingCheckpoint,
        checkpoint.compile_embedding_checkpoint(promise_rejection_policy)?,
    );

    let compact_array_body = if string_pool.has_compact_array_recipes() {
        let mut builder = FunctionBuilder::new_runtime_operation_helper(
            schema,
            RuntimeHelperId::CompactArrayLiteral,
            string_pool,
            function_metas,
            uses_heap,
            runtime_bootstrap_plan.clone(),
            runtime_helper_base,
        );
        builder.compile_compact_array_literal_helper()?
    } else {
        let mut body = Function::new(LocalDeclarations::from_types([]));
        body.instruction(&Instruction::Unreachable);
        body.instruction(&Instruction::End);
        body
    };
    helper_bodies.insert(RuntimeHelperId::CompactArrayLiteral, compact_array_body);
    for operation in crate::modules::ModuleRuntimeOperation::ALL {
        let body = if function_metas.module_graph().is_some() {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                schema,
                operation.helper(),
                string_pool,
                function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_module_runtime_operation(operation)?
        } else {
            // No trusted module operation may occur without its graph witness.
            // Keep indices stable without rooting unused async execution machinery.
            let mut body = Function::new(LocalDeclarations::from_types([]));
            body.instruction(&Instruction::Unreachable);
            body.instruction(&Instruction::End);
            body
        };
        helper_bodies.insert(operation.helper(), body);
    }
    for hook in ProgramHook::ALL {
        let body = if provided_hooks.provides(hook) {
            let mut builder = FunctionBuilder::new_runtime_operation_helper(
                schema,
                hook.helper(),
                string_pool,
                function_metas,
                uses_heap,
                runtime_bootstrap_plan.clone(),
                runtime_helper_base,
            );
            builder.compile_program_hook(hook)?
        } else {
            // No ProgramInstance field selects this absent body: keep the slot dense.
            let mut body = Function::new(LocalDeclarations::from_types([]));
            body.instruction(&Instruction::Unreachable);
            body.instruction(&Instruction::End);
            body
        };
        helper_bodies.insert(hook.helper(), body);
    }
    let mut initializer = FunctionBuilder::new_runtime_operation_helper(
        schema,
        RuntimeHelperId::InitializeInstanceElements,
        string_pool,
        function_metas,
        uses_heap,
        runtime_bootstrap_plan.clone(),
        runtime_helper_base,
    );
    helper_bodies.insert(
        RuntimeHelperId::InitializeInstanceElements,
        initializer.compile_initialize_instance_elements_hook()?,
    );
    let mut factory = FunctionBuilder::new_runtime_operation_helper(
        schema,
        RuntimeHelperId::NativeDirectEvalFactory,
        string_pool,
        function_metas,
        uses_heap,
        runtime_bootstrap_plan.clone(),
        runtime_helper_base,
    );
    helper_bodies.insert(
        RuntimeHelperId::NativeDirectEvalFactory,
        factory.compile_native_direct_eval_factory()?,
    );
    Ok(())
}
