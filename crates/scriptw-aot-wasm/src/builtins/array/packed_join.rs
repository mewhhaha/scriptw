//! Exact-size join for a stable packed snapshot of present primitive strings,
//! null and undefined. Unsupported values return to the ordered generic loop.
use super::*;
use crate::gc_types::{
    CodeUnitArray, Nullable, PackedValueTable, StoredValueSchema, StringConstruction, StringValue,
    StringValueSchema,
};
use crate::runtime_helpers::{
    ArrayPackedJoinArguments, ArrayPackedJoinParameters, HelperParameters,
};

impl FunctionBuilder<'_> {
    pub(super) fn emit_try_packed_array_join(
        &mut self,
        receiver: &ValueLocals,
        length: I64Local,
        separator: &GcLocal<StringValue>,
        result: &ValueLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let text = s
            .call_helper(
                ArrayPackedJoinArguments::new(receiver, length, separator),
                self.runtime_helper_base()?,
                f,
            )
            .bind(s, s.reserve_gc_local(f), f);
        let handled = s.reserve_i32_local(f);
        text.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        handled.store(f);
        handled.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let present = s
            .reserve_gc_local(f)
            .initialize(text.load(s, f).require_non_null(f), f);
        result.set_reference(&present, s, f);
        present.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        text.clear(f);
        Ok(handled)
    }

    /// Null declines without observable reads/coercions; a String is the real
    /// completed result. Both builtin entry and guarded source calls use this
    /// sole String-producing kernel and its original resource checks.
    pub(crate) fn compile_array_packed_join_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::ArrayPackedJoin);
        let p = self.helper_parameters::<ArrayPackedJoinParameters>(&mut f);
        let s = self.runtime_schema();
        let text = s
            .reserve_gc_local::<StringValue, Nullable>(&mut f)
            .initialize_null(s, &mut f);
        p.length.load(&mut f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        // Length was captured before separator conversion in the builtin.
        // Zero still needs no indexed reads if that conversion changed it.
        text.replace(
            self.emit_interned_string_reference("", &mut f)?.nullable(),
            &mut f,
        );
        f.instruction(&Instruction::Else);
        let result = s.reserve_value_local(&mut f);
        let handled = self.emit_packed_array_join_kernel(
            &p.receiver,
            p.length,
            &p.separator,
            &result,
            &mut f,
        )?;
        handled.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        text.replace(
            result.cast_reference::<StringValue>(s, &mut f).nullable(),
            &mut f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(handled, &mut f);
        result.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        text.load(s, &mut f);
        text.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    fn emit_packed_array_join_kernel(
        &mut self,
        receiver: &ValueLocals,
        length: I64Local,
        separator: &GcLocal<StringValue>,
        result: &ValueLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        let element = s.reserve_value_local(f);
        let index = s.reserve_i64_local(f);
        let total = s.reserve_i64_local(f);
        let found = s.reserve_i32_local(f);
        let part_length = s.reserve_i32_local(f);
        let separator_length = s.reserve_i32_local(f);
        let offset = s.reserve_i32_local(f);
        let separator_units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(separator, s, f)
                .reference(),
            f,
        );
        s.array_type::<CodeUnitArray>()
            .length(&separator_units, s, f);
        separator_length.store(f);
        self.emit_with_packed_array_snapshot(
            receiver,
            length,
            f,
            |builder, packed, decline, f| {
                // Only PackedValues can hold strings, null or undefined when
                // length > 0; hoist the backing table and capacity check once.
                s.array_storage_mode_is(packed.storage(), ArrayIndexedStorageMode::PackedValues, f);
                f.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(decline, f);
                let values = s.reserve_gc_local(f).initialize(
                    s.field(ArrayObjectSchema::VALUES)
                        .read(packed.storage(), s, f)
                        .reference()
                        .require_non_null(f),
                    f,
                );
                length.load(f);
                s.array_type::<PackedValueTable>().length(&values, s, f);
                f.instruction(&Instruction::I64ExtendI32U);
                f.instruction(&Instruction::I64GtU);
                builder.emit_branch_if_to_target(decline, f);
                let slot = s.reserve_i32_local(f);
                let tag = s.reserve_i32_local(f);
                // Precompute total separator length once: length <= u32::MAX
                // and separator_length <= u32::MAX, so their product never
                // overflows u64.
                length.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Sub);
                separator_length.load(f);
                f.instruction(&Instruction::I64ExtendI32U);
                f.instruction(&Instruction::I64Mul);
                total.store(f);
                total.load(f);
                f.instruction(&Instruction::I64Const(u32::MAX as i64));
                f.instruction(&Instruction::I64GtU);
                builder.open_frame(ControlFrameKind::If, f);
                total.set_constant(u32::MAX as i64 + 1, f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                let all_strings = s.reserve_i32_local(f);
                all_strings.set_constant(1, f);
                // length <= values.len() <= i32::MAX was proven above.
                let count32 = s.reserve_i32_local(f);
                length.load(f);
                f.instruction(&Instruction::I32WrapI64);
                count32.store(f);
                // PackedValueTable length is <= PACKED_INDEX_LIMIT (1 << 20),
                // and every valid String is < 2^28 units, so sum < 2^48 << i64::MAX.
                // Guard explicitly so even a hypothetical > 1M backing stays exact.
                let need_saturate = s.reserve_i32_local(f);
                count32.load(f);
                f.instruction(&Instruction::I32Const(1 << 20));
                f.instruction(&Instruction::I32GtU);
                need_saturate.store(f);
                slot.set_constant(0, f);
                let validated = builder.open_frame(ControlFrameKind::Block, f);
                let validate = builder.open_frame(ControlFrameKind::Loop, f);
                slot.load(f);
                count32.load(f);
                f.instruction(&Instruction::I32GeU);
                builder.emit_branch_if_to_target(validated, f);
                let stored = s.reserve_gc_local(f).initialize(
                    s.array_type::<PackedValueTable>()
                        .read(&values, slot, s, f)
                        .reference()
                        .non_null_or_branch(builder.local_branch_label(decline), f),
                    f,
                );
                s.field(StoredValueSchema::TAG)
                    .read(&stored, s, f)
                    .store(tag, f);
                tag.load(f);
                f.instruction(&Instruction::I32Const(ValueKind::String.tag() as i32));
                f.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, f);
                s.field(StoredValueSchema::REFERENCE)
                    .read(&stored, s, f)
                    .store_eq(element.reference(), f);
                let text = s
                    .reserve_gc_local(f)
                    .initialize(element.cast_reference::<StringValue>(s, f), f);
                let units = s.reserve_gc_local(f).initialize(
                    s.field(StringValueSchema::CODE_UNITS)
                        .read(&text, s, f)
                        .reference(),
                    f,
                );
                s.array_type::<CodeUnitArray>().length(&units, s, f);
                f.instruction(&Instruction::I64ExtendI32U);
                total.load(f);
                f.instruction(&Instruction::I64Add);
                total.store(f);
                units.clear(f);
                text.clear(f);
                need_saturate.load(f);
                builder.open_frame(ControlFrameKind::If, f);
                total.load(f);
                f.instruction(&Instruction::I64Const(u32::MAX as i64));
                f.instruction(&Instruction::I64GtU);
                builder.open_frame(ControlFrameKind::If, f);
                total.set_constant(u32::MAX as i64 + 1, f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                f.instruction(&Instruction::Else);
                all_strings.set_constant(0, f);
                tag.load(f);
                f.instruction(&Instruction::I32Const(ValueKind::Null.tag() as i32));
                f.instruction(&Instruction::I32Eq);
                tag.load(f);
                f.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
                f.instruction(&Instruction::I32Eq);
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(decline, f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                stored.clear(f);
                slot.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Add);
                slot.store(f);
                builder.emit_branch_to_target(validate, f);
                builder.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                s.release_i32_local(need_saturate, f);
                s.release_i32_local(count32, f);
                // Too long a result is the general join's RangeError, at the
                // separator or element that crosses the limit; every value
                // here is a String or nullish, so declining repeats no effect.
                total.load(f);
                f.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
                f.instruction(&Instruction::I64GtU);
                builder.emit_branch_if_to_target(decline, f);
                total.load(f);
                f.instruction(&Instruction::I64Eqz);
                builder.open_frame(ControlFrameKind::If, f);
                let empty = s
                    .reserve_gc_local(f)
                    .initialize(builder.emit_interned_string_reference("", f)?, f);
                result.set_reference(&empty, s, f);
                empty.clear(f);
                f.instruction(&Instruction::Else);
                length.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Eq);
                builder.open_frame(ControlFrameKind::If, f);
                // Nonempty singleton can only be the validated immutable String.
                slot.set_constant(0, f);
                let single = s.reserve_gc_local(f).initialize(
                    s.array_type::<PackedValueTable>()
                        .read(&values, slot, s, f)
                        .reference()
                        .require_non_null(f),
                    f,
                );
                s.field(StoredValueSchema::REFERENCE)
                    .read(&single, s, f)
                    .store_eq(element.reference(), f);
                let text = s
                    .reserve_gc_local(f)
                    .initialize(element.cast_reference::<StringValue>(s, f), f);
                result.set_reference(&text, s, f);
                text.clear(f);
                single.clear(f);
                f.instruction(&Instruction::Else);
                total.load(f);
                f.instruction(&Instruction::I32WrapI64);
                part_length.store(f);
                let construction =
                    StringConstruction::allocate(s, s.reserve_gc_local(f), part_length, f);
                let single_sep = s.reserve_i32_local(f);
                let sep_unit = s.reserve_i32_local(f);
                separator_length.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Eq);
                single_sep.store(f);
                single_sep.load(f);
                builder.open_frame(ControlFrameKind::If, f);
                slot.set_constant(0, f);
                s.array_type::<CodeUnitArray>()
                    .read(&separator_units, slot, s, f)
                    .store(sep_unit, f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                offset.set_constant(0, f);
                all_strings.load(f);
                single_sep.load(f);
                f.instruction(&Instruction::I32And);
                builder.open_frame(ControlFrameKind::If, f);
                // Fast branch-free loop for all-String arrays joined by a 1-unit
                // separator: hoist element 0 before the loop so every iteration
                // writes sep_unit + bulk-copies element without tag/index tests.
                slot.set_constant(0, f);
                let first_stored = s.reserve_gc_local(f).initialize(
                    s.array_type::<PackedValueTable>()
                        .read(&values, slot, s, f)
                        .reference()
                        .require_non_null(f),
                    f,
                );
                s.field(StoredValueSchema::REFERENCE)
                    .read(&first_stored, s, f)
                    .store_eq(element.reference(), f);
                first_stored.clear(f);
                let first_text = s
                    .reserve_gc_local(f)
                    .initialize(element.cast_reference::<StringValue>(s, f), f);
                let first_units = s.reserve_gc_local(f).initialize(
                    s.field(StringValueSchema::CODE_UNITS)
                        .read(&first_text, s, f)
                        .reference(),
                    f,
                );
                s.array_type::<CodeUnitArray>().length(&first_units, s, f);
                part_length.store(f);
                construction.copy_concatenated_units_from(offset, &first_units, part_length, s, f);
                part_length.load(f);
                offset.store(f);
                first_units.clear(f);
                first_text.clear(f);
                let count32 = s.reserve_i32_local(f);
                length.load(f);
                f.instruction(&Instruction::I32WrapI64);
                count32.store(f);
                slot.set_constant(1, f);
                let fast_done = builder.open_frame(ControlFrameKind::Block, f);
                let fast_loop = builder.open_frame(ControlFrameKind::Loop, f);
                slot.load(f);
                count32.load(f);
                f.instruction(&Instruction::I32GeU);
                builder.emit_branch_if_to_target(fast_done, f);
                construction.write(offset, sep_unit, s, f);
                offset.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Add);
                offset.store(f);
                let stored = s.reserve_gc_local(f).initialize(
                    s.array_type::<PackedValueTable>()
                        .read(&values, slot, s, f)
                        .reference()
                        .require_non_null(f),
                    f,
                );
                s.field(StoredValueSchema::REFERENCE)
                    .read(&stored, s, f)
                    .store_eq(element.reference(), f);
                stored.clear(f);
                let text = s
                    .reserve_gc_local(f)
                    .initialize(element.cast_reference::<StringValue>(s, f), f);
                let units = s.reserve_gc_local(f).initialize(
                    s.field(StringValueSchema::CODE_UNITS)
                        .read(&text, s, f)
                        .reference(),
                    f,
                );
                s.array_type::<CodeUnitArray>().length(&units, s, f);
                part_length.store(f);
                construction.copy_concatenated_units_from(offset, &units, part_length, s, f);
                offset.load(f);
                part_length.load(f);
                f.instruction(&Instruction::I32Add);
                offset.store(f);
                units.clear(f);
                text.clear(f);
                slot.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Add);
                slot.store(f);
                builder.emit_branch_to_target(fast_loop, f);
                builder.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                s.release_i32_local(count32, f);
                f.instruction(&Instruction::Else);
                index.set_constant(0, f);
                let copied = builder.open_frame(ControlFrameKind::Block, f);
                let copy = builder.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                length.load(f);
                f.instruction(&Instruction::I64GeU);
                builder.emit_branch_if_to_target(copied, f);
                separator_length.load(f);
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::I32Ne);
                index.load(f);
                f.instruction(&Instruction::I64Eqz);
                f.instruction(&Instruction::I32Eqz);
                f.instruction(&Instruction::I32And);
                builder.open_frame(ControlFrameKind::If, f);
                single_sep.load(f);
                builder.open_frame(ControlFrameKind::If, f);
                construction.write(offset, sep_unit, s, f);
                offset.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Add);
                offset.store(f);
                f.instruction(&Instruction::Else);
                construction.copy_concatenated_units_from(
                    offset,
                    &separator_units,
                    separator_length,
                    s,
                    f,
                );
                offset.load(f);
                separator_length.load(f);
                f.instruction(&Instruction::I32Add);
                offset.store(f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                // No user code occurs between proof and these reads. The backing,
                // separator and construction remain GC roots across allocation.
                index.load(f);
                f.instruction(&Instruction::I32WrapI64);
                slot.store(f);
                let stored = s.reserve_gc_local(f).initialize(
                    s.array_type::<PackedValueTable>()
                        .read(&values, slot, s, f)
                        .reference()
                        .require_non_null(f),
                    f,
                );
                s.field(StoredValueSchema::TAG)
                    .read(&stored, s, f)
                    .store(tag, f);
                tag.load(f);
                f.instruction(&Instruction::I32Const(ValueKind::String.tag() as i32));
                f.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, f);
                s.field(StoredValueSchema::REFERENCE)
                    .read(&stored, s, f)
                    .store_eq(element.reference(), f);
                let text = s
                    .reserve_gc_local(f)
                    .initialize(element.cast_reference::<StringValue>(s, f), f);
                let units = s.reserve_gc_local(f).initialize(
                    s.field(StringValueSchema::CODE_UNITS)
                        .read(&text, s, f)
                        .reference(),
                    f,
                );
                s.array_type::<CodeUnitArray>().length(&units, s, f);
                part_length.store(f);
                construction.copy_concatenated_units_from(offset, &units, part_length, s, f);
                offset.load(f);
                part_length.load(f);
                f.instruction(&Instruction::I32Add);
                offset.store(f);
                units.clear(f);
                text.clear(f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                stored.clear(f);
                builder.emit_increment_local(index, 1, f);
                builder.emit_branch_to_target(copy, f);
                builder.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                s.release_i32_local(sep_unit, f);
                s.release_i32_local(single_sep, f);
                let text = s
                    .reserve_gc_local(f)
                    .initialize(construction.publish(s, f), f);
                result.set_reference(&text, s, f);
                text.clear(f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                s.release_i32_local(all_strings, f);
                s.release_i32_local(tag, f);
                s.release_i32_local(slot, f);
                values.clear(f);
                handled.set_constant(1, f);
                Ok(())
            },
        )?;
        separator_units.clear(f);
        element.clear(f);
        s.release_i32_local(offset, f);
        s.release_i32_local(separator_length, f);
        s.release_i32_local(part_length, f);
        s.release_i32_local(found, f);
        s.release_i64_local(total, f);
        s.release_i64_local(index, f);
        Ok(handled)
    }
}
