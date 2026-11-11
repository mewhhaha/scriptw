//! Closed coordinate, brand, calendar and option proofs with strict LIFO owners.

use super::super::temporal::TemporalEpochNanoseconds;
use super::super::temporal_options::TemporalConversionOverflowOptions;
use super::super::temporal_plain_date::TemporalCalendarCanonicalizationContext;
use super::super::temporal_plain_date::TemporalCalendarId;
use super::super::temporal_zoned_arithmetic::{
    CompletedTemporalEpochArithmeticLocals, CompletedTemporalIsoArithmeticLocals,
    CompletedTemporalStringRoundingLocals,
};
use super::*;

const INSTANT_LIMIT_SECONDS: i64 = 8_640_000_000_000;
const NS: i64 = 1_000_000_000;

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_temporal_normalized_instant_from_relative_view(
        &mut self,
        input: &super::RelativeEpochView<'_>,
        function: &mut Function,
    ) -> NormalizedTemporalInstantLocals {
        NormalizedTemporalInstantLocals {
            epoch: self.emit_temporal_epoch_from_relative_view(input, function),
        }
    }
    pub(in crate::builtins) fn emit_temporal_calendar_from_relative_view(
        &mut self,
        input: &super::RelativeCalendarView<'_>,
        function: &mut Function,
    ) -> TemporalCalendarSlotLocals {
        let schema = self.runtime_schema();
        let identifier = schema.reserve_gc_local(function).initialize(
            input
                .value()
                .cast_reference::<StringValue>(schema, function),
            function,
        );
        let calendar_id = schema.reserve_i64_local(function);
        input.calendar_id().load(function);
        calendar_id.store(function);
        TemporalCalendarSlotLocals {
            identifier,
            calendar_id,
        }
    }
    pub(in crate::builtins) fn emit_temporal_to_normalized_instant(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
    ) -> Result<NormalizedTemporalInstantLocals, EmitError> {
        Ok(NormalizedTemporalInstantLocals {
            epoch: self.emit_temporal_epoch_from_value(input, f)?,
        })
    }

    pub(in crate::builtins) fn emit_temporal_zone_snapshot_iso_record(
        &mut self,
        snapshot: &TemporalZoneSnapshotLocals<'_>,
        function: &mut Function,
    ) -> Result<RegulatedTemporalIsoRecordLocals, EmitError> {
        let output = self.reserve_temporal_iso_record_result(function);
        let time = self.runtime_schema().reserve_i64_local(function);
        let components = [
            self.runtime_schema().reserve_i64_local(function),
            self.runtime_schema().reserve_i64_local(function),
            self.runtime_schema().reserve_i64_local(function),
        ];
        let hour = self.runtime_schema().reserve_i64_local(function);
        let minute = self.runtime_schema().reserve_i64_local(function);
        let second = self.runtime_schema().reserve_i64_local(function);
        let milli = self.runtime_schema().reserve_i64_local(function);
        // Floor milliseconds and canonical subseconds are exact for negative
        // fractional epochs as well as positive epochs. Offset is seconds,
        // and the resulting integral milliseconds fit f64's exact integer grid.
        (snapshot.epoch.floor_seconds()).load(function);
        function.instruction(&Instruction::I64Const(1000));
        function.instruction(&Instruction::I64Mul);
        (snapshot.epoch.nanosecond()).load(function);
        function.instruction(&Instruction::I64Const(1_000_000));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Add);
        (snapshot.offset).load(function);
        function.instruction(&Instruction::I64Const(1000));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::F64ConvertI64S);
        function.instruction(&Instruction::I64ReinterpretF64);
        (time).store(function);
        self.emit_date_components_from_time(
            time,
            components[0],
            components[1],
            components[2],
            hour,
            minute,
            second,
            milli,
            function,
        );
        for (source, dest) in [
            components[0],
            components[1],
            components[2],
            hour,
            minute,
            second,
            milli,
        ]
        .into_iter()
        .zip(output.fields[..7].iter().copied())
        {
            (source).load(function);
            function.instruction(&Instruction::F64ReinterpretI64);
            function.instruction(&Instruction::I64TruncF64S);
            (dest).store(function);
        }
        // Date's MonthFromTime is zero based.
        (output.fields[1]).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (output.fields[1]).store(function);
        (snapshot.epoch.nanosecond()).load(function);
        function.instruction(&Instruction::I64Const(1_000_000));
        function.instruction(&Instruction::I64RemU);
        function.instruction(&Instruction::I64Const(1000));
        function.instruction(&Instruction::I64DivU);
        (output.fields[7]).store(function);
        (snapshot.epoch.nanosecond()).load(function);
        function.instruction(&Instruction::I64Const(1000));
        function.instruction(&Instruction::I64RemU);
        (output.fields[8]).store(function);
        for slot in [milli, second, minute, hour] {
            self.runtime_schema().release_i64_local(slot, function);
        }
        for slot in components.into_iter().rev() {
            self.runtime_schema().release_i64_local(slot, function);
        }
        self.runtime_schema().release_i64_local(time, function);
        // GetISODateTimeFor balances fields; it does not impose the unrelated
        // PlainDateTime construction limits here.
        Ok(RegulatedTemporalIsoRecordLocals {
            fields: output.fields,
        })
    }
}

pub(crate) struct BrandedTemporalInstantRecordLocals {
    record: GcLocal<TemporalInstantObject>,
}
pub(crate) struct BrandedTemporalZonedRecordLocals {
    record: GcLocal<TemporalZonedDateTimeObject>,
}
impl BrandedTemporalInstantRecordLocals {
    pub(in crate::builtins) fn record(&self) -> &GcLocal<TemporalInstantObject> {
        &self.record
    }
    pub(crate) fn release(self, f: &mut Function) {
        self.record.clear(f);
    }
}
impl BrandedTemporalZonedRecordLocals {
    pub(in crate::builtins) fn record(&self) -> &GcLocal<TemporalZonedDateTimeObject> {
        &self.record
    }
    pub(crate) fn release(self, f: &mut Function) {
        self.record.clear(f);
    }
}

/// Canonical range-checked GC epoch plus exact floor coordinates.
pub(crate) struct NormalizedTemporalInstantLocals {
    epoch: TemporalEpochNanoseconds,
}
pub(crate) struct PreparedTemporalInstantLocals {
    pub(super) seconds: I64Local,
    pub(super) nano: I64Local,
}
impl NormalizedTemporalInstantLocals {
    pub(in crate::builtins) fn from_epoch(epoch: TemporalEpochNanoseconds) -> Self {
        Self { epoch }
    }
    pub(crate) fn epoch_nanoseconds(&self) -> &GcLocal<BigIntValue> {
        self.epoch.value()
    }
    pub(crate) fn floor_seconds(&self) -> I64Local {
        self.epoch.floor_seconds()
    }
    pub(crate) fn nanosecond(&self) -> I64Local {
        self.epoch.nanosecond()
    }
    pub(crate) fn release(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
        self.epoch.clear(builder, f);
    }
}
pub(crate) struct TemporalLocalCoordinateLocals {
    pub(super) seconds: I64Local,
    pub(super) nano: I64Local,
}
impl TemporalLocalCoordinateLocals {
    pub(crate) fn floor_seconds(&self) -> I64Local {
        self.seconds
    }
    pub(crate) fn nanosecond(&self) -> I64Local {
        self.nano
    }
    pub(crate) fn release(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
        builder.runtime_schema().release_i64_local(self.nano, f);
        builder.runtime_schema().release_i64_local(self.seconds, f);
    }
}
/// Balanced fields remain distinct from the subsequent carrier limit proof.
pub(crate) struct RegulatedTemporalIsoRecordLocals {
    pub(super) fields: [I64Local; 9],
}
pub(crate) struct PreparedTemporalIsoRecordLocals {
    fields: [I64Local; 9],
}
pub(crate) struct TemporalIsoDateLocals {
    pub(super) fields: [I64Local; 3],
}
impl RegulatedTemporalIsoRecordLocals {
    pub(crate) fn fields(&self) -> &[I64Local; 9] {
        &self.fields
    }
    pub(crate) fn release(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
        builder.release_temporal_plain_date_time_field_locals(self.fields, f);
    }
}
impl TemporalIsoDateLocals {
    pub(crate) fn fields(&self) -> &[I64Local; 3] {
        &self.fields
    }
    pub(crate) fn release(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
        for local in self.fields.into_iter().rev() {
            builder.runtime_schema().release_i64_local(local, f);
        }
    }
}
pub(crate) struct TemporalCalendarSlotLocals {
    identifier: GcLocal<StringValue>,
    calendar_id: I64Local,
}
impl TemporalCalendarSlotLocals {
    pub(crate) fn identifier(&self) -> &GcLocal<StringValue> {
        &self.identifier
    }
    pub(crate) fn calendar_id(&self) -> I64Local {
        self.calendar_id
    }
    pub(crate) fn release(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
        builder
            .runtime_schema()
            .release_i64_local(self.calendar_id, f);
        self.identifier.clear(f);
    }
}
pub(crate) struct ResolvedTemporalZoneLocals {
    pub(super) identifier: GcLocal<StringValue>,
    pub(super) primary: GcLocal<StringValue>,
    pub(super) kind: I64Local,
    pub(super) fixed_seconds: I64Local,
}
impl ResolvedTemporalZoneLocals {
    pub(crate) fn identifier(&self) -> &GcLocal<StringValue> {
        &self.identifier
    }
    pub(crate) fn primary_identifier(&self) -> &GcLocal<StringValue> {
        &self.primary
    }
    pub(crate) fn kind_local(&self) -> I64Local {
        self.kind
    }
    pub(crate) fn fixed_offset_seconds(&self) -> I64Local {
        self.fixed_seconds
    }
    pub(crate) fn release(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
        builder
            .runtime_schema()
            .release_i64_local(self.fixed_seconds, f);
        builder.runtime_schema().release_i64_local(self.kind, f);
        self.primary.clear(f);
        self.identifier.clear(f);
    }
}
pub(crate) struct TemporalZoneSnapshotLocals<'a> {
    pub(super) zone: &'a ResolvedTemporalZoneLocals,
    pub(super) epoch: &'a NormalizedTemporalInstantLocals,
    pub(super) offset: I64Local,
}
impl<'a> TemporalZoneSnapshotLocals<'a> {
    pub(crate) fn zone(&self) -> &'a ResolvedTemporalZoneLocals {
        self.zone
    }
    pub(crate) fn epoch(&self) -> &'a NormalizedTemporalInstantLocals {
        self.epoch
    }
    pub(crate) fn offset_seconds(&self) -> I64Local {
        self.offset
    }
    pub(crate) fn release(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
        builder.runtime_schema().release_i64_local(self.offset, f);
    }
}
pub(crate) struct RetainedTemporalOptionLocals<O: StringValuedOption> {
    local: I64Local,
    _domain: core::marker::PhantomData<O>,
}
pub(crate) type TemporalDisambiguationLocals = RetainedTemporalOptionLocals<Disambiguation>;
pub(crate) type TemporalOffsetOptionLocals = RetainedTemporalOptionLocals<OffsetOption>;
pub(crate) type TemporalOverflowLocals = RetainedTemporalOptionLocals<TemporalOverflow>;
impl<O: StringValuedOption> RetainedTemporalOptionLocals<O> {
    pub(crate) fn local(&self) -> I64Local {
        self.local
    }
    pub(crate) fn release(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
        builder.runtime_schema().release_i64_local(self.local, f);
    }
    fn proved(local: I64Local) -> Self {
        Self {
            local,
            _domain: core::marker::PhantomData,
        }
    }
}
pub(crate) struct TemporalZonedOptionsSlots {
    disambiguation: I64Local,
    offset: I64Local,
    overflow: I64Local,
}
pub(crate) struct TemporalZonedOptionsLocals {
    disambiguation: TemporalDisambiguationLocals,
    offset: TemporalOffsetOptionLocals,
    overflow: TemporalOverflowLocals,
}
impl TemporalZonedOptionsLocals {
    pub(crate) fn disambiguation(&self) -> &TemporalDisambiguationLocals {
        &self.disambiguation
    }
    pub(crate) fn offset(&self) -> &TemporalOffsetOptionLocals {
        &self.offset
    }
    pub(crate) fn overflow(&self) -> &TemporalOverflowLocals {
        &self.overflow
    }
    pub(crate) fn overflow_local(&self) -> I64Local {
        self.overflow.local()
    }
    pub(crate) fn release(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
        self.overflow.release(builder, f);
        self.offset.release(builder, f);
        self.disambiguation.release(builder, f);
    }
}
pub(crate) struct TemporalRoundingModeLocals {
    local: I64Local,
}
pub(crate) struct TemporalInstantRoundingQuantumLocals {
    local: I64Local,
}
pub(crate) struct TemporalOffsetNanosecondsLocals {
    pub(super) local: I64Local,
}
pub(crate) struct TemporalTransitionDirectionLocals {
    pub(super) local: I64Local,
}
macro_rules! single_local_owner {
    ($ty:ident) => {
        impl $ty {
            pub(crate) fn local(&self) -> I64Local {
                self.local
            }
            pub(crate) fn release(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
                builder.runtime_schema().release_i64_local(self.local, f);
            }
        }
    };
}
single_local_owner!(TemporalRoundingModeLocals);
single_local_owner!(TemporalInstantRoundingQuantumLocals);
single_local_owner!(TemporalOffsetNanosecondsLocals);
single_local_owner!(TemporalTransitionDirectionLocals);

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn reserve_temporal_instant_result(
        &mut self,
        f: &mut Function,
    ) -> PreparedTemporalInstantLocals {
        PreparedTemporalInstantLocals {
            seconds: self.runtime_schema().reserve_i64_local(f),
            nano: self.runtime_schema().reserve_i64_local(f),
        }
    }
    pub(in crate::builtins) fn reserve_temporal_iso_record_result(
        &mut self,
        f: &mut Function,
    ) -> PreparedTemporalIsoRecordLocals {
        PreparedTemporalIsoRecordLocals {
            fields: self.reserve_temporal_plain_date_time_field_locals(f),
        }
    }
    pub(in crate::builtins) fn emit_temporal_branded_instant_receiver(
        &mut self,
        f: &mut Function,
    ) -> Result<BrandedTemporalInstantRecordLocals, EmitError> {
        Ok(BrandedTemporalInstantRecordLocals {
            record: self.emit_temporal_record_from_receiver::<TemporalInstantObject>(f)?,
        })
    }
    pub(in crate::builtins) fn emit_temporal_branded_zoned_receiver(
        &mut self,
        f: &mut Function,
    ) -> Result<BrandedTemporalZonedRecordLocals, EmitError> {
        Ok(BrandedTemporalZonedRecordLocals {
            record: self.emit_temporal_record_from_receiver::<TemporalZonedDateTimeObject>(f)?,
        })
    }
    /// A branded-input copy may observe options before reading its slots.
    /// The record proof exists only inside the actual emitted brand guard and
    /// cannot escape as an unchecked handle from the non-branded branch.
    pub(in crate::builtins) fn emit_temporal_zoned_value_branch<F>(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
        branded: F,
    ) -> Result<(), EmitError>
    where
        F: FnOnce(
            &mut Self,
            &mut Function,
            &BrandedTemporalZonedRecordLocals,
        ) -> Result<(), EmitError>,
    {
        let schema = self.runtime_schema();
        input.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<TemporalZonedDateTimeObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let record = BrandedTemporalZonedRecordLocals {
            record: schema.reserve_gc_local(f).initialize(
                input.cast_reference::<TemporalZonedDateTimeObject>(schema, f),
                f,
            ),
        };
        branded(self, f, &record)?;
        record.release(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    pub(in crate::builtins) fn emit_temporal_branded_zoned_record_from_value(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
    ) -> Result<BrandedTemporalZonedRecordLocals, EmitError> {
        let schema = self.runtime_schema();
        input.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<TemporalZonedDateTimeObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_error_and_return(scriptw_ir::NativeErrorKind::TypeError,<TemporalZonedDateTimeObject as super::super::temporal::TemporalReceiverRecord>::RECEIVER_ERROR,f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(BrandedTemporalZonedRecordLocals {
            record: schema.reserve_gc_local(f).initialize(
                input.cast_reference::<TemporalZonedDateTimeObject>(schema, f),
                f,
            ),
        })
    }
    pub(in crate::builtins) fn emit_temporal_to_zoned_record(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
    ) -> Result<BrandedTemporalZonedRecordLocals, EmitError> {
        Ok(BrandedTemporalZonedRecordLocals {
            record: self.emit_temporal_zoned_date_time_from_value(input, f)?,
        })
    }
    pub(in crate::builtins) fn emit_temporal_normalized_instant_from_instant_record(
        &mut self,
        record: &BrandedTemporalInstantRecordLocals,
        f: &mut Function,
    ) -> Result<NormalizedTemporalInstantLocals, EmitError> {
        Ok(NormalizedTemporalInstantLocals {
            epoch: self.emit_temporal_instant_epoch_from_record(record.record(), f)?,
        })
    }
    pub(in crate::builtins) fn emit_temporal_normalized_instant_from_zoned_record(
        &mut self,
        record: &BrandedTemporalZonedRecordLocals,
        f: &mut Function,
    ) -> Result<NormalizedTemporalInstantLocals, EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<TemporalZonedDateTimeObject>()
                .field(TemporalZonedDateTimeObjectSchema::EPOCH_NANOSECONDS)
                .read(record.record(), schema, f)
                .reference(),
            f,
        );
        let epoch = self.emit_temporal_instant_validated_epoch(&value, f)?;
        value.clear(f);
        Ok(NormalizedTemporalInstantLocals { epoch })
    }

    pub(in crate::builtins) fn emit_temporal_euclidean_epoch_pair(
        &self,
        seconds: I64Local,
        nano: I64Local,
        function: &mut Function,
    ) {
        (nano).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        (seconds).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        (seconds).store(function);
        (nano).load(function);
        function.instruction(&Instruction::I64Const(NS));
        function.instruction(&Instruction::I64Add);
        (nano).store(function);
        function.instruction(&Instruction::End);
    }
    pub(in crate::builtins) fn emit_temporal_normalized_instant_from_arithmetic_into(
        &mut self,
        output: PreparedTemporalInstantLocals,
        input: &CompletedTemporalEpochArithmeticLocals,
        f: &mut Function,
    ) -> Result<NormalizedTemporalInstantLocals, EmitError> {
        self.emit_temporal_copy_pair(
            input.floor_seconds(),
            input.nanosecond(),
            output.seconds,
            output.nano,
            f,
        );
        self.emit_temporal_require_instant_pair(output.seconds, output.nano, f)?;
        let value = self.emit_temporal_epoch_nanoseconds_bigint(output.seconds, output.nano, f);
        let epoch = self.emit_temporal_instant_validated_epoch(&value, f)?;
        value.clear(f);
        self.runtime_schema().release_i64_local(output.nano, f);
        self.runtime_schema().release_i64_local(output.seconds, f);
        Ok(NormalizedTemporalInstantLocals { epoch })
    }
    pub(in crate::builtins) fn emit_temporal_normalized_instant_from_string_rounding_into(
        &mut self,
        output: PreparedTemporalInstantLocals,
        input: &CompletedTemporalStringRoundingLocals,
        f: &mut Function,
    ) -> Result<NormalizedTemporalInstantLocals, EmitError> {
        self.emit_temporal_copy_pair(
            input.floor_seconds(),
            input.nanosecond(),
            output.seconds,
            output.nano,
            f,
        );
        let value = self.emit_temporal_epoch_nanoseconds_bigint(output.seconds, output.nano, f);
        // The actual completed grid operation proves range without another
        // IsValidEpochNanoseconds step in Temporal string rounding.
        let epoch = self.emit_temporal_epoch_from_string_rounding(input, value, f);
        self.runtime_schema().release_i64_local(output.nano, f);
        self.runtime_schema().release_i64_local(output.seconds, f);
        Ok(NormalizedTemporalInstantLocals { epoch })
    }
    pub(in crate::builtins) fn emit_temporal_require_instant_pair(
        &mut self,
        seconds: I64Local,
        nano: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        (seconds).load(function);
        function.instruction(&Instruction::I64Const(-INSTANT_LIMIT_SECONDS));
        function.instruction(&Instruction::I64LtS);
        (seconds).load(function);
        function.instruction(&Instruction::I64Const(INSTANT_LIMIT_SECONDS));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        (seconds).load(function);
        function.instruction(&Instruction::I64Const(INSTANT_LIMIT_SECONDS));
        function.instruction(&Instruction::I64Eq);
        (nano).load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_instant_range_error(function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }
    pub(in crate::builtins) fn emit_temporal_copy_pair(
        &self,
        seconds: I64Local,
        nano: I64Local,
        dest_seconds: I64Local,
        dest_nano: I64Local,
        function: &mut Function,
    ) {
        for (source, dest) in [(seconds, dest_seconds), (nano, dest_nano)] {
            (source).load(function);
            (dest).store(function);
        }
    }
    pub(in crate::builtins) fn emit_temporal_iso_calendar_slot(
        &mut self,
        f: &mut Function,
    ) -> Result<TemporalCalendarSlotLocals, EmitError> {
        let schema = self.runtime_schema();
        let identifier = schema.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference(TemporalCalendarId::DEFAULT.canonical(), f)?,
            f,
        );
        let calendar_id = schema.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(
            TemporalCalendarId::DEFAULT.runtime_code(),
        ));
        calendar_id.store(f);
        Ok(TemporalCalendarSlotLocals {
            identifier,
            calendar_id,
        })
    }
    /// Copy an already retained calendar payload through the canonical identifier
    /// validator. Plain carriers store only the payload; the result owns the
    /// canonical String pair required by calendar arithmetic.
    pub(in crate::builtins) fn emit_temporal_calendar_slot_from_identifier(
        &mut self,
        input: &GcLocal<StringValue>,
        f: &mut Function,
    ) -> Result<TemporalCalendarSlotLocals, EmitError> {
        self.emit_temporal_calendar_canonical_string(
            input,
            TemporalCalendarCanonicalizationContext::PlainDateFamily,
            f,
        )
    }
    pub(in crate::builtins) fn emit_temporal_calendar_from_zoned_record(
        &mut self,
        record: &BrandedTemporalZonedRecordLocals,
        f: &mut Function,
    ) -> Result<TemporalCalendarSlotLocals, EmitError> {
        let schema = self.runtime_schema();
        let identifier = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<TemporalZonedDateTimeObject>()
                .field(TemporalZonedDateTimeObjectSchema::CALENDAR)
                .read(record.record(), schema, f)
                .reference(),
            f,
        );
        let result = self.emit_temporal_calendar_slot_from_identifier(&identifier, f)?;
        identifier.clear(f);
        Ok(result)
    }
    pub(in crate::builtins) fn emit_temporal_calendar_slot_from_value(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
    ) -> Result<TemporalCalendarSlotLocals, EmitError> {
        self.emit_temporal_to_temporal_calendar_identifier(
            input,
            RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_CALENDAR_MUST_BE_A_STRING,
            f,
        )
    }
    pub(in crate::builtins) fn emit_temporal_constructor_calendar_slot(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
    ) -> Result<TemporalCalendarSlotLocals, EmitError> {
        self.emit_temporal_canonicalize_calendar(
            input,
            TemporalCalendarCanonicalizationContext::ZonedDateTime,
            f,
        )
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_temporal_iso_date_from_plain_date_receiver(
        &mut self,
        f: &mut Function,
    ) -> Result<(TemporalIsoDateLocals, TemporalCalendarSlotLocals), EmitError> {
        let schema = self.runtime_schema();
        let fields = std::array::from_fn(|_| schema.reserve_i64_local(f));
        let value = schema.reserve_value_local(f);
        let record = self.emit_temporal_plain_date_record_from_receiver(f)?;
        self.emit_temporal_plain_date_load_record(&record, &fields, &value, f);
        record.clear(f);
        let text = schema
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<StringValue>(schema, f), f);
        let calendar = self.emit_temporal_calendar_slot_from_identifier(&text, f)?;
        text.clear(f);
        value.clear(f);
        Ok((TemporalIsoDateLocals { fields }, calendar))
    }
    pub(in crate::builtins) fn emit_temporal_iso_record_from_plain_date_time_receiver(
        &mut self,
        f: &mut Function,
    ) -> Result<(RegulatedTemporalIsoRecordLocals, TemporalCalendarSlotLocals), EmitError> {
        let schema = self.runtime_schema();
        let fields = self.reserve_temporal_plain_date_time_field_locals(f);
        let value = schema.reserve_value_local(f);
        self.emit_temporal_plain_date_time_fields_from_receiver(&fields, &value, f)?;
        let text = schema
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<StringValue>(schema, f), f);
        let calendar = self.emit_temporal_calendar_slot_from_identifier(&text, f)?;
        text.clear(f);
        value.clear(f);
        Ok((RegulatedTemporalIsoRecordLocals { fields }, calendar))
    }
    pub(in crate::builtins) fn emit_temporal_combine_iso_date_and_time(
        &mut self,
        date: &TemporalIsoDateLocals,
        time: &ValueLocals,
        f: &mut Function,
    ) -> Result<RegulatedTemporalIsoRecordLocals, EmitError> {
        let output = self.reserve_temporal_iso_record_result(f);
        for (source, dest) in date.fields.iter().zip(output.fields[..3].iter()) {
            source.load(f);
            dest.store(f);
        }
        let time_fields = Self::temporal_plain_date_time_time_locals(&output.fields);
        self.emit_to_temporal_time(
            time,
            TemporalConversionOverflowOptions::Omit,
            &time_fields,
            f,
        )?;
        self.emit_temporal_reject_iso_date(
            output.fields[0],
            output.fields[1],
            output.fields[2],
            f,
        )?;
        self.emit_temporal_reject_date_time_lower_bound(&output.fields, f)?;
        Ok(RegulatedTemporalIsoRecordLocals {
            fields: output.fields,
        })
    }

    pub(in crate::builtins) fn emit_temporal_regulated_iso_record_into(
        &mut self,
        output: PreparedTemporalIsoRecordLocals,
        fields: &[I64Local; 9],
        function: &mut Function,
    ) -> Result<RegulatedTemporalIsoRecordLocals, EmitError> {
        for (source, dest) in fields.iter().zip(output.fields.iter()) {
            (*source).load(function);
            (*dest).store(function);
        }
        self.emit_temporal_require_balanced_iso_fields(&output.fields, function);
        Ok(RegulatedTemporalIsoRecordLocals {
            fields: output.fields,
        })
    }
    pub(in crate::builtins) fn emit_temporal_regulated_iso_record(
        &mut self,
        fields: &[I64Local; 9],
        function: &mut Function,
    ) -> Result<RegulatedTemporalIsoRecordLocals, EmitError> {
        let output = self.reserve_temporal_iso_record_result(function);
        self.emit_temporal_regulated_iso_record_into(output, fields, function)
    }
    pub(in crate::builtins) fn emit_temporal_iso_record_from_arithmetic_into(
        &mut self,
        output: PreparedTemporalIsoRecordLocals,
        input: &CompletedTemporalIsoArithmeticLocals,
        function: &mut Function,
    ) -> Result<RegulatedTemporalIsoRecordLocals, EmitError> {
        // Completion is produced only by the actual balanced ISO operation.
        // Each owning spec algorithm performs its own required range steps.
        for (source, dest) in input.fields().iter().zip(output.fields.iter()) {
            (*source).load(function);
            (*dest).store(function);
        }
        Ok(RegulatedTemporalIsoRecordLocals {
            fields: output.fields,
        })
    }
    pub(in crate::builtins) fn emit_temporal_iso_date_from_record(
        &mut self,
        record: &RegulatedTemporalIsoRecordLocals,
        function: &mut Function,
    ) -> TemporalIsoDateLocals {
        let fields = [
            self.runtime_schema().reserve_i64_local(function),
            self.runtime_schema().reserve_i64_local(function),
            self.runtime_schema().reserve_i64_local(function),
        ];
        for (source, dest) in record.fields[..3].iter().zip(fields.iter()) {
            (*source).load(function);
            (*dest).store(function);
        }
        TemporalIsoDateLocals { fields }
    }
    pub(in crate::builtins) fn emit_temporal_add_iso_date_days(
        &mut self,
        date: &TemporalIsoDateLocals,
        days: i64,
        function: &mut Function,
    ) -> Result<TemporalIsoDateLocals, EmitError> {
        let fields = [
            self.runtime_schema().reserve_i64_local(function),
            self.runtime_schema().reserve_i64_local(function),
            self.runtime_schema().reserve_i64_local(function),
        ];
        let epoch_days = self.runtime_schema().reserve_i64_local(function);
        self.emit_temporal_plain_date_epoch_days(
            date.fields[0],
            date.fields[1],
            date.fields[2],
            epoch_days,
            function,
        );
        (epoch_days).load(function);
        function.instruction(&Instruction::I64Const(days));
        function.instruction(&Instruction::I64Add);
        (epoch_days).store(function);
        self.emit_temporal_civil_from_days(epoch_days, fields[0], fields[1], fields[2], function);
        self.runtime_schema()
            .release_i64_local(epoch_days, function);
        // Internal AddDaysToISODate has no ISODateWithinLimits check.
        Ok(TemporalIsoDateLocals { fields })
    }
    fn emit_temporal_require_balanced_iso_fields(
        &mut self,
        fields: &[I64Local; 9],
        function: &mut Function,
    ) {
        let maximum_day = self.runtime_schema().reserve_i64_local(function);
        self.emit_temporal_iso_days_in_month(fields[0], fields[1], maximum_day, function);
        (fields[1]).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64LtS);
        (fields[1]).load(function);
        function.instruction(&Instruction::I64Const(12));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        (fields[2]).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::I32Or);
        (fields[2]).load(function);
        (maximum_day).load(function);
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        for (field, limit) in fields[3..].iter().zip([24, 60, 60, 1000, 1000, 1000]) {
            (*field).load(function);
            function.instruction(&Instruction::I64Const(limit));
            function.instruction(&Instruction::I64GeU);
            function.instruction(&Instruction::I32Or);
        }
        self.emit_temporal_provider_corruption_if_i32(function);
        self.runtime_schema()
            .release_i64_local(maximum_day, function);
    }
    pub(in crate::builtins) fn emit_temporal_local_coordinate_from_iso_record(
        &mut self,
        iso: &RegulatedTemporalIsoRecordLocals,
        function: &mut Function,
    ) -> Result<TemporalLocalCoordinateLocals, EmitError> {
        let seconds = self.runtime_schema().reserve_i64_local(function);
        let nano = self.runtime_schema().reserve_i64_local(function);
        let days = self.runtime_schema().reserve_i64_local(function);
        self.emit_temporal_plain_date_epoch_days(
            iso.fields[0],
            iso.fields[1],
            iso.fields[2],
            days,
            function,
        );
        (days).load(function);
        function.instruction(&Instruction::I64Const(86_400));
        function.instruction(&Instruction::I64Mul);
        for (field, scale) in [
            (iso.fields[3], 3600),
            (iso.fields[4], 60),
            (iso.fields[5], 1),
        ] {
            (field).load(function);
            function.instruction(&Instruction::I64Const(scale));
            function.instruction(&Instruction::I64Mul);
            function.instruction(&Instruction::I64Add);
        }
        (seconds).store(function);
        function.instruction(&Instruction::I64Const(0));
        for (field, scale) in [
            (iso.fields[6], 1_000_000),
            (iso.fields[7], 1000),
            (iso.fields[8], 1),
        ] {
            (field).load(function);
            function.instruction(&Instruction::I64Const(scale));
            function.instruction(&Instruction::I64Mul);
            function.instruction(&Instruction::I64Add);
        }
        (nano).store(function);
        self.emit_temporal_require_local_coordinate_context(seconds, nano, function);
        self.runtime_schema().release_i64_local(days, function);
        Ok(TemporalLocalCoordinateLocals { seconds, nano })
    }
    pub(in crate::builtins) fn emit_temporal_midnight_coordinate_from_date(
        &mut self,
        date: &TemporalIsoDateLocals,
        function: &mut Function,
    ) -> TemporalLocalCoordinateLocals {
        let seconds = self.runtime_schema().reserve_i64_local(function);
        let nano = self.runtime_schema().reserve_i64_local(function);
        let days = self.runtime_schema().reserve_i64_local(function);
        self.emit_temporal_plain_date_epoch_days(
            date.fields[0],
            date.fields[1],
            date.fields[2],
            days,
            function,
        );
        (days).load(function);
        function.instruction(&Instruction::I64Const(86_400));
        function.instruction(&Instruction::I64Mul);
        (seconds).store(function);
        function.instruction(&Instruction::I64Const(0));
        (nano).store(function);
        self.emit_temporal_require_local_coordinate_context(seconds, nano, function);
        self.runtime_schema().release_i64_local(days, function);
        TemporalLocalCoordinateLocals { seconds, nano }
    }
    pub(in crate::builtins) fn emit_temporal_require_local_coordinate_context(
        &self,
        seconds: I64Local,
        nano: I64Local,
        function: &mut Function,
    ) {
        let limit = INSTANT_LIMIT_SECONDS + 2 * 86_400;
        (seconds).load(function);
        function.instruction(&Instruction::I64Const(-limit));
        function.instruction(&Instruction::I64LtS);
        (seconds).load(function);
        function.instruction(&Instruction::I64Const(limit));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        (nano).load(function);
        function.instruction(&Instruction::I64Const(NS));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::I32Or);
        self.emit_temporal_provider_corruption_if_i32(function);
    }
    pub(in crate::builtins) fn reserve_temporal_zoned_options_slots(
        &mut self,
        f: &mut Function,
    ) -> TemporalZonedOptionsSlots {
        let schema = self.runtime_schema();
        TemporalZonedOptionsSlots {
            disambiguation: schema.reserve_i64_local(f),
            offset: schema.reserve_i64_local(f),
            overflow: schema.reserve_i64_local(f),
        }
    }
    pub(in crate::builtins) fn emit_temporal_zoned_date_time_options_from_slots(
        &mut self,
        context: super::super::temporal::TemporalZonedDateTimeOptionsContext,
        options: &ValueLocals,
        slots: TemporalZonedOptionsSlots,
        f: &mut Function,
    ) -> Result<TemporalZonedOptionsLocals, EmitError> {
        self.emit_temporal_read_zoned_options_fields(
            context,
            options,
            slots.disambiguation,
            slots.offset,
            slots.overflow,
            f,
        )?;
        Ok(TemporalZonedOptionsLocals {
            disambiguation: RetainedTemporalOptionLocals::proved(slots.disambiguation),
            offset: RetainedTemporalOptionLocals::proved(slots.offset),
            overflow: RetainedTemporalOptionLocals::proved(slots.overflow),
        })
    }
    pub(in crate::builtins) fn emit_temporal_zoned_date_time_options(
        &mut self,
        context: super::super::temporal::TemporalZonedDateTimeOptionsContext,
        options: &ValueLocals,
        f: &mut Function,
    ) -> Result<TemporalZonedOptionsLocals, EmitError> {
        let slots = self.reserve_temporal_zoned_options_slots(f);
        self.emit_temporal_zoned_date_time_options_from_slots(context, options, slots, f)
    }
    pub(in crate::builtins) fn emit_temporal_plain_date_time_zoned_disambiguation(
        &mut self,
        options: &ValueLocals,
        f: &mut Function,
    ) -> Result<TemporalDisambiguationLocals, EmitError> {
        let schema = self.runtime_schema();
        let local = schema.reserve_i64_local(f);
        let offset = schema.reserve_i64_local(f);
        let overflow = schema.reserve_i64_local(f);
        self.emit_temporal_read_zoned_options_fields(super::super::temporal::TemporalZonedDateTimeOptionsContext::PlainDateTimeToZonedDateTime,options,local,offset,overflow,f)?;
        schema.release_i64_local(overflow, f);
        schema.release_i64_local(offset, f);
        Ok(RetainedTemporalOptionLocals::proved(local))
    }
    pub(in crate::builtins) fn emit_temporal_constant_disambiguation(
        &mut self,
        value: Disambiguation,
        function: &mut Function,
    ) -> TemporalDisambiguationLocals {
        self.emit_temporal_constant_option(value, function)
    }
    pub(in crate::builtins) fn emit_temporal_constant_offset_option(
        &mut self,
        value: OffsetOption,
        function: &mut Function,
    ) -> TemporalOffsetOptionLocals {
        self.emit_temporal_constant_option(value, function)
    }
    pub(in crate::builtins) fn emit_temporal_constant_overflow(
        &mut self,
        value: TemporalOverflow,
        function: &mut Function,
    ) -> TemporalOverflowLocals {
        self.emit_temporal_constant_option(value, function)
    }
    pub(in crate::builtins) fn emit_temporal_zoned_overflow(
        &mut self,
        options: &ValueLocals,
        f: &mut Function,
    ) -> Result<TemporalOverflowLocals, EmitError> {
        let local = self.runtime_schema().reserve_i64_local(f);
        self.emit_temporal_string_valued_option::<TemporalOverflow>(options,local,RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_PROTOTYPE_WITH_OPTIONS_MUST_BE_AN_OBJECT_OR_UNDEFINED,ZonedDateTimeOptionKey::Overflow.range_error(),f)?;
        Ok(RetainedTemporalOptionLocals::proved(local))
    }
    fn emit_temporal_constant_option<O: StringValuedOption>(
        &mut self,
        value: O,
        function: &mut Function,
    ) -> RetainedTemporalOptionLocals<O> {
        let local = self.runtime_schema().reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(value.code()));
        (local).store(function);
        RetainedTemporalOptionLocals::proved(local)
    }
    pub(in crate::builtins) fn emit_temporal_validated_rounding_mode(
        &mut self,
        input: I64Local,
        function: &mut Function,
    ) -> Result<TemporalRoundingModeLocals, EmitError> {
        let local = self.runtime_schema().reserve_i64_local(function);
        (input).load(function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64GtU);
        self.emit_temporal_provider_corruption_if_i32(function);
        (input).load(function);
        (local).store(function);
        Ok(TemporalRoundingModeLocals { local })
    }
    pub(in crate::builtins) fn emit_temporal_validated_instant_rounding_quantum(
        &mut self,
        input: I64Local,
        function: &mut Function,
    ) -> Result<TemporalInstantRoundingQuantumLocals, EmitError> {
        let local = self.runtime_schema().reserve_i64_local(function);
        (input).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LeS);
        (input).load(function);
        function.instruction(&Instruction::I64Const(86_400 * NS));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        self.emit_temporal_provider_corruption_if_i32(function);
        function.instruction(&Instruction::I64Const(86_400 * NS));
        (input).load(function);
        function.instruction(&Instruction::I64RemU);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.emit_temporal_provider_corruption_if_i32(function);
        (input).load(function);
        (local).store(function);
        Ok(TemporalInstantRoundingQuantumLocals { local })
    }
    pub(in crate::builtins) fn emit_temporal_validated_offset_nanoseconds(
        &mut self,
        input: I64Local,
        function: &mut Function,
    ) -> Result<TemporalOffsetNanosecondsLocals, EmitError> {
        let local = self.runtime_schema().reserve_i64_local(function);
        (input).load(function);
        function.instruction(&Instruction::I64Const(-86_400 * NS));
        function.instruction(&Instruction::I64LeS);
        (input).load(function);
        function.instruction(&Instruction::I64Const(86_400 * NS));
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::I32Or);
        self.emit_temporal_provider_corruption_if_i32(function);
        (input).load(function);
        (local).store(function);
        Ok(TemporalOffsetNanosecondsLocals { local })
    }
    pub(in crate::builtins) fn emit_temporal_transition_direction(
        &mut self,
        input: &GcLocal<StringValue>,
        f: &mut Function,
    ) -> Result<TemporalTransitionDirectionLocals, EmitError> {
        let local = self.runtime_schema().reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        local.store(f);
        for (name, direction) in [
            ("next", scriptw_intl::NamedTimeZoneTransitionDirection::Next),
            (
                "previous",
                scriptw_intl::NamedTimeZoneTransitionDirection::Previous,
            ),
        ] {
            self.emit_temporal_string_matches(input, name, f)?;
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I64Const(direction as i64));
            local.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        local.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::INVALID_TEMPORAL_ZONEDDATETIME_TRANSITION_DIRECTION,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(TemporalTransitionDirectionLocals { local })
    }
}

impl FunctionBuilder<'_> {
    fn emit_temporal_calendar_slot_fast_path(
        &mut self,
        input: &ValueLocals,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        output.copy_from(input, f);
        macro_rules! carrier {
            ($record:ty,$schema:ty) => {{
                input.reference().load(f);
                f.instruction(&Instruction::RefTestNonNull(
                    schema
                        .reference_type::<$record>(GcNullability::NonNullable)
                        .heap_type,
                ));
                self.open_frame(ControlFrameKind::If, f);
                let record = schema
                    .reserve_gc_local(f)
                    .initialize(input.cast_reference::<$record>(schema, f), f);
                let identifier = schema.reserve_gc_local(f).initialize(
                    schema
                        .struct_type::<$record>()
                        .field(<$schema>::CALENDAR)
                        .read(&record, schema, f)
                        .reference(),
                    f,
                );
                output.set_reference(&identifier, schema, f);
                identifier.clear(f);
                record.clear(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }};
        }
        carrier!(TemporalPlainDateObject, TemporalPlainDateObjectSchema);
        carrier!(
            TemporalPlainDateTimeObject,
            TemporalPlainDateTimeObjectSchema
        );
        carrier!(
            TemporalPlainYearMonthObject,
            TemporalPlainYearMonthObjectSchema
        );
        carrier!(
            TemporalPlainMonthDayObject,
            TemporalPlainMonthDayObjectSchema
        );
        carrier!(
            TemporalZonedDateTimeObject,
            TemporalZonedDateTimeObjectSchema
        );
        Ok(())
    }

    /// Actual spelling admission also publishes the correlated numeric policy.
    fn emit_temporal_calendar_canonical_string(
        &mut self,
        input: &GcLocal<StringValue>,
        context: TemporalCalendarCanonicalizationContext,
        f: &mut Function,
    ) -> Result<TemporalCalendarSlotLocals, EmitError> {
        let schema = self.runtime_schema();
        let chosen = schema.reserve_value_local(f);
        chosen.set_undefined(f);
        let calendar_id = schema.reserve_i64_local(f);
        let matched = schema.reserve_i32_local(f);
        let fold = schema.reserve_i32_local(f);
        f.instruction(&Instruction::I64Const(0));
        calendar_id.store(f);
        f.instruction(&Instruction::I32Const(0));
        matched.store(f);
        f.instruction(&Instruction::I32Const(1));
        fold.store(f);
        for calendar in TemporalCalendarId::ALL {
            for spelling in calendar.spellings() {
                let expected = schema
                    .reserve_gc_local(f)
                    .initialize(self.emit_interned_string_reference(spelling, f)?, f);
                self.emit_string_payload_equality_i32_with_ascii_case_folding(
                    input,
                    &expected,
                    Some(fold),
                    f,
                );
                self.open_frame(ControlFrameKind::If, f);
                let canonical = schema.reserve_gc_local(f).initialize(
                    self.emit_interned_string_reference(calendar.canonical(), f)?,
                    f,
                );
                chosen.set_reference(&canonical, schema, f);
                canonical.clear(f);
                f.instruction(&Instruction::I64Const(calendar.runtime_code()));
                calendar_id.store(f);
                f.instruction(&Instruction::I32Const(1));
                matched.store(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                expected.clear(f);
            }
        }
        matched.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            context.range_error_message(),
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let identifier = schema
            .reserve_gc_local(f)
            .initialize(chosen.cast_reference::<StringValue>(schema, f), f);
        schema.release_i32_local(fold, f);
        schema.release_i32_local(matched, f);
        chosen.clear(f);
        Ok(TemporalCalendarSlotLocals {
            identifier,
            calendar_id,
        })
    }

    pub(in crate::builtins) fn emit_temporal_canonicalize_calendar(
        &mut self,
        input: &ValueLocals,
        context: TemporalCalendarCanonicalizationContext,
        f: &mut Function,
    ) -> Result<TemporalCalendarSlotLocals, EmitError> {
        let schema = self.runtime_schema();
        let selected = schema.reserve_value_local(f);
        self.emit_temporal_calendar_slot_fast_path(input, &selected, f)?;
        selected.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let default = schema.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference(TemporalCalendarId::DEFAULT.canonical(), f)?,
            f,
        );
        selected.set_reference(&default, schema, f);
        default.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        selected.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            context.type_error_message(),
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let string = schema
            .reserve_gc_local(f)
            .initialize(selected.cast_reference::<StringValue>(schema, f), f);
        let result = self.emit_temporal_calendar_canonical_string(&string, context, f)?;
        string.clear(f);
        selected.clear(f);
        Ok(result)
    }

    pub(crate) fn emit_temporal_plain_date_calendar(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
    ) -> Result<TemporalCalendarSlotLocals, EmitError> {
        self.emit_temporal_canonicalize_calendar(
            input,
            TemporalCalendarCanonicalizationContext::PlainDateFamily,
            f,
        )
    }

    /// The registered String parser retains its own trusted caller Environment
    /// and may recover failed date probes without losing an original Throw.
    pub(crate) fn emit_temporal_to_temporal_calendar_identifier(
        &mut self,
        input: &ValueLocals,
        type_error: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<TemporalCalendarSlotLocals, EmitError> {
        let schema = self.runtime_schema();
        let selected = schema.reserve_value_local(f);
        self.emit_temporal_calendar_slot_fast_path(input, &selected, f)?;
        selected.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let default = schema.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference(TemporalCalendarId::DEFAULT.canonical(), f)?,
            f,
        );
        selected.set_reference(&default, schema, f);
        default.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        selected.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_error_and_return(scriptw_ir::NativeErrorKind::TypeError, type_error, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let string = schema
            .reserve_gc_local(f)
            .initialize(selected.cast_reference::<StringValue>(schema, f), f);
        let pending = schema.reserve_completion(f);
        schema
            .call_helper(
                crate::runtime_helpers::TemporalCalendarIdentifierArguments::new(
                    &string,
                    self.current_environment(),
                ),
                self.runtime_helper_base()?,
                f,
            )
            .store(&pending, f);
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.completion().copy_from(&pending, f);
        self.emit_return_current_completion(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let canonical = schema
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(schema, f), f);
        let result = self.emit_temporal_calendar_slot_from_identifier(&canonical, f)?;
        canonical.clear(f);
        pending.clear(f);
        string.clear(f);
        selected.clear(f);
        Ok(result)
    }
}
