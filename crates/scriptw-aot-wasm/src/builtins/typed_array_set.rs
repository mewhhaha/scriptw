//! Native Set keeps bounds/coercion order and snapshots only aliased data blocks.
use super::super::*;
use crate::gc_types::{
    BufferOwnerKind, BufferOwnerSchema, BufferViewSchema, ByteArray, CompletionLocals,
    GcHostImport, GcI32Constant, GcLocal, GcOperand, HostResourceSchema, I32Local, I64Local,
    SharedArrayBufferSchema, TypedArrayObject, TypedArrayObjectSchema,
};

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_typed_array_set_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        let source = s.reserve_value_local(f);
        let offset_argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let target_length = s.reserve_i64_local(f);
        let source_length = s.reserve_i64_local(f);
        let relative = s.reserve_i64_local(f);
        let offset = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let target_index = s.reserve_i64_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let target = self.emit_array_native_typed_brand(
            &receiver,
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SET_REQUIRES_TYPEDARRAY,
            &pending,
            f,
        )?;
        // The proposal's immutable check precedes offset conversion; bounds
        // validation deliberately follows its negative-offset rejection.
        self.emit_validate_typed_array_writable_buffer(&target, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_builtin_arg_to_value(1, &offset_argument, f);
        self.emit_value_to_number_payload(&offset_argument, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            relative,
            f,
        );
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_range_error(
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SET_OFFSET_IS_OUT_OF_RANGE,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_validate_typed_array_view(&target, target_length, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_builtin_arg_to_value(0, &source, f);
        source.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<TypedArrayObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(source.cast_reference::<TypedArrayObject>(s, f), f);
        self.emit_validate_typed_array_view(&array, source_length, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        self.emit_typed_array_set_range(
            relative,
            target_length,
            source_length,
            offset,
            &pending,
            f,
        )?;
        let target_kind = s.reserve_i32_local(f);
        let source_kind = s.reserve_i32_local(f);
        let target_bigint = s.reserve_i32_local(f);
        let source_bigint = s.reserve_i32_local(f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&target, s, f)
            .store(target_kind, f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&array, s, f)
            .store(source_kind, f);
        self.emit_typed_array_set_bigint_kind(target_kind, target_bigint, f);
        self.emit_typed_array_set_bigint_kind(source_kind, source_bigint, f);
        target_bigint.load(f);
        source_bigint.load(f);
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SET_SOURCE_AND_TARGET_CONTENT_TYPES_DIFFER,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let same_block = s.reserve_i32_local(f);
        self.emit_typed_array_set_same_block(&target, &array, same_block, f);
        same_block.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_set_byte_snapshot(
            &target,
            &array,
            source_length,
            offset,
            target_kind,
            source_kind,
            &pending,
            f,
        )?;
        f.instruction(&Instruction::Else);
        // Distinct shared data blocks retain the specification's ordered
        // read/write accesses instead of an unobservable eager snapshot.
        self.emit_typed_array_set_distinct(
            &target,
            &array,
            source_length,
            offset,
            target_kind,
            source_kind,
            &pending,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(same_block, f);
        s.release_i32_local(source_bigint, f);
        s.release_i32_local(target_bigint, f);
        s.release_i32_local(source_kind, f);
        s.release_i32_local(target_kind, f);
        array.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_array_like_length_snapshot(&source, source_length, &pending, f)?;
        // Infinity and the size check follow source ToObject/Get(length).
        self.emit_typed_array_set_range(
            relative,
            target_length,
            source_length,
            offset,
            &pending,
            f,
        )?;
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        source_length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        self.emit_typed_array_or_object_index_read_from_locals(&source, index, &pending, f)?;
        self.emit_array_native_propagate(&pending, f);
        offset_argument.copy_from(pending.value(), f);
        offset.load(f);
        index.load(f);
        f.instruction(&Instruction::I64Add);
        target_index.store(f);
        self.emit_typed_array_element_write_from_locals(
            &target,
            target_index,
            &offset_argument,
            &pending,
            f,
        )?;
        self.emit_array_native_propagate(&pending, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        offset_argument.set_undefined(f);
        self.completion().set_normal(&offset_argument, f);
        target.clear(f);
        for local in [
            target_index,
            index,
            offset,
            relative,
            source_length,
            target_length,
        ] {
            s.release_i64_local(local, f);
        }
        pending.clear(f);
        offset_argument.clear(f);
        source.clear(f);
        receiver.clear(f);
        Ok(())
    }

    fn emit_typed_array_set_range(
        &mut self,
        relative: I64Local,
        target: I64Local,
        source: I64Local,
        offset: I64Local,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        target.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Gt);
        source.load(f);
        target.load(f);
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_range_error(
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SET_SOURCE_IS_TOO_LARGE,
            pending,
            f,
        )?;
        self.emit_array_native_propagate(pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        relative.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncF64U);
        offset.store(f);
        offset.load(f);
        target.load(f);
        source.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_range_error(
            RuntimeErrorMessage::TYPEDARRAY_PROTOTYPE_SET_SOURCE_IS_TOO_LARGE,
            pending,
            f,
        )?;
        self.emit_array_native_propagate(pending, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_typed_array_set_bigint_kind(
        &mut self,
        kind: I32Local,
        output: I32Local,
        f: &mut Function,
    ) {
        f.instruction(&Instruction::I32Const(0));
        for k in TypedArrayElementKind::ALL {
            if k.content_type() == TypedArrayContentType::BigInt {
                kind.load(f);
                f.instruction(&Instruction::I32Const(k.encode()));
                f.instruction(&Instruction::I32Eq);
                f.instruction(&Instruction::I32Or);
            }
        }
        output.store(f);
    }

    fn emit_typed_array_set_same_block(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        source: &GcLocal<TypedArrayObject>,
        same: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let target_view = s.reserve_gc_local(f).initialize(
            s.field(TypedArrayObjectSchema::VIEW)
                .read(target, s, f)
                .reference(),
            f,
        );
        let source_view = s.reserve_gc_local(f).initialize(
            s.field(TypedArrayObjectSchema::VIEW)
                .read(source, s, f)
                .reference(),
            f,
        );
        let target_buffer = s.reserve_gc_local(f).initialize(
            s.field(BufferViewSchema::BUFFER)
                .read(&target_view, s, f)
                .reference(),
            f,
        );
        let source_buffer = s.reserve_gc_local(f).initialize(
            s.field(BufferViewSchema::BUFFER)
                .read(&source_view, s, f)
                .reference(),
            f,
        );
        let target_kind = s.reserve_i32_local(f);
        let source_kind = s.reserve_i32_local(f);
        s.field(BufferOwnerSchema::KIND)
            .read(&target_buffer, s, f)
            .store(target_kind, f);
        s.field(BufferOwnerSchema::KIND)
            .read(&source_buffer, s, f)
            .store(source_kind, f);
        f.instruction(&Instruction::I32Const(0));
        same.store(f);
        target_kind.load(f);
        source_kind.load(f);
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        target_kind.load(f);
        f.instruction(&Instruction::I32Const(
            BufferOwnerKind::ArrayBuffer.encode(),
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let _ = s
            .field(BufferOwnerSchema::ARRAY_BUFFER)
            .read(&target_buffer, s, f);
        let _ = s
            .field(BufferOwnerSchema::ARRAY_BUFFER)
            .read(&source_buffer, s, f);
        f.instruction(&Instruction::RefEq);
        same.store(f);
        f.instruction(&Instruction::Else);
        if let Some(base) = self
            .functions
            .gc_host_imports()
            .get(GcHostImport::SharedBufferBase)
        {
            let first = s.reserve_i64_local(f);
            for (buffer, save) in [(&target_buffer, true), (&source_buffer, false)] {
                let shared = s.reserve_gc_local(f).initialize(
                    s.field(BufferOwnerSchema::SHARED_ARRAY_BUFFER)
                        .read(buffer, s, f)
                        .reference()
                        .require_non_null(f),
                    f,
                );
                let resource = s.reserve_gc_local(f).initialize(
                    s.field(SharedArrayBufferSchema::BACKING_RESOURCE)
                        .read(&shared, s, f)
                        .reference(),
                    f,
                );
                let _ = s.field(HostResourceSchema::RESOURCE).read(&resource, s, f);
                base.emit_call_instruction(f);
                if save {
                    first.store(f);
                } else {
                    first.load(f);
                    f.instruction(&Instruction::I64Eq);
                    same.store(f);
                }
                resource.clear(f);
                shared.clear(f);
            }
            s.release_i64_local(first, f);
        } else {
            f.instruction(&Instruction::Unreachable);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(source_kind, f);
        s.release_i32_local(target_kind, f);
        source_buffer.clear(f);
        target_buffer.clear(f);
        source_view.clear(f);
        target_view.clear(f);
    }

    fn emit_typed_array_set_distinct(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        source: &GcLocal<TypedArrayObject>,
        length: I64Local,
        offset: I64Local,
        target_kind: I32Local,
        source_kind: I32Local,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        let index = s.reserve_i64_local(f);
        let target_index = s.reserve_i64_local(f);
        target_kind.load(f);
        source_kind.load(f);
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_typed_array_set_byte_copy(target, source, length, offset, f)?;
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        offset.load(f);
        index.load(f);
        f.instruction(&Instruction::I64Add);
        target_index.store(f);
        self.emit_typed_array_element_read_from_locals(source, index, &value, f)?;
        self.emit_typed_array_element_write_from_locals(target, target_index, &value, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(target_index, f);
        s.release_i64_local(index, f);
        value.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn emit_native_typed_array_byte_access(
        &mut self,
        array: &GcLocal<TypedArrayObject>,
        offset: I64Local,
        width: I64Local,
        kind: I32Local,
        f: &mut Function,
    ) -> super::binary_data::BufferAccess {
        let s = self.runtime_schema();
        let view = s.reserve_gc_local(f).initialize(
            s.field(TypedArrayObjectSchema::VIEW)
                .read(array, s, f)
                .reference(),
            f,
        );
        s.field(BufferViewSchema::BYTE_OFFSET)
            .read(&view, s, f)
            .store_i64(offset, f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(array, s, f)
            .store(kind, f);
        for k in TypedArrayElementKind::ALL {
            kind.load(f);
            f.instruction(&Instruction::I32Const(k.encode()));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            f.instruction(&Instruction::I64Const(k.bytes_per_element() as i64));
            width.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        let buffer = s.reserve_gc_local(f).initialize(
            s.field(BufferViewSchema::BUFFER)
                .read(&view, s, f)
                .reference(),
            f,
        );
        let access = self.emit_binary_buffer_access(&buffer, f);
        buffer.clear(f);
        view.clear(f);
        access
    }

    pub(in crate::builtins) fn emit_typed_array_set_byte_copy(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        source: &GcLocal<TypedArrayObject>,
        length: I64Local,
        offset: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let from = s.reserve_i64_local(f);
        let to = s.reserve_i64_local(f);
        let width = s.reserve_i64_local(f);
        let target_width = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let kind = s.reserve_i32_local(f);
        let source_access = self.emit_native_typed_array_byte_access(source, from, width, kind, f);
        let target_access =
            self.emit_native_typed_array_byte_access(target, to, target_width, kind, f);
        offset.load(f);
        target_width.load(f);
        f.instruction(&Instruction::I64Mul);
        to.load(f);
        f.instruction(&Instruction::I64Add);
        to.store(f);
        length.load(f);
        width.load(f);
        f.instruction(&Instruction::I64Mul);
        count.store(f);
        self.emit_binary_copy_bytes(&source_access, from, &target_access, to, count, f)?;
        target_access.clear(s, f);
        source_access.clear(s, f);
        s.release_i32_local(kind, f);
        s.release_i64_local(count, f);
        s.release_i64_local(target_width, f);
        s.release_i64_local(width, f);
        s.release_i64_local(to, f);
        s.release_i64_local(from, f);
        Ok(())
    }

    fn emit_typed_array_set_byte_snapshot(
        &mut self,
        target: &GcLocal<TypedArrayObject>,
        source: &GcLocal<TypedArrayObject>,
        length: I64Local,
        offset: I64Local,
        target_kind: I32Local,
        source_kind: I32Local,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        let from = s.reserve_i64_local(f);
        let to = s.reserve_i64_local(f);
        let width = s.reserve_i64_local(f);
        let target_width = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let cursor = s.reserve_i64_local(f);
        let address = s.reserve_i64_local(f);
        let word = s.reserve_i64_local(f);
        let byte_index = s.reserve_i64_local(f);
        let target_index = s.reserve_i64_local(f);
        let kind = s.reserve_i32_local(f);
        let extent = s.reserve_i32_local(f);
        let slot = s.reserve_i32_local(f);
        let byte = s.reserve_i32_local(f);
        let source_access = self.emit_native_typed_array_byte_access(source, from, width, kind, f);
        let target_access =
            self.emit_native_typed_array_byte_access(target, to, target_width, kind, f);
        length.load(f);
        width.load(f);
        f.instruction(&Instruction::I64Mul);
        count.store(f);
        count.load(f);
        f.instruction(&Instruction::I32WrapI64);
        extent.store(f);
        let snapshot = s.reserve_gc_local(f).initialize(
            s.array_type::<ByteArray>()
                .filled(GcOperand::i32(0), extent, f),
            f,
        );
        f.instruction(&Instruction::I64Const(0));
        cursor.store(f);
        let cloned = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(cloned, f);
        from.load(f);
        cursor.load(f);
        f.instruction(&Instruction::I64Add);
        address.store(f);
        source_access.read_byte(address, byte, self, f)?;
        cursor.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<ByteArray>()
            .write(&snapshot, slot, GcOperand::i32_local(byte), s, f);
        self.emit_increment_local(cursor, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        target_kind.load(f);
        source_kind.load(f);
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        offset.load(f);
        target_width.load(f);
        f.instruction(&Instruction::I64Mul);
        to.load(f);
        f.instruction(&Instruction::I64Add);
        to.store(f);
        f.instruction(&Instruction::I64Const(0));
        cursor.store(f);
        let copied = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(copied, f);
        cursor.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<ByteArray>()
            .read(&snapshot, slot, s, f)
            .store(byte, f);
        to.load(f);
        cursor.load(f);
        f.instruction(&Instruction::I64Add);
        address.store(f);
        target_access.write_byte(address, byte, self, f)?;
        self.emit_increment_local(cursor, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I64Const(0));
        cursor.store(f);
        let copied = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(copied, f);
        f.instruction(&Instruction::I64Const(0));
        word.store(f);
        f.instruction(&Instruction::I64Const(0));
        byte_index.store(f);
        let next_byte = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        width.load(f);
        f.instruction(&Instruction::I64Mul);
        byte_index.load(f);
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_type::<ByteArray>()
            .read(&snapshot, slot, s, f)
            .store(byte, f);
        word.load(f);
        byte.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        byte_index.load(f);
        f.instruction(&Instruction::I64Const(8));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Shl);
        f.instruction(&Instruction::I64Or);
        word.store(f);
        self.emit_increment_local(byte_index, 1, f);
        byte_index.load(f);
        width.load(f);
        f.instruction(&Instruction::I64LtU);
        self.emit_branch_if_to_target(next_byte, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.emit_typed_array_element_word_to_value(source_kind, word, &value, f)?;
        offset.load(f);
        cursor.load(f);
        f.instruction(&Instruction::I64Add);
        target_index.store(f);
        self.emit_typed_array_element_write_from_locals(target, target_index, &value, pending, f)?;
        self.emit_array_native_propagate(pending, f);
        self.emit_increment_local(cursor, 1, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        snapshot.clear(f);
        target_access.clear(s, f);
        source_access.clear(s, f);
        for local in [byte, slot, extent, kind] {
            s.release_i32_local(local, f);
        }
        for local in [
            target_index,
            byte_index,
            word,
            address,
            cursor,
            count,
            target_width,
            width,
            to,
            from,
        ] {
            s.release_i64_local(local, f);
        }
        value.clear(f);
        Ok(())
    }
}
