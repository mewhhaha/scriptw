//! Exact private Number snapshots retain their original unboxed bit patterns.
use super::*;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_try_packed_number_sort_snapshot(
        &mut self,
        receiver: &ValueLocals,
        length: I64Local,
        f: &mut Function,
    ) -> Result<GcLocal<NumberBitsI64Array, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let result = s
            .reserve_gc_local::<NumberBitsI64Array, Nullable>(f)
            .initialize_null(s, f);
        self.emit_with_packed_array_snapshot(
            receiver,
            length,
            f,
            |builder, packed, decline, f| {
                s.array_storage_mode_is(&packed.storage, ArrayIndexedStorageMode::PackedNumbers, f);
                f.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(decline, f);
                s.field(ArrayIndexStorageSchema::COUNT)
                    .read(&packed.storage, s, f);
                length.load(f);
                f.instruction(&Instruction::I64Ne);
                builder.emit_branch_if_to_target(decline, f);
                let numbers = s.reserve_gc_local(f).initialize(
                    s.field(ArrayIndexStorageSchema::NUMBERS)
                        .read(&packed.storage, s, f)
                        .reference(),
                    f,
                );
                let extent = s.reserve_i32_local(f);
                length.load(f);
                f.instruction(&Instruction::I32WrapI64);
                extent.store(f);
                let zero = s.reserve_i64_local(f);
                zero.set_constant(0, f);
                let list = s.reserve_gc_local(f).initialize(
                    s.array_type::<NumberBitsI64Array>().filled(
                        GcOperand::i64_local(zero),
                        extent,
                        f,
                    ),
                    f,
                );
                s.array_type::<NumberBitsI64Array>()
                    .copy_prefix_from(&list, &numbers, extent, s, f);
                result.replace(list.load(s, f).nullable(), f);
                list.clear(f);
                numbers.clear(f);
                s.release_i64_local(zero, f);
                s.release_i32_local(extent, f);
                Ok(())
            },
        )?;
        Ok(result)
    }

    /// Revalidate live own writable data before one non-observable bulk copy.
    /// Flags, occupancy, length and capacity remain owned by canonical storage.
    pub(crate) fn emit_try_packed_number_sort_writeback(
        &mut self,
        receiver: &ValueLocals,
        length: I64Local,
        list: &GcLocal<NumberBitsI64Array>,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        self.emit_with_packed_array_snapshot(
            receiver,
            length,
            f,
            |builder, packed, decline, f| {
                s.array_storage_mode_is(&packed.storage, ArrayIndexedStorageMode::PackedNumbers, f);
                f.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(decline, f);
                s.field(ArrayIndexStorageSchema::COUNT)
                    .read(&packed.storage, s, f);
                length.load(f);
                f.instruction(&Instruction::I64Ne);
                builder.emit_branch_if_to_target(decline, f);
                let numbers = s.reserve_gc_local(f).initialize(
                    s.field(ArrayIndexStorageSchema::NUMBERS)
                        .read(&packed.storage, s, f)
                        .reference(),
                    f,
                );
                let extent = s.reserve_i32_local(f);
                length.load(f);
                f.instruction(&Instruction::I32WrapI64);
                extent.store(f);
                s.array_type::<NumberBitsI64Array>()
                    .copy_prefix_from(&numbers, list, extent, s, f);
                numbers.clear(f);
                s.release_i32_local(extent, f);
                handled.set_constant(1, f);
                Ok(())
            },
        )?;
        Ok(handled)
    }
}
