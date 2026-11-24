use super::super::*;
use crate::gc_types::{
    FunctionObject, FunctionObjectSchema, GcLocal, GcNullability, Nullable, StringValue,
    ValueLocals,
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
                let lazy = schema
                    .reserve_gc_local::<StringValue, Nullable>(function)
                    .initialize(
                        schema
                            .struct_type::<FunctionObject>()
                            .field(FunctionObjectSchema::TO_STRING)
                            .read(&callable, schema, function)
                            .reference(),
                        function,
                    );
                lazy.load(schema, function).is_null(function);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_resolve_function_source_text(&callable, &lazy, function)?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                let text = schema.reserve_gc_local(function).initialize(
                    lazy.load(schema, function).require_non_null(function),
                    function,
                );
                lazy.clear(function);
                argument.set_reference(&text, schema, function);
                result.set_normal(&argument, function);
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

impl FunctionBuilder<'_> {
    /// A source function's text that its defining P has not published yet:
    /// TO_STRING_LITERAL is an ordinal of that P, which the function's context
    /// retains. The P's own resolver publishes it, so every later read of the
    /// literal, through toString or source code, sees the same String.
    fn emit_resolve_function_source_text(
        &mut self,
        callable: &GcLocal<FunctionObject>,
        text: &GcLocal<StringValue, Nullable>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::gc_types::{FunctionContext, FunctionContextSchema, ProgramInstanceLocals};
        let schema = self.runtime_schema();
        let ordinal = schema.reserve_i32_local(function);
        schema
            .struct_type::<FunctionObject>()
            .field(FunctionObjectSchema::TO_STRING_LITERAL)
            .read(callable, schema, function)
            .store(ordinal, function);
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::CONTEXT)
                .read(callable, schema, function)
                .reference(),
            function,
        );
        // Only a JavaScript callable stores a lazy text, and its context
        // always captures the defining P.
        let owner = ProgramInstanceLocals::bind(
            schema,
            schema
                .struct_type::<FunctionContext>()
                .field(FunctionContextSchema::DEFINING_PROGRAM)
                .read(&context, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        context.clear(function);
        let cached = owner.cached_literal_at(schema, ordinal, function);
        text.replace(cached.load(schema, function), function);
        cached.clear(function);
        text.load(schema, function).is_null(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_owned_program_hook_dispatch::<crate::gc_types::ProgramLiteralResolveHook>(
            &owner,
            |_, hook, function| {
                let resolved = schema.call_hook(
                    hook,
                    crate::runtime_helpers::ProgramLiteralResolveArguments::new(ordinal),
                    function,
                );
                text.replace(
                    schema.helper_reference_on_stack(resolved).nullable(),
                    function,
                );
                Ok(())
            },
            // A P that stores a literal ordinal always installs its resolver.
            |_, function| {
                function.instruction(&Instruction::Unreachable);
                Ok(())
            },
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        owner.clear(function);
        schema.release_i32_local(ordinal, function);
        Ok(())
    }
}
