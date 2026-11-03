use super::*;

impl FunctionBuilder<'_> {
    fn emit_web_append_listener(
        &mut self,
        target: &GcLocal<EventTargetObject>,
        listener: &GcLocal<WebEventListener>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let tail = s.reserve_gc_local(f).initialize(
            s.field(EventTargetObjectSchema::TAIL)
                .read(target, s, f)
                .reference(),
            f,
        );
        tail.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(EventTargetObjectSchema::HEAD).write(
            target,
            GcOperand::nullable_reference(listener, s),
            s,
            f,
        );
        f.instruction(&Instruction::Else);
        s.field(WebEventListenerSchema::NEXT).write(
            &tail,
            GcOperand::nullable_reference(listener, s),
            s,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.field(EventTargetObjectSchema::TAIL).write(
            target,
            GcOperand::nullable_reference(listener, s),
            s,
            f,
        );
        tail.clear(f);
    }

    fn emit_web_new_listener(
        &mut self,
        kind: &GcLocal<StringValue>,
        callback: &ValueLocals,
        capture: I32Local,
        once: I32Local,
        passive: I32Local,
        signal: &GcLocal<AbortSignalObject, Nullable>,
        f: &mut Function,
    ) -> GcLocal<WebEventListener> {
        let s = self.runtime_schema();
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(callback, f), f);
        let result = s.reserve_gc_local(f).initialize(
            s.struct_type::<WebEventListener>().construct(
                (
                    GcOperand::reference(kind, s),
                    GcOperand::reference(&stored, s),
                    GcOperand::boolean_local(capture),
                    GcOperand::boolean_local(once),
                    GcOperand::boolean_local(passive),
                    GcOperand::boolean(false),
                    GcOperand::reference(signal, s),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        stored.clear(f);
        result
    }

    /// Removing a listener releases its callback and unlinks the list node.
    /// Dispatch snapshots retain the actual node and observe REMOVED afterward.
    fn emit_web_remove_listener(
        &mut self,
        target: &GcLocal<EventTargetObject>,
        selected: &GcLocal<WebEventListener>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        self.emit_web_unregister_abort_listener(selected, f);
        s.field(WebEventListenerSchema::REMOVED)
            .write(selected, GcOperand::boolean(true), s, f);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&undefined, f), f);
        s.field(WebEventListenerSchema::CALLBACK).write(
            selected,
            GcOperand::reference(&stored, s),
            s,
            f,
        );
        stored.clear(f);
        undefined.clear(f);
        let current = s.reserve_gc_local(f).initialize(
            s.field(EventTargetObjectSchema::HEAD)
                .read(target, s, f)
                .reference(),
            f,
        );
        let previous = s
            .reserve_gc_local::<WebEventListener, Nullable>(f)
            .initialize_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        current.load(s, f).is_null(f);
        self.emit_branch_if_to_target(done, f);
        current.load(s, f);
        selected.load(s, f);
        f.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, f);
        let following = s.reserve_gc_local(f).initialize(
            s.field(WebEventListenerSchema::NEXT)
                .read(&current, s, f)
                .reference(),
            f,
        );
        previous.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(EventTargetObjectSchema::HEAD).write(
            target,
            GcOperand::reference(&following, s),
            s,
            f,
        );
        f.instruction(&Instruction::Else);
        s.field(WebEventListenerSchema::NEXT).write(
            &previous,
            GcOperand::reference(&following, s),
            s,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        following.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(EventTargetObjectSchema::TAIL).write(
            target,
            GcOperand::reference(&previous, s),
            s,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        following.clear(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        previous.replace(current.load(s, f), f);
        current.replace(
            s.field(WebEventListenerSchema::NEXT)
                .read(&current, s, f)
                .reference(),
            f,
        );
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        previous.clear(f);
        current.clear(f);
    }

    fn emit_web_listener_signal_aborted(
        &mut self,
        listener: &GcLocal<WebEventListener>,
        aborted: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        f.instruction(&Instruction::I32Const(0));
        aborted.store(f);
        let signal = s.reserve_gc_local(f).initialize(
            s.field(WebEventListenerSchema::SIGNAL)
                .read(listener, s, f)
                .reference(),
            f,
        );
        signal.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.field(AbortSignalObjectSchema::ABORTED)
            .read(&signal, s, f)
            .store(aborted, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        signal.clear(f);
    }

    fn emit_web_find_listener(
        &mut self,
        target: &GcLocal<EventTargetObject>,
        kind: &GcLocal<StringValue>,
        callback: &ValueLocals,
        capture: I32Local,
        f: &mut Function,
    ) -> Result<GcLocal<WebEventListener, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let current = s.reserve_gc_local(f).initialize(
            s.field(EventTargetObjectSchema::HEAD)
                .read(target, s, f)
                .reference(),
            f,
        );
        let found = s
            .reserve_gc_local::<WebEventListener, Nullable>(f)
            .initialize_null(s, f);
        let value = s.reserve_value_local(f);
        let equal = s.reserve_i32_local(f);
        let flag = s.reserve_i32_local(f);
        let folding = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        folding.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        current.load(s, f).is_null(f);
        self.emit_branch_if_to_target(done, f);
        let listener = s
            .reserve_gc_local(f)
            .initialize(current.load(s, f).require_non_null(f), f);
        let following = s.reserve_gc_local(f).initialize(
            s.field(WebEventListenerSchema::NEXT)
                .read(&listener, s, f)
                .reference(),
            f,
        );
        let skipped = self.open_frame(ControlFrameKind::Block, f);
        self.emit_web_listener_signal_aborted(&listener, flag, f);
        flag.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_web_remove_listener(target, &listener, f);
        self.emit_branch_to_target(skipped, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.field(WebEventListenerSchema::CAPTURE)
            .read(&listener, s, f)
            .store(flag, f);
        flag.load(f);
        capture.load(f);
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(skipped, f);
        let actual_kind = s.reserve_gc_local(f).initialize(
            s.field(WebEventListenerSchema::TYPE)
                .read(&listener, s, f)
                .reference(),
            f,
        );
        self.emit_gc_string_equality(&actual_kind, kind, folding, equal, f);
        actual_kind.clear(f);
        equal.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(skipped, f);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(WebEventListenerSchema::CALLBACK)
                .read(&listener, s, f)
                .reference(),
            f,
        );
        self.emit_stored_value_to_locals(&stored, &value, f);
        stored.clear(f);
        self.emit_tagged_payload_same_value_i32(&value, callback, f)?;
        self.open_frame(ControlFrameKind::If, f);
        found.replace(listener.load(s, f).nullable(), f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        current.replace(following.load(s, f), f);
        following.clear(f);
        listener.clear(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(folding, f);
        s.release_i32_local(flag, f);
        s.release_i32_local(equal, f);
        value.clear(f);
        current.clear(f);
        Ok(found)
    }

    pub(super) fn emit_web_listener_builtin(
        &mut self,
        builtin: StandardBuiltinId,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        match builtin {
            StandardBuiltinId::EventTargetPrototypeAddEventListener => {
                self.emit_web_listener_registration(false, output, exit, f)
            }
            StandardBuiltinId::EventTargetPrototypeRemoveEventListener => {
                self.emit_web_listener_registration(true, output, exit, f)
            }
            StandardBuiltinId::EventTargetPrototypeDispatchEvent => {
                let s = self.runtime_schema();
                let this = s.reserve_value_local(f);
                self.compile_this_to_locals(&this, f)?;
                let target = self.emit_web_target(&this, output, exit, f)?;
                self.emit_web_required_arguments(1, output, exit, f)?;
                let argument = s.reserve_value_local(f);
                self.emit_builtin_arg_to_value(0, &argument, f);
                let event = self.emit_web_brand::<WebEventObject>(&argument, output, exit, f)?;
                s.field(WebEventObjectSchema::DISPATCHING)
                    .read(&event, s, f);
                self.emit_web_invalid_dispatch_if(output, exit, f)?;
                s.field(WebEventObjectSchema::TRUSTED).write(
                    &event,
                    GcOperand::boolean(false),
                    s,
                    f,
                );
                self.emit_web_dispatch_event(&target, &this, &event, output, exit, f)?;
                let canceled = s.reserve_i32_local(f);
                s.field(WebEventObjectSchema::CANCELED)
                    .read(&event, s, f)
                    .store(canceled, f);
                canceled.load(f);
                f.instruction(&Instruction::I32Eqz);
                canceled.store(f);
                output.value().set_boolean(canceled, f);
                output.set_kind(CompletionKind::Normal, f);
                s.release_i32_local(canceled, f);
                event.clear(f);
                argument.clear(f);
                target.clear(f);
                this.clear(f);
                Ok(())
            }
            StandardBuiltinId::AbortSignalPrototypeOnabortGetter
            | StandardBuiltinId::AbortSignalPrototypeOnabortSetter => self.emit_web_onabort(
                builtin == StandardBuiltinId::AbortSignalPrototypeOnabortSetter,
                output,
                exit,
                f,
            ),
            _ => unreachable!("closed Web listener entry"),
        }
    }

    fn emit_web_listener_registration(
        &mut self,
        remove: bool,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let this = s.reserve_value_local(f);
        self.compile_this_to_locals(&this, f)?;
        let target = self.emit_web_target(&this, output, exit, f)?;
        self.emit_web_required_arguments(2, output, exit, f)?;
        let argument = s.reserve_value_local(f);
        let callback = s.reserve_value_local(f);
        let options = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        self.emit_builtin_arg_to_value(0, &argument, f);
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        self.emit_web_abrupt(&pending, output, exit, f);
        let kind = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        self.emit_builtin_arg_to_value(1, &callback, f);
        self.emit_is_heap_object_like_tag_i32(callback.tag(), f);
        self.compile_nullish_tagged_i32(callback.tag(), f)?;
        f.instruction(&Instruction::I32Or);
        f.instruction(&Instruction::I32Eqz);
        self.emit_web_type_error_if(
            RuntimeErrorMessage::WEB_EVENT_INVALID_CALLBACK,
            output,
            exit,
            f,
        )?;
        self.emit_builtin_arg_to_value(2, &options, f);
        let capture = s.reserve_i32_local(f);
        let once = s.reserve_i32_local(f);
        let passive = s.reserve_i32_local(f);
        for local in [capture, once, passive] {
            f.instruction(&Instruction::I32Const(0));
            local.store(f);
        }
        let signal = s
            .reserve_gc_local::<AbortSignalObject, Nullable>(f)
            .initialize_null(s, f);
        self.emit_is_heap_object_like_tag_i32(options.tag(), f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_web_get(&options, "capture", &pending, f)?;
        self.emit_web_abrupt(&pending, output, exit, f);
        self.emit_to_boolean_payload_from_tagged_locals(pending.value(), f)?;
        f.instruction(&Instruction::I32WrapI64);
        capture.store(f);
        if !remove {
            for (name, local) in [("once", once), ("passive", passive)] {
                self.emit_web_get(&options, name, &pending, f)?;
                self.emit_web_abrupt(&pending, output, exit, f);
                self.emit_to_boolean_payload_from_tagged_locals(pending.value(), f)?;
                f.instruction(&Instruction::I32WrapI64);
                local.store(f);
            }
            self.emit_web_get(&options, "signal", &pending, f)?;
            self.emit_web_abrupt(&pending, output, exit, f);
            pending.value().tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            f.instruction(&Instruction::I32Ne);
            self.open_frame(ControlFrameKind::If, f);
            let selected =
                self.emit_web_brand::<AbortSignalObject>(pending.value(), output, exit, f)?;
            signal.replace(selected.load(s, f).nullable(), f);
            selected.clear(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::Else);
        self.emit_to_boolean_payload_from_tagged_locals(&options, f)?;
        f.instruction(&Instruction::I32WrapI64);
        capture.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        output.value().set_undefined(f);
        output.set_kind(CompletionKind::Normal, f);
        self.compile_nullish_tagged_i32(callback.tag(), f)?;
        self.emit_branch_if_to_target(exit, f);
        let found = self.emit_web_find_listener(&target, &kind, &callback, capture, f)?;
        if remove {
            found.load(s, f).is_null(f);
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            let selected = s
                .reserve_gc_local(f)
                .initialize(found.load(s, f).require_non_null(f), f);
            self.emit_web_remove_listener(&target, &selected, f);
            selected.clear(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        } else {
            found.load(s, f).is_null(f);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(exit, f);
            signal.load(s, f).is_null(f);
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            let aborted = s.reserve_i32_local(f);
            s.field(AbortSignalObjectSchema::ABORTED)
                .read(&signal, s, f)
                .store(aborted, f);
            aborted.load(f);
            self.emit_branch_if_to_target(exit, f);
            s.release_i32_local(aborted, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            let listener =
                self.emit_web_new_listener(&kind, &callback, capture, once, passive, &signal, f);
            self.emit_web_append_listener(&target, &listener, f);
            self.emit_web_register_abort_listener(&target, &listener, &signal, f);
            listener.clear(f);
        }
        found.clear(f);
        signal.clear(f);
        kind.clear(f);
        for local in [passive, once, capture] {
            s.release_i32_local(local, f);
        }
        pending.clear(f);
        options.clear(f);
        callback.clear(f);
        argument.clear(f);
        target.clear(f);
        this.clear(f);
        Ok(())
    }

    fn emit_web_onabort(
        &mut self,
        setter: bool,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let signal = self.emit_web_receiver::<AbortSignalObject>(output, exit, f)?;
        let target = s.reserve_gc_local(f).initialize(
            s.field(AbortSignalObjectSchema::TARGET)
                .read(&signal, s, f)
                .reference(),
            f,
        );
        let listener = s.reserve_gc_local(f).initialize(
            s.field(EventTargetObjectSchema::ONABORT)
                .read(&target, s, f)
                .reference(),
            f,
        );
        let callback = s.reserve_value_local(f);
        if setter {
            self.emit_builtin_arg_to_value(0, &callback, f);
            self.emit_is_callable_i32(&callback, f)?;
            self.open_frame(ControlFrameKind::If, f);
            listener.load(s, f).is_null(f);
            self.open_frame(ControlFrameKind::If, f);
            let kind = s
                .reserve_gc_local(f)
                .initialize(self.emit_interned_string_reference("abort", f)?, f);
            let zero = s.reserve_i32_local(f);
            f.instruction(&Instruction::I32Const(0));
            zero.store(f);
            let none = s
                .reserve_gc_local::<AbortSignalObject, Nullable>(f)
                .initialize_null(s, f);
            let added = self.emit_web_new_listener(&kind, &callback, zero, zero, zero, &none, f);
            self.emit_web_append_listener(&target, &added, f);
            s.field(EventTargetObjectSchema::ONABORT).write(
                &target,
                GcOperand::nullable_reference(&added, s),
                s,
                f,
            );
            added.clear(f);
            none.clear(f);
            s.release_i32_local(zero, f);
            kind.clear(f);
            f.instruction(&Instruction::Else);
            let stored = s
                .reserve_gc_local(f)
                .initialize(s.struct_type::<StoredValue>().from_value(&callback, f), f);
            s.field(WebEventListenerSchema::CALLBACK).write(
                &listener,
                GcOperand::reference(&stored, s),
                s,
                f,
            );
            stored.clear(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::Else);
            listener.load(s, f).is_null(f);
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            let selected = s
                .reserve_gc_local(f)
                .initialize(listener.load(s, f).require_non_null(f), f);
            self.emit_web_remove_listener(&target, &selected, f);
            selected.clear(f);
            s.field(EventTargetObjectSchema::ONABORT)
                .write(&target, GcOperand::null(s), s, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            output.value().set_undefined(f);
        } else {
            output.value().set_scalar(ScalarValue::Null, f);
            listener.load(s, f).is_null(f);
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            let stored = s.reserve_gc_local(f).initialize(
                s.field(WebEventListenerSchema::CALLBACK)
                    .read(&listener, s, f)
                    .reference(),
                f,
            );
            self.emit_stored_value_to_locals(&stored, output.value(), f);
            stored.clear(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        output.set_kind(CompletionKind::Normal, f);
        callback.clear(f);
        listener.clear(f);
        target.clear(f);
        signal.clear(f);
        Ok(())
    }

    fn emit_web_listener_snapshot(
        &mut self,
        target: &GcLocal<EventTargetObject>,
        f: &mut Function,
    ) -> GcLocal<WebEventListenerTable> {
        let s = self.runtime_schema();
        let current = s.reserve_gc_local(f).initialize(
            s.field(EventTargetObjectSchema::HEAD)
                .read(target, s, f)
                .reference(),
            f,
        );
        let count = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        count.store(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        current.load(s, f).is_null(f);
        self.emit_branch_if_to_target(done, f);
        count.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        count.store(f);
        current.replace(
            s.field(WebEventListenerSchema::NEXT)
                .read(&current, s, f)
                .reference(),
            f,
        );
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        current.replace(
            s.field(EventTargetObjectSchema::HEAD)
                .read(target, s, f)
                .reference(),
            f,
        );
        let result = s.reserve_gc_local(f).initialize(
            s.array_type::<WebEventListenerTable>()
                .filled(GcOperand::null(s), count, f),
            f,
        );
        let index = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        index.store(f);
        let copied = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(copied, f);
        s.array_type::<WebEventListenerTable>().write(
            &result,
            index,
            GcOperand::reference(&current, s),
            s,
            f,
        );
        current.replace(
            s.field(WebEventListenerSchema::NEXT)
                .read(&current, s, f)
                .reference(),
            f,
        );
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(index, f);
        s.release_i32_local(count, f);
        current.clear(f);
        result
    }

    fn emit_web_report_listener_exception(
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
            .gc_host_imports()
            .get(GcHostImport::ReportCleanupException)
            .ok_or_else(|| {
                EmitError::unsupported("Web listener exception reporting import is absent")
            })?
            .report_cleanup_exception(pending.value(), &name, &message, s, f);
        message.clear(f);
        name.clear(f);
        s.clear_throw_diagnostic(ThrowDiagnosticRole::Name, f);
        s.clear_throw_diagnostic(ThrowDiagnosticRole::Message, f);
        s.clear_throw_diagnostic(ThrowDiagnosticRole::ConstructorName, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(super) fn emit_web_dispatch_event(
        &mut self,
        target: &GcLocal<EventTargetObject>,
        target_value: &ValueLocals,
        event: &GcLocal<WebEventObject>,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let _ = (output, exit);
        let flag = s.reserve_i32_local(f);
        // Public dispatch checks reentrancy before entry. Abort dispatch owns a
        // freshly constructed Event, so both callers enter with idle state.
        s.field(WebEventObjectSchema::DISPATCHING)
            .write(event, GcOperand::boolean(true), s, f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<StoredValue>().from_value(target_value, f),
            f,
        );
        s.field(WebEventObjectSchema::TARGET)
            .write(event, GcOperand::reference(&stored, s), s, f);
        s.field(WebEventObjectSchema::CURRENT_TARGET).write(
            event,
            GcOperand::reference(&stored, s),
            s,
            f,
        );
        stored.clear(f);
        let event_value = s.reserve_value_local(f);
        event_value.set_reference(event, s, f);
        let kind = s.reserve_gc_local(f).initialize(
            s.field(WebEventObjectSchema::TYPE)
                .read(event, s, f)
                .reference(),
            f,
        );
        let callback = s.reserve_value_local(f);
        let method = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let index = s.reserve_i32_local(f);
        let length = s.reserve_i32_local(f);
        let equal = s.reserve_i32_local(f);
        let folding = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        folding.store(f);
        let dispatched = self.open_frame(ControlFrameKind::Block, f);
        // At-target capture listeners precede non-capture listeners. Each
        // invocation snapshots current registrations, as the DOM algorithm does.
        for capture in [true, false] {
            let snapshot = self.emit_web_listener_snapshot(target, f);
            s.array_type::<WebEventListenerTable>()
                .length(&snapshot, s, f);
            length.store(f);
            f.instruction(&Instruction::I32Const(0));
            index.store(f);
            let done = self.open_frame(ControlFrameKind::Block, f);
            let next = self.open_frame(ControlFrameKind::Loop, f);
            s.field(WebEventObjectSchema::STOP_IMMEDIATE)
                .read(event, s, f)
                .store(flag, f);
            flag.load(f);
            self.emit_branch_if_to_target(dispatched, f);
            index.load(f);
            length.load(f);
            f.instruction(&Instruction::I32GeU);
            self.emit_branch_if_to_target(done, f);
            let listener = s.reserve_gc_local(f).initialize(
                s.array_type::<WebEventListenerTable>()
                    .read(&snapshot, index, s, f)
                    .reference()
                    .require_non_null(f),
                f,
            );
            let skipped = self.open_frame(ControlFrameKind::Block, f);
            s.field(WebEventListenerSchema::REMOVED)
                .read(&listener, s, f)
                .store(flag, f);
            flag.load(f);
            self.emit_branch_if_to_target(skipped, f);
            self.emit_web_listener_signal_aborted(&listener, flag, f);
            flag.load(f);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_web_remove_listener(target, &listener, f);
            self.emit_branch_to_target(skipped, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            s.field(WebEventListenerSchema::CAPTURE)
                .read(&listener, s, f)
                .store(flag, f);
            flag.load(f);
            f.instruction(&Instruction::I32Const(i32::from(capture)));
            f.instruction(&Instruction::I32Ne);
            self.emit_branch_if_to_target(skipped, f);
            let actual_kind = s.reserve_gc_local(f).initialize(
                s.field(WebEventListenerSchema::TYPE)
                    .read(&listener, s, f)
                    .reference(),
                f,
            );
            self.emit_gc_string_equality(&actual_kind, &kind, folding, equal, f);
            actual_kind.clear(f);
            equal.load(f);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(skipped, f);
            let stored = s.reserve_gc_local(f).initialize(
                s.field(WebEventListenerSchema::CALLBACK)
                    .read(&listener, s, f)
                    .reference(),
                f,
            );
            self.emit_stored_value_to_locals(&stored, &callback, f);
            stored.clear(f);
            s.field(WebEventListenerSchema::ONCE)
                .read(&listener, s, f)
                .store(flag, f);
            flag.load(f);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_web_remove_listener(target, &listener, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            s.field(WebEventListenerSchema::PASSIVE)
                .read(&listener, s, f)
                .store(flag, f);
            s.field(WebEventObjectSchema::PASSIVE).write(
                event,
                GcOperand::boolean_local(flag),
                s,
                f,
            );
            let handler = s.reserve_gc_local(f).initialize(
                s.field(EventTargetObjectSchema::ONABORT)
                    .read(target, s, f)
                    .reference(),
                f,
            );
            let invoked = self.open_frame(ControlFrameKind::Block, f);
            s.clear_throw_diagnostic(ThrowDiagnosticRole::Name, f);
            s.clear_throw_diagnostic(ThrowDiagnosticRole::Message, f);
            s.clear_throw_diagnostic(ThrowDiagnosticRole::ConstructorName, f);
            self.emit_is_callable_i32(&callback, f)?;
            self.open_frame(ControlFrameKind::If, f);
            method.copy_from(&callback, f);
            callback.copy_from(target_value, f);
            f.instruction(&Instruction::Else);
            self.emit_web_get(&callback, "handleEvent", &pending, f)?;
            pending.kind().load(f);
            f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
            f.instruction(&Instruction::I32Eq);
            self.emit_branch_if_to_target(invoked, f);
            method.copy_from(pending.value(), f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            let argv = self.emit_pre_evaluated_arg_vector(&[&event_value], f);
            self.emit_function_or_proxy_call_with_argv(&method, &callback, &argv, &pending, f)?;
            argv.clear(f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            // Event-handler attributes cancel cancelable events only when
            // their callback returns the Boolean false. addEventListener
            // callback return values have no effect.
            handler.load(s, f);
            listener.load(s, f);
            f.instruction(&Instruction::RefEq);
            pending.kind().load(f);
            f.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
            f.instruction(&Instruction::I32Eq);
            f.instruction(&Instruction::I32And);
            pending.value().tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Boolean.tag()));
            f.instruction(&Instruction::I32Eq);
            f.instruction(&Instruction::I32And);
            pending.value().scalar().load(f);
            f.instruction(&Instruction::I64Eqz);
            f.instruction(&Instruction::I32And);
            s.field(WebEventObjectSchema::CANCELABLE).read(event, s, f);
            f.instruction(&Instruction::I32And);
            self.open_frame(ControlFrameKind::If, f);
            s.field(WebEventObjectSchema::CANCELED)
                .write(event, GcOperand::boolean(true), s, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            handler.clear(f);
            self.emit_web_report_listener_exception(&pending, f)?;
            s.field(WebEventObjectSchema::PASSIVE)
                .write(event, GcOperand::boolean(false), s, f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            listener.clear(f);
            index.load(f);
            f.instruction(&Instruction::I32Const(1));
            f.instruction(&Instruction::I32Add);
            index.store(f);
            self.emit_branch_to_target(next, f);
            self.pop_control(ControlFrameKind::Loop);
            f.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            snapshot.clear(f);
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.field(WebEventObjectSchema::DISPATCHING)
            .write(event, GcOperand::boolean(false), s, f);
        s.field(WebEventObjectSchema::STOP_IMMEDIATE)
            .write(event, GcOperand::boolean(false), s, f);
        s.field(WebEventObjectSchema::PASSIVE)
            .write(event, GcOperand::boolean(false), s, f);
        let null = s.reserve_value_local(f);
        null.set_scalar(ScalarValue::Null, f);
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&null, f), f);
        s.field(WebEventObjectSchema::CURRENT_TARGET).write(
            event,
            GcOperand::reference(&stored, s),
            s,
            f,
        );
        stored.clear(f);
        null.clear(f);
        for local in [folding, equal, length, index, flag] {
            s.release_i32_local(local, f);
        }
        pending.clear(f);
        method.clear(f);
        callback.clear(f);
        kind.clear(f);
        event_value.clear(f);
        Ok(())
    }
}

impl FunctionBuilder<'_> {
    fn emit_web_register_abort_listener(
        &mut self,
        target: &GcLocal<EventTargetObject>,
        listener: &GcLocal<WebEventListener>,
        signal: &GcLocal<AbortSignalObject, Nullable>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        signal.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let next = s.reserve_gc_local(f).initialize(
            s.field(AbortSignalObjectSchema::REGISTRATIONS)
                .read(signal, s, f)
                .reference(),
            f,
        );
        let registration = s.reserve_gc_local(f).initialize(
            s.struct_type::<AbortListenerRegistration>().construct(
                (
                    GcOperand::reference(target, s),
                    GcOperand::reference(listener, s),
                    GcOperand::reference(&next, s),
                ),
                f,
            ),
            f,
        );
        s.field(AbortSignalObjectSchema::REGISTRATIONS).write(
            signal,
            GcOperand::nullable_reference(&registration, s),
            s,
            f,
        );
        registration.clear(f);
        next.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    fn emit_web_unregister_abort_listener(
        &mut self,
        listener: &GcLocal<WebEventListener>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let signal = s.reserve_gc_local(f).initialize(
            s.field(WebEventListenerSchema::SIGNAL)
                .read(listener, s, f)
                .reference(),
            f,
        );
        signal.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let current = s.reserve_gc_local(f).initialize(
            s.field(AbortSignalObjectSchema::REGISTRATIONS)
                .read(&signal, s, f)
                .reference(),
            f,
        );
        let previous = s
            .reserve_gc_local::<AbortListenerRegistration, Nullable>(f)
            .initialize_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        current.load(s, f).is_null(f);
        self.emit_branch_if_to_target(done, f);
        let registered = s.reserve_gc_local(f).initialize(
            s.field(AbortListenerRegistrationSchema::LISTENER)
                .read(&current, s, f)
                .reference(),
            f,
        );
        registered.load(s, f);
        listener.load(s, f);
        f.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, f);
        let following = s.reserve_gc_local(f).initialize(
            s.field(AbortListenerRegistrationSchema::NEXT)
                .read(&current, s, f)
                .reference(),
            f,
        );
        previous.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(AbortSignalObjectSchema::REGISTRATIONS).write(
            &signal,
            GcOperand::reference(&following, s),
            s,
            f,
        );
        f.instruction(&Instruction::Else);
        s.field(AbortListenerRegistrationSchema::NEXT).write(
            &previous,
            GcOperand::reference(&following, s),
            s,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        following.clear(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        registered.clear(f);
        previous.replace(current.load(s, f), f);
        current.replace(
            s.field(AbortListenerRegistrationSchema::NEXT)
                .read(&current, s, f)
                .reference(),
            f,
        );
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        previous.clear(f);
        current.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        signal.clear(f);
    }

    pub(super) fn emit_web_run_abort_algorithms(
        &mut self,
        signal: &GcLocal<AbortSignalObject>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let current = s
            .reserve_gc_local::<AbortListenerRegistration, Nullable>(f)
            .initialize_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        current.replace(
            s.field(AbortSignalObjectSchema::REGISTRATIONS)
                .read(signal, s, f)
                .reference(),
            f,
        );
        current.load(s, f).is_null(f);
        self.emit_branch_if_to_target(done, f);
        let next = s.reserve_gc_local(f).initialize(
            s.field(AbortListenerRegistrationSchema::NEXT)
                .read(&current, s, f)
                .reference(),
            f,
        );
        // Pop before removal: remove's unregister cannot retain this cell or
        // walk an already completed abort algorithm a second time.
        s.field(AbortSignalObjectSchema::REGISTRATIONS).write(
            signal,
            GcOperand::reference(&next, s),
            s,
            f,
        );
        let target = s.reserve_gc_local(f).initialize(
            s.field(AbortListenerRegistrationSchema::TARGET)
                .read(&current, s, f)
                .reference(),
            f,
        );
        let listener = s.reserve_gc_local(f).initialize(
            s.field(AbortListenerRegistrationSchema::LISTENER)
                .read(&current, s, f)
                .reference(),
            f,
        );
        self.emit_web_remove_listener(&target, &listener, f);
        listener.clear(f);
        target.clear(f);
        next.clear(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        current.clear(f);
        Ok(())
    }
}
