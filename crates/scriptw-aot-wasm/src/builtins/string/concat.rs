//! Convert each argument once, in order, then copy immutable UTF-16 parts once.
use super::*;

impl FunctionBuilder<'_> {
    pub(super) fn emit_batched_string_concat(
        &mut self,
        receiver: &GcLocal<StringValue>,
        arguments: &GcLocal<ValueArray>,
        count: I32Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let parts = s.reserve_gc_local(f).initialize(
            s.array_type::<PooledStringTable>()
                .filled(GcOperand::null(s), count, f),
            f,
        );
        let receiver_units = s.reserve_gc_local(f).initialize(
            s.struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(receiver, s, f)
                .reference(),
            f,
        );
        let total = s.reserve_i32_local(f);
        let index = s.reserve_i32_local(f);
        let length = s.reserve_i32_local(f);
        let next_length = s.reserve_i32_local(f);
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        s.array_type::<CodeUnitArray>()
            .length(&receiver_units, s, f);
        total.store(f);
        index.set_constant(0, f);
        let converted = self.open_frame(ControlFrameKind::Block, f);
        let convert_next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(converted, f);
        self.emit_argument_vector_entry_to_value(arguments, index, &argument, f);
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        self.emit_native_string_abrupt_exit(&pending, output, exit, f);
        let part = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        let units = s.reserve_gc_local(f).initialize(
            s.struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(&part, s, f)
                .reference(),
            f,
        );
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        length.store(f);
        total.load(f);
        length.load(f);
        f.instruction(&Instruction::I32Add);
        next_length.store(f);
        next_length.load(f);
        total.store(f);
        s.array_type::<PooledStringTable>().write(
            &parts,
            index,
            GcOperand::nullable_reference(&part, s),
            s,
            f,
        );
        units.clear(f);
        part.clear(f);
        // R + nextString past the implementation's maximum String length is
        // a RangeError (`MAX_STRING_LENGTH`), thrown before a later
        // argument's observable ToString runs. Both lengths are at most
        // 2^31 - 1 and the running total at most the limit, so the unsigned
        // sum cannot wrap.
        next_length.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
        f.instruction(&Instruction::I64GtU);
        self.emit_native_string_error_if(
            RuntimeErrorMessage::STRING_LENGTH_EXCEEDS_IMPLEMENTATION_LIMIT,
            NativeErrorKind::RangeError,
            output,
            exit,
            f,
        )?;
        argument.set_undefined(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(convert_next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // Only the completed conversion loop reaches this exact-size allocation.
        let construction = StringConstruction::allocate(s, s.reserve_gc_local(f), total, f);
        index.set_constant(0, f);
        s.array_type::<CodeUnitArray>()
            .length(&receiver_units, s, f);
        length.store(f);
        construction.copy_units_from(index, &receiver_units, length, s, f);
        let destination = s.reserve_i32_local(f);
        length.load(f);
        destination.store(f);
        let copied = self.open_frame(ControlFrameKind::Block, f);
        let copy_next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(copied, f);
        let retained = s.reserve_gc_local(f).initialize(
            s.array_type::<PooledStringTable>()
                .read(&parts, index, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let units = s.reserve_gc_local(f).initialize(
            s.struct_type::<StringValue>()
                .field(StringValueSchema::CODE_UNITS)
                .read(&retained, s, f)
                .reference(),
            f,
        );
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        length.store(f);
        construction.copy_units_from(destination, &units, length, s, f);
        destination.load(f);
        length.load(f);
        f.instruction(&Instruction::I32Add);
        destination.store(f);
        units.clear(f);
        retained.clear(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(copy_next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let value = s
            .reserve_gc_local(f)
            .initialize(construction.publish(s, f), f);
        self.emit_native_string_normal_reference(&value, output, f);
        value.clear(f);
        pending.clear(f);
        argument.clear(f);
        s.release_i32_local(destination, f);
        s.release_i32_local(next_length, f);
        s.release_i32_local(length, f);
        s.release_i32_local(index, f);
        s.release_i32_local(total, f);
        receiver_units.clear(f);
        parts.clear(f);
        Ok(())
    }
}
