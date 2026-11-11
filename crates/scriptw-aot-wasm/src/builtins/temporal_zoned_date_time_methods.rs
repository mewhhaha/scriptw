//! Zoned date-time calendar replacement and shared zoned arithmetic entry points.

use super::super::*;
use super::temporal_zone_provider::TemporalZonedAllocationInput;
use super::temporal_zoned_arithmetic::{
    TemporalZonedArithmeticOperation, TemporalZonedDifferenceOperation,
};
use crate::intrinsics::temporal::TemporalPrototypeSource;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_temporal_zoned_date_time_with_calendar(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let branded = self.emit_temporal_branded_zoned_receiver(function)?;
        self.emit_builtin_arg_to_value(0, &argument, function);
        argument.tag().load(function);
        function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_CALENDAR_MUST_BE_A_STRING,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let calendar = self.emit_temporal_calendar_slot_from_value(&argument, function)?;
        let instant =
            self.emit_temporal_normalized_instant_from_zoned_record(&branded, function)?;
        let zone = self.emit_temporal_zone_from_zoned_record(&branded, function)?;
        self.emit_alloc_temporal_zoned_date_time(
            TemporalZonedAllocationInput::new(&instant, &zone, &calendar),
            TemporalPrototypeSource::Intrinsic,
            function,
        )?;
        zone.release(self, function);
        instant.release(self, function);
        calendar.release(self, function);
        branded.release(function);
        argument.clear(function);
        Ok(())
    }

    pub(super) fn emit_temporal_zoned_date_time_add_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_zoned_date_time_add_or_subtract(
            TemporalZonedArithmeticOperation::Add,
            function,
        )
    }

    pub(super) fn emit_temporal_zoned_date_time_subtract_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_zoned_date_time_add_or_subtract(
            TemporalZonedArithmeticOperation::Subtract,
            function,
        )
    }

    fn emit_temporal_zoned_date_time_add_or_subtract(
        &mut self,
        operation: TemporalZonedArithmeticOperation,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let duration = schema.reserve_value_local(function);
        let options = schema.reserve_value_local(function);
        let branded = self.emit_temporal_branded_zoned_receiver(function)?;
        self.emit_builtin_arg_to_value(0, &duration, function);
        self.emit_builtin_arg_to_value(1, &options, function);
        self.emit_temporal_add_duration_to_zoned_date_time(
            &branded, operation, &duration, &options, function,
        )?;
        branded.release(function);
        options.clear(function);
        duration.clear(function);
        Ok(())
    }

    pub(super) fn emit_temporal_zoned_date_time_until_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_zoned_date_time_until_or_since(
            TemporalZonedDifferenceOperation::Until,
            function,
        )
    }

    pub(super) fn emit_temporal_zoned_date_time_since_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_zoned_date_time_until_or_since(
            TemporalZonedDifferenceOperation::Since,
            function,
        )
    }

    fn emit_temporal_zoned_date_time_until_or_since(
        &mut self,
        operation: TemporalZonedDifferenceOperation,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let other = schema.reserve_value_local(function);
        let options = schema.reserve_value_local(function);
        let branded = self.emit_temporal_branded_zoned_receiver(function)?;
        self.emit_builtin_arg_to_value(0, &other, function);
        self.emit_builtin_arg_to_value(1, &options, function);
        self.emit_temporal_difference_zoned_date_time(
            &branded, operation, &other, &options, function,
        )?;
        branded.release(function);
        options.clear(function);
        other.clear(function);
        Ok(())
    }
}
