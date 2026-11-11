//! Concrete Temporal receiver admission and complete native abrupt publication.
use super::*;

/// Only the eight real Temporal GC records implement this receiver contract.
/// The record type fixes its diagnostic before any field can be projected.
pub(in crate::builtins) trait TemporalReceiverRecord:
    JavaScriptReference
{
    const RECEIVER_ERROR: RuntimeErrorMessage;
}
impl TemporalReceiverRecord for TemporalInstantObject {
    const RECEIVER_ERROR: RuntimeErrorMessage =
        RuntimeErrorMessage::TEMPORAL_INSTANT_RECEIVER_DOES_NOT_HAVE_INITIALIZEDTEMPORALINSTANT;
}
impl TemporalReceiverRecord for TemporalDurationObject {
    const RECEIVER_ERROR: RuntimeErrorMessage =
        RuntimeErrorMessage::TEMPORAL_DURATION_RECEIVER_DOES_NOT_HAVE_INITIALIZEDTEMPORALDURATION;
}
impl TemporalReceiverRecord for TemporalPlainDateObject {
    const RECEIVER_ERROR: RuntimeErrorMessage =
        RuntimeErrorMessage::TEMPORAL_PLAINDATE_RECEIVER_DOES_NOT_HAVE_INITIALIZEDTEMPORALDATE;
}
impl TemporalReceiverRecord for TemporalPlainYearMonthObject {
    const RECEIVER_ERROR: RuntimeErrorMessage = RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_RECEIVER_DOES_NOT_HAVE_INITIALIZEDTEMPORALYEARMONTH;
}
impl TemporalReceiverRecord for TemporalPlainMonthDayObject {
    const RECEIVER_ERROR: RuntimeErrorMessage = RuntimeErrorMessage::TEMPORAL_PLAINMONTHDAY_RECEIVER_DOES_NOT_HAVE_INITIALIZEDTEMPORALMONTHDAY;
}
impl TemporalReceiverRecord for TemporalPlainTimeObject {
    const RECEIVER_ERROR: RuntimeErrorMessage =
        RuntimeErrorMessage::TEMPORAL_PLAINTIME_RECEIVER_DOES_NOT_HAVE_INITIALIZEDTEMPORALTIME;
}
impl TemporalReceiverRecord for TemporalPlainDateTimeObject {
    const RECEIVER_ERROR: RuntimeErrorMessage = RuntimeErrorMessage::TEMPORAL_PLAINDATETIME_RECEIVER_DOES_NOT_HAVE_INITIALIZEDTEMPORALDATETIME;
}
impl TemporalReceiverRecord for TemporalZonedDateTimeObject {
    const RECEIVER_ERROR: RuntimeErrorMessage = RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_RECEIVER_DOES_NOT_HAVE_INITIALIZEDTEMPORALZONEDDATETIME;
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_temporal_error_and_return(
        &mut self,
        kind: scriptw_ir::NativeErrorKind,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let pending = self.runtime_schema().reserve_completion(function);
        self.emit_throw_runtime_error(kind, message, &pending, function)?;
        self.completion().copy_from(&pending, function);
        pending.clear(function);
        self.emit_return_current_completion(function);
        Ok(())
    }

    pub(in crate::builtins) fn emit_temporal_require_construct_call(
        &mut self,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.body_entry_locals()
            .ok_or_else(|| EmitError::unsupported("Temporal constructor entry absent"))?
            .new_target()
            .tag()
            .load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            message,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(in crate::builtins) fn emit_temporal_record_from_receiver<T: TemporalReceiverRecord>(
        &mut self,
        function: &mut Function,
    ) -> Result<GcLocal<T>, EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        receiver.copy_from(
            self.body_entry_locals()
                .ok_or_else(|| EmitError::unsupported("Temporal receiver has no callable entry"))?
                .this_value(),
            function,
        );
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<T>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            T::RECEIVER_ERROR,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let record = schema
            .reserve_gc_local(function)
            .initialize(receiver.cast_reference::<T>(schema, function), function);
        receiver.clear(function);
        Ok(record)
    }
}
