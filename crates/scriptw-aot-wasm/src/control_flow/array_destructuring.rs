//! Ordinary and retained Array patterns share native iterator semantics.

use super::*;

impl FunctionBuilder<'_> {
    pub(super) fn compile_array_destructuring_operation_statement(
        &mut self,
        operation: &scriptw_ir::ArrayDestructuringOperationIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use scriptw_ir::ArrayDestructuringOperationView;

        let storage = self.array_iterator_storage(operation.storage())?;
        if operation.result_binding().is_some_and(|binding| {
            !self.owned_env_bindings.iter().any(|owned| owned == binding)
                || self.owned_env_slot(&binding.name) != Some(binding.slot)
        }) {
            return Err(EmitError::unsupported(
                "compiler invariant: array operation result must use its exact owned activation cell",
            ));
        }
        let schema = self.runtime_schema();
        let saved = self.save_statement_list_value(function);
        let iterator = OwnedSyncIterator {
            record: self.emit_load_retained_array_iterator(&storage, function),
            consumer: SyncIteratorConsumer::ArrayDestructuring,
        };
        let done = schema.reserve_i32_local(function);
        let value = schema.reserve_value_local(function);
        match operation.use_view() {
            ArrayDestructuringOperationView::StepValue(_) => {
                self.emit_sync_iterator_step_value(&iterator, done, &value, function)?;
            }
            ArrayDestructuringOperationView::Elision(_) => {
                self.emit_sync_iterator_step_without_value(&iterator, done, function)?;
            }
            ArrayDestructuringOperationView::RestArray(_) => {
                self.emit_array_destructuring_rest_array(&iterator, done, &value, function)?;
            }
        }
        if let Some(binding) = operation.result_binding() {
            // Reuse the actual original-invocation cell writer; never replay
            // the target's Reference lookup or a cached lexical-hop storage.
            self.write_generator_statement_list_binding(binding, &value, function);
        }
        value.clear(function);
        schema.release_i32_local(done, function);
        iterator.clear(function);
        // This compiler-private statement has an Empty normal result. Its
        // step observation or rest Array is published only in the result cell.
        self.restore_statement_list_value(saved, function)
    }

    pub(crate) fn compile_array_destructure_to_locals(
        &mut self,
        expression: &TypedExpr,
        pattern: &ArrayDestructuringPatternIr,
        evaluation: ArrayDestructuringEvaluationIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let source = self.runtime_schema().reserve_value_local(function);
        self.compile_expr_to_value(expression, &source, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        self.compile_array_destructure_from_value_locals(&source, pattern, function)?;
        match evaluation {
            ArrayDestructuringEvaluationIr::BindingInitialization => output.set_undefined(function),
            ArrayDestructuringEvaluationIr::AssignmentEvaluation => {
                output.copy_from(&source, function)
            }
        }
        source.clear(function);
        Ok(())
    }

    pub(crate) fn compile_array_destructure_from_value_locals(
        &mut self,
        source: &ValueLocals,
        pattern: &ArrayDestructuringPatternIr,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let iterator = self.emit_get_sync_iterator(
            source,
            SyncIteratorConsumer::ArrayDestructuring,
            function,
        )?;
        let done = schema.reserve_i32_local(function);
        let value = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let closed = schema.reserve_completion(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let abrupt = self.open_frame(ControlFrameKind::Block, function);
        self.finally_stack.push(abrupt);
        for element in &pattern.elements {
            self.compile_array_destructuring_element(element, &iterator, done, &value, function)?;
        }
        self.finally_stack.pop();
        pending.initialize(function);
        self.emit_sync_iterator_close(&iterator, &pending, &closed, function)?;
        self.completion().copy_from(&closed, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        // StepValue marks DONE on protocol/Get/value abrupt. Target/default/
        // PutValue failures leave it false and therefore close exactly once.
        pending.copy_from(self.completion(), function);
        self.emit_sync_iterator_close(&iterator, &pending, &closed, function)?;
        self.completion().copy_from(&closed, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        closed.clear(function);
        pending.clear(function);
        value.clear(function);
        schema.release_i32_local(done, function);
        iterator.clear(function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    fn compile_array_destructuring_element(
        &mut self,
        element: &ArrayDestructuringElementIr,
        iterator: &OwnedSyncIterator,
        done: I32Local,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match element {
            ArrayDestructuringElementIr::Elision => {
                self.emit_sync_iterator_step_without_value(iterator, done, function)?
            }
            ArrayDestructuringElementIr::Target { target, default } => {
                let prepared = self.prepare_destructuring_target(target, function)?;
                self.emit_sync_iterator_step_value(iterator, done, value, function)?;
                if let Some(default) = default {
                    value.tag().load(function);
                    function.instruction(&Instruction::I32Const(
                        WasmRuntimeValueTag::Undefined as i32,
                    ));
                    function.instruction(&Instruction::I32Eq);
                    self.open_frame(ControlFrameKind::If, function);
                    self.compile_expr_to_value(default, value, function)?;
                    self.emit_propagate_current_throw_if_needed(function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                self.put_destructuring_target(prepared, value, function)?;
            }
            ArrayDestructuringElementIr::Rest { target } => {
                let prepared = self.prepare_destructuring_target(target, function)?;
                self.emit_array_destructuring_rest_array(iterator, done, value, function)?;
                self.put_destructuring_target(prepared, value, function)?;
            }
        }
        Ok(())
    }

    fn emit_array_destructuring_rest_array(
        &mut self,
        iterator: &OwnedSyncIterator,
        done: I32Local,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let list = crate::functions::ArgumentListConstruction::new(self, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        self.emit_sync_iterator_step_value(iterator, done, value, function)?;
        done.load(function);
        self.emit_branch_if_to_target(exit, function);
        list.append(value, schema, function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let arguments = list.finish(self, function);
        let rest = self.emit_array_from_argument_list(&arguments, function)?;
        value.set_reference(&rest, schema, function);
        rest.clear(function);
        arguments.clear(function);
        Ok(())
    }
}
