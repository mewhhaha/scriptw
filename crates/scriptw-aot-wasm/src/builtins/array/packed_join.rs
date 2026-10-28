//! Exact-size join for a stable packed snapshot of present primitive strings,
//! null and undefined. Unsupported values return to the ordered generic loop.
use super::*;
use crate::gc_types::{
    CodeUnitArray, Nullable, StringConstruction, StringValue, StringValueSchema,
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
                index.set_constant(0, f);
                total.set_constant(0, f);
                let validated = builder.open_frame(ControlFrameKind::Block, f);
                let validate = builder.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                length.load(f);
                f.instruction(&Instruction::I64GeU);
                builder.emit_branch_if_to_target(validated, f);
                packed.read(builder, index, &element, found, f);
                found.load(f);
                f.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(decline, f);
                part_length.set_constant(0, f);
                element.tag().load(f);
                f.instruction(&Instruction::I32Const(ValueKind::String.tag() as i32));
                f.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, f);
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
                units.clear(f);
                text.clear(f);
                f.instruction(&Instruction::Else);
                element.tag().load(f);
                f.instruction(&Instruction::I32Const(ValueKind::Null.tag() as i32));
                f.instruction(&Instruction::I32Eq);
                element.tag().load(f);
                f.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
                f.instruction(&Instruction::I32Eq);
                f.instruction(&Instruction::I32Or);
                f.instruction(&Instruction::I32Eqz);
                builder.emit_branch_if_to_target(decline, f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                index.load(f);
                f.instruction(&Instruction::I64Eqz);
                f.instruction(&Instruction::I32Eqz);
                builder.open_frame(ControlFrameKind::If, f);
                total.load(f);
                separator_length.load(f);
                f.instruction(&Instruction::I64ExtendI32U);
                f.instruction(&Instruction::I64Add);
                total.store(f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                total.load(f);
                part_length.load(f);
                f.instruction(&Instruction::I64ExtendI32U);
                f.instruction(&Instruction::I64Add);
                total.store(f);
                // Defer resource failure until the entire supported-value proof.
                // Saturation prevents a huge captured length from wrapping i64.
                total.load(f);
                f.instruction(&Instruction::I64Const(u32::MAX as i64));
                f.instruction(&Instruction::I64GtU);
                builder.open_frame(ControlFrameKind::If, f);
                total.set_constant(u32::MAX as i64 + 1, f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                builder.emit_increment_local(index, 1, f);
                builder.emit_branch_to_target(validate, f);
                builder.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                total.load(f);
                f.instruction(&Instruction::I64Const(u32::MAX as i64));
                f.instruction(&Instruction::I64GtU);
                builder.open_frame(ControlFrameKind::If, f);
                f.instruction(&Instruction::Unreachable);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
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
                result.copy_from(&element, f);
                f.instruction(&Instruction::Else);
                total.load(f);
                f.instruction(&Instruction::I32WrapI64);
                part_length.store(f);
                let construction =
                    StringConstruction::allocate(s, s.reserve_gc_local(f), part_length, f);
                offset.set_constant(0, f);
                index.set_constant(0, f);
                let copied = builder.open_frame(ControlFrameKind::Block, f);
                let copy = builder.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                length.load(f);
                f.instruction(&Instruction::I64GeU);
                builder.emit_branch_if_to_target(copied, f);
                index.load(f);
                f.instruction(&Instruction::I64Eqz);
                f.instruction(&Instruction::I32Eqz);
                builder.open_frame(ControlFrameKind::If, f);
                construction.copy_units_from(offset, &separator_units, separator_length, s, f);
                offset.load(f);
                separator_length.load(f);
                f.instruction(&Instruction::I32Add);
                offset.store(f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                // No user code occurs between proof and these reads. The backing,
                // separator and construction remain GC roots across allocation.
                packed.read(builder, index, &element, found, f);
                element.tag().load(f);
                f.instruction(&Instruction::I32Const(ValueKind::String.tag() as i32));
                f.instruction(&Instruction::I32Eq);
                builder.open_frame(ControlFrameKind::If, f);
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
                construction.copy_units_from(offset, &units, part_length, s, f);
                offset.load(f);
                part_length.load(f);
                f.instruction(&Instruction::I32Add);
                offset.store(f);
                units.clear(f);
                text.clear(f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                builder.emit_increment_local(index, 1, f);
                builder.emit_branch_to_target(copy, f);
                builder.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                let text = s
                    .reserve_gc_local(f)
                    .initialize(construction.publish(s, f), f);
                result.set_reference(&text, s, f);
                text.clear(f);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                builder.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
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
