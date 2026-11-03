use super::*;

impl FunctionBuilder<'_> {
    pub(super) fn emit_stream_locked(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let stream = self.stream_this::<ReadableStreamObject>(&output, exit, f)?;
        let reader = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamObjectSchema::READER)
                .read(&stream, s, f)
                .reference(),
            f,
        );
        let locked = s.reserve_i32_local(f);
        reader.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        locked.store(f);
        output.value().set_boolean(locked, f);
        s.release_i32_local(locked, f);
        reader.clear(f);
        stream.clear(f);
        self.stream_end_block(f);
        self.completion().copy_from(&output, f);
        output.clear(f);
        Ok(())
    }
    pub(super) fn emit_stream_get_reader(
        &mut self,
        constructor: bool,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        let source = s.reserve_value_local(f);
        let options = s.reserve_value_local(f);
        let new_target = s.reserve_value_local(f);
        let prototype = s.reserve_value_local(f);
        if constructor {
            self.emit_builtin_arg_to_value(0, &source, f);
            self.compile_new_target_to_locals(&new_target, f)?;
        } else {
            self.compile_this_to_locals(&source, f)?;
            self.emit_builtin_arg_to_value(0, &options, f);
        }
        let exit = self.open_frame(ControlFrameKind::Block, f);
        if constructor {
            self.stream_is_undefined(&new_target, f);
            self.open_frame(ControlFrameKind::If, f);
            self.stream_error_exit(RuntimeErrorMessage::STREAM_REQUIRES_NEW, &output, exit, f)?;
            self.stream_end_if(f);
        }
        let stream = self.emit_binary_require_ref::<ReadableStreamObject>(
            &source,
            RuntimeErrorMessage::STREAM_RECEIVER,
            &output,
            exit,
            f,
        )?;
        if constructor {
            self.emit_get_prototype_from_constructor(
                &new_target,
                OrdinaryDefaultPrototype::ReadableStreamDefaultReader,
                &pending,
                f,
            )?;
            self.stream_abrupt(&pending, &output, exit, f);
            prototype.copy_from(pending.value(), f);
        }
        if !constructor {
            self.compile_nullish_tagged_i32(options.tag(), f)?;
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_is_heap_object_like_tag_i32(options.tag(), f);
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            self.stream_error_exit(RuntimeErrorMessage::STREAM_DICTIONARY, &output, exit, f)?;
            self.stream_end_if(f);
            self.emit_array_native_get(&options, "mode", &pending, f)?;
            self.stream_abrupt(&pending, &output, exit, f);
            self.stream_is_undefined(pending.value(), f);
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            let mode = s.reserve_value_local(f);
            mode.copy_from(pending.value(), f);
            self.emit_value_to_string_payload(&mode, &pending, f)?;
            mode.clear(f);
            self.stream_abrupt(&pending, &output, exit, f);
            self.stream_error_exit(
                RuntimeErrorMessage::STREAM_BYOB_UNSUPPORTED,
                &output,
                exit,
                f,
            )?;
            self.stream_end_if(f);
            self.stream_end_if(f);
        }
        let existing = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamObjectSchema::READER)
                .read(&stream, s, f)
                .reference(),
            f,
        );
        existing.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(RuntimeErrorMessage::STREAM_LOCKED, &output, exit, f)?;
        self.stream_end_if(f);
        existing.clear(f);
        if !constructor {
            let realm = s
                .reserve_gc_local(f)
                .initialize(self.emit_current_function_realm(f), f);
            self.emit_load_non_array_realm_intrinsic(
                &realm,
                Slot::ReadableStreamDefaultReaderPrototype,
                &prototype,
                f,
            );
            realm.clear(f);
        }
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), f)?,
            f,
        );
        let closed = self.stream_new_promise(f)?;
        let reader = s.reserve_gc_local(f).initialize(
            s.struct_type::<ReadableStreamDefaultReaderObject>()
                .construct(
                    (
                        GcOperand::reference(&header, s),
                        GcOperand::nullable_reference(&stream, s),
                        GcOperand::reference(&closed, s),
                        GcOperand::null(s),
                        GcOperand::null(s),
                    ),
                    f,
                ),
            f,
        );
        s.field(ReadableStreamObjectSchema::READER).write(
            &stream,
            GcOperand::nullable_reference(&reader, s),
            s,
            f,
        );
        self.stream_is_state(&stream, ReadableStreamState::Closed, f);
        self.open_frame(ControlFrameKind::If, f);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        self.emit_resolve_promise_record(&closed, &undefined, f)?;
        undefined.clear(f);
        self.stream_end_if(f);
        self.stream_is_state(&stream, ReadableStreamState::Errored, f);
        self.open_frame(ControlFrameKind::If, f);
        let error = self.stream_stored_error(&stream, f);
        s.field(PromiseObjectSchema::HANDLED)
            .write(&closed, GcOperand::boolean(true), s, f);
        self.emit_settle_promise_record(&closed, PromiseSettlement::Reject, &error, f)?;
        error.clear(f);
        self.stream_end_if(f);
        output.value().set_reference(&reader, s, f);
        reader.clear(f);
        closed.clear(f);
        header.clear(f);
        stream.clear(f);
        self.stream_end_block(f);
        self.completion().copy_from(&output, f);
        prototype.clear(f);
        new_target.clear(f);
        options.clear(f);
        source.clear(f);
        pending.clear(f);
        output.clear(f);
        Ok(())
    }
    fn stream_stored_error(
        &self,
        stream: &GcLocal<ReadableStreamObject>,
        f: &mut Function,
    ) -> ValueLocals {
        let s = self.runtime_schema();
        let stored = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamObjectSchema::STORED_ERROR)
                .read(stream, s, f)
                .reference(),
            f,
        );
        let error = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &error, s, f);
        stored.clear(f);
        error
    }
    fn stream_require_reader_stream(
        &mut self,
        reader: &GcLocal<ReadableStreamDefaultReaderObject>,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<ReadableStreamObject>, EmitError> {
        let s = self.runtime_schema();
        let maybe = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::STREAM)
                .read(reader, s, f)
                .reference(),
            f,
        );
        maybe.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(RuntimeErrorMessage::STREAM_RELEASED, output, exit, f)?;
        self.stream_end_if(f);
        let stream = s
            .reserve_gc_local(f)
            .initialize(maybe.load(s, f).require_non_null(f), f);
        maybe.clear(f);
        Ok(stream)
    }
    pub(super) fn emit_stream_reader_read(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let promise = self.stream_new_promise(f)?;
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let reader = self.stream_this::<ReadableStreamDefaultReaderObject>(&output, exit, f)?;
        let stream = self.stream_require_reader_stream(&reader, &output, exit, f)?;
        self.stream_is_state(&stream, ReadableStreamState::Closed, f);
        self.open_frame(ControlFrameKind::If, f);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        self.stream_resolve_read(&promise, &undefined, true, f)?;
        undefined.clear(f);
        self.emit_branch_to_target(exit, f);
        self.stream_end_if(f);
        self.stream_is_state(&stream, ReadableStreamState::Errored, f);
        self.open_frame(ControlFrameKind::If, f);
        let error = self.stream_stored_error(&stream, f);
        self.emit_settle_promise_record(&promise, PromiseSettlement::Reject, &error, f)?;
        error.clear(f);
        self.emit_branch_to_target(exit, f);
        self.stream_end_if(f);
        let controller = self.stream_controller(&stream, f);
        let head = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::QUEUE_HEAD)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        head.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_append_read(&reader, &promise, f);
        self.stream_call_pull(&controller, f)?;
        f.instruction(&Instruction::Else);
        let next = s.reserve_gc_local(f).initialize(
            s.field(StreamByteChunkSchema::NEXT)
                .read(&head, s, f)
                .reference(),
            f,
        );
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_HEAD)
            .write(&controller, GcOperand::reference(&next, s), s, f);
        next.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_TAIL)
            .write(&controller, GcOperand::null(s), s, f);
        self.stream_end_if(f);
        let count = s.reserve_i64_local(f);
        let length = s.reserve_i64_local(f);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_BYTES)
            .read(&controller, s, f)
            .store_i64(count, f);
        s.field(StreamByteChunkSchema::BYTE_LENGTH)
            .read(&head, s, f)
            .store_i64(length, f);
        count.load(f);
        length.load(f);
        f.instruction(&Instruction::I64Sub);
        count.store(f);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_BYTES)
            .write(&controller, GcOperand::i64_local(count), s, f);
        let closing = s.reserve_i32_local(f);
        s.field(ReadableByteStreamControllerObjectSchema::CLOSE_REQUESTED)
            .read(&controller, s, f)
            .store(closing, f);
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        closing.load(f);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_finish(&stream, None, f)?;
        f.instruction(&Instruction::Else);
        self.stream_call_pull(&controller, f)?;
        self.stream_end_if(f);
        let chunk = s.reserve_gc_local(f).initialize(
            s.field(StreamByteChunkSchema::VALUE)
                .read(&head, s, f)
                .reference(),
            f,
        );
        let value = s.reserve_value_local(f);
        value.set_reference(&chunk, s, f);
        self.stream_resolve_read(&promise, &value, false, f)?;
        value.clear(f);
        chunk.clear(f);
        s.release_i32_local(closing, f);
        s.release_i64_local(length, f);
        s.release_i64_local(count, f);
        next.clear(f);
        self.stream_end_if(f);
        head.clear(f);
        controller.clear(f);
        stream.clear(f);
        reader.clear(f);
        self.stream_end_block(f);
        output.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_settle_promise_record(&promise, PromiseSettlement::Reject, output.value(), f)?;
        self.stream_end_if(f);
        self.completion().value().set_reference(&promise, s, f);
        self.completion().set_kind(CompletionKind::Normal, f);
        output.clear(f);
        promise.clear(f);
        Ok(())
    }
    pub(super) fn emit_stream_reader_closed(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let reader = self.stream_this::<ReadableStreamDefaultReaderObject>(&output, exit, f)?;
        let closed = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::CLOSED)
                .read(&reader, s, f)
                .reference(),
            f,
        );
        output.value().set_reference(&closed, s, f);
        closed.clear(f);
        reader.clear(f);
        self.stream_end_block(f);
        output.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let promise = self.stream_new_promise(f)?;
        self.emit_settle_promise_record(&promise, PromiseSettlement::Reject, output.value(), f)?;
        output.value().set_reference(&promise, s, f);
        output.set_kind(CompletionKind::Normal, f);
        promise.clear(f);
        self.stream_end_if(f);
        self.completion().copy_from(&output, f);
        output.clear(f);
        Ok(())
    }
    pub(super) fn emit_stream_reader_release(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let reader = self.stream_this::<ReadableStreamDefaultReaderObject>(&output, exit, f)?;
        let maybe = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::STREAM)
                .read(&reader, s, f)
                .reference(),
            f,
        );
        maybe.load(s, f).is_null(f);
        self.emit_branch_if_to_target(exit, f);
        let stream = s
            .reserve_gc_local(f)
            .initialize(maybe.load(s, f).require_non_null(f), f);
        let error = s.reserve_completion(f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::STREAM_RELEASED,
            &error,
            f,
        )?;
        let closed = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::CLOSED)
                .read(&reader, s, f)
                .reference(),
            f,
        );
        self.stream_is_state(&stream, ReadableStreamState::Readable, f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(PromiseObjectSchema::HANDLED)
            .write(&closed, GcOperand::boolean(true), s, f);
        self.emit_settle_promise_record(&closed, PromiseSettlement::Reject, error.value(), f)?;
        f.instruction(&Instruction::Else);
        let replacement = self.stream_new_promise(f)?;
        s.field(PromiseObjectSchema::HANDLED)
            .write(&replacement, GcOperand::boolean(true), s, f);
        self.emit_settle_promise_record(&replacement, PromiseSettlement::Reject, error.value(), f)?;
        s.field(ReadableStreamDefaultReaderObjectSchema::CLOSED)
            .write(&reader, GcOperand::reference(&replacement, s), s, f);
        replacement.clear(f);
        self.stream_end_if(f);
        s.field(ReadableStreamObjectSchema::READER)
            .write(&stream, GcOperand::null(s), s, f);
        s.field(ReadableStreamDefaultReaderObjectSchema::STREAM)
            .write(&reader, GcOperand::null(s), s, f);
        let stop = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        let request = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::READ_HEAD)
                .read(&reader, s, f)
                .reference(),
            f,
        );
        request.load(s, f).is_null(f);
        self.emit_branch_if_to_target(stop, f);
        request.clear(f);
        let promise = self.stream_shift_read(&reader, f);
        self.emit_settle_promise_record(&promise, PromiseSettlement::Reject, error.value(), f)?;
        promise.clear(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.stream_end_block(f);
        closed.clear(f);
        error.clear(f);
        stream.clear(f);
        maybe.clear(f);
        reader.clear(f);
        self.stream_end_block(f);
        self.completion().copy_from(&output, f);
        output.clear(f);
        Ok(())
    }
    pub(super) fn emit_stream_cancel(
        &mut self,
        via_reader: bool,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let constructor = self.emit_current_function_realm_intrinsic_promise_constructor(f);
        let capability =
            self.emit_new_current_function_realm_intrinsic_promise_capability(constructor, f)?;
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let reason = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &reason, f);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let stream = if via_reader {
            let reader = self.stream_this::<ReadableStreamDefaultReaderObject>(&output, exit, f)?;
            let stream = self.stream_require_reader_stream(&reader, &output, exit, f)?;
            reader.clear(f);
            stream
        } else {
            let stream = self.stream_this::<ReadableStreamObject>(&output, exit, f)?;
            let reader = s.reserve_gc_local(f).initialize(
                s.field(ReadableStreamObjectSchema::READER)
                    .read(&stream, s, f)
                    .reference(),
                f,
            );
            reader.load(s, f).is_null(f);
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            self.stream_error_exit(RuntimeErrorMessage::STREAM_LOCKED, &output, exit, f)?;
            self.stream_end_if(f);
            reader.clear(f);
            stream
        };
        self.stream_is_state(&stream, ReadableStreamState::Closed, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_call_promise_capability(
            &capability,
            PromiseSettlement::Fulfill,
            &undefined,
            &pending,
            f,
        )?;
        self.emit_branch_to_target(exit, f);
        self.stream_end_if(f);
        self.stream_is_state(&stream, ReadableStreamState::Errored, f);
        self.open_frame(ControlFrameKind::If, f);
        let error = self.stream_stored_error(&stream, f);
        self.emit_call_promise_capability(
            &capability,
            PromiseSettlement::Reject,
            &error,
            &pending,
            f,
        )?;
        error.clear(f);
        self.emit_branch_to_target(exit, f);
        self.stream_end_if(f);
        let controller = self.stream_controller(&stream, f);
        let source = s.reserve_value_local(f);
        let cancel = s.reserve_value_local(f);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::SOURCE)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &source, s, f);
        stored.clear(f);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::CANCEL)
                .read(&controller, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &cancel, s, f);
        stored.clear(f);
        self.stream_clear_queue(&controller, f);
        self.stream_finish(&stream, None, f)?;
        let callback_realm = self.stream_callback_realm(&cancel, f)?;
        self.stream_is_undefined(&cancel, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&reason], f);
        self.emit_function_or_proxy_call_with_argv(&cancel, &source, &argv, &pending, f)?;
        argv.clear(f);
        self.stream_end_if(f);
        let fulfilled = self.stream_callback(
            &controller,
            StandardBuiltinId::ReadableStreamCancelFulfilled,
            f,
        )?;
        let promise = self.stream_callback_promise(&callback_realm, &pending, f)?;
        self.emit_perform_promise_then(&promise, &fulfilled, &undefined, &capability, f)?;
        promise.clear(f);
        callback_realm.clear(f);
        fulfilled.clear(f);
        cancel.clear(f);
        source.clear(f);
        controller.clear(f);
        stream.clear(f);
        self.stream_end_block(f);
        output.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_call_promise_capability(
            &capability,
            PromiseSettlement::Reject,
            output.value(),
            &pending,
            f,
        )?;
        self.stream_end_if(f);
        self.emit_read_promise_capability_promise(&capability, self.completion().value(), f);
        self.completion().set_kind(CompletionKind::Normal, f);
        undefined.clear(f);
        reason.clear(f);
        pending.clear(f);
        output.clear(f);
        capability.clear(f);
        Ok(())
    }
}
