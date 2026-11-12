use super::*;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::module::TypedArrayElementKind;

impl FunctionBuilder<'_> {
    fn emit_text_encoder_brand(
        &mut self,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let this = s.reserve_value_local(f);
        self.compile_this_to_locals(&this, f)?;
        this.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<TextEncoderObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.emit_text_codec_error_if(
            RuntimeErrorMessage::TEXT_ENCODER_INVALID_RECEIVER,
            output,
            exit,
            f,
        )?;
        this.clear(f);
        Ok(())
    }

    pub(crate) fn emit_text_encoder_constructor(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let target = s.reserve_value_local(f);
        let output = s.reserve_completion(f);
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
        self.emit_get_prototype_from_constructor(
            &target,
            OrdinaryDefaultPrototype::TextEncoder,
            &output,
            f,
        )?;
        self.emit_uint8_codec_abrupt_exit(&output, &output, exit, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(output.value()), f)?,
            f,
        );
        let encoder = s.reserve_gc_local(f).initialize(
            s.struct_type::<TextEncoderObject>()
                .construct((GcOperand::reference(&header, s),), f),
            f,
        );
        output.value().set_reference(&encoder, s, f);
        output.set_kind(CompletionKind::Normal, f);
        encoder.clear(f);
        header.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        target.clear(f);
        Ok(())
    }

    pub(crate) fn emit_text_encoder_encoding_getter(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_text_encoder_brand(&output, exit, f)?;
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("utf-8", f)?, f);
        output.value().set_reference(&text, s, f);
        output.set_kind(CompletionKind::Normal, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        Ok(())
    }

    /// encode(input): the UTF-8 length is counted first, so the result's
    /// ArrayBuffer backing is allocated exactly once and written directly.
    /// The Uint8Array and its ArrayBuffer take the current function Realm's
    /// %Uint8Array.prototype% and %ArrayBuffer.prototype%, exactly what
    /// constructing that Realm's %Uint8Array% with the length would select:
    /// both constructors' "prototype" properties are non-writable and
    /// non-configurable, and the fresh view is unobservable until returned.
    pub(crate) fn emit_text_encoder_encode(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_text_encoder_brand(&output, exit, f)?;
        self.emit_builtin_arg_to_value(0, &argument, f);
        // The optional input has an empty-string default.
        argument.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let empty = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("", f)?, f);
        argument.set_reference(&empty, s, f);
        empty.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        self.emit_uint8_codec_abrupt_exit(&pending, &output, exit, f);
        let input = self.emit_uint8_codec_string(pending.value(), &output, exit, f)?;
        let count = s.reserve_i32_local(f);
        let read = s.reserve_i32_local(f);
        let written = s.reserve_i32_local(f);
        let unit = s.reserve_i32_local(f);
        let next_unit = s.reserve_i32_local(f);
        let byte = s.reserve_i32_local(f);
        let byte_length = s.reserve_i64_local(f);
        input.length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        count.store(f);
        // Pass 1: the exact UTF-8 length. Lone surrogates become U+FFFD,
        // which, like every other BMP scalar from U+0800, takes three bytes.
        f.instruction(&Instruction::I64Const(0));
        byte_length.store(f);
        f.instruction(&Instruction::I32Const(0));
        read.store(f);
        let counted = self.open_frame(ControlFrameKind::Block, f);
        let count_next = self.open_frame(ControlFrameKind::Loop, f);
        read.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(counted, f);
        s.array_type::<CodeUnitArray>()
            .read(&input.units, read, s, f)
            .store(unit, f);
        encoder_increment(read, f);
        byte_length.load(f);
        f.instruction(&Instruction::I64Const(1));
        unit.load(f);
        f.instruction(&Instruction::I32Const(0x80));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Add);
        unit.load(f);
        f.instruction(&Instruction::I32Const(0x800));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Add);
        byte_length.store(f);
        // A high surrogate followed by a low surrogate is one four-byte
        // scalar: one more byte than the three already counted for it.
        unit.load(f);
        f.instruction(&Instruction::I32Const(0xfc00));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Const(0xd800));
        f.instruction(&Instruction::I32Eq);
        read.load(f);
        count.load(f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        s.array_type::<CodeUnitArray>()
            .read(&input.units, read, s, f)
            .store(next_unit, f);
        next_unit.load(f);
        f.instruction(&Instruction::I32Const(0xfc00));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Const(0xdc00));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        encoder_increment(read, f);
        byte_length.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        byte_length.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(count_next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let buffer_prototype = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
            &buffer_prototype,
            f,
        );
        let fixed = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        fixed.store(f);
        let buffer = self.emit_binary_allocate_array_buffer(
            &buffer_prototype,
            byte_length,
            byte_length,
            fixed,
            false,
            &output,
            exit,
            f,
        )?;
        let bytes = s.reserve_gc_local(f).initialize(
            s.field(ArrayBufferSchema::BYTES)
                .read(&buffer, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        // Pass 2: write the bytes into the fresh private backing.
        f.instruction(&Instruction::I32Const(0));
        read.store(f);
        f.instruction(&Instruction::I32Const(0));
        written.store(f);
        let encoded = self.open_frame(ControlFrameKind::Block, f);
        let encode_next = self.open_frame(ControlFrameKind::Loop, f);
        read.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(encoded, f);
        s.array_type::<CodeUnitArray>()
            .read(&input.units, read, s, f)
            .store(unit, f);
        encoder_increment(read, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(0x80));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_text_encoder_put(&bytes, written, unit, f);
        self.emit_branch_to_target(encode_next, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        unit.load(f);
        f.instruction(&Instruction::I32Const(0x800));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        for (shift, prefix) in [(6, 0xc0), (0, 0x80)] {
            unit.load(f);
            if shift != 0 {
                f.instruction(&Instruction::I32Const(shift));
                f.instruction(&Instruction::I32ShrU);
            } else {
                f.instruction(&Instruction::I32Const(0x3f));
                f.instruction(&Instruction::I32And);
            }
            f.instruction(&Instruction::I32Const(prefix));
            f.instruction(&Instruction::I32Or);
            byte.store(f);
            self.emit_text_encoder_put(&bytes, written, byte, f);
        }
        self.emit_branch_to_target(encode_next, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        unit.load(f);
        f.instruction(&Instruction::I32Const(0xf800));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Const(0xd800));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        // A surrogate: a valid pair is one supplementary scalar, and any
        // other surrogate is the replacement character.
        unit.load(f);
        f.instruction(&Instruction::I32Const(0xdc00));
        f.instruction(&Instruction::I32LtU);
        read.load(f);
        count.load(f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        s.array_type::<CodeUnitArray>()
            .read(&input.units, read, s, f)
            .store(next_unit, f);
        next_unit.load(f);
        f.instruction(&Instruction::I32Const(0xfc00));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Const(0xdc00));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        encoder_increment(read, f);
        unit.load(f);
        f.instruction(&Instruction::I32Const(0xd800));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32Shl);
        next_unit.load(f);
        f.instruction(&Instruction::I32Const(0xdc00));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Const(0x10000));
        f.instruction(&Instruction::I32Add);
        unit.store(f);
        for (shift, prefix) in [(18, 0xf0), (12, 0x80), (6, 0x80), (0, 0x80)] {
            unit.load(f);
            if shift != 0 {
                f.instruction(&Instruction::I32Const(shift));
                f.instruction(&Instruction::I32ShrU);
            }
            if prefix == 0x80 {
                f.instruction(&Instruction::I32Const(0x3f));
                f.instruction(&Instruction::I32And);
            }
            f.instruction(&Instruction::I32Const(prefix));
            f.instruction(&Instruction::I32Or);
            byte.store(f);
            self.emit_text_encoder_put(&bytes, written, byte, f);
        }
        self.emit_branch_to_target(encode_next, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(0xfffd));
        unit.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        for (shift, prefix) in [(12, 0xe0), (6, 0x80), (0, 0x80)] {
            unit.load(f);
            if shift != 0 {
                f.instruction(&Instruction::I32Const(shift));
                f.instruction(&Instruction::I32ShrU);
            }
            if prefix == 0x80 {
                f.instruction(&Instruction::I32Const(0x3f));
                f.instruction(&Instruction::I32And);
            }
            f.instruction(&Instruction::I32Const(prefix));
            f.instruction(&Instruction::I32Or);
            byte.store(f);
            self.emit_text_encoder_put(&bytes, written, byte, f);
        }
        self.emit_branch_to_target(encode_next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        bytes.clear(f);
        let owner = s.reserve_gc_local(f).initialize(
            s.struct_type::<BufferOwner>().construct(
                (
                    GcOperand::constant(BufferOwnerKind::ArrayBuffer),
                    GcOperand::nullable_reference(&buffer, s),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        let prototype = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::Uint8ArrayPrototype,
            &prototype,
            f,
        );
        let offset = s.reserve_i64_local(f);
        f.instruction(&Instruction::I64Const(0));
        offset.store(f);
        let array = self.emit_binary_allocate_typed_array(
            TypedArrayElementKind::Uint8,
            &prototype,
            &owner,
            offset,
            byte_length,
            fixed,
            f,
        )?;
        let result = s.reserve_value_local(f);
        result.set_reference(&array, s, f);
        output.set_normal(&result, f);
        result.clear(f);
        array.clear(f);
        s.release_i64_local(offset, f);
        prototype.clear(f);
        owner.clear(f);
        buffer.clear(f);
        s.release_i32_local(fixed, f);
        buffer_prototype.clear(f);
        realm.clear(f);
        s.release_i64_local(byte_length, f);
        for local in [byte, next_unit, unit, written, read, count] {
            s.release_i32_local(local, f);
        }
        input.clear(s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        argument.clear(f);
        Ok(())
    }

    fn emit_text_encoder_put(
        &self,
        bytes: &GcLocal<ByteArray>,
        written: I32Local,
        byte: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.array_type::<ByteArray>()
            .write(bytes, written, GcOperand::i32_local(byte), s, f);
        written.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        written.store(f);
    }
    pub(crate) fn emit_text_encoder_encode_into(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_text_encoder_brand(&output, exit, f)?;
        self.emit_builtin_arg_to_value(0, &argument, f);
        // encodeInto's required DOMString converts even an explicit undefined.
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        self.emit_uint8_codec_abrupt_exit(&pending, &output, exit, f);
        let input = self.emit_uint8_codec_string(pending.value(), &output, exit, f)?;
        let capacity = s.reserve_i64_local(f);
        self.emit_builtin_arg_to_value(1, &argument, f);
        let destination = self.emit_uint8_codec_receiver(
            Uint8ArrayCodecAccess::Write,
            &argument,
            &pending,
            &output,
            exit,
            f,
        )?;
        self.emit_validate_typed_array_view(&destination.object, capacity, &pending, f)?;
        self.emit_uint8_codec_abrupt_exit(&pending, &output, exit, f);
        let result = self.emit_uint8_codec_decoded_list(capacity, capacity, f);
        let scalar = s.reserve_i32_local(f);
        let trailing = s.reserve_i32_local(f);
        let consumed = s.reserve_i64_local(f);
        let octets = s.reserve_i64_local(f);
        let byte = s.reserve_i32_local(f);
        let end = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        result.read.load(f);
        input.length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(end, f);
        self.emit_uint8_codec_string_unit(&input, result.read, scalar, f);
        f.instruction(&Instruction::I64Const(1));
        consumed.store(f);
        scalar.load(f);
        f.instruction(&Instruction::I32Const(0xd800));
        f.instruction(&Instruction::I32GeU);
        scalar.load(f);
        f.instruction(&Instruction::I32Const(0xdfff));
        f.instruction(&Instruction::I32LeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(0xfffd));
        trailing.store(f);
        scalar.load(f);
        f.instruction(&Instruction::I32Const(0xdbff));
        f.instruction(&Instruction::I32LeU);
        result.read.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        input.length.load(f);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_increment_local(result.read, 1, f);
        self.emit_uint8_codec_string_unit(&input, result.read, byte, f);
        self.emit_increment_local(result.read, -1, f);
        byte.load(f);
        f.instruction(&Instruction::I32Const(0xdc00));
        f.instruction(&Instruction::I32GeU);
        byte.load(f);
        f.instruction(&Instruction::I32Const(0xdfff));
        f.instruction(&Instruction::I32LeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        scalar.load(f);
        f.instruction(&Instruction::I32Const(0xd800));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32Shl);
        byte.load(f);
        f.instruction(&Instruction::I32Const(0xdc00));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Const(0x10000));
        f.instruction(&Instruction::I32Add);
        trailing.store(f);
        f.instruction(&Instruction::I64Const(2));
        consumed.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        trailing.load(f);
        scalar.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I64Const(1));
        octets.store(f);
        for (threshold, width) in [(0x7f, 2), (0x7ff, 3), (0xffff, 4)] {
            scalar.load(f);
            f.instruction(&Instruction::I32Const(threshold));
            f.instruction(&Instruction::I32GtU);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I64Const(width));
            octets.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        result.written.load(f);
        octets.load(f);
        f.instruction(&Instruction::I64Add);
        capacity.load(f);
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(end, f);
        for width in 1..=4 {
            octets.load(f);
            f.instruction(&Instruction::I64Const(width));
            f.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, f);
            let prefix = [0, 0, 0xc0, 0xe0, 0xf0][width as usize];
            for index in 0..width {
                scalar.load(f);
                f.instruction(&Instruction::I32Const(((width - index - 1) * 6) as i32));
                f.instruction(&Instruction::I32ShrU);
                if index == 0 {
                    f.instruction(&Instruction::I32Const(prefix));
                } else {
                    f.instruction(&Instruction::I32Const(0x3f));
                    f.instruction(&Instruction::I32And);
                    f.instruction(&Instruction::I32Const(0x80));
                }
                f.instruction(&Instruction::I32Or);
                byte.store(f);
                self.emit_uint8_codec_push_byte(&result, byte, f);
            }
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        result.read.load(f);
        consumed.load(f);
        f.instruction(&Instruction::I64Add);
        result.read.store(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_uint8_codec_copy_bytes(
            &destination.object,
            &result.bytes,
            result.written,
            &pending,
            &output,
            exit,
            f,
        )?;
        self.emit_uint8_codec_count_result(&result, &pending, &output, exit, f)?;
        destination.clear(f);
        result.clear(s, f);
        for local in [byte, trailing, scalar] {
            s.release_i32_local(local, f);
        }
        for local in [octets, consumed, capacity] {
            s.release_i64_local(local, f);
        }
        input.clear(s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        argument.clear(f);
        Ok(())
    }
}

fn encoder_increment(local: I32Local, f: &mut Function) {
    local.load(f);
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    local.store(f);
}
