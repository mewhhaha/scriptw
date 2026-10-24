//! Foreign transfers own snapshots; host roots never alias published JS data.
use super::*;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_foreign_value_error(
        &mut self,
        kind: scriptw_ir::NativeErrorKind,
        message: RuntimeErrorMessage,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let pending = self.runtime_schema().reserve_completion(f);
        pending.initialize(f);
        self.emit_throw_runtime_error_to_active_handler(kind, message, &pending, f)?;
        pending.clear(f);
        Ok(())
    }

    pub(crate) fn emit_foreign_string_argument(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcLocal<CodeUnitArray>, EmitError> {
        let s = self.runtime_schema();
        input.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_foreign_value_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::FOREIGN_STRING_ARGUMENT_REQUIRED,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let text = s
            .reserve_gc_local(f)
            .initialize(input.cast_reference::<StringValue>(s, f), f);
        let units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(&text, s, f)
                .reference(),
            f,
        );
        let copy = self.emit_foreign_copy_string_units(&units, f)?;
        units.clear(f);
        text.clear(f);
        Ok(s.reserve_gc_local(f)
            .initialize(copy.publish_units(s, f), f))
    }

    pub(crate) fn emit_foreign_string_result(
        &mut self,
        units: &GcLocal<CodeUnitArray>,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let copy = self.emit_foreign_copy_string_units(units, f)?;
        let text = s.reserve_gc_local(f).initialize(copy.publish(s, f), f);
        output.set_reference(&text, s, f);
        text.clear(f);
        Ok(())
    }

    fn emit_foreign_copy_string_units(
        &mut self,
        units: &GcLocal<CodeUnitArray>,
        f: &mut Function,
    ) -> Result<StringConstruction, EmitError> {
        let s = self.runtime_schema();
        let length = s.reserve_i32_local(f);
        s.array_type::<CodeUnitArray>().length(units, s, f);
        length.store(f);
        length.load(f);
        f.instruction(&Instruction::I32Const(i32::MAX));
        f.instruction(&Instruction::I32GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_foreign_value_error(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::FOREIGN_TRANSFER_TOO_LARGE,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let copy = StringConstruction::allocate(s, s.reserve_gc_local(f), length, f);
        let zero = s.reserve_i32_local(f);
        zero.set_constant(0, f);
        copy.copy_units_from(zero, units, length, s, f);
        s.release_i32_local(zero, f);
        s.release_i32_local(length, f);
        Ok(copy)
    }

    /// Host observation rejects invalid views without invoking JavaScript or
    /// changing its completion. A null result is a conversion failure.
    pub(crate) fn emit_foreign_bytes_read(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcLocal<ByteArray, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let output = s
            .reserve_gc_local::<ByteArray, Nullable>(f)
            .initialize_null(s, f);
        input.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let source = s
            .reserve_gc_local(f)
            .initialize(input.cast_reference::<TypedArrayObject>(s, f), f);
        let kind = s.reserve_i32_local(f);
        let valid = s.reserve_i32_local(f);
        let length = s.reserve_i64_local(f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&source, s, f)
            .store(kind, f);
        self.emit_typed_array_length_snapshot(&source, length, valid, f);
        kind.load(f);
        f.instruction(&Instruction::I32Const(
            TypedArrayElementKind::Uint8.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        valid.load(f);
        f.instruction(&Instruction::I32And);
        length.load(f);
        f.instruction(&Instruction::I64Const(i64::from(i32::MAX)));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        // No user code can detach/shrink between this witness and the copy.
        let bytes = self.emit_foreign_bytes_snapshot(&source, length, f)?;
        output.replace(bytes.load(s, f).nullable(), f);
        bytes.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(length, f);
        s.release_i32_local(valid, f);
        s.release_i32_local(kind, f);
        source.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(output)
    }

    pub(crate) fn emit_foreign_bytes_argument(
        &mut self,
        input: &ValueLocals,
        f: &mut Function,
    ) -> Result<GcLocal<ByteArray>, EmitError> {
        let s = self.runtime_schema();
        input.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_foreign_value_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::FOREIGN_BYTES_ARGUMENT_REQUIRED,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let source = s
            .reserve_gc_local(f)
            .initialize(input.cast_reference::<TypedArrayObject>(s, f), f);
        let kind = s.reserve_i32_local(f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&source, s, f)
            .store(kind, f);
        kind.load(f);
        f.instruction(&Instruction::I32Const(
            TypedArrayElementKind::Uint8.encode(),
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_foreign_value_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::FOREIGN_BYTES_ARGUMENT_REQUIRED,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(kind, f);
        let length = s.reserve_i64_local(f);
        let pending = s.reserve_completion(f);
        self.emit_validate_typed_array_view(&source, length, &pending, f)?;
        self.completion().copy_from(&pending, f);
        pending.clear(f);
        self.emit_propagate_current_throw_if_needed(f);
        length.load(f);
        f.instruction(&Instruction::I64Const(i64::from(i32::MAX)));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_foreign_value_error(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::FOREIGN_TRANSFER_TOO_LARGE,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let bytes = self.emit_foreign_bytes_snapshot(&source, length, f)?;
        s.release_i64_local(length, f);
        source.clear(f);
        Ok(bytes)
    }

    /// Snapshot a validated Uint8Array view without a JavaScript throw edge.
    /// Callers own the brand, bounds and representable-length witnesses.
    fn emit_foreign_bytes_snapshot(
        &mut self,
        source: &GcLocal<TypedArrayObject>,
        length: I64Local,
        f: &mut Function,
    ) -> Result<GcLocal<ByteArray>, EmitError> {
        let s = self.runtime_schema();
        let view = s.reserve_gc_local(f).initialize(
            s.field(TypedArrayObjectSchema::VIEW)
                .read(source, s, f)
                .reference(),
            f,
        );
        let owner = s.reserve_gc_local(f).initialize(
            s.field(BufferViewSchema::BUFFER)
                .read(&view, s, f)
                .reference(),
            f,
        );
        let view_offset = s.reserve_i64_local(f);
        s.field(BufferViewSchema::BYTE_OFFSET)
            .read(&view, s, f)
            .store_i64(view_offset, f);
        let access = self.emit_binary_buffer_access(&owner, f);
        let size = s.reserve_i32_local(f);
        length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        size.store(f);
        let bytes = s.reserve_gc_local(f).initialize(
            s.array_type::<ByteArray>()
                .filled(GcOperand::i32(0), size, f),
            f,
        );
        let index = s.reserve_i64_local(f);
        let offset = s.reserve_i32_local(f);
        let byte = s.reserve_i32_local(f);
        let position = s.reserve_i64_local(f);
        index.set_constant(0, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        // Internal view reads perform no user code. Shared buffers may grow;
        // this operation retains the entry length and snapshots each byte.
        view_offset.load(f);
        index.load(f);
        f.instruction(&Instruction::I64Add);
        position.store(f);
        access.read_byte(position, byte, self, f)?;
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        offset.store(f);
        s.array_type::<ByteArray>()
            .write(&bytes, offset, GcOperand::i32_local(byte), s, f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(position, f);
        s.release_i32_local(byte, f);
        s.release_i32_local(offset, f);
        s.release_i64_local(index, f);
        s.release_i32_local(size, f);
        access.clear(s, f);
        s.release_i64_local(view_offset, f);
        owner.clear(f);
        view.clear(f);
        Ok(bytes)
    }

    pub(crate) fn emit_foreign_bytes_result(
        &mut self,
        bytes: &GcLocal<ByteArray>,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let realm = self.emit_execution_realm(f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let prototype = s.reserve_value_local(f);
        let buffer_value = s.reserve_value_local(f);
        let length = s.reserve_i64_local(f);
        let offset = s.reserve_i64_local(f);
        let fixed = s.reserve_i32_local(f);
        let byte = s.reserve_i32_local(f);
        let index = s.reserve_i32_local(f);
        s.array_type::<ByteArray>().length(bytes, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        length.store(f);
        offset.set_constant(0, f);
        fixed.set_constant(0, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
            &prototype,
            f,
        );
        let buffer = self.emit_binary_allocate_array_buffer(
            &prototype, length, length, fixed, false, &pending, exit, f,
        )?;
        buffer_value.set_reference(&buffer, s, f);
        let owner = self.emit_binary_buffer_owner(&buffer_value, &pending, exit, f)?;
        let access = self.emit_binary_buffer_access(&owner, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        offset.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(done, f);
        offset.load(f);
        f.instruction(&Instruction::I32WrapI64);
        index.store(f);
        s.array_type::<ByteArray>()
            .read(bytes, index, s, f)
            .store(byte, f);
        access.write_byte(offset, byte, self, f)?;
        self.emit_increment_local(offset, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        access.clear(s, f);
        offset.set_constant(0, f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::Uint8ArrayPrototype,
            &prototype,
            f,
        );
        let array = self.emit_binary_allocate_typed_array(
            TypedArrayElementKind::Uint8,
            &prototype,
            &owner,
            offset,
            length,
            fixed,
            f,
        )?;
        output.set_reference(&array, s, f);
        pending.set_normal(output, f);
        array.clear(f);
        owner.clear(f);
        buffer.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&pending, f);
        pending.clear(f);
        buffer_value.clear(f);
        prototype.clear(f);
        s.release_i32_local(index, f);
        s.release_i32_local(byte, f);
        s.release_i32_local(fixed, f);
        s.release_i64_local(offset, f);
        s.release_i64_local(length, f);
        realm.clear(f);
        self.emit_propagate_current_throw_if_needed(f);
        Ok(())
    }
}
