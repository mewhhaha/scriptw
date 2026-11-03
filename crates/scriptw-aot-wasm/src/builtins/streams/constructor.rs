use super::*;

impl FunctionBuilder<'_> {
    /// Web-IDL dictionary access order is preserved, including the otherwise
    /// unsupported auto-allocation member. Callback identity is captured once.
    pub(super) fn emit_stream_constructor(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let new_target = s.reserve_value_local(f);
        self.compile_new_target_to_locals(&new_target, f)?;
        let source = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &source, f);
        let strategy = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(1, &strategy, f);
        let start = s.reserve_value_local(f);
        let pull = s.reserve_value_local(f);
        let cancel = s.reserve_value_local(f);
        let prototype = s.reserve_value_local(f);
        let kind = s.reserve_value_local(f);
        let hwm_value = s.reserve_value_local(f);
        let size = s.reserve_value_local(f);
        let hwm = s.reserve_f64_local(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        hwm.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.stream_is_undefined(&new_target, f);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(RuntimeErrorMessage::STREAM_REQUIRES_NEW, &output, exit, f)?;
        self.stream_end_if(f);
        self.stream_dictionary(&source, &output, exit, f)?;
        // Web-IDL converts the second argument before the constructor steps
        // convert the source object into the UnderlyingSource dictionary.
        self.stream_dictionary(&strategy, &output, exit, f)?;
        self.emit_array_native_get(&strategy, "highWaterMark", &pending, f)?;
        self.stream_abrupt(&pending, &output, exit, f);
        hwm_value.copy_from(pending.value(), f);
        self.stream_is_undefined(&hwm_value, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_number_payload(&hwm_value, &pending, f)?;
        self.stream_abrupt(&pending, &output, exit, f);
        pending.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        hwm.store(f);
        self.stream_end_if(f);
        self.emit_array_native_get(&strategy, "size", &pending, f)?;
        self.stream_abrupt(&pending, &output, exit, f);
        size.copy_from(pending.value(), f);
        self.stream_is_undefined(&size, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_callable_i32(&size, f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(RuntimeErrorMessage::STREAM_CALLBACK, &output, exit, f)?;
        self.stream_end_if(f);
        self.stream_end_if(f);
        // Web-IDL creates the platform object after argument conversion and
        // before the Streams constructor converts the source dictionary.
        self.emit_get_prototype_from_constructor(
            &new_target,
            OrdinaryDefaultPrototype::ReadableStream,
            &pending,
            f,
        )?;
        self.stream_abrupt(&pending, &output, exit, f);
        prototype.copy_from(pending.value(), f);
        self.emit_array_native_get(&source, "autoAllocateChunkSize", &pending, f)?;
        self.stream_abrupt(&pending, &output, exit, f);
        self.stream_is_undefined(pending.value(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(
            RuntimeErrorMessage::STREAM_BYOB_UNSUPPORTED,
            &output,
            exit,
            f,
        )?;
        self.stream_end_if(f);
        for (name, value) in [("cancel", &cancel), ("pull", &pull), ("start", &start)] {
            self.emit_array_native_get(&source, name, &pending, f)?;
            self.stream_abrupt(&pending, &output, exit, f);
            value.copy_from(pending.value(), f);
            self.stream_is_undefined(value, f);
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_is_callable_i32(value, f)?;
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
            self.stream_error_exit(RuntimeErrorMessage::STREAM_CALLBACK, &output, exit, f)?;
            self.stream_end_if(f);
            self.stream_end_if(f);
        }
        self.emit_array_native_get(&source, "type", &pending, f)?;
        self.stream_abrupt(&pending, &output, exit, f);
        kind.copy_from(pending.value(), f);
        self.stream_is_undefined(&kind, f);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(RuntimeErrorMessage::STREAM_BYTES_ONLY, &output, exit, f)?;
        self.stream_end_if(f);
        self.emit_value_to_string_payload(&kind, &pending, f)?;
        self.stream_abrupt(&pending, &output, exit, f);
        let actual = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        let expected = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("bytes", f)?, f);
        self.emit_string_payload_equality_i32(&actual, &expected, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(RuntimeErrorMessage::STREAM_BYTES_ONLY, &output, exit, f)?;
        self.stream_end_if(f);
        expected.clear(f);
        actual.clear(f);
        self.stream_is_undefined(&size, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_range_error(
            RuntimeErrorMessage::STREAM_SIZE,
            &output,
            f,
        )?;
        self.emit_branch_to_target(exit, f);
        self.stream_end_if(f);
        hwm.load(f);
        hwm.load(f);
        f.instruction(&Instruction::F64Ne);
        hwm.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Lt);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_range_error(
            RuntimeErrorMessage::STREAM_HWM,
            &output,
            f,
        )?;
        self.emit_branch_to_target(exit, f);
        self.stream_end_if(f);
        let header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), f)?,
            f,
        );
        let undefined = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        let stream = s.reserve_gc_local(f).initialize(
            s.struct_type::<ReadableStreamObject>().construct(
                (
                    GcOperand::reference(&header, s),
                    GcOperand::constant(ReadableStreamState::Readable),
                    GcOperand::reference(&undefined, s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let proto = s.reserve_value_local(f);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            Slot::ReadableByteStreamControllerPrototype,
            &proto,
            f,
        );
        let controller_header = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&proto), f)?,
            f,
        );
        let source_stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&source, f), f);
        let pull_stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&pull, f), f);
        let cancel_stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&cancel, f), f);
        let controller = s.reserve_gc_local(f).initialize(
            s.struct_type::<ReadableByteStreamControllerObject>()
                .construct(
                    (
                        GcOperand::reference(&controller_header, s),
                        GcOperand::reference(&stream, s),
                        GcOperand::reference(&realm, s),
                        GcOperand::reference(&source_stored, s),
                        GcOperand::reference(&pull_stored, s),
                        GcOperand::reference(&cancel_stored, s),
                        GcOperand::null(s),
                        GcOperand::null(s),
                        GcOperand::i64(0),
                        GcOperand::f64_local(hwm),
                        GcOperand::boolean(false),
                        GcOperand::boolean(false),
                        GcOperand::boolean(false),
                        GcOperand::boolean(false),
                    ),
                    f,
                ),
            f,
        );
        s.field(ReadableStreamObjectSchema::CONTROLLER).write(
            &stream,
            GcOperand::nullable_reference(&controller, s),
            s,
            f,
        );
        let controller_value = s.reserve_value_local(f);
        controller_value.set_reference(&controller, s, f);
        pending.initialize(f);
        self.stream_is_undefined(&start, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&controller_value], f);
        self.emit_function_or_proxy_call_with_argv(&start, &source, &argv, &pending, f)?;
        argv.clear(f);
        self.stream_abrupt(&pending, &output, exit, f);
        self.stream_end_if(f);
        self.stream_observe(&controller, pending.value(), true, &output, f)?;
        output.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        output.value().set_reference(&stream, s, f);
        output.set_kind(CompletionKind::Normal, f);
        self.stream_end_if(f);
        controller_value.clear(f);
        controller.clear(f);
        cancel_stored.clear(f);
        pull_stored.clear(f);
        source_stored.clear(f);
        controller_header.clear(f);
        proto.clear(f);
        realm.clear(f);
        stream.clear(f);
        undefined.clear(f);
        header.clear(f);
        self.stream_end_block(f);
        self.completion().copy_from(&output, f);
        s.release_f64_local(hwm, f);
        size.clear(f);
        hwm_value.clear(f);
        kind.clear(f);
        prototype.clear(f);
        cancel.clear(f);
        pull.clear(f);
        start.clear(f);
        strategy.clear(f);
        source.clear(f);
        new_target.clear(f);
        pending.clear(f);
        output.clear(f);
        Ok(())
    }
    fn stream_dictionary(
        &mut self,
        value: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.compile_nullish_tagged_i32(value.tag(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        let null = s.reserve_value_local(f);
        null.set_scalar(ScalarValue::Null, f);
        let empty = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&null), f)?,
            f,
        );
        value.set_reference(&empty, s, f);
        empty.clear(f);
        null.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_is_heap_object_like_tag_i32(value.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.stream_error_exit(RuntimeErrorMessage::STREAM_DICTIONARY, output, exit, f)?;
        self.stream_end_if(f);
        self.stream_end_if(f);
        Ok(())
    }
}
