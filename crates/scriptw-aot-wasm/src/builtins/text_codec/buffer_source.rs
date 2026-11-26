use super::*;

pub(super) struct TextDecoderInput {
    pub(super) bytes: GcLocal<ByteArray>,
    pub(super) start: I32Local,
    pub(super) length: I32Local,
}

impl TextDecoderInput {
    pub(super) fn clear(self, s: &RuntimeSchema, f: &mut Function) {
        self.bytes.clear(f);
        s.release_i32_local(self.start, f);
        s.release_i32_local(self.length, f);
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_text_decoder_input_brand(
        &mut self,
        input: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        input.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        for heap in [
            s.reference_type::<ArrayBuffer>(GcNullability::NonNullable)
                .heap_type,
            s.reference_type::<SharedArrayBuffer>(GcNullability::NonNullable)
                .heap_type,
            s.reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                .heap_type,
            s.reference_type::<DataViewObject>(GcNullability::NonNullable)
                .heap_type,
        ] {
            input.reference().load(f);
            f.instruction(&Instruction::RefTestNonNull(heap));
            f.instruction(&Instruction::I32Or);
        }
        f.instruction(&Instruction::I32Eqz);
        self.emit_text_codec_error_if(
            RuntimeErrorMessage::TEXT_DECODER_INVALID_INPUT,
            output,
            exit,
            f,
        )
    }

    /// Borrow the validated ordinary backing after dictionary getters have
    /// run. Shared memory still needs a private snapshot before decoding.
    pub(super) fn emit_text_decoder_input(
        &mut self,
        input: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<TextDecoderInput, EmitError> {
        let s = self.runtime_schema();
        let extent = s.reserve_i32_local(f);
        let start = s.reserve_i32_local(f);
        let result = s
            .reserve_gc_local::<ByteArray, Nullable>(f)
            .initialize_null(s, f);
        input.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        let view = s
            .reserve_gc_local::<BufferView, Nullable>(f)
            .initialize_null(s, f);
        let owner = s
            .reserve_gc_local::<BufferOwner, Nullable>(f)
            .initialize_null(s, f);
        for typed in [true, false] {
            input.reference().load(f);
            if typed {
                f.instruction(&Instruction::RefTestNonNull(
                    s.reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                        .heap_type,
                ));
            } else {
                f.instruction(&Instruction::RefTestNonNull(
                    s.reference_type::<DataViewObject>(GcNullability::NonNullable)
                        .heap_type,
                ));
            }
            self.open_frame(ControlFrameKind::If, f);
            if typed {
                let object = s
                    .reserve_gc_local(f)
                    .initialize(input.cast_reference::<TypedArrayObject>(s, f), f);
                view.replace(
                    s.field(TypedArrayObjectSchema::VIEW)
                        .read(&object, s, f)
                        .reference()
                        .nullable(),
                    f,
                );
                object.clear(f);
            } else {
                let object = s
                    .reserve_gc_local(f)
                    .initialize(input.cast_reference::<DataViewObject>(s, f), f);
                view.replace(
                    s.field(DataViewObjectSchema::VIEW)
                        .read(&object, s, f)
                        .reference()
                        .nullable(),
                    f,
                );
                object.clear(f);
            }
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        view.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        // The shared BufferOwner constructor performs the remaining genuine
        // ArrayBuffer/SharedArrayBuffer brands, with no property lookups.
        let selected = self.emit_binary_buffer_owner(input, output, exit, f)?;
        owner.replace(selected.load(s, f).nullable(), f);
        selected.clear(f);
        f.instruction(&Instruction::Else);
        owner.replace(
            s.field(BufferViewSchema::BUFFER)
                .read(&view, s, f)
                .reference()
                .nullable(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let selected = s
            .reserve_gc_local(f)
            .initialize(owner.load(s, f).require_non_null(f), f);
        let access = self.emit_binary_buffer_access(&selected, f);
        let offset = s.reserve_i64_local(f);
        let length = s.reserve_i64_local(f);
        let valid = s.reserve_i32_local(f);
        f.instruction(&Instruction::I64Const(0));
        offset.store(f);
        access.length.load(f);
        length.store(f);
        access.valid.load(f);
        valid.store(f);
        view.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.field(BufferViewSchema::BYTE_OFFSET)
            .read(&view, s, f)
            .store_i64(offset, f);
        let nonnull_view = s
            .reserve_gc_local(f)
            .initialize(view.load(s, f).require_non_null(f), f);
        self.emit_binary_view_length(&nonnull_view, &access, length, valid, f);
        nonnull_view.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        valid.load(f);
        self.open_frame(ControlFrameKind::If, f);
        length.load(f);
        f.instruction(&Instruction::I64Const(i32::MAX as i64));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        extent.store(f);
        access.shared.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let bytes = s.reserve_gc_local(f).initialize(
            s.array_type::<ByteArray>()
                .filled(GcOperand::i32(0), extent, f),
            f,
        );
        let index = s.reserve_i64_local(f);
        let position = s.reserve_i64_local(f);
        let at = s.reserve_i32_local(f);
        let byte = s.reserve_i32_local(f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        index.load(f);
        offset.load(f);
        f.instruction(&Instruction::I64Add);
        position.store(f);
        access.read_byte(position, byte, self, f)?;
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        at.store(f);
        s.array_type::<ByteArray>()
            .write(&bytes, at, GcOperand::i32_local(byte), s, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        result.replace(bytes.load(s, f).nullable(), f);
        start.set_constant(0, f);
        bytes.clear(f);
        for local in [byte, at] {
            s.release_i32_local(local, f);
        }
        for local in [position, index] {
            s.release_i64_local(local, f);
        }
        f.instruction(&Instruction::Else);
        result.replace(access.bytes.load(s, f), f);
        offset.load(f);
        f.instruction(&Instruction::I32WrapI64);
        start.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        // Brand admission already ran before the `stream` getter; a detached
        // buffer or out-of-bounds view yields an empty byte sequence per Web IDL.
        extent.set_constant(0, f);
        start.set_constant(0, f);
        result.replace(
            s.array_type::<ByteArray>()
                .filled(GcOperand::i32(0), extent, f)
                .nullable(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(valid, f);
        s.release_i64_local(length, f);
        s.release_i64_local(offset, f);
        access.clear(s, f);
        selected.clear(f);
        owner.clear(f);
        view.clear(f);
        f.instruction(&Instruction::Else);
        extent.set_constant(0, f);
        start.set_constant(0, f);
        result.replace(
            s.array_type::<ByteArray>()
                .filled(GcOperand::i32(0), extent, f)
                .nullable(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let bytes = s
            .reserve_gc_local(f)
            .initialize(result.load(s, f).require_non_null(f), f);
        result.clear(f);
        Ok(TextDecoderInput {
            bytes,
            start,
            length: extent,
        })
    }
}
