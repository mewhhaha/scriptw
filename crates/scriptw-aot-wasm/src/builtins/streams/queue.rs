use super::*;

impl FunctionBuilder<'_> {
    pub(super) fn stream_clear_algorithms(
        &self,
        controller: &GcLocal<ReadableByteStreamControllerObject>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let undefined = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        for field in [
            ReadableByteStreamControllerObjectSchema::SOURCE,
            ReadableByteStreamControllerObjectSchema::PULL,
            ReadableByteStreamControllerObjectSchema::CANCEL,
        ] {
            s.field(field)
                .write(controller, GcOperand::reference(&undefined, s), s, f);
        }
        undefined.clear(f);
    }
    pub(super) fn stream_clear_queue(
        &self,
        controller: &GcLocal<ReadableByteStreamControllerObject>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_HEAD)
            .write(controller, GcOperand::null(s), s, f);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_TAIL)
            .write(controller, GcOperand::null(s), s, f);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_BYTES)
            .write(controller, GcOperand::i64(0), s, f);
    }
    pub(super) fn stream_resolve_read(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        value: &ValueLocals,
        done: bool,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let realm = s.reserve_gc_local(f).initialize(
            s.field(PromiseObjectSchema::REALM)
                .read(promise, s, f)
                .reference(),
            f,
        );
        let done_local = s.reserve_i32_local(f);
        done_local.set_constant(i32::from(done), f);
        let object = self.emit_iterator_result_object_in_realm(&realm, value, done_local, f)?;
        let result = s.reserve_value_local(f);
        result.set_reference(&object, s, f);
        // Promise resolve, rather than direct fulfillment: an inherited `then`
        // on the fresh result remains observable through ordinary resolution.
        self.emit_resolve_promise_record(promise, &result, f)?;
        result.clear(f);
        object.clear(f);
        s.release_i32_local(done_local, f);
        realm.clear(f);
        Ok(())
    }
    pub(super) fn stream_shift_read(
        &self,
        reader: &GcLocal<ReadableStreamDefaultReaderObject>,
        f: &mut Function,
    ) -> GcLocal<PromiseObject> {
        let s = self.runtime_schema();
        let node = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::READ_HEAD)
                .read(reader, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let next = s.reserve_gc_local(f).initialize(
            s.field(StreamReadRequestSchema::NEXT)
                .read(&node, s, f)
                .reference(),
            f,
        );
        s.field(ReadableStreamDefaultReaderObjectSchema::READ_HEAD)
            .write(reader, GcOperand::reference(&next, s), s, f);
        next.load(s, f).is_null(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        s.field(ReadableStreamDefaultReaderObjectSchema::READ_TAIL)
            .write(reader, GcOperand::null(s), s, f);
        f.instruction(&Instruction::End);
        let promise = s.reserve_gc_local(f).initialize(
            s.field(StreamReadRequestSchema::PROMISE)
                .read(&node, s, f)
                .reference(),
            f,
        );
        next.clear(f);
        node.clear(f);
        promise
    }
    pub(super) fn stream_finish(
        &mut self,
        stream: &GcLocal<ReadableStreamObject>,
        error: Option<&ValueLocals>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.stream_is_state(stream, ReadableStreamState::Readable, f);
        self.open_frame(ControlFrameKind::If, f);
        let state = if error.is_some() {
            ReadableStreamState::Errored
        } else {
            ReadableStreamState::Closed
        };
        s.field(ReadableStreamObjectSchema::STATE)
            .write(stream, GcOperand::constant(state), s, f);
        let controller = self.stream_controller(stream, f);
        self.stream_clear_algorithms(&controller, f);
        if let Some(error) = error {
            let stored = s
                .reserve_gc_local(f)
                .initialize(s.struct_type::<StoredValue>().from_value(error, f), f);
            s.field(ReadableStreamObjectSchema::STORED_ERROR).write(
                stream,
                GcOperand::reference(&stored, s),
                s,
                f,
            );
            stored.clear(f);
            self.stream_clear_queue(&controller, f);
        }
        let reader = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamObjectSchema::READER)
                .read(stream, s, f)
                .reference(),
            f,
        );
        reader.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let closed = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::CLOSED)
                .read(&reader, s, f)
                .reference(),
            f,
        );
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        if let Some(error) = error {
            s.field(PromiseObjectSchema::HANDLED)
                .write(&closed, GcOperand::boolean(true), s, f);
            self.emit_settle_promise_record(&closed, PromiseSettlement::Reject, error, f)?;
        } else {
            self.emit_resolve_promise_record(&closed, &undefined, f)?;
        }
        // Detach the entire request list before resolving a result. Resolving
        // can synchronously observe Object.prototype.then and reenter releaseLock.
        let current = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::READ_HEAD)
                .read(&reader, s, f)
                .reference(),
            f,
        );
        s.field(ReadableStreamDefaultReaderObjectSchema::READ_HEAD)
            .write(&reader, GcOperand::null(s), s, f);
        s.field(ReadableStreamDefaultReaderObjectSchema::READ_TAIL)
            .write(&reader, GcOperand::null(s), s, f);
        let stop = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        current.load(s, f).is_null(f);
        self.emit_branch_if_to_target(stop, f);
        let node = s
            .reserve_gc_local(f)
            .initialize(current.load(s, f).require_non_null(f), f);
        let promise = s.reserve_gc_local(f).initialize(
            s.field(StreamReadRequestSchema::PROMISE)
                .read(&node, s, f)
                .reference(),
            f,
        );
        current.replace(
            s.field(StreamReadRequestSchema::NEXT)
                .read(&node, s, f)
                .reference(),
            f,
        );
        node.clear(f);
        if let Some(error) = error {
            self.emit_settle_promise_record(&promise, PromiseSettlement::Reject, error, f)?;
        } else {
            self.stream_resolve_read(&promise, &undefined, true, f)?;
        }
        promise.clear(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.stream_end_block(f);
        current.clear(f);
        undefined.clear(f);
        closed.clear(f);
        self.stream_end_if(f);
        reader.clear(f);
        controller.clear(f);
        self.stream_end_if(f);
        Ok(())
    }
    pub(super) fn stream_append_read(
        &self,
        reader: &GcLocal<ReadableStreamDefaultReaderObject>,
        promise: &GcLocal<PromiseObject>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let node = s.reserve_gc_local(f).initialize(
            s.struct_type::<StreamReadRequest>()
                .construct((GcOperand::reference(promise, s), GcOperand::null(s)), f),
            f,
        );
        let tail = s.reserve_gc_local(f).initialize(
            s.field(ReadableStreamDefaultReaderObjectSchema::READ_TAIL)
                .read(reader, s, f)
                .reference(),
            f,
        );
        tail.load(s, f).is_null(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        s.field(ReadableStreamDefaultReaderObjectSchema::READ_HEAD)
            .write(reader, GcOperand::nullable_reference(&node, s), s, f);
        f.instruction(&Instruction::Else);
        s.field(StreamReadRequestSchema::NEXT).write(
            &tail,
            GcOperand::nullable_reference(&node, s),
            s,
            f,
        );
        f.instruction(&Instruction::End);
        s.field(ReadableStreamDefaultReaderObjectSchema::READ_TAIL)
            .write(reader, GcOperand::nullable_reference(&node, s), s, f);
        tail.clear(f);
        node.clear(f);
    }
    pub(super) fn stream_append_chunk(
        &self,
        controller: &GcLocal<ReadableByteStreamControllerObject>,
        chunk: &GcLocal<TypedArrayObject>,
        length: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let node = s.reserve_gc_local(f).initialize(
            s.struct_type::<StreamByteChunk>().construct(
                (
                    GcOperand::reference(chunk, s),
                    GcOperand::i64_local(length),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        let tail = s.reserve_gc_local(f).initialize(
            s.field(ReadableByteStreamControllerObjectSchema::QUEUE_TAIL)
                .read(controller, s, f)
                .reference(),
            f,
        );
        tail.load(s, f).is_null(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_HEAD)
            .write(controller, GcOperand::nullable_reference(&node, s), s, f);
        f.instruction(&Instruction::Else);
        s.field(StreamByteChunkSchema::NEXT).write(
            &tail,
            GcOperand::nullable_reference(&node, s),
            s,
            f,
        );
        f.instruction(&Instruction::End);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_TAIL)
            .write(controller, GcOperand::nullable_reference(&node, s), s, f);
        let count = s.reserve_i64_local(f);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_BYTES)
            .read(controller, s, f)
            .store_i64(count, f);
        count.load(f);
        length.load(f);
        f.instruction(&Instruction::I64Add);
        count.store(f);
        s.field(ReadableByteStreamControllerObjectSchema::QUEUE_BYTES)
            .write(controller, GcOperand::i64_local(count), s, f);
        s.release_i64_local(count, f);
        tail.clear(f);
        node.clear(f);
    }
}
