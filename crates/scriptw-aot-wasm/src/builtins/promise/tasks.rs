//! Host tasks retain compiled callbacks in the ordinary Wasm GC heap. Timers
//! have a separate queue; Promise jobs and queueMicrotask share one FIFO.
use super::*;

impl FunctionBuilder<'_> {
    fn emit_check_task_callback(
        &mut self,
        callback: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_is_callable_i32(callback, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let pending = self.runtime_schema().reserve_completion(f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::HOST_TASK_CALLBACK_IS_NOT_CALLABLE,
            &pending,
            f,
        )?;
        self.completion().copy_from(&pending, f);
        self.emit_propagate_current_throw(f);
        pending.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_queue_microtask_builtin(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let callback = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &callback, f);
        self.emit_check_task_callback(&callback, f)?;
        self.emit_enqueue_microtask_callback(&callback, f)?;
        self.completion().initialize(f);
        callback.clear(f);
        Ok(())
    }

    pub(crate) fn emit_set_timeout_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let callback = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &callback, f);
        self.emit_check_task_callback(&callback, f)?;
        let timeout = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(1, &timeout, f);
        let number = s.reserve_completion(f);
        self.emit_value_to_number_payload(&timeout, &number, f)?;
        number.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.completion().copy_from(&number, f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let delay = s.reserve_i64_local(f);
        self.emit_to_uint32_i64_from_number_payload(number.value().scalar(), delay, f);
        // Web IDL long conversion, followed by HTML's nonnegative timeout.
        delay.load(f);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::I64ExtendI32S);
        delay.store(f);
        delay.load(f);
        f.instruction(&Instruction::I64Const(0));
        f.instruction(&Instruction::I64LtS);
        self.open_frame(ControlFrameKind::If, f);
        delay.set_constant(0, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let queue = s.load_timer_queue(f);
        let nesting = s.reserve_i64_local(f);
        s.field(TimerQueueSchema::NESTING)
            .read(&queue, s, f)
            .store_i64(nesting, f);
        nesting.load(f);
        f.instruction(&Instruction::I64Const(5));
        f.instruction(&Instruction::I64GtU);
        delay.load(f);
        f.instruction(&Instruction::I64Const(4));
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        delay.set_constant(4, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        nesting.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        nesting.store(f);
        let deadline = s.reserve_i64_local(f);
        f.instruction(&Instruction::Call(
            self.functions
                .monotonic_clock_nanos_import_function_index()
                .ok_or_else(|| EmitError::unsupported("setTimeout requires a monotonic clock"))?,
        ));
        deadline.store(f);
        delay.load(f);
        f.instruction(&Instruction::I64Const(1_000_000));
        f.instruction(&Instruction::I64Mul);
        delay.store(f);
        deadline.load(f);
        f.instruction(&Instruction::I64Const(i64::MAX));
        delay.load(f);
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        deadline.set_constant(i64::MAX, f);
        f.instruction(&Instruction::Else);
        deadline.load(f);
        delay.load(f);
        f.instruction(&Instruction::I64Add);
        deadline.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let id = s.reserve_i64_local(f);
        s.field(TimerQueueSchema::NEXT_ID)
            .read(&queue, s, f)
            .store_i64(id, f);
        id.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        let next_id = s.reserve_i64_local(f);
        next_id.store(f);
        s.field(TimerQueueSchema::NEXT_ID)
            .write(&queue, GcOperand::i64_local(next_id), s, f);
        // Standalone timer globals belong to their native defining Realm.
        // Ordinary Call still enters the callback's own defining Realm.
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&callback, f), f);
        let arguments = self.emit_builtin_argument_vector_tail(2, f);
        let head = s.reserve_gc_local(f).initialize(
            s.field(TimerQueueSchema::HEAD)
                .read(&queue, s, f)
                .reference(),
            f,
        );
        let task = s.reserve_gc_local(f).initialize(
            s.struct_type::<TimerTask>().construct(
                (
                    GcOperand::reference(&stored, s),
                    GcOperand::reference(&arguments, s),
                    GcOperand::reference(&realm, s),
                    GcOperand::i64_local(deadline),
                    GcOperand::i64_local(id),
                    GcOperand::i64_local(nesting),
                    GcOperand::boolean(false),
                    GcOperand::reference(&head, s),
                ),
                f,
            ),
            f,
        );
        s.field(TimerQueueSchema::HEAD).write(
            &queue,
            GcOperand::nullable_reference(&task, s),
            s,
            f,
        );
        id.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        let bits = s.reserve_i64_local(f);
        bits.store(f);
        self.completion().initialize(f);
        self.completion().value().set_number(bits, f);
        s.release_i64_local(bits, f);
        task.clear(f);
        head.clear(f);
        arguments.clear(f);
        stored.clear(f);
        realm.clear(f);
        s.release_i64_local(next_id, f);
        s.release_i64_local(id, f);
        s.release_i64_local(deadline, f);
        s.release_i64_local(nesting, f);
        queue.clear(f);
        s.release_i64_local(delay, f);
        number.clear(f);
        timeout.clear(f);
        callback.clear(f);
        Ok(())
    }

    pub(crate) fn emit_clear_timeout_builtin(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let input = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        let number = s.reserve_completion(f);
        self.emit_value_to_number_payload(&input, &number, f)?;
        number.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.completion().copy_from(&number, f);
        self.emit_propagate_current_throw(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let id = s.reserve_i64_local(f);
        number.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncSatF64U);
        id.store(f);
        let owner = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let queue = s.load_timer_queue(f);
        let current = s.reserve_gc_local(f).initialize(
            s.field(TimerQueueSchema::HEAD)
                .read(&queue, s, f)
                .reference(),
            f,
        );
        let done = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        current.load(s, f).is_null(f);
        self.emit_branch_if_to_target(done, f);
        let task = s
            .reserve_gc_local(f)
            .initialize(current.load(s, f).require_non_null(f), f);
        let _ = s.field(TimerTaskSchema::ID).read(&task, s, f);
        id.load(f);
        f.instruction(&Instruction::I64Eq);
        let task_owner = s.reserve_gc_local(f).initialize(
            s.field(TimerTaskSchema::OWNER_REALM)
                .read(&task, s, f)
                .reference(),
            f,
        );
        task_owner.load(s, f);
        owner.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        task_owner.clear(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(TimerTaskSchema::CANCELLED)
            .write(&task, GcOperand::boolean(true), s, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        current.replace(
            s.field(TimerTaskSchema::NEXT).read(&task, s, f).reference(),
            f,
        );
        task.clear(f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().initialize(f);
        current.clear(f);
        queue.clear(f);
        owner.clear(f);
        s.release_i64_local(id, f);
        number.clear(f);
        input.clear(f);
        Ok(())
    }

    fn emit_report_task_exception(
        &mut self,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let name = s.load_throw_diagnostic(ThrowDiagnosticRole::Name, f);
        let message = s.load_throw_diagnostic(ThrowDiagnosticRole::Message, f);
        self.functions
            .gc_host_import(GcHostImport::ReportCleanupException)
            .ok_or_else(|| {
                EmitError::unsupported("scheduled callbacks require exception reporting")
            })?
            .report_cleanup_exception(pending.value(), &name, &message, s, f);
        message.clear(f);
        name.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_clear_task_diagnostics(&self, f: &mut Function) {
        let s = self.runtime_schema();
        for role in [
            ThrowDiagnosticRole::Name,
            ThrowDiagnosticRole::Message,
            ThrowDiagnosticRole::ConstructorName,
        ] {
            s.clear_throw_diagnostic(role, f);
        }
    }

    /// Callback exceptions are reported directly, without synthetic Promises
    /// or rejection conversion; the remaining queued jobs continue.
    pub(super) fn emit_run_microtask_callback(
        &mut self,
        stored: &GcLocal<StoredValue>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let callback = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(stored, &callback, s, f);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let args = self
            .emit_argument_vector_from_snapshots(std::iter::empty::<&GcLocal<StoredValue>>(), f);
        let pending = s.reserve_completion(f);
        self.emit_clear_task_diagnostics(f);
        self.emit_function_or_proxy_call_with_argv(&callback, &undefined, &args, &pending, f)?;
        self.emit_report_task_exception(&pending, f)?;
        self.completion().initialize(f);
        pending.clear(f);
        args.clear(f);
        undefined.clear(f);
        callback.clear(f);
        Ok(())
    }

    /// Polling never sleeps. The standalone drain waits through the existing
    /// deadline-aware host sleep import, then takes a microtask checkpoint after
    /// each task. Equal deadlines retain insertion order.
    pub(crate) fn emit_timer_checkpoint(
        &mut self,
        wait: bool,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let clock = self
            .functions
            .monotonic_clock_nanos_import_function_index()
            .ok_or_else(|| EmitError::unsupported("timer checkpoint requires a monotonic clock"))?;
        let existing = s.load_existing_timer_queue(f);
        existing.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let queue = s
            .reserve_gc_local(f)
            .initialize(existing.load(s, f).require_non_null(f), f);
        let saved = s.reserve_completion(f);
        saved.copy_from(self.completion(), f);
        let saved_realm = self.load_current_realm(f);
        let saved_name = s.load_throw_diagnostic(ThrowDiagnosticRole::Name, f);
        let saved_message = s.load_throw_diagnostic(ThrowDiagnosticRole::Message, f);
        let saved_constructor = s.load_throw_diagnostic(ThrowDiagnosticRole::ConstructorName, f);
        let current = s
            .reserve_gc_local::<TimerTask, Nullable>(f)
            .initialize_null(s, f);
        let previous = s
            .reserve_gc_local::<TimerTask, Nullable>(f)
            .initialize_null(s, f);
        let selected = s
            .reserve_gc_local::<TimerTask, Nullable>(f)
            .initialize_null(s, f);
        let selected_previous = s
            .reserve_gc_local::<TimerTask, Nullable>(f)
            .initialize_null(s, f);
        let nearest = s.reserve_i64_local(f);
        let deadline = s.reserve_i64_local(f);
        let now = s.reserve_i64_local(f);
        let nesting = s.reserve_i64_local(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        // WaitAsync and timers share the monotonic clock. Do not let a later
        // timer postpone an earlier waiter or its cancellation microtasks.
        if self.functions.agent_call_import_function_index().is_some() {
            self.emit_poll_atomics_wait_async_timeouts(f)?;
            self.emit_drain_promise_jobs(f)?;
        }
        f.instruction(&Instruction::Call(clock));
        now.store(f);
        nearest.set_constant(i64::MAX, f);
        selected.set_null(s, f);
        selected_previous.set_null(s, f);
        previous.set_null(s, f);
        current.replace(
            s.field(TimerQueueSchema::HEAD)
                .read(&queue, s, f)
                .reference(),
            f,
        );
        let scanned = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        current.load(s, f).is_null(f);
        self.emit_branch_if_to_target(scanned, f);
        let task = s
            .reserve_gc_local(f)
            .initialize(current.load(s, f).require_non_null(f), f);
        s.field(TimerTaskSchema::DEADLINE)
            .read(&task, s, f)
            .store_i64(deadline, f);
        let _ = s.field(TimerTaskSchema::CANCELLED).read(&task, s, f);
        self.open_frame(ControlFrameKind::If, f);
        deadline.set_constant(0, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        deadline.load(f);
        nearest.load(f);
        f.instruction(&Instruction::I64LeU);
        self.open_frame(ControlFrameKind::If, f);
        deadline.load(f);
        nearest.store(f);
        selected.replace(current.load(s, f), f);
        selected_previous.replace(previous.load(s, f), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        previous.replace(current.load(s, f), f);
        current.replace(
            s.field(TimerTaskSchema::NEXT).read(&task, s, f).reference(),
            f,
        );
        task.clear(f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        selected.load(s, f).is_null(f);
        self.emit_branch_if_to_target(done, f);
        nearest.load(f);
        now.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        if wait {
            nearest.load(f);
            now.load(f);
            f.instruction(&Instruction::I64Sub);
            f.instruction(&Instruction::Call(
                self.functions
                    .sleep_nanos_import_function_index()
                    .ok_or_else(|| EmitError::unsupported("timer drain requires host sleep"))?,
            ));
            self.emit_branch_to_target(again, f);
        } else {
            self.emit_branch_to_target(done, f);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let task = s
            .reserve_gc_local(f)
            .initialize(selected.load(s, f).require_non_null(f), f);
        let next = s.reserve_gc_local(f).initialize(
            s.field(TimerTaskSchema::NEXT).read(&task, s, f).reference(),
            f,
        );
        selected_previous.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(TimerQueueSchema::HEAD)
            .write(&queue, GcOperand::reference(&next, s), s, f);
        f.instruction(&Instruction::Else);
        let prev = s
            .reserve_gc_local(f)
            .initialize(selected_previous.load(s, f).require_non_null(f), f);
        s.field(TimerTaskSchema::NEXT)
            .write(&prev, GcOperand::reference(&next, s), s, f);
        prev.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let _ = s.field(TimerTaskSchema::CANCELLED).read(&task, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let realm = s.reserve_gc_local(f).initialize(
            s.field(TimerTaskSchema::OWNER_REALM)
                .read(&task, s, f)
                .reference(),
            f,
        );
        self.replace_current_realm(&realm, f);
        s.field(TimerTaskSchema::NESTING)
            .read(&task, s, f)
            .store_i64(nesting, f);
        s.field(TimerQueueSchema::NESTING)
            .write(&queue, GcOperand::i64_local(nesting), s, f);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(TimerTaskSchema::CALLBACK)
                .read(&task, s, f)
                .reference(),
            f,
        );
        let callback = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &callback, s, f);
        let args = s.reserve_gc_local(f).initialize(
            s.field(TimerTaskSchema::ARGUMENTS)
                .read(&task, s, f)
                .reference(),
            f,
        );
        let this = s.reserve_value_local(f);
        let global_this = s.reserve_gc_local(f).initialize(
            s.field(RealmRecordSchema::GLOBAL_THIS)
                .read(&realm, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&global_this, &this, s, f);
        global_this.clear(f);
        let pending = s.reserve_completion(f);
        self.emit_clear_task_diagnostics(f);
        self.emit_function_or_proxy_call_with_argv(&callback, &this, &args, &pending, f)?;
        self.emit_report_task_exception(&pending, f)?;
        self.completion().initialize(f);
        self.emit_finish_weak_job(f)?;
        self.completion().initialize(f);
        self.emit_drain_promise_jobs(f)?;
        pending.clear(f);
        this.clear(f);
        args.clear(f);
        callback.clear(f);
        stored.clear(f);
        realm.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        task.clear(f);
        next.clear(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.field(TimerQueueSchema::NESTING)
            .write(&queue, GcOperand::i64(0), s, f);
        self.replace_current_realm(&saved_realm, f);
        self.completion().copy_from(&saved, f);
        s.replace_throw_diagnostic(ThrowDiagnosticRole::Name, &saved_name, f);
        s.replace_throw_diagnostic(ThrowDiagnosticRole::Message, &saved_message, f);
        s.replace_throw_diagnostic(ThrowDiagnosticRole::ConstructorName, &saved_constructor, f);
        saved_constructor.clear(f);
        saved_message.clear(f);
        saved_name.clear(f);
        s.release_i64_local(nesting, f);
        s.release_i64_local(now, f);
        s.release_i64_local(deadline, f);
        s.release_i64_local(nearest, f);
        selected_previous.clear(f);
        selected.clear(f);
        previous.clear(f);
        current.clear(f);
        saved_realm.clear(f);
        saved.clear(f);
        queue.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        existing.clear(f);
        Ok(())
    }
}
