use super::super::*;
use crate::gc_types::{
    FunctionObject, FunctionObjectSchema, GcNullability, GcOperand, ValueLocals,
};

mod constructor;
mod dynamic_constructor;
pub(crate) use dynamic_constructor::{
    append_empty_dynamic_function_bodies, is_empty_dynamic_function_body,
    is_empty_dynamic_function_id,
};

enum FunctionBuiltin {
    Constructor,
    Prototype,
    PrototypeSymbolHasInstance,
    PrototypeCall,
    PrototypeApply,
    PrototypeBind,
    PrototypeToString,
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_reject_dynamic_source(
        &self,
        operation: scriptw_ir::DynamicSourceRuntimeOperation,
        function: &mut Function,
    ) {
        self.emit_reject_runtime_semantics(
            scriptw_ir::RuntimeSemanticRejection::DynamicSource(operation),
            function,
        );
    }

    pub(super) fn emit_function_constructor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_function_builtin(FunctionBuiltin::Constructor, function)
    }
    pub(super) fn emit_function_prototype_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_function_builtin(FunctionBuiltin::Prototype, function)
    }
    pub(super) fn emit_function_prototype_symbol_has_instance_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_function_builtin(FunctionBuiltin::PrototypeSymbolHasInstance, function)
    }
    pub(super) fn emit_function_prototype_call_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_function_builtin(FunctionBuiltin::PrototypeCall, function)
    }
    pub(super) fn emit_function_prototype_apply_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_function_builtin(FunctionBuiltin::PrototypeApply, function)
    }
    pub(super) fn emit_function_prototype_bind_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_function_builtin(FunctionBuiltin::PrototypeBind, function)
    }
    pub(super) fn emit_function_prototype_to_string_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_function_builtin(FunctionBuiltin::PrototypeToString, function)
    }

    fn emit_function_builtin(
        &mut self,
        builtin: FunctionBuiltin,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if matches!(builtin, FunctionBuiltin::Constructor) {
            return self.compile_function_constructor_builtin(function);
        }
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        receiver.copy_from(
            self.body_entry_locals()
                .expect("native Function entry is cached")
                .this_value(),
            function,
        );
        let argument = schema.reserve_value_local(function);
        let result = schema.reserve_completion(function);
        argument.set_undefined(function);
        result.initialize(function);
        match builtin {
            FunctionBuiltin::Constructor => {
                unreachable!("constructor returned before receiver acquisition")
            }
            FunctionBuiltin::Prototype => result.set_normal(&argument, function),
            FunctionBuiltin::PrototypeSymbolHasInstance => {
                self.emit_builtin_arg_to_value(0, &argument, function);
                self.emit_ordinary_has_instance_from_locals(
                    &receiver, &argument, &result, function,
                )?;
            }
            FunctionBuiltin::PrototypeCall => {
                self.emit_builtin_arg_to_value(0, &argument, function);
                self.emit_is_callable_i32(&receiver, function)?;
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_throw_current_function_realm_type_error(
                    RuntimeErrorMessage::VALUE_IS_NOT_CALLABLE,
                    &result,
                    function,
                )?;
                function.instruction(&Instruction::Else);
                let arguments = self.emit_builtin_argument_vector_tail(1, function);
                self.emit_prepared_tail_call(&receiver, &argument, &arguments, function)?;
                arguments.clear(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            FunctionBuiltin::PrototypeApply => {
                let apply_input = schema.reserve_value_local(function);
                self.emit_builtin_arg_to_value(0, &argument, function);
                self.emit_builtin_arg_to_value(1, &apply_input, function);
                self.emit_is_callable_i32(&receiver, function)?;
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_throw_current_function_realm_type_error(
                    RuntimeErrorMessage::FUNCTION_PROTOTYPE_APPLY_RECEIVER_IS_NOT_CALLABLE,
                    &result,
                    function,
                )?;
                function.instruction(&Instruction::Else);
                self.compile_nullish_tagged_i32(apply_input.tag(), function)?;
                self.open_frame(ControlFrameKind::If, function);
                let empty = self.emit_pre_evaluated_arg_vector(&[], function);
                self.emit_prepared_tail_call(&receiver, &argument, &empty, function)?;
                empty.clear(function);
                function.instruction(&Instruction::Else);
                self.emit_with_array_like_argument_vector(
                    &apply_input,
                    RuntimeErrorMessage::FUNCTION_PROTOTYPE_APPLY_ARGUMENT_LIST_MUST_BE_ARRAY_LIKE,
                    &result,
                    function,
                    |builder, arguments, _result, function| {
                        builder.emit_prepared_tail_call(&receiver, &argument, arguments, function)
                    },
                )?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                apply_input.clear(function);
            }
            FunctionBuiltin::PrototypeBind => {
                let arguments = self.emit_builtin_argument_vector_tail(1, function);
                self.emit_alloc_bound_function_for_bind(&receiver, &arguments, &result, function)?;
                arguments.clear(function);
            }
            FunctionBuiltin::PrototypeToString => {
                receiver.reference().load(function);
                function.instruction(&Instruction::RefTestNonNull(
                    schema
                        .reference_type::<FunctionObject>(GcNullability::NonNullable)
                        .heap_type,
                ));
                self.open_frame(ControlFrameKind::If, function);
                let callable = schema.reserve_gc_local(function).initialize(
                    receiver.cast_reference::<FunctionObject>(schema, function),
                    function,
                );
                let text = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<FunctionObject>()
                        .field(FunctionObjectSchema::TO_STRING)
                        .read(&callable, schema, function)
                        .reference(),
                    function,
                );
                text.load(schema, function).is_null(function);
                self.open_frame(ControlFrameKind::If, function);
                let ordinal = schema.reserve_i32_local(function);
                schema
                    .field(FunctionObjectSchema::SOURCE_ORDINAL)
                    .read(&callable, schema, function)
                    .store(ordinal, function);
                let resolved = schema.reserve_gc_local(function).initialize(
                    schema.resolve_pooled_string_reference(
                        ordinal,
                        self.runtime_helper_base()?,
                        function,
                    ),
                    function,
                );
                schema.field(FunctionObjectSchema::TO_STRING).write(
                    &callable,
                    GcOperand::nullable_reference(&resolved, schema),
                    schema,
                    function,
                );
                text.replace(resolved.load(schema, function).nullable(), function);
                resolved.clear(function);
                schema.release_i32_local(ordinal, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                let resolved = schema.reserve_gc_local(function).initialize(
                    text.load(schema, function).require_non_null(function),
                    function,
                );
                argument.set_reference(&resolved, schema, function);
                result.set_normal(&argument, function);
                resolved.clear(function);
                text.clear(function);
                callable.clear(function);
                function.instruction(&Instruction::Else);
                self.emit_is_callable_i32(&receiver, function)?;
                self.open_frame(ControlFrameKind::If, function);
                let text = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference("function () { [native code] }", function)?,
                    function,
                );
                argument.set_reference(&text, schema, function);
                result.set_normal(&argument, function);
                text.clear(function);
                function.instruction(&Instruction::Else);
                self.emit_throw_current_function_realm_type_error(
                    RuntimeErrorMessage::FUNCTION_PROTOTYPE_TOSTRING_RECEIVER_IS_NOT_CALLABLE,
                    &result,
                    function,
                )?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        self.completion().copy_from(&result, function);
        result.clear(function);
        argument.clear(function);
        receiver.clear(function);
        Ok(())
    }
}
