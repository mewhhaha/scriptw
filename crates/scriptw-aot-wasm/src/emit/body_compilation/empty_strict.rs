use super::*;
use crate::arguments_protocol::EmptyStrictBody;

impl FunctionBuilder<'_> {
    pub(super) fn compile_empty_strict_body(
        &mut self,
        _plan: EmptyStrictBody,
    ) -> Result<Function, EmitError> {
        // take_owned_body retains the already materialized callable ABI checks.
        // The admission proof excludes parameter effects, closures and all body
        // statements, so no observer can reach an allocated binding environment.
        let mut function = self.take_owned_body();
        self.emit_profile_body_entry(&mut function)?;
        self.push_scope();
        self.init_template_source_execution(&mut function)?;
        // Strict bodies still own the source-depth guard and the active-callable
        // barrier; every completion uses their normal restoration and root exit.
        self.emit_enter_source_activation(&mut function)?;
        let epilogue = self.open_source_activation_epilogue(&mut function);
        self.completion
            .set_kind(CompletionKind::Normal, &mut function);
        self.completion.value().set_undefined(&mut function);
        self.pop_scope();
        self.finish_source_activation_epilogue(epilogue, &mut function);
        self.completion.emit(&mut function);
        self.clear_body_entry_roots(&mut function);
        if let Some(execution) = self.template_source_execution.take() {
            execution.clear(&mut function);
        }
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
