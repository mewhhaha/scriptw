//! Checked immutable GC program sections. Every dynamic read retains its extent.
use super::compiler::CompilerLocals;
use super::*;
use crate::gc_types::{
    CodeUnitArray, CompletionLocals, GcLocal, GcLocalSlot, GcStackReference, I64Local,
    ImmutableByteArray, NonNullable, RegExpProgram, RegExpProgramSchema, StringConstruction,
    StringValue,
};
use scriptw_ir::{
    NativeErrorKind, RegExpProgramWord, RegExpRepeatBoundWord, RegExpRepeatMaximumKind,
    REGEXP_MAX_INSTRUCTIONS, REGEXP_MAX_RANGE_ENTRIES, REGEXP_PROGRAM_HEADER_SIZE,
    REGEXP_PROGRAM_MAGIC_VERSION, REGEXP_REPEAT_BOUND_RECORD_SIZE,
    REGEXP_REPEAT_COUNTER_DECIMAL_DIGITS, REGEXP_REPEAT_COUNTER_LIMB_WIDTH,
    REGEXP_REPEAT_STATE_HEADER_SIZE,
};

mod counted_regions;
mod repeat_bounds;

struct ProgramSections {
    end: I64Local,
    instructions: I64Local,
    instruction_count: I64Local,
    capture_count: I64Local,
    ranges: I64Local,
    range_count: I64Local,
    range_end: I64Local,
    split_count: I64Local,
    repeatable_split_count: I64Local,
    repeat_slot_count: I64Local,
    repeat_bounds: I64Local,
    repeat_end: I64Local,
    repeat_state_byte_length: I64Local,
    named_groups: I64Local,
}

#[must_use = "a pending program must pass validation"]
pub(in crate::builtins) struct PendingRegExpProgramLayoutLocals {
    sections: ProgramSections,
    bytes: GcLocalSlot<ImmutableByteArray>,
}
#[must_use = "a completed program view must be released"]
pub(in crate::builtins) struct ValidatedRegExpProgramLayoutLocals {
    sections: ProgramSections,
    bytes: GcLocal<ImmutableByteArray>,
}

#[derive(Clone, Copy)]
pub(in crate::builtins) enum RegExpProgramLayoutFailure<'a> {
    Matcher,
    Compiler(&'a CompilerLocals),
    Exec {
        result: &'a CompletionLocals,
        exit: ControlTarget,
    },
}

impl RegExpProgramLayoutFailure<'_> {
    fn emit(
        self,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        match self {
            Self::Matcher => {
                function.instruction(&Instruction::I32Const(0));
                function.instruction(&Instruction::I64Const(0));
                function.instruction(&Instruction::I64Const(0));
                function.instruction(&Instruction::I32Const(
                    RegExpMatcherFailure::CorruptProgram.abi_word() as i32,
                ));
                function.instruction(&Instruction::Return);
            }
            Self::Compiler(compiler) => {
                builder.emit_regexp_compile_corrupt_program_failure(compiler, function)
            }
            Self::Exec { result, exit } => {
                builder.emit_throw_runtime_error(
                    NativeErrorKind::Error,
                    RegExpMatcherFailure::CorruptProgram.message(),
                    result,
                    function,
                )?;
                builder.emit_branch_to_target(exit, function);
            }
        }
        Ok(())
    }
    // The stack condition means invalid. A failure owns its complete exit.
    fn if_invalid(
        self,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        builder.open_frame(ControlFrameKind::If, function);
        self.emit(builder, function)?;
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }
}

impl ValidatedRegExpProgramLayoutLocals {
    pub(in crate::builtins) const fn instructions(&self) -> I64Local {
        self.sections.instructions
    }
    pub(in crate::builtins) const fn instruction_count(&self) -> I64Local {
        self.sections.instruction_count
    }
    pub(in crate::builtins) const fn capture_count(&self) -> I64Local {
        self.sections.capture_count
    }
    pub(in crate::builtins) const fn ranges(&self) -> I64Local {
        self.sections.ranges
    }
    pub(in crate::builtins) const fn range_count(&self) -> I64Local {
        self.sections.range_count
    }
    pub(in crate::builtins) const fn end(&self) -> I64Local {
        self.sections.end
    }
    pub(in crate::builtins) const fn repeat_slot_count(&self) -> I64Local {
        self.sections.repeat_slot_count
    }
    pub(in crate::builtins) const fn repeat_bounds(&self) -> I64Local {
        self.sections.repeat_bounds
    }
    pub(in crate::builtins) const fn repeat_state_byte_length(&self) -> I64Local {
        self.sections.repeat_state_byte_length
    }
    pub(in crate::builtins) const fn named_groups(&self) -> I64Local {
        self.sections.named_groups
    }

    fn require_range(
        &self,
        offset: I64Local,
        width: I64Local,
        builder: &mut FunctionBuilder<'_>,
        failure: RegExpProgramLayoutFailure<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        // Subtraction follows offset<=end, so wrapping additions cannot authorize a read.
        offset.load(function);
        self.sections.end.load(function);
        function.instruction(&Instruction::I64GtU);
        failure.if_invalid(builder, function)?;
        width.load(function);
        self.sections.end.load(function);
        offset.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64GtU);
        failure.if_invalid(builder, function)
    }
    fn read_byte_in_range(
        &self,
        offset: I64Local,
        output: I64Local,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let schema = builder.runtime_schema();
        let index = schema.reserve_i32_local(function);
        let byte = schema.reserve_i32_local(function);
        offset.load(function);
        function.instruction(&Instruction::I32WrapI64);
        index.store(function);
        schema
            .array_type::<ImmutableByteArray>()
            .read(&self.bytes, index, schema, function)
            .store(byte, function);
        byte.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        output.store(function);
        schema.release_i32_local(byte, function);
        schema.release_i32_local(index, function);
    }
    pub(in crate::builtins) fn read_byte(
        &self,
        offset: I64Local,
        output: I64Local,
        builder: &mut FunctionBuilder<'_>,
        failure: RegExpProgramLayoutFailure<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = builder.runtime_schema();
        let width = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(1));
        width.store(function);
        self.require_range(offset, width, builder, failure, function)?;
        self.read_byte_in_range(offset, output, builder, function);
        schema.release_i64_local(width, function);
        Ok(())
    }
    pub(in crate::builtins) fn read_word(
        &self,
        offset: I64Local,
        output: I64Local,
        builder: &mut FunctionBuilder<'_>,
        failure: RegExpProgramLayoutFailure<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = builder.runtime_schema();
        let width = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(8));
        width.store(function);
        self.require_range(offset, width, builder, failure, function)?;
        self.read_word_in_range(offset, output, builder, function);
        schema.release_i64_local(width, function);
        Ok(())
    }
    fn read_word_in_range(
        &self,
        offset: I64Local,
        output: I64Local,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let schema = builder.runtime_schema();
        let cursor = schema.reserve_i64_local(function);
        let byte = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(0));
        output.store(function);
        for index in 0..8 {
            offset.load(function);
            function.instruction(&Instruction::I64Const(index));
            function.instruction(&Instruction::I64Add);
            cursor.store(function);
            self.read_byte_in_range(cursor, byte, builder, function);
            output.load(function);
            byte.load(function);
            function.instruction(&Instruction::I64Const(index * 8));
            function.instruction(&Instruction::I64Shl);
            function.instruction(&Instruction::I64Or);
            output.store(function);
        }
        schema.release_i64_local(byte, function);
        schema.release_i64_local(cursor, function);
    }

    /// The encoded name table uses canonical UTF-8, never WTF-8 or a String handle.
    /// A first pass validates the complete range and counts exact UTF-16 units.
    pub(in crate::builtins) fn read_utf8_string(
        &self,
        start: I64Local,
        length: I64Local,
        builder: &mut FunctionBuilder<'_>,
        failure: RegExpProgramLayoutFailure<'_>,
        function: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        let schema = builder.runtime_schema();
        self.require_range(start, length, builder, failure, function)?;
        let end = schema.reserve_i64_local(function);
        let cursor = schema.reserve_i64_local(function);
        let point = schema.reserve_i64_local(function);
        let width = schema.reserve_i64_local(function);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        let unit = schema.reserve_i32_local(function);
        start.load(function);
        length.load(function);
        function.instruction(&Instruction::I64Add);
        end.store(function);
        start.load(function);
        cursor.store(function);
        function.instruction(&Instruction::I32Const(0));
        count.store(function);
        let counted = builder.open_frame(ControlFrameKind::Block, function);
        let count_loop = builder.open_frame(ControlFrameKind::Loop, function);
        cursor.load(function);
        end.load(function);
        function.instruction(&Instruction::I64GeU);
        builder.emit_branch_if_to_target(counted, function);
        self.decode_utf8(cursor, end, point, width, builder, failure, function)?;
        count.load(function);
        point.load(function);
        function.instruction(&Instruction::I64Const(0xffff));
        function.instruction(&Instruction::I64GtU);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::I32Add);
        count.store(function);
        cursor.load(function);
        width.load(function);
        function.instruction(&Instruction::I64Add);
        cursor.store(function);
        builder.emit_branch_to_target(count_loop, function);
        builder.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let string = StringConstruction::allocate(
            schema,
            schema.reserve_gc_local::<CodeUnitArray, NonNullable>(function),
            count,
            function,
        );
        start.load(function);
        cursor.store(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let copied = builder.open_frame(ControlFrameKind::Block, function);
        let copy_loop = builder.open_frame(ControlFrameKind::Loop, function);
        cursor.load(function);
        end.load(function);
        function.instruction(&Instruction::I64GeU);
        builder.emit_branch_if_to_target(copied, function);
        self.decode_utf8(cursor, end, point, width, builder, failure, function)?;
        point.load(function);
        function.instruction(&Instruction::I64Const(0xffff));
        function.instruction(&Instruction::I64GtU);
        builder.open_frame(ControlFrameKind::If, function);
        point.load(function);
        function.instruction(&Instruction::I64Const(0x10000));
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(10));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I64Const(0xd800));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        unit.store(function);
        string.write(index, unit, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        point.load(function);
        function.instruction(&Instruction::I64Const(0x3ff));
        function.instruction(&Instruction::I64And);
        function.instruction(&Instruction::I64Const(0xdc00));
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I32WrapI64);
        unit.store(function);
        function.instruction(&Instruction::Else);
        point.load(function);
        function.instruction(&Instruction::I32WrapI64);
        unit.store(function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        string.write(index, unit, schema, function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        cursor.load(function);
        width.load(function);
        function.instruction(&Instruction::I64Add);
        cursor.store(function);
        builder.emit_branch_to_target(copy_loop, function);
        builder.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        builder.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let result = string.publish(schema, function);
        schema.release_i32_local(unit, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        schema.release_i64_local(width, function);
        schema.release_i64_local(point, function);
        schema.release_i64_local(cursor, function);
        schema.release_i64_local(end, function);
        Ok(result)
    }
    fn decode_utf8(
        &self,
        cursor: I64Local,
        end: I64Local,
        point: I64Local,
        width: I64Local,
        builder: &mut FunctionBuilder<'_>,
        failure: RegExpProgramLayoutFailure<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = builder.runtime_schema();
        let byte = schema.reserve_i64_local(function);
        let offset = schema.reserve_i64_local(function);
        let continuation = schema.reserve_i64_local(function);
        self.read_byte_in_range(cursor, byte, builder, function);
        function.instruction(&Instruction::I64Const(0));
        width.store(function);
        // Width=0 rejects continuation starts, overlong prefixes and values >U+10FFFF.
        for (low, high, size, mask) in [
            (0, 0x7f, 1, 0x7f),
            (0xc2, 0xdf, 2, 0x1f),
            (0xe0, 0xef, 3, 0xf),
            (0xf0, 0xf4, 4, 7),
        ] {
            byte.load(function);
            function.instruction(&Instruction::I64Const(low));
            function.instruction(&Instruction::I64GeU);
            byte.load(function);
            function.instruction(&Instruction::I64Const(high));
            function.instruction(&Instruction::I64LeU);
            function.instruction(&Instruction::I32And);
            builder.open_frame(ControlFrameKind::If, function);
            function.instruction(&Instruction::I64Const(size));
            width.store(function);
            byte.load(function);
            function.instruction(&Instruction::I64Const(mask));
            function.instruction(&Instruction::I64And);
            point.store(function);
            builder.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        width.load(function);
        function.instruction(&Instruction::I64Eqz);
        failure.if_invalid(builder, function)?;
        width.load(function);
        end.load(function);
        cursor.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64GtU);
        failure.if_invalid(builder, function)?;
        for delta in 1..4 {
            width.load(function);
            function.instruction(&Instruction::I64Const(delta));
            function.instruction(&Instruction::I64GtU);
            builder.open_frame(ControlFrameKind::If, function);
            cursor.load(function);
            function.instruction(&Instruction::I64Const(delta));
            function.instruction(&Instruction::I64Add);
            offset.store(function);
            self.read_byte_in_range(offset, continuation, builder, function);
            continuation.load(function);
            function.instruction(&Instruction::I64Const(0xc0));
            function.instruction(&Instruction::I64And);
            function.instruction(&Instruction::I64Const(0x80));
            function.instruction(&Instruction::I64Ne);
            failure.if_invalid(builder, function)?;
            point.load(function);
            function.instruction(&Instruction::I64Const(6));
            function.instruction(&Instruction::I64Shl);
            continuation.load(function);
            function.instruction(&Instruction::I64Const(0x3f));
            function.instruction(&Instruction::I64And);
            function.instruction(&Instruction::I64Or);
            point.store(function);
            builder.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for (size, minimum) in [(2, 0x80), (3, 0x800), (4, 0x10000)] {
            width.load(function);
            function.instruction(&Instruction::I64Const(size));
            function.instruction(&Instruction::I64Eq);
            point.load(function);
            function.instruction(&Instruction::I64Const(minimum));
            function.instruction(&Instruction::I64LtU);
            function.instruction(&Instruction::I32And);
            failure.if_invalid(builder, function)?;
        }
        point.load(function);
        function.instruction(&Instruction::I64Const(0x10ffff));
        function.instruction(&Instruction::I64GtU);
        point.load(function);
        function.instruction(&Instruction::I64Const(0xd800));
        function.instruction(&Instruction::I64GeU);
        point.load(function);
        function.instruction(&Instruction::I64Const(0xdfff));
        function.instruction(&Instruction::I64LeU);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Or);
        failure.if_invalid(builder, function)?;
        schema.release_i64_local(continuation, function);
        schema.release_i64_local(offset, function);
        schema.release_i64_local(byte, function);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn reserve_regexp_program_layout(
        &mut self,
        function: &mut Function,
    ) -> PendingRegExpProgramLayoutLocals {
        let schema = self.runtime_schema();
        PendingRegExpProgramLayoutLocals {
            sections: ProgramSections {
                end: schema.reserve_i64_local(function),
                instructions: schema.reserve_i64_local(function),
                instruction_count: schema.reserve_i64_local(function),
                capture_count: schema.reserve_i64_local(function),
                ranges: schema.reserve_i64_local(function),
                range_count: schema.reserve_i64_local(function),
                range_end: schema.reserve_i64_local(function),
                split_count: schema.reserve_i64_local(function),
                repeatable_split_count: schema.reserve_i64_local(function),
                repeat_slot_count: schema.reserve_i64_local(function),
                repeat_bounds: schema.reserve_i64_local(function),
                repeat_end: schema.reserve_i64_local(function),
                repeat_state_byte_length: schema.reserve_i64_local(function),
                named_groups: schema.reserve_i64_local(function),
            },
            bytes: schema.reserve_gc_local(function),
        }
    }
    pub(in crate::builtins) fn release_regexp_program_layout(
        &mut self,
        layout: ValidatedRegExpProgramLayoutLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        layout.bytes.clear(function);
        for local in [
            layout.sections.named_groups,
            layout.sections.repeat_slot_count,
            layout.sections.repeat_bounds,
            layout.sections.repeat_end,
            layout.sections.repeat_state_byte_length,
            layout.sections.repeatable_split_count,
            layout.sections.split_count,
            layout.sections.range_end,
            layout.sections.range_count,
            layout.sections.ranges,
            layout.sections.capture_count,
            layout.sections.instruction_count,
            layout.sections.instructions,
            layout.sections.end,
        ] {
            schema.release_i64_local(local, function);
        }
    }
    pub(in crate::builtins) fn emit_validate_regexp_program_layout(
        &mut self,
        program: &GcLocal<RegExpProgram>,
        pending: PendingRegExpProgramLayoutLocals,
        failure: RegExpProgramLayoutFailure<'_>,
        function: &mut Function,
    ) -> Result<ValidatedRegExpProgramLayoutLocals, EmitError> {
        let schema = self.runtime_schema();
        let view = ValidatedRegExpProgramLayoutLocals {
            sections: pending.sections,
            bytes: pending.bytes.initialize(
                schema
                    .field(RegExpProgramSchema::ENCODED_BYTES)
                    .read(program, schema, function)
                    .reference(),
                function,
            ),
        };
        let valid = schema.reserve_i32_local(function);
        let offset = schema.reserve_i64_local(function);
        let word = schema.reserve_i64_local(function);
        schema
            .array_type::<ImmutableByteArray>()
            .length(&view.bytes, schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
        view.sections.end.store(function);
        function.instruction(&Instruction::I32Const(0));
        valid.store(function);
        let rejected = self.open_frame(ControlFrameKind::Block, function);
        view.sections.end.load(function);
        function.instruction(&Instruction::I64Const(REGEXP_PROGRAM_HEADER_SIZE as i64));
        function.instruction(&Instruction::I64LtU);
        self.emit_branch_if_to_target(rejected, function);
        for (field, destination) in [
            (RegExpProgramWord::MagicVersion, word),
            (RegExpProgramWord::ByteLength, offset),
            (
                RegExpProgramWord::InstructionCount,
                view.sections.instruction_count,
            ),
            (RegExpProgramWord::CaptureCount, view.sections.capture_count),
            (RegExpProgramWord::RangeCount, view.sections.range_count),
            (RegExpProgramWord::SplitCount, view.sections.split_count),
            (
                RegExpProgramWord::RepeatSlotCount,
                view.sections.repeat_slot_count,
            ),
            (
                RegExpProgramWord::RepeatStateByteLength,
                view.sections.repeat_state_byte_length,
            ),
            (
                RegExpProgramWord::RepeatableSplitCount,
                view.sections.repeatable_split_count,
            ),
            (
                RegExpProgramWord::NamedGroupTableOffset,
                view.sections.named_groups,
            ),
        ] {
            // All ten words lie in the already-authorized complete header.
            let address = schema.reserve_i64_local(function);
            function.instruction(&Instruction::I64Const(field.offset() as i64));
            address.store(function);
            view.read_word_in_range(address, destination, self, function);
            schema.release_i64_local(address, function);
        }
        word.load(function);
        function.instruction(&Instruction::I64Const(REGEXP_PROGRAM_MAGIC_VERSION as i64));
        function.instruction(&Instruction::I64Ne);
        self.emit_branch_if_to_target(rejected, function);
        offset.load(function);
        view.sections.end.load(function);
        function.instruction(&Instruction::I64Ne);
        self.emit_branch_if_to_target(rejected, function);
        view.sections.instruction_count.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(rejected, function);
        for (value, maximum) in [
            (
                view.sections.instruction_count,
                REGEXP_MAX_INSTRUCTIONS as i64,
            ),
            (view.sections.capture_count, u32::MAX as i64),
            (view.sections.range_count, REGEXP_MAX_RANGE_ENTRIES as i64),
        ] {
            value.load(function);
            function.instruction(&Instruction::I64Const(maximum));
            function.instruction(&Instruction::I64GtU);
            self.emit_branch_if_to_target(rejected, function);
        }
        for (value, maximum) in [
            (
                view.sections.repeat_slot_count,
                view.sections.instruction_count,
            ),
            (view.sections.split_count, view.sections.instruction_count),
            (
                view.sections.repeatable_split_count,
                view.sections.split_count,
            ),
        ] {
            value.load(function);
            maximum.load(function);
            function.instruction(&Instruction::I64GtU);
            self.emit_branch_if_to_target(rejected, function);
        }
        function.instruction(&Instruction::I64Const(REGEXP_PROGRAM_HEADER_SIZE as i64));
        view.sections.instructions.store(function);
        view.sections.instructions.load(function);
        view.sections.instruction_count.load(function);
        function.instruction(&Instruction::I64Const(REGEXP_INSTRUCTION_WIDTH as i64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        view.sections.ranges.store(function);
        view.sections.ranges.load(function);
        view.sections.range_count.load(function);
        function.instruction(&Instruction::I64Const(REGEXP_RANGE_ENTRY_WIDTH as i64));
        function.instruction(&Instruction::I64Mul);
        function.instruction(&Instruction::I64Add);
        view.sections.range_end.store(function);
        view.sections.range_end.load(function);
        view.sections.end.load(function);
        function.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(rejected, function);
        self.emit_validate_regexp_repeat_bounds(&view, rejected, function);
        self.emit_validate_regexp_counted_regions(&view, rejected, function);
        view.sections.named_groups.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        view.sections.repeat_end.load(function);
        view.sections.end.load(function);
        function.instruction(&Instruction::I64Ne);
        self.emit_branch_if_to_target(rejected, function);
        function.instruction(&Instruction::Else);
        view.sections.named_groups.load(function);
        view.sections.repeat_end.load(function);
        function.instruction(&Instruction::I64Ne);
        self.emit_branch_if_to_target(rejected, function);
        view.sections.end.load(function);
        view.sections.repeat_end.load(function);
        function.instruction(&Instruction::I64Sub);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64LtU);
        self.emit_branch_if_to_target(rejected, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::I32Const(1));
        valid.store(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        valid.load(function);
        function.instruction(&Instruction::I32Eqz);
        failure.if_invalid(self, function)?;
        schema.release_i64_local(word, function);
        schema.release_i64_local(offset, function);
        schema.release_i32_local(valid, function);
        Ok(view)
    }
}
