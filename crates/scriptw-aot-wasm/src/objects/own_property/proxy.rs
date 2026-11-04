//! Proxy [[GetOwnProperty]] preserves trap/target/conversion step order.
use super::*;
use crate::emit::AccessorThrowRouting;

impl FunctionBuilder<'_> {
    pub(super) fn emit_proxy_get_own_property_descriptor(
        &mut self,
        slots: &ProxySlotLocals,
        key: &PropertyKeyLocals,
        descriptor: &GcLocal<PropertyDescriptor, Nullable>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let trap = schema.reserve_value_local(function);
        let trap_result = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let name = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("getOwnPropertyDescriptor", function)?,
            function,
        );
        let method_key = PropertyKeyLocals::from_string(schema, &name, function);
        self.emit_object_read_with_throw_routing(
            slots.handler(),
            slots.handler(),
            &method_key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        method_key.clear(function);
        name.clear(function);
        self.emit_gpd_abrupt_exit(&pending, result, exit, function);
        trap.copy_from(pending.value(), function);
        trap.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Eq);
        trap.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        let returned = self.emit_object_get_own_property(slots.target(), key, result, function)?;
        descriptor.replace(returned.load(schema, function), function);
        returned.clear(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_callable_i32(&trap, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.emit_gpd_error_if(
            RuntimeErrorMessage::PROXY_GETOWNPROPERTYDESCRIPTOR_TRAP_IS_NOT_CALLABLE,
            result,
            exit,
            function,
        )?;
        let arguments =
            self.emit_pre_evaluated_arg_vector(&[slots.target(), key.value()], function);
        self.emit_function_or_proxy_call_with_argv(
            &trap,
            slots.handler(),
            &arguments,
            &pending,
            function,
        )?;
        arguments.clear(function);
        self.emit_gpd_abrupt_exit(&pending, result, exit, function);
        trap_result.copy_from(pending.value(), function);
        self.emit_is_heap_object_like_tag_i32(trap_result.tag(), function);
        trap_result.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        self.emit_gpd_error_if(
            RuntimeErrorMessage::PROXY_GETOWNPROPERTYDESCRIPTOR_TRAP_RESULT_MUST_BE_OBJECT_OR_UNDEFINED,
            result, exit, function,
        )?;
        // Target GetOwnProperty occurs before target extensibility and before
        // any observable ToPropertyDescriptor reads of the trap result.
        let target_descriptor =
            self.emit_proxy_target_own_descriptor(slots.target(), key, function)?;
        let extensible = schema.reserve_i32_local(function);
        let valid = schema.reserve_i32_local(function);
        trap_result.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        target_descriptor.emit_found_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        target_descriptor.emit_configurable_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_gpd_error_if(
            RuntimeErrorMessage::PROXY_GETOWNPROPERTYDESCRIPTOR_TRAP_RETURNED_UNDEFINED_FOR_NON_CONFIGURABLE_TARGET_PROPERTY,
            result, exit, function,
        )?;
        self.emit_object_is_extensible(slots.target(), &pending, function)?;
        self.emit_gpd_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.emit_gpd_error_if(
            RuntimeErrorMessage::PROXY_GETOWNPROPERTYDESCRIPTOR_TRAP_RETURNED_UNDEFINED_FOR_NON_EXTENSIBLE_TARGET,
            result, exit, function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // An absent target descriptor returns Undefined without IsExtensible.
        result.initialize(function);
        function.instruction(&Instruction::Else);
        self.emit_object_is_extensible(slots.target(), &pending, function)?;
        self.emit_gpd_abrupt_exit(&pending, result, exit, function);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        extensible.store(function);
        let converted = self.emit_to_property_descriptor(
            &trap_result,
            RuntimeErrorMessage::PROXY_GETOWNPROPERTYDESCRIPTOR_TRAP_RESULT_MUST_BE_OBJECT_OR_UNDEFINED,
            function,
        )?;
        let completed = self.emit_complete_property_descriptor(converted, function);
        target_descriptor.emit_found_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        extensible.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.emit_gpd_error_if(
            RuntimeErrorMessage::PROXY_GETOWNPROPERTYDESCRIPTOR_TRAP_RESULT_INCOMPATIBLE_WITH_NON_EXTENSIBLE_TARGET,
            result, exit, function,
        )?;
        target_descriptor.emit_found_i32(schema, function);
        target_descriptor.emit_configurable_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        completed.emit_configurable_i32(function);
        function.instruction(&Instruction::I32And);
        self.emit_gpd_error_if(
            RuntimeErrorMessage::PROXY_GETOWNPROPERTYDESCRIPTOR_TRAP_RESULT_CANNOT_REPORT_CONFIGURABLE_FOR_NON_CONFIGURABLE_TARGET_PROPERTY,
            result, exit, function,
        )?;
        // The same compatibility kernel is used by ordinary DefineOwnProperty.
        target_descriptor.emit_compatibility(
            &completed.object_fields().from_runtime_checked(),
            extensible,
            valid,
            self,
            function,
        )?;
        valid.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_gpd_error_if(
            RuntimeErrorMessage::PROXY_GETOWNPROPERTYDESCRIPTOR_TRAP_RESULT_IS_INCOMPATIBLE_WITH_TARGET_PROPERTY,
            result, exit, function,
        )?;
        completed.emit_configurable_i32(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        target_descriptor.emit_found_i32(schema, function);
        function.instruction(&Instruction::I32Eqz);
        target_descriptor.emit_configurable_i32(schema, function);
        function.instruction(&Instruction::I32Or);
        self.emit_gpd_error_if(
            RuntimeErrorMessage::PROXY_GETOWNPROPERTYDESCRIPTOR_TRAP_RESULT_CANNOT_REPORT_NON_CONFIGURABLE_TARGET_PROPERTY,
            result, exit, function,
        )?;
        completed.emit_accessor_i32(function);
        function.instruction(&Instruction::I32Eqz);
        completed.emit_writable_i32(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        target_descriptor.emit_writable_i32(schema, function);
        function.instruction(&Instruction::I32And);
        self.emit_gpd_error_if(
            RuntimeErrorMessage::PROXY_GETOWNPROPERTYDESCRIPTOR_TRAP_RESULT_CANNOT_REPORT_NON_WRITABLE_TARGET_PROPERTY,
            result, exit, function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let absent = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize_null(schema, function);
        let record = self.emit_merge_property_descriptor(
            &absent,
            &completed.object_fields().from_runtime_checked(),
            function,
        )?;
        descriptor.replace(record.load(schema, function).nullable(), function);
        record.clear(function);
        absent.clear(function);
        result.initialize(function);
        completed.clear(schema, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(valid, function);
        schema.release_i32_local(extensible, function);
        target_descriptor.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        trap_result.clear(function);
        trap.clear(function);
        Ok(())
    }

    fn emit_gpd_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        result: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        result.copy_from(pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    fn emit_gpd_error_if(
        &mut self,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            message,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }
}
