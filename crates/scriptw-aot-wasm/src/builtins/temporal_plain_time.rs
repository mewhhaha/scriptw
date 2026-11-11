//! `Temporal.PlainTime` codegen: record layout, validation, constructor and
//! the six unit accessors.
//!
//! Temporal proposal 4. A `PlainTime` is a wall-clock time with no date, no
//! time zone and no calendar — six small integers and nothing else. That makes
//! it the one Temporal type whose whole value fits in a single `i64`: the
//! nanosecond-of-day, in `[0, 86400 x 10^9)`. Almost every operation here goes
//! through that scalar and back, which is why `emit_temporal_plain_time_total_nanoseconds`
//! and `emit_temporal_plain_time_from_nanoseconds` are the two functions to
//! read first.

use crate::gc_types::*;

use super::super::*;
use super::temporal_options::{TemporalOverflow, TemporalTimeUnit, TemporalUnit};
use crate::intrinsics::temporal::{TemporalIntrinsicFamily, TemporalPrototypeSource};

impl TemporalTimeUnit {
    const fn plain_time_field_index(self) -> usize {
        match self {
            Self::Hour => 0,
            Self::Minute => 1,
            Self::Second => 2,
            Self::Millisecond => 3,
            Self::Microsecond => 4,
            Self::Nanosecond => 5,
        }
    }

    const fn plain_time_field_maximum(self) -> i64 {
        match self {
            Self::Hour => 23,
            Self::Minute | Self::Second => 59,
            Self::Millisecond | Self::Microsecond | Self::Nanosecond => 999,
        }
    }
}

/// `ToTemporalTimeRecord` reads the property bag in alphabetical order and the
/// reads are observable, so the order here is load-bearing. Each entry is
/// `(property name, index into the declaration-order arrays)`.
pub(crate) const TEMPORAL_PLAIN_TIME_ALPHABETICAL_FIELDS: [(&str, usize); 6] = [
    ("hour", 0),
    ("microsecond", 4),
    ("millisecond", 3),
    ("minute", 1),
    ("nanosecond", 5),
    ("second", 2),
];

/// `nsPerDay`. Derived from the unit table rather than restated, so the two
/// cannot drift; the `panic!` arm is const-evaluated and would fail the build
/// if `day` ever stopped having a fixed length.
pub(crate) const NANOSECONDS_PER_TEMPORAL_DAY: i64 = match TemporalUnit::Day.nanoseconds() {
    Some(nanoseconds) => nanoseconds,
    None => panic!("the day unit has a fixed nanosecond length"),
};

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn reserve_temporal_plain_time_field_locals(
        &self,
        function: &mut Function,
    ) -> [I64Local; 6] {
        std::array::from_fn(|_| self.runtime_schema().reserve_i64_local(function))
    }

    pub(crate) fn release_temporal_plain_time_field_locals(
        &self,
        fields: [I64Local; 6],
        function: &mut Function,
    ) {
        for field in fields.into_iter().rev() {
            self.runtime_schema().release_i64_local(field, function);
        }
    }

    pub(crate) fn emit_alloc_temporal_plain_time(
        &mut self,
        fields: &[I64Local; 6],
        prototype: TemporalPrototypeSource<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let scalars: [I32Local; 6] = std::array::from_fn(|index| {
            let local = schema.reserve_i32_local(function);
            fields[index].load(function);
            function.instruction(&Instruction::I32WrapI64);
            local.store(function);
            local
        });
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_temporal_object_header(
                TemporalIntrinsicFamily::PlainTime,
                prototype,
                function,
            )?,
            function,
        );
        let record = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<TemporalPlainTimeObject>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::i32_local(scalars[0]),
                    GcOperand::i32_local(scalars[1]),
                    GcOperand::i32_local(scalars[2]),
                    GcOperand::i32_local(scalars[3]),
                    GcOperand::i32_local(scalars[4]),
                    GcOperand::i32_local(scalars[5]),
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

    /// The `[[InitializedTemporalTime]]` brand check on `this`, leaving the six
    /// fields loaded. On failure it throws and returns, so callers may treat
    /// the fields as live afterwards.
    pub(crate) fn emit_temporal_plain_time_fields_from_receiver(
        &mut self,
        fields: &[I64Local; 6],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let record =
            self.emit_temporal_record_from_receiver::<TemporalPlainTimeObject>(function)?;
        self.emit_temporal_plain_time_load_record(&record, fields, function);
        record.clear(function);
        Ok(())
    }

    pub(crate) fn emit_temporal_plain_time_load_record(
        &self,
        record: &GcLocal<TemporalPlainTimeObject>,
        fields: &[I64Local; 6],
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let value = schema.reserve_i32_local(function);
        for (field, output) in [
            TemporalPlainTimeObjectSchema::HOUR,
            TemporalPlainTimeObjectSchema::MINUTE,
            TemporalPlainTimeObjectSchema::SECOND,
            TemporalPlainTimeObjectSchema::MILLISECOND,
            TemporalPlainTimeObjectSchema::MICROSECOND,
            TemporalPlainTimeObjectSchema::NANOSECOND,
        ]
        .into_iter()
        .zip(fields)
        {
            schema
                .struct_type::<TemporalPlainTimeObject>()
                .field(field)
                .read(record, schema, function)
                .store(value, function);
            value.load(function);
            function.instruction(&Instruction::I64ExtendI32S);
            output.store(function);
        }
        schema.release_i32_local(value, function);
    }

    /// `RejectTime`: every field must already be in range.
    pub(crate) fn emit_temporal_reject_time(
        &mut self,
        field_locals: &[I64Local; 6],
        function: &mut Function,
    ) -> Result<(), EmitError> {
        for unit in TemporalTimeUnit::ALL {
            let field_local = field_locals[unit.plain_time_field_index()];
            (field_local).load(function);
            function.instruction(&Instruction::I64Const(0));
            function.instruction(&Instruction::I64LtS);
            (field_local).load(function);
            function.instruction(&Instruction::I64Const(unit.plain_time_field_maximum()));
            function.instruction(&Instruction::I64GtS);
            function.instruction(&Instruction::I32Or);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_temporal_error_and_return(
                scriptw_ir::NativeErrorKind::RangeError,
                RuntimeErrorMessage::TEMPORAL_PLAINTIME_FIELD_IS_OUT_OF_RANGE,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        Ok(())
    }

    /// `RegulateTime`: clamp under `constrain`, throw under `reject`.
    pub(crate) fn emit_temporal_regulate_time(
        &mut self,
        field_locals: &[I64Local; 6],
        overflow_local: I64Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        (overflow_local).load(function);
        function.instruction(&Instruction::I64Const(TemporalOverflow::Reject.code()));
        function.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_reject_time(field_locals, function)?;
        function.instruction(&Instruction::Else);
        for unit in TemporalTimeUnit::ALL {
            let field_local = field_locals[unit.plain_time_field_index()];
            (field_local).load(function);
            function.instruction(&Instruction::I64Const(0));
            function.instruction(&Instruction::I64LtS);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(0));
            (field_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            (field_local).load(function);
            function.instruction(&Instruction::I64Const(unit.plain_time_field_maximum()));
            function.instruction(&Instruction::I64GtS);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(unit.plain_time_field_maximum()));
            (field_local).store(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// The nanosecond-of-day scalar. Bounded by `RejectTime`, so it always
    /// stays under `86400 x 10^9` and never comes near the `i64` ceiling.
    pub(crate) fn emit_temporal_plain_time_total_nanoseconds(
        &mut self,
        field_locals: &[I64Local; 6],
        output_local: I64Local,
        function: &mut Function,
    ) {
        function.instruction(&Instruction::I64Const(0));
        for unit in TemporalTimeUnit::ALL {
            let field_local = field_locals[unit.plain_time_field_index()];
            (field_local).load(function);
            function.instruction(&Instruction::I64Const(unit.nanoseconds()));
            function.instruction(&Instruction::I64Mul);
            function.instruction(&Instruction::I64Add);
        }
        (output_local).store(function);
    }

    /// `BalanceTime` on a nanosecond-of-day scalar that may have run off either
    /// end of the day: the result wraps, because `PlainTime` arithmetic has no
    /// date to carry into.
    pub(crate) fn emit_temporal_plain_time_from_nanoseconds(
        &mut self,
        nanoseconds_local: I64Local,
        field_locals: &[I64Local; 6],
        function: &mut Function,
    ) {
        let remaining_local = self.runtime_schema().reserve_i64_local(function);
        (nanoseconds_local).load(function);
        function.instruction(&Instruction::I64Const(NANOSECONDS_PER_TEMPORAL_DAY));
        function.instruction(&Instruction::I64RemS);
        (remaining_local).store(function);
        // `I64RemS` truncates toward zero, so a negative input leaves a
        // negative remainder; one day restores the floor semantics wrapping
        // needs.
        (remaining_local).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, function);
        (remaining_local).load(function);
        function.instruction(&Instruction::I64Const(NANOSECONDS_PER_TEMPORAL_DAY));
        function.instruction(&Instruction::I64Add);
        (remaining_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for (index, divisor) in [
            (5_usize, 1_000_i64),
            (4, 1_000),
            (3, 1_000),
            (2, 60),
            (1, 60),
        ] {
            (remaining_local).load(function);
            function.instruction(&Instruction::I64Const(divisor));
            function.instruction(&Instruction::I64RemS);
            (field_locals[index]).store(function);
            (remaining_local).load(function);
            function.instruction(&Instruction::I64Const(divisor));
            function.instruction(&Instruction::I64DivS);
            (remaining_local).store(function);
        }
        (remaining_local).load(function);
        (field_locals[0]).store(function);
        self.runtime_schema()
            .release_i64_local(remaining_local, function);
    }

    /// Round a signed nanosecond count to a whole number of `quantum_local`
    /// nanoseconds, reusing the Duration rounding-mode decision table.
    pub(crate) fn emit_temporal_plain_time_round_nanoseconds(
        &mut self,
        nanoseconds_local: I64Local,
        quantum_local: I64Local,
        mode_local: I64Local,
        function: &mut Function,
    ) {
        let sign_local = self.runtime_schema().reserve_i64_local(function);
        let magnitude_local = self.runtime_schema().reserve_i64_local(function);
        let quotient_local = self.runtime_schema().reserve_i64_local(function);
        let remainder_local = self.runtime_schema().reserve_i64_local(function);

        function.instruction(&Instruction::I64Const(1));
        (sign_local).store(function);
        (nanoseconds_local).load(function);
        function.instruction(&Instruction::I64Const(0));
        function.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(-1));
        (sign_local).store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        (nanoseconds_local).load(function);
        (sign_local).load(function);
        function.instruction(&Instruction::I64Mul);
        (magnitude_local).store(function);
        (magnitude_local).load(function);
        (quantum_local).load(function);
        function.instruction(&Instruction::I64DivU);
        (quotient_local).store(function);
        (magnitude_local).load(function);
        (quantum_local).load(function);
        function.instruction(&Instruction::I64RemU);
        (remainder_local).store(function);
        self.emit_temporal_duration_round_up_i32(
            remainder_local,
            quantum_local,
            quotient_local,
            sign_local,
            mode_local,
            function,
        );
        function.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        (quotient_local).load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::Else);
        (quotient_local).load(function);
        function.instruction(&Instruction::End);
        (quantum_local).load(function);
        function.instruction(&Instruction::I64Mul);
        (sign_local).load(function);
        function.instruction(&Instruction::I64Mul);
        (nanoseconds_local).store(function);

        for local in [remainder_local, quotient_local, magnitude_local, sign_local] {
            self.runtime_schema().release_i64_local(local, function);
        }
    }

    /// Temporal proposal 4.1: `Temporal.PlainTime([hour[, minute[, second[,
    /// millisecond[, microsecond[, nanosecond]]]]]])`. Every argument is
    /// optional and defaults to zero, so `length` is 0 even though six are
    /// read.
    pub(crate) fn emit_temporal_plain_time_constructor(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_require_construct_call(
            RuntimeErrorMessage::TEMPORAL_PLAINTIME_CONSTRUCTOR_REQUIRES_NEW,
            function,
        )?;
        let schema = self.runtime_schema();
        let input = schema.reserve_value_local(function);
        let fields = self.reserve_temporal_plain_time_field_locals(function);
        for (index, field) in fields.into_iter().enumerate() {
            self.emit_builtin_arg_to_value(index, &input, function);
            input.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(0));
            field.store(function);
            function.instruction(&Instruction::Else);
            self.emit_temporal_to_integer_with_truncation(
                &input,
                field,
                RuntimeErrorMessage::TEMPORAL_PLAINTIME_FIELD_MUST_BE_AN_INTEGER,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.emit_temporal_reject_time(&fields, function)?;
        let prototype =
            self.emit_temporal_constructor_prototype(TemporalIntrinsicFamily::PlainTime, function)?;
        self.emit_alloc_temporal_plain_time(
            &fields,
            TemporalPrototypeSource::Constructor(&prototype),
            function,
        )?;
        prototype.release(function);
        input.clear(function);
        self.release_temporal_plain_time_field_locals(fields, function);
        Ok(())
    }

    /// Every `Temporal.PlainTime.prototype` accessor: one record read and one
    /// `i64`-to-Number conversion, selected by field index.
    pub(crate) fn emit_temporal_plain_time_field(
        &mut self,
        unit: TemporalTimeUnit,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let fields = self.reserve_temporal_plain_time_field_locals(function);
        let output = self.runtime_schema().reserve_value_local(function);
        self.emit_temporal_plain_time_fields_from_receiver(&fields, function)?;
        self.emit_temporal_integer_number(fields[unit.plain_time_field_index()], &output, function);
        self.completion().set_normal(&output, function);
        output.clear(function);
        self.release_temporal_plain_time_field_locals(fields, function);
        Ok(())
    }

    /// Temporal deliberately forbids implicit comparison, so `valueOf` always
    /// throws — `a < b` on two times must be a loud error, not a silent string
    /// comparison.
    pub(crate) fn emit_temporal_plain_time_value_of(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_error_and_return(scriptw_ir::NativeErrorKind::TypeError, RuntimeErrorMessage::TEMPORAL_PLAINTIME_DOES_NOT_SUPPORT_IMPLICIT_CONVERSION_USE_COMPARE_OR_EQUALS, function)
    }
}
