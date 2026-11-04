//! One private numeric projection for a complete repeated masked-row domain.
use super::*;

pub(super) struct RowProjection {
    values: GcLocal<NumberBitsI64Array, Nullable>,
    period: u32,
    width: u32,
}

impl RowProjection {
    pub(super) fn new(
        period: Option<u32>,
        paths: &[CompactReadPath],
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Option<Self> {
        let period = period?;
        if !period.is_power_of_two()
            || paths.is_empty()
            || paths
                .iter()
                .any(|path| matches!(path, CompactReadPath::ArrayNumber(..)))
        {
            return None;
        }
        let width = u32::try_from(paths.len()).ok()?;
        let entries = period.checked_mul(width)?;
        i32::try_from(entries).ok()?;
        Some(Self {
            values: builder
                .runtime_schema()
                .reserve_gc_local(function)
                .initialize_null(builder.runtime_schema(), function),
            period,
            width,
        })
    }

    pub(super) fn guard_reuse(
        &self,
        slot: I32Local,
        length: I64Local,
        done: ControlTarget,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        // Admission already bounds 0 <= slot <= length <= 2^20. More than
        // one complete low-bit period makes every preflight row addressable.
        length.load(function);
        slot.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(i64::from(self.period)));
        function.instruction(&Instruction::I64LeU);
        builder.emit_branch_if_to_target(done, function);
    }

    pub(super) fn prepare(
        &self,
        rows: &CompactPathBorrow<'_>,
        first_slot: I32Local,
        done: ControlTarget,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let s = builder.runtime_schema();
        for (path, name) in rows.paths.iter().zip(&rows.names) {
            let cached = path.name().cached(builder, function);
            name.replace(cached.load(s, function), function);
            cached.clear(function);
            name.load(s, function).is_null(function);
            builder.emit_branch_if_to_target(done, function);
        }
        // The preallocation gate uses the same current own-data decoder as
        // every preflight row; no streaming shape/slot admission precedes it.
        let first = s.reserve_i32_local(function);
        first_slot.load(function);
        function.instruction(&Instruction::I32Const((self.period - 1) as i32));
        function.instruction(&Instruction::I32And);
        first.store(function);
        let accepted = self.try_prepare_row(rows, first, builder, function)?;
        s.release_i32_local(first, function);
        accepted.load(function);
        s.release_i32_local(accepted, function);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(done, function);
        let index = s.reserve_i32_local(function);
        let cursor = s.reserve_i32_local(function);
        let bits = s.reserve_i64_local(function);
        index.set_constant((self.period * self.width) as i32, function);
        self.values.replace(
            s.array_type::<NumberBitsI64Array>()
                .filled(GcOperand::i64(0), index, function)
                .nullable(),
            function,
        );
        cursor.set_constant(0, function);
        index.set_constant(0, function);
        let finished = builder.open_frame(ControlFrameKind::Block, function);
        let next = builder.open_frame(ControlFrameKind::Loop, function);
        cursor.load(function);
        function.instruction(&Instruction::I32Const(self.period as i32));
        function.instruction(&Instruction::I32GeU);
        builder.emit_branch_if_to_target(finished, function);
        // Canonical own-data decoding may allocate/normalize internal recipes,
        // but invokes no source/provider callback and writes no source local.
        let accepted = self.try_prepare_row(rows, cursor, builder, function)?;
        accepted.load(function);
        s.release_i32_local(accepted, function);
        function.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(done, function);
        for number in &rows.numbers {
            number.load(function);
            function.instruction(&Instruction::I64ReinterpretF64);
            bits.store(function);
            s.array_type::<NumberBitsI64Array>().write(
                &self.values,
                index,
                GcOperand::i64_local(bits),
                s,
                function,
            );
            index.load(function);
            function.instruction(&Instruction::I32Const(1));
            function.instruction(&Instruction::I32Add);
            index.store(function);
        }
        cursor.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        cursor.store(function);
        builder.emit_branch_to_target(next, function);
        builder.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        s.release_i64_local(bits, function);
        s.release_i32_local(cursor, function);
        s.release_i32_local(index, function);
        Ok(())
    }

    /// Keep the complete current row rooted across canonical recipe resolution.
    /// Every miss clears private scratch before the original whole-loop path.
    fn try_prepare_row(
        &self,
        rows: &CompactPathBorrow<'_>,
        slot: I32Local,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = builder.runtime_schema();
        let accepted = s.reserve_i32_local(function);
        accepted.set_constant(0, function);
        let stored = s.reserve_gc_local(function).initialize(
            s.array_type::<PackedValueTable>()
                .read(&rows.values, slot, s, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let row = s.reserve_value_local(function);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &row, s, function);
        let key = s.reserve_value_local(function);
        key.set_undefined(function);
        let result = s.reserve_completion(function);
        result.initialize(function);
        let declined = builder.open_frame(ControlFrameKind::Block, function);
        for ((path, name), number) in rows.paths.iter().zip(&rows.names).zip(&rows.numbers) {
            let text = s
                .reserve_gc_local(function)
                .initialize(name.load(s, function).require_non_null(function), function);
            key.set_reference(&text, s, function);
            text.clear(function);
            let found = builder.emit_raw_string_own_data_read(&row, &key, &result, function)?;
            found.load(function);
            s.release_i32_local(found, function);
            function.instruction(&Instruction::I32Eqz);
            builder.emit_branch_if_to_target(declined, function);
            result.value().tag().load(function);
            function.instruction(&Instruction::I32Const(path.tag().tag()));
            function.instruction(&Instruction::I32Ne);
            builder.emit_branch_if_to_target(declined, function);
            if matches!(path, CompactReadPath::StringLength(_)) {
                result.value().reference().load(function);
                function.instruction(&Instruction::RefTestNonNull(
                    s.reference_type::<StringValue>(crate::gc_types::GcNullability::NonNullable)
                        .heap_type,
                ));
                function.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(declined, function);
                let text = s.reserve_gc_local(function).initialize(
                    result.value().cast_reference::<StringValue>(s, function),
                    function,
                );
                let length = s.reserve_i64_local(function);
                builder.emit_native_gc_string_length(&text, length, function);
                length.load(function);
                function.instruction(&Instruction::F64ConvertI64U);
                s.release_i64_local(length, function);
                text.clear(function);
            } else {
                // Construction excludes ArrayNumber paths.
                result.value().scalar().load(function);
                function.instruction(&Instruction::F64ReinterpretI64);
            }
            number.store(function);
        }
        accepted.set_constant(1, function);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        result.clear(function);
        key.clear(function);
        row.clear(function);
        stored.clear(function);
        Ok(accepted)
    }

    pub(super) fn read_row(
        &self,
        rows: &CompactPathBorrow<'_>,
        slot: I32Local,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let s = builder.runtime_schema();
        let index = s.reserve_i32_local(function);
        slot.load(function);
        function.instruction(&Instruction::I32Const((self.period - 1) as i32));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Const(self.width as i32));
        function.instruction(&Instruction::I32Mul);
        index.store(function);
        for (at, number) in rows.numbers.iter().enumerate() {
            if at != 0 {
                index.load(function);
                function.instruction(&Instruction::I32Const(1));
                function.instruction(&Instruction::I32Add);
                index.store(function);
            }
            s.array_type::<NumberBitsI64Array>()
                .read(&self.values, index, s, function);
            function.instruction(&Instruction::F64ReinterpretI64);
            number.store(function);
        }
        s.release_i32_local(index, function);
    }

    pub(super) fn clear_roots(&self, builder: &FunctionBuilder<'_>, function: &mut Function) {
        self.values.set_null(builder.runtime_schema(), function);
    }

    pub(super) fn release(self, function: &mut Function) {
        self.values.clear(function);
    }
}
