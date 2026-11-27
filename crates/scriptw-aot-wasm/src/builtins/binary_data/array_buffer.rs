use super::*;

// The host GC allocator takes an unsigned 32-bit byte-array length (`u32`,
// up to 4 GiB - 8 bytes so 8-byte rounding stays representable in `u32`).
const MAX_ARRAY_BUFFER_BACKING_LENGTH: i64 = (u32::MAX as i64) - 7;

impl FunctionBuilder<'_> {
    /// Allocation limits belong after prototype selection. The logical maximum
    /// remains an I64; both extents must fit the same GC ByteArray representation.
    pub(in crate::builtins) fn emit_binary_allocate_array_buffer(
        &mut self,
        prototype: &ValueLocals,
        length: I64Local,
        maximum: I64Local,
        resizable: I32Local,
        immutable: bool,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<ArrayBuffer>, EmitError> {
        let s = self.runtime_schema();
        length.load(f);
        f.instruction(&Instruction::I64Const(MAX_ARRAY_BUFFER_BACKING_LENGTH));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::ARRAYBUFFER_ALLOCATION_SIZE_IS_TOO_LARGE,
            output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let size = s.reserve_i32_local(f);
        length.load(f);
        f.instruction(&Instruction::I64Const(7));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Const(-8));
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I32WrapI64);
        size.store(f);
        let import = self
            .functions
            .gc_host_imports()
            .get(GcHostImport::ByteArrayAllocate)
            .ok_or_else(|| {
                EmitError::unsupported("ArrayBuffer backing requires its native GC allocator")
            })?;
        let allocation = s
            .reserve_gc_local::<ByteArray, Nullable>(f)
            .initialize(import.allocate_byte_array(size, f)?, f);
        allocation.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::ARRAYBUFFER_ALLOCATION_SIZE_IS_TOO_LARGE,
            output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let bytes = s
            .reserve_gc_local(f)
            .initialize(allocation.load(s, f).require_non_null(f), f);
        allocation.clear(f);
        let object = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(prototype), f)?,
            f,
        );
        // AllocateArrayBuffer admits the resizable maximum after creating the
        // object/current backing. A wide ToIndex result is not enough: growth
        // to this extent must be representable by the same backing allocator.
        maximum.load(f);
        f.instruction(&Instruction::I64Const(MAX_ARRAY_BUFFER_BACKING_LENGTH));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::ARRAYBUFFER_ALLOCATION_SIZE_IS_TOO_LARGE,
            output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let key = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&undefined, f), f);
        let buffer = s.reserve_gc_local(f).initialize(
            s.struct_type::<ArrayBuffer>().construct(
                (
                    GcOperand::reference(&object, s),
                    GcOperand::nullable_reference(&bytes, s),
                    GcOperand::i64_local(maximum),
                    GcOperand::reference(&key, s),
                    GcOperand::boolean_local(resizable),
                    GcOperand::boolean(immutable),
                    GcOperand::i64_local(length),
                ),
                f,
            ),
            f,
        );
        key.clear(f);
        undefined.clear(f);
        object.clear(f);
        bytes.clear(f);
        s.release_i32_local(size, f);
        Ok(buffer)
    }

    pub(in crate::builtins) fn emit_array_buffer_constructor_builtin(
        &mut self,
        kind: BufferConstructorKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        let length_arg = s.reserve_value_local(f);
        let options = s.reserve_value_local(f);
        let new_target = s.reserve_value_local(f);
        new_target.copy_from(
            self.body_entry_locals()
                .expect("native constructor")
                .new_target(),
            f,
        );
        let length = s.reserve_i64_local(f);
        let maximum = s.reserve_i64_local(f);
        let resizable = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_binary_require_new_target(
            &new_target,
            RuntimeErrorMessage::ARRAYBUFFER_CONSTRUCTOR_REQUIRES_NEW,
            &output,
            exit,
            f,
        )?;
        self.emit_builtin_arg_to_value(0, &length_arg, f);
        self.emit_builtin_arg_to_value(1, &options, f);
        self.emit_to_index_i64_from_value_locals(
            &length_arg,
            length,
            RuntimeErrorMessage::ARRAYBUFFER_ALLOCATION_SIZE_IS_TOO_LARGE,
            &pending,
            f,
        )?;
        self.emit_binary_abrupt_exit(&pending, &output, exit, f);
        length.load(f);
        maximum.store(f);
        f.instruction(&Instruction::I32Const(0));
        resizable.store(f);
        self.emit_is_heap_object_like_tag_i32(options.tag(), f);
        self.open_frame(ControlFrameKind::If, f);
        let key = self.emit_binary_string_key("maxByteLength", f)?;
        self.emit_object_read(&options, &options, &key, &pending, f)?;
        key.clear(f);
        self.emit_binary_abrupt_exit(&pending, &output, exit, f);
        pending.value().tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        let max_arg = s.reserve_value_local(f);
        max_arg.copy_from(pending.value(), f);
        self.emit_to_index_i64_from_value_locals(
            &max_arg,
            maximum,
            RuntimeErrorMessage::ARRAYBUFFER_ALLOCATION_SIZE_IS_TOO_LARGE,
            &pending,
            f,
        )?;
        max_arg.clear(f);
        self.emit_binary_abrupt_exit(&pending, &output, exit, f);
        f.instruction(&Instruction::I32Const(1));
        resizable.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        maximum.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::ARRAYBUFFER_ALLOCATION_SIZE_IS_TOO_LARGE,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let intrinsic = match kind {
            BufferConstructorKind::ArrayBuffer => OrdinaryDefaultPrototype::ArrayBuffer,
            BufferConstructorKind::SharedArrayBuffer => OrdinaryDefaultPrototype::SharedArrayBuffer,
        };
        self.emit_get_prototype_from_constructor(&new_target, intrinsic, &pending, f)?;
        self.emit_binary_abrupt_exit(&pending, &output, exit, f);
        match kind {
            BufferConstructorKind::ArrayBuffer => {
                let buffer = self.emit_binary_allocate_array_buffer(
                    pending.value(),
                    length,
                    maximum,
                    resizable,
                    false,
                    &output,
                    exit,
                    f,
                )?;
                output.value().set_reference(&buffer, s, f);
                buffer.clear(f);
            }
            BufferConstructorKind::SharedArrayBuffer => {
                let import = self
                    .functions
                    .gc_host_imports()
                    .get(GcHostImport::SharedBufferAllocate)
                    .ok_or_else(|| {
                        EmitError::unsupported("SAB allocation requires its native resource import")
                    })?;
                let resource = import.allocate_shared_buffer(length, maximum, resizable, f)?;
                resource.is_null(f);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_binary_range_error(
                    RuntimeErrorMessage::ARRAYBUFFER_ALLOCATION_SIZE_IS_TOO_LARGE,
                    &output,
                    exit,
                    f,
                )?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                let resource = s
                    .reserve_gc_local(f)
                    .initialize(resource.into_resource(s, f), f);
                let object = s.reserve_gc_local(f).initialize(
                    self.emit_alloc_plain_object_with_prototype(Some(pending.value()), f)?,
                    f,
                );
                let buffer = s.reserve_gc_local(f).initialize(
                    s.struct_type::<SharedArrayBuffer>().construct(
                        (
                            GcOperand::reference(&object, s),
                            GcOperand::reference(&resource, s),
                        ),
                        f,
                    ),
                    f,
                );
                output.value().set_reference(&buffer, s, f);
                buffer.clear(f);
                object.clear(f);
                resource.clear(f);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        s.release_i32_local(resizable, f);
        s.release_i64_local(maximum, f);
        s.release_i64_local(length, f);
        new_target.clear(f);
        options.clear(f);
        length_arg.clear(f);
        pending.clear(f);
        output.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_array_buffer_accessor_builtin(
        &mut self,
        kind: ArrayBufferAccessor,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let receiver = s.reserve_value_local(f);
        receiver.copy_from(
            self.body_entry_locals()
                .expect("native accessor")
                .this_value(),
            f,
        );
        let number = s.reserve_i64_local(f);
        let flag = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let message = match kind {
            ArrayBufferAccessor::ByteLength => {
                RuntimeErrorMessage::ARRAYBUFFER_BYTELENGTH_GETTER_REQUIRES_ARRAYBUFFER
            }
            ArrayBufferAccessor::MaxByteLength => {
                RuntimeErrorMessage::ARRAYBUFFER_MAXBYTELENGTH_GETTER_REQUIRES_ARRAYBUFFER
            }
            ArrayBufferAccessor::Resizable => {
                RuntimeErrorMessage::ARRAYBUFFER_RESIZABLE_GETTER_REQUIRES_ARRAYBUFFER
            }
            ArrayBufferAccessor::Immutable => {
                RuntimeErrorMessage::ARRAYBUFFER_IMMUTABLE_GETTER_REQUIRES_ARRAYBUFFER
            }
            ArrayBufferAccessor::Detached => {
                RuntimeErrorMessage::ARRAYBUFFER_DETACHED_GETTER_REQUIRES_ARRAYBUFFER
            }
        };
        let buffer =
            self.emit_binary_require_ref::<ArrayBuffer>(&receiver, message, &output, exit, f)?;
        let bytes = s.reserve_gc_local(f).initialize(
            s.field(ArrayBufferSchema::BYTES)
                .read(&buffer, s, f)
                .reference(),
            f,
        );
        match kind {
            ArrayBufferAccessor::Detached => {
                bytes.load(s, f).is_null(f);
                flag.store(f);
                output.value().set_boolean(flag, f);
            }
            ArrayBufferAccessor::Resizable => {
                s.field(ArrayBufferSchema::RESIZABLE)
                    .read(&buffer, s, f)
                    .store(flag, f);
                output.value().set_boolean(flag, f);
            }
            ArrayBufferAccessor::Immutable => {
                s.field(ArrayBufferSchema::IMMUTABLE)
                    .read(&buffer, s, f)
                    .store(flag, f);
                output.value().set_boolean(flag, f);
            }
            ArrayBufferAccessor::ByteLength | ArrayBufferAccessor::MaxByteLength => {
                f.instruction(&Instruction::I64Const(0));
                number.store(f);
                bytes.load(s, f).is_null(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                match kind {
                    ArrayBufferAccessor::ByteLength => {
                        s.field(ArrayBufferSchema::BYTE_LENGTH)
                            .read(&buffer, s, f)
                            .store_i64(number, f);
                    }
                    ArrayBufferAccessor::MaxByteLength => {
                        s.field(ArrayBufferSchema::MAX_BYTE_LENGTH)
                            .read(&buffer, s, f)
                            .store_i64(number, f);
                    }
                    ArrayBufferAccessor::Resizable
                    | ArrayBufferAccessor::Immutable
                    | ArrayBufferAccessor::Detached => {
                        unreachable!("numeric accessor")
                    }
                }
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                number.load(f);
                f.instruction(&Instruction::F64ConvertI64U);
                f.instruction(&Instruction::I64ReinterpretF64);
                number.store(f);
                output.value().set_number(number, f);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        bytes.clear(f);
        buffer.clear(f);
        self.completion().copy_from(&output, f);
        s.release_i32_local(flag, f);
        s.release_i64_local(number, f);
        receiver.clear(f);
        output.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_shared_array_buffer_accessor_builtin(
        &mut self,
        kind: SharedArrayBufferAccessor,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let receiver = s.reserve_value_local(f);
        receiver.copy_from(
            self.body_entry_locals()
                .expect("native accessor")
                .this_value(),
            f,
        );
        let number = s.reserve_i64_local(f);
        let flag = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let buffer = self.emit_binary_require_ref::<SharedArrayBuffer>(
            &receiver,
            RuntimeErrorMessage::SHAREDARRAYBUFFER_GETTER_REQUIRES_SHAREDARRAYBUFFER,
            &output,
            exit,
            f,
        )?;
        let resource = s.reserve_gc_local(f).initialize(
            s.field(SharedArrayBufferSchema::BACKING_RESOURCE)
                .read(&buffer, s, f)
                .reference(),
            f,
        );
        let import = match kind {
            SharedArrayBufferAccessor::ByteLength => GcHostImport::SharedBufferLength,
            SharedArrayBufferAccessor::MaxByteLength => GcHostImport::SharedBufferMaximum,
            SharedArrayBufferAccessor::Growable => GcHostImport::SharedBufferGrowable,
        };
        let import = self
            .functions
            .gc_host_imports()
            .get(import)
            .ok_or_else(|| EmitError::unsupported("SAB accessor requires its resource import"))?;
        let _ = s.field(HostResourceSchema::RESOURCE).read(&resource, s, f);
        import.emit_call_instruction(f);
        match kind {
            SharedArrayBufferAccessor::Growable => {
                flag.store(f);
                output.value().set_boolean(flag, f);
            }
            SharedArrayBufferAccessor::ByteLength | SharedArrayBufferAccessor::MaxByteLength => {
                f.instruction(&Instruction::F64ConvertI64U);
                f.instruction(&Instruction::I64ReinterpretF64);
                number.store(f);
                output.value().set_number(number, f);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        resource.clear(f);
        buffer.clear(f);
        self.completion().copy_from(&output, f);
        s.release_i32_local(flag, f);
        s.release_i64_local(number, f);
        receiver.clear(f);
        output.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_array_buffer_is_view_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let input = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        let flag = s.reserve_i32_local(f);
        input.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<DataViewObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        input.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Or);
        flag.store(f);
        let output = s.reserve_completion(f);
        output.initialize(f);
        output.value().set_boolean(flag, f);
        self.completion().copy_from(&output, f);
        output.clear(f);
        s.release_i32_local(flag, f);
        input.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_array_buffer_resize_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        let receiver = s.reserve_value_local(f);
        receiver.copy_from(
            self.body_entry_locals()
                .expect("native resize")
                .this_value(),
            f,
        );
        let argument = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &argument, f);
        let size = s.reserve_i64_local(f);
        let maximum = s.reserve_i64_local(f);
        let copy = s.reserve_i64_local(f);
        let zero = s.reserve_i64_local(f);
        let flag = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let buffer = self.emit_binary_require_ref::<ArrayBuffer>(
            &receiver,
            RuntimeErrorMessage::ARRAYBUFFER_RESIZE_RECEIVER_IS_NOT_RESIZABLE_ARRAYBUFFER,
            &output,
            exit,
            f,
        )?;
        s.field(ArrayBufferSchema::RESIZABLE)
            .read(&buffer, s, f)
            .store(flag, f);
        flag.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::ARRAYBUFFER_RESIZE_RECEIVER_IS_NOT_RESIZABLE_ARRAYBUFFER,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_to_index_i64_from_value_locals(
            &argument,
            size,
            RuntimeErrorMessage::ARRAYBUFFER_RESIZE_LENGTH_IS_OUT_OF_RANGE,
            &pending,
            f,
        )?;
        self.emit_binary_abrupt_exit(&pending, &output, exit, f);
        let owner = self.emit_binary_buffer_owner(&receiver, &output, exit, f)?;
        let source = self.emit_binary_buffer_access(&owner, f);
        source.valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::TYPEDARRAY_BACKING_BUFFER_IS_DETACHED,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.field(ArrayBufferSchema::MAX_BYTE_LENGTH)
            .read(&buffer, s, f)
            .store_i64(maximum, f);
        size.load(f);
        maximum.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::ARRAYBUFFER_RESIZE_LENGTH_IS_OUT_OF_RANGE,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let prototype = s.reserve_value_local(f);
        prototype.set_scalar(ScalarValue::Null, f);
        let fresh = self.emit_binary_allocate_array_buffer(
            &prototype, size, maximum, flag, false, &output, exit, f,
        )?;
        let fresh_value = s.reserve_value_local(f);
        fresh_value.set_reference(&fresh, s, f);
        let fresh_owner = self.emit_binary_buffer_owner(&fresh_value, &output, exit, f)?;
        let target = self.emit_binary_buffer_access(&fresh_owner, f);
        size.load(f);
        source.length.load(f);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        size.load(f);
        f.instruction(&Instruction::Else);
        source.length.load(f);
        f.instruction(&Instruction::End);
        copy.store(f);
        f.instruction(&Instruction::I64Const(0));
        zero.store(f);
        self.emit_binary_copy_bytes(&source, zero, &target, zero, copy, f)?;
        s.field(ArrayBufferSchema::BYTES).write(
            &buffer,
            GcOperand::reference(&target.bytes, s),
            s,
            f,
        );
        s.field(ArrayBufferSchema::BYTE_LENGTH)
            .write(&buffer, GcOperand::i64_local(size), s, f);
        target.clear(s, f);
        fresh_owner.clear(f);
        fresh_value.clear(f);
        fresh.clear(f);
        prototype.clear(f);
        source.clear(s, f);
        owner.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        buffer.clear(f);
        self.completion().copy_from(&output, f);
        s.release_i32_local(flag, f);
        s.release_i64_local(zero, f);
        s.release_i64_local(copy, f);
        s.release_i64_local(maximum, f);
        s.release_i64_local(size, f);
        argument.clear(f);
        receiver.clear(f);
        pending.clear(f);
        output.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_shared_array_buffer_grow_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        let receiver = s.reserve_value_local(f);
        receiver.copy_from(
            self.body_entry_locals().expect("native grow").this_value(),
            f,
        );
        let argument = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &argument, f);
        let size = s.reserve_i64_local(f);
        let maximum = s.reserve_i64_local(f);
        let current = s.reserve_i64_local(f);
        let status = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let buffer = self.emit_binary_require_ref::<SharedArrayBuffer>(
            &receiver,
            RuntimeErrorMessage::SHAREDARRAYBUFFER_GROW_RECEIVER_IS_NOT_GROWABLE_SHAREDARRAYBUFFER,
            &output,
            exit,
            f,
        )?;
        let resource = s.reserve_gc_local(f).initialize(
            s.field(SharedArrayBufferSchema::BACKING_RESOURCE)
                .read(&buffer, s, f)
                .reference(),
            f,
        );
        let imports = self.functions.gc_host_imports();
        let growable = imports
            .get(GcHostImport::SharedBufferGrowable)
            .ok_or_else(|| EmitError::unsupported("SAB growability import"))?;
        let maximum_import = imports
            .get(GcHostImport::SharedBufferMaximum)
            .ok_or_else(|| EmitError::unsupported("SAB maximum import"))?;
        let length_import = imports
            .get(GcHostImport::SharedBufferLength)
            .ok_or_else(|| EmitError::unsupported("SAB length import"))?;
        let grow = imports
            .get(GcHostImport::SharedBufferGrow)
            .ok_or_else(|| EmitError::unsupported("SAB grow import"))?;
        let _ = s.field(HostResourceSchema::RESOURCE).read(&resource, s, f);
        growable.emit_call_instruction(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::SHAREDARRAYBUFFER_GROW_RECEIVER_IS_NOT_GROWABLE_SHAREDARRAYBUFFER,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_to_index_i64_from_value_locals(
            &argument,
            size,
            RuntimeErrorMessage::SHAREDARRAYBUFFER_GROW_LENGTH_IS_OUT_OF_RANGE,
            &pending,
            f,
        )?;
        self.emit_binary_abrupt_exit(&pending, &output, exit, f);
        let _ = s.field(HostResourceSchema::RESOURCE).read(&resource, s, f);
        maximum_import.emit_call_instruction(f);
        maximum.store(f);
        let _ = s.field(HostResourceSchema::RESOURCE).read(&resource, s, f);
        length_import.emit_call_instruction(f);
        current.store(f);
        size.load(f);
        maximum.load(f);
        f.instruction(&Instruction::I64GtU);
        size.load(f);
        current.load(f);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::SHAREDARRAYBUFFER_GROW_LENGTH_IS_OUT_OF_RANGE,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let _ = s.field(HostResourceSchema::RESOURCE).read(&resource, s, f);
        size.load(f);
        grow.emit_call_instruction(f);
        status.store(f);
        status.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::SHAREDARRAYBUFFER_GROW_LENGTH_IS_OUT_OF_RANGE,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        resource.clear(f);
        buffer.clear(f);
        self.completion().copy_from(&output, f);
        s.release_i32_local(status, f);
        s.release_i64_local(current, f);
        s.release_i64_local(maximum, f);
        s.release_i64_local(size, f);
        argument.clear(f);
        receiver.clear(f);
        pending.clear(f);
        output.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    fn emit_binary_buffer_species(
        &mut self,
        receiver: &ValueLocals,
        shared: bool,
        result: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let constructor = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            if shared {
                NonArrayRealmIntrinsicSlot::SharedArrayBufferConstructor
            } else {
                NonArrayRealmIntrinsicSlot::ArrayBufferConstructor
            },
            &constructor,
            f,
        );
        let property = s.reserve_completion(f);
        let key = self.emit_binary_string_key("constructor", f)?;
        self.emit_object_read(receiver, receiver, &key, &property, f)?;
        key.clear(f);
        self.emit_binary_abrupt_exit(&property, output, exit, f);
        property.value().tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_heap_object_like_tag_i32(property.value().tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::ARRAYBUFFER_SPECIES_CONSTRUCTOR_RETURNED_INVALID_ARRAYBUFFER,
            output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let original = s.reserve_value_local(f);
        original.copy_from(property.value(), f);
        let symbol = s.reserve_gc_local(f).initialize(
            self.emit_well_known_symbol_reference(WellKnownSymbol::Species, f)?,
            f,
        );
        let key = PropertyKeyLocals::from_symbol(s, &symbol, f);
        symbol.clear(f);
        self.emit_object_read(&original, &original, &key, &property, f)?;
        key.clear(f);
        original.clear(f);
        self.emit_binary_abrupt_exit(&property, output, exit, f);
        property.value().tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        property.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_constructor_i32(property.value(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::ARRAYBUFFER_SPECIES_CONSTRUCTOR_RETURNED_INVALID_ARRAYBUFFER,
            output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        constructor.copy_from(property.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        result.set_normal(&constructor, f);
        property.clear(f);
        constructor.clear(f);
        realm.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_array_buffer_slice_builtin(
        &mut self,
        kind: BufferSliceKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        let receiver = s.reserve_value_local(f);
        receiver.copy_from(
            self.body_entry_locals().expect("native slice").this_value(),
            f,
        );
        let start = s.reserve_value_local(f);
        let end = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &start, f);
        self.emit_builtin_arg_to_value(1, &end, f);
        let first = s.reserve_i64_local(f);
        let final_index = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let copied = s.reserve_i64_local(f);
        let zero = s.reserve_i64_local(f);
        let target_value = s.reserve_value_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let shared = matches!(kind, BufferSliceKind::SharedArrayBuffer);
        match kind {
            BufferSliceKind::ArrayBuffer | BufferSliceKind::Immutable => {
                let buffer = self.emit_binary_require_ref::<ArrayBuffer>(
                    &receiver,
                    RuntimeErrorMessage::ARRAYBUFFER_SLICE_RECEIVER_IS_NOT_ARRAYBUFFER,
                    &output,
                    exit,
                    f,
                )?;
                buffer.clear(f);
            }
            BufferSliceKind::SharedArrayBuffer => {
                let buffer = self.emit_binary_require_ref::<SharedArrayBuffer>(
                    &receiver,
                    RuntimeErrorMessage::SHAREDARRAYBUFFER_SLICE_RECEIVER_IS_NOT_SHAREDARRAYBUFFER,
                    &output,
                    exit,
                    f,
                )?;
                buffer.clear(f);
            }
        }
        let source_owner = self.emit_binary_buffer_owner(&receiver, &output, exit, f)?;
        let entry = self.emit_binary_buffer_access(&source_owner, f);
        entry.valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::ARRAYBUFFER_SLICE_RECEIVER_IS_DETACHED,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_binary_relative_index(
            &start,
            entry.length,
            false,
            first,
            &pending,
            &output,
            exit,
            f,
        )?;
        self.emit_binary_relative_index(
            &end,
            entry.length,
            true,
            final_index,
            &pending,
            &output,
            exit,
            f,
        )?;
        final_index.load(f);
        first.load(f);
        f.instruction(&Instruction::I64GtU);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        final_index.load(f);
        first.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::Else);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::End);
        count.store(f);
        match kind {
            BufferSliceKind::Immutable => {
                // Both index conversions precede source reobservation, and the
                // exact final bound must still exist before target allocation.
                let source = self.emit_binary_buffer_access(&source_owner, f);
                source.valid.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_binary_type_error(
                    RuntimeErrorMessage::ARRAYBUFFER_SLICE_RECEIVER_IS_DETACHED,
                    &output,
                    exit,
                    f,
                )?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                source.length.load(f);
                final_index.load(f);
                f.instruction(&Instruction::I64LtU);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_binary_range_error(
                    RuntimeErrorMessage::ARRAYBUFFER_SLICE_SOURCE_IS_SHORTER_THAN_THE_RESOLVED_FINAL_BOUND,
                    &output,
                    exit,
                    f,
                )?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                source.clear(s, f);
                let realm = s
                    .reserve_gc_local(f)
                    .initialize(self.emit_current_function_realm(f), f);
                let prototype = s.reserve_value_local(f);
                self.emit_load_non_array_realm_intrinsic(
                    &realm,
                    NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
                    &prototype,
                    f,
                );
                let fixed = s.reserve_i32_local(f);
                f.instruction(&Instruction::I32Const(0));
                fixed.store(f);
                let buffer = self.emit_binary_allocate_array_buffer(
                    &prototype, count, count, fixed, true, &output, exit, f,
                )?;
                target_value.set_reference(&buffer, s, f);
                buffer.clear(f);
                s.release_i32_local(fixed, f);
                prototype.clear(f);
                realm.clear(f);
            }
            BufferSliceKind::ArrayBuffer | BufferSliceKind::SharedArrayBuffer => {
                self.emit_binary_buffer_species(&receiver, shared, &pending, &output, exit, f)?;
                let constructor = s.reserve_value_local(f);
                constructor.copy_from(pending.value(), f);
                let length_arg = s.reserve_value_local(f);
                count.load(f);
                f.instruction(&Instruction::F64ConvertI64U);
                f.instruction(&Instruction::I64ReinterpretF64);
                length_arg.scalar().store(f);
                length_arg.set_number(length_arg.scalar(), f);
                let arguments = self.emit_pre_evaluated_arg_vector(&[&length_arg], f);
                self.emit_function_or_proxy_construct_with_argv(
                    &constructor,
                    &constructor,
                    &arguments,
                    &pending,
                    f,
                )?;
                arguments.clear(f);
                length_arg.clear(f);
                constructor.clear(f);
                self.emit_binary_abrupt_exit(&pending, &output, exit, f);
                target_value.copy_from(pending.value(), f);
            }
        }
        if shared {
            let buffer=self.emit_binary_require_ref::<SharedArrayBuffer>(&target_value,
            RuntimeErrorMessage::SHAREDARRAYBUFFER_SPECIES_CONSTRUCTOR_RETURNED_INVALID_SHAREDARRAYBUFFER,&output,exit,f)?;
            buffer.clear(f);
        } else {
            let buffer = self.emit_binary_require_ref::<ArrayBuffer>(
                &target_value,
                RuntimeErrorMessage::ARRAYBUFFER_SPECIES_CONSTRUCTOR_RETURNED_INVALID_ARRAYBUFFER,
                &output,
                exit,
                f,
            )?;
            buffer.clear(f);
        }
        receiver.reference().load(f);
        target_value.reference().load(f);
        f.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::ARRAYBUFFER_SPECIES_CONSTRUCTOR_RETURNED_INVALID_ARRAYBUFFER,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let target_owner = self.emit_binary_buffer_owner(&target_value, &output, exit, f)?;
        let target = self.emit_binary_buffer_access(&target_owner, f);
        target.valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        target.length.load(f);
        count.load(f);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::I32Or);
        if !matches!(kind, BufferSliceKind::Immutable) {
            self.emit_binary_buffer_immutable_i32(&target_owner, f);
            f.instruction(&Instruction::I32Or);
        }
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::ARRAYBUFFER_SPECIES_CONSTRUCTOR_RETURNED_INVALID_ARRAYBUFFER,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        if shared {
            let base = self
                .functions
                .gc_host_imports()
                .get(GcHostImport::SharedBufferBase)
                .ok_or_else(|| EmitError::unsupported("shared slice base import"))?;
            let source_base = s.reserve_i64_local(f);
            let _ = s
                .field(HostResourceSchema::RESOURCE)
                .read(&entry.resource, s, f);
            base.emit_call_instruction(f);
            source_base.store(f);
            let _ = s
                .field(HostResourceSchema::RESOURCE)
                .read(&target.resource, s, f);
            base.emit_call_instruction(f);
            source_base.load(f);
            f.instruction(&Instruction::I64Eq);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_binary_type_error(RuntimeErrorMessage::SHAREDARRAYBUFFER_SPECIES_CONSTRUCTOR_RETURNED_INVALID_SHAREDARRAYBUFFER,&output,exit,f)?;
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            s.release_i64_local(source_base, f);
        }
        let source = self.emit_binary_buffer_access(&source_owner, f);
        source.valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::ARRAYBUFFER_SLICE_RECEIVER_IS_DETACHED,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        match kind {
            BufferSliceKind::Immutable => {
                count.load(f);
                copied.store(f);
            }
            BufferSliceKind::SharedArrayBuffer => {
                count.load(f);
                copied.store(f);
            }
            BufferSliceKind::ArrayBuffer => {
                f.instruction(&Instruction::I64Const(0));
                copied.store(f);
                first.load(f);
                source.length.load(f);
                f.instruction(&Instruction::I64LtU);
                self.open_frame(ControlFrameKind::If, f);
                source.length.load(f);
                first.load(f);
                f.instruction(&Instruction::I64Sub);
                copied.store(f);
                copied.load(f);
                count.load(f);
                f.instruction(&Instruction::I64GtU);
                self.open_frame(ControlFrameKind::If, f);
                count.load(f);
                copied.store(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        f.instruction(&Instruction::I64Const(0));
        zero.store(f);
        self.emit_binary_copy_bytes(&source, first, &target, zero, copied, f)?;
        output.set_normal(&target_value, f);
        source.clear(s, f);
        target.clear(s, f);
        target_owner.clear(f);
        entry.clear(s, f);
        source_owner.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        target_value.clear(f);
        s.release_i64_local(zero, f);
        s.release_i64_local(copied, f);
        s.release_i64_local(count, f);
        s.release_i64_local(final_index, f);
        s.release_i64_local(first, f);
        end.clear(f);
        start.clear(f);
        receiver.clear(f);
        pending.clear(f);
        output.clear(f);
        Ok(())
    }

    pub(in crate::builtins) fn emit_array_buffer_transfer_builtin(
        &mut self,
        kind: BufferTransferKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        let receiver = s.reserve_value_local(f);
        receiver.copy_from(
            self.body_entry_locals()
                .expect("native transfer")
                .this_value(),
            f,
        );
        let argument = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &argument, f);
        let length = s.reserve_i64_local(f);
        let maximum = s.reserve_i64_local(f);
        let count = s.reserve_i64_local(f);
        let zero = s.reserve_i64_local(f);
        let resizable = s.reserve_i32_local(f);
        let immutable = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let buffer = self.emit_binary_require_ref::<ArrayBuffer>(
            &receiver,
            RuntimeErrorMessage::ARRAYBUFFER_TRANSFER_RECEIVER_IS_NOT_ARRAYBUFFER,
            &output,
            exit,
            f,
        )?;
        let owner = self.emit_binary_buffer_owner(&receiver, &output, exit, f)?;
        let entry = self.emit_binary_buffer_access(&owner, f);
        argument.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        entry.length.load(f);
        length.store(f);
        f.instruction(&Instruction::Else);
        self.emit_to_index_i64_from_value_locals(
            &argument,
            length,
            RuntimeErrorMessage::ARRAYBUFFER_TRANSFER_LENGTH_IS_OUT_OF_RANGE,
            &pending,
            f,
        )?;
        self.emit_binary_abrupt_exit(&pending, &output, exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let source = self.emit_binary_buffer_access(&owner, f);
        source.valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::ARRAYBUFFER_TRANSFER_RECEIVER_IS_DETACHED,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.field(ArrayBufferSchema::IMMUTABLE)
            .read(&buffer, s, f)
            .store(immutable, f);
        immutable.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::ARRAYBUFFER_RECEIVER_IS_IMMUTABLE,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(ArrayBufferSchema::DETACH_KEY)
                .read(&buffer, s, f)
                .reference(),
            f,
        );
        let detach_key = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &detach_key, s, f);
        detach_key.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_type_error(
            RuntimeErrorMessage::DETACHARRAYBUFFER_KEY_DOES_NOT_MATCH_THE_ARRAYBUFFER_DETACH_KEY,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        length.load(f);
        maximum.store(f);
        f.instruction(&Instruction::I32Const(0));
        resizable.store(f);
        if matches!(kind, BufferTransferKind::PreserveResizable) {
            s.field(ArrayBufferSchema::RESIZABLE)
                .read(&buffer, s, f)
                .store(resizable, f);
            resizable.load(f);
            self.open_frame(ControlFrameKind::If, f);
            s.field(ArrayBufferSchema::MAX_BYTE_LENGTH)
                .read(&buffer, s, f)
                .store_i64(maximum, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        length.load(f);
        maximum.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_binary_range_error(
            RuntimeErrorMessage::ARRAYBUFFER_TRANSFER_LENGTH_IS_OUT_OF_RANGE,
            &output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let prototype = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
            &prototype,
            f,
        );
        let fresh = self.emit_binary_allocate_array_buffer(
            &prototype,
            length,
            maximum,
            resizable,
            matches!(kind, BufferTransferKind::Immutable),
            &output,
            exit,
            f,
        )?;
        let target_value = s.reserve_value_local(f);
        target_value.set_reference(&fresh, s, f);
        let target_owner = self.emit_binary_buffer_owner(&target_value, &output, exit, f)?;
        let target = self.emit_binary_buffer_access(&target_owner, f);
        length.load(f);
        source.length.load(f);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I64)));
        length.load(f);
        f.instruction(&Instruction::Else);
        source.length.load(f);
        f.instruction(&Instruction::End);
        count.store(f);
        f.instruction(&Instruction::I64Const(0));
        zero.store(f);
        self.emit_binary_copy_bytes(&source, zero, &target, zero, count, f)?;
        s.field(ArrayBufferSchema::BYTES)
            .write(&buffer, GcOperand::null(s), s, f);
        s.field(ArrayBufferSchema::BYTE_LENGTH)
            .write(&buffer, GcOperand::i64(0), s, f);
        output.set_normal(&target_value, f);
        target.clear(s, f);
        target_owner.clear(f);
        target_value.clear(f);
        fresh.clear(f);
        prototype.clear(f);
        realm.clear(f);
        detach_key.clear(f);
        stored.clear(f);
        source.clear(s, f);
        entry.clear(s, f);
        owner.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        buffer.clear(f);
        self.completion().copy_from(&output, f);
        s.release_i32_local(immutable, f);
        s.release_i32_local(resizable, f);
        s.release_i64_local(zero, f);
        s.release_i64_local(count, f);
        s.release_i64_local(maximum, f);
        s.release_i64_local(length, f);
        argument.clear(f);
        receiver.clear(f);
        pending.clear(f);
        output.clear(f);
        Ok(())
    }
}
