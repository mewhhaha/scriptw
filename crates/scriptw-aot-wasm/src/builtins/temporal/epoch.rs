//! Exact Instant admission over the sole immutable GC BigInt representation.
use super::*;

/// Publication accepts this completed range proof, never an unchecked BigInt.
/// The floor pair is exact, including negative fractional epochs.
pub(in crate::builtins) struct TemporalEpochNanoseconds {
    value: GcLocal<BigIntValue>,
    seconds: I64Local,
    nanosecond: I64Local,
}

impl TemporalEpochNanoseconds {
    pub(in crate::builtins) fn value(&self) -> &GcLocal<BigIntValue> {
        &self.value
    }
    pub(in crate::builtins) fn floor_seconds(&self) -> I64Local {
        self.seconds
    }
    pub(in crate::builtins) fn nanosecond(&self) -> I64Local {
        self.nanosecond
    }
    pub(in crate::builtins) fn clear(
        self,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        builder
            .runtime_schema()
            .release_i64_local(self.nanosecond, function);
        builder
            .runtime_schema()
            .release_i64_local(self.seconds, function);
        self.value.clear(function);
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_temporal_epoch_from_relative_view(
        &mut self,
        input: &crate::builtins::temporal_zone_provider::RelativeEpochView<'_>,
        function: &mut Function,
    ) -> TemporalEpochNanoseconds {
        let schema = self.runtime_schema();
        let value = schema.reserve_gc_local(function).initialize(
            input
                .value()
                .cast_reference::<BigIntValue>(schema, function),
            function,
        );
        let seconds = schema.reserve_i64_local(function);
        let nanosecond = schema.reserve_i64_local(function);
        input.floor_seconds().load(function);
        seconds.store(function);
        input.nanosecond().load(function);
        nanosecond.store(function);
        TemporalEpochNanoseconds {
            value,
            seconds,
            nanosecond,
        }
    }

    pub(in crate::builtins) fn emit_temporal_epoch_from_string_rounding(
        &mut self,
        input: &super::super::temporal_zoned_arithmetic::CompletedTemporalStringRoundingLocals,
        value: GcLocal<BigIntValue>,
        function: &mut Function,
    ) -> TemporalEpochNanoseconds {
        let schema = self.runtime_schema();
        let seconds = schema.reserve_i64_local(function);
        let nanosecond = schema.reserve_i64_local(function);
        input.floor_seconds().load(function);
        seconds.store(function);
        input.nanosecond().load(function);
        nanosecond.store(function);
        TemporalEpochNanoseconds {
            value,
            seconds,
            nanosecond,
        }
    }

    /// The immutable producer has already canonicalized magnitude and sign.
    /// Empty zero limbs are handled before any indexed read.
    pub(in crate::builtins) fn emit_temporal_instant_validated_epoch(
        &mut self,
        input: &GcLocal<BigIntValue>,
        function: &mut Function,
    ) -> Result<TemporalEpochNanoseconds, EmitError> {
        let schema = self.runtime_schema();
        let bigint = schema.struct_type::<BigIntValue>();
        let limbs = schema.reserve_gc_local(function).initialize(
            bigint
                .field(BigIntValueSchema::LIMBS)
                .read(input, schema, function)
                .reference(),
            function,
        );
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let negative = schema.reserve_i32_local(function);
        let low = schema.reserve_i64_local(function);
        let high = schema.reserve_i64_local(function);
        let quotient = schema.reserve_i64_local(function);
        let remainder = schema.reserve_i64_local(function);
        let chunk = schema.reserve_i64_local(function);
        let array = schema.array_type::<BigIntLimbArray>();
        array.length(&limbs, schema, function);
        count.store(function);
        bigint
            .field(BigIntValueSchema::NEGATIVE)
            .read(input, schema, function)
            .store(negative, function);
        count.load(function);
        function.instruction(&Instruction::I32Const(2));
        function.instruction(&Instruction::I32GtU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_instant_range_error(function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        for local in [low, high] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }
        count.load(function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        array
            .read(&limbs, index, schema, function)
            .store_i64(low, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        count.load(function);
        function.instruction(&Instruction::I32Const(2));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I32Const(1));
        index.store(function);
        array
            .read(&limbs, index, schema, function)
            .store_i64(high, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        high.load(function);
        function.instruction(&Instruction::I64Const(TEMPORAL_INSTANT_LIMIT_HIGH_LIMB));
        function.instruction(&Instruction::I64GtU);
        high.load(function);
        function.instruction(&Instruction::I64Const(TEMPORAL_INSTANT_LIMIT_HIGH_LIMB));
        function.instruction(&Instruction::I64Eq);
        low.load(function);
        function.instruction(&Instruction::I64Const(TEMPORAL_INSTANT_LIMIT_LOW_LIMB));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_temporal_instant_range_error(function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // Base-2^32 long division keeps every dividend below 10^9*2^32.
        // The completed range bounds the accumulated quotient below 2^44.
        for local in [quotient, remainder] {
            function.instruction(&Instruction::I64Const(0));
            local.store(function);
        }
        for (word, shift) in [(high, 32), (high, 0), (low, 32), (low, 0)] {
            remainder.load(function);
            function.instruction(&Instruction::I64Const(32));
            function.instruction(&Instruction::I64Shl);
            word.load(function);
            if shift != 0 {
                function.instruction(&Instruction::I64Const(shift));
                function.instruction(&Instruction::I64ShrU);
            }
            function.instruction(&Instruction::I64Const(u32::MAX as i64));
            function.instruction(&Instruction::I64And);
            function.instruction(&Instruction::I64Or);
            chunk.store(function);
            quotient.load(function);
            function.instruction(&Instruction::I64Const(32));
            function.instruction(&Instruction::I64Shl);
            chunk.load(function);
            function.instruction(&Instruction::I64Const(NANOSECONDS_PER_SECOND));
            function.instruction(&Instruction::I64DivU);
            function.instruction(&Instruction::I64Or);
            quotient.store(function);
            chunk.load(function);
            function.instruction(&Instruction::I64Const(NANOSECONDS_PER_SECOND));
            function.instruction(&Instruction::I64RemU);
            remainder.store(function);
        }
        negative.load(function);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::I64Const(0));
        quotient.load(function);
        function.instruction(&Instruction::I64Sub);
        quotient.store(function);
        remainder.load(function);
        function.instruction(&Instruction::I64Eqz);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        quotient.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        quotient.store(function);
        function.instruction(&Instruction::I64Const(NANOSECONDS_PER_SECOND));
        remainder.load(function);
        function.instruction(&Instruction::I64Sub);
        remainder.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let value = schema
            .reserve_gc_local(function)
            .initialize(input.load(schema, function), function);
        let seconds = schema.reserve_i64_local(function);
        let nanosecond = schema.reserve_i64_local(function);
        quotient.load(function);
        seconds.store(function);
        remainder.load(function);
        nanosecond.store(function);
        for local in [chunk, remainder, quotient, high, low] {
            schema.release_i64_local(local, function);
        }
        for local in [negative, index, count] {
            schema.release_i32_local(local, function);
        }
        limbs.clear(function);
        Ok(TemporalEpochNanoseconds {
            value,
            seconds,
            nanosecond,
        })
    }

    pub(in crate::builtins) fn emit_temporal_instant_range_error(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_temporal_error_and_return(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::TEMPORAL_INSTANT_EPOCH_NANOSECONDS_ARE_OUTSIDE_THE_SUPPORTED_RANGE,
            function,
        )
    }

    pub(in crate::builtins) fn emit_alloc_temporal_instant(
        &mut self,
        epoch: &TemporalEpochNanoseconds,
        prototype: TemporalPrototypeSource<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_temporal_object_header(
                TemporalIntrinsicFamily::Instant,
                prototype,
                function,
            )?,
            function,
        );
        let record = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<TemporalInstantObject>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(epoch.value(), schema),
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
        Ok(())
    }
}
