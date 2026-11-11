//! `Temporal.PlainYearMonth` codegen.
//!
//! Temporal proposal 9: a calendar year and month with a *reference* ISO day
//! that is not observable through any accessor. A concrete GC record retains
//! three bounded ISO coordinates and the canonical calendar String. Calendar
//! algorithms and ISO date regulation share the PlainDate operation owners.
//!
//! `monthsInYear` comes from the retained calendar-year projection.
//! Calendar-derived references use calendar day1 converted to the ISO carrier;
//! explicit constructor references remain intact. `era`/`eraYear` are the one field pair that differs
//! and they go through `emit_temporal_calendar_era_field`; the reference day is
//! printed by `toString` exactly when the calendar is not `iso8601`, which is
//! `emit_temporal_calendar_is_default_i32`.

use crate::gc_types::*;

use super::super::*;
use super::temporal_calendar_arithmetic::{
    CompletedTemporalPartialReferenceLocals, TemporalCalendarDateLocals,
};
use super::temporal_plain_date::{
    TemporalCalendarId, TemporalCalendarMonthCode,
};
use crate::intrinsics::temporal::{TemporalIntrinsicFamily, TemporalPrototypeSource};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TemporalPlainYearMonthField {
    CalendarId,
    Era,
    EraYear,
    Year,
    Month,
    MonthCode,
    DaysInMonth,
    DaysInYear,
    MonthsInYear,
    InLeapYear,
}
impl TemporalPlainYearMonthField {
    fn date_field(self) -> super::temporal_plain_date::TemporalPlainDateField {
        use super::temporal_plain_date::TemporalPlainDateField as Date;
        match self {
            Self::CalendarId => Date::CalendarId,
            Self::Era => Date::Era,
            Self::EraYear => Date::EraYear,
            Self::Year => Date::Year,
            Self::Month => Date::Month,
            Self::MonthCode => Date::MonthCode,
            Self::DaysInMonth => Date::DaysInMonth,
            Self::DaysInYear => Date::DaysInYear,
            Self::MonthsInYear => Date::MonthsInYear,
            Self::InLeapYear => Date::InLeapYear,
        }
    }
}

/// The two Temporal types stored in the `Temporal.PlainDate` record shape.
///
/// The internal brand and the prototype are two halves of one decision, and
/// they used to be two independent arguments to
/// [`FunctionBuilder::emit_alloc_temporal_partial_date`]: pairing the month-day
/// brand with the year-month prototype compiled, and produced an object that is
/// a `Temporal.PlainMonthDay` to every brand check and a
/// `Temporal.PlainYearMonth` to every method lookup. Nothing throws on such an
/// object; it simply answers the wrong questions. Naming the type once, and
/// deriving both halves from it, makes that pairing unrepresentable.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TemporalPartialDateType {
    PlainYearMonth,
    PlainMonthDay,
}

impl TemporalPartialDateType {
    const fn intrinsic_family(self) -> TemporalIntrinsicFamily {
        match self {
            Self::PlainYearMonth => TemporalIntrinsicFamily::PlainYearMonth,
            Self::PlainMonthDay => TemporalIntrinsicFamily::PlainMonthDay,
        }
    }
}

/// `ISOYearMonthWithinLimits`. The year bound is one wider than
/// `ISODateWithinLimits` at each end because a whole month, not a single day,
/// has to fit.
const TEMPORAL_PLAIN_YEAR_MONTH_MINIMUM_YEAR: i64 = -271_821;
const TEMPORAL_PLAIN_YEAR_MONTH_MAXIMUM_YEAR: i64 = 275_760;
const TEMPORAL_PLAIN_YEAR_MONTH_MINIMUM_MONTH: i64 = 4;
const TEMPORAL_PLAIN_YEAR_MONTH_MAXIMUM_MONTH: i64 = 9;

impl<'a> FunctionBuilder<'a> {
    /// `IsValidISODate` followed by `ISOYearMonthWithinLimits`. Both failures
    /// are RangeErrors.
    pub(crate) fn emit_temporal_reject_iso_year_month(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let maximum_day_local = self.runtime_schema().reserve_i64_local(function);

        self.emit_temporal_iso_days_in_month(year_local, month_local, maximum_day_local, function);
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64LtS);
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(12));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        (day_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::I32Or);
        (day_local).load(function);
        (maximum_day_local).load(function);
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_IS_NOT_A_VALID_ISO_DATE,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.emit_temporal_year_month_within_limits_check(year_local, month_local, function)?;

        self.runtime_schema()
            .release_i64_local(maximum_day_local, function);
        Ok(())
    }

    pub(super) fn emit_alloc_temporal_partial_reference(
        &mut self,
        reference: &CompletedTemporalPartialReferenceLocals,
        prototype: TemporalPrototypeSource<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let fields = reference.fields();
        let scalars: [I32Local; 3] = std::array::from_fn(|index| {
            let local = schema.reserve_i32_local(function);
            fields[index].load(function);
            function.instruction(&Instruction::I32WrapI64);
            local.store(function);
            local
        });
        let calendar_value = schema.reserve_value_local(function);
        calendar_value.set_undefined(function);
        for calendar in TemporalCalendarId::ALL {
            reference.calendar_id().load(function);
            function.instruction(&Instruction::I64Const(calendar.runtime_code()));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            let text = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(calendar.canonical(), function)?,
                function,
            );
            calendar_value.set_reference(&text, schema, function);
            text.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        let calendar = schema.reserve_gc_local(function).initialize(
            calendar_value.cast_reference::<StringValue>(schema, function),
            function,
        );
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_temporal_object_header(
                reference.kind().intrinsic_family(),
                prototype,
                function,
            )?,
            function,
        );
        match reference.kind() {
            TemporalPartialDateType::PlainYearMonth => {
                let record = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<TemporalPlainYearMonthObject>()
                        .construct(
                            (
                                GcOperand::reference(&header, schema),
                                GcOperand::i32_local(scalars[0]),
                                GcOperand::i32_local(scalars[1]),
                                GcOperand::i32_local(scalars[2]),
                                GcOperand::reference(&calendar, schema),
                            ),
                            function,
                        ),
                    function,
                );
                self.completion()
                    .value()
                    .set_reference(&record, schema, function);
                record.clear(function);
            }
            TemporalPartialDateType::PlainMonthDay => {
                let record = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<TemporalPlainMonthDayObject>()
                        .construct(
                            (
                                GcOperand::reference(&header, schema),
                                GcOperand::i32_local(scalars[0]),
                                GcOperand::i32_local(scalars[1]),
                                GcOperand::i32_local(scalars[2]),
                                GcOperand::reference(&calendar, schema),
                            ),
                            function,
                        ),
                    function,
                );
                self.completion()
                    .value()
                    .set_reference(&record, schema, function);
                record.clear(function);
            }
        }
        self.completion()
            .set_normal(self.completion().value(), function);
        header.clear(function);
        calendar.clear(function);
        calendar_value.clear(function);
        for local in scalars.into_iter().rev() {
            schema.release_i32_local(local, function);
        }
        Ok(())
    }

    /// `ISOYearMonthWithinLimits` on its own, for callers that have already
    /// validated the day.
    pub(crate) fn emit_temporal_year_month_within_limits_check(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_YEAR_MONTH_MINIMUM_YEAR,
        ));
        function.instruction(&Instruction::I64LtS);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_YEAR_MONTH_MAXIMUM_YEAR,
        ));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_YEAR_MONTH_MINIMUM_YEAR,
        ));
        function.instruction(&Instruction::I64Eq);
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_YEAR_MONTH_MINIMUM_MONTH,
        ));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_YEAR_MONTH_MAXIMUM_YEAR,
        ));
        function.instruction(&Instruction::I64Eq);
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(
            TEMPORAL_PLAIN_YEAR_MONTH_MAXIMUM_MONTH,
        ));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_IS_OUTSIDE_THE_SUPPORTED_RANGE,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// Temporal proposal 9.1.1:
    /// `Temporal.PlainYearMonth(isoYear, isoMonth [, calendar [, referenceISODay]])`.
    pub(crate) fn emit_temporal_plain_year_month_constructor(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_require_construct_call(
            RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_CONSTRUCTOR_REQUIRES_NEW,
            function,
        )?;
        let schema = self.runtime_schema();
        let input = schema.reserve_value_local(function);
        let fields: [I64Local; 3] = std::array::from_fn(|_| schema.reserve_i64_local(function));
        self.emit_builtin_arg_to_value(0, &input, function);
        self.emit_temporal_to_integer_with_truncation(
            &input,
            fields[0],
            RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_YEAR_MUST_BE_AN_INTEGER,
            function,
        )?;
        self.emit_builtin_arg_to_value(1, &input, function);
        self.emit_temporal_to_integer_with_truncation(
            &input,
            fields[1],
            RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_MONTH_MUST_BE_AN_INTEGER,
            function,
        )?;
        self.emit_builtin_arg_to_value(2, &input, function);
        let calendar = self.emit_temporal_plain_date_calendar(&input, function)?;
        function.instruction(&Instruction::I64Const(1));
        fields[2].store(function);
        self.emit_builtin_arg_to_value(3, &input, function);
        input.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_to_integer_with_truncation(
            &input,
            fields[2],
            RuntimeErrorMessage::TEMPORAL_PLAINYEARMONTH_REFERENCE_DAY_MUST_BE_AN_INTEGER,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let reference = self.emit_temporal_complete_partial_reference(
            calendar.calendar_id(),
            TemporalPartialDateType::PlainYearMonth,
            fields,
            function,
        )?;
        let prototype = self.emit_temporal_constructor_prototype(
            TemporalIntrinsicFamily::PlainYearMonth,
            function,
        )?;
        self.emit_alloc_temporal_partial_reference(
            &reference,
            TemporalPrototypeSource::Constructor(&prototype),
            function,
        )?;
        prototype.release(function);
        reference.release(self, function);
        calendar.release(self, function);
        input.clear(function);
        for local in fields.into_iter().rev() {
            schema.release_i64_local(local, function);
        }
        Ok(())
    }

    /// The `[[InitializedTemporalYearMonth]]` brand check.
    pub(crate) fn emit_temporal_plain_year_month_record_from_receiver(
        &mut self,
        function: &mut Function,
    ) -> Result<GcLocal<TemporalPlainYearMonthObject>, EmitError> {
        self.emit_temporal_record_from_receiver(function)
    }

    pub(crate) fn emit_temporal_plain_year_month_load_record(
        &self,
        record: &GcLocal<TemporalPlainYearMonthObject>,
        fields: &[I64Local; 3],
        calendar: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let scalar = schema.reserve_i32_local(function);
        for (field, output) in [
            TemporalPlainYearMonthObjectSchema::ISO_YEAR,
            TemporalPlainYearMonthObjectSchema::ISO_MONTH,
            TemporalPlainYearMonthObjectSchema::REFERENCE_ISO_DAY,
        ]
        .into_iter()
        .zip(fields)
        {
            schema
                .struct_type::<TemporalPlainYearMonthObject>()
                .field(field)
                .read(record, schema, function)
                .store(scalar, function);
            scalar.load(function);
            function.instruction(&Instruction::I64ExtendI32S);
            output.store(function);
        }
        let text = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<TemporalPlainYearMonthObject>()
                .field(TemporalPlainYearMonthObjectSchema::CALENDAR)
                .read(record, schema, function)
                .reference(),
            function,
        );
        calendar.set_reference(&text, schema, function);
        text.clear(function);
        schema.release_i32_local(scalar, function);
    }

    /// Every `Temporal.PlainYearMonth.prototype` accessor.
    pub(crate) fn emit_temporal_plain_year_month_field(
        &mut self,
        field: TemporalPlainYearMonthField,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record =
            self.emit_temporal_record_from_receiver::<TemporalPlainYearMonthObject>(function)?;
        let fields = std::array::from_fn(|_| schema.reserve_i64_local(function));
        let calendar_value = schema.reserve_value_local(function);
        let output = schema.reserve_value_local(function);
        self.emit_temporal_plain_year_month_load_record(
            &record,
            &fields,
            &calendar_value,
            function,
        );
        let identifier = schema.reserve_gc_local(function).initialize(
            calendar_value.cast_reference::<StringValue>(schema, function),
            function,
        );
        let calendar = self.emit_temporal_calendar_slot_from_identifier(&identifier, function)?;
        self.emit_temporal_date_field_value(
            field.date_field(),
            &calendar,
            fields,
            &output,
            function,
        )?;
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

    /// `ToMonthCode`: `ToPrimitive` with a string hint, then the result must
    /// *be* a String - `{ toString: () => 5 }` primitivises to the Number 5 and
    /// is a TypeError, not the string `"5"`. The syntax check (`M` + two digits
    /// + an optional leap marker `L`) runs at *read* time, before any later
    /// field is fetched, because Test262's `from/monthcode-invalid.js` pins
    /// that `{ monthCode: "L99M", year: Symbol() }` is a RangeError while
    /// `{ monthCode: "M99L", year: Symbol() }` is a TypeError - syntax first,
    /// suitability last.
    pub(crate) fn emit_temporal_month_code_string(
        &mut self,
        value: &ValueLocals,
        type_error: RuntimeErrorMessage,
        range_error: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        let primitive = self.emit_tagged_to_primitive_locals_in_current_function_realm(
            ToPrimitiveHint::String,
            value,
            function,
        )?;
        self.emit_current_function_realm_primitive_to_tagged_locals(primitive, value, function);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            type_error,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let text = schema.reserve_gc_local(function).initialize(
            value.cast_reference::<StringValue>(schema, function),
            function,
        );
        let length = schema.reserve_i64_local(function);
        let index = schema.reserve_i64_local(function);
        let first = schema.reserve_i64_local(function);
        let second = schema.reserve_i64_local(function);
        let unit = schema.reserve_i64_local(function);
        let valid = schema.reserve_i32_local(function);
        self.emit_temporal_string_length(&text, length, function);
        length.load(function);
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::I64Eq);
        length.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        valid.store(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        self.emit_temporal_load_code_unit(&text, index, unit, function);
        valid.load(function);
        unit.load(function);
        function.instruction(&Instruction::I64Const(b'M' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32And);
        valid.store(function);
        for (position, digit) in [(1, first), (2, second)] {
            function.instruction(&Instruction::I64Const(position));
            index.store(function);
            self.emit_temporal_load_code_unit(&text, index, digit, function);
            valid.load(function);
            digit.load(function);
            function.instruction(&Instruction::I64Const(b'0' as i64));
            function.instruction(&Instruction::I64GeU);
            digit.load(function);
            function.instruction(&Instruction::I64Const(b'9' as i64));
            function.instruction(&Instruction::I64LeU);
            function.instruction(&Instruction::I32And);
            function.instruction(&Instruction::I32And);
            valid.store(function);
        }
        // The grammar admits M00L but not regular M00. Suitability is deferred
        // until every requested field and overflow option has been acquired.
        valid.load(function);
        first.load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64Ne);
        second.load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32Or);
        length.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32And);
        valid.store(function);
        length.load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(3));
        index.store(function);
        self.emit_temporal_load_code_unit(&text, index, unit, function);
        valid.load(function);
        unit.load(function);
        function.instruction(&Instruction::I64Const(b'L' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32And);
        valid.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        valid.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            range_error,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(valid, function);
        for local in [unit, second, first, index, length] {
            schema.release_i64_local(local, function);
        }
        text.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// Canonical Temporal month code from one completed calendar projection.
    /// The private projection retains the year needed by lunisolar ordinals;
    /// provider formatting aliases cannot enter this closed spelling domain.
    pub(crate) fn emit_temporal_calendar_month_code_payload(
        &mut self,
        projected: &TemporalCalendarDateLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let code = schema.reserve_i64_local(function);
        self.emit_temporal_calendar_month_code(
            projected.calendar_id(),
            projected.year(),
            projected.month(),
            code,
            function,
        );
        output.set_undefined(function);
        for candidate in TemporalCalendarMonthCode::ALL {
            code.load(function);
            function.instruction(&Instruction::I64Const(candidate.encoding()));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            let text = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(candidate.spelling(), function)?,
                function,
            );
            output.set_reference(&text, schema, function);
            text.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        schema.release_i64_local(code, function);
        Ok(())
    }

    /// Temporal deliberately forbids implicit comparison, so `valueOf` always
    /// throws on every Temporal type.
    pub(crate) fn emit_temporal_partial_date_value_of(
        &mut self,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            message,
            function,
        )
    }
}
