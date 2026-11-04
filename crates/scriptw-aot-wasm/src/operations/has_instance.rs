use super::*;
use crate::emit::ControlTarget;
use crate::gc_types::{
    BoundFunction, BoundFunctionSchema, CompletionLocals, GcNullability, StoredValue,
};
use crate::runtime_helpers::{
    HasInstanceArguments, HasInstanceParameters, HelperParameters, ObjectGetPrototypeOfArguments,
    RuntimeHelperId,
};

/// These two entry algorithms have different hook and non-callable policies.
/// Borrowed whole values preserve the specification roles without raw slots.
enum HasInstanceRequest<'value> {
    InstanceofOperator {
        object: &'value ValueLocals,
        constructor: &'value ValueLocals,
    },
    OrdinaryHasInstance {
        constructor: &'value ValueLocals,
        object: &'value ValueLocals,
    },
}

enum HasInstanceRuntimeState {
    InstanceofOperator,
    OrdinaryHasInstance,
}
impl HasInstanceRuntimeState {
    const fn runtime_code(&self) -> i32 {
        match self {
            Self::InstanceofOperator => 0,
            Self::OrdinaryHasInstance => 1,
        }
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_instanceof_i32(
        &mut self,
        left: &TypedExpr,
        right: &TypedExpr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let object = schema.reserve_value_local(function);
        let constructor = schema.reserve_value_local(function);
        let result = schema.reserve_completion(function);
        self.compile_expr_to_value(left, &object, function)?;
        self.compile_expr_to_value(right, &constructor, function)?;
        self.emit_instanceof_operator_from_locals(&object, &constructor, &result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&result, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.value().scalar().load(function);
        function.instruction(&Instruction::I32WrapI64);
        result.clear(function);
        constructor.clear(function);
        object.clear(function);
        Ok(())
    }

    pub(crate) fn emit_instanceof_operator_from_locals(
        &mut self,
        object: &ValueLocals,
        constructor: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_has_instance_request(
            HasInstanceRequest::InstanceofOperator {
                object,
                constructor,
            },
            result,
            function,
        )
    }

    pub(crate) fn emit_ordinary_has_instance_from_locals(
        &mut self,
        constructor: &ValueLocals,
        object: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_has_instance_request(
            HasInstanceRequest::OrdinaryHasInstance {
                constructor,
                object,
            },
            result,
            function,
        )
    }

    /// Abstract-operation errors belong to the executing context. A Script
    /// operator has no callable; the native method retains its actual callee
    /// Realm through the common error owner's FunctionContext projection.
    fn emit_has_instance_request(
        &mut self,
        request: HasInstanceRequest<'_>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let (state, constructor, object) = match request {
            HasInstanceRequest::InstanceofOperator {
                object,
                constructor,
            } => (
                HasInstanceRuntimeState::InstanceofOperator,
                constructor,
                object,
            ),
            HasInstanceRequest::OrdinaryHasInstance {
                constructor,
                object,
            } => (
                HasInstanceRuntimeState::OrdinaryHasInstance,
                constructor,
                object,
            ),
        };
        let entry = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(state.runtime_code()));
        entry.store(function);
        let realm = self.emit_execution_realm(function);
        schema
            .call_helper(
                HasInstanceArguments::new(
                    entry,
                    object,
                    constructor,
                    &realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        realm.clear(function);
        schema.release_i32_local(entry, function);
        Ok(())
    }

    pub(crate) fn compile_has_instance_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::HasInstance);
        let parameters = self.helper_parameters::<HasInstanceParameters>(&mut function);
        self.completion().initialize(&mut function);
        let result = self.runtime_schema().reserve_completion(&mut function);
        self.emit_has_instance_kernel(
            parameters.entry,
            &parameters.object,
            &parameters.constructor,
            &result,
            &mut function,
        )?;
        result.emit(&mut function);
        result.clear(&mut function);
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// One emitted state machine owns both entry modes and bound-target
    /// transitions. Its errors use the supplied execution Realm; this helper
    /// has no actual callable context of its own.
    fn emit_has_instance_kernel(
        &mut self,
        entry: crate::gc_types::I32Local,
        object: &ValueLocals,
        initial_constructor: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let state_local = schema.reserve_i32_local(function);
        let constructor = schema.reserve_value_local(function);
        let handler = schema.reserve_value_local(function);
        let prototype = schema.reserve_value_local(function);
        let search = schema.reserve_value_local(function);
        let call_result = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        constructor.copy_from(initial_constructor, function);
        entry.load(function);
        state_local.store(function);
        function.instruction(&Instruction::I32Const(0));
        boolean.store(function);
        handler.set_undefined(function);
        prototype.set_undefined(function);
        search.set_undefined(function);
        call_result.initialize(function);
        result.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let dispatch = self.open_frame(ControlFrameKind::Loop, function);
        state_local.load(function);
        function.instruction(&Instruction::I32Const(
            HasInstanceRuntimeState::InstanceofOperator.runtime_code(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_heap_object_like_tag_i32(constructor.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::RIGHT_HAND_SIDE_OF_INSTANCEOF_IS_NOT_CALLABLE,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let symbol_slot = schema.reserve_gc_local(function);
        let symbol = symbol_slot.initialize(
            self.emit_well_known_symbol_reference(
                scriptw_ir::WellKnownSymbol::HasInstance,
                function,
            )?,
            function,
        );
        let has_instance_key = PropertyKeyLocals::from_symbol(schema, &symbol, function);
        symbol.clear(function);
        self.emit_object_read_with_throw_routing(
            &constructor,
            &constructor,
            &has_instance_key,
            &call_result,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        has_instance_key.clear(function);
        self.emit_has_instance_abrupt_exit(&call_result, result, exit, function);
        handler.copy_from(call_result.value(), function);
        self.compile_nullish_tagged_i32(handler.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_is_callable_i32(&constructor, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::RIGHT_HAND_SIDE_OF_INSTANCEOF_IS_NOT_CALLABLE,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I32Const(
            HasInstanceRuntimeState::OrdinaryHasInstance.runtime_code(),
        ));
        state_local.store(function);
        self.emit_branch_to_target(dispatch, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_callable_i32(&handler, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::RIGHT_HAND_SIDE_OF_INSTANCEOF_IS_NOT_CALLABLE,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let arguments = self.emit_pre_evaluated_arg_vector(&[object], function);
        self.emit_function_handle_call_with_argv_inner(
            &handler,
            Some(&constructor),
            &arguments,
            &call_result,
            PropagateCallThrow::LeaveInCompletion,
            function,
        )?;
        arguments.clear(function);
        self.emit_has_instance_abrupt_exit(&call_result, result, exit, function);
        self.compile_truthy_tagged_i32(call_result.value(), function)?;
        boolean.store(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // The ordinary entry returns false for non-callable C. A bound target
        // re-enters the operator and observes its own @@hasInstance exactly once.
        self.emit_is_callable_i32(&constructor, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, function);
        constructor.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<BoundFunction>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let bound_slot = schema.reserve_gc_local(function);
        let bound = bound_slot.initialize(
            constructor.cast_reference::<BoundFunction>(schema, function),
            function,
        );
        let target_slot = schema.reserve_gc_local(function);
        let target = target_slot.initialize(
            schema
                .struct_type::<BoundFunction>()
                .field(BoundFunctionSchema::TARGET)
                .read(&bound, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&target, &constructor, schema, function);
        target.clear(function);
        bound.clear(function);
        function.instruction(&Instruction::I32Const(
            HasInstanceRuntimeState::InstanceofOperator.runtime_code(),
        ));
        state_local.store(function);
        self.emit_branch_to_target(dispatch, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_heap_object_like_tag_i32(object.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, function);
        let string_slot = schema.reserve_gc_local(function);
        let string = string_slot.initialize(
            self.emit_interned_string_reference("prototype", function)?,
            function,
        );
        let prototype_key = PropertyKeyLocals::from_string(schema, &string, function);
        string.clear(function);
        self.emit_object_read_with_throw_routing(
            &constructor,
            &constructor,
            &prototype_key,
            &call_result,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        prototype_key.clear(function);
        self.emit_has_instance_abrupt_exit(&call_result, result, exit, function);
        prototype.copy_from(call_result.value(), function);
        self.emit_is_heap_object_like_tag_i32(prototype.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::FUNCTION_HAS_NON_OBJECT_PROTOTYPE_IN_INSTANCEOF_CHECK,
            result,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        search.copy_from(object, function);
        let walk = self.open_frame(ControlFrameKind::Loop, function);
        let base = self.runtime_helper_base()?;
        schema
            .call_helper(
                ObjectGetPrototypeOfArguments::new(
                    &search,
                    self.current_environment(),
                    self.current_program(),
                ),
                base,
                function,
            )
            .store(&call_result, function);
        self.emit_has_instance_abrupt_exit(&call_result, result, exit, function);
        call_result.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(exit, function);
        call_result.value().reference().load(function);
        prototype.reference().load(function);
        function.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        boolean.store(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        search.copy_from(call_result.value(), function);
        self.emit_branch_to_target(walk, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        result.value().set_boolean(boolean, function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(boolean, function);
        call_result.clear(function);
        search.clear(function);
        prototype.clear(function);
        handler.clear(function);
        constructor.clear(function);
        schema.release_i32_local(state_local, function);
        Ok(())
    }

    fn emit_has_instance_abrupt_exit(
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
}
