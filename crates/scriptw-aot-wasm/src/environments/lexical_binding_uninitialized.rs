use super::*;
use crate::runtime_helpers::{
    HelperParameters, LexicalBindingUninitializedArguments, LexicalBindingUninitializedParameters,
};

impl FunctionBuilder<'_> {
    /// Only the failing initialized-bit branch calls R. Normal reads keep their
    /// actual value words and current completion, without Realm lookup or a call.
    pub(super) fn emit_lexical_binding_uninitialized(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        // A read or write in the temporal dead zone throws.
        function.rarely_reached();
        if !self.uses_heap {
            return self.emit_environment_native_error(
                NativeErrorKind::ReferenceError,
                RuntimeErrorMessage::LEXICAL_BINDING_ACCESSED_BEFORE_INITIALIZATION,
                value,
                function,
            );
        }
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        schema
            .call_helper(
                LexicalBindingUninitializedArguments::new(&realm, self.current_environment()),
                self.runtime_helper_base()?,
                function,
            )
            .store(self.completion(), function);
        realm.clear(function);
        value.copy_from(self.completion().value(), function);
        Ok(())
    }

    pub(crate) fn compile_lexical_binding_uninitialized_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::LexicalBindingUninitialized);
        let parameters =
            self.helper_parameters::<LexicalBindingUninitializedParameters>(&mut function);
        self.push_scope();
        self.completion().initialize(&mut function);
        let pending = self.runtime_schema().reserve_completion(&mut function);
        pending.initialize(&mut function);
        self.emit_throw_runtime_error(
            NativeErrorKind::ReferenceError,
            RuntimeErrorMessage::LEXICAL_BINDING_ACCESSED_BEFORE_INITIALIZATION,
            &pending,
            &mut function,
        )?;
        self.completion().copy_from(&pending, &mut function);
        pending.clear(&mut function);
        self.pop_scope();
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        self.completion().emit(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
