//! `Temporal.PlainDateTime` codegen: record layout, validation, constructor and
//! the twenty-two accessors.
//!
//! Temporal proposal 5. A `PlainDateTime` is exactly a `PlainDate` glued to a
//! `PlainTime`, so this type owns almost no arithmetic of its own: the three
//! ISO date fields reuse `temporal_plain_date.rs` (`RejectISODate`,
//! `ISODaysInMonth`, the week/day-of-year accessors) and the six wall-clock
//! fields reuse `temporal_plain_time.rs` (`RejectTime`, `RegulateTime`, the
//! nanosecond-of-day scalar). The one genuinely new primitive here is
//! `emit_temporal_civil_from_days`, the inverse of `emit_temporal_days_from_civil`,
//! which date arithmetic needs to turn an epoch-day count back into a civil
//! date.
//!
//! The calendar is likewise `temporal_plain_date.rs`'s: `era`/`eraYear` are the
//! shared `emit_temporal_calendar_era_field`, so a `gregory` `PlainDateTime`
//! and a `gregory` `PlainDate` cannot disagree about the year-0 boundary.

use crate::gc_types::*;

use super::super::*;
use super::temporal_options::TemporalTimeUnit;
use super::temporal_zone_provider::TemporalCalendarSlotLocals;
use crate::intrinsics::temporal::{TemporalIntrinsicFamily, TemporalPrototypeSource};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TemporalPlainDateTimeField {
    Date(super::temporal_plain_date::TemporalPlainDateField),
    Time(TemporalTimeUnit),
}

/// The first epoch day a `PlainDateTime` may name. Equal to
/// `TEMPORAL_PLAIN_DATE_MINIMUM_EPOCH_DAY`: `PlainDate` may hold the whole day,
/// but `PlainDateTime` may not hold its midnight.
const TEMPORAL_PLAIN_DATE_TIME_MINIMUM_EPOCH_DAY: i64 = -100_000_001;

/// Field order: the constructor argument order, and the order the fields are
/// written into the record. Indices 0..3 are the date, 3..9 the time.

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn reserve_temporal_plain_date_time_field_locals(
        &self,
        function: &mut Function,
    ) -> [I64Local; 9] {
        std::array::from_fn(|_| self.runtime_schema().reserve_i64_local(function))
    }

    pub(crate) fn release_temporal_plain_date_time_field_locals(
        &self,
        locals: [I64Local; 9],
        function: &mut Function,
    ) {
        for local in locals.into_iter().rev() {
            self.runtime_schema().release_i64_local(local, function);
        }
    }

    /// The six time locals of a `PlainDateTime` field array, in the shape the
    /// `Temporal.PlainTime` helpers expect.
    pub(crate) fn temporal_plain_date_time_time_locals(
        field_locals: &[I64Local; 9],
    ) -> [I64Local; 6] {
        [
            field_locals[3],
            field_locals[4],
            field_locals[5],
            field_locals[6],
            field_locals[7],
            field_locals[8],
        ]
    }

    /// Howard Hinnant's `civil_from_days`, the inverse of
    /// `emit_temporal_days_from_civil`. Every intermediate after `era` is
    /// non-negative, so the unsigned divisions are exact.
    pub(crate) fn emit_temporal_civil_from_days(
        &mut self,
        days_local: I64Local,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        function: &mut Function,
    ) {
        let shifted_local = self.runtime_schema().reserve_i64_local(function);
        let era_local = self.runtime_schema().reserve_i64_local(function);
        let day_of_era_local = self.runtime_schema().reserve_i64_local(function);
        let year_of_era_local = self.runtime_schema().reserve_i64_local(function);
        let day_of_year_local = self.runtime_schema().reserve_i64_local(function);
        let month_index_local = self.runtime_schema().reserve_i64_local(function);

        (days_local).load(function);
        function.instruction(&Instruction::I64Const(719_468));
        function.instruction(&Instruction::I64Add);
        (shifted_local).store(function);
        (shifted_local).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64GeS);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        (shifted_local).load(function);
        function.instruction(&Instruction::Else);
        (shifted_local).load(function);
        function.instruction(&Instruction::I64Const(146_096));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(146_097));
        function.instruction(&Instruction::I64DivS);
        (era_local).store(function);

        (shifted_local).load(function);
        (era_local).load(function);
        function.instruction(&Instruction::I64Const(146_097));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Sub);
        (day_of_era_local).store(function);

        (day_of_era_local).load(function);
        (day_of_era_local).load(function);
        function.instruction(&Instruction::I64Const(1_460));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Sub);
        (day_of_era_local).load(function);
        function.instruction(&Instruction::I64Const(36_524));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Add);
        (day_of_era_local).load(function);
        function.instruction(&Instruction::I64Const(146_096));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(365));
        function.instruction(&Instruction::I64DivU);
        (year_of_era_local).store(function);

        (year_of_era_local).load(function);
        (era_local).load(function);
        function.instruction(&Instruction::I64Const(400));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        (year_local).store(function);

        (day_of_era_local).load(function);
        (year_of_era_local).load(function);
        function.instruction(&Instruction::I64Const(365));
        function.instruction(&Instruction::I64Mul);
        (year_of_era_local).load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Add);
        (year_of_era_local).load(function);
        function.instruction(&Instruction::I64Const(100));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Sub);
        (day_of_year_local).store(function);

        (day_of_year_local).load(function);
        function.instruction(&Instruction::I64Const(5));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(153));
        function.instruction(&Instruction::I64DivU);
        (month_index_local).store(function);

        (day_of_year_local).load(function);
        (month_index_local).load(function);
        function.instruction(&Instruction::I64Const(153));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(5));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (day_local).store(function);

        (month_index_local).load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        (month_index_local).load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::Else);
        (month_index_local).load(function);
        function.instruction(&Instruction::I64Const(9));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::End);
        (month_local).store(function);

        (year_local).load(function);
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64LeS);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Add);
        (year_local).store(function);

        for local in [
            month_index_local,
            day_of_year_local,
            year_of_era_local,
            day_of_era_local,
            era_local,
            shifted_local,
        ] {
            self.runtime_schema().release_i64_local(local, function);
        }
    }

    pub(crate) fn emit_alloc_temporal_plain_date_time(
        &mut self,
        fields: &[I64Local; 9],
        calendar: &TemporalCalendarSlotLocals,
        prototype: TemporalPrototypeSource<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_reject_date_time_lower_bound(fields, function)?;
        let schema = self.runtime_schema();
        let scalars: [I32Local; 9] = std::array::from_fn(|index| {
            let local = schema.reserve_i32_local(function);
            fields[index].load(function);
            function.instruction(&Instruction::I32WrapI64);
            local.store(function);
            local
        });
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_temporal_object_header(
                TemporalIntrinsicFamily::PlainDateTime,
                prototype,
                function,
            )?,
            function,
        );
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<TemporalPlainDateTimeObject>()
                .construct(
                    (
                        GcOperand::reference(&header, schema),
                        GcOperand::i32_local(scalars[0]),
                        GcOperand::i32_local(scalars[1]),
                        GcOperand::i32_local(scalars[2]),
                        GcOperand::i32_local(scalars[3]),
                        GcOperand::i32_local(scalars[4]),
                        GcOperand::i32_local(scalars[5]),
                        GcOperand::i32_local(scalars[6]),
                        GcOperand::i32_local(scalars[7]),
                        GcOperand::i32_local(scalars[8]),
                        GcOperand::reference(calendar.identifier(), schema),
                    ),
                    function,
                ),
            function,
        );
        self.completion()
            .value()
            .set_reference(&record, schema, function);
        self.completion()
            .set_normal(self.completion().value(), function);
        record.clear(function);
        header.clear(function);
        for scalar in scalars.into_iter().rev() {
            schema.release_i32_local(scalar, function);
        }
        Ok(())
    }

    pub(crate) fn emit_temporal_plain_date_time_load_record(
        &self,
        record: &GcLocal<TemporalPlainDateTimeObject>,
        fields: &[I64Local; 9],
        calendar: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let scalar = schema.reserve_i32_local(function);
        for (field, output) in [
            TemporalPlainDateTimeObjectSchema::ISO_YEAR,
            TemporalPlainDateTimeObjectSchema::ISO_MONTH,
            TemporalPlainDateTimeObjectSchema::ISO_DAY,
            TemporalPlainDateTimeObjectSchema::HOUR,
            TemporalPlainDateTimeObjectSchema::MINUTE,
            TemporalPlainDateTimeObjectSchema::SECOND,
            TemporalPlainDateTimeObjectSchema::MILLISECOND,
            TemporalPlainDateTimeObjectSchema::MICROSECOND,
            TemporalPlainDateTimeObjectSchema::NANOSECOND,
        ]
        .into_iter()
        .zip(fields)
        {
            schema
                .struct_type::<TemporalPlainDateTimeObject>()
                .field(field)
                .read(record, schema, function)
                .store(scalar, function);
            scalar.load(function);
            function.instruction(&Instruction::I64ExtendI32S);
            output.store(function);
        }
        let text = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<TemporalPlainDateTimeObject>()
                .field(TemporalPlainDateTimeObjectSchema::CALENDAR)
                .read(record, schema, function)
                .reference(),
            function,
        );
        calendar.set_reference(&text, schema, function);
        text.clear(function);
        schema.release_i32_local(scalar, function);
    }

    /// The `[[InitializedTemporalDateTime]]` brand check on `this`, leaving the
    /// nine fields and the calendar loaded. On failure it throws and returns.
    pub(crate) fn emit_temporal_plain_date_time_fields_from_receiver(
        &mut self,
        fields: &[I64Local; 9],
        calendar: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let record =
            self.emit_temporal_record_from_receiver::<TemporalPlainDateTimeObject>(function)?;
        self.emit_temporal_plain_date_time_load_record(&record, fields, calendar, function);
        record.clear(function);
        Ok(())
    }

    /// `RejectDateTime`: `RejectISODate` on the date half (which also runs the
    /// `ISODateTimeWithinLimits` day-range check) and `RejectTime` on the time
    /// half.
    pub(crate) fn emit_temporal_reject_date_time(
        &mut self,
        field_locals: &[I64Local; 9],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_reject_iso_date(
            field_locals[0],
            field_locals[1],
            field_locals[2],
            function,
        )?;
        let time_locals = Self::temporal_plain_date_time_time_locals(field_locals);
        self.emit_temporal_reject_time(&time_locals, function)?;
        self.emit_temporal_reject_date_time_lower_bound(field_locals, function)
    }

    /// The nanosecond the day-range check cannot see. `ISODateTimeWithinLimits`
    /// rejects `ns <= nsMinInstant - nsPerDay`, and that bound lands exactly on
    /// `-271821-04-19T00:00:00`. The day itself is inside the `RejectISODate`
    /// range (`PlainDate` may hold it), so only the midnight instant on the
    /// first representable day is out of range; every later time that day is
    /// fine. The upper bound needs no companion check: `+275760-09-14` is
    /// already outside the day range, and `nsMaxInstant + nsPerDay` is that
    /// day's midnight.
    pub(crate) fn emit_temporal_reject_date_time_lower_bound(
        &mut self,
        field_locals: &[I64Local; 9],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let days_local = self.runtime_schema().reserve_i64_local(function);
        self.emit_temporal_plain_date_epoch_days(
            field_locals[0],
            field_locals[1],
            field_locals[2],
            days_local,
            function,
        );
        (days_local).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_DATE_TIME_MINIMUM_EPOCH_DAY,
        ));
        function.instruction(&Instruction::I64Eq);
        for time_local in Self::temporal_plain_date_time_time_locals(field_locals) {
            (time_local).load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32And);
        }
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::TEMPORAL_PLAINDATETIME_IS_OUTSIDE_THE_SUPPORTED_DATE_RANGE,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.runtime_schema()
            .release_i64_local(days_local, function);
        Ok(())
    }

    /// Temporal proposal 5.1: `Temporal.PlainDateTime(isoYear, isoMonth, isoDay
    /// [, hour [, minute [, second [, millisecond [, microsecond [, nanosecond
    /// [, calendar]]]]]]])`.
    pub(crate) fn emit_temporal_plain_date_time_constructor(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_require_construct_call(
            RuntimeErrorMessage::TEMPORAL_PLAINDATETIME_CONSTRUCTOR_REQUIRES_NEW,
            function,
        )?;
        let schema = self.runtime_schema();
        let input = schema.reserve_value_local(function);
        let fields = self.reserve_temporal_plain_date_time_field_locals(function);
        for (index, field) in fields.into_iter().enumerate() {
            self.emit_builtin_arg_to_value(index, &input, function);
            if index >= 3 {
                input.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                function.instruction(&Instruction::I64Const(0));
                field.store(function);
                function.instruction(&Instruction::Else);
            }
            self.emit_temporal_to_integer_with_truncation(
                &input,
                field,
                RuntimeErrorMessage::TEMPORAL_PLAINDATETIME_FIELD_MUST_BE_AN_INTEGER,
                function,
            )?;
            if index >= 3 {
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        self.emit_builtin_arg_to_value(9, &input, function);
        let calendar = self.emit_temporal_plain_date_calendar(&input, function)?;
        self.emit_temporal_reject_date_time(&fields, function)?;
        let prototype = self.emit_temporal_constructor_prototype(
            TemporalIntrinsicFamily::PlainDateTime,
            function,
        )?;
        self.emit_alloc_temporal_plain_date_time(
            &fields,
            &calendar,
            TemporalPrototypeSource::Constructor(&prototype),
            function,
        )?;
        prototype.release(function);
        calendar.release(self, function);
        input.clear(function);
        self.release_temporal_plain_date_time_field_locals(fields, function);
        Ok(())
    }

    /// Every `Temporal.PlainDateTime.prototype` accessor. The date-derived ones
    /// delegate to the `Temporal.PlainDate` emitters, so the calendar
    /// arithmetic lives in exactly one place.
    pub(crate) fn emit_temporal_plain_date_time_field(
        &mut self,
        field: TemporalPlainDateTimeField,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record =
            self.emit_temporal_record_from_receiver::<TemporalPlainDateTimeObject>(function)?;
        let fields: [I64Local; 9] = std::array::from_fn(|_| schema.reserve_i64_local(function));
        let calendar_value = schema.reserve_value_local(function);
        let output = schema.reserve_value_local(function);
        self.emit_temporal_plain_date_time_load_record(&record, &fields, &calendar_value, function);
        let identifier = schema.reserve_gc_local(function).initialize(
            calendar_value.cast_reference::<StringValue>(schema, function),
            function,
        );
        let calendar = self.emit_temporal_calendar_slot_from_identifier(&identifier, function)?;
        match field {
            TemporalPlainDateTimeField::Date(date_field) => self.emit_temporal_date_field_value(
                date_field,
                &calendar,
                [fields[0], fields[1], fields[2]],
                &output,
                function,
            )?,
            TemporalPlainDateTimeField::Time(unit) => {
                let index = match unit {
                    TemporalTimeUnit::Hour => 3,
                    TemporalTimeUnit::Minute => 4,
                    TemporalTimeUnit::Second => 5,
                    TemporalTimeUnit::Millisecond => 6,
                    TemporalTimeUnit::Microsecond => 7,
                    TemporalTimeUnit::Nanosecond => 8,
                };
                self.emit_temporal_integer_number(fields[index], &output, function);
            }
        }
        self.completion().set_normal(&output, function);
        calendar.release(self, function);
        identifier.clear(function);
        output.clear(function);
        calendar_value.clear(function);
        for local in fields.into_iter().rev() {
            schema.release_i64_local(local, function);
        }
        record.clear(function);
        Ok(())
    }

    /// The `Temporal.PlainDate` accessor that computes the same value from the
    /// same three ISO fields.

    /// Temporal deliberately forbids implicit comparison, so `valueOf` always
    /// throws.
    pub(crate) fn emit_temporal_plain_date_time_value_of(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_error_and_return(scriptw_ir::NativeErrorKind::TypeError, RuntimeErrorMessage::TEMPORAL_PLAINDATETIME_DOES_NOT_SUPPORT_IMPLICIT_CONVERSION_USE_COMPARE_OR_EQUALS, function)
    }
}
