//! PlainDate conversion distinguishes absent time from explicit midnight.
use super::super::*;
use super::temporal::TemporalTimeZoneStringGoal;
use super::temporal_options::Disambiguation;
use super::temporal_zone_provider::TemporalZonedAllocationInput;
use crate::intrinsics::temporal::TemporalPrototypeSource;
impl FunctionBuilder<'_> {
    pub(super) fn emit_temporal_plain_date_to_zoned_date_time(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let (date, calendar) = self.emit_temporal_iso_date_from_plain_date_receiver(f)?;
        let argument = schema.reserve_value_local(f);
        let zone_value = schema.reserve_value_local(f);
        let time = schema.reserve_value_local(f);
        let has_time = schema.reserve_i32_local(f);
        time.set_undefined(f);
        f.instruction(&Instruction::I32Const(0));
        has_time.store(f);
        self.emit_builtin_arg_to_value(0, &argument, f);
        zone_value.copy_from(&argument, f);
        self.emit_is_primitive_tag_i32(argument.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_duration_option_get(&argument, "timeZone", &zone_value, f)?;
        zone_value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        zone_value.copy_from(&argument, f);
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I32Const(1));
        has_time.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Identifier conversion finishes before the optional plainTime Get.
        let zone = self.emit_temporal_zoned_date_time_time_zone(
            &zone_value,
            TemporalTimeZoneStringGoal::Object,
            f,
        )?;
        has_time.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_duration_option_get(&argument, "plainTime", &time, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        time.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let instant = self.emit_temporal_get_start_of_day(&zone, &date, f)?;
        self.emit_alloc_temporal_zoned_date_time(
            TemporalZonedAllocationInput::new(&instant, &zone, &calendar),
            TemporalPrototypeSource::Intrinsic,
            f,
        )?;
        instant.release(self, f);
        f.instruction(&Instruction::Else);
        let iso = self.emit_temporal_combine_iso_date_and_time(&date, &time, f)?;
        let local = self.emit_temporal_local_coordinate_from_iso_record(&iso, f)?;
        let disambiguation =
            self.emit_temporal_constant_disambiguation(Disambiguation::Compatible, f);
        let instant =
            self.emit_temporal_get_epoch_nanoseconds_for(&zone, &local, &disambiguation, f)?;
        self.emit_alloc_temporal_zoned_date_time(
            TemporalZonedAllocationInput::new(&instant, &zone, &calendar),
            TemporalPrototypeSource::Intrinsic,
            f,
        )?;
        instant.release(self, f);
        disambiguation.release(self, f);
        local.release(self, f);
        iso.release(self, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        zone.release(self, f);
        schema.release_i32_local(has_time, f);
        time.clear(f);
        zone_value.clear(f);
        argument.clear(f);
        calendar.release(self, f);
        date.release(self, f);
        Ok(())
    }
}
