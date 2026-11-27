use super::*;
use crate::gc_types::I32Local;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_object_has_property_i32(
        &mut self,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        output: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectHasPropertyArguments::new(
                    object,
                    key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        function.instruction(&Instruction::Else);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        output.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    pub(crate) fn compile_object_has_property_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ObjectHasProperty);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::ObjectHasPropertyParameters>(
                &mut function,
            );
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_has_property_dispatch(
            &parameters.target,
            &parameters.key,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_has_property_dispatch(
        &mut self,
        object: &ValueLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let boolean = schema.reserve_i32_local(function);
        let pending = schema.reserve_completion(function);
        let value = schema.reserve_value_local(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(object.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::RIGHT_HAND_SIDE_OF_IN_IS_NOT_AN_OBJECT,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ProxyObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            object.cast_reference::<crate::gc_types::ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |builder, slots, function| builder.emit_proxy_has(slots, key, result, function),
        )?;
        proxy.clear(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_module_namespace_i32(object, function);
        self.open_frame(ControlFrameKind::If, function);
        let namespace = schema.reserve_gc_local(function).initialize(
            object.cast_reference::<crate::gc_types::ModuleNamespaceObject>(schema, function),
            function,
        );
        self.emit_namespace_property(
            &namespace,
            key,
            NamespaceBindingRead::Presence,
            boolean,
            &pending,
            function,
        )?;
        namespace.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let enumerable = schema.reserve_i32_local(function);
        self.emit_own_property_enumerability(object, key, boolean, enumerable, function)?;
        schema.release_i32_local(enumerable, function);
        boolean.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        object.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::TypedArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        self.emit_canonical_numeric_property_key_i32(key, function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_ordinary_get_prototype_of(object, &value, function);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        function.instruction(&Instruction::Else);
        // OrdinaryHasProperty returns parent.[[HasProperty]](P): a tail call,
        // so a prototype chain of any length runs in one helper frame instead
        // of exhausting the Wasm stack.
        schema.return_call_helper(
            crate::runtime_helpers::ObjectHasPropertyArguments::new(
                &value,
                key,
                self.current_environment(),
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        value.clear(function);
        pending.clear(function);
        schema.release_i32_local(boolean, function);
        Ok(())
    }

    fn emit_proxy_has(
        &mut self,
        slots: &ProxySlotLocals,
        key: &PropertyKeyLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let method = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        result.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_proxy_named_method(slots, "has", &method, &pending, result, exit, function)?;
        self.compile_nullish_tagged_i32(method.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectHasPropertyArguments::new(
                    slots.target(),
                    key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_proxy_method_callable_check(
            &method,
            RuntimeErrorMessage::PROXY_HAS_TRAP_IS_NOT_CALLABLE,
            result,
            exit,
            function,
        )?;
        let arguments =
            self.emit_pre_evaluated_arg_vector(&[slots.target(), key.value()], function);
        self.emit_function_handle_call_with_argv_inner(
            &method,
            Some(slots.handler()),
            &arguments,
            &pending,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        arguments.clear(function);
        self.emit_object_operation_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        boolean.store(function);
        boolean.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let found = schema.reserve_i32_local(function);
        let configurable = schema.reserve_i32_local(function);
        self.emit_own_property_configurability(slots.target(), key, found, configurable, function)?;
        found.load(function);
        self.open_frame(ControlFrameKind::If, function);
        configurable.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_HAS_TRAP_RETURNED_FALSE_FOR_NON_CONFIGURABLE_TARGET_PROPERTY,
            result,
            function,
        )?;
        function.instruction(&Instruction::Else);
        schema
            .call_helper(
                crate::runtime_helpers::ObjectIsExtensibleArguments::new(
                    slots.target(),
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(&pending, function);
        function.instruction(&Instruction::Else);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::PROXY_HAS_TRAP_RETURNED_FALSE_FOR_NON_EXTENSIBLE_TARGET_PROPERTY,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(configurable, function);
        schema.release_i32_local(found, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_object_boolean_result(boolean, result, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(boolean, function);
        pending.clear(function);
        method.clear(function);
        Ok(())
    }
}
