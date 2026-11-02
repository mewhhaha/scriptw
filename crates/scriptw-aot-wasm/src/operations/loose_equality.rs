//! One abstract-equality kernel owns coercion and its complete abrupt result.
use super::*;
use crate::runtime_helpers::{HelperParameters, LooseEqualityArguments, LooseEqualityParameters};

impl FunctionBuilder<'_> {
    pub(crate) fn emit_loose_tagged_equality_i32(
        &mut self,
        left: &ValueLocals,
        right: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        let realm = self.emit_execution_realm(function);
        schema
            .call_helper(
                LooseEqualityArguments::new(
                    left,
                    right,
                    &realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        realm.clear(function);
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ActiveHandler,
            &pending,
            function,
        )?;
        pending.value().scalar().load(function);
        function.instruction(&Instruction::I32WrapI64);
        pending.clear(function);
        Ok(())
    }

    pub(crate) fn compile_loose_equality_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::LooseEquality);
        let parameters = self.helper_parameters::<LooseEqualityParameters>(&mut function);
        self.push_scope();
        self.completion().initialize(&mut function);
        let schema = self.runtime_schema();
        let predicate = schema.reserve_i32_local(&mut function);
        self.emit_loose_tagged_equality_kernel_i32(
            &parameters.left,
            &parameters.right,
            &mut function,
        )?;
        predicate.store(&mut function);
        let value = schema.reserve_value_local(&mut function);
        value.set_boolean(predicate, &mut function);
        self.completion().set_normal(&value, &mut function);
        value.clear(&mut function);
        schema.release_i32_local(predicate, &mut function);
        self.pop_scope();
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        self.completion().emit(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
