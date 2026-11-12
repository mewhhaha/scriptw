use super::*;

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

    /// Snapshot the selected byte range, including non-byte TypedArrays and
    /// DataViews. Backing admission happens after dictionary getters.
    pub(super) fn emit_text_decoder_input(
        &mut self,
        input: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<ByteArray>, EmitError> {
        let s = self.runtime_schema();
        let extent = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        extent.store(f);
        let result = s.reserve_gc_local(f).initialize(
            s.array_type::<ByteArray>()
                .filled(GcOperand::i32(0), extent, f),
            f,
        );
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
        f.instruction(&Instruction::I32Eqz);
        self.emit_text_codec_error_if(
            RuntimeErrorMessage::TEXT_DECODER_INVALID_INPUT,
            output,
            exit,
            f,
        )?;
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
        // An ArrayBuffer's validated live window is one bulk copy. Shared
        // backing keeps its per-byte host-memory reads below.
        access.shared.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let source = s
            .reserve_gc_local(f)
            .initialize(access.bytes.load(s, f).require_non_null(f), f);
        let zero = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        zero.store(f);
        offset.load(f);
        f.instruction(&Instruction::I32WrapI64);
        at.store(f);
        s.array_type::<ByteArray>()
            .copy_range_from(&bytes, zero, &source, at, extent, s, f);
        s.release_i32_local(zero, f);
        source.clear(f);
        length.load(f);
        index.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
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
        result.replace(bytes.load(s, f), f);
        bytes.clear(f);
        for local in [byte, at, valid] {
            s.release_i32_local(local, f);
        }
        for local in [position, index, length, offset] {
            s.release_i64_local(local, f);
        }
        access.clear(s, f);
        selected.clear(f);
        owner.clear(f);
        view.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(extent, f);
        Ok(result)
    }
}
