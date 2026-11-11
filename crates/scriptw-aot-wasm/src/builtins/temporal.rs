use super::super::*;
use crate::gc_types::*;

mod records;
pub(in crate::builtins) use records::TemporalReceiverRecord;
mod epoch;
mod zoned_conversion;
use super::temporal_options::{
    Disambiguation, OffsetOption, StringValuedOption, TemporalOverflow, TemporalRoundingMode, TemporalUnitOptionProperty,
};
use super::temporal_plain_date::{TemporalCalendarId, TemporalMonthFieldContext};
use super::temporal_plain_time::NANOSECONDS_PER_TEMPORAL_DAY;
use super::temporal_plain_year_month_methods::TemporalPartialDateRewrite;
use super::temporal_zone_provider::*;
use crate::intrinsics::temporal::{TemporalIntrinsicFamily, TemporalPrototypeSource};
pub(in crate::builtins) use epoch::TemporalEpochNanoseconds;

pub(super) enum TemporalZonedDateTimePlainTarget {
    Date,
    DateTime,
    Time,
}

/// Which `Temporal.Now.plain*ISO` member is being emitted. A closed set gets
/// a closed type: three adjacent dispatch arms passing `0`/`1`/`2` would
/// compile a transposition into a silently wrong return brand.
#[derive(Clone, Copy)]
enum TemporalNowPlainKind {
    DateTime,
    Date,
    Time,
}

/// Where the `Instant` string core reads its options bag from. `toJSON` is
/// `toString` with `undefined` options and must never observe its argument.
#[derive(Clone, Copy)]
enum InstantStringOptions {
    FromArgument,
    Undefined,
}

#[derive(Clone, Copy)]
pub(super) enum TemporalZonedDateTimeOptionsContext {
    From,
    With,
    PlainDateTimeToZonedDateTime,
}

impl TemporalZonedDateTimeOptionsContext {
    const fn default_offset(self) -> OffsetOption {
        match self {
            Self::From => OffsetOption::Reject,
            Self::With | Self::PlainDateTimeToZonedDateTime => OffsetOption::Prefer,
        }
    }

    const fn object_error(self) -> RuntimeErrorMessage {
        match self {
            Self::From => RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_FROM_OPTIONS_MUST_BE_AN_OBJECT_OR_UNDEFINED,
            Self::With => {
                RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_PROTOTYPE_WITH_OPTIONS_MUST_BE_AN_OBJECT_OR_UNDEFINED
            }
            Self::PlainDateTimeToZonedDateTime => {
                RuntimeErrorMessage::TEMPORAL_PLAINDATETIME_PROTOTYPE_TOZONEDDATETIME_OPTIONS_MUST_BE_AN_OBJECT_OR_UNDEFINED
            }
        }
    }

    /// Which option keys this entry point reads. `toZonedDateTime` reads
    /// `disambiguation` only: the spec never consults `offset` or `overflow`
    /// there, so reading either would turn an unrelated getter into observable
    /// behavior (`options-read-before-algorithmic-validation.js`,
    /// `order-of-operations.js`).
    fn keys(self) -> &'static [ZonedDateTimeOptionKey] {
        match self {
            Self::From | Self::With => &ZonedDateTimeOptionKey::ALL,
            Self::PlainDateTimeToZonedDateTime => &ZonedDateTimeOptionKey::TO_ZONED_DATE_TIME,
        }
    }
}

/// Which `Temporal.ZonedDateTime.prototype` accessor
/// [`FunctionBuilder::emit_temporal_zoned_date_time_iso_field`] is producing.
///
/// The emitter used to take a [`StandardBuiltinId`] and end its dispatch in
/// `_ => unreachable!()` — a catch-all over a several-hundred-variant enum,
/// which is precisely the shape `AGENTS.md` bans: "you added a getter and
/// forgot an arm" became a live `unreachable!()` in the compiler instead of a
/// compile error. The parameter is now a closed field domain matched
/// with no catch-all, so the omission fails to build.
///
/// The other direction — `StandardBuiltinId -> ZonedDateTimeField` — lives in
/// `compile_standard_builtin`'s flat exhaustive match, which already fails to
/// build on a builtin with no arm. So a new ZonedDateTime accessor cannot reach
/// this emitter without naming its field here.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ZonedDateTimeField {
    Era,
    EraYear,
    Year,
    Month,
    MonthCode,
    Day,
    Hour,
    Minute,
    Second,
    Millisecond,
    Microsecond,
    Nanosecond,
    DayOfWeek,
    DayOfYear,
    WeekOfYear,
    YearOfWeek,
    DaysInWeek,
    DaysInMonth,
    DaysInYear,
    MonthsInYear,
    InLeapYear,
}

/// The three options `Temporal.ZonedDateTime.from` reads, in read order.
///
/// The option's identity used to be the `&str` the loop iterated, compared
/// again at two later points; a typo in the loop literal still compiled, routed
/// the code into the wrong destination local and reached a live `unreachable!()`
/// in the compiler. Every derived fact is now a total function of the variant.
#[derive(Clone, Copy)]
pub(crate) enum ZonedDateTimeOptionKey {
    Disambiguation,
    Offset,
    Overflow,
}

impl ZonedDateTimeOptionKey {
    pub(crate) const ALL: [ZonedDateTimeOptionKey; 3] = [
        ZonedDateTimeOptionKey::Disambiguation,
        ZonedDateTimeOptionKey::Offset,
        ZonedDateTimeOptionKey::Overflow,
    ];

    /// The `toZonedDateTime` subset: `disambiguation` only. `offset` and
    /// `overflow` are deliberately absent (see
    /// [`TemporalZonedDateTimeOptionsContext::keys`]).
    const TO_ZONED_DATE_TIME: [ZonedDateTimeOptionKey; 1] =
        [ZonedDateTimeOptionKey::Disambiguation];

    pub(crate) fn property(self) -> &'static str {
        match self {
            ZonedDateTimeOptionKey::Disambiguation => Disambiguation::PROPERTY,
            ZonedDateTimeOptionKey::Offset => OffsetOption::PROPERTY,
            ZonedDateTimeOptionKey::Overflow => TemporalOverflow::PROPERTY,
        }
    }

    pub(crate) fn range_error(self) -> RuntimeErrorMessage {
        match self {
            ZonedDateTimeOptionKey::Disambiguation => {
                RuntimeErrorMessage::INVALID_TEMPORAL_ZONEDDATETIME_DISAMBIGUATION_OPTION
            }
            ZonedDateTimeOptionKey::Offset => {
                RuntimeErrorMessage::INVALID_TEMPORAL_ZONEDDATETIME_OFFSET_OPTION
            }
            ZonedDateTimeOptionKey::Overflow => {
                RuntimeErrorMessage::INVALID_TEMPORAL_ZONEDDATETIME_OVERFLOW_OPTION
            }
        }
    }

    pub(crate) fn allowed(self) -> Vec<(&'static str, i64)> {
        fn pairs<O: StringValuedOption>() -> Vec<(&'static str, i64)> {
            O::ALLOWED
                .iter()
                .map(|value| (value.name(), value.code()))
                .collect()
        }
        match self {
            ZonedDateTimeOptionKey::Disambiguation => pairs::<Disambiguation>(),
            ZonedDateTimeOptionKey::Offset => pairs::<OffsetOption>(),
            ZonedDateTimeOptionKey::Overflow => pairs::<TemporalOverflow>(),
        }
    }

    fn default_code(self, context: TemporalZonedDateTimeOptionsContext) -> i64 {
        match self {
            ZonedDateTimeOptionKey::Disambiguation => Disambiguation::DEFAULT.code(),
            ZonedDateTimeOptionKey::Offset => context.default_offset().code(),
            ZonedDateTimeOptionKey::Overflow => StringValuedOption::code(TemporalOverflow::DEFAULT),
        }
    }

    fn destination(
        self,
        disambiguation_local: I64Local,
        offset_local: I64Local,
        overflow_local: I64Local,
    ) -> I64Local {
        match self {
            Self::Disambiguation => disambiguation_local,
            Self::Offset => offset_local,
            Self::Overflow => overflow_local,
        }
    }
}

const TEMPORAL_INSTANT_LIMIT_HIGH_LIMB: i64 = 468;
const TEMPORAL_INSTANT_LIMIT_LOW_LIMB: i64 = 6_923_773_503_929_843_712;
const NANOSECONDS_PER_MILLISECOND: i64 = 1_000_000;
const NANOSECONDS_PER_SECOND: i64 = 1_000_000_000;
pub(super) const SECONDS_PER_DAY: i64 = 86_400;

#[derive(Clone, Copy)]
pub(super) enum TemporalTimeCalendarUse<'a> {
    Ignore,
    Resolve { calendar: &'a ValueLocals },
}

/// Which string forms a time-zone-taking entry point accepts.
///
/// The constructor takes `Identifier` (`ParseTemporalTimeZoneString` rejects
/// a datetime string — `timezone-iso-string.js`); every other time-zone-taking
/// entry point takes `Object` (`ToTemporalTimeZoneObject` extracts the zone
/// from an ISO string — `Now/*/timezone-string-datetime.js`,
/// `from/argument-propertybag-timezone-string-*.js`,
/// `withTimeZone/timezone-string-*.js`).
#[derive(Clone, Copy)]
pub(crate) enum TemporalTimeZoneStringGoal {
    Identifier,
    Object,
}

enum TemporalIsoParseGoal<'a> {
    Instant {
        seconds: I64Local,
        nanosecond: I64Local,
    },
    TimeZoneIdentifier {
        time_zone: &'a ValueLocals,
        string_goal: TemporalTimeZoneStringGoal,
    },
    ZonedDateTimeSyntax {
        time_zone: &'a ValueLocals,
        calendar: &'a ValueLocals,
    },
    ZonedDateTime {
        policies: &'a TemporalZonedOptionsLocals,
        zone: &'a ResolvedTemporalZoneLocals,
        output: PreparedTemporalInstantLocals,
    },
    PlainDate {
        year_destination_local: I64Local,
        month_destination_local: I64Local,
        day_destination_local: I64Local,
        calendar: &'a ValueLocals,
    },
    PlainDateTime {
        year_destination_local: I64Local,
        month_destination_local: I64Local,
        day_destination_local: I64Local,
        hour_destination_local: I64Local,
        minute_destination_local: I64Local,
        second_destination_local: I64Local,
        nanosecond_destination_local: I64Local,
        calendar: &'a ValueLocals,
    },
    PlainTime {
        hour_destination_local: I64Local,
        minute_destination_local: I64Local,
        second_destination_local: I64Local,
        nanosecond_destination_local: I64Local,
        calendar_use: TemporalTimeCalendarUse<'a>,
    },
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn emit_temporal_now_time_zone_id(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let system = self.emit_system_time_zone(f)?;
        self.completion()
            .value()
            .set_reference(system.identifier(), self.runtime_schema(), f);
        self.completion().set_normal(self.completion().value(), f);
        system.release(self, f);
        Ok(())
    }
    /// Millisecond host clock split by floor division; negative clocks retain
    /// a nonnegative subsecond remainder before immutable BigInt publication.
    fn emit_temporal_now_epoch_seconds_and_subseconds(
        &mut self,
        seconds: I64Local,
        nano: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let import = self
            .functions
            .wall_clock_millis_import_function_index()
            .ok_or_else(|| EmitError::unsupported("Temporal.Now requires wall_clock_millis"))?;
        let schema = self.runtime_schema();
        let millis = schema.reserve_i64_local(f);
        f.instruction(&Instruction::Call(import));
        f.instruction(&Instruction::I64TruncF64S);
        millis.store(f);
        millis.load(f);
        f.instruction(&Instruction::I64Const(1000));
        f.instruction(&Instruction::I64DivS);
        seconds.store(f);
        millis.load(f);
        seconds.load(f);
        f.instruction(&Instruction::I64Const(1000));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Sub);
        nano.store(f);
        nano.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, f);
        seconds.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        seconds.store(f);
        nano.load(f);
        f.instruction(&Instruction::I64Const(1000));
        f.instruction(&Instruction::I64Add);
        nano.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        nano.load(f);
        f.instruction(&Instruction::I64Const(NANOSECONDS_PER_MILLISECOND));
        f.instruction(&Instruction::I64Mul);
        nano.store(f);
        schema.release_i64_local(millis, f);
        Ok(())
    }
    fn emit_temporal_now_epoch(
        &mut self,
        f: &mut Function,
    ) -> Result<TemporalEpochNanoseconds, EmitError> {
        let schema = self.runtime_schema();
        let seconds = schema.reserve_i64_local(f);
        let nano = schema.reserve_i64_local(f);
        self.emit_temporal_now_epoch_seconds_and_subseconds(seconds, nano, f)?;
        let value = self.emit_temporal_epoch_nanoseconds_bigint(seconds, nano, f);
        let epoch = self.emit_temporal_instant_validated_epoch(&value, f)?;
        value.clear(f);
        schema.release_i64_local(nano, f);
        schema.release_i64_local(seconds, f);
        Ok(epoch)
    }
    pub(crate) fn emit_temporal_now_instant(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let epoch = self.emit_temporal_now_epoch(f)?;
        self.emit_alloc_temporal_instant(&epoch, TemporalPrototypeSource::Intrinsic, f)?;
        epoch.clear(self, f);
        Ok(())
    }
    pub(crate) fn emit_temporal_now_zoned_date_time_iso(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let input = self.runtime_schema().reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        let prepared = self.reserve_temporal_zone_identity(f);
        let zone = self.emit_temporal_time_zone_or_system_into(prepared, &input, f)?;
        let calendar = self.emit_temporal_iso_calendar_slot(f)?;
        let epoch = self.emit_temporal_now_epoch(f)?;
        let instant = NormalizedTemporalInstantLocals::from_epoch(epoch);
        self.emit_alloc_temporal_zoned_date_time(
            TemporalZonedAllocationInput::new(&instant, &zone, &calendar),
            TemporalPrototypeSource::Intrinsic,
            f,
        )?;
        instant.release(self, f);
        calendar.release(self, f);
        zone.release(self, f);
        input.clear(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_now_plain_date_time_iso(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_now_plain_iso(TemporalNowPlainKind::DateTime, f)
    }
    pub(crate) fn emit_temporal_now_plain_date_iso(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_now_plain_iso(TemporalNowPlainKind::Date, f)
    }
    pub(crate) fn emit_temporal_now_plain_time_iso(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_now_plain_iso(TemporalNowPlainKind::Time, f)
    }
    fn emit_temporal_now_plain_iso(
        &mut self,
        kind: TemporalNowPlainKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let input = self.runtime_schema().reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        let prepared = self.reserve_temporal_zone_identity(f);
        let zone = self.emit_temporal_time_zone_or_system_into(prepared, &input, f)?;
        let epoch = self.emit_temporal_now_epoch(f)?;
        let instant = NormalizedTemporalInstantLocals::from_epoch(epoch);
        let snapshot = self.emit_temporal_zone_snapshot(&zone, &instant, f)?;
        let iso = self.emit_temporal_zone_snapshot_iso_record(&snapshot, f)?;
        let calendar = self.emit_temporal_iso_calendar_slot(f)?;
        match kind {
            TemporalNowPlainKind::Date => self.emit_alloc_temporal_plain_date(
                iso.fields()[0],
                iso.fields()[1],
                iso.fields()[2],
                &calendar,
                TemporalPrototypeSource::Intrinsic,
                f,
            )?,
            TemporalNowPlainKind::DateTime => self.emit_alloc_temporal_plain_date_time(
                iso.fields(),
                &calendar,
                TemporalPrototypeSource::Intrinsic,
                f,
            )?,
            TemporalNowPlainKind::Time => self.emit_alloc_temporal_plain_time(
                &Self::temporal_plain_date_time_time_locals(iso.fields()),
                TemporalPrototypeSource::Intrinsic,
                f,
            )?,
        }
        calendar.release(self, f);
        iso.release(self, f);
        snapshot.release(self, f);
        instant.release(self, f);
        zone.release(self, f);
        input.clear(f);
        Ok(())
    }

    pub(crate) fn emit_temporal_zoned_date_time_compare(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left_value = schema.reserve_value_local(f);
        let right_value = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &left_value, f);
        self.emit_builtin_arg_to_value(1, &right_value, f);
        let left = self.emit_temporal_to_zoned_record(&left_value, f)?;
        let right = self.emit_temporal_to_zoned_record(&right_value, f)?;
        let left_epoch = self.emit_temporal_normalized_instant_from_zoned_record(&left, f)?;
        let right_epoch = self.emit_temporal_normalized_instant_from_zoned_record(&right, f)?;
        let comparison = schema.reserve_i32_local(f);
        let number = schema.reserve_i64_local(f);
        self.emit_bigint_compare(
            left_epoch.epoch_nanoseconds(),
            right_epoch.epoch_nanoseconds(),
            comparison,
            f,
        );
        comparison.load(f);
        f.instruction(&Instruction::F64ConvertI32S);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.store(f);
        let output = schema.reserve_value_local(f);
        output.set_number(number, f);
        self.completion().set_normal(&output, f);
        output.clear(f);
        schema.release_i64_local(number, f);
        schema.release_i32_local(comparison, f);
        right_epoch.release(self, f);
        left_epoch.release(self, f);
        right.release(f);
        left.release(f);
        right_value.clear(f);
        left_value.clear(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_zoned_date_time_from(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let input = schema.reserve_value_local(f);
        let options = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        self.emit_builtin_arg_to_value(1, &options, f);
        self.emit_temporal_zoned_date_time_from_input(&input, &options, f)?;
        options.clear(f);
        input.clear(f);
        Ok(())
    }
    pub(super) fn emit_temporal_zoned_date_time_from_value(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcLocal<TemporalZonedDateTimeObject>, EmitError> {
        let schema = self.runtime_schema();
        let options = schema.reserve_value_local(f);
        options.set_undefined(f);
        self.emit_temporal_zoned_date_time_from_input(input, &options, f)?;
        options.clear(f);
        Ok(schema.reserve_gc_local(f).initialize(
            self.completion()
                .value()
                .cast_reference::<TemporalZonedDateTimeObject>(schema, f),
            f,
        ))
    }
    #[allow(clippy::too_many_arguments)]
    fn emit_temporal_regulate_property_bag_date_time(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        hour_local: I64Local,
        minute_local: I64Local,
        second_local: I64Local,
        millisecond_local: I64Local,
        microsecond_local: I64Local,
        nanosecond_local: I64Local,
        overflow_option_local: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let maximum_day_local = self.runtime_schema().reserve_i64_local(function);
        let invalid_local = self.runtime_schema().reserve_i64_local(function);

        (year_local).load(function);
        function.instruction(&Instruction::I64Const(-271_821));
        function.instruction(&Instruction::I64LtS);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(275_760));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(scriptw_ir::NativeErrorKind::RangeError,RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_PROPERTY_BAG_YEAR_IS_OUTSIDE_THE_SUPPORTED_INSTANT_RANGE,function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        (month_local).load(function);
        function.instruction(&Instruction::I64Const(12));
        function.instruction(&Instruction::I64GtS);
        self.open_frame(ControlFrameKind::If, function);
        (overflow_option_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_PROPERTY_BAG_MONTH_IS_OUT_OF_RANGE,
            function,
        )?;
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(12));
        (month_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        function.instruction(&Instruction::I64Const(31));
        (maximum_day_local).store(function);
        for month in [4_i64, 6, 9, 11] {
            (month_local).load(function);
            function.instruction(&Instruction::I64Const(month));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(30));
            (maximum_day_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Eqz);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(100));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(400));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(29));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(28));
        function.instruction(&Instruction::End);
        (maximum_day_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        function.instruction(&Instruction::I64Const(0));
        (invalid_local).store(function);
        for (local, minimum, maximum) in [
            (hour_local, 0_i64, 23_i64),
            (minute_local, 0, 59),
            (second_local, 0, 59),
            (millisecond_local, 0, 999),
            (microsecond_local, 0, 999),
            (nanosecond_local, 0, 999),
        ] {
            (local).load(function);
            function.instruction(&Instruction::I64Const(minimum));
            function.instruction(&Instruction::I64LtS);
            (local).load(function);
            function.instruction(&Instruction::I64Const(maximum));
            function.instruction(&Instruction::I64GtS);
            function.instruction(&Instruction::I32Or);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(1));
            (invalid_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        (day_local).load(function);
        (maximum_day_local).load(function);
        function.instruction(&Instruction::I64GtS);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (invalid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        (invalid_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        (overflow_option_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(scriptw_ir::NativeErrorKind::RangeError,RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_PROPERTY_BAG_DATE_TIME_FIELD_IS_OUT_OF_RANGE,function)?;
        function.instruction(&Instruction::Else);
        (day_local).load(function);
        (maximum_day_local).load(function);
        function.instruction(&Instruction::I64GtS);
        self.open_frame(ControlFrameKind::If, function);
        (maximum_day_local).load(function);
        (day_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for (local, minimum, maximum) in [
            (hour_local, 0_i64, 23_i64),
            (minute_local, 0, 59),
            (second_local, 0, 59),
            (millisecond_local, 0, 999),
            (microsecond_local, 0, 999),
            (nanosecond_local, 0, 999),
        ] {
            (local).load(function);
            function.instruction(&Instruction::I64Const(minimum));
            function.instruction(&Instruction::I64LtS);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(minimum));
            (local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            (local).load(function);
            function.instruction(&Instruction::I64Const(maximum));
            function.instruction(&Instruction::I64GtS);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(maximum));
            (local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.runtime_schema()
            .release_i64_local(invalid_local, function);
        self.runtime_schema()
            .release_i64_local(maximum_day_local, function);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_temporal_property_bag_integer(
        &mut self,
        input: &ValueLocals,
        property: &str,
        present: I64Local,
        output: I64Local,
        default: i64,
        not_finite_error: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        self.emit_temporal_duration_option_get(input, property, &value, function)?;
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::I64ExtendI32U);
        present.store(function);
        present.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(default));
        output.store(function);
        function.instruction(&Instruction::Else);
        self.emit_temporal_to_integer_with_truncation(&value, output, not_finite_error, function)?;

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.clear(function);
        Ok(())
    }

    /// `IsPartialTemporalObject` step 2: a *branded* Temporal object is never a
    /// partial property bag, even though its prototype supplies every field
    /// name `PrepareCalendarFields` looks for. Without this, `.with(plainDate)`
    /// happily reads `plainDate.year` / `.month` / `.day` through the getters
    /// and succeeds where the specification demands a TypeError.
    ///
    /// Runs *before* the observable `calendar` / `timeZone` reads, matching the
    /// step order, and leaves non-objects alone — callers have already rejected
    /// those.
    ///
    /// Instant and Duration are deliberately absent: the specification's slot
    /// list is `[[InitializedTemporalDate]]`, `[[...DateTime]]`,
    /// `[[...MonthDay]]`, `[[...Time]]`, `[[...YearMonth]]`,
    /// `[[...ZonedDateTime]]`.
    pub(crate) fn emit_temporal_reject_branded_partial_object(
        &mut self,
        input: &ValueLocals,
        error_message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        for (index, test) in [
            Instruction::RefTestNonNull(
                schema
                    .reference_type::<TemporalPlainDateObject>(GcNullability::NonNullable)
                    .heap_type,
            ),
            Instruction::RefTestNonNull(
                schema
                    .reference_type::<TemporalPlainDateTimeObject>(GcNullability::NonNullable)
                    .heap_type,
            ),
            Instruction::RefTestNonNull(
                schema
                    .reference_type::<TemporalPlainMonthDayObject>(GcNullability::NonNullable)
                    .heap_type,
            ),
            Instruction::RefTestNonNull(
                schema
                    .reference_type::<TemporalPlainTimeObject>(GcNullability::NonNullable)
                    .heap_type,
            ),
            Instruction::RefTestNonNull(
                schema
                    .reference_type::<TemporalPlainYearMonthObject>(GcNullability::NonNullable)
                    .heap_type,
            ),
            Instruction::RefTestNonNull(
                schema
                    .reference_type::<TemporalZonedDateTimeObject>(GcNullability::NonNullable)
                    .heap_type,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            input.reference().load(function);
            function.instruction(&test);
            if index != 0 {
                function.instruction(&Instruction::I32Or);
            }
        }
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            error_message,
            function,
        )?;

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// `PrepareCalendarFields` with `ToPositiveIntegerWithTruncation`, the
    /// conversion the field table names for `day` and `month` (and only those).
    /// Identical to [`Self::emit_temporal_property_bag_integer`] except that a
    /// *present* field truncating to zero or below is a RangeError raised right
    /// here — before the next field is read and long before
    /// `GetTemporalOverflowOption`, which is why `{ day: -1 }` beats a primitive
    /// `options` argument to the throw in Test262's `with/options-wrong-type.js`
    /// and why `overflow: "constrain"` cannot rescue `{ month: 0 }`.
    ///
    /// An *absent* field takes `default` unvalidated, so callers may keep using
    /// `0` as the "no value" sentinel.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_temporal_property_bag_positive_integer(
        &mut self,
        input: &ValueLocals,
        property: &str,
        present: I64Local,
        output: I64Local,
        default: i64,
        not_finite_error: RuntimeErrorMessage,
        not_positive_error: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_property_bag_integer(
            input,
            property,
            present,
            output,
            default,
            not_finite_error,
            function,
        )?;
        present.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        output.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            not_positive_error,
            function,
        )?;

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_temporal_property_bag_string(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        let pending = self.runtime_schema().reserve_completion(function);
        self.emit_value_to_string_payload(value, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        value.copy_from(pending.value(), function);
        pending.clear(function);

        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(super) fn emit_temporal_offset_string(
        &mut self,
        value: &ValueLocals,
        error: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        let pending = self.runtime_schema().reserve_completion(f);
        self.emit_tagged_to_primitive_locals(
            ToPrimitiveHint::String,
            value,
            &pending,
            ToPrimitiveAbruptRoute::ReturnCurrentFunction,
            f,
        )?;
        pending.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_error_and_return(scriptw_ir::NativeErrorKind::TypeError, error, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.copy_from(pending.value(), f);
        pending.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    pub(super) fn emit_temporal_read_zoned_options_fields(
        &mut self,
        context: TemporalZonedDateTimeOptionsContext,
        options: &ValueLocals,
        disambiguation: I64Local,
        offset: I64Local,
        overflow: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        for key in ZonedDateTimeOptionKey::ALL {
            f.instruction(&Instruction::I64Const(key.default_code(context)));
            key.destination(disambiguation, offset, overflow).store(f);
        }
        options.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_heap_object_like_tag_i32(options.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            context.object_error(),
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let value = schema.reserve_value_local(f);
        let pending = schema.reserve_completion(f);
        let recognized = schema.reserve_i32_local(f);
        for key in context.keys() {
            self.emit_temporal_duration_option_get(options, key.property(), &value, f)?;
            value.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            f.instruction(&Instruction::I32Ne);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_value_to_string_payload(&value, &pending, f)?;
            self.completion().copy_from(&pending, f);
            self.emit_propagate_current_throw_if_needed(f);
            let text = schema
                .reserve_gc_local(f)
                .initialize(pending.value().cast_reference::<StringValue>(schema, f), f);
            f.instruction(&Instruction::I32Const(0));
            recognized.store(f);
            for (expected, code) in key.allowed() {
                self.emit_temporal_string_matches(&text, expected, f)?;
                self.open_frame(ControlFrameKind::If, f);
                f.instruction(&Instruction::I32Const(1));
                recognized.store(f);
                f.instruction(&Instruction::I64Const(code));
                key.destination(disambiguation, offset, overflow).store(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            recognized.load(f);
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                key.range_error(),
                f,
            )?;
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            text.clear(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        schema.release_i32_local(recognized, f);
        pending.clear(f);
        value.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_temporal_zoned_date_time_constructor(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_require_construct_call(
            RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_CONSTRUCTOR_REQUIRES_NEW,
            f,
        )?;
        let input = self.runtime_schema().reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        let instant = self.emit_temporal_to_normalized_instant(&input, f)?;
        self.emit_builtin_arg_to_value(1, &input, f);
        let zone = self.emit_temporal_zoned_date_time_time_zone(
            &input,
            TemporalTimeZoneStringGoal::Identifier,
            f,
        )?;
        self.emit_builtin_arg_to_value(2, &input, f);
        let calendar = self.emit_temporal_constructor_calendar_slot(&input, f)?;
        let prototype =
            self.emit_temporal_constructor_prototype(TemporalIntrinsicFamily::ZonedDateTime, f)?;
        self.emit_alloc_temporal_zoned_date_time(
            TemporalZonedAllocationInput::new(&instant, &zone, &calendar),
            TemporalPrototypeSource::Constructor(&prototype),
            f,
        )?;
        prototype.release(f);
        calendar.release(self, f);
        zone.release(self, f);
        instant.release(self, f);
        input.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_temporal_parse_time_zone_value_into(
        &mut self,
        input: &ValueLocals,
        goal: TemporalTimeZoneStringGoal,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        handled.store(f);
        if matches!(goal, TemporalTimeZoneStringGoal::Object) {
            input.reference().load(f);
            f.instruction(&Instruction::RefTestNonNull(
                schema
                    .reference_type::<TemporalZonedDateTimeObject>(GcNullability::NonNullable)
                    .heap_type,
            ));
            self.open_frame(ControlFrameKind::If, f);
            let record = schema.reserve_gc_local(f).initialize(
                input.cast_reference::<TemporalZonedDateTimeObject>(schema, f),
                f,
            );
            let zone = schema.reserve_gc_local(f).initialize(
                schema
                    .struct_type::<TemporalZonedDateTimeObject>()
                    .field(TemporalZonedDateTimeObjectSchema::TIME_ZONE)
                    .read(&record, schema, f)
                    .reference(),
                f,
            );
            input.set_reference(&zone, schema, f);
            zone.clear(f);
            record.clear(f);
            f.instruction(&Instruction::I32Const(1));
            handled.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        handled.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        input.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_TIME_ZONE_MUST_BE_A_STRING,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let string = schema
            .reserve_gc_local(f)
            .initialize(input.cast_reference::<StringValue>(schema, f), f);
        let named = schema.reserve_i64_local(f);
        let first = schema.reserve_i64_local(f);
        let length = schema.reserve_i64_local(f);
        let zero = schema.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        zero.store(f);
        self.emit_temporal_load_code_unit(&string, zero, first, f);
        self.emit_temporal_string_length(&string, length, f);
        self.emit_temporal_named_identifier_syntax(&string, named, f);
        first.load(f);
        f.instruction(&Instruction::I64Const(b'+' as i64));
        f.instruction(&Instruction::I64Eq);
        first.load(f);
        f.instruction(&Instruction::I64Const(b'-' as i64));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::I32Or);
        length.load(f);
        f.instruction(&Instruction::I64Const(6));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        named.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        // A direct identifier is canonicalized by the identity factory. Direct
        // numeric forms are parsed there once; ISO forms follow their own goal.
        f.instruction(&Instruction::Else);
        match goal {
            TemporalTimeZoneStringGoal::Identifier => self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::INVALID_TEMPORAL_TIME_ZONE_IDENTIFIER,
                f,
            )?,
            TemporalTimeZoneStringGoal::Object => {
                self.emit_temporal_parse_iso_string(
                    &string,
                    TemporalIsoParseGoal::TimeZoneIdentifier {
                        time_zone: input,
                        string_goal: goal,
                    },
                    f,
                )?;
            }
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i64_local(zero, f);
        schema.release_i64_local(length, f);
        schema.release_i64_local(first, f);
        schema.release_i64_local(named, f);
        string.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(handled, f);
        Ok(())
    }

    pub(crate) fn emit_temporal_fixed_time_zone_offset_seconds(
        &mut self,
        input: &GcLocal<StringValue>,
        output: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let length = schema.reserve_i64_local(f);
        let cursor = schema.reserve_i64_local(f);
        let unit = schema.reserve_i64_local(f);
        let sign = schema.reserve_i64_local(f);
        let hour = schema.reserve_i64_local(f);
        let minute = schema.reserve_i64_local(f);
        let valid = schema.reserve_i64_local(f);
        self.emit_temporal_string_length(input, length, f);
        cursor.set_constant(0, f);
        minute.set_constant(0, f);
        self.emit_temporal_load_code_unit(input, cursor, unit, f);
        unit.load(f);
        f.instruction(&Instruction::I64Const(b'+' as i64));
        f.instruction(&Instruction::I64Eq);
        unit.load(f);
        f.instruction(&Instruction::I64Const(b'-' as i64));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::I64ExtendI32U);
        valid.store(f);
        unit.load(f);
        f.instruction(&Instruction::I64Const(b'-' as i64));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        f.instruction(&Instruction::I64Const(-1));
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::End);
        sign.store(f);
        cursor.set_constant(1, f);
        self.emit_temporal_parse_fixed_decimal(input, cursor, length, unit, valid, hour, 2, f);
        cursor.load(f);
        length.load(f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_load_code_unit(input, cursor, unit, f);
        unit.load(f);
        f.instruction(&Instruction::I64Const(b':' as i64));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_advance_cursor(cursor, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_temporal_parse_fixed_decimal(input, cursor, length, unit, valid, minute, 2, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        valid.load(f);
        f.instruction(&Instruction::I64Eqz);
        cursor.load(f);
        length.load(f);
        f.instruction(&Instruction::I64Ne);
        f.instruction(&Instruction::I32Or);
        hour.load(f);
        f.instruction(&Instruction::I64Const(23));
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::I32Or);
        minute.load(f);
        f.instruction(&Instruction::I64Const(59));
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::INVALID_TEMPORAL_ZONEDDATETIME_TIME_ZONE,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        hour.load(f);
        f.instruction(&Instruction::I64Const(3600));
        f.instruction(&Instruction::I64Mul);
        minute.load(f);
        f.instruction(&Instruction::I64Const(60));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Add);
        sign.load(f);
        f.instruction(&Instruction::I64Mul);
        output.store(f);
        for local in [valid, minute, hour, sign, unit, cursor, length] {
            schema.release_i64_local(local, f);
        }
        Ok(())
    }

    pub(in crate::builtins) fn emit_alloc_temporal_zoned_date_time(
        &mut self,
        input: TemporalZonedAllocationInput<'_>,
        prototype: TemporalPrototypeSource<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(f).initialize(
            self.emit_alloc_temporal_object_header(
                TemporalIntrinsicFamily::ZonedDateTime,
                prototype,
                f,
            )?,
            f,
        );
        let record = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<TemporalZonedDateTimeObject>()
                .construct(
                    (
                        GcOperand::reference(&header, schema),
                        GcOperand::reference(input.instant().epoch_nanoseconds(), schema),
                        GcOperand::reference(input.zone().identifier(), schema),
                        GcOperand::reference(input.calendar().identifier(), schema),
                    ),
                    f,
                ),
            f,
        );
        self.completion().value().set_reference(&record, schema, f);
        self.completion().set_normal(self.completion().value(), f);
        record.clear(f);
        header.clear(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_zoned_date_time_record_from_receiver(
        &mut self,
        f: &mut Function,
    ) -> Result<GcLocal<TemporalZonedDateTimeObject>, EmitError> {
        self.emit_temporal_record_from_receiver(f)
    }
    pub(crate) fn emit_temporal_zoned_date_time_epoch_nanoseconds(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record = self.emit_temporal_zoned_date_time_record_from_receiver(f)?;
        let epoch = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<TemporalZonedDateTimeObject>()
                .field(TemporalZonedDateTimeObjectSchema::EPOCH_NANOSECONDS)
                .read(&record, schema, f)
                .reference(),
            f,
        );
        self.completion().value().set_reference(&epoch, schema, f);
        self.completion().set_normal(self.completion().value(), f);
        epoch.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_zoned_date_time_epoch_milliseconds(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record = self.emit_temporal_branded_zoned_receiver(f)?;
        let epoch = self.emit_temporal_normalized_instant_from_zoned_record(&record, f)?;
        let millis = schema.reserve_i64_local(f);
        epoch.floor_seconds().load(f);
        f.instruction(&Instruction::I64Const(1000));
        f.instruction(&Instruction::I64Mul);
        epoch.nanosecond().load(f);
        f.instruction(&Instruction::I64Const(1_000_000));
        f.instruction(&Instruction::I64DivU);
        f.instruction(&Instruction::I64Add);
        millis.store(f);
        let output = schema.reserve_value_local(f);
        self.emit_temporal_integer_number(millis, &output, f);
        self.completion().set_normal(&output, f);
        output.clear(f);
        schema.release_i64_local(millis, f);
        epoch.release(self, f);
        record.release(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_zoned_date_time_offset(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record = self.emit_temporal_branded_zoned_receiver(f)?;
        let instant = self.emit_temporal_normalized_instant_from_zoned_record(&record, f)?;
        let zone = self.emit_temporal_zone_from_zoned_record(&record, f)?;
        let snapshot = self.emit_temporal_zone_snapshot(&zone, &instant, f)?;
        let output = schema.reserve_value_local(f);
        self.emit_temporal_format_exact_offset_seconds(snapshot.offset_seconds(), &output, f)?;
        self.completion().set_normal(&output, f);
        output.clear(f);
        snapshot.release(self, f);
        zone.release(self, f);
        instant.release(self, f);
        record.release(f);
        Ok(())
    }

    fn emit_temporal_format_exact_offset_seconds(
        &mut self,
        offset: I64Local,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_format_fixed_time_zone_offset(offset, output, f)?;
        let schema = self.runtime_schema();
        let seconds = schema.reserve_i64_local(f);
        offset.load(f);
        f.instruction(&Instruction::I64Const(60));
        f.instruction(&Instruction::I64RemS);
        seconds.store(f);
        seconds.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(0));
        seconds.load(f);
        f.instruction(&Instruction::I64Sub);
        seconds.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        seconds.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let text = schema
            .reserve_gc_local(f)
            .initialize(output.cast_reference::<StringValue>(schema, f), f);
        self.emit_temporal_append_gc_literal(&text, ":", f)?;
        seconds.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        seconds.store(f);
        self.emit_date_append_padded_decimal(&text, seconds, 2, f)?;
        output.set_reference(&text, schema, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i64_local(seconds, f);
        Ok(())
    }
    pub(super) fn emit_temporal_format_fixed_time_zone_offset(
        &mut self,
        offset: I64Local,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let text = schema
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("+", f)?, f);
        let magnitude = schema.reserve_i64_local(f);
        let number = schema.reserve_i64_local(f);
        offset.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, f);
        text.replace(self.emit_interned_string_reference("-", f)?, f);
        f.instruction(&Instruction::I64Const(0));
        offset.load(f);
        f.instruction(&Instruction::I64Sub);
        magnitude.store(f);
        f.instruction(&Instruction::Else);
        offset.load(f);
        magnitude.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        magnitude.load(f);
        f.instruction(&Instruction::I64Const(3600));
        f.instruction(&Instruction::I64DivU);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.store(f);
        self.emit_date_append_padded_decimal(&text, number, 2, f)?;
        self.emit_temporal_append_gc_literal(&text, ":", f)?;
        magnitude.load(f);
        f.instruction(&Instruction::I64Const(60));
        f.instruction(&Instruction::I64DivU);
        f.instruction(&Instruction::I64Const(60));
        f.instruction(&Instruction::I64RemU);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.store(f);
        self.emit_date_append_padded_decimal(&text, number, 2, f)?;
        output.set_reference(&text, schema, f);
        schema.release_i64_local(number, f);
        schema.release_i64_local(magnitude, f);
        text.clear(f);
        Ok(())
    }

    pub(crate) fn emit_temporal_zoned_date_time_offset_nanoseconds(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record = self.emit_temporal_branded_zoned_receiver(f)?;
        let instant = self.emit_temporal_normalized_instant_from_zoned_record(&record, f)?;
        let zone = self.emit_temporal_zone_from_zoned_record(&record, f)?;
        let snapshot = self.emit_temporal_zone_snapshot(&zone, &instant, f)?;
        let offset = schema.reserve_i64_local(f);
        let output = schema.reserve_value_local(f);
        snapshot.offset_seconds().load(f);
        f.instruction(&Instruction::I64Const(NANOSECONDS_PER_SECOND));
        f.instruction(&Instruction::I64Mul);
        offset.store(f);
        self.emit_temporal_integer_number(offset, &output, f);
        self.completion().set_normal(&output, f);
        output.clear(f);
        schema.release_i64_local(offset, f);
        snapshot.release(self, f);
        zone.release(self, f);
        instant.release(self, f);
        record.release(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_zoned_date_time_time_zone_id(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record = self.emit_temporal_zoned_date_time_record_from_receiver(f)?;
        let string = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<TemporalZonedDateTimeObject>()
                .field(TemporalZonedDateTimeObjectSchema::TIME_ZONE)
                .read(&record, schema, f)
                .reference(),
            f,
        );
        self.completion().value().set_reference(&string, schema, f);
        self.completion().set_normal(self.completion().value(), f);
        string.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_zoned_date_time_calendar_id(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let record = self.emit_temporal_zoned_date_time_record_from_receiver(f)?;
        let string = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<TemporalZonedDateTimeObject>()
                .field(TemporalZonedDateTimeObjectSchema::CALENDAR)
                .read(&record, schema, f)
                .reference(),
            f,
        );
        self.completion().value().set_reference(&string, schema, f);
        self.completion().set_normal(self.completion().value(), f);
        string.clear(f);
        record.clear(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_zoned_date_time_iso_field(
        &mut self,
        field: ZonedDateTimeField,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        use super::temporal_plain_date::TemporalPlainDateField as D;
        let schema = self.runtime_schema();
        let record = self.emit_temporal_branded_zoned_receiver(f)?;
        let instant = self.emit_temporal_normalized_instant_from_zoned_record(&record, f)?;
        let zone = self.emit_temporal_zone_from_zoned_record(&record, f)?;
        let snapshot = self.emit_temporal_zone_snapshot(&zone, &instant, f)?;
        let iso = self.emit_temporal_zone_snapshot_iso_record(&snapshot, f)?;
        let calendar = self.emit_temporal_calendar_from_zoned_record(&record, f)?;
        let output = schema.reserve_value_local(f);
        match field {
            ZonedDateTimeField::Hour => {
                self.emit_temporal_integer_number(iso.fields()[3], &output, f)
            }
            ZonedDateTimeField::Minute => {
                self.emit_temporal_integer_number(iso.fields()[4], &output, f)
            }
            ZonedDateTimeField::Second => {
                self.emit_temporal_integer_number(iso.fields()[5], &output, f)
            }
            ZonedDateTimeField::Millisecond => {
                self.emit_temporal_integer_number(iso.fields()[6], &output, f)
            }
            ZonedDateTimeField::Microsecond => {
                self.emit_temporal_integer_number(iso.fields()[7], &output, f)
            }
            ZonedDateTimeField::Nanosecond => {
                self.emit_temporal_integer_number(iso.fields()[8], &output, f)
            }
            ZonedDateTimeField::Era => self.emit_temporal_date_field_value(
                D::Era,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::EraYear => self.emit_temporal_date_field_value(
                D::EraYear,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::Year => self.emit_temporal_date_field_value(
                D::Year,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::Month => self.emit_temporal_date_field_value(
                D::Month,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::MonthCode => self.emit_temporal_date_field_value(
                D::MonthCode,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::Day => self.emit_temporal_date_field_value(
                D::Day,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::DayOfWeek => self.emit_temporal_date_field_value(
                D::DayOfWeek,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::DayOfYear => self.emit_temporal_date_field_value(
                D::DayOfYear,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::WeekOfYear => self.emit_temporal_date_field_value(
                D::WeekOfYear,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::YearOfWeek => self.emit_temporal_date_field_value(
                D::YearOfWeek,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::DaysInWeek => self.emit_temporal_date_field_value(
                D::DaysInWeek,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::DaysInMonth => self.emit_temporal_date_field_value(
                D::DaysInMonth,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::DaysInYear => self.emit_temporal_date_field_value(
                D::DaysInYear,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::MonthsInYear => self.emit_temporal_date_field_value(
                D::MonthsInYear,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
            ZonedDateTimeField::InLeapYear => self.emit_temporal_date_field_value(
                D::InLeapYear,
                &calendar,
                [iso.fields()[0], iso.fields()[1], iso.fields()[2]],
                &output,
                f,
            )?,
        }
        self.completion().set_normal(&output, f);
        output.clear(f);
        calendar.release(self, f);
        iso.release(self, f);
        snapshot.release(self, f);
        zone.release(self, f);
        instant.release(self, f);
        record.release(f);
        Ok(())
    }
    pub(super) fn emit_temporal_zoned_date_time_to_plain(
        &mut self,
        target: TemporalZonedDateTimePlainTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let record = self.emit_temporal_branded_zoned_receiver(f)?;
        let instant = self.emit_temporal_normalized_instant_from_zoned_record(&record, f)?;
        let zone = self.emit_temporal_zone_from_zoned_record(&record, f)?;
        let snapshot = self.emit_temporal_zone_snapshot(&zone, &instant, f)?;
        let iso = self.emit_temporal_zone_snapshot_iso_record(&snapshot, f)?;
        let calendar = self.emit_temporal_calendar_from_zoned_record(&record, f)?;
        match target {
            TemporalZonedDateTimePlainTarget::Date => self.emit_alloc_temporal_plain_date(
                iso.fields()[0],
                iso.fields()[1],
                iso.fields()[2],
                &calendar,
                TemporalPrototypeSource::Intrinsic,
                f,
            )?,
            TemporalZonedDateTimePlainTarget::DateTime => self
                .emit_alloc_temporal_plain_date_time(
                    iso.fields(),
                    &calendar,
                    TemporalPrototypeSource::Intrinsic,
                    f,
                )?,
            TemporalZonedDateTimePlainTarget::Time => self.emit_alloc_temporal_plain_time(
                &Self::temporal_plain_date_time_time_locals(iso.fields()),
                TemporalPrototypeSource::Intrinsic,
                f,
            )?,
        }
        calendar.release(self, f);
        iso.release(self, f);
        snapshot.release(self, f);
        zone.release(self, f);
        instant.release(self, f);
        record.release(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_zoned_date_time_equals(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = self.emit_temporal_branded_zoned_receiver(f)?;
        let input = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        let other = self.emit_temporal_to_zoned_record(&input, f)?;
        let left = self.emit_temporal_normalized_instant_from_zoned_record(&receiver, f)?;
        let right = self.emit_temporal_normalized_instant_from_zoned_record(&other, f)?;
        let lzone = self.emit_temporal_zone_from_zoned_record(&receiver, f)?;
        let rzone = self.emit_temporal_zone_from_zoned_record(&other, f)?;
        let lcal = self.emit_temporal_calendar_from_zoned_record(&receiver, f)?;
        let rcal = self.emit_temporal_calendar_from_zoned_record(&other, f)?;
        let equal = schema.reserve_i32_local(f);
        let comparison = schema.reserve_i32_local(f);
        self.emit_bigint_compare(
            left.epoch_nanoseconds(),
            right.epoch_nanoseconds(),
            comparison,
            f,
        );
        comparison.load(f);
        f.instruction(&Instruction::I32Eqz);
        equal.store(f);
        self.emit_string_payload_equality_i32(
            lzone.primary_identifier(),
            rzone.primary_identifier(),
            f,
        );
        equal.load(f);
        f.instruction(&Instruction::I32And);
        equal.store(f);
        self.emit_string_payload_equality_i32(lcal.identifier(), rcal.identifier(), f);
        equal.load(f);
        f.instruction(&Instruction::I32And);
        equal.store(f);
        self.completion().value().set_boolean(equal, f);
        self.completion().set_normal(self.completion().value(), f);
        schema.release_i32_local(comparison, f);
        schema.release_i32_local(equal, f);
        rcal.release(self, f);
        lcal.release(self, f);
        rzone.release(self, f);
        lzone.release(self, f);
        right.release(self, f);
        left.release(self, f);
        other.release(f);
        input.clear(f);
        receiver.release(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_zoned_date_time_to_instant(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let record = self.emit_temporal_branded_zoned_receiver(f)?;
        let epoch = self.emit_temporal_normalized_instant_from_zoned_record(&record, f)?;
        let value = self.emit_temporal_instant_validated_epoch(epoch.epoch_nanoseconds(), f)?;
        self.emit_alloc_temporal_instant(&value, TemporalPrototypeSource::Intrinsic, f)?;
        value.clear(self, f);
        epoch.release(self, f);
        record.release(f);
        Ok(())
    }
    pub(crate) fn emit_temporal_zoned_date_time_with_time_zone(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let input = self.runtime_schema().reserve_value_local(f);
        let record = self.emit_temporal_branded_zoned_receiver(f)?;
        self.emit_builtin_arg_to_value(0, &input, f);
        let zone = self.emit_temporal_zoned_date_time_time_zone(
            &input,
            TemporalTimeZoneStringGoal::Object,
            f,
        )?;
        let instant = self.emit_temporal_normalized_instant_from_zoned_record(&record, f)?;
        let calendar = self.emit_temporal_calendar_from_zoned_record(&record, f)?;
        self.emit_alloc_temporal_zoned_date_time(
            TemporalZonedAllocationInput::new(&instant, &zone, &calendar),
            TemporalPrototypeSource::Intrinsic,
            f,
        )?;
        calendar.release(self, f);
        instant.release(self, f);
        zone.release(self, f);
        record.release(f);
        input.clear(f);
        Ok(())
    }

    fn emit_temporal_parse_iso_string(
        &mut self,
        string: &GcLocal<StringValue>,
        parse_goal: TemporalIsoParseGoal<'_>,
        function: &mut Function,
    ) -> Result<Option<NormalizedTemporalInstantLocals>, EmitError> {
        let slice_length = self.runtime_schema().reserve_i64_local(function);
        let string_len_local = self.runtime_schema().reserve_i64_local(function);
        let main_end_local = self.runtime_schema().reserve_i64_local(function);
        let cursor_local = self.runtime_schema().reserve_i64_local(function);
        let byte_local = self.runtime_schema().reserve_i64_local(function);
        let valid_local = self.runtime_schema().reserve_i64_local(function);
        let negative_year_local = self.runtime_schema().reserve_i64_local(function);
        let year_local = self.runtime_schema().reserve_i64_local(function);
        let month_local = self.runtime_schema().reserve_i64_local(function);
        let day_local = self.runtime_schema().reserve_i64_local(function);
        let hour_local = self.runtime_schema().reserve_i64_local(function);
        let minute_local = self.runtime_schema().reserve_i64_local(function);
        let second_local = self.runtime_schema().reserve_i64_local(function);
        let fraction_local = self.runtime_schema().reserve_i64_local(function);
        let fraction_digits_local = self.runtime_schema().reserve_i64_local(function);
        let date_separated_local = self.runtime_schema().reserve_i64_local(function);
        let time_separated_local = self.runtime_schema().reserve_i64_local(function);
        let has_minute_local = self.runtime_schema().reserve_i64_local(function);
        let has_second_local = self.runtime_schema().reserve_i64_local(function);
        let has_time_local = self.runtime_schema().reserve_i64_local(function);
        let offset_kind_local = self.runtime_schema().reserve_i64_local(function);
        let offset_sign_local = self.runtime_schema().reserve_i64_local(function);
        let offset_hour_local = self.runtime_schema().reserve_i64_local(function);
        let offset_minute_local = self.runtime_schema().reserve_i64_local(function);
        let offset_second_local = self.runtime_schema().reserve_i64_local(function);
        let offset_has_second_local = self.runtime_schema().reserve_i64_local(function);
        let offset_fraction_local = self.runtime_schema().reserve_i64_local(function);
        let offset_fraction_digits_local = self.runtime_schema().reserve_i64_local(function);
        let maximum_day_local = self.runtime_schema().reserve_i64_local(function);
        let calendar_count_local = self.runtime_schema().reserve_i64_local(function);
        let calendar_critical_local = self.runtime_schema().reserve_i64_local(function);
        let timezone_count_local = self.runtime_schema().reserve_i64_local(function);
        let annotation_start_local = self.runtime_schema().reserve_i64_local(function);
        let annotation_equals_local = self.runtime_schema().reserve_i64_local(function);
        let annotation_critical_local = self.runtime_schema().reserve_i64_local(function);
        let annotation_key_uppercase_local = self.runtime_schema().reserve_i64_local(function);
        let annotation_numeric_timezone_local = self.runtime_schema().reserve_i64_local(function);
        let annotation_colon_count_local = self.runtime_schema().reserve_i64_local(function);
        let time_zone_start_local = self.runtime_schema().reserve_i64_local(function);
        let time_zone_end_local = self.runtime_schema().reserve_i64_local(function);
        let calendar_start_local = self.runtime_schema().reserve_i64_local(function);
        let calendar_end_local = self.runtime_schema().reserve_i64_local(function);
        let time_zone_offset_seconds_local = self.runtime_schema().reserve_i64_local(function);
        let selected_offset_seconds_local = self.runtime_schema().reserve_i64_local(function);
        let selected_offset_subsecond_local = self.runtime_schema().reserve_i64_local(function);
        let offset_matches_time_zone_local = self.runtime_schema().reserve_i64_local(function);
        let days_local = self.runtime_schema().reserve_i64_local(function);
        let era_local = self.runtime_schema().reserve_i64_local(function);
        let adjusted_year_local = self.runtime_schema().reserve_i64_local(function);
        let month_index_local = self.runtime_schema().reserve_i64_local(function);
        let seconds_local = self.runtime_schema().reserve_i64_local(function);
        let subsecond_local = self.runtime_schema().reserve_i64_local(function);
        let parse_locals = [
            slice_length,
            string_len_local,
            main_end_local,
            cursor_local,
            byte_local,
            valid_local,
            negative_year_local,
            year_local,
            month_local,
            day_local,
            hour_local,
            minute_local,
            second_local,
            fraction_local,
            fraction_digits_local,
            date_separated_local,
            time_separated_local,
            has_minute_local,
            has_second_local,
            has_time_local,
            offset_kind_local,
            offset_sign_local,
            offset_hour_local,
            offset_minute_local,
            offset_second_local,
            offset_has_second_local,
            offset_fraction_local,
            offset_fraction_digits_local,
            maximum_day_local,
            calendar_count_local,
            calendar_critical_local,
            timezone_count_local,
            annotation_start_local,
            annotation_equals_local,
            annotation_critical_local,
            annotation_key_uppercase_local,
            annotation_numeric_timezone_local,
            annotation_colon_count_local,
            time_zone_start_local,
            time_zone_end_local,
            calendar_start_local,
            calendar_end_local,
            time_zone_offset_seconds_local,
            selected_offset_seconds_local,
            selected_offset_subsecond_local,
            offset_matches_time_zone_local,
            days_local,
            era_local,
            adjusted_year_local,
            month_index_local,
            seconds_local,
            subsecond_local,
        ];

        self.emit_temporal_string_length(string, string_len_local, function);
        for (local, value) in [
            (cursor_local, 0),
            (valid_local, 1),
            (negative_year_local, 0),
            (date_separated_local, 0),
            (time_separated_local, 0),
            (has_minute_local, 0),
            (has_second_local, 0),
            (has_time_local, 0),
            (offset_kind_local, 0),
            (fraction_local, 0),
            (fraction_digits_local, 0),
            (offset_sign_local, 0),
            (offset_hour_local, 0),
            (offset_minute_local, 0),
            (offset_second_local, 0),
            (offset_has_second_local, 0),
            (offset_fraction_local, 0),
            (offset_fraction_digits_local, 0),
            (calendar_count_local, 0),
            (calendar_critical_local, 0),
            (timezone_count_local, 0),
            (time_zone_start_local, -1),
            (time_zone_end_local, -1),
            (calendar_start_local, -1),
            (calendar_end_local, -1),
            (time_zone_offset_seconds_local, 0),
            (selected_offset_seconds_local, 0),
            (selected_offset_subsecond_local, 0),
            (offset_matches_time_zone_local, 0),
        ] {
            function.instruction(&Instruction::I64Const(value));
            (local).store(function);
        }

        (string_len_local).load(function);
        (main_end_local).store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        (cursor_local).load(function);
        (string_len_local).load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_temporal_load_code_unit(string, cursor_local, byte_local, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'[' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        (cursor_local).load(function);
        (main_end_local).store(function);
        function.instruction(&Instruction::Br(2));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (cursor_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (cursor_local).store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        function.instruction(&Instruction::I64Const(0));
        (cursor_local).store(function);
        self.emit_temporal_load_code_unit(string, cursor_local, byte_local, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'+' as i64));
        function.instruction(&Instruction::I64Eq);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I64ExtendI32U);
        (negative_year_local).store(function);
        function.instruction(&Instruction::I64Const(1));
        (cursor_local).store(function);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            year_local,
            6,
            function,
        );
        (negative_year_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Else);
        (year_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(0));
        (year_local).load(function);
        function.instruction(&Instruction::I64Sub);
        (year_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            year_local,
            4,
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.emit_temporal_peek_byte(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            function,
        );
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (date_separated_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            month_local,
            2,
            function,
        );
        (date_separated_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Else);
        self.emit_temporal_expect_byte(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            b'-',
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            day_local,
            2,
            function,
        );

        (cursor_local).load(function);
        (main_end_local).load(function);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (has_time_local).store(function);
        self.emit_temporal_peek_byte(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            function,
        );
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'T' as i64));
        function.instruction(&Instruction::I64Eq);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b't' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b' ' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_temporal_advance_cursor(cursor_local, function);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            hour_local,
            2,
            function,
        );

        self.emit_temporal_peek_byte_if_available(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            function,
        );
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b':' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (time_separated_local).store(function);
        function.instruction(&Instruction::I64Const(1));
        (has_minute_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        function.instruction(&Instruction::Else);
        self.emit_temporal_byte_is_digit(byte_local, function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (has_minute_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (has_minute_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (minute_local).store(function);
        function.instruction(&Instruction::Else);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            minute_local,
            2,
            function,
        );
        self.emit_temporal_peek_byte_if_available(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            function,
        );
        (time_separated_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_byte_is_digit(byte_local, function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (has_second_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b':' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (has_second_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        (has_second_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (second_local).store(function);
        function.instruction(&Instruction::Else);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            second_local,
            2,
            function,
        );
        self.emit_temporal_parse_optional_fraction(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            fraction_local,
            fraction_digits_local,
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        (cursor_local).load(function);
        (main_end_local).load(function);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_load_code_unit(string, cursor_local, byte_local, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'Z' as i64));
        function.instruction(&Instruction::I64Eq);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'z' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (offset_kind_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        function.instruction(&Instruction::Else);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'+' as i64));
        function.instruction(&Instruction::I64Eq);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(2));
        (offset_kind_local).store(function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::End);
        (offset_sign_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            offset_hour_local,
            2,
            function,
        );
        self.emit_temporal_parse_offset_tail(
            string,
            cursor_local,
            main_end_local,
            byte_local,
            valid_local,
            offset_minute_local,
            offset_second_local,
            offset_has_second_local,
            offset_fraction_local,
            offset_fraction_digits_local,
            function,
        );
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        if matches!(parse_goal, TemporalIsoParseGoal::Instant { .. }) {
            function.instruction(&Instruction::I64Const(0));
            (valid_local).store(function);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        for local in [
            hour_local,
            minute_local,
            second_local,
            fraction_local,
            fraction_digits_local,
        ] {
            function.instruction(&Instruction::I64Const(0));
            (local).store(function);
        }
        if matches!(parse_goal, TemporalIsoParseGoal::Instant { .. }) {
            function.instruction(&Instruction::I64Const(0));
            (valid_local).store(function);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        (cursor_local).load(function);
        (main_end_local).load(function);
        function.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.emit_temporal_validate_annotations(
            string,
            main_end_local,
            string_len_local,
            cursor_local,
            byte_local,
            valid_local,
            calendar_count_local,
            calendar_critical_local,
            timezone_count_local,
            annotation_start_local,
            annotation_equals_local,
            annotation_critical_local,
            annotation_key_uppercase_local,
            annotation_numeric_timezone_local,
            annotation_colon_count_local,
            time_zone_start_local,
            time_zone_end_local,
            calendar_start_local,
            calendar_end_local,
            function,
        );

        self.emit_temporal_validate_date_time(
            year_local,
            month_local,
            day_local,
            hour_local,
            minute_local,
            second_local,
            offset_hour_local,
            offset_minute_local,
            offset_second_local,
            maximum_day_local,
            valid_local,
            function,
        );
        (valid_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            match parse_goal {
                TemporalIsoParseGoal::Instant { .. } => {
                    RuntimeErrorMessage::INVALID_TEMPORAL_INSTANT_STRING
                }
                TemporalIsoParseGoal::TimeZoneIdentifier { .. } => {
                    RuntimeErrorMessage::INVALID_TEMPORAL_TIME_ZONE_IDENTIFIER
                }
                TemporalIsoParseGoal::ZonedDateTimeSyntax { .. }
                | TemporalIsoParseGoal::ZonedDateTime { .. } => {
                    RuntimeErrorMessage::INVALID_TEMPORAL_ZONEDDATETIME_STRING
                }
                TemporalIsoParseGoal::PlainDate { .. } => {
                    RuntimeErrorMessage::INVALID_TEMPORAL_PLAINDATE_STRING
                }
                TemporalIsoParseGoal::PlainDateTime { .. } => {
                    RuntimeErrorMessage::INVALID_TEMPORAL_PLAINDATETIME_STRING
                }
                TemporalIsoParseGoal::PlainTime { .. } => {
                    RuntimeErrorMessage::INVALID_TEMPORAL_PLAINTIME_STRING
                }
            },
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        if let TemporalIsoParseGoal::TimeZoneIdentifier {
            time_zone,
            string_goal,
        } = parse_goal
        {
            // The constructor's `Identifier` goal rejects a date or datetime
            // prefix, even with a valid bracketed zone
            // (`timezone-iso-string.js`): `main_end > 0` means the string had
            // a main section, and the only main sections that survive the
            // shared validity check are dates and datetimes — offsets are
            // filtered before this parser runs, and every other main section
            // fails the date parse. The `Object` goal skips this: an ISO
            // string is a valid time-zone argument everywhere else.
            if matches!(string_goal, TemporalTimeZoneStringGoal::Identifier) {
                (main_end_local).load(function);
                function.instruction(&Instruction::I64Eqz);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_temporal_error_and_return(
                    scriptw_ir::NativeErrorKind::RangeError,
                    RuntimeErrorMessage::INVALID_TEMPORAL_TIME_ZONE_IDENTIFIER,
                    function,
                )?;
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            (timezone_count_local).load(function);
            function.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, function);
            (offset_kind_local).load(function);
            function.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(scriptw_ir::NativeErrorKind::RangeError,RuntimeErrorMessage::TEMPORAL_TIME_ZONE_STRING_REQUIRES_AN_OFFSET_OR_BRACKETED_TIME_ZONE,function)?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            (offset_kind_local).load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            let literal = self.runtime_schema().reserve_gc_local(function).initialize(
                self.emit_interned_string_reference("UTC", function)?,
                function,
            );
            time_zone.set_reference(&literal, self.runtime_schema(), function);
            literal.clear(function);
            function.instruction(&Instruction::Else);
            (offset_has_second_local).load(function);
            function.instruction(&Instruction::I64Eqz);
            (offset_fraction_digits_local).load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32And);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::TEMPORAL_TIME_ZONE_OFFSET_MUST_USE_MINUTE_PRECISION,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            (offset_sign_local).load(function);
            (offset_hour_local).load(function);
            function.instruction(&Instruction::I64Const(3_600));
            function.instruction(&Instruction::I64Mul);
            (offset_minute_local).load(function);
            function.instruction(&Instruction::I64Const(60));
            function.instruction(&Instruction::I64Mul);
            function.instruction(&Instruction::I64Add);
            function.instruction(&Instruction::I64Mul);
            (time_zone_offset_seconds_local).store(function);
            self.emit_temporal_format_fixed_time_zone_offset(
                time_zone_offset_seconds_local,
                time_zone,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            function.instruction(&Instruction::Else);
            (time_zone_end_local).load(function);
            (time_zone_start_local).load(function);
            function.instruction(&Instruction::I64Sub);
            (slice_length).store(function);
            let piece = self.runtime_schema().reserve_gc_local(function).initialize(
                self.emit_temporal_string_slice(
                    string,
                    time_zone_start_local,
                    slice_length,
                    function,
                ),
                function,
            );
            time_zone.set_reference(&piece, self.runtime_schema(), function);
            piece.clear(function);
            let named = self.runtime_schema().reserve_i64_local(function);
            let parsed_zone = self.runtime_schema().reserve_gc_local(function).initialize(
                time_zone.cast_reference::<StringValue>(self.runtime_schema(), function),
                function,
            );
            self.emit_temporal_named_identifier_syntax(&parsed_zone, named, function);
            (named).load(function);
            function.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_fixed_time_zone_offset_seconds(
                &parsed_zone,
                time_zone_offset_seconds_local,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            parsed_zone.clear(function);
            self.runtime_schema().release_i64_local(named, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);

            for local in parse_locals.iter().rev() {
                self.runtime_schema().release_i64_local(*local, function);
            }
            return Ok(None);
        }

        if let TemporalIsoParseGoal::PlainTime {
            hour_destination_local,
            minute_destination_local,
            second_destination_local,
            nanosecond_destination_local,
            calendar_use,
        } = parse_goal
        {
            // `09:00:00Z` and `2019-10-01T09:00:00Z` both name an instant, not
            // a wall-clock time, so the UTC designator is a RangeError.
            // A numeric offset (`offset_kind == 2`) is merely ignored.
            (offset_kind_local).load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::TEMPORAL_PLAINTIME_STRING_MUST_NOT_USE_THE_UTC_DESIGNATOR,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            // A date-only string never gains an implicit midnight.
            (has_time_local).load(function);
            function.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::INVALID_TEMPORAL_PLAINTIME_STRING,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            (timezone_count_local).load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64GtU);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::INVALID_TEMPORAL_PLAINTIME_STRING,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);

            match calendar_use {
                TemporalTimeCalendarUse::Ignore => {}
                TemporalTimeCalendarUse::Resolve { calendar } => self
                    .emit_temporal_iso_calendar_annotation(
                        string,
                        calendar_count_local,
                        calendar_start_local,
                        calendar_end_local,
                        calendar,
                        RuntimeErrorMessage::INVALID_TEMPORAL_TIME_STRING_CALENDAR_ANNOTATION,
                        function,
                    )?,
            }

            // A parsed leap second is clamped, not rejected: `23:59:60` is
            // `23:59:59`.
            (second_local).load(function);
            function.instruction(&Instruction::I64Const(60));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(59));
            (second_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            for (source, destination) in [
                (hour_local, hour_destination_local),
                (minute_local, minute_destination_local),
                (second_local, second_destination_local),
            ] {
                (source).load(function);
                (destination).store(function);
            }
            self.emit_temporal_scale_fraction_to_nanoseconds(
                fraction_local,
                fraction_digits_local,
                function,
            );
            (nanosecond_destination_local).store(function);

            for local in parse_locals.iter().rev() {
                self.runtime_schema().release_i64_local(*local, function);
            }
            return Ok(None);
        }

        if let TemporalIsoParseGoal::PlainDateTime {
            year_destination_local,
            month_destination_local,
            day_destination_local,
            hour_destination_local,
            minute_destination_local,
            second_destination_local,
            nanosecond_destination_local,
            calendar,
        } = parse_goal
        {
            // `2019-10-01T09:00:00Z` names an instant, not a wall-clock
            // date-time, so the UTC designator is a RangeError. A numeric
            // offset (`offset_kind == 2`) is merely ignored.
            (offset_kind_local).load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::TEMPORAL_PLAINDATETIME_STRING_MUST_NOT_USE_THE_UTC_DESIGNATOR,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);

            self.emit_temporal_iso_calendar_annotation(
                string,
                calendar_count_local,
                calendar_start_local,
                calendar_end_local,
                calendar,
                RuntimeErrorMessage::INVALID_TEMPORAL_PLAINDATETIME_CALENDAR_ANNOTATION,
                function,
            )?;

            // A parsed leap second is clamped, not rejected: `23:59:60` is
            // `23:59:59`.
            (second_local).load(function);
            function.instruction(&Instruction::I64Const(60));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(59));
            (second_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);

            for (source, destination) in [
                (year_local, year_destination_local),
                (month_local, month_destination_local),
                (day_local, day_destination_local),
                (hour_local, hour_destination_local),
                (minute_local, minute_destination_local),
                (second_local, second_destination_local),
            ] {
                (source).load(function);
                (destination).store(function);
            }
            self.emit_temporal_scale_fraction_to_nanoseconds(
                fraction_local,
                fraction_digits_local,
                function,
            );
            (nanosecond_destination_local).store(function);

            for local in parse_locals.iter().rev() {
                self.runtime_schema().release_i64_local(*local, function);
            }
            return Ok(None);
        }

        if let TemporalIsoParseGoal::PlainDate {
            year_destination_local,
            month_destination_local,
            day_destination_local,
            calendar,
        } = parse_goal
        {
            // A `PlainDate` has no instant, so the UTC designator is not just
            // redundant, it is forbidden: `2019-10-01T09:00:00Z` must throw.
            // `offset_kind == 1` is the `Z` form; `2` is an explicit numeric
            // offset, which is merely ignored.
            (offset_kind_local).load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::TEMPORAL_PLAINDATE_STRING_MUST_NOT_USE_THE_UTC_DESIGNATOR,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);

            self.emit_temporal_iso_calendar_annotation(
                string,
                calendar_count_local,
                calendar_start_local,
                calendar_end_local,
                calendar,
                RuntimeErrorMessage::INVALID_TEMPORAL_PLAINDATE_CALENDAR_ANNOTATION,
                function,
            )?;

            for (source, destination) in [
                (year_local, year_destination_local),
                (month_local, month_destination_local),
                (day_local, day_destination_local),
            ] {
                (source).load(function);
                (destination).store(function);
            }

            for local in parse_locals.iter().rev() {
                self.runtime_schema().release_i64_local(*local, function);
            }
            return Ok(None);
        }

        if let TemporalIsoParseGoal::ZonedDateTimeSyntax {
            time_zone,
            calendar,
        } = parse_goal
        {
            (timezone_count_local).load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Ne);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::TEMPORAL_ZONEDDATETIME_STRING_REQUIRES_ONE_BRACKETED_TIME_ZONE,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);

            (time_zone_end_local).load(function);
            (time_zone_start_local).load(function);
            function.instruction(&Instruction::I64Sub);
            (slice_length).store(function);
            let piece = self.runtime_schema().reserve_gc_local(function).initialize(
                self.emit_temporal_string_slice(
                    string,
                    time_zone_start_local,
                    slice_length,
                    function,
                ),
                function,
            );
            time_zone.set_reference(&piece, self.runtime_schema(), function);
            piece.clear(function);

            let literal = self.runtime_schema().reserve_gc_local(function).initialize(
                self.emit_interned_string_reference("iso8601", function)?,
                function,
            );
            calendar.set_reference(&literal, self.runtime_schema(), function);
            literal.clear(function);

            (calendar_count_local).load(function);
            function.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::Else);
            (calendar_end_local).load(function);
            (calendar_start_local).load(function);
            function.instruction(&Instruction::I64Sub);
            (slice_length).store(function);
            let piece = self.runtime_schema().reserve_gc_local(function).initialize(
                self.emit_temporal_string_slice(
                    string,
                    calendar_start_local,
                    slice_length,
                    function,
                ),
                function,
            );
            calendar.set_reference(&piece, self.runtime_schema(), function);
            piece.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }

        if matches!(parse_goal, TemporalIsoParseGoal::ZonedDateTimeSyntax { .. }) {
            for local in parse_locals.iter().rev() {
                self.runtime_schema().release_i64_local(*local, function);
            }
            return Ok(None);
        }

        self.emit_temporal_scale_fraction_to_nanoseconds(
            fraction_local,
            fraction_digits_local,
            function,
        );
        (fraction_local).store(function);
        self.emit_temporal_scale_fraction_to_nanoseconds(
            offset_fraction_local,
            offset_fraction_digits_local,
            function,
        );
        (offset_fraction_local).store(function);
        (offset_sign_local).load(function);
        (offset_hour_local).load(function);
        function.instruction(&Instruction::I64Const(3_600));
        function.instruction(&Instruction::I64Mul);
        (offset_minute_local).load(function);
        function.instruction(&Instruction::I64Const(60));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        (offset_second_local).load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Mul);
        (selected_offset_seconds_local).store(function);
        (offset_sign_local).load(function);
        (offset_fraction_local).load(function);
        function.instruction(&Instruction::I64Mul);
        (selected_offset_subsecond_local).store(function);

        self.emit_temporal_days_from_civil(
            year_local,
            month_local,
            day_local,
            adjusted_year_local,
            era_local,
            month_index_local,
            days_local,
            function,
        );
        if let TemporalIsoParseGoal::ZonedDateTime {
            policies,
            zone,
            output,
        } = parse_goal
        {
            let milli = self.runtime_schema().reserve_i64_local(function);
            let micro = self.runtime_schema().reserve_i64_local(function);
            let nano = self.runtime_schema().reserve_i64_local(function);
            let supplied_raw = self.runtime_schema().reserve_i64_local(function);
            for (divisor, modulus, destination) in [
                (1_000_000, 1000, milli),
                (1000, 1000, micro),
                (1, 1000, nano),
            ] {
                (fraction_local).load(function);
                function.instruction(&Instruction::I64Const(divisor));
                function.instruction(&Instruction::I64DivU);
                function.instruction(&Instruction::I64Const(modulus));
                function.instruction(&Instruction::I64RemU);
                (destination).store(function);
            }
            (second_local).load(function);
            function.instruction(&Instruction::I64Const(60));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(59));
            (second_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            let iso = self.emit_temporal_regulated_iso_record(
                &[
                    year_local,
                    month_local,
                    day_local,
                    hour_local,
                    minute_local,
                    second_local,
                    milli,
                    micro,
                    nano,
                ],
                function,
            )?;
            (selected_offset_seconds_local).load(function);
            function.instruction(&Instruction::I64Const(1_000_000_000));
            function.instruction(&Instruction::I64Mul);
            (selected_offset_subsecond_local).load(function);
            function.instruction(&Instruction::I64Add);
            (supplied_raw).store(function);
            let supplied =
                self.emit_temporal_validated_offset_nanoseconds(supplied_raw, function)?;
            let instant = self.emit_temporal_parsed_zoned_epoch_into(
                output,
                &iso,
                zone,
                &supplied,
                policies,
                TemporalZonedParseFlags::String {
                    has_time: has_time_local,
                    offset_kind: offset_kind_local,
                    offset_has_seconds: offset_has_second_local,
                },
                function,
            )?;
            supplied.release(self, function);
            iso.release(self, function);
            for slot in [supplied_raw, nano, micro, milli] {
                self.runtime_schema().release_i64_local(slot, function);
            }
            for local in parse_locals.iter().rev() {
                self.runtime_schema().release_i64_local(*local, function);
            }
            return Ok(Some(instant));
        }

        (days_local).load(function);
        function.instruction(&Instruction::I64Const(SECONDS_PER_DAY));
        function.instruction(&Instruction::I64Mul);
        (hour_local).load(function);
        function.instruction(&Instruction::I64Const(3_600));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        (minute_local).load(function);
        function.instruction(&Instruction::I64Const(60));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        (second_local).load(function);
        function.instruction(&Instruction::I64Const(59));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(59));
        function.instruction(&Instruction::Else);
        (second_local).load(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Add);
        (selected_offset_seconds_local).load(function);
        function.instruction(&Instruction::I64Sub);
        (seconds_local).store(function);
        (fraction_local).load(function);
        (selected_offset_subsecond_local).load(function);
        function.instruction(&Instruction::I64Sub);
        (subsecond_local).store(function);
        self.emit_temporal_normalize_seconds_and_subseconds(
            seconds_local,
            subsecond_local,
            function,
        );
        if let TemporalIsoParseGoal::Instant {
            seconds,
            nanosecond,
        } = parse_goal
        {
            seconds_local.load(function);
            seconds.store(function);
            subsecond_local.load(function);
            nanosecond.store(function);
        }

        for local in parse_locals.iter().rev() {
            self.runtime_schema().release_i64_local(*local, function);
        }
        Ok(None)
    }

    /// Resolves the `[u-ca=...]` annotation an ISO string may carry into a
    /// calendar payload. `emit_temporal_validate_annotations` has already
    /// captured only the FIRST annotation and rejected a repeated-and-critical
    /// pair, which is exactly the split Test262 asks for:
    /// `[u-ca=iso8601][u-ca=discord]` succeeds with the second ignored, while
    /// `[u-ca=iso8601][!u-ca=iso8601]` throws. Do not "fix" that asymmetry.
    ///
    /// The annotation value goes through the same
    /// [`TemporalCalendarId`] table as `CanonicalizeCalendar`, so
    /// `Temporal.PlainDate.from("2000-05-02[u-ca=gregory]").calendarId` is
    /// `"gregory"` and the round trip through `toString` is closed. An
    /// annotation naming no known calendar stays a RangeError, which is what
    /// `withCalendar/calendar-invalid-iso-string.js` pins with
    /// `"1997-12-04[u-ca=notacal]"`.
    #[allow(clippy::too_many_arguments)]
    fn emit_temporal_iso_calendar_annotation(
        &mut self,
        string: &GcLocal<StringValue>,
        count: I64Local,
        start: I64Local,
        end: I64Local,
        output: &ValueLocals,
        error: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let default = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(TemporalCalendarId::DEFAULT.canonical(), function)?,
            function,
        );
        output.set_reference(&default, schema, function);
        default.clear(function);
        count.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Else);
        let length = schema.reserve_i64_local(function);
        end.load(function);
        start.load(function);
        function.instruction(&Instruction::I64Sub);
        length.store(function);
        let annotation = schema.reserve_gc_local(function).initialize(
            self.emit_temporal_string_slice(string, start, length, function),
            function,
        );
        let matched = schema.reserve_i32_local(function);
        let fold = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        matched.store(function);
        function.instruction(&Instruction::I32Const(1));
        fold.store(function);
        for calendar in TemporalCalendarId::ALL {
            for &spelling in calendar.spellings() {
                let expected = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference(spelling, function)?,
                    function,
                );
                self.emit_string_payload_equality_i32_with_ascii_case_folding(
                    &annotation,
                    &expected,
                    Some(fold),
                    function,
                );
                expected.clear(function);
                self.open_frame(ControlFrameKind::If, function);
                let canonical = schema.reserve_gc_local(function).initialize(
                    self.emit_interned_string_reference(calendar.canonical(), function)?,
                    function,
                );
                output.set_reference(&canonical, schema, function);
                canonical.clear(function);
                function.instruction(&Instruction::I32Const(1));
                matched.store(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        matched.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            error,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(fold, function);
        schema.release_i32_local(matched, function);
        annotation.clear(function);
        schema.release_i64_local(length, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// `ParseISODateTime` restricted to the `TemporalDateString` goal. Wraps
    /// the private parser so the `Temporal.PlainDate` emitters can reach it
    /// without the goal enum leaving this module.
    pub(crate) fn emit_temporal_parse_plain_date_string(
        &mut self,
        string: &GcLocal<StringValue>,
        year: I64Local,
        month: I64Local,
        day: I64Local,
        calendar: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_parse_iso_string(
            string,
            TemporalIsoParseGoal::PlainDate {
                year_destination_local: year,
                month_destination_local: month,
                day_destination_local: day,
                calendar,
            },
            function,
        )?;
        Ok(())
    }

    /// `ParseISODateTime` restricted to the `TemporalDateTimeString` goal.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_temporal_parse_plain_date_time_string(
        &mut self,
        string: &GcLocal<StringValue>,
        year: I64Local,
        month: I64Local,
        day: I64Local,
        hour: I64Local,
        minute: I64Local,
        second: I64Local,
        nanosecond: I64Local,
        calendar: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_parse_iso_string(
            string,
            TemporalIsoParseGoal::PlainDateTime {
                year_destination_local: year,
                month_destination_local: month,
                day_destination_local: day,
                hour_destination_local: hour,
                minute_destination_local: minute,
                second_destination_local: second,
                nanosecond_destination_local: nanosecond,
                calendar,
            },
            function,
        )?;
        Ok(())
    }

    /// `ParseTemporalTimeString`. A bare time (`15:23`, `T15:23`,
    /// `152330-0800`) is rewritten as `0000-01-01T` plus the same tail so the
    /// one ISO parser can serve both spellings; a string that already carries
    /// a date is passed through untouched.
    ///
    /// The `AmbiguousTemporalTimeString` rules are checked here, before the
    /// rewrite, because `1214` is a legal `MMDD` date *and* a legal `HHMM`
    /// time, and the proposal resolves that tie by demanding the `T`
    /// designator. `1232` is not ambiguous — there is no 32nd day — so it
    /// stays a time.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_temporal_parse_plain_time_string(
        &mut self,
        string: &GcLocal<StringValue>,
        hour_destination_local: I64Local,
        minute_destination_local: I64Local,
        second_destination_local: I64Local,
        nanosecond_destination_local: I64Local,
        calendar_use: TemporalTimeCalendarUse<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let length_local = self.runtime_schema().reserve_i64_local(function);
        let main_end_local = self.runtime_schema().reserve_i64_local(function);
        let cursor_local = self.runtime_schema().reserve_i64_local(function);
        let byte_local = self.runtime_schema().reserve_i64_local(function);
        let date_form_local = self.runtime_schema().reserve_i64_local(function);
        let designated_local = self.runtime_schema().reserve_i64_local(function);
        let digits_local = self.runtime_schema().reserve_i64_local(function);
        let month_local = self.runtime_schema().reserve_i64_local(function);
        let day_local = self.runtime_schema().reserve_i64_local(function);
        let maximum_day_local = self.runtime_schema().reserve_i64_local(function);

        self.emit_temporal_string_length(string, length_local, function);
        let rewritten_local = self
            .runtime_schema()
            .reserve_gc_local(function)
            .initialize(string.load(self.runtime_schema(), function), function);
        let slice_length = self.runtime_schema().reserve_i64_local(function);

        // `main_end` is the first annotation bracket, or the whole string.
        (length_local).load(function);
        (main_end_local).store(function);
        function.instruction(&Instruction::I64Const(0));
        (cursor_local).store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        (cursor_local).load(function);
        (length_local).load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_temporal_load_code_unit(string, cursor_local, byte_local, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'[' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        (cursor_local).load(function);
        (main_end_local).store(function);
        function.instruction(&Instruction::Br(2));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_temporal_advance_cursor(cursor_local, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        // A date-shaped head is one of `±YYYYYY…`, `YYYY-MM-DD…` or eight
        // digits followed by a date/time separator. Everything else is a bare
        // time.
        function.instruction(&Instruction::I64Const(0));
        (date_form_local).store(function);
        function.instruction(&Instruction::I64Const(0));
        (designated_local).store(function);
        (main_end_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_load_byte_at_index(string, 0, cursor_local, byte_local, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'+' as i64));
        function.instruction(&Instruction::I64Eq);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (date_form_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'T' as i64));
        function.instruction(&Instruction::I64Eq);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b't' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (designated_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        (main_end_local).load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_load_byte_at_index(string, 4, cursor_local, byte_local, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I64ExtendI32U);
        (digits_local).store(function);
        self.emit_temporal_load_byte_at_index(string, 7, cursor_local, byte_local, function);
        (digits_local).load(function);
        function.instruction(&Instruction::I32WrapI64);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (date_form_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        (main_end_local).load(function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (digits_local).store(function);
        for index in 0..8 {
            self.emit_temporal_load_byte_at_index(
                string,
                index,
                cursor_local,
                byte_local,
                function,
            );
            self.emit_temporal_byte_is_digit(byte_local, function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(0));
            (digits_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        (main_end_local).load(function);
        function.instruction(&Instruction::I64Const(8));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::Else);
        self.emit_temporal_load_byte_at_index(string, 8, cursor_local, byte_local, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'T' as i64));
        function.instruction(&Instruction::I64Eq);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b't' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b' ' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::End);
        (digits_local).load(function);
        function.instruction(&Instruction::I32WrapI64);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (date_form_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        (date_form_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);

        // `AmbiguousTemporalTimeString`: only an undesignated bare time can
        // collide with a `MM-DD` or `YYYY-MM` date.
        (designated_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        // (length, month digit indices, day digit indices, separator index)
        for (length, month_indices, day_indices, separator) in [
            (4_i64, [0_i64, 1_i64], Some([2_i64, 3_i64]), None),
            (5, [0, 1], Some([3, 4]), Some(2_i64)),
            (6, [4, 5], None, None),
            (7, [5, 6], None, Some(4)),
        ] {
            (main_end_local).load(function);
            function.instruction(&Instruction::I64Const(length));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(1));
            (digits_local).store(function);
            for index in 0..length {
                if Some(index) == separator {
                    self.emit_temporal_load_byte_at_index(
                        string,
                        index,
                        cursor_local,
                        byte_local,
                        function,
                    );
                    (byte_local).load(function);
                    function.instruction(&Instruction::I64Const(b'-' as i64));
                    function.instruction(&Instruction::I64Ne);
                    self.open_frame(ControlFrameKind::If, function);
                    function.instruction(&Instruction::I64Const(0));
                    (digits_local).store(function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                    continue;
                }
                self.emit_temporal_load_byte_at_index(
                    string,
                    index,
                    cursor_local,
                    byte_local,
                    function,
                );
                self.emit_temporal_byte_is_digit(byte_local, function);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                function.instruction(&Instruction::I64Const(0));
                (digits_local).store(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            self.emit_temporal_two_digit_value(
                string,
                month_indices,
                cursor_local,
                byte_local,
                month_local,
                function,
            );
            match day_indices {
                Some(indices) => self.emit_temporal_two_digit_value(
                    string,
                    indices,
                    cursor_local,
                    byte_local,
                    day_local,
                    function,
                ),
                // A `YYYY-MM` collision has no day component; 1 is always in
                // range for a valid month.
                None => {
                    function.instruction(&Instruction::I64Const(1));
                    (day_local).store(function);
                }
            }
            // February is treated as 29 days: `0229` is ambiguous even though
            // the year is unknown.
            function.instruction(&Instruction::I64Const(31));
            (maximum_day_local).store(function);
            for (month, days) in [(4_i64, 30_i64), (6, 30), (9, 30), (11, 30), (2, 29)] {
                (month_local).load(function);
                function.instruction(&Instruction::I64Const(month));
                function.instruction(&Instruction::I64Eq);
                self.open_frame(ControlFrameKind::If, function);
                function.instruction(&Instruction::I64Const(days));
                (maximum_day_local).store(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
            (digits_local).load(function);
            function.instruction(&Instruction::I64Eqz);
            function.instruction(&Instruction::I32Eqz);
            (month_local).load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64GeS);
            function.instruction(&Instruction::I32And);
            (month_local).load(function);
            function.instruction(&Instruction::I64Const(12));
            function.instruction(&Instruction::I64LeS);
            function.instruction(&Instruction::I32And);
            (day_local).load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64GeS);
            function.instruction(&Instruction::I32And);
            (day_local).load(function);
            (maximum_day_local).load(function);
            function.instruction(&Instruction::I64LeS);
            function.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::AMBIGUOUS_TEMPORAL_PLAINTIME_STRING_REQUIRES_THE_T_DESIGNATOR,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // Rewrite: `0000-01-01T` + the tail, minus any `T` designator.
        (length_local).load(function);
        (designated_local).load(function);
        function.instruction(&Instruction::I64Sub);
        (slice_length).store(function);
        let piece = self.runtime_schema().reserve_gc_local(function).initialize(
            self.emit_temporal_string_slice(string, designated_local, slice_length, function),
            function,
        );
        let prefix = self.runtime_schema().reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("0000-01-01T", function)?,
            function,
        );
        rewritten_local.replace(
            self.emit_concat_gc_strings(&prefix, &piece, function),
            function,
        );
        prefix.clear(function);
        piece.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        self.emit_temporal_parse_iso_string(
            &rewritten_local,
            TemporalIsoParseGoal::PlainTime {
                hour_destination_local,
                minute_destination_local,
                second_destination_local,
                nanosecond_destination_local,
                calendar_use,
            },
            function,
        )?;

        for local in [
            maximum_day_local,
            day_local,
            month_local,
            digits_local,
            designated_local,
            date_form_local,
            byte_local,
            cursor_local,
            main_end_local,
            length_local,
        ] {
            self.runtime_schema().release_i64_local(local, function);
        }
        self.runtime_schema()
            .release_i64_local(slice_length, function);
        rewritten_local.clear(function);
        Ok(())
    }

    /// Loads the byte at a compile-time-known index, reusing `index_local` as
    /// the scratch cursor the byte loader wants.
    fn emit_temporal_load_byte_at_index(
        &mut self,
        string: &GcLocal<StringValue>,
        index: i64,
        index_local: I64Local,
        byte_local: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I64Const(index));
        (index_local).store(function);
        self.emit_temporal_load_code_unit(string, index_local, byte_local, function);
    }

    /// Two ASCII digits at fixed indices, read as a decimal number. The caller
    /// has already checked that both are digits.
    fn emit_temporal_two_digit_value(
        &mut self,
        string: &GcLocal<StringValue>,
        indices: [i64; 2],
        index_local: I64Local,
        byte_local: I64Local,
        output_local: I64Local,
        function: &mut Function,
    ) {
        self.emit_temporal_load_byte_at_index(
            string,
            indices[0],
            index_local,
            byte_local,
            function,
        );
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64Mul);
        (output_local).store(function);
        self.emit_temporal_load_byte_at_index(
            string,
            indices[1],
            index_local,
            byte_local,
            function,
        );
        (output_local).load(function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Add);
        (output_local).store(function);
    }

    pub(super) fn emit_temporal_advance_cursor(
        &mut self,
        cursor_local: I64Local,
        function: &mut Function,
    ) {
        (cursor_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (cursor_local).store(function);
    }

    pub(super) fn emit_temporal_byte_is_digit(
        &mut self,
        byte_local: I64Local,
        function: &mut Function,
    ) {
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64GeU);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'9' as i64));
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32And);
    }

    fn emit_temporal_peek_byte(
        &mut self,
        string: &GcLocal<StringValue>,
        cursor_local: I64Local,
        end_local: I64Local,
        byte_local: I64Local,
        valid_local: I64Local,
        function: &mut Function,
    ) {
        (cursor_local).load(function);
        (end_local).load(function);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_load_code_unit(string, cursor_local, byte_local, function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        (byte_local).store(function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    fn emit_temporal_peek_byte_if_available(
        &mut self,
        string: &GcLocal<StringValue>,
        cursor_local: I64Local,
        end_local: I64Local,
        byte_local: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I64Const(0));
        (byte_local).store(function);
        (cursor_local).load(function);
        (end_local).load(function);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_load_code_unit(string, cursor_local, byte_local, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_temporal_expect_byte(
        &mut self,
        string: &GcLocal<StringValue>,
        cursor_local: I64Local,
        end_local: I64Local,
        byte_local: I64Local,
        valid_local: I64Local,
        expected: u8,
        function: &mut Function,
    ) {
        self.emit_temporal_peek_byte(
            string,
            cursor_local,
            end_local,
            byte_local,
            valid_local,
            function,
        );
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(expected as i64));
        function.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_temporal_advance_cursor(cursor_local, function);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_temporal_parse_fixed_decimal(
        &mut self,
        string: &GcLocal<StringValue>,
        cursor_local: I64Local,
        end_local: I64Local,
        byte_local: I64Local,
        valid_local: I64Local,
        destination_local: I64Local,
        width: u32,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I64Const(0));
        (destination_local).store(function);
        for _ in 0..width {
            self.emit_temporal_peek_byte(
                string,
                cursor_local,
                end_local,
                byte_local,
                valid_local,
                function,
            );
            self.emit_temporal_byte_is_digit(byte_local, function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(0));
            (valid_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            (destination_local).load(function);
            function.instruction(&Instruction::I64Const(10));
            function.instruction(&Instruction::I64Mul);
            (byte_local).load(function);
            function.instruction(&Instruction::I64Const(b'0' as i64));
            function.instruction(&Instruction::I64Sub);
            function.instruction(&Instruction::I64Add);
            (destination_local).store(function);
            self.emit_temporal_advance_cursor(cursor_local, function);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_temporal_parse_optional_fraction(
        &mut self,
        string: &GcLocal<StringValue>,
        cursor_local: I64Local,
        end_local: I64Local,
        byte_local: I64Local,
        valid_local: I64Local,
        fraction_local: I64Local,
        fraction_digits_local: I64Local,
        function: &mut Function,
    ) {
        self.emit_temporal_peek_byte_if_available(
            string,
            cursor_local,
            end_local,
            byte_local,
            function,
        );
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'.' as i64));
        function.instruction(&Instruction::I64Eq);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b',' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        (cursor_local).load(function);
        (end_local).load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_temporal_load_code_unit(string, cursor_local, byte_local, function);
        self.emit_temporal_byte_is_digit(byte_local, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        (fraction_digits_local).load(function);
        function.instruction(&Instruction::I64Const(9));
        function.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        function.instruction(&Instruction::Else);
        (fraction_local).load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64Mul);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'0' as i64));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Add);
        (fraction_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (fraction_digits_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (fraction_digits_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        (fraction_digits_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    /// Parse UTCOffset at the field-conversion boundary, before later getters.
    /// Unlike a time-zone identifier, this grammar allows seconds and fractions.
    pub(super) fn emit_temporal_utc_offset_nanoseconds(
        &mut self,
        string: &GcLocal<StringValue>,
        nanoseconds_local: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let length_local = self.runtime_schema().reserve_i64_local(function);
        let cursor_local = self.runtime_schema().reserve_i64_local(function);
        let byte_local = self.runtime_schema().reserve_i64_local(function);
        let valid_local = self.runtime_schema().reserve_i64_local(function);
        let sign_local = self.runtime_schema().reserve_i64_local(function);
        let hour_local = self.runtime_schema().reserve_i64_local(function);
        let minute_local = self.runtime_schema().reserve_i64_local(function);
        let second_local = self.runtime_schema().reserve_i64_local(function);
        let has_second_local = self.runtime_schema().reserve_i64_local(function);
        let fraction_local = self.runtime_schema().reserve_i64_local(function);
        let fraction_digits_local = self.runtime_schema().reserve_i64_local(function);
        self.emit_temporal_string_length(string, length_local, function);
        function.instruction(&Instruction::I64Const(0));
        (cursor_local).store(function);
        function.instruction(&Instruction::I64Const(0));
        (hour_local).store(function);
        function.instruction(&Instruction::I64Const(0));
        (minute_local).store(function);
        function.instruction(&Instruction::I64Const(0));
        (second_local).store(function);
        function.instruction(&Instruction::I64Const(0));
        (fraction_local).store(function);
        function.instruction(&Instruction::I64Const(0));
        (fraction_digits_local).store(function);
        self.emit_temporal_peek_byte_if_available(
            string,
            cursor_local,
            length_local,
            byte_local,
            function,
        );
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'+' as i64));
        function.instruction(&Instruction::I64Eq);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I64ExtendI32U);
        (valid_local).store(function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::End);
        (sign_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            length_local,
            byte_local,
            valid_local,
            hour_local,
            2,
            function,
        );
        self.emit_temporal_parse_offset_tail(
            string,
            cursor_local,
            length_local,
            byte_local,
            valid_local,
            minute_local,
            second_local,
            has_second_local,
            fraction_local,
            fraction_digits_local,
            function,
        );
        (valid_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        (cursor_local).load(function);
        (length_local).load(function);
        function.instruction(&Instruction::I64Ne);
        function.instruction(&Instruction::I32Or);
        (hour_local).load(function);
        function.instruction(&Instruction::I64Const(23));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        (minute_local).load(function);
        function.instruction(&Instruction::I64Const(59));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        (second_local).load(function);
        function.instruction(&Instruction::I64Const(59));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::INVALID_TEMPORAL_ZONEDDATETIME_STRING,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // The shared parser stores unscaled decimal fraction digits.
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        (fraction_digits_local).load(function);
        function.instruction(&Instruction::I64Const(9));
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        (fraction_local).load(function);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64Mul);
        (fraction_local).store(function);
        (fraction_digits_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (fraction_digits_local).store(function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        (hour_local).load(function);
        function.instruction(&Instruction::I64Const(3600));
        function.instruction(&Instruction::I64Mul);
        (minute_local).load(function);
        function.instruction(&Instruction::I64Const(60));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        (second_local).load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1_000_000_000));
        function.instruction(&Instruction::I64Mul);
        (fraction_local).load(function);
        function.instruction(&Instruction::I64Add);
        (sign_local).load(function);
        function.instruction(&Instruction::I64Mul);
        (nanoseconds_local).store(function);
        self.runtime_schema()
            .release_i64_local(fraction_digits_local, function);
        self.runtime_schema()
            .release_i64_local(fraction_local, function);
        self.runtime_schema()
            .release_i64_local(has_second_local, function);
        self.runtime_schema()
            .release_i64_local(second_local, function);
        self.runtime_schema()
            .release_i64_local(minute_local, function);
        self.runtime_schema()
            .release_i64_local(hour_local, function);
        self.runtime_schema()
            .release_i64_local(sign_local, function);
        self.runtime_schema()
            .release_i64_local(valid_local, function);
        self.runtime_schema()
            .release_i64_local(byte_local, function);
        self.runtime_schema()
            .release_i64_local(cursor_local, function);
        self.runtime_schema()
            .release_i64_local(length_local, function);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_temporal_parse_offset_tail(
        &mut self,
        string: &GcLocal<StringValue>,
        cursor_local: I64Local,
        end_local: I64Local,
        byte_local: I64Local,
        valid_local: I64Local,
        minute_local: I64Local,
        second_local: I64Local,
        has_second_local: I64Local,
        fraction_local: I64Local,
        fraction_digits_local: I64Local,
        function: &mut Function,
    ) {
        let separated_local = self.runtime_schema().reserve_i64_local(function);
        let has_minute_local = self.runtime_schema().reserve_i64_local(function);
        for local in [separated_local, has_minute_local, has_second_local] {
            function.instruction(&Instruction::I64Const(0));
            (local).store(function);
        }
        self.emit_temporal_peek_byte_if_available(
            string,
            cursor_local,
            end_local,
            byte_local,
            function,
        );
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b':' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (separated_local).store(function);
        function.instruction(&Instruction::I64Const(1));
        (has_minute_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        function.instruction(&Instruction::Else);
        self.emit_temporal_byte_is_digit(byte_local, function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (has_minute_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (has_minute_local).load(function);
        function.instruction(&Instruction::I32WrapI64);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            end_local,
            byte_local,
            valid_local,
            minute_local,
            2,
            function,
        );
        self.emit_temporal_peek_byte_if_available(
            string,
            cursor_local,
            end_local,
            byte_local,
            function,
        );
        (separated_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_byte_is_digit(byte_local, function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (has_second_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b':' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (has_second_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (has_second_local).load(function);
        function.instruction(&Instruction::I32WrapI64);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_parse_fixed_decimal(
            string,
            cursor_local,
            end_local,
            byte_local,
            valid_local,
            second_local,
            2,
            function,
        );
        self.emit_temporal_parse_optional_fraction(
            string,
            cursor_local,
            end_local,
            byte_local,
            valid_local,
            fraction_local,
            fraction_digits_local,
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.runtime_schema()
            .release_i64_local(has_minute_local, function);
        self.runtime_schema()
            .release_i64_local(separated_local, function);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_temporal_validate_annotations(
        &mut self,
        string: &GcLocal<StringValue>,
        main_end_local: I64Local,
        string_len_local: I64Local,
        cursor_local: I64Local,
        byte_local: I64Local,
        valid_local: I64Local,
        calendar_count_local: I64Local,
        calendar_critical_local: I64Local,
        timezone_count_local: I64Local,
        annotation_start_local: I64Local,
        annotation_equals_local: I64Local,
        annotation_critical_local: I64Local,
        annotation_key_uppercase_local: I64Local,
        annotation_numeric_timezone_local: I64Local,
        annotation_colon_count_local: I64Local,
        time_zone_start_local: I64Local,
        time_zone_end_local: I64Local,
        calendar_start_local: I64Local,
        calendar_end_local: I64Local,
        function: &mut Function,
    ) {
        let annotation_index = self.runtime_schema().reserve_i64_local(function);
        let annotation_end_local = self.runtime_schema().reserve_i64_local(function);
        let key_is_calendar_local = self.runtime_schema().reserve_i64_local(function);
        (main_end_local).load(function);
        (cursor_local).store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        (cursor_local).load(function);
        (string_len_local).load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.emit_temporal_expect_byte(
            string,
            cursor_local,
            string_len_local,
            byte_local,
            valid_local,
            b'[',
            function,
        );
        for (local, value) in [
            (annotation_critical_local, 0),
            (annotation_key_uppercase_local, 0),
            (annotation_numeric_timezone_local, 0),
            (annotation_colon_count_local, 0),
            (annotation_equals_local, -1),
        ] {
            function.instruction(&Instruction::I64Const(value));
            (local).store(function);
        }
        self.emit_temporal_peek_byte(
            string,
            cursor_local,
            string_len_local,
            byte_local,
            valid_local,
            function,
        );
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'!' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (annotation_critical_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (cursor_local).load(function);
        (annotation_start_local).store(function);
        (cursor_local).load(function);
        (annotation_end_local).store(function);
        self.open_frame(ControlFrameKind::Block, function);
        self.open_frame(ControlFrameKind::Loop, function);
        (cursor_local).load(function);
        (string_len_local).load(function);
        function.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        function.instruction(&Instruction::Br(2));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_temporal_load_code_unit(string, cursor_local, byte_local, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b']' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        (cursor_local).load(function);
        (annotation_end_local).store(function);
        self.emit_temporal_advance_cursor(cursor_local, function);
        function.instruction(&Instruction::Br(2));
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'[' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (annotation_equals_local).load(function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'=' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        (cursor_local).load(function);
        (annotation_equals_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'A' as i64));
        function.instruction(&Instruction::I64GeU);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'Z' as i64));
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(1));
        (annotation_key_uppercase_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b':' as i64));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        (annotation_colon_count_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (annotation_colon_count_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_temporal_advance_cursor(cursor_local, function);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        (annotation_end_local).load(function);
        (annotation_start_local).load(function);
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        (annotation_equals_local).load(function);
        function.instruction(&Instruction::I64Const(-1));
        function.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, function);
        (annotation_key_uppercase_local).load(function);
        function.instruction(&Instruction::I32WrapI64);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(0));
        (key_is_calendar_local).store(function);
        (annotation_equals_local).load(function);
        (annotation_start_local).load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        for (offset, expected) in [(0, b'u'), (1, b'-'), (2, b'c'), (3, b'a')] {
            (annotation_start_local).load(function);
            function.instruction(&Instruction::I64Const(offset));
            function.instruction(&Instruction::I64Add);
            (annotation_index).store(function);
            self.emit_temporal_load_code_unit(string, annotation_index, byte_local, function);
            // emit_load_string_byte consumes a local index, so materialize it.
            (byte_local).load(function);
            function.instruction(&Instruction::I64Const(expected as i64));
            function.instruction(&Instruction::I64Eq);
            if offset == 0 {
                function.instruction(&Instruction::I64ExtendI32U);
                (key_is_calendar_local).store(function);
            } else {
                (key_is_calendar_local).load(function);
                function.instruction(&Instruction::I32WrapI64);
                function.instruction(&Instruction::I32And);
                function.instruction(&Instruction::I64ExtendI32U);
                (key_is_calendar_local).store(function);
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (key_is_calendar_local).load(function);
        function.instruction(&Instruction::I32WrapI64);
        self.open_frame(ControlFrameKind::If, function);
        (calendar_count_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        (annotation_equals_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (calendar_start_local).store(function);
        (annotation_end_local).load(function);
        (calendar_end_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (calendar_count_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (calendar_count_local).store(function);
        (calendar_critical_local).load(function);
        (annotation_critical_local).load(function);
        function.instruction(&Instruction::I64Or);
        (calendar_critical_local).store(function);
        (calendar_count_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64GtU);
        (calendar_critical_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        (annotation_critical_local).load(function);
        function.instruction(&Instruction::I32WrapI64);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        (timezone_count_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        (annotation_start_local).load(function);
        (time_zone_start_local).store(function);
        (annotation_end_local).load(function);
        (time_zone_end_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (timezone_count_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (timezone_count_local).store(function);
        (timezone_count_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_temporal_load_code_unit(string, annotation_start_local, byte_local, function);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'+' as i64));
        function.instruction(&Instruction::I64Eq);
        (byte_local).load(function);
        function.instruction(&Instruction::I64Const(b'-' as i64));
        function.instruction(&Instruction::I64Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        // `TimeZoneNumericUTCOffset` is `±HH`, `±HHMM` or `±HH:MM`
        // (lengths 3, 5 and 6); anything with seconds stays invalid.
        for expected in [3, 5, 6] {
            (annotation_end_local).load(function);
            (annotation_start_local).load(function);
            function.instruction(&Instruction::I64Sub);
            function.instruction(&Instruction::I64Const(expected));
            function.instruction(&Instruction::I64Eq);
            if expected != 3 {
                function.instruction(&Instruction::I32Or);
            }
        }
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Br(0));
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.runtime_schema()
            .release_i64_local(key_is_calendar_local, function);
        self.runtime_schema()
            .release_i64_local(annotation_end_local, function);
        self.runtime_schema()
            .release_i64_local(annotation_index, function);
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_temporal_validate_date_time(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        hour_local: I64Local,
        minute_local: I64Local,
        second_local: I64Local,
        offset_hour_local: I64Local,
        offset_minute_local: I64Local,
        offset_second_local: I64Local,
        maximum_day_local: I64Local,
        valid_local: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I64Const(31));
        (maximum_day_local).store(function);
        for month in [4_i64, 6, 9, 11] {
            (month_local).load(function);
            function.instruction(&Instruction::I64Const(month));
            function.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(30));
            (maximum_day_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Eqz);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(100));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        (year_local).load(function);
        function.instruction(&Instruction::I64Const(400));
        function.instruction(&Instruction::I64RemS);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(29));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(28));
        function.instruction(&Instruction::End);
        (maximum_day_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        for (local, minimum, maximum) in [
            (month_local, 1_i64, 12_i64),
            (day_local, 1, 31),
            (hour_local, 0, 23),
            (minute_local, 0, 59),
            (second_local, 0, 60),
            (offset_hour_local, 0, 23),
            (offset_minute_local, 0, 59),
            (offset_second_local, 0, 59),
        ] {
            (local).load(function);
            function.instruction(&Instruction::I64Const(minimum));
            function.instruction(&Instruction::I64LtS);
            (local).load(function);
            function.instruction(&Instruction::I64Const(maximum));
            function.instruction(&Instruction::I64GtS);
            function.instruction(&Instruction::I32Or);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(0));
            (valid_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        (day_local).load(function);
        (maximum_day_local).load(function);
        function.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        (valid_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    fn emit_temporal_scale_fraction_to_nanoseconds(
        &mut self,
        fraction_local: I64Local,
        digit_count_local: I64Local,
        function: &mut Function,
    ) {
        let counter_local = self.runtime_schema().reserve_i64_local(function);
        (digit_count_local).load(function);
        (counter_local).store(function);
        for _ in 0..9 {
            (counter_local).load(function);
            function.instruction(&Instruction::I64Const(9));
            function.instruction(&Instruction::I64LtU);
            self.open_frame(ControlFrameKind::If, function);
            (fraction_local).load(function);
            function.instruction(&Instruction::I64Const(10));
            function.instruction(&Instruction::I64Mul);
            (fraction_local).store(function);
            (counter_local).load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Add);
            (counter_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        (fraction_local).load(function);
        self.runtime_schema()
            .release_i64_local(counter_local, function);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_temporal_days_from_civil(
        &mut self,
        year_local: I64Local,
        month_local: I64Local,
        day_local: I64Local,
        adjusted_year_local: I64Local,
        era_local: I64Local,
        month_index_local: I64Local,
        days_local: I64Local,
        function: &mut Function,
    ) {
        (year_local).load(function);
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64LeS);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Sub);
        (adjusted_year_local).store(function);
        (adjusted_year_local).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        (adjusted_year_local).load(function);
        function.instruction(&Instruction::I64Const(399));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::Else);
        (adjusted_year_local).load(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Const(400));
        function.instruction(&Instruction::I64DivS);
        (era_local).store(function);
        (month_local).load(function);
        (month_local).load(function);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64GtS);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        function.instruction(&Instruction::I64Const(3));
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(-9));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I64Sub);
        (month_index_local).store(function);
        (era_local).load(function);
        function.instruction(&Instruction::I64Const(146_097));
        function.instruction(&Instruction::I64Mul);
        (adjusted_year_local).load(function);
        (era_local).load(function);
        function.instruction(&Instruction::I64Const(400));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Sub);
        (days_local).store(function);
        (days_local).load(function);
        function.instruction(&Instruction::I64Const(365));
        function.instruction(&Instruction::I64Mul);
        (days_local).load(function);
        function.instruction(&Instruction::I64Const(4));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Add);
        (days_local).load(function);
        function.instruction(&Instruction::I64Const(100));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Sub);
        (month_index_local).load(function);
        function.instruction(&Instruction::I64Const(153));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Const(2));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(5));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Add);
        (day_local).load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(719_468));
        function.instruction(&Instruction::I64Sub);
        (days_local).store(function);
    }

    pub(super) fn emit_temporal_normalize_seconds_and_subseconds(
        &mut self,
        seconds_local: I64Local,
        subsecond_local: I64Local,
        function: &mut Function,
    ) {
        (subsecond_local).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, function);
        (subsecond_local).load(function);
        function.instruction(&Instruction::I64Const(NANOSECONDS_PER_SECOND));
        function.instruction(&Instruction::I64Add);
        (subsecond_local).store(function);
        (seconds_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        (seconds_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (subsecond_local).load(function);
        function.instruction(&Instruction::I64Const(NANOSECONDS_PER_SECOND));
        function.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, function);
        (subsecond_local).load(function);
        function.instruction(&Instruction::I64Const(NANOSECONDS_PER_SECOND));
        function.instruction(&Instruction::I64Sub);
        (subsecond_local).store(function);
        (seconds_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (seconds_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_temporal_epoch_nanoseconds_bigint(
        &mut self,
        seconds_local: I64Local,
        subsecond_local: I64Local,
        function: &mut Function,
    ) -> GcLocal<BigIntValue> {
        let negative_local = self.runtime_schema().reserve_i64_local(function);
        let magnitude_seconds_local = self.runtime_schema().reserve_i64_local(function);
        let magnitude_subsecond_local = self.runtime_schema().reserve_i64_local(function);
        let low_word_local = self.runtime_schema().reserve_i64_local(function);
        let low_product_local = self.runtime_schema().reserve_i64_local(function);
        let high_product_local = self.runtime_schema().reserve_i64_local(function);
        let low_limb_local = self.runtime_schema().reserve_i64_local(function);
        let high_limb_local = self.runtime_schema().reserve_i64_local(function);

        (seconds_local).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        function.instruction(&Instruction::I64ExtendI32U);
        (negative_local).store(function);
        (negative_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        (seconds_local).load(function);
        (magnitude_seconds_local).store(function);
        (subsecond_local).load(function);
        (magnitude_subsecond_local).store(function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        (seconds_local).load(function);
        function.instruction(&Instruction::I64Sub);
        (magnitude_seconds_local).store(function);
        function.instruction(&Instruction::I64Const(0));
        (magnitude_subsecond_local).store(function);
        (subsecond_local).load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Else);
        (magnitude_seconds_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        (magnitude_seconds_local).store(function);
        function.instruction(&Instruction::I64Const(NANOSECONDS_PER_SECOND));
        (subsecond_local).load(function);
        function.instruction(&Instruction::I64Sub);
        (magnitude_subsecond_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        (magnitude_seconds_local).load(function);
        function.instruction(&Instruction::I64Const(u32::MAX as i64));
        function.instruction(&Instruction::I64And);
        (low_word_local).store(function);
        (low_word_local).load(function);
        function.instruction(&Instruction::I64Const(NANOSECONDS_PER_SECOND));
        function.instruction(&Instruction::I64Mul);
        (low_product_local).store(function);
        (magnitude_seconds_local).load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(NANOSECONDS_PER_SECOND));
        function.instruction(&Instruction::I64Mul);
        (high_product_local).store(function);
        (low_product_local).load(function);
        (high_product_local).load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64Shl);
        function.instruction(&Instruction::I64Add);
        (low_limb_local).store(function);
        (high_product_local).load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        (low_limb_local).load(function);
        (low_product_local).load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Add);
        (high_limb_local).store(function);
        (low_limb_local).load(function);
        (magnitude_subsecond_local).load(function);
        function.instruction(&Instruction::I64Add);
        (low_limb_local).store(function);
        (low_limb_local).load(function);
        (magnitude_subsecond_local).load(function);
        function.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, function);
        (high_limb_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        (high_limb_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let schema = self.runtime_schema();
        let negative = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        negative_local.load(function);
        function.instruction(&Instruction::I32WrapI64);
        negative.store(function);
        function.instruction(&Instruction::I32Const(2));
        index.store(function);
        let slot = schema.reserve_gc_local(function);
        let construction = BigIntConstruction::allocate(schema, slot, index, function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        construction.write(index, low_limb_local, schema, function);
        function.instruction(&Instruction::I32Const(1));
        index.store(function);
        construction.write(index, high_limb_local, schema, function);
        let result = schema
            .reserve_gc_local(function)
            .initialize(construction.publish(negative, schema, function), function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(negative, function);
        for local in [
            high_limb_local,
            low_limb_local,
            high_product_local,
            low_product_local,
            low_word_local,
            magnitude_subsecond_local,
            magnitude_seconds_local,
            negative_local,
        ] {
            schema.release_i64_local(local, function);
        }
        result
    }

    /// Round `(seconds_local, subseconds_local)` in place to a
    /// `quantum_local`-nanosecond increment with `mode_local`.
    ///
    /// The `RoundTemporalInstant` core shared by `Instant.prototype.round`
    /// and `Instant.prototype.toString`: day/within-day split with the
    /// floor correction for pre-epoch instants, half-even parity from the
    /// global quotient, the `round_up` decision, and carry absorption back
    /// into seconds. All scratch locals are reserved and released inside.
    pub(super) fn emit_temporal_round_seconds_and_subseconds_to_quantum(
        &mut self,
        quantum_local: I64Local,
        mode_local: I64Local,
        seconds_local: I64Local,
        subseconds_local: I64Local,
        function: &mut Function,
    ) {
        let day_local = self.runtime_schema().reserve_i64_local(function);
        let within_day_local = self.runtime_schema().reserve_i64_local(function);
        let quotient_local = self.runtime_schema().reserve_i64_local(function);
        let remainder_local = self.runtime_schema().reserve_i64_local(function);
        let parity_local = self.runtime_schema().reserve_i64_local(function);
        let positive_local = self.runtime_schema().reserve_i64_local(function);
        (seconds_local).load(function);
        function.instruction(&Instruction::I64Const(86_400));
        function.instruction(&Instruction::I64DivS);
        (day_local).store(function);
        (seconds_local).load(function);
        function.instruction(&Instruction::I64Const(86_400));
        function.instruction(&Instruction::I64RemS);
        (within_day_local).store(function);
        (within_day_local).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, function);
        (within_day_local).load(function);
        function.instruction(&Instruction::I64Const(86_400));
        function.instruction(&Instruction::I64Add);
        (within_day_local).store(function);
        (day_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        (day_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (within_day_local).load(function);
        function.instruction(&Instruction::I64Const(1_000_000_000));
        function.instruction(&Instruction::I64Mul);
        (subseconds_local).load(function);
        function.instruction(&Instruction::I64Add);
        (within_day_local).store(function);

        // RoundNumberToIncrementAsIfPositive uses the floor quotient even
        // before the epoch. A day is an exact multiple of every admitted
        // quantum, so its remainder can be computed without a wide product.
        (within_day_local).load(function);
        (quantum_local).load(function);
        function.instruction(&Instruction::I64DivU);
        (quotient_local).store(function);
        (within_day_local).load(function);
        (quantum_local).load(function);
        function.instruction(&Instruction::I64RemU);
        (remainder_local).store(function);
        // Half-even needs the global quotient parity. Keeping only the
        // within-day quotient would misround odd days with a 24-hour quantum.
        (day_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(NANOSECONDS_PER_TEMPORAL_DAY));
        (quantum_local).load(function);
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Mul);
        (quotient_local).load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64And);
        (parity_local).store(function);
        function.instruction(&Instruction::I64Const(1));
        (positive_local).store(function);
        (within_day_local).load(function);
        (remainder_local).load(function);
        function.instruction(&Instruction::I64Sub);
        (within_day_local).store(function);
        self.emit_temporal_duration_round_up_i32(
            remainder_local,
            quantum_local,
            parity_local,
            positive_local,
            mode_local,
            function,
        );
        self.open_frame(ControlFrameKind::If, function);
        (within_day_local).load(function);
        (quantum_local).load(function);
        function.instruction(&Instruction::I64Add);
        (within_day_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // within_day may equal one full day after rounding; the exact
        // seconds addition absorbs that carry without another calendar step.
        (day_local).load(function);
        function.instruction(&Instruction::I64Const(86_400));
        function.instruction(&Instruction::I64Mul);
        (within_day_local).load(function);
        function.instruction(&Instruction::I64Const(1_000_000_000));
        function.instruction(&Instruction::I64DivU);
        function.instruction(&Instruction::I64Add);
        (seconds_local).store(function);
        (within_day_local).load(function);
        function.instruction(&Instruction::I64Const(1_000_000_000));
        function.instruction(&Instruction::I64RemU);
        (subseconds_local).store(function);
        for local in [
            positive_local,
            parity_local,
            remainder_local,
            quotient_local,
            within_day_local,
            day_local,
        ] {
            self.runtime_schema().release_i64_local(local, function);
        }
    }

    /// Temporal proposal `Temporal.Instant.prototype.toString`.
    ///
    /// Reads `fractionalSecondDigits`, `roundingMode`, `smallestUnit` and
    /// `timeZone` in spec order, rounds the epoch to the implied quantum,
    /// shifts it into the zone, and renders. `smallestUnit` overrides
    /// `fractionalSecondDigits` for both rounding and display; its range
    /// check runs after every option read
    /// (`options-read-before-algorithmic-validation.js`).
    pub(crate) fn emit_temporal_instant_to_string(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_instant_to_string_core(InstantStringOptions::FromArgument, function)
    }

    /// Temporal proposal `Temporal.Instant.prototype.toJSON`.
    ///
    /// `TemporalInstantToString(instant, AUTO)` — the same body with
    /// `undefined` options, but a distinct function object. The options
    /// argument is never read, so `toJSON/basic.js`'s throwing Proxy options
    /// bag passes without any extra guard.
    pub(crate) fn emit_temporal_instant_to_json(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_instant_to_string_core(InstantStringOptions::Undefined, function)
    }

    fn emit_temporal_instant_to_string_core(
        &mut self,
        source: InstantStringOptions,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let branded = self.emit_temporal_branded_instant_receiver(f)?;
        let options = schema.reserve_value_local(f);
        let zone_value = schema.reserve_value_local(f);
        let output = schema.reserve_value_local(f);
        let time_value = schema.reserve_value_local(f);
        let piece = schema.reserve_value_local(f);
        let digits = schema.reserve_i64_local(f);
        let mode_code = schema.reserve_i64_local(f);
        let unit = schema.reserve_i64_local(f);
        let precision = schema.reserve_i64_local(f);
        let increment = schema.reserve_i64_local(f);
        let quantum_code = schema.reserve_i64_local(f);
        let present = schema.reserve_i32_local(f);
        match source {
            InstantStringOptions::FromArgument => self.emit_builtin_arg_to_value(0, &options, f),
            InstantStringOptions::Undefined => options.set_undefined(f),
        }
        self.emit_temporal_duration_options_object(&options, f)?;
        // Get each option in its normative order, before unit validation and
        // the later time-zone identifier conversion.
        self.emit_temporal_plain_time_fractional_digits_option(&options, digits, f)?;
        self.emit_temporal_duration_rounding_mode_option(
            &options,
            TemporalRoundingMode::Trunc,
            mode_code,
            f,
        )?;
        self.emit_temporal_duration_unit_option(
            &options,
            TemporalUnitOptionProperty::SmallestUnit,
            unit,
            f,
        )?;
        self.emit_temporal_duration_option_get(&options, "timeZone", &zone_value, f)?;
        self.emit_temporal_seconds_string_precision(
            digits,
            unit,
            precision,
            increment,
            RuntimeErrorMessage::INVALID_TEMPORAL_INSTANT_UNIT_OPTION,
            f,
        )?;
        zone_value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        present.store(f);
        present.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let utc = schema
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("UTC", f)?, f);
        zone_value.set_reference(&utc, schema, f);
        utc.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let zone = self.emit_temporal_zoned_date_time_time_zone(
            &zone_value,
            TemporalTimeZoneStringGoal::Object,
            f,
        )?;
        self.emit_temporal_plain_time_rounding_quantum(unit, increment, quantum_code, f);
        let mode = self.emit_temporal_validated_rounding_mode(mode_code, f)?;
        let quantum = self.emit_temporal_validated_instant_rounding_quantum(quantum_code, f)?;
        let instant = self.emit_temporal_normalized_instant_from_instant_record(&branded, f)?;
        let rounded = self.emit_temporal_round_instant_for_string(&instant, &quantum, &mode, f)?;
        let snapshot = self.emit_temporal_zone_snapshot(&zone, &rounded, f)?;
        let iso = self.emit_temporal_zone_snapshot_iso_record(&snapshot, f)?;
        self.emit_temporal_iso_date_string(
            iso.fields()[0],
            iso.fields()[1],
            iso.fields()[2],
            &output,
            f,
        )?;
        self.emit_temporal_plain_time_record_to_string(
            &Self::temporal_plain_date_time_time_locals(iso.fields()),
            precision,
            &time_value,
            f,
        )?;
        let date = schema
            .reserve_gc_local(f)
            .initialize(output.cast_reference::<StringValue>(schema, f), f);
        self.emit_temporal_append_gc_literal(&date, "T", f)?;
        let time = schema
            .reserve_gc_local(f)
            .initialize(time_value.cast_reference::<StringValue>(schema, f), f);
        let text = schema
            .reserve_gc_local(f)
            .initialize(self.emit_concat_gc_strings(&date, &time, f), f);
        time.clear(f);
        date.clear(f);
        output.set_reference(&text, schema, f);
        text.clear(f);
        present.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_temporal_format_rounded_time_zone_offset(&snapshot, &piece, f)?;
        f.instruction(&Instruction::Else);
        let z = schema
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("Z", f)?, f);
        piece.set_reference(&z, schema, f);
        z.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let left = schema
            .reserve_gc_local(f)
            .initialize(output.cast_reference::<StringValue>(schema, f), f);
        let right = schema
            .reserve_gc_local(f)
            .initialize(piece.cast_reference::<StringValue>(schema, f), f);
        let complete = schema
            .reserve_gc_local(f)
            .initialize(self.emit_concat_gc_strings(&left, &right, f), f);
        self.completion()
            .value()
            .set_reference(&complete, schema, f);
        self.completion().set_normal(self.completion().value(), f);
        complete.clear(f);
        right.clear(f);
        left.clear(f);
        iso.release(self, f);
        snapshot.release(self, f);
        rounded.release(self, f);
        instant.release(self, f);
        quantum.release(self, f);
        mode.release(self, f);
        zone.release(self, f);
        schema.release_i32_local(present, f);
        for local in [quantum_code, increment, precision, unit, mode_code, digits] {
            schema.release_i64_local(local, f);
        }
        piece.clear(f);
        time_value.clear(f);
        output.clear(f);
        zone_value.clear(f);
        options.clear(f);
        branded.release(f);
        Ok(())
    }

    pub(super) fn temporal_calendar_helper_stub(&mut self, helper: RuntimeHelperId) -> Function {
        let mut function = self.begin_helper_body(helper);
        // Planning proves that no Temporal consumer reaches this declaration.
        // Unreachable is polymorphic over its actual typed result ABI.
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        self.finish_function(function)
    }

    /// One typed date parser serves the three date-shaped calendar probes.
    /// Rewrites precede parsing and retain the omitted-field grammar fact.
    /// The registered ABI carries GC input, rewrite I32, trusted Environment,
    /// and the complete five-result JavaScript completion.
    pub(crate) fn compile_temporal_calendar_iso_date_probe_helper(
        &mut self,
        real: bool,
    ) -> Result<Function, EmitError> {
        use crate::runtime_helpers::{HelperParameters, TemporalCalendarIsoDateProbeParameters};
        if !real {
            return Ok(
                self.temporal_calendar_helper_stub(RuntimeHelperId::TemporalCalendarIsoDateProbe)
            );
        }
        let mut function = self.begin_helper_body(RuntimeHelperId::TemporalCalendarIsoDateProbe);
        let parameters =
            self.helper_parameters::<TemporalCalendarIsoDateProbeParameters>(&mut function);
        let schema = self.runtime_schema();
        let normalized = schema.reserve_value_local(&mut function);
        normalized.set_reference(&parameters.input, schema, &mut function);
        let fields: [I64Local; 3] =
            std::array::from_fn(|_| schema.reserve_i64_local(&mut function));
        let calendar = schema.reserve_value_local(&mut function);
        let missing = schema.reserve_i64_local(&mut function);
        function.instruction(&Instruction::I64Const(0));
        missing.store(&mut function);
        parameters.rewrite.load(&mut function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut function);
        self.emit_temporal_partial_date_rewrite_string(
            &parameters.input,
            TemporalPartialDateRewrite::YearMonth {
                day_empty_out: Some(missing),
            },
            &normalized,
            &mut function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        parameters.rewrite.load(&mut function);
        function.instruction(&Instruction::I32Const(2));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut function);
        self.emit_temporal_month_day_rewrite_string(
            &parameters.input,
            &normalized,
            Some(missing),
            &mut function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let input = schema.reserve_gc_local(&mut function).initialize(
            normalized.cast_reference::<StringValue>(schema, &mut function),
            &mut function,
        );
        self.emit_temporal_parse_plain_date_string(
            &input,
            fields[0],
            fields[1],
            fields[2],
            &calendar,
            &mut function,
        )?;
        let identifier = schema.reserve_gc_local(&mut function).initialize(
            calendar.cast_reference::<StringValue>(schema, &mut function),
            &mut function,
        );
        let iso = schema.reserve_gc_local(&mut function).initialize(
            self.emit_interned_string_reference(
                TemporalCalendarId::DEFAULT.canonical(),
                &mut function,
            )?,
            &mut function,
        );
        missing.load(&mut function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.emit_string_payload_equality_i32(&identifier, &iso, &mut function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, &mut function);
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::INVALID_TEMPORAL_PLAINDATE_CALENDAR,
            &mut function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().set_normal(&calendar, &mut function);
        self.completion().emit(&mut function);
        iso.clear(&mut function);
        identifier.clear(&mut function);
        input.clear(&mut function);
        schema.release_i64_local(missing, &mut function);
        calendar.clear(&mut function);
        for local in fields.into_iter().rev() {
            schema.release_i64_local(local, &mut function);
        }
        normalized.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Date goals precede bare identifiers and the final time goal. A private
    /// rejected probe is discarded; the final parser's original Throw is kept.
    /// Current calendar spellings cannot collide with ISO date/time grammar.
    pub(crate) fn compile_temporal_calendar_identifier_helper(
        &mut self,
        real: bool,
    ) -> Result<Function, EmitError> {
        use crate::runtime_helpers::{
            HelperParameters, TemporalCalendarIdentifierParameters,
            TemporalCalendarIsoDateProbeArguments,
        };
        if !real {
            return Ok(
                self.temporal_calendar_helper_stub(RuntimeHelperId::TemporalCalendarIdentifier)
            );
        }
        let mut function = self.begin_helper_body(RuntimeHelperId::TemporalCalendarIdentifier);
        let parameters =
            self.helper_parameters::<TemporalCalendarIdentifierParameters>(&mut function);
        let schema = self.runtime_schema();
        let resolved = schema.reserve_i32_local(&mut function);
        let rewrite = schema.reserve_i32_local(&mut function);
        let fold = schema.reserve_i32_local(&mut function);
        let fields: [I64Local; 4] =
            std::array::from_fn(|_| schema.reserve_i64_local(&mut function));
        let answer = schema.reserve_value_local(&mut function);
        let probe = schema.reserve_completion(&mut function);
        let default = schema.reserve_gc_local(&mut function).initialize(
            self.emit_interned_string_reference(
                TemporalCalendarId::DEFAULT.canonical(),
                &mut function,
            )?,
            &mut function,
        );
        answer.set_reference(&default, schema, &mut function);
        default.clear(&mut function);
        function.instruction(&Instruction::I32Const(0));
        resolved.store(&mut function);
        function.instruction(&Instruction::I32Const(1));
        fold.store(&mut function);
        // Each rejected probe stays private. It cannot overwrite the final
        // helper completion before the next grammar alternative is attempted.
        for form in 0..3_i32 {
            resolved.load(&mut function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, &mut function);
            function.instruction(&Instruction::I32Const(form));
            rewrite.store(&mut function);
            schema
                .call_helper(
                    TemporalCalendarIsoDateProbeArguments::new(
                        &parameters.input,
                        rewrite,
                        &parameters.caller_environment,
                    ),
                    self.runtime_helper_base()?,
                    &mut function,
                )
                .store(&probe, &mut function);
            probe.kind().load(&mut function);
            function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
            function.instruction(&Instruction::I32Ne);
            self.open_frame(ControlFrameKind::If, &mut function);
            answer.copy_from(probe.value(), &mut function);
            function.instruction(&Instruction::I32Const(1));
            resolved.store(&mut function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        resolved.load(&mut function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut function);
        for calendar in TemporalCalendarId::ALL {
            for &spelling in calendar.spellings() {
                let expected = schema.reserve_gc_local(&mut function).initialize(
                    self.emit_interned_string_reference(spelling, &mut function)?,
                    &mut function,
                );
                self.emit_string_payload_equality_i32_with_ascii_case_folding(
                    &parameters.input,
                    &expected,
                    Some(fold),
                    &mut function,
                );
                expected.clear(&mut function);
                self.open_frame(ControlFrameKind::If, &mut function);
                let canonical = schema.reserve_gc_local(&mut function).initialize(
                    self.emit_interned_string_reference(calendar.canonical(), &mut function)?,
                    &mut function,
                );
                answer.set_reference(&canonical, schema, &mut function);
                canonical.clear(&mut function);
                function.instruction(&Instruction::I32Const(1));
                resolved.store(&mut function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        resolved.load(&mut function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut function);
        self.emit_temporal_parse_plain_time_string(
            &parameters.input,
            fields[0],
            fields[1],
            fields[2],
            fields[3],
            TemporalTimeCalendarUse::Resolve { calendar: &answer },
            &mut function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().set_normal(&answer, &mut function);
        self.completion().emit(&mut function);
        probe.clear(&mut function);
        answer.clear(&mut function);
        for local in fields.into_iter().rev() {
            schema.release_i64_local(local, &mut function);
        }
        schema.release_i32_local(fold, &mut function);
        schema.release_i32_local(rewrite, &mut function);
        schema.release_i32_local(resolved, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
    pub(in crate::builtins) fn emit_temporal_string_length(
        &self,
        input: &GcLocal<StringValue>,
        length: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(input, schema, function)
                .reference(),
            function,
        );
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
        length.store(function);
        units.clear(function);
    }
    pub(in crate::builtins) fn emit_temporal_load_code_unit(
        &self,
        input: &GcLocal<StringValue>,
        index: I64Local,
        output: I64Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let length = schema.reserve_i64_local(function);
        self.emit_temporal_string_length(input, length, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I64LtU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        self.emit_gc_string_code_unit_i32(input, index, function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::End);
        output.store(function);
        schema.release_i64_local(length, function);
    }
    pub(in crate::builtins) fn emit_temporal_string_slice(
        &self,
        input: &GcLocal<StringValue>,
        start: I64Local,
        length: I64Local,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        let schema = self.runtime_schema();
        let end = schema.reserve_i64_local(function);
        start.load(function);
        length.load(function);
        function.instruction(&Instruction::I64Add);
        end.store(function);
        let output = self.emit_gc_string_slice(input, start, end, function);
        schema.release_i64_local(end, function);
        output
    }
    pub(in crate::builtins) fn emit_temporal_parse_instant_string(
        &mut self,
        input: &GcLocal<StringValue>,
        function: &mut Function,
    ) -> Result<GcLocal<BigIntValue>, EmitError> {
        let schema = self.runtime_schema();
        let seconds = schema.reserve_i64_local(function);
        let nanosecond = schema.reserve_i64_local(function);
        self.emit_temporal_parse_iso_string(
            input,
            TemporalIsoParseGoal::Instant {
                seconds,
                nanosecond,
            },
            function,
        )?;
        let value = self.emit_temporal_epoch_nanoseconds_bigint(seconds, nanosecond, function);
        schema.release_i64_local(nanosecond, function);
        schema.release_i64_local(seconds, function);
        Ok(value)
    }
}
