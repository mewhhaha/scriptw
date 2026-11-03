use super::*;

impl FunctionBuilder<'_> {
    pub(super) fn emit_stream_pull_if_needed(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let controller = self.stream_capture(f);
        let stream = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::STREAM)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let flag = s.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.stream_is_state(&stream, ReadableStreamState::Readable, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        s.field(ReadableByteStreamControllerObjectSchema::STARTED)
            .read(&controller, s, f)
            .store(flag, f);
        flag.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        s.field(ReadableByteStreamControllerObjectSchema::CLOSE_REQUESTED)
            .read(&controller, s, f)
            .store(flag, f);
        flag.load(f);
        self.emit_branch_if_to_target(exit, f);
        let should_pull = s.reserve_i32_local(f);
        should_pull.set_constant(0, f);
        let reader = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamObjectSchema::READER)
                .read(&stream, s, f)
                .reference(),
            f,
        );
        reader.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let request = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::READ_HEAD)
                .read(&reader, s, f)
                .reference(),
            f,
        );
        request.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        should_pull.store(f);
        request.clear(f);
        self.stream_end_if(f);
        let hwm = s.reserve_f64_local(f);
        s.field(ReadableByteStreamControllerObjectSchema::HIGH_WATER_MARK)
            .read(&controller, s, f)
            .store_f64(hwm, f);
        let bytes = s.reserve_i64_local(f);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_BYTES)
            .read(&controller, s, f)
            .store_i64(bytes, f);
        hwm.load(f);
        bytes.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Gt);
        should_pull.load(f);
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        s.field(ReadableByteStreamControllerObjectSchema::PULLING)
            .read(&controller, s, f)
            .store(flag, f);
        flag.load(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(ReadableByteStreamControllerObjectSchema::PULL_AGAIN)
            .write(&controller, GcOperand::boolean(true), s, f);
        self.emit_branch_to_target(exit, f);
        self.stream_end_if(f);
        s.field(ReadableByteStreamControllerObjectSchema::PULLING)
            .write(&controller, GcOperand::boolean(true), s, f);
        let source = s.reserve_value_local(f);
        let pull = s.reserve_value_local(f);
        let source_stored = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::SOURCE)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&source_stored, &source, s, f);
        let pull_stored = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::PULL)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&pull_stored, &pull, s, f);
        let callback_realm = self.stream_callback_realm(&pull, f)?;
        self.stream_is_undefined(&pull, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let value = s.reserve_value_local(f);
        value.set_reference(&controller, s, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&value], f);
        self.emit_function_or_proxy_call_with_argv(&pull, &source, &argv, &pending, f)?;
        argv.clear(f);
        value.clear(f);
        self.stream_end_if(f);
        let promise = self.stream_callback_promise(&callback_realm, &pending, f)?;
        self.stream_observe_promise(&controller, &promise, false, f)?;
        promise.clear(f);
        callback_realm.clear(f);
        pull_stored.clear(f);
        source_stored.clear(f);
        pull.clear(f);
        source.clear(f);
        s.release_i64_local(bytes, f);
        s.release_f64_local(hwm, f);
        reader.clear(f);
        s.release_i32_local(should_pull, f);
        self.stream_end_block(f);
        self.completion().value().set_undefined(f);
        self.completion().set_kind(CompletionKind::Normal, f);
        s.release_i32_local(flag, f);
        pending.clear(f);
        stream.clear(f);
        controller.clear(f);
        Ok(())
    }
    pub(super) fn emit_stream_reaction(
        &mut self,
        builtin: StandardBuiltinId,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let controller = self.stream_capture(f);
        let stream = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::STREAM)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        match builtin {
            StandardBuiltinId::ReadableStreamStartRejected
            | StandardBuiltinId::ReadableStreamPullRejected => {
                let error = s.reserve_value_local(f);
                self.emit_builtin_arg_to_value(0, &error, f);
                self.stream_finish(&stream, Some(&error), f)?;
                error.clear(f);
            }
            StandardBuiltinId::ReadableStreamStartFulfilled => {
                s.field(ReadableByteStreamControllerObjectSchema::STARTED)
                    .write(&controller, GcOperand::boolean(true), s, f);
                self.stream_call_pull(&controller, f)?;
            }
            StandardBuiltinId::ReadableStreamPullFulfilled => {
                s.field(ReadableByteStreamControllerObjectSchema::PULLING)
                    .write(&controller, GcOperand::boolean(false), s, f);
                let again = s.reserve_i32_local(f);
                s.field(ReadableByteStreamControllerObjectSchema::PULL_AGAIN)
                    .read(&controller, s, f)
                    .store(again, f);
                again.load(f);
                self.open_frame(ControlFrameKind::If, f);
                s.field(ReadableByteStreamControllerObjectSchema::PULL_AGAIN)
                    .write(&controller, GcOperand::boolean(false), s, f);
                self.stream_call_pull(&controller, f)?;
                self.stream_end_if(f);
                s.release_i32_local(again, f);
            }
            _ => return Err(EmitError::unsupported("invalid stream reaction")),
        }
        self.completion().value().set_undefined(f);
        self.completion().set_kind(CompletionKind::Normal, f);
        stream.clear(f);
        controller.clear(f);
        Ok(())
    }
    pub(super) fn emit_stream_controller_close(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let controller =
            self.stream_this::<ReadableByteStreamControllerObject>(&output, exit, f)?;
        let stream = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::STREAM)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        self.stream_require_enqueue(&controller, &stream, &output, exit, f)?;
        s.field(ReadableByteStreamControllerObjectSchema::CLOSE_REQUESTED)
            .write(&controller, GcOperand::boolean(true), s, f);
        let bytes = s.reserve_i64_local(f);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_BYTES)
            .read(&controller, s, f)
            .store_i64(bytes, f);
        bytes.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_finish(&stream, None, f)?;
        self.stream_end_if(f);
        s.release_i64_local(bytes, f);
        stream.clear(f);
        controller.clear(f);
        self.stream_end_block(f);
        self.completion().copy_from(&output, f);
        output.clear(f);
        Ok(())
    }
    pub(super) fn emit_stream_controller_error(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let controller =
            self.stream_this::<ReadableByteStreamControllerObject>(&output, exit, f)?;
        let stream = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::STREAM)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        let error = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &error, f);
        self.stream_finish(&stream, Some(&error), f)?;
        error.clear(f);
        stream.clear(f);
        controller.clear(f);
        self.stream_end_block(f);
        self.completion().copy_from(&output, f);
        output.clear(f);
        Ok(())
    }
    pub(super) fn emit_stream_controller_desired_size(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let controller =
            self.stream_this::<ReadableByteStreamControllerObject>(&output, exit, f)?;
        let stream = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::STREAM)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        self.stream_is_state(&stream, ReadableStreamState::Errored, f);
        self.open_frame(ControlFrameKind::If, f);
        output.value().set_scalar(ScalarValue::Null, f);
        f.instruction(&Instruction::Else);
        self.stream_is_state(&stream, ReadableStreamState::Closed, f);
        self.open_frame(ControlFrameKind::If, f);
        output.value().set_scalar(ScalarValue::NumberBits(0), f);
        f.instruction(&Instruction::Else);
        let bytes = s.reserve_i64_local(f);
        let hwm = s.reserve_f64_local(f);
        let result = s.reserve_i64_local(f);
        s.field(ReadableByteStreamControllerObjectSchema::HIGH_WATER_MARK)
            .read(&controller, s, f)
            .store_f64(hwm, f);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_BYTES)
            .read(&controller, s, f)
            .store_i64(bytes, f);
        hwm.load(f);
        bytes.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::F64Sub);
        f.instruction(&Instruction::I64ReinterpretF64);
        result.store(f);
        output.value().set_number(result, f);
        s.release_i64_local(result, f);
        s.release_f64_local(hwm, f);
        s.release_i64_local(bytes, f);
        self.stream_end_if(f);
        self.stream_end_if(f);
        stream.clear(f);
        controller.clear(f);
        self.stream_end_block(f);
        self.completion().copy_from(&output, f);
        output.clear(f);
        Ok(())
    }
    fn stream_require_enqueue(
        &mut self,
        controller: &GcLocal<ReadableByteStreamControllerObject>,
        stream: &GcLocal<ReadableStreamObject>,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let closing = s.reserve_i32_local(f);
        s.field(ReadableByteStreamControllerObjectSchema::CLOSE_REQUESTED)
            .read(controller, s, f)
            .store(closing, f);
        self.stream_is_state(stream, ReadableStreamState::Readable, f);
        f.instruction(&Instruction::I32Eqz);
        closing.load(f);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(RuntimeErrorMessage::STREAM_CLOSED, output, exit, f)?;
        self.stream_end_if(f);
        s.release_i32_local(closing, f);
        Ok(())
    }
    pub(super) fn emit_stream_controller_enqueue(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let input = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let controller =
            self.stream_this::<ReadableByteStreamControllerObject>(&output, exit, f)?;
        let stream = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::STREAM)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        let length = s.reserve_i64_local(f);
        // Validate the view before checking the stream state, then transfer only
        // after both are known valid. The sender's entire buffer is detached.
        let view = self.stream_chunk_view(&input, length, &output, exit, f)?;
        self.stream_require_enqueue(&controller, &stream, &output, exit, f)?;
        let chunk = self.stream_transfer_chunk(&view, length, &output, exit, f)?;
        let reader = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamObjectSchema::READER)
                .read(&stream, s, f)
                .reference(),
            f,
        );
        let delivered = s.reserve_i32_local(f);
        delivered.set_constant(0, f);
        reader.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let request = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::READ_HEAD)
                .read(&reader, s, f)
                .reference(),
            f,
        );
        request.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let actual = s
            .reserve_gc_local(f)
            .initialize(reader.load(s, f).require_non_null(f), f);
        let promise = self.stream_shift_read(&actual, f);
        let value = s.reserve_value_local(f);
        value.set_reference(&chunk, s, f);
        self.stream_resolve_read(&promise, &value, false, f)?;
        delivered.set_constant(1, f);
        value.clear(f);
        promise.clear(f);
        actual.clear(f);
        self.stream_end_if(f);
        request.clear(f);
        self.stream_end_if(f);
        delivered.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_append_chunk(&controller, &chunk, length, f);
        self.stream_end_if(f);
        self.stream_call_pull(&controller, f)?;
        s.release_i32_local(delivered, f);
        reader.clear(f);
        chunk.clear(f);
        view.clear(f);
        s.release_i64_local(length, f);
        stream.clear(f);
        controller.clear(f);
        self.stream_end_block(f);
        self.completion().copy_from(&output, f);
        input.clear(f);
        output.clear(f);
        Ok(())
    }
    fn stream_chunk_view(
        &mut self,
        input: &ValueLocals,
        length: I64Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<BufferView>, EmitError> {
        let s = self.runtime_schema();
        let selected = s
            .reserve_gc_local::<BufferView, Nullable>(f)
            .initialize_null(s, f);
        let width = s.reserve_i64_local(f);
        width.set_constant(1, f);
        input.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(input.cast_reference::<TypedArrayObject>(s, f), f);
        selected.replace(
            s.field(TypedArrayObjectSchema::VIEW)
                .read(&array, s, f)
                .reference()
                .nullable(),
            f,
        );
        let kind = s.reserve_i32_local(f);
        s.field(TypedArrayObjectSchema::ELEMENT_KIND)
            .read(&array, s, f)
            .store(kind, f);
        for element in TypedArrayElementKind::ALL {
            kind.load(f);
            f.instruction(&Instruction::I32Const(GcI32Constant::encode(element)));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            width.set_constant(element.bytes_per_element() as i64, f);
            self.stream_end_if(f);
        }
        s.release_i32_local(kind, f);
        array.clear(f);
        f.instruction(&Instruction::Else);
        let data = self.emit_binary_require_ref::<DataViewObject>(
            input,
            RuntimeErrorMessage::STREAM_CHUNK,
            output,
            exit,
            f,
        )?;
        selected.replace(
            s.field(DataViewObjectSchema::VIEW)
                .read(&data, s, f)
                .reference()
                .nullable(),
            f,
        );
        data.clear(f);
        self.stream_end_if(f);
        let view = s
            .reserve_gc_local(f)
            .initialize(selected.load(s, f).require_non_null(f), f);
        selected.clear(f);
        let owner = s.reserve_gc_local(f).initialize(
            s.field(BufferViewSchema::BUFFER)
                .read(&view, s, f)
                .reference(),
            f,
        );
        let access = self.emit_binary_buffer_access(&owner, f);
        let valid = s.reserve_i32_local(f);
        self.emit_binary_view_length(&view, &access, length, valid, f);
        length.load(f);
        width.load(f);
        f.instruction(&Instruction::I64DivU);
        width.load(f);
        f.instruction(&Instruction::I64Mul);
        length.store(f);
        valid.load(f);
        f.instruction(&Instruction::I32Eqz);
        access.shared.load(f);
        f.instruction(&Instruction::I32Or);
        length.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(RuntimeErrorMessage::STREAM_CHUNK, output, exit, f)?;
        self.stream_end_if(f);
        s.release_i32_local(valid, f);
        access.clear(s, f);
        owner.clear(f);
        s.release_i64_local(width, f);
        Ok(view)
    }
    fn stream_transfer_chunk(
        &mut self,
        view: &GcLocal<BufferView>,
        length: I64Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<TypedArrayObject>, EmitError> {
        let s = self.runtime_schema();
        let owner = s.reserve_gc_local(f).initialize(
            s.field(BufferViewSchema::BUFFER)
                .read(view, s, f)
                .reference(),
            f,
        );
        let buffer = s.reserve_gc_local(f).initialize(
            s.field(BufferOwnerSchema::ARRAY_BUFFER)
                .read(&owner, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let immutable = s.reserve_i32_local(f);
        s.field(ArrayBufferSchema::IMMUTABLE)
            .read(&buffer, s, f)
            .store(immutable, f);
        immutable.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(RuntimeErrorMessage::STREAM_CHUNK, output, exit, f)?;
        self.stream_end_if(f);
        let bytes = s.reserve_gc_local(f).initialize(
            s.field(ArrayBufferSchema::BYTES)
                .read(&buffer, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let byte_length = s.reserve_i64_local(f);
        let offset = s.reserve_i64_local(f);
        s.field(ArrayBufferSchema::BYTE_LENGTH)
            .read(&buffer, s, f)
            .store_i64(byte_length, f);
        s.field(BufferViewSchema::BYTE_OFFSET)
            .read(view, s, f)
            .store_i64(offset, f);
        let original = s.reserve_value_local(f);
        original.set_reference(&buffer, s, f);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let pending = s.reserve_completion(f);
        self.emit_detach_array_buffer(&original, &undefined, &pending, f)?;
        self.stream_abrupt(&pending, output, exit, f);
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let prototype = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(&realm, Slot::ArrayBufferPrototype, &prototype, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), f)?,
            f,
        );
        let key = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        let transferred = s.reserve_gc_local(f).initialize(
            s.struct_type::<ArrayBuffer>().construct(
                (
                    GcOperand::reference(&header, s),
                    GcOperand::nullable_reference(&bytes, s),
                    GcOperand::i64_local(byte_length),
                    GcOperand::reference(&key, s),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::i64_local(byte_length),
                ),
                f,
            ),
            f,
        );
        let new_owner = s.reserve_gc_local(f).initialize(
            s.struct_type::<BufferOwner>().construct(
                (
                    GcOperand::constant(BufferOwnerKind::ArrayBuffer),
                    GcOperand::nullable_reference(&transferred, s),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        self.emit_load_non_array_realm_intrinsic(&realm, Slot::Uint8ArrayPrototype, &prototype, f);
        let tracking = s.reserve_i32_local(f);
        tracking.set_constant(0, f);
        let chunk = self.emit_binary_allocate_typed_array(
            TypedArrayElementKind::Uint8,
            &prototype,
            &new_owner,
            offset,
            length,
            tracking,
            f,
        )?;
        s.release_i32_local(tracking, f);
        new_owner.clear(f);
        transferred.clear(f);
        key.clear(f);
        header.clear(f);
        prototype.clear(f);
        realm.clear(f);
        pending.clear(f);
        undefined.clear(f);
        original.clear(f);
        s.release_i64_local(offset, f);
        s.release_i64_local(byte_length, f);
        bytes.clear(f);
        s.release_i32_local(immutable, f);
        buffer.clear(f);
        owner.clear(f);
        Ok(chunk)
    }
}
