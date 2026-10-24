use super::*;
use crate::functions::ArgumentListConstruction;

/// Message and cause processing is complete before IteratorToList begins.
#[must_use]
pub(super) struct PreparedAggregateErrorLocal(constructor::PreparedNativeErrorInstance);

impl FunctionBuilder<'_> {
    pub(super) fn emit_prepare_aggregate_error_instance(
        &mut self,
        prototype: &ValueLocals,
        message: &ValueLocals,
        function: &mut Function,
    ) -> Result<PreparedAggregateErrorLocal, EmitError> {
        let instance = self.emit_prepare_native_error_instance(
            prototype,
            NativeErrorKind::AggregateError,
            function,
        )?;
        let message =
            self.emit_install_optional_error_message(instance.header(), message, function)?;
        self.emit_install_error_cause_from_arg(
            instance.header(),
            ErrorCauseOptionsArgument::AggregateError,
            function,
        )?;
        self.emit_capture_error_stack_from_message(&instance, &message, function)?;
        message.clear(function);
        Ok(PreparedAggregateErrorLocal(instance))
    }

    pub(super) fn emit_finish_aggregate_error_instance(
        &mut self,
        prepared: PreparedAggregateErrorLocal,
        errors: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_append_error_data(prepared.0.header(), "errors", errors, function)?;
        prepared.0.publish(self, result, function);
        Ok(())
    }

    /// IteratorToList uses the checked shared GetIterator/IteratorStepValue
    /// owners. Protocol and value abrupts propagate directly, without Close.
    /// The compiler List becomes a JS Array only after the terminal step, in
    /// the called constructor's Realm independently of the chosen NewTarget.
    pub(super) fn emit_aggregate_error_iterable_to_list(
        &mut self,
        source: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcLocal<ArrayObject>, EmitError> {
        let schema = self.runtime_schema();
        let iterator =
            self.emit_get_sync_iterator(source, SyncIteratorConsumer::AggregateError, function)?;
        let list = ArgumentListConstruction::new(self, function);
        let done = schema.reserve_i32_local(function);
        let value = schema.reserve_value_local(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let repeat = self.open_frame(ControlFrameKind::Loop, function);
        self.emit_sync_iterator_step_value(&iterator, done, &value, function)?;
        done.load(function);
        self.emit_branch_if_to_target(exit, function);
        list.append(&value, schema, function);
        self.emit_branch_to_target(repeat, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        value.clear(function);
        schema.release_i32_local(done, function);
        let values = list.finish(self, function);
        let array = self.emit_array_from_argument_list(&values, function)?;
        values.clear(function);
        iterator.clear(function);
        Ok(array)
    }
}
