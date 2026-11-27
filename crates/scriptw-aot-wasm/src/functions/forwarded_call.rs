//! `F.call(thisArg, ...args)` syntax only selects a candidate. When the actual
//! callee is the %Function.prototype.call% builtin of any Realm, its algorithm
//! is Call(F, thisArg, args) after IsCallable(F). A published local entry of F
//! proves F callable and permits direct entry. The actual hasOwnProperty
//! builtin can also answer from a live compact key witness. Other cases keep
//! the complete dispatch with the same once-evaluated operands.
use super::*;
use crate::emit::ControlTarget;
use crate::gc_types::{
    ExecutableCodeKind, ExecutableCodeSchema, FunctionObject, FunctionObjectSchema, GcI32Constant,
    ScalarValue, ValueLocals,
};

impl FunctionBuilder<'_> {
    /// A real FunctionObject's immutable code identifies the standard
    /// builtin across Realms. Proxies, wrappers and source functions fail.
    fn emit_forwarded_builtin_or_branch(
        &self,
        value: &ValueLocals,
        builtin: StandardBuiltinId,
        miss: ControlTarget,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(miss, function);
        let callable = schema.reserve_non_null_gc_local(function).initialize(
            value.sealed_reference_or_branch::<FunctionObject>(
                schema,
                self.local_branch_label(miss),
                function,
            ),
            function,
        );
        let code = schema.reserve_non_null_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        callable.clear(function);
        schema
            .field(ExecutableCodeSchema::KIND)
            .read(&code, schema, function);
        function.instruction(&Instruction::I32Const(
            ExecutableCodeKind::StandardBuiltin.encode(),
        ));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(miss, function);
        schema
            .field(ExecutableCodeSchema::STANDARD_BUILTIN)
            .read(&code, schema, function);
        function.instruction(&Instruction::I32Const(builtin.encode()));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(miss, function);
        code.clear(function);
    }

    pub(super) fn try_emit_function_call_forwarding(
        &mut self,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        // The forwarded list omits thisArg and must fit a local entry.
        if arguments.is_empty()
            || arguments.len() > 6
            || arguments.iter().any(|argument| {
                matches!(
                    argument.expr,
                    ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
                )
            })
        {
            return Ok(false);
        }
        let schema = self.runtime_schema();
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = schema.reserve_value_local(function);
            self.compile_expr_to_value(argument, &value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            // A later operand can call, allocate, or replace the source binding.
            values.push(value);
        }
        let references: Vec<_> = values.iter().collect();
        let result = schema.reserve_completion(function);
        let handled = schema.reserve_i32_local(function);
        handled.set_constant(0, function);
        let dispatched = self.open_frame(ControlFrameKind::Block, function);
        let ordinary_call = self.open_frame(ControlFrameKind::Block, function);
        self.emit_forwarded_builtin_or_branch(
            callee,
            StandardBuiltinId::FunctionPrototypeCall,
            ordinary_call,
            function,
        );
        if let [object, key] = references.as_slice() {
            if arguments[1].possible_kinds.contains(ValueKind::String) {
                let forward = self.open_frame(ControlFrameKind::Block, function);
                self.emit_forwarded_builtin_or_branch(
                    receiver,
                    StandardBuiltinId::ObjectPrototypeHasOwnProperty,
                    forward,
                    function,
                );
                schema.emit_hinted_compact_own_key_from_values_or_branch(
                    object,
                    key,
                    self.local_branch_label(forward),
                    function,
                );
                result
                    .value()
                    .set_scalar(ScalarValue::Boolean(true), function);
                result.set_kind(CompletionKind::Normal, function);
                result.target().set_constant(0, function);
                handled.set_constant(1, function);
                self.emit_branch_to_target(dispatched, function);
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
            }
        }
        let entered = self.emit_local_argument_call_values_for_count(
            receiver,
            references[0],
            &references[1..],
            continuation,
            &result,
            function,
        )?;
        entered.load(function);
        handled.store(function);
        schema.release_i32_local(entered, function);
        self.emit_branch_to_target(dispatched, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let entered = self.emit_local_argument_call_values_for_count(
            callee,
            receiver,
            &references,
            continuation,
            &result,
            function,
        )?;
        entered.load(function);
        handled.store(function);
        schema.release_i32_local(entered, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let list = self.emit_pre_evaluated_arg_vector(&references, function);
        match continuation {
            CallContinuation::Continue => self.emit_function_or_proxy_call_with_argv(
                callee, receiver, &list, &result, function,
            )?,
            CallContinuation::Return => {
                self.emit_prepared_tail_call(callee, receiver, &list, function)?;
            }
        }
        list.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.emit_propagate_pending_throw(&result, &[], function);
        output.copy_from(result.value(), function);
        schema.release_i32_local(handled, function);
        result.clear(function);
        drop(references);
        for value in values.into_iter().rev() {
            value.clear(function);
        }
        Ok(true)
    }
}
