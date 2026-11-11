//! Zoned date-time field replacement through retained zone and offset policies.

use super::super::*;
use super::temporal::TemporalZonedDateTimeOptionsContext;
use super::temporal_options::Disambiguation;
use super::temporal_plain_date_time_methods::TemporalDateTimeFieldReadMode;
use super::temporal_zone_provider::{
    TemporalOffsetBehavior, TemporalOffsetMatchBehavior, TemporalZonedAllocationInput,
};
use crate::intrinsics::temporal::TemporalPrototypeSource;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_temporal_zoned_date_time_with(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let options_value = schema.reserve_value_local(function);
        let value = schema.reserve_value_local(function);
        let month_code = schema.reserve_value_local(function);
        let receiver_month_code = schema.reserve_value_local(function);
        let encoded_month_code = schema.reserve_i64_local(function);
        let month_code_present = schema.reserve_i64_local(function);
        let any_present = schema.reserve_i64_local(function);
        let offset_nanoseconds = schema.reserve_i64_local(function);
        let fields = self.reserve_temporal_plain_date_time_field_locals(function);
        let present = self.reserve_temporal_plain_date_time_field_locals(function);
        let branded = self.emit_temporal_branded_zoned_receiver(function)?;
        self.emit_builtin_arg_to_value(0, &argument, function);
        self.emit_builtin_arg_to_value(1, &options_value, function);
        self.emit_is_heap_object_like_tag_i32(argument.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_PROTOTYPE_WITH_REQUIRES_AN_OBJECT,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_temporal_reject_branded_partial_object(&argument,
            RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_PROTOTYPE_WITH_DOES_NOT_ACCEPT_A_TEMPORAL_OBJECT,
            function)?;
        for property in ["calendar", "timeZone"] {
            self.emit_temporal_duration_option_get(&argument, property, &value, function)?;
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
            function.instruction(&Instruction::I32Ne);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(scriptw_ir::NativeErrorKind::TypeError,
                RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_PROTOTYPE_WITH_DOES_NOT_ACCEPT_CALENDAR_OR_TIMEZONE,
                function)?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }

        let instant =
            self.emit_temporal_normalized_instant_from_zoned_record(&branded, function)?;
        let zone = self.emit_temporal_zone_from_zoned_record(&branded, function)?;
        let calendar = self.emit_temporal_calendar_from_zoned_record(&branded, function)?;
        let snapshot = self.emit_temporal_zone_snapshot(&zone, &instant, function)?;
        let iso = self.emit_temporal_zone_snapshot_iso_record(&snapshot, function)?;
        for (source, destination) in iso.fields().iter().zip(fields) {
            source.load(function);
            destination.store(function);
        }
        snapshot.offset_seconds().load(function);
        function.instruction(&Instruction::I64Const(1_000_000_000));
        function.instruction(&Instruction::I64Mul);
        offset_nanoseconds.store(function);
        let calendar_date = self.emit_temporal_project_calendar_date(
            calendar.calendar_id(),
            [fields[0], fields[1], fields[2]],
            function,
        );
        for (source, destination) in calendar_date
            .fields()
            .into_iter()
            .zip([fields[0], fields[1], fields[2]])
        {
            source.load(function);
            destination.store(function);
        }
        self.emit_temporal_calendar_month_code_payload(
            &calendar_date,
            &receiver_month_code,
            function,
        )?;
        calendar_date.release(self, function);
        iso.release(self, function);
        snapshot.release(self, function);

        // Retain option outputs before era slots, while their observable reads
        // remain after the complete partial field sweep.
        let option_slots = self.reserve_temporal_zoned_options_slots(function);
        for local in present {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }
        function.instruction(&Instruction::I64Const(0));
        month_code_present.store(function);
        function.instruction(&Instruction::I64Const(0));
        encoded_month_code.store(function);
        month_code.set_undefined(function);
        let era = self.emit_temporal_date_time_read_fields(
            &argument,
            &calendar,
            &fields,
            &present,
            &month_code,
            month_code_present,
            any_present,
            TemporalDateTimeFieldReadMode::ZonedWith {
                offset_nanoseconds_local: offset_nanoseconds,
            },
            function,
        )?;
        any_present.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_PROTOTYPE_WITH_REQUIRES_AT_LEAST_ONE_DATE_TIME_OR_OFFSET_FIELD,
            function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let options = self.emit_temporal_zoned_date_time_options_from_slots(
            TemporalZonedDateTimeOptionsContext::With,
            &options_value,
            option_slots,
            function,
        )?;
        let resolved_year = self.emit_temporal_resolve_era_to_calendar_year(
            era,
            calendar.calendar_id(),
            fields[0],
            present[0],
            function,
        )?;
        present[1].load(function);
        month_code_present.load(function);
        function.instruction(&Instruction::I64Or);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        month_code.copy_from(&receiver_month_code, function);
        function.instruction(&Instruction::I64Const(1));
        month_code_present.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for index in [0_usize, 2] {
            function.instruction(&Instruction::I64Const(1));
            present[index].store(function);
        }
        self.emit_temporal_plain_date_resolve_fields(
            resolved_year,
            fields[1],
            present[1],
            &month_code,
            encoded_month_code,
            month_code_present,
            fields[2],
            present[2],
            options.overflow().local(),
            function,
        )?;
        self.emit_temporal_regulate_time(
            &Self::temporal_plain_date_time_time_locals(&fields),
            options.overflow().local(),
            function,
        )?;
        let iso = self.emit_temporal_regulated_iso_record(&fields, function)?;
        let supplied_offset =
            self.emit_temporal_validated_offset_nanoseconds(offset_nanoseconds, function)?;
        let replacement = self.emit_temporal_interpret_iso_date_time_offset(
            &iso,
            &zone,
            TemporalOffsetBehavior::Option,
            &supplied_offset,
            options.disambiguation(),
            options.offset(),
            TemporalOffsetMatchBehavior::MatchExactly,
            function,
        )?;
        self.emit_alloc_temporal_zoned_date_time(
            TemporalZonedAllocationInput::new(&replacement, &zone, &calendar),
            TemporalPrototypeSource::Intrinsic,
            function,
        )?;
        replacement.release(self, function);
        supplied_offset.release(self, function);
        iso.release(self, function);
        options.release(self, function);
        calendar.release(self, function);
        zone.release(self, function);
        instant.release(self, function);
        branded.release(function);
        self.release_temporal_plain_date_time_field_locals(present, function);
        self.release_temporal_plain_date_time_field_locals(fields, function);
        for local in [
            offset_nanoseconds,
            any_present,
            month_code_present,
            encoded_month_code,
        ] {
            schema.release_i64_local(local, function);
        }
        receiver_month_code.clear(function);
        month_code.clear(function);
        value.clear(function);
        options_value.clear(function);
        argument.clear(function);
        Ok(())
    }

    pub(crate) fn emit_temporal_zoned_date_time_with_plain_time(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let branded = self.emit_temporal_branded_zoned_receiver(function)?;
        self.emit_builtin_arg_to_value(0, &argument, function);
        let instant =
            self.emit_temporal_normalized_instant_from_zoned_record(&branded, function)?;
        let zone = self.emit_temporal_zone_from_zoned_record(&branded, function)?;
        let calendar = self.emit_temporal_calendar_from_zoned_record(&branded, function)?;
        let snapshot = self.emit_temporal_zone_snapshot(&zone, &instant, function)?;
        let iso = self.emit_temporal_zone_snapshot_iso_record(&snapshot, function)?;
        let date = self.emit_temporal_iso_date_from_record(&iso, function);
        argument.tag().load(function);
        function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let start = self.emit_temporal_get_start_of_day(&zone, &date, function)?;
        self.emit_alloc_temporal_zoned_date_time(
            TemporalZonedAllocationInput::new(&start, &zone, &calendar),
            TemporalPrototypeSource::Intrinsic,
            function,
        )?;
        start.release(self, function);
        function.instruction(&Instruction::Else);
        let combined = self.emit_temporal_combine_iso_date_and_time(&date, &argument, function)?;
        let local = self.emit_temporal_local_coordinate_from_iso_record(&combined, function)?;
        let compatible =
            self.emit_temporal_constant_disambiguation(Disambiguation::Compatible, function);
        let replacement =
            self.emit_temporal_get_epoch_nanoseconds_for(&zone, &local, &compatible, function)?;
        self.emit_alloc_temporal_zoned_date_time(
            TemporalZonedAllocationInput::new(&replacement, &zone, &calendar),
            TemporalPrototypeSource::Intrinsic,
            function,
        )?;
        replacement.release(self, function);
        compatible.release(self, function);
        local.release(self, function);
        combined.release(self, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        date.release(self, function);
        iso.release(self, function);
        snapshot.release(self, function);
        calendar.release(self, function);
        zone.release(self, function);
        instant.release(self, function);
        branded.release(function);
        argument.clear(function);
        Ok(())
    }
}
