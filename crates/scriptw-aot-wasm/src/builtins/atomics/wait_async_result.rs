use super::*;

impl FunctionBuilder<'_> {
    pub(super) fn emit_atomics_wait_async_object(
        &mut self,
        asynchronous: bool,
        value: &ValueLocals,
        out: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let prototype = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            f,
        );
        let object = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), f)?,
            f,
        );
        let key = self.emit_binary_string_key("async", f)?;
        let flag = s.reserve_value_local(f);
        let boolean = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(i32::from(asynchronous)));
        boolean.store(f);
        flag.set_boolean(boolean, f);
        self.emit_object_append_data_property_with_flags(
            &object, &key, &flag, true, true, true, f,
        )?;
        key.clear(f);
        let key = self.emit_binary_string_key("value", f)?;
        self.emit_object_append_data_property_with_flags(
            &object, &key, value, true, true, true, f,
        )?;
        key.clear(f);
        out.value().set_reference(&object, s, f);
        s.release_i32_local(boolean, f);
        flag.clear(f);
        object.clear(f);
        prototype.clear(f);
        realm.clear(f);
        Ok(())
    }
    pub(super) fn emit_atomics_wait_async_promise(
        &mut self,
        buffer: &GcLocal<SharedArrayBuffer>,
        offset: I64Local,
        deadline: I64Local,
        host_id: I64Local,
        out: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let context = self.emit_current_function_realm_intrinsic_promise_allocation_context(f);
        let promise = self.emit_alloc_promise_with_prototype(context, f)?;
        let waiter = s.reserve_gc_local(f).initialize(
            s.struct_type::<AtomicsAsyncWaiter>().construct(
                (
                    GcOperand::boolean(true),
                    GcOperand::reference(buffer, s),
                    GcOperand::i64_local(offset),
                    GcOperand::reference(&promise, s),
                    GcOperand::i64_local(deadline),
                    GcOperand::i64_local(host_id),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        let tail = s.load_atomics_async_waiter_queue(RuntimeQueueEnd::Tail, f);
        tail.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        s.replace_atomics_async_waiter_queue(RuntimeQueueEnd::Head, &waiter, f);
        f.instruction(&Instruction::Else);
        s.field(AtomicsAsyncWaiterSchema::NEXT).write(
            &tail,
            GcOperand::nullable_reference(&waiter, s),
            s,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.replace_atomics_async_waiter_queue(RuntimeQueueEnd::Tail, &waiter, f);
        let value = s.reserve_value_local(f);
        value.set_reference(&promise, s, f);
        self.emit_atomics_wait_async_object(true, &value, out, f)?;
        value.clear(f);
        tail.clear(f);
        waiter.clear(f);
        promise.clear(f);
        Ok(())
    }
    pub(crate) fn emit_drain_atomics_wait_async_timeouts(
        &mut self,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        self.emit_atomics_wait_async_timeout_checkpoint(
            AtomicsWaitAsyncTimeoutCheckpointMode::Drain,
            f,
        )
    }
    pub(crate) fn emit_poll_atomics_wait_async_timeouts(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let progressed = self.emit_atomics_wait_async_timeout_checkpoint(
            AtomicsWaitAsyncTimeoutCheckpointMode::Poll,
            f,
        )?;
        self.runtime_schema().release_i32_local(progressed, f);
        Ok(())
    }
    fn emit_atomics_wait_async_timeout_checkpoint(
        &mut self,
        mode: AtomicsWaitAsyncTimeoutCheckpointMode,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let saved = s.reserve_completion(f);
        saved.copy_from(self.completion(), f);
        let progressed = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        progressed.store(f);
        let outcome = s.reserve_value_local(f);
        let host_id = s.reserve_i64_local(f);
        let status = s.reserve_i64_local(f);
        let deadline = s.reserve_i64_local(f);
        let now = s.reserve_i64_local(f);
        let active = s.reserve_i32_local(f);
        let settle = s.reserve_i32_local(f);
        let any_active = s.reserve_i32_local(f);
        let nearest = s.reserve_i64_local(f);
        let delay = s.reserve_i64_local(f);
        let current = s
            .reserve_gc_local::<AtomicsAsyncWaiter, Nullable>(f)
            .initialize_null(s, f);
        let previous = s
            .reserve_gc_local::<AtomicsAsyncWaiter, Nullable>(f)
            .initialize_null(s, f);
        let next = s
            .reserve_gc_local::<AtomicsAsyncWaiter, Nullable>(f)
            .initialize_null(s, f);
        let agent = self
            .functions
            .agent_call_import_function_index()
            .ok_or_else(|| {
                EmitError::unsupported("waitAsync polling requires its native agent import")
            })?;
        let clock = self
            .functions
            .monotonic_clock_nanos_import_function_index()
            .ok_or_else(|| {
                EmitError::unsupported("waitAsync polling requires its monotonic clock")
            })?;
        let done = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        let head = s.load_atomics_async_waiter_queue(RuntimeQueueEnd::Head, f);
        current.replace(head.load(s, f), f);
        head.clear(f);
        current.load(s, f).is_null(f);
        self.emit_branch_if_to_target(done, f);
        f.instruction(&Instruction::Call(clock));
        now.store(f);
        f.instruction(&Instruction::I64Const(i64::MAX));
        nearest.store(f);
        f.instruction(&Instruction::I32Const(0));
        any_active.store(f);
        previous.set_null(s, f);
        let scanned = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        current.load(s, f).is_null(f);
        self.emit_branch_if_to_target(scanned, f);
        next.replace(
            s.field(AtomicsAsyncWaiterSchema::NEXT)
                .read(&current, s, f)
                .reference(),
            f,
        );
        s.field(AtomicsAsyncWaiterSchema::HOST_WAITER_ID)
            .read(&current, s, f)
            .store_i64(host_id, f);
        s.field(AtomicsAsyncWaiterSchema::DEADLINE_NANOS)
            .read(&current, s, f)
            .store_i64(deadline, f);
        s.field(AtomicsAsyncWaiterSchema::ACTIVE)
            .read(&current, s, f)
            .store(active, f);
        f.instruction(&Instruction::I32Const(0));
        settle.store(f);
        active.load(f);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I64Const(
            AgentHostOperation::PollAsyncWaiter.wire(),
        ));
        host_id.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::Call(agent));
        status.store(f);
        status.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_atomics_wait_outcome(AtomicsWaitOutcome::Ok, &outcome, f)?;
        f.instruction(&Instruction::I32Const(1));
        settle.store(f);
        f.instruction(&Instruction::Else);
        status.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        deadline.load(f);
        f.instruction(&Instruction::I64Const(i64::MAX));
        f.instruction(&Instruction::I64Ne);
        deadline.load(f);
        now.load(f);
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        // Cancellation arbitrates a notify racing the timeout: status 1 still wins.
        f.instruction(&Instruction::I64Const(
            AgentHostOperation::CancelAsyncWaiter.wire(),
        ));
        host_id.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::Call(agent));
        status.store(f);
        status.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        status.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_atomics_wait_outcome(AtomicsWaitOutcome::Ok, &outcome, f)?;
        f.instruction(&Instruction::Else);
        self.emit_atomics_wait_outcome(AtomicsWaitOutcome::TimedOut, &outcome, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(1));
        settle.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        settle.load(f);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(1));
        progressed.store(f);
        s.field(AtomicsAsyncWaiterSchema::ACTIVE)
            .write(&current, GcOperand::boolean(false), s, f);
        let promise = s.reserve_gc_local(f).initialize(
            s.field(AtomicsAsyncWaiterSchema::PROMISE)
                .read(&current, s, f)
                .reference(),
            f,
        );
        self.emit_resolve_promise_record(&promise, &outcome, f)?;
        promise.clear(f);
        previous.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        s.replace_atomics_async_waiter_queue(RuntimeQueueEnd::Head, &next, f);
        f.instruction(&Instruction::Else);
        s.field(AtomicsAsyncWaiterSchema::NEXT).write(
            &previous,
            GcOperand::reference(&next, s),
            s,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        next.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        s.replace_atomics_async_waiter_queue(RuntimeQueueEnd::Tail, &previous, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.field(AtomicsAsyncWaiterSchema::NEXT)
            .write(&current, GcOperand::null(s), s, f);
        f.instruction(&Instruction::Else);
        previous.replace(current.load(s, f), f);
        f.instruction(&Instruction::I32Const(1));
        any_active.store(f);
        deadline.load(f);
        nearest.load(f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        deadline.load(f);
        nearest.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        current.replace(next.load(s, f), f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        match mode {
            AtomicsWaitAsyncTimeoutCheckpointMode::Poll => self.emit_branch_to_target(done, f),
            AtomicsWaitAsyncTimeoutCheckpointMode::Drain => {
                // Reactions from a settled waiter can notify another waiter.
                // Give the actual Promise queue its turn before waiting again.
                progressed.load(f);
                self.emit_branch_if_to_target(done, f);
                any_active.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.emit_branch_if_to_target(done, f);
                f.instruction(&Instruction::I64Const(1));
                delay.store(f);
                nearest.load(f);
                f.instruction(&Instruction::I64Const(i64::MAX));
                f.instruction(&Instruction::I64Ne);
                nearest.load(f);
                now.load(f);
                f.instruction(&Instruction::I64GtU);
                f.instruction(&Instruction::I32And);
                self.open_frame(ControlFrameKind::If, f);
                nearest.load(f);
                now.load(f);
                f.instruction(&Instruction::I64Sub);
                f.instruction(&Instruction::I64Const(999_999));
                f.instruction(&Instruction::I64Add);
                f.instruction(&Instruction::I64Const(1_000_000));
                f.instruction(&Instruction::I64DivU);
                f.instruction(&Instruction::I64Const(1));
                f.instruction(&Instruction::I64LtU);
                self.open_frame(ControlFrameKind::If, f);
                f.instruction(&Instruction::I64Const(0));
                delay.store(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                f.instruction(&Instruction::I64Const(AgentHostOperation::Sleep.wire()));
                delay.load(f);
                f.instruction(&Instruction::F64ConvertI64U);
                f.instruction(&Instruction::I64ReinterpretF64);
                f.instruction(&Instruction::I64Const(0));
                f.instruction(&Instruction::Call(agent));
                f.instruction(&Instruction::Drop);
                self.emit_branch_to_target(again, f);
            }
        }
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&saved, f);
        next.clear(f);
        previous.clear(f);
        current.clear(f);
        s.release_i64_local(delay, f);
        s.release_i64_local(nearest, f);
        s.release_i32_local(any_active, f);
        s.release_i32_local(settle, f);
        s.release_i32_local(active, f);
        s.release_i64_local(now, f);
        s.release_i64_local(deadline, f);
        s.release_i64_local(status, f);
        s.release_i64_local(host_id, f);
        outcome.clear(f);
        saved.clear(f);
        Ok(progressed)
    }
}
