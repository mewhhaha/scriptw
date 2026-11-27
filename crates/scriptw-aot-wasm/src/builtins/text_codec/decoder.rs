use super::buffer_source::TextDecoderInput;
use super::*;

// The existing PENDING backing holds a byte count and three carry bytes.
// Fatal streaming errors can leave a longer queue, whose count is its
// backing length minus the header byte.
const CARRY_BYTES: i32 = 3;

struct TextDecoderBytes<'a> {
    current: &'a TextDecoderInput,
    prior: GcLocal<ByteArray>,
    prior_capacity: I32Local,
    prior_length: I32Local,
    current_offset: I32Local,
    total: I32Local,
}

impl TextDecoderBytes<'_> {
    fn read(
        &self,
        index: I32Local,
        byte: I32Local,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) {
        let s = builder.runtime_schema();
        let at = s.reserve_i32_local(f);
        let array = s.array_type::<ByteArray>();
        index.load(f);
        self.prior_length.load(f);
        f.instruction(&Instruction::I32LtU);
        builder.open_frame(ControlFrameKind::If, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        at.store(f);
        array.read(&self.prior, at, s, f).store(byte, f);
        f.instruction(&Instruction::Else);
        index.load(f);
        self.current_offset.load(f);
        f.instruction(&Instruction::I32Add);
        at.store(f);
        array.read(&self.current.bytes, at, s, f).store(byte, f);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(at, f);
    }

    fn retain_tail(
        &self,
        decoder: &GcLocal<TextDecoderObject>,
        from: I32Local,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) {
        let s = builder.runtime_schema();
        let array = s.array_type::<ByteArray>();
        let count = s.reserve_i32_local(f);
        let queued = s.reserve_i32_local(f);
        let source = s.reserve_i32_local(f);
        let destination = s.reserve_i32_local(f);
        self.total.load(f);
        from.load(f);
        f.instruction(&Instruction::I32Sub);
        count.store(f);

        let tail = s.reserve_gc_local(f).initialize(self.prior.load(s, f), f);
        self.prior_capacity.load(f);
        f.instruction(&Instruction::I32Const(CARRY_BYTES + 1));
        f.instruction(&Instruction::I32Ne);
        count.load(f);
        f.instruction(&Instruction::I32Const(CARRY_BYTES));
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::I32Or);
        builder.open_frame(ControlFrameKind::If, f);
        let capacity = s.reserve_i32_local(f);
        count.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Const(CARRY_BYTES + 1));
        count.load(f);
        f.instruction(&Instruction::I32Const(CARRY_BYTES));
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::Select);
        capacity.store(f);
        tail.replace(array.filled(GcOperand::i32(0), capacity, f), f);
        s.release_i32_local(capacity, f);
        s.field(TextDecoderObjectSchema::PENDING).write(
            decoder,
            GcOperand::reference(&tail, s),
            s,
            f,
        );
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        array.write(
            &tail,
            GcIndex::Constant(0),
            GcOperand::i32_local(count),
            s,
            f,
        );

        queued.set_constant(0, f);
        from.load(f);
        self.prior_length.load(f);
        f.instruction(&Instruction::I32LtU);
        builder.open_frame(ControlFrameKind::If, f);
        self.prior_length.load(f);
        from.load(f);
        f.instruction(&Instruction::I32Sub);
        queued.store(f);
        from.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        source.store(f);
        destination.set_constant(1, f);
        array.copy_range_from(&tail, destination, &self.prior, source, queued, s, f);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        count.load(f);
        queued.load(f);
        f.instruction(&Instruction::I32Sub);
        count.store(f);
        count.load(f);
        builder.open_frame(ControlFrameKind::If, f);
        queued.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        destination.store(f);
        self.total.load(f);
        count.load(f);
        f.instruction(&Instruction::I32Sub);
        self.current_offset.load(f);
        f.instruction(&Instruction::I32Add);
        source.store(f);
        array.copy_range_from(&tail, destination, &self.current.bytes, source, count, s, f);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        tail.clear(f);
        for local in [destination, source, queued, count] {
            s.release_i32_local(local, f);
        }
    }
}

impl FunctionBuilder<'_> {
    fn emit_text_decoder_brand(
        &mut self,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<TextDecoderObject>, EmitError> {
        let s = self.runtime_schema();
        let this = s.reserve_value_local(f);
        self.compile_this_to_locals(&this, f)?;
        this.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<TextDecoderObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.emit_text_codec_error_if(
            RuntimeErrorMessage::TEXT_DECODER_INVALID_RECEIVER,
            output,
            exit,
            f,
        )?;
        let object = s
            .reserve_gc_local(f)
            .initialize(this.cast_reference::<TextDecoderObject>(s, f), f);
        this.clear(f);
        Ok(object)
    }

    // WHATWG Encoding labels trim ASCII whitespace and fold ASCII capitals.
    // This backend currently provides the UTF-8 decoder and rejects all others.
    fn emit_text_decoder_validate_label(
        &mut self,
        label: &ValueLocals,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        label.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_string_payload(label, pending, f)?;
        self.emit_uint8_codec_abrupt_exit(pending, output, exit, f);
        let input = self.emit_uint8_codec_string(pending.value(), output, exit, f)?;
        let start = s.reserve_i64_local(f);
        let end = s.reserve_i64_local(f);
        let at = s.reserve_i64_local(f);
        let unit = s.reserve_i32_local(f);
        let equal = s.reserve_i32_local(f);
        let valid = s.reserve_i32_local(f);
        f.instruction(&Instruction::I64Const(0));
        start.store(f);
        input.length.load(f);
        end.store(f);
        for leading in [true, false] {
            let done = self.open_frame(ControlFrameKind::Block, f);
            let again = self.open_frame(ControlFrameKind::Loop, f);
            start.load(f);
            end.load(f);
            f.instruction(&Instruction::I64GeU);
            self.emit_branch_if_to_target(done, f);
            if leading {
                start.load(f);
            } else {
                end.load(f);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64Sub);
            }
            at.store(f);
            self.emit_uint8_codec_string_unit(&input, at, unit, f);
            unit.load(f);
            f.instruction(&Instruction::I32Const(0x20));
            f.instruction(&Instruction::I32Eq);
            for whitespace in [9, 10, 12, 13] {
                unit.load(f);
                f.instruction(&Instruction::I32Const(whitespace));
                f.instruction(&Instruction::I32Eq);
                f.instruction(&Instruction::I32Or);
            }
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(done, f);
            if leading {
                self.emit_increment_local(start, 1, f);
            } else {
                self.emit_increment_local(end, -1, f);
            }
            self.emit_branch_to_target(again, f);
            self.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::I32Const(0));
        valid.store(f);
        for spelling in [
            "utf-8",
            "utf8",
            "unicode-1-1-utf-8",
            "unicode11utf8",
            "unicode20utf8",
            "x-unicode20utf8",
        ] {
            end.load(f);
            start.load(f);
            f.instruction(&Instruction::I64Sub);
            f.instruction(&Instruction::I64Const(spelling.len() as i64));
            f.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I32Const(1));
            equal.store(f);
            for (index, byte) in spelling.bytes().enumerate() {
                start.load(f);
                f.instruction(&Instruction::I64Const(index as i64));
                f.instruction(&Instruction::I64Add);
                at.store(f);
                self.emit_uint8_codec_string_unit(&input, at, unit, f);
                // All accepted label letters are alphabetic ASCII. Only fold
                // them, so punctuation never gains accidental aliases.
                if byte.is_ascii_alphabetic() {
                    unit.load(f);
                    f.instruction(&Instruction::I32Const(0x20));
                    f.instruction(&Instruction::I32Or);
                    unit.store(f);
                }
                equal.load(f);
                unit.load(f);
                f.instruction(&Instruction::I32Const(i32::from(byte)));
                f.instruction(&Instruction::I32Eq);
                f.instruction(&Instruction::I32And);
                equal.store(f);
            }
            valid.load(f);
            equal.load(f);
            f.instruction(&Instruction::I32Or);
            valid.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_range_error(
            RuntimeErrorMessage::TEXT_DECODER_INVALID_LABEL,
            output,
            f,
        )?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        for local in [valid, equal, unit] {
            s.release_i32_local(local, f);
        }
        for local in [at, end, start] {
            s.release_i64_local(local, f);
        }
        input.clear(s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_text_codec_validate_options(
        &mut self,
        options: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_is_heap_object_like_tag_i32(options.tag(), f);
        self.compile_nullish_tagged_i32(options.tag(), f)?;
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::I32Eqz);
        self.emit_text_codec_error_if(
            RuntimeErrorMessage::TEXT_CODEC_OPTIONS_INVALID,
            output,
            exit,
            f,
        )
    }

    pub(crate) fn emit_text_decoder_constructor(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let target = s.reserve_value_local(f);
        let options = s.reserve_value_local(f);
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        let fatal = s.reserve_i32_local(f);
        let ignore_bom = s.reserve_i32_local(f);
        output.initialize(f);
        self.compile_new_target_to_locals(&target, f)?;
        let exit = self.open_frame(ControlFrameKind::Block, f);
        target.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.emit_text_codec_error_if(
            RuntimeErrorMessage::TEXT_CODEC_REQUIRES_NEW,
            &output,
            exit,
            f,
        )?;
        self.emit_builtin_arg_to_value(0, &argument, f);
        argument.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        self.emit_uint8_codec_abrupt_exit(&pending, &output, exit, f);
        argument.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_builtin_arg_to_value(1, &options, f);
        self.emit_text_codec_validate_options(&options, &output, exit, f)?;
        for (name, local) in [("fatal", fatal), ("ignoreBOM", ignore_bom)] {
            self.emit_text_codec_option(&options, name, &pending, f)?;
            self.emit_uint8_codec_abrupt_exit(&pending, &output, exit, f);
            self.emit_to_boolean_payload_from_tagged_locals(pending.value(), f)?;
            f.instruction(&Instruction::I32WrapI64);
            local.store(f);
        }
        self.emit_text_decoder_validate_label(&argument, &pending, &output, exit, f)?;
        self.emit_get_prototype_from_constructor(
            &target,
            OrdinaryDefaultPrototype::TextDecoder,
            &output,
            f,
        )?;
        self.emit_uint8_codec_abrupt_exit(&output, &output, exit, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(output.value()), f)?,
            f,
        );
        let capacity = s.reserve_i32_local(f);
        capacity.set_constant(CARRY_BYTES + 1, f);
        let bytes = s.reserve_gc_local(f).initialize(
            s.array_type::<ByteArray>()
                .filled(GcOperand::i32(0), capacity, f),
            f,
        );
        let decoder = s.reserve_gc_local(f).initialize(
            s.struct_type::<TextDecoderObject>().construct(
                (
                    GcOperand::reference(&header, s),
                    GcOperand::boolean_local(fatal),
                    GcOperand::boolean_local(ignore_bom),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::reference(&bytes, s),
                ),
                f,
            ),
            f,
        );
        output.value().set_reference(&decoder, s, f);
        output.set_kind(CompletionKind::Normal, f);
        decoder.clear(f);
        bytes.clear(f);
        header.clear(f);
        s.release_i32_local(capacity, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        for local in [ignore_bom, fatal] {
            s.release_i32_local(local, f);
        }
        output.clear(f);
        pending.clear(f);
        argument.clear(f);
        options.clear(f);
        target.clear(f);
        Ok(())
    }

    pub(crate) fn emit_text_decoder_encoding_getter(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_text_decoder_getter(0, f)
    }
    pub(crate) fn emit_text_decoder_fatal_getter(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_text_decoder_getter(1, f)
    }
    pub(crate) fn emit_text_decoder_ignore_bom_getter(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_text_decoder_getter(2, f)
    }
    fn emit_text_decoder_getter(&mut self, kind: u8, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let decoder = self.emit_text_decoder_brand(&output, exit, f)?;
        if kind == 0 {
            let text = s
                .reserve_gc_local(f)
                .initialize(self.emit_interned_string_reference("utf-8", f)?, f);
            output.value().set_reference(&text, s, f);
            text.clear(f);
        } else {
            let flag = s.reserve_i32_local(f);
            if kind == 1 {
                s.field(TextDecoderObjectSchema::FATAL)
                    .read(&decoder, s, f)
                    .store(flag, f);
            } else {
                s.field(TextDecoderObjectSchema::IGNORE_BOM)
                    .read(&decoder, s, f)
                    .store(flag, f);
            }
            output.value().set_boolean(flag, f);
            s.release_i32_local(flag, f);
        }
        output.set_kind(CompletionKind::Normal, f);
        decoder.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        Ok(())
    }

    pub(crate) fn emit_text_decoder_decode(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        let argument = s.reserve_value_local(f);
        let options = s.reserve_value_local(f);
        let stream = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let decoder = self.emit_text_decoder_brand(&output, exit, f)?;
        self.emit_builtin_arg_to_value(0, &argument, f);
        self.emit_text_decoder_input_brand(&argument, &output, exit, f)?;
        self.emit_builtin_arg_to_value(1, &options, f);
        self.emit_text_codec_validate_options(&options, &output, exit, f)?;
        self.emit_text_codec_option(&options, "stream", &pending, f)?;
        self.emit_uint8_codec_abrupt_exit(&pending, &output, exit, f);
        self.emit_to_boolean_payload_from_tagged_locals(pending.value(), f)?;
        f.instruction(&Instruction::I32WrapI64);
        stream.store(f);
        let input = self.emit_text_decoder_input(&argument, &output, exit, f)?;
        self.emit_text_decoder_utf8(&decoder, &input, stream, &output, exit, f)?;
        input.clear(s, f);
        decoder.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        s.release_i32_local(stream, f);
        options.clear(f);
        argument.clear(f);
        pending.clear(f);
        output.clear(f);
        Ok(())
    }

    fn emit_text_decoder_utf8(
        &mut self,
        decoder: &GcLocal<TextDecoderObject>,
        input: &TextDecoderInput,
        stream: I32Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let base = self.runtime_helper_base()?;
        let old_stream = s.reserve_i32_local(f);
        let fatal = s.reserve_i32_local(f);
        let ignore_bom = s.reserve_i32_local(f);
        let initial_bom = s.reserve_i32_local(f);
        let bom_seen = s.reserve_i32_local(f);
        let prior_capacity = s.reserve_i32_local(f);
        let prior_length = s.reserve_i32_local(f);
        let current_offset = s.reserve_i32_local(f);
        let total = s.reserve_i32_local(f);
        let index = s.reserve_i32_local(f);
        let retain_from = s.reserve_i32_local(f);
        let rendering = s.reserve_i32_local(f);
        let failed = s.reserve_i32_local(f);
        let scalar = s.reserve_i32_local(f);
        let byte = s.reserve_i32_local(f);
        let start = s.reserve_i32_local(f);
        let need = s.reserve_i32_local(f);
        let seen = s.reserve_i32_local(f);
        let lower = s.reserve_i32_local(f);
        let upper = s.reserve_i32_local(f);
        let invalid = s.reserve_i32_local(f);
        let written = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        s.field(TextDecoderObjectSchema::STREAMING)
            .read(decoder, s, f)
            .store(old_stream, f);
        s.field(TextDecoderObjectSchema::FATAL)
            .read(decoder, s, f)
            .store(fatal, f);
        s.field(TextDecoderObjectSchema::IGNORE_BOM)
            .read(decoder, s, f)
            .store(ignore_bom, f);
        s.field(TextDecoderObjectSchema::BOM_SEEN)
            .read(decoder, s, f)
            .store(initial_bom, f);
        let prior = s.reserve_gc_local(f).initialize(
            s.field(TextDecoderObjectSchema::PENDING)
                .read(decoder, s, f)
                .reference(),
            f,
        );
        s.array_type::<ByteArray>().length(&prior, s, f);
        prior_capacity.store(f);
        old_stream.load(f);
        self.open_frame(ControlFrameKind::If, f);
        prior_capacity.load(f);
        f.instruction(&Instruction::I32Const(CARRY_BYTES + 1));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        s.array_type::<ByteArray>()
            .read(&prior, GcIndex::Constant(0), s, f)
            .store(prior_length, f);
        f.instruction(&Instruction::Else);
        prior_capacity.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        prior_length.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        prior_length.set_constant(0, f);
        initial_bom.set_constant(0, f);
        s.field(TextDecoderObjectSchema::BOM_SEEN)
            .write(decoder, GcOperand::boolean(false), s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.field(TextDecoderObjectSchema::STREAMING).write(
            decoder,
            GcOperand::boolean_local(stream),
            s,
            f,
        );

        // The decoder cursor spans the retained queue and the new input.
        // Both are stable across the sizing and rendering passes.
        prior_length.load(f);
        input.length.load(f);
        f.instruction(&Instruction::I32Add);
        total.store(f);
        total.load(f);
        f.instruction(&Instruction::I32Const(i32::MAX));
        f.instruction(&Instruction::I32GtU);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        input.start.load(f);
        prior_length.load(f);
        f.instruction(&Instruction::I32Sub);
        current_offset.store(f);
        let bytes = TextDecoderBytes {
            current: input,
            prior,
            prior_capacity,
            prior_length,
            current_offset,
            total,
        };

        let slot = s.reserve_gc_local(f);
        slot.hold_null_until_initialized(s, f);
        written.set_constant(0, f);
        rendering.set_constant(0, f);
        failed.set_constant(0, f);
        let completed = self.open_frame(ControlFrameKind::Block, f);
        let pass = self.open_frame(ControlFrameKind::Loop, f);
        rendering.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let construction = StringConstruction::allocate(s, slot, written, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        initial_bom.load(f);
        bom_seen.store(f);
        total.load(f);
        retain_from.store(f);
        index.set_constant(0, f);
        written.set_constant(0, f);

        let decoded = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        total.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(decoded, f);
        // Once the BOM decision is made, count or widen a whole ASCII run
        // from the current segment. The unusual retained fatal queue can
        // use the scalar path.
        bom_seen.load(f);
        index.load(f);
        prior_length.load(f);
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let position = s.reserve_i32_local(f);
        let end = s.reserve_i32_local(f);
        index.load(f);
        current_offset.load(f);
        f.instruction(&Instruction::I32Add);
        position.store(f);
        total.load(f);
        current_offset.load(f);
        f.instruction(&Instruction::I32Add);
        end.store(f);
        rendering.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let current = s
            .reserve_gc_local(f)
            .initialize(input.bytes.load(s, f).require_non_null(f), f);
        construction.widen_ascii_from(written, &current, position, end, base, s, f);
        current.clear(f);
        f.instruction(&Instruction::Else);
        let run_start = s.reserve_i32_local(f);
        position.load(f);
        run_start.store(f);
        self.emit_text_decoder_count_ascii(input, position, end, f);
        written.load(f);
        position.load(f);
        run_start.load(f);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Add);
        written.store(f);
        s.release_i32_local(run_start, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        position.load(f);
        current_offset.load(f);
        f.instruction(&Instruction::I32Sub);
        index.store(f);
        s.release_i32_local(end, f);
        s.release_i32_local(position, f);
        index.load(f);
        total.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(decoded, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        index.load(f);
        start.store(f);
        bytes.read(index, byte, self, f);
        increment_decoder_index(index, f);
        byte.load(f);
        scalar.store(f);
        f.instruction(&Instruction::I32Const(1));
        need.store(f);
        f.instruction(&Instruction::I32Const(0));
        invalid.store(f);
        f.instruction(&Instruction::I32Const(0x80));
        lower.store(f);
        f.instruction(&Instruction::I32Const(0xbf));
        upper.store(f);
        byte.load(f);
        f.instruction(&Instruction::I32Const(0x80));
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(1));
        invalid.store(f);
        for (min, max, width, mask) in [
            (0xc2, 0xdf, 2, 0x1f),
            (0xe0, 0xef, 3, 0xf),
            (0xf0, 0xf4, 4, 7),
        ] {
            byte.load(f);
            f.instruction(&Instruction::I32Const(min));
            f.instruction(&Instruction::I32GeU);
            byte.load(f);
            f.instruction(&Instruction::I32Const(max));
            f.instruction(&Instruction::I32LeU);
            f.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I32Const(0));
            invalid.store(f);
            f.instruction(&Instruction::I32Const(width));
            need.store(f);
            byte.load(f);
            f.instruction(&Instruction::I32Const(mask));
            f.instruction(&Instruction::I32And);
            scalar.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        for (lead, bound, local) in [
            (0xe0, 0xa0, lower),
            (0xed, 0x9f, upper),
            (0xf0, 0x90, lower),
            (0xf4, 0x8f, upper),
        ] {
            byte.load(f);
            f.instruction(&Instruction::I32Const(lead));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I32Const(bound));
            local.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(1));
        seen.store(f);
        let sequence_done = self.open_frame(ControlFrameKind::Block, f);
        let continuation = self.open_frame(ControlFrameKind::Loop, f);
        invalid.load(f);
        self.emit_branch_if_to_target(sequence_done, f);
        seen.load(f);
        need.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(sequence_done, f);
        index.load(f);
        total.load(f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        stream.load(f);
        self.open_frame(ControlFrameKind::If, f);
        // The unfinished sequence is retained after the rendering pass.
        start.load(f);
        retain_from.store(f);
        self.emit_branch_to_target(decoded, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(1));
        invalid.store(f);
        self.emit_branch_to_target(sequence_done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        bytes.read(index, byte, self, f);
        byte.load(f);
        lower.load(f);
        f.instruction(&Instruction::I32LtU);
        byte.load(f);
        upper.load(f);
        f.instruction(&Instruction::I32GtU);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(1));
        invalid.store(f);
        self.emit_branch_to_target(sequence_done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        scalar.load(f);
        f.instruction(&Instruction::I32Const(6));
        f.instruction(&Instruction::I32Shl);
        byte.load(f);
        f.instruction(&Instruction::I32Const(0x3f));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Or);
        scalar.store(f);
        f.instruction(&Instruction::I32Const(0x80));
        lower.store(f);
        f.instruction(&Instruction::I32Const(0xbf));
        upper.store(f);
        for local in [index, seen] {
            local.load(f);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Add);
            local.store(f);
        }
        self.emit_branch_to_target(continuation, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        invalid.load(f);
        self.open_frame(ControlFrameKind::If, f);
        fatal.load(f);
        self.open_frame(ControlFrameKind::If, f);
        // The error leaves the BOM flag at its last serialized value.
        // Keep any unread bytes before reporting a fatal streaming error.
        stream.load(f);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        retain_from.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        failed.set_constant(1, f);
        self.emit_branch_to_target(completed, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(0xfffd));
        scalar.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let emitted = self.open_frame(ControlFrameKind::Block, f);
        bom_seen.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(1));
        bom_seen.store(f);
        ignore_bom.load(f);
        f.instruction(&Instruction::I32Eqz);
        scalar.load(f);
        f.instruction(&Instruction::I32Const(0xfeff));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        self.emit_branch_if_to_target(emitted, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        scalar.load(f);
        f.instruction(&Instruction::I32Const(0xffff));
        f.instruction(&Instruction::I32GtU);
        self.open_frame(ControlFrameKind::If, f);
        rendering.load(f);
        self.open_frame(ControlFrameKind::If, f);
        scalar.load(f);
        f.instruction(&Instruction::I32Const(0x10000));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Const(0xd800));
        f.instruction(&Instruction::I32Add);
        unit.store(f);
        construction.write(written, unit, s, f);
        scalar.load(f);
        f.instruction(&Instruction::I32Const(0x3ff));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Const(0xdc00));
        f.instruction(&Instruction::I32Add);
        scalar.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        increment_decoder_index(written, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        rendering.load(f);
        self.open_frame(ControlFrameKind::If, f);
        construction.write(written, scalar, s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        increment_decoder_index(written, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        rendering.load(f);
        self.emit_branch_if_to_target(completed, f);
        rendering.set_constant(1, f);
        self.emit_branch_to_target(pass, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        bytes.retain_tail(decoder, retain_from, self, f);
        failed.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::TEXT_DECODER_INVALID_DATA,
            output,
            f,
        )?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.field(TextDecoderObjectSchema::BOM_SEEN).write(
            decoder,
            GcOperand::boolean_local(bom_seen),
            s,
            f,
        );
        let text = s
            .reserve_gc_local(f)
            .initialize(construction.publish(s, f), f);
        output.value().set_reference(&text, s, f);
        output.set_kind(CompletionKind::Normal, f);
        text.clear(f);
        bytes.prior.clear(f);
        for local in [
            unit,
            written,
            invalid,
            upper,
            lower,
            seen,
            need,
            start,
            byte,
            scalar,
            failed,
            rendering,
            retain_from,
            index,
            total,
            current_offset,
            prior_length,
            prior_capacity,
            bom_seen,
            initial_bom,
            ignore_bom,
            fatal,
            old_stream,
        ] {
            s.release_i32_local(local, f);
        }
        Ok(())
    }

    fn emit_text_decoder_count_ascii(
        &self,
        input: &TextDecoderInput,
        position: I32Local,
        end: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        f.instruction(&Instruction::Block(BlockType::Empty));
        f.instruction(&Instruction::Loop(BlockType::Empty));
        s.array_type::<ByteArray>()
            .read(&input.bytes, position, s, f);
        f.instruction(&Instruction::I32Const(0x80));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::BrIf(1));
        increment_decoder_index(position, f);
        position.load(f);
        end.load(f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::BrIf(0));
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
    }
}

fn increment_decoder_index(local: I32Local, f: &mut Function) {
    local.load(f);
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    local.store(f);
}
