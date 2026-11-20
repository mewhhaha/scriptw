use super::*;

impl FunctionBuilder<'_> {
    pub(in crate::builtins::standard) fn emit_lazy_iterator_create(
        &mut self,
        kind: LazyIteratorKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        match kind {
            LazyIteratorKind::Map => self.emit_lazy_map_create(f),
            LazyIteratorKind::Filter => self.emit_lazy_filter_create(f),
            LazyIteratorKind::FlatMap => self.emit_lazy_flatmap_create(f),
            LazyIteratorKind::Take => self.emit_lazy_take_create(f),
            LazyIteratorKind::Drop => self.emit_lazy_drop_create(f),
        }
    }
    pub(in crate::builtins::standard) fn emit_lazy_iterator_resume(
        &mut self,
        kind: LazyIteratorKind,
        operation: IteratorHelperOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        match kind {
            LazyIteratorKind::Map => self.emit_lazy_map_resume(operation, f),
            LazyIteratorKind::Filter => self.emit_lazy_filter_resume(operation, f),
            LazyIteratorKind::FlatMap => self.emit_lazy_flatmap_resume(operation, f),
            LazyIteratorKind::Take => self.emit_lazy_take_resume(operation, f),
            LazyIteratorKind::Drop => self.emit_lazy_drop_resume(operation, f),
        }
    }
    fn emit_helper_limit(
        &mut self,
        receiver: &ValueLocals,
        input: &ValueLocals,
        message: RuntimeErrorMessage,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<F64Local, EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        self.emit_value_to_number_payload(input, &pending, f)?;
        self.emit_helper_close_receiver_on_throw(receiver, &pending, output, exit, f)?;
        let number = schema.reserve_f64_local(f);
        pending.value().scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        number.store(f);
        let integer = schema.reserve_i64_local(f);
        self.emit_to_integer_or_infinity_number_payload_from_number_payload(
            pending.value().scalar(),
            integer,
            f,
        );
        number.load(f);
        number.load(f);
        f.instruction(&Instruction::F64Ne);
        number.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(9007199254740991.0)));
        f.instruction(&Instruction::F64Gt);
        number.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
        f.instruction(&Instruction::F64Lt);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Or);
        integer.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Lt);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_range_error(message, &pending, f)?;
        self.emit_iterator_close_with_completion(receiver, &pending, output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        integer.load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        number.store(f);
        schema.release_i64_local(integer, f);
        pending.clear(f);
        Ok(number)
    }
    fn emit_lazy_map_create(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let parameter = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &parameter, f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        self.emit_helper_callback_admission(
            &receiver,
            &parameter,
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_MAP_MAPPER_MUST_BE_CALLABLE,
            &output,
            exit,
            f,
        )?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let realm = schema
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let header = self.emit_helper_header(&realm, f)?;
        let callback = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&parameter, f),
            f,
        );
        let object = schema.reserve_gc_local(f).initialize(
            schema.struct_type::<IteratorMapHelper>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&record, schema),
                    GcOperand::reference(&callback, schema),
                    GcOperand::f64(0.0),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::reference(&realm, schema),
                    GcOperand::boolean(false),
                ),
                f,
            ),
            f,
        );
        output.value().set_reference(&object, schema, f);
        output.set_kind(CompletionKind::Normal, f);
        object.clear(f);
        header.clear(f);
        realm.clear(f);
        record.clear(f);
        callback.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        parameter.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_lazy_map_resume(
        &mut self,
        operation: IteratorHelperOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let flag = schema.reserve_i32_local(f);
        let done = schema.reserve_i32_local(f);
        let value = schema.reserve_value_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        HelperKind::Map.test(&receiver, schema, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_HELPER_CALLED_ON_INCOMPATIBLE_RECEIVER,
            &output,
            exit,
            f,
        )?;
        let object = schema
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<IteratorMapHelper>(schema, f), f);
        let record = schema.reserve_gc_local(f).initialize(
            schema
                .field(IteratorMapHelperSchema::RECORD)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        let callback = schema.reserve_value_local(f);
        let stored = schema.reserve_gc_local(f).initialize(
            schema
                .field(IteratorMapHelperSchema::MAPPER)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &callback, schema, f);
        stored.clear(f);
        let counter = schema.reserve_f64_local(f);
        schema
            .field(IteratorMapHelperSchema::INDEX)
            .read(&object, schema, f)
            .store_f64(counter, f);
        let index = schema.reserve_value_local(f);
        let state = HelperState::Map(
            schema
                .reserve_gc_local(f)
                .initialize(object.load(schema, f), f),
        );
        state.read_flag(HelperFlag::Executing, flag, schema, f);
        flag.load(f);
        self.emit_helper_type_error_if(HelperKind::Map.running_message(), &output, exit, f)?;
        state.read_flag(HelperFlag::Done, flag, schema, f);
        flag.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_helper_done_result(&output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let stop = self.open_frame(ControlFrameKind::Block, f);
        match operation {
            IteratorHelperOperation::Return => {
                state.read_flag(HelperFlag::Started, flag, schema, f);
                flag.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Executing, true, schema, f);
                f.instruction(&Instruction::Else);
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                pending.initialize(f);
                self.emit_helper_close_record(&record, &pending, &output, f)?;
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_abrupt_exit(&output, &pending, stop, f);
                self.emit_helper_done_result(&output, f)?;
            }
            IteratorHelperOperation::Next => {
                state.write_flag(HelperFlag::Executing, true, schema, f);
                state.write_flag(HelperFlag::Started, true, schema, f);
                self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
                self.emit_helper_abrupt_exit(&pending, &output, stop, f);
                done.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_done_result(&output, f)?;
                self.emit_branch_to_target(stop, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                counter.load(f);
                f.instruction(&Instruction::I64ReinterpretF64);
                index.scalar().store(f);
                index.set_number(index.scalar(), f);
                self.emit_helper_call(&callback, &[&value, &index], &pending, f)?;
                pending.kind().load(f);
                f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
                f.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_helper_close_record(&record, &pending, &output, f)?;
                self.emit_branch_to_target(stop, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                counter.load(f);
                f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
                f.instruction(&Instruction::F64Add);
                counter.store(f);
                {
                    schema.field(IteratorMapHelperSchema::INDEX).write(
                        &object,
                        GcOperand::f64_local(counter),
                        schema,
                        f,
                    );
                }
                self.emit_iterator_result_object_from_locals(pending.value(), false, &output, f)?;
                self.emit_branch_to_target(stop, f);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        state.write_flag(HelperFlag::Executing, false, schema, f);
        output.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        state.write_flag(HelperFlag::Done, true, schema, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        callback.clear(f);
        index.clear(f);
        schema.release_f64_local(counter, f);
        state.clear(f);
        object.clear(f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        value.clear(f);
        receiver.clear(f);
        schema.release_i32_local(flag, f);
        schema.release_i32_local(done, f);
        Ok(())
    }
    fn emit_lazy_filter_create(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let parameter = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &parameter, f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        self.emit_helper_callback_admission(
            &receiver,
            &parameter,
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_FILTER_PREDICATE_MUST_BE_CALLABLE,
            &output,
            exit,
            f,
        )?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let realm = schema
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let header = self.emit_helper_header(&realm, f)?;
        let callback = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&parameter, f),
            f,
        );
        let object = schema.reserve_gc_local(f).initialize(
            schema.struct_type::<IteratorFilterHelper>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&record, schema),
                    GcOperand::reference(&callback, schema),
                    GcOperand::f64(0.0),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::reference(&realm, schema),
                    GcOperand::boolean(false),
                ),
                f,
            ),
            f,
        );
        output.value().set_reference(&object, schema, f);
        output.set_kind(CompletionKind::Normal, f);
        object.clear(f);
        header.clear(f);
        realm.clear(f);
        record.clear(f);
        callback.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        parameter.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_lazy_filter_resume(
        &mut self,
        operation: IteratorHelperOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let flag = schema.reserve_i32_local(f);
        let done = schema.reserve_i32_local(f);
        let value = schema.reserve_value_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        HelperKind::Filter.test(&receiver, schema, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_HELPER_CALLED_ON_INCOMPATIBLE_RECEIVER,
            &output,
            exit,
            f,
        )?;
        let object = schema.reserve_gc_local(f).initialize(
            receiver.cast_reference::<IteratorFilterHelper>(schema, f),
            f,
        );
        let record = schema.reserve_gc_local(f).initialize(
            schema
                .field(IteratorFilterHelperSchema::RECORD)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        let callback = schema.reserve_value_local(f);
        let stored = schema.reserve_gc_local(f).initialize(
            schema
                .field(IteratorFilterHelperSchema::PREDICATE)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &callback, schema, f);
        stored.clear(f);
        let counter = schema.reserve_f64_local(f);
        schema
            .field(IteratorFilterHelperSchema::INDEX)
            .read(&object, schema, f)
            .store_f64(counter, f);
        let index = schema.reserve_value_local(f);
        let state = HelperState::Filter(
            schema
                .reserve_gc_local(f)
                .initialize(object.load(schema, f), f),
        );
        state.read_flag(HelperFlag::Executing, flag, schema, f);
        flag.load(f);
        self.emit_helper_type_error_if(HelperKind::Filter.running_message(), &output, exit, f)?;
        state.read_flag(HelperFlag::Done, flag, schema, f);
        flag.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_helper_done_result(&output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let stop = self.open_frame(ControlFrameKind::Block, f);
        match operation {
            IteratorHelperOperation::Return => {
                state.read_flag(HelperFlag::Started, flag, schema, f);
                flag.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Executing, true, schema, f);
                f.instruction(&Instruction::Else);
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                pending.initialize(f);
                self.emit_helper_close_record(&record, &pending, &output, f)?;
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_abrupt_exit(&output, &pending, stop, f);
                self.emit_helper_done_result(&output, f)?;
            }
            IteratorHelperOperation::Next => {
                state.write_flag(HelperFlag::Executing, true, schema, f);
                state.write_flag(HelperFlag::Started, true, schema, f);
                let again = self.open_frame(ControlFrameKind::Loop, f);
                self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
                self.emit_helper_abrupt_exit(&pending, &output, stop, f);
                done.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_done_result(&output, f)?;
                self.emit_branch_to_target(stop, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                counter.load(f);
                f.instruction(&Instruction::I64ReinterpretF64);
                index.scalar().store(f);
                index.set_number(index.scalar(), f);
                self.emit_helper_call(&callback, &[&value, &index], &pending, f)?;
                pending.kind().load(f);
                f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
                f.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_helper_close_record(&record, &pending, &output, f)?;
                self.emit_branch_to_target(stop, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                counter.load(f);
                f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
                f.instruction(&Instruction::F64Add);
                counter.store(f);
                {
                    schema.field(IteratorFilterHelperSchema::INDEX).write(
                        &object,
                        GcOperand::f64_local(counter),
                        schema,
                        f,
                    );
                }
                self.compile_truthy_tagged_i32(pending.value(), f)?;
                f.instruction(&Instruction::I32Eqz);
                self.emit_branch_if_to_target(again, f);
                self.emit_iterator_result_object_from_locals(&value, false, &output, f)?;
                self.emit_branch_to_target(stop, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        state.write_flag(HelperFlag::Executing, false, schema, f);
        output.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        state.write_flag(HelperFlag::Done, true, schema, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        callback.clear(f);
        index.clear(f);
        schema.release_f64_local(counter, f);
        state.clear(f);
        object.clear(f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        value.clear(f);
        receiver.clear(f);
        schema.release_i32_local(flag, f);
        schema.release_i32_local(done, f);
        Ok(())
    }
    fn emit_lazy_flatmap_create(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let parameter = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &parameter, f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        self.emit_helper_callback_admission(
            &receiver,
            &parameter,
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_FLATMAP_MAPPER_MUST_BE_CALLABLE,
            &output,
            exit,
            f,
        )?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let realm = schema
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let header = self.emit_helper_header(&realm, f)?;
        let callback = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&parameter, f),
            f,
        );
        let object = schema.reserve_gc_local(f).initialize(
            schema.struct_type::<IteratorFlatMapHelper>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&record, schema),
                    GcOperand::reference(&callback, schema),
                    GcOperand::null(schema),
                    GcOperand::boolean(false),
                    GcOperand::f64(0.0),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::reference(&realm, schema),
                    GcOperand::boolean(false),
                ),
                f,
            ),
            f,
        );
        output.value().set_reference(&object, schema, f);
        output.set_kind(CompletionKind::Normal, f);
        object.clear(f);
        header.clear(f);
        realm.clear(f);
        record.clear(f);
        callback.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        parameter.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_lazy_flatmap_resume(
        &mut self,
        operation: IteratorHelperOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let flag = schema.reserve_i32_local(f);
        let done = schema.reserve_i32_local(f);
        let value = schema.reserve_value_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        HelperKind::FlatMap.test(&receiver, schema, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_HELPER_CALLED_ON_INCOMPATIBLE_RECEIVER,
            &output,
            exit,
            f,
        )?;
        let object = schema.reserve_gc_local(f).initialize(
            receiver.cast_reference::<IteratorFlatMapHelper>(schema, f),
            f,
        );
        let record = schema.reserve_gc_local(f).initialize(
            schema
                .field(IteratorFlatMapHelperSchema::OUTER)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        let callback = schema.reserve_value_local(f);
        let stored = schema.reserve_gc_local(f).initialize(
            schema
                .field(IteratorFlatMapHelperSchema::MAPPER)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &callback, schema, f);
        stored.clear(f);
        let counter = schema.reserve_f64_local(f);
        schema
            .field(IteratorFlatMapHelperSchema::INDEX)
            .read(&object, schema, f)
            .store_f64(counter, f);
        let index = schema.reserve_value_local(f);
        let state = HelperState::FlatMap(
            schema
                .reserve_gc_local(f)
                .initialize(object.load(schema, f), f),
        );
        state.read_flag(HelperFlag::Executing, flag, schema, f);
        flag.load(f);
        self.emit_helper_type_error_if(HelperKind::FlatMap.running_message(), &output, exit, f)?;
        state.read_flag(HelperFlag::Done, flag, schema, f);
        flag.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_helper_done_result(&output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let stop = self.open_frame(ControlFrameKind::Block, f);
        match operation {
            IteratorHelperOperation::Return => {
                state.read_flag(HelperFlag::Started, flag, schema, f);
                flag.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Executing, true, schema, f);
                f.instruction(&Instruction::Else);
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                pending.initialize(f);
                {
                    schema
                        .field(IteratorFlatMapHelperSchema::INNER_ACTIVE)
                        .read(&object, schema, f)
                        .store(flag, f);
                    flag.load(f);
                    self.open_frame(ControlFrameKind::If, f);
                    let inner = schema.reserve_gc_local(f).initialize(
                        schema
                            .field(IteratorFlatMapHelperSchema::INNER)
                            .read(&object, schema, f)
                            .reference()
                            .require_non_null(f),
                        f,
                    );
                    self.emit_helper_close_record(&inner, &pending, &output, f)?;
                    pending.copy_from(&output, f);
                    schema
                        .field(IteratorFlatMapHelperSchema::INNER_ACTIVE)
                        .write(&object, GcOperand::boolean(false), schema, f);
                    schema.field(IteratorFlatMapHelperSchema::INNER).write(
                        &object,
                        GcOperand::null(schema),
                        schema,
                        f,
                    );
                    inner.clear(f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                }
                self.emit_helper_close_record(&record, &pending, &output, f)?;
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_abrupt_exit(&output, &pending, stop, f);
                self.emit_helper_done_result(&output, f)?;
            }
            IteratorHelperOperation::Next => {
                state.write_flag(HelperFlag::Executing, true, schema, f);
                state.write_flag(HelperFlag::Started, true, schema, f);
                {
                    let again = self.open_frame(ControlFrameKind::Loop, f);
                    schema
                        .field(IteratorFlatMapHelperSchema::INNER_ACTIVE)
                        .read(&object, schema, f)
                        .store(flag, f);
                    flag.load(f);
                    self.open_frame(ControlFrameKind::If, f);
                    let inner = schema.reserve_gc_local(f).initialize(
                        schema
                            .field(IteratorFlatMapHelperSchema::INNER)
                            .read(&object, schema, f)
                            .reference()
                            .require_non_null(f),
                        f,
                    );
                    self.emit_helper_step(&inner, done, Some(&value), &pending, f)?;
                    pending.kind().load(f);
                    f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
                    f.instruction(&Instruction::I32Eq);
                    self.open_frame(ControlFrameKind::If, f);
                    self.emit_helper_close_record(&record, &pending, &output, f)?;
                    self.emit_branch_to_target(stop, f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    done.load(f);
                    self.open_frame(ControlFrameKind::If, f);
                    schema
                        .field(IteratorFlatMapHelperSchema::INNER_ACTIVE)
                        .write(&object, GcOperand::boolean(false), schema, f);
                    schema.field(IteratorFlatMapHelperSchema::INNER).write(
                        &object,
                        GcOperand::null(schema),
                        schema,
                        f,
                    );
                    f.instruction(&Instruction::Else);
                    self.emit_iterator_result_object_from_locals(&value, false, &output, f)?;
                    self.emit_branch_to_target(stop, f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    inner.clear(f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
                    self.emit_helper_abrupt_exit(&pending, &output, stop, f);
                    done.load(f);
                    self.open_frame(ControlFrameKind::If, f);
                    state.write_flag(HelperFlag::Done, true, schema, f);
                    self.emit_helper_done_result(&output, f)?;
                    self.emit_branch_to_target(stop, f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    counter.load(f);
                    f.instruction(&Instruction::I64ReinterpretF64);
                    index.scalar().store(f);
                    index.set_number(index.scalar(), f);
                    self.emit_helper_call(&callback, &[&value, &index], &pending, f)?;
                    pending.kind().load(f);
                    f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
                    f.instruction(&Instruction::I32Eq);
                    self.open_frame(ControlFrameKind::If, f);
                    self.emit_helper_close_record(&record, &pending, &output, f)?;
                    self.emit_branch_to_target(stop, f);
                    self.pop_control(ControlFrameKind::If);
                    f.instruction(&Instruction::End);
                    // The inner record is used only inside the block that
                    // acquires it: every early exit from that block is a
                    // Throw in `output`, reached before the record's holder
                    // is written.
                    let acquired = self.open_frame(ControlFrameKind::Block, f);
                    let inner = self
                        .emit_helper_flattenable_record(pending.value(), &output, acquired, f)?
                        .into_record();
                    schema.field(IteratorFlatMapHelperSchema::INNER).write(
                        &object,
                        GcOperand::nullable_reference(&inner, schema),
                        schema,
                        f,
                    );
                    schema
                        .field(IteratorFlatMapHelperSchema::INNER_ACTIVE)
                        .write(&object, GcOperand::boolean(true), schema, f);
                    inner.clear(f);
                    counter.load(f);
                    f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
                    f.instruction(&Instruction::F64Add);
                    counter.store(f);
                    schema.field(IteratorFlatMapHelperSchema::INDEX).write(
                        &object,
                        GcOperand::f64_local(counter),
                        schema,
                        f,
                    );
                    self.emit_branch_to_target(again, f);
                    self.pop_control(ControlFrameKind::Block);
                    f.instruction(&Instruction::End);
                    self.emit_helper_close_record(&record, &output, &pending, f)?;
                    output.copy_from(&pending, f);
                    self.emit_branch_to_target(stop, f);
                    self.pop_control(ControlFrameKind::Loop);
                    f.instruction(&Instruction::End);
                }
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        state.write_flag(HelperFlag::Executing, false, schema, f);
        output.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        state.write_flag(HelperFlag::Done, true, schema, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        callback.clear(f);
        index.clear(f);
        schema.release_f64_local(counter, f);
        state.clear(f);
        object.clear(f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        value.clear(f);
        receiver.clear(f);
        schema.release_i32_local(flag, f);
        schema.release_i32_local(done, f);
        Ok(())
    }
    fn emit_lazy_take_create(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let parameter = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &parameter, f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        let limit = self.emit_helper_limit(
            &receiver,
            &parameter,
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_TAKE_LIMIT_MUST_BE_A_NON_NEGATIVE_NUMBER,
            &output,
            exit,
            f,
        )?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let realm = schema
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let header = self.emit_helper_header(&realm, f)?;
        let object = schema.reserve_gc_local(f).initialize(
            schema.struct_type::<IteratorTakeHelper>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&record, schema),
                    GcOperand::f64_local(limit),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::reference(&realm, schema),
                    GcOperand::boolean(false),
                ),
                f,
            ),
            f,
        );
        output.value().set_reference(&object, schema, f);
        output.set_kind(CompletionKind::Normal, f);
        object.clear(f);
        header.clear(f);
        realm.clear(f);
        record.clear(f);
        schema.release_f64_local(limit, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        parameter.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_lazy_take_resume(
        &mut self,
        operation: IteratorHelperOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let flag = schema.reserve_i32_local(f);
        let done = schema.reserve_i32_local(f);
        let value = schema.reserve_value_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        HelperKind::Take.test(&receiver, schema, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_HELPER_CALLED_ON_INCOMPATIBLE_RECEIVER,
            &output,
            exit,
            f,
        )?;
        let object = schema
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<IteratorTakeHelper>(schema, f), f);
        let record = schema.reserve_gc_local(f).initialize(
            schema
                .field(IteratorTakeHelperSchema::RECORD)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        let remaining = schema.reserve_f64_local(f);
        schema
            .field(IteratorTakeHelperSchema::REMAINING)
            .read(&object, schema, f)
            .store_f64(remaining, f);
        let state = HelperState::Take(
            schema
                .reserve_gc_local(f)
                .initialize(object.load(schema, f), f),
        );
        state.read_flag(HelperFlag::Executing, flag, schema, f);
        flag.load(f);
        self.emit_helper_type_error_if(HelperKind::Take.running_message(), &output, exit, f)?;
        state.read_flag(HelperFlag::Done, flag, schema, f);
        flag.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_helper_done_result(&output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let stop = self.open_frame(ControlFrameKind::Block, f);
        match operation {
            IteratorHelperOperation::Return => {
                state.read_flag(HelperFlag::Started, flag, schema, f);
                flag.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Executing, true, schema, f);
                f.instruction(&Instruction::Else);
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                pending.initialize(f);
                self.emit_helper_close_record(&record, &pending, &output, f)?;
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_abrupt_exit(&output, &pending, stop, f);
                self.emit_helper_done_result(&output, f)?;
            }
            IteratorHelperOperation::Next => {
                state.write_flag(HelperFlag::Executing, true, schema, f);
                state.write_flag(HelperFlag::Started, true, schema, f);
                remaining.load(f);
                f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
                f.instruction(&Instruction::F64Eq);
                self.open_frame(ControlFrameKind::If, f);
                pending.initialize(f);
                self.emit_helper_close_record(&record, &pending, &output, f)?;
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_abrupt_exit(&output, &pending, stop, f);
                self.emit_helper_done_result(&output, f)?;
                self.emit_branch_to_target(stop, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                remaining.load(f);
                f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
                f.instruction(&Instruction::F64Sub);
                remaining.store(f);
                {
                    schema.field(IteratorTakeHelperSchema::REMAINING).write(
                        &object,
                        GcOperand::f64_local(remaining),
                        schema,
                        f,
                    );
                }
                self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
                self.emit_helper_abrupt_exit(&pending, &output, stop, f);
                done.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_done_result(&output, f)?;
                f.instruction(&Instruction::Else);
                self.emit_iterator_result_object_from_locals(&value, false, &output, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        state.write_flag(HelperFlag::Executing, false, schema, f);
        output.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        state.write_flag(HelperFlag::Done, true, schema, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_f64_local(remaining, f);
        state.clear(f);
        object.clear(f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        value.clear(f);
        receiver.clear(f);
        schema.release_i32_local(flag, f);
        schema.release_i32_local(done, f);
        Ok(())
    }
    fn emit_lazy_drop_create(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let parameter = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &parameter, f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        let limit = self.emit_helper_limit(
            &receiver,
            &parameter,
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_DROP_LIMIT_MUST_BE_A_NON_NEGATIVE_NUMBER,
            &output,
            exit,
            f,
        )?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let realm = schema
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let header = self.emit_helper_header(&realm, f)?;
        let object = schema.reserve_gc_local(f).initialize(
            schema.struct_type::<IteratorDropHelper>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&record, schema),
                    GcOperand::f64_local(limit),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::reference(&realm, schema),
                    GcOperand::boolean(false),
                ),
                f,
            ),
            f,
        );
        output.value().set_reference(&object, schema, f);
        output.set_kind(CompletionKind::Normal, f);
        object.clear(f);
        header.clear(f);
        realm.clear(f);
        record.clear(f);
        schema.release_f64_local(limit, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        parameter.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_lazy_drop_resume(
        &mut self,
        operation: IteratorHelperOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let flag = schema.reserve_i32_local(f);
        let done = schema.reserve_i32_local(f);
        let value = schema.reserve_value_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        HelperKind::Drop.test(&receiver, schema, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_HELPER_CALLED_ON_INCOMPATIBLE_RECEIVER,
            &output,
            exit,
            f,
        )?;
        let object = schema
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<IteratorDropHelper>(schema, f), f);
        let record = schema.reserve_gc_local(f).initialize(
            schema
                .field(IteratorDropHelperSchema::RECORD)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        let remaining = schema.reserve_f64_local(f);
        schema
            .field(IteratorDropHelperSchema::REMAINING)
            .read(&object, schema, f)
            .store_f64(remaining, f);
        let state = HelperState::Drop(
            schema
                .reserve_gc_local(f)
                .initialize(object.load(schema, f), f),
        );
        state.read_flag(HelperFlag::Executing, flag, schema, f);
        flag.load(f);
        self.emit_helper_type_error_if(HelperKind::Drop.running_message(), &output, exit, f)?;
        state.read_flag(HelperFlag::Done, flag, schema, f);
        flag.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_helper_done_result(&output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let stop = self.open_frame(ControlFrameKind::Block, f);
        match operation {
            IteratorHelperOperation::Return => {
                state.read_flag(HelperFlag::Started, flag, schema, f);
                flag.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Executing, true, schema, f);
                f.instruction(&Instruction::Else);
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                pending.initialize(f);
                self.emit_helper_close_record(&record, &pending, &output, f)?;
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_abrupt_exit(&output, &pending, stop, f);
                self.emit_helper_done_result(&output, f)?;
            }
            IteratorHelperOperation::Next => {
                state.write_flag(HelperFlag::Executing, true, schema, f);
                state.write_flag(HelperFlag::Started, true, schema, f);
                let dropped = self.open_frame(ControlFrameKind::Block, f);
                let skip = self.open_frame(ControlFrameKind::Loop, f);
                remaining.load(f);
                f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
                f.instruction(&Instruction::F64Gt);
                f.instruction(&Instruction::I32Eqz);
                self.emit_branch_if_to_target(dropped, f);
                remaining.load(f);
                f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
                f.instruction(&Instruction::F64Sub);
                remaining.store(f);
                {
                    schema.field(IteratorDropHelperSchema::REMAINING).write(
                        &object,
                        GcOperand::f64_local(remaining),
                        schema,
                        f,
                    );
                }
                self.emit_helper_step(&record, done, None, &pending, f)?;
                self.emit_helper_abrupt_exit(&pending, &output, stop, f);
                done.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_done_result(&output, f)?;
                self.emit_branch_to_target(stop, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.emit_branch_to_target(skip, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
                self.emit_helper_abrupt_exit(&pending, &output, stop, f);
                done.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Done, true, schema, f);
                self.emit_helper_done_result(&output, f)?;
                f.instruction(&Instruction::Else);
                self.emit_iterator_result_object_from_locals(&value, false, &output, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        state.write_flag(HelperFlag::Executing, false, schema, f);
        output.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        state.write_flag(HelperFlag::Done, true, schema, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_f64_local(remaining, f);
        state.clear(f);
        object.clear(f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        value.clear(f);
        receiver.clear(f);
        schema.release_i32_local(flag, f);
        schema.release_i32_local(done, f);
        Ok(())
    }
}
