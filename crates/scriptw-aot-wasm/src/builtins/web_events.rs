//! Standalone Web events and abort state use the canonical Wasm GC heap.
use super::super::*;
use crate::functions::OrdinaryDefaultPrototype;
use crate::gc_types::*;

mod listeners;

impl FunctionBuilder<'_> {
    fn emit_web_abrupt(
        &mut self,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        output.copy_from(pending, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    fn emit_web_type_error_if(
        &mut self,
        message: RuntimeErrorMessage,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(message, output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_web_invalid_dispatch_if(
        &mut self,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.open_frame(ControlFrameKind::If, f);
        let header = self.emit_web_canonical_header(
            crate::functions::NonArrayRealmIntrinsicSlot::DOMExceptionPrototype,
            f,
        )?;
        let name = s.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference("InvalidStateError", f)?,
            f,
        );
        let message = s.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference(
                RuntimeErrorMessage::WEB_EVENT_DISPATCHING
                    .catalog_text()
                    .expect("static Web event diagnostic"),
                f,
            )?,
            f,
        );
        let exception = self.emit_web_alloc_dom_exception(&header, &name, &message, f);
        output.value().set_reference(&exception, s, f);
        output.set_kind(CompletionKind::Throw, f);
        s.store_throw_diagnostic(ThrowDiagnosticRole::Name, &name, f);
        s.store_throw_diagnostic(ThrowDiagnosticRole::Message, &message, f);
        let constructor = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("DOMException", f)?, f);
        s.store_throw_diagnostic(ThrowDiagnosticRole::ConstructorName, &constructor, f);
        constructor.clear(f);
        exception.clear(f);
        message.clear(f);
        name.clear(f);
        header.clear(f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_web_brand<T: JavaScriptReference>(
        &mut self,
        value: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<T>, EmitError> {
        let s = self.runtime_schema();
        value.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<T>(GcNullability::NonNullable).heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.emit_web_type_error_if(
            RuntimeErrorMessage::WEB_EVENT_INVALID_RECEIVER,
            output,
            exit,
            f,
        )?;
        Ok(s.reserve_gc_local(f)
            .initialize(value.cast_reference::<T>(s, f), f))
    }

    fn emit_web_receiver<T: JavaScriptReference>(
        &mut self,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<T>, EmitError> {
        let this = self.runtime_schema().reserve_value_local(f);
        self.compile_this_to_locals(&this, f)?;
        let result = self.emit_web_brand(&this, output, exit, f)?;
        this.clear(f);
        Ok(result)
    }

    fn emit_web_target(
        &mut self,
        value: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<EventTargetObject>, EmitError> {
        let s = self.runtime_schema();
        let result = s
            .reserve_gc_local::<EventTargetObject, Nullable>(f)
            .initialize_null(s, f);
        value.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<EventTargetObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        result.replace(
            value.cast_reference::<EventTargetObject>(s, f).nullable(),
            f,
        );
        f.instruction(&Instruction::Else);
        let signal = self.emit_web_brand::<AbortSignalObject>(value, output, exit, f)?;
        result.replace(
            s.field(AbortSignalObjectSchema::TARGET)
                .read(&signal, s, f)
                .reference()
                .nullable(),
            f,
        );
        signal.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let target = s
            .reserve_gc_local(f)
            .initialize(result.load(s, f).require_non_null(f), f);
        result.clear(f);
        Ok(target)
    }

    fn emit_web_get(
        &mut self,
        value: &ValueLocals,
        name: &str,
        pending: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference(name, f)?, f);
        let key = crate::operations::PropertyKeyLocals::from_string(s, &text, f);
        self.emit_object_read(value, value, &key, pending, f)?;
        key.clear(f);
        text.clear(f);
        Ok(())
    }

    fn emit_web_required_arguments(
        &mut self,
        count: i32,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.body_entry_locals()
            .expect("native Web callable entry")
            .argument_count()
            .load(f);
        f.instruction(&Instruction::I64Const(i64::from(count)));
        f.instruction(&Instruction::I64LtU);
        self.emit_web_type_error_if(
            RuntimeErrorMessage::WEB_EVENT_REQUIRED_ARGUMENT,
            output,
            exit,
            f,
        )
    }

    fn emit_web_constructor_header(
        &mut self,
        default: OrdinaryDefaultPrototype,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let s = self.runtime_schema();
        let new_target = s.reserve_value_local(f);
        self.compile_new_target_to_locals(&new_target, f)?;
        new_target.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.emit_web_type_error_if(RuntimeErrorMessage::WEB_EVENT_REQUIRES_NEW, output, exit, f)?;
        self.emit_get_prototype_from_constructor(&new_target, default, output, f)?;
        self.emit_web_abrupt(output, output, exit, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(output.value()), f)?,
            f,
        );
        new_target.clear(f);
        Ok(header)
    }

    fn emit_web_canonical_header(
        &mut self,
        slot: crate::functions::NonArrayRealmIntrinsicSlot,
        f: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let s = self.runtime_schema();
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let prototype = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(&realm, slot, &prototype, f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), f)?,
            f,
        );
        prototype.clear(f);
        realm.clear(f);
        Ok(header)
    }

    fn emit_web_alloc_target(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        f: &mut Function,
    ) -> GcLocal<EventTargetObject> {
        let s = self.runtime_schema();
        s.reserve_gc_local(f).initialize(
            s.struct_type::<EventTargetObject>().construct(
                (
                    GcOperand::reference(header, s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        )
    }

    fn emit_web_alloc_signal(
        &mut self,
        f: &mut Function,
    ) -> Result<GcLocal<AbortSignalObject>, EmitError> {
        let s = self.runtime_schema();
        let header = self.emit_web_canonical_header(
            crate::functions::NonArrayRealmIntrinsicSlot::AbortSignalPrototype,
            f,
        )?;
        let target = self.emit_web_alloc_target(&header, f);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let reason = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&undefined, f), f);
        let signal = s.reserve_gc_local(f).initialize(
            s.struct_type::<AbortSignalObject>().construct(
                (
                    GcOperand::reference(&header, s),
                    GcOperand::reference(&target, s),
                    GcOperand::boolean(false),
                    GcOperand::reference(&reason, s),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        reason.clear(f);
        undefined.clear(f);
        target.clear(f);
        header.clear(f);
        Ok(signal)
    }

    fn emit_web_alloc_dom_exception(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        name: &GcLocal<StringValue>,
        message: &GcLocal<StringValue>,
        f: &mut Function,
    ) -> GcLocal<DOMExceptionObject> {
        let s = self.runtime_schema();
        s.reserve_gc_local(f).initialize(
            s.struct_type::<DOMExceptionObject>().construct(
                (
                    GcOperand::reference(header, s),
                    GcOperand::reference(name, s),
                    GcOperand::reference(message, s),
                ),
                f,
            ),
            f,
        )
    }

    fn emit_web_alloc_event(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        kind: &GcLocal<StringValue>,
        bubbles: I32Local,
        cancelable: I32Local,
        composed: I32Local,
        f: &mut Function,
    ) -> GcLocal<WebEventObject> {
        let s = self.runtime_schema();
        let null = s.reserve_value_local(f);
        null.set_scalar(ScalarValue::Null, f);
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&null, f), f);
        let event = s.reserve_gc_local(f).initialize(
            s.struct_type::<WebEventObject>().construct(
                (
                    GcOperand::reference(header, s),
                    GcOperand::reference(kind, s),
                    GcOperand::reference(&stored, s),
                    GcOperand::reference(&stored, s),
                    GcOperand::boolean_local(bubbles),
                    GcOperand::boolean_local(cancelable),
                    GcOperand::boolean_local(composed),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                ),
                f,
            ),
            f,
        );
        stored.clear(f);
        null.clear(f);
        event
    }

    fn emit_web_event_constructor(
        &mut self,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.emit_web_required_arguments(1, output, exit, f)?;
        let header =
            self.emit_web_constructor_header(OrdinaryDefaultPrototype::Event, output, exit, f)?;
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        self.emit_builtin_arg_to_value(0, &argument, f);
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        self.emit_web_abrupt(&pending, output, exit, f);
        let kind = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        self.emit_builtin_arg_to_value(1, &argument, f);
        let flags = [
            s.reserve_i32_local(f),
            s.reserve_i32_local(f),
            s.reserve_i32_local(f),
        ];
        self.compile_nullish_tagged_i32(argument.tag(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        for flag in flags {
            flag.set_constant(0, f);
        }
        f.instruction(&Instruction::Else);
        self.emit_is_heap_object_like_tag_i32(argument.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_web_type_error_if(
            RuntimeErrorMessage::WEB_EVENT_INVALID_RECEIVER,
            output,
            exit,
            f,
        )?;
        // WebIDL dictionary members are read in lexicographic order.
        for (name, flag) in [
            ("bubbles", flags[0]),
            ("cancelable", flags[1]),
            ("composed", flags[2]),
        ] {
            self.emit_web_get(&argument, name, &pending, f)?;
            self.emit_web_abrupt(&pending, output, exit, f);
            self.emit_to_boolean_payload_from_tagged_locals(pending.value(), f)?;
            f.instruction(&Instruction::I32WrapI64);
            flag.store(f);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let event = self.emit_web_alloc_event(&header, &kind, flags[0], flags[1], flags[2], f);
        output.initialize(f);
        output.value().set_reference(&event, s, f);
        event.clear(f);
        for flag in flags {
            s.release_i32_local(flag, f);
        }
        kind.clear(f);
        pending.clear(f);
        argument.clear(f);
        header.clear(f);
        Ok(())
    }

    fn emit_web_abort_signal(
        &mut self,
        signal: &GcLocal<AbortSignalObject>,
        reason: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        s.field(AbortSignalObjectSchema::ABORTED).read(signal, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let actual = s.reserve_value_local(f);
        actual.copy_from(reason, f);
        reason.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let header = self.emit_web_canonical_header(
            crate::functions::NonArrayRealmIntrinsicSlot::DOMExceptionPrototype,
            f,
        )?;
        let name = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("AbortError", f)?, f);
        let message = s.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference("This operation was aborted", f)?,
            f,
        );
        let error = self.emit_web_alloc_dom_exception(&header, &name, &message, f);
        actual.set_reference(&error, s, f);
        error.clear(f);
        message.clear(f);
        name.clear(f);
        header.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&actual, f), f);
        s.field(AbortSignalObjectSchema::REASON).write(
            signal,
            GcOperand::reference(&stored, s),
            s,
            f,
        );
        s.field(AbortSignalObjectSchema::ABORTED)
            .write(signal, GcOperand::boolean(true), s, f);
        self.emit_web_run_abort_algorithms(signal, f)?;
        let header = self.emit_web_canonical_header(
            crate::functions::NonArrayRealmIntrinsicSlot::EventPrototype,
            f,
        )?;
        let kind = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("abort", f)?, f);
        let zero = s.reserve_i32_local(f);
        zero.set_constant(0, f);
        let event = self.emit_web_alloc_event(&header, &kind, zero, zero, zero, f);
        s.field(WebEventObjectSchema::TRUSTED)
            .write(&event, GcOperand::boolean(true), s, f);
        let target = s.reserve_gc_local(f).initialize(
            s.field(AbortSignalObjectSchema::TARGET)
                .read(signal, s, f)
                .reference(),
            f,
        );
        let target_value = s.reserve_value_local(f);
        target_value.set_reference(signal, s, f);
        self.emit_web_dispatch_event(&target, &target_value, &event, output, exit, f)?;
        target_value.clear(f);
        target.clear(f);
        event.clear(f);
        s.release_i32_local(zero, f);
        kind.clear(f);
        header.clear(f);
        stored.clear(f);
        actual.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_web_event_builtin(
        &mut self,
        builtin: StandardBuiltinId,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        use StandardBuiltinId as B;
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        match builtin {
            B::AbortSignalConstructor => {
                self.emit_throw_current_function_realm_type_error(
                    RuntimeErrorMessage::WEB_EVENT_ILLEGAL_CONSTRUCTOR,
                    &output,
                    f,
                )?;
            }
            B::EventTargetConstructor => {
                let header = self.emit_web_constructor_header(
                    OrdinaryDefaultPrototype::EventTarget,
                    &output,
                    exit,
                    f,
                )?;
                let target = self.emit_web_alloc_target(&header, f);
                output.initialize(f);
                output.value().set_reference(&target, s, f);
                target.clear(f);
                header.clear(f);
            }
            B::AbortControllerConstructor => {
                let header = self.emit_web_constructor_header(
                    OrdinaryDefaultPrototype::AbortController,
                    &output,
                    exit,
                    f,
                )?;
                let signal = self.emit_web_alloc_signal(f)?;
                let controller = s.reserve_gc_local(f).initialize(
                    s.struct_type::<AbortControllerObject>().construct(
                        (
                            GcOperand::reference(&header, s),
                            GcOperand::reference(&signal, s),
                        ),
                        f,
                    ),
                    f,
                );
                output.initialize(f);
                output.value().set_reference(&controller, s, f);
                controller.clear(f);
                signal.clear(f);
                header.clear(f);
            }
            B::AbortControllerPrototypeSignalGetter | B::AbortControllerPrototypeAbort => {
                let controller =
                    self.emit_web_receiver::<AbortControllerObject>(&output, exit, f)?;
                let signal = s.reserve_gc_local(f).initialize(
                    s.field(AbortControllerObjectSchema::SIGNAL)
                        .read(&controller, s, f)
                        .reference(),
                    f,
                );
                if builtin == B::AbortControllerPrototypeSignalGetter {
                    output.value().set_reference(&signal, s, f);
                } else {
                    let reason = s.reserve_value_local(f);
                    self.emit_builtin_arg_to_value(0, &reason, f);
                    self.emit_web_abort_signal(&signal, &reason, &output, exit, f)?;
                    output.value().set_undefined(f);
                    reason.clear(f);
                }
                signal.clear(f);
                controller.clear(f);
            }
            B::AbortSignalAbort => {
                let signal = self.emit_web_alloc_signal(f)?;
                let reason = s.reserve_value_local(f);
                self.emit_builtin_arg_to_value(0, &reason, f);
                self.emit_web_abort_signal(&signal, &reason, &output, exit, f)?;
                output.value().set_reference(&signal, s, f);
                reason.clear(f);
                signal.clear(f);
            }
            B::AbortSignalPrototypeAbortedGetter
            | B::AbortSignalPrototypeReasonGetter
            | B::AbortSignalPrototypeThrowIfAborted => {
                let signal = self.emit_web_receiver::<AbortSignalObject>(&output, exit, f)?;
                if builtin == B::AbortSignalPrototypeAbortedGetter {
                    let flag = s.reserve_i32_local(f);
                    s.field(AbortSignalObjectSchema::ABORTED)
                        .read(&signal, s, f)
                        .store(flag, f);
                    output.value().set_boolean(flag, f);
                    s.release_i32_local(flag, f);
                } else if builtin == B::AbortSignalPrototypeReasonGetter {
                    let reason = s.reserve_gc_local(f).initialize(
                        s.field(AbortSignalObjectSchema::REASON)
                            .read(&signal, s, f)
                            .reference(),
                        f,
                    );
                    s.struct_type::<StoredValue>()
                        .read_into(&reason, output.value(), s, f);
                    reason.clear(f);
                } else {
                    s.field(AbortSignalObjectSchema::ABORTED)
                        .read(&signal, s, f);
                    self.open_frame(ControlFrameKind::If, f);
                    let reason = s.reserve_gc_local(f).initialize(
                        s.field(AbortSignalObjectSchema::REASON)
                            .read(&signal, s, f)
                            .reference(),
                        f,
                    );
                    s.struct_type::<StoredValue>()
                        .read_into(&reason, output.value(), s, f);
                    output.set_kind(CompletionKind::Throw, f);
                    reason.clear(f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                }
                signal.clear(f);
            }
            B::EventConstructor => self.emit_web_event_constructor(&output, exit, f)?,
            B::DOMExceptionConstructor => {
                self.emit_web_dom_exception_constructor(&output, exit, f)?
            }
            B::EventTargetPrototypeAddEventListener
            | B::EventTargetPrototypeRemoveEventListener
            | B::EventTargetPrototypeDispatchEvent
            | B::AbortSignalPrototypeOnabortGetter
            | B::AbortSignalPrototypeOnabortSetter => {
                self.emit_web_listener_builtin(builtin, &output, exit, f)?;
            }
            B::DOMExceptionPrototypeNameGetter
            | B::DOMExceptionPrototypeMessageGetter
            | B::DOMExceptionPrototypeCodeGetter => {
                let error = self.emit_web_receiver::<DOMExceptionObject>(&output, exit, f)?;
                if builtin == B::DOMExceptionPrototypeCodeGetter {
                    self.emit_web_dom_exception_code(&error, output.value(), f)?;
                } else {
                    let field = match builtin {
                        B::DOMExceptionPrototypeNameGetter => DOMExceptionObjectSchema::NAME,
                        _ => DOMExceptionObjectSchema::MESSAGE,
                    };
                    let text = s
                        .reserve_gc_local(f)
                        .initialize(s.field(field).read(&error, s, f).reference(), f);
                    output.value().set_reference(&text, s, f);
                    text.clear(f);
                }
                error.clear(f);
            }
            B::EventPrototypeTypeGetter
            | B::EventPrototypeTargetGetter
            | B::EventPrototypeCurrentTargetGetter
            | B::EventPrototypeDefaultPreventedGetter
            | B::EventPrototypeEventPhaseGetter
            | B::EventPrototypeBubblesGetter
            | B::EventPrototypeCancelableGetter
            | B::EventPrototypeComposedGetter
            | B::EventPrototypeIsTrustedGetter
            | B::EventPrototypePreventDefault
            | B::EventPrototypeStopPropagation
            | B::EventPrototypeStopImmediatePropagation => {
                self.emit_web_event_member(builtin, &output, exit, f)?;
            }
            _ => unreachable!("Web event callable was admitted by the standard dispatch"),
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        Ok(())
    }

    fn emit_web_dom_exception_constructor(
        &mut self,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let header = self.emit_web_constructor_header(
            OrdinaryDefaultPrototype::DOMException,
            output,
            exit,
            f,
        )?;
        let argument = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        let mut strings = Vec::new();
        for (index, default) in [(0, ""), (1, "Error")] {
            self.emit_builtin_arg_to_value(index, &argument, f);
            argument.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            let text = s
                .reserve_gc_local(f)
                .initialize(self.emit_interned_string_reference(default, f)?, f);
            pending.initialize(f);
            pending.value().set_reference(&text, s, f);
            text.clear(f);
            f.instruction(&Instruction::Else);
            self.emit_value_to_string_payload(&argument, &pending, f)?;
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            self.emit_web_abrupt(&pending, output, exit, f);
            strings.push(
                s.reserve_gc_local::<StringValue, NonNullable>(f)
                    .initialize(pending.value().cast_reference::<StringValue>(s, f), f),
            );
        }
        let exception = self.emit_web_alloc_dom_exception(&header, &strings[1], &strings[0], f);
        output.initialize(f);
        output.value().set_reference(&exception, s, f);
        exception.clear(f);
        for text in strings {
            text.clear(f);
        }
        pending.clear(f);
        argument.clear(f);
        header.clear(f);
        Ok(())
    }

    fn emit_web_dom_exception_code(
        &mut self,
        error: &GcLocal<DOMExceptionObject>,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let name = s.reserve_gc_local(f).initialize(
            s.field(DOMExceptionObjectSchema::NAME)
                .read(error, s, f)
                .reference(),
            f,
        );
        let fold = s.reserve_i32_local(f);
        let equal = s.reserve_i32_local(f);
        fold.set_constant(0, f);
        output.set_scalar(ScalarValue::NumberBits(0), f);
        for (code, spelling) in WEB_DOM_EXCEPTION_CODES {
            if matches!(code, 2 | 6 | 16) {
                continue;
            }
            let other = s
                .reserve_gc_local(f)
                .initialize(self.emit_interned_string_reference(spelling, f)?, f);
            self.emit_gc_string_equality(&name, &other, fold, equal, f);
            equal.load(f);
            self.open_frame(ControlFrameKind::If, f);
            output.set_scalar(
                ScalarValue::NumberBits(f64::from(*code).to_bits() as i64),
                f,
            );
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            other.clear(f);
        }
        s.release_i32_local(equal, f);
        s.release_i32_local(fold, f);
        name.clear(f);
        Ok(())
    }

    fn emit_web_event_member(
        &mut self,
        builtin: StandardBuiltinId,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        use StandardBuiltinId as B;
        let s = self.runtime_schema();
        let event = self.emit_web_receiver::<WebEventObject>(output, exit, f)?;
        match builtin {
            B::EventPrototypeTypeGetter => {
                let text = s.reserve_gc_local(f).initialize(
                    s.field(WebEventObjectSchema::TYPE)
                        .read(&event, s, f)
                        .reference(),
                    f,
                );
                output.value().set_reference(&text, s, f);
                text.clear(f);
            }
            B::EventPrototypeTargetGetter | B::EventPrototypeCurrentTargetGetter => {
                let field = match builtin {
                    B::EventPrototypeTargetGetter => WebEventObjectSchema::TARGET,
                    _ => WebEventObjectSchema::CURRENT_TARGET,
                };
                let stored = s
                    .reserve_gc_local(f)
                    .initialize(s.field(field).read(&event, s, f).reference(), f);
                s.struct_type::<StoredValue>()
                    .read_into(&stored, output.value(), s, f);
                stored.clear(f);
            }
            B::EventPrototypeBubblesGetter
            | B::EventPrototypeCancelableGetter
            | B::EventPrototypeComposedGetter
            | B::EventPrototypeDefaultPreventedGetter => {
                let flag = s.reserve_i32_local(f);
                match builtin {
                    B::EventPrototypeBubblesGetter => s
                        .field(WebEventObjectSchema::BUBBLES)
                        .read(&event, s, f)
                        .store(flag, f),
                    B::EventPrototypeCancelableGetter => s
                        .field(WebEventObjectSchema::CANCELABLE)
                        .read(&event, s, f)
                        .store(flag, f),
                    B::EventPrototypeComposedGetter => s
                        .field(WebEventObjectSchema::COMPOSED)
                        .read(&event, s, f)
                        .store(flag, f),
                    _ => s
                        .field(WebEventObjectSchema::CANCELED)
                        .read(&event, s, f)
                        .store(flag, f),
                }
                output.value().set_boolean(flag, f);
                s.release_i32_local(flag, f);
            }
            B::EventPrototypeIsTrustedGetter => {
                let flag = s.reserve_i32_local(f);
                s.field(WebEventObjectSchema::TRUSTED)
                    .read(&event, s, f)
                    .store(flag, f);
                output.value().set_boolean(flag, f);
                s.release_i32_local(flag, f);
            }
            B::EventPrototypeEventPhaseGetter => {
                s.field(WebEventObjectSchema::DISPATCHING)
                    .read(&event, s, f);
                self.open_frame(ControlFrameKind::If, f);
                output
                    .value()
                    .set_scalar(ScalarValue::NumberBits(2.0_f64.to_bits() as i64), f);
                f.instruction(&Instruction::Else);
                output.value().set_scalar(ScalarValue::NumberBits(0), f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            B::EventPrototypePreventDefault => {
                s.field(WebEventObjectSchema::CANCELABLE).read(&event, s, f);
                s.field(WebEventObjectSchema::PASSIVE).read(&event, s, f);
                f.instruction(&Instruction::I32Eqz);
                f.instruction(&Instruction::I32And);
                self.open_frame(ControlFrameKind::If, f);
                s.field(WebEventObjectSchema::CANCELED).write(
                    &event,
                    GcOperand::boolean(true),
                    s,
                    f,
                );
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
            B::EventPrototypeStopImmediatePropagation => {
                s.field(WebEventObjectSchema::STOP_IMMEDIATE).write(
                    &event,
                    GcOperand::boolean(true),
                    s,
                    f,
                );
            }
            // Standalone targets have no ancestor path; stopping propagation
            // still permits the remaining listeners on this target.
            B::EventPrototypeStopPropagation => {}
            _ => unreachable!("validated Event member"),
        }
        event.clear(f);
        Ok(())
    }
}

pub(crate) const WEB_DOM_EXCEPTION_CODES: &[(u8, &str)] = &[
    (1, "IndexSizeError"),
    (2, "DOMStringSizeError"),
    (3, "HierarchyRequestError"),
    (4, "WrongDocumentError"),
    (5, "InvalidCharacterError"),
    (6, "NoDataAllowedError"),
    (7, "NoModificationAllowedError"),
    (8, "NotFoundError"),
    (9, "NotSupportedError"),
    (10, "InUseAttributeError"),
    (11, "InvalidStateError"),
    (12, "SyntaxError"),
    (13, "InvalidModificationError"),
    (14, "NamespaceError"),
    (15, "InvalidAccessError"),
    (16, "ValidationError"),
    (17, "TypeMismatchError"),
    (18, "SecurityError"),
    (19, "NetworkError"),
    (20, "AbortError"),
    (21, "URLMismatchError"),
    (22, "QuotaExceededError"),
    (23, "TimeoutError"),
    (24, "InvalidNodeTypeError"),
    (25, "DataCloneError"),
];
