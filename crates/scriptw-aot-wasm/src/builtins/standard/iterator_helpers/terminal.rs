use super::*;
use crate::functions::ArgumentListConstruction;
impl FunctionBuilder<'_> {
    pub(in crate::builtins::standard) fn emit_native_iterator_terminal(
        &mut self,
        kind: IteratorTerminalKind,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        match kind {
            IteratorTerminalKind::ToArray => self.emit_iterator_terminal_toarray(f),
            IteratorTerminalKind::Includes => self.emit_iterator_terminal_includes(f),
            IteratorTerminalKind::Join => self.emit_iterator_terminal_join(f),
            IteratorTerminalKind::ForEach => self.emit_iterator_terminal_foreach(f),
            IteratorTerminalKind::Every => self.emit_iterator_terminal_every(f),
            IteratorTerminalKind::Some => self.emit_iterator_terminal_some(f),
            IteratorTerminalKind::Find => self.emit_iterator_terminal_find(f),
            IteratorTerminalKind::Reduce => self.emit_iterator_terminal_reduce(f),
        }
    }
    fn emit_iterator_includes_argument_error_if(
        &mut self,
        range: bool,
        receiver: &ValueLocals,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.open_frame(ControlFrameKind::If, f);
        if range {
            self.emit_throw_current_function_realm_range_error(
                RuntimeErrorMessage::ITERATOR_INCLUDES_SKIP_OUT_OF_RANGE,
                pending,
                f,
            )?;
        } else {
            self.emit_throw_current_function_realm_type_error(
                RuntimeErrorMessage::ITERATOR_INCLUDES_SKIP_MUST_BE_INTEGRAL_NUMBER,
                pending,
                f,
            )?;
        }
        // The provisional record has not acquired next. IteratorClose keeps
        // the original validation error when return acquisition or call throws.
        self.emit_iterator_close_with_completion(receiver, pending, output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_iterator_terminal_includes(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let search = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &search, f);
        let input = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(1, &input, f);
        let value = schema.reserve_value_local(f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let done = schema.reserve_i32_local(f);
        let to_skip = schema.reserve_f64_local(f);
        let skipped = schema.reserve_f64_local(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        to_skip.store(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        skipped.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        input.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        input.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_iterator_includes_argument_error_if(
            false, &receiver, &pending, &output, exit, f,
        )?;
        input.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        to_skip.store(f);
        // Truncation preserves infinities and signed zero. NaN and finite
        // fractions compare unequal; no user conversion hook is called.
        to_skip.load(f);
        f.instruction(&Instruction::F64Trunc);
        to_skip.load(f);
        f.instruction(&Instruction::F64Ne);
        self.emit_iterator_includes_argument_error_if(
            false, &receiver, &pending, &output, exit, f,
        )?;
        to_skip.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        f.instruction(&Instruction::F64Lt);
        to_skip.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(9007199254740991.0)));
        f.instruction(&Instruction::F64Gt);
        to_skip.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(f64::INFINITY)));
        f.instruction(&Instruction::F64Ne);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Or);
        self.emit_iterator_includes_argument_error_if(true, &receiver, &pending, &output, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
        self.emit_helper_abrupt_exit(&pending, &output, exit, f);
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
        skipped.load(f);
        to_skip.load(f);
        f.instruction(&Instruction::F64Lt);
        self.open_frame(ControlFrameKind::If, f);
        skipped.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        f.instruction(&Instruction::F64Add);
        skipped.store(f);
        f.instruction(&Instruction::Else);
        self.emit_tagged_payload_same_value_zero_i32(&value, &search, f)?;
        self.open_frame(ControlFrameKind::If, f);
        value.set_scalar(ScalarValue::Boolean(true), f);
        pending.set_normal(&value, f);
        self.emit_helper_close_record(&record, &pending, &output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.set_scalar(ScalarValue::Boolean(false), f);
        output.set_normal(&value, f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        schema.release_f64_local(skipped, f);
        schema.release_f64_local(to_skip, f);
        schema.release_i32_local(done, f);
        pending.clear(f);
        output.clear(f);
        value.clear(f);
        input.clear(f);
        search.clear(f);
        receiver.clear(f);
        Ok(())
    }

    fn emit_iterator_terminal_join(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let argument = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &argument, f);
        let value = schema.reserve_value_local(f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let done = schema.reserve_i32_local(f);
        let first = schema.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(1));
        first.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        let separator = schema
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference(",", f)?, f);
        argument.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_string_payload(&argument, &pending, f)?;
        self.emit_helper_close_receiver_on_throw(&receiver, &pending, &output, exit, f)?;
        separator.replace(pending.value().cast_reference::<StringValue>(schema, f), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let text = schema
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("", f)?, f);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
        self.emit_helper_abrupt_exit(&pending, &output, exit, f);
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
        first.load(f);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::I32Const(0));
        first.store(f);
        f.instruction(&Instruction::Else);
        text.replace(self.emit_concat_gc_strings(&text, &separator, f), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null as i32));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_value_to_string_payload(&value, &pending, f)?;
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_helper_close_record(&record, &pending, &output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let part = schema
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(schema, f), f);
        text.replace(self.emit_concat_gc_strings(&text, &part, f), f);
        part.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.set_reference(&text, schema, f);
        output.set_normal(&value, f);
        text.clear(f);
        separator.clear(f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        schema.release_i32_local(first, f);
        schema.release_i32_local(done, f);
        pending.clear(f);
        output.clear(f);
        value.clear(f);
        argument.clear(f);
        receiver.clear(f);
        Ok(())
    }

    fn emit_iterator_terminal_toarray(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let value = schema.reserve_value_local(f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let done = schema.reserve_i32_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let construction = ArgumentListConstruction::new(self, f);
        let collected = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
        self.emit_helper_abrupt_exit(&pending, &output, exit, f);
        done.load(f);
        self.emit_branch_if_to_target(collected, f);
        construction.append(&value, schema, f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let list = construction.finish(self, f);
        let array = self.emit_array_from_argument_list(&list, f)?;
        output.value().set_reference(&array, schema, f);
        output.set_kind(CompletionKind::Normal, f);
        array.clear(f);
        list.clear(f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        output.clear(f);
        pending.clear(f);
        value.clear(f);
        receiver.clear(f);
        schema.release_i32_local(done, f);
        Ok(())
    }
    fn emit_iterator_terminal_foreach(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let callback = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &callback, f);
        let value = schema.reserve_value_local(f);
        let index = schema.reserve_value_local(f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let done = schema.reserve_i32_local(f);
        let counter = schema.reserve_f64_local(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        counter.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        self.emit_helper_callback_admission(
            &receiver,
            &callback,
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_FOREACH_CALLBACK_MUST_BE_CALLABLE,
            &output,
            exit,
            f,
        )?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
        self.emit_helper_abrupt_exit(&pending, &output, exit, f);
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
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
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        counter.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        f.instruction(&Instruction::F64Add);
        counter.store(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.set_undefined(f);
        output.set_normal(&value, f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        schema.release_f64_local(counter, f);
        schema.release_i32_local(done, f);
        output.clear(f);
        pending.clear(f);
        index.clear(f);
        value.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_iterator_terminal_every(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let callback = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &callback, f);
        let value = schema.reserve_value_local(f);
        let index = schema.reserve_value_local(f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let done = schema.reserve_i32_local(f);
        let counter = schema.reserve_f64_local(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        counter.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        self.emit_helper_callback_admission(
            &receiver,
            &callback,
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_EVERY_CALLBACK_MUST_BE_CALLABLE,
            &output,
            exit,
            f,
        )?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
        self.emit_helper_abrupt_exit(&pending, &output, exit, f);
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
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
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        value.set_scalar(ScalarValue::Boolean(false), f);
        pending.set_normal(&value, f);
        self.emit_helper_close_record(&record, &pending, &output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        counter.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        f.instruction(&Instruction::F64Add);
        counter.store(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.set_scalar(ScalarValue::Boolean(true), f);
        output.set_normal(&value, f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        schema.release_f64_local(counter, f);
        schema.release_i32_local(done, f);
        output.clear(f);
        pending.clear(f);
        index.clear(f);
        value.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_iterator_terminal_some(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let callback = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &callback, f);
        let value = schema.reserve_value_local(f);
        let index = schema.reserve_value_local(f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let done = schema.reserve_i32_local(f);
        let counter = schema.reserve_f64_local(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        counter.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        self.emit_helper_callback_admission(
            &receiver,
            &callback,
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_SOME_CALLBACK_MUST_BE_CALLABLE,
            &output,
            exit,
            f,
        )?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
        self.emit_helper_abrupt_exit(&pending, &output, exit, f);
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
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
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        value.set_scalar(ScalarValue::Boolean(true), f);
        pending.set_normal(&value, f);
        self.emit_helper_close_record(&record, &pending, &output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        counter.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        f.instruction(&Instruction::F64Add);
        counter.store(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.set_scalar(ScalarValue::Boolean(false), f);
        output.set_normal(&value, f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        schema.release_f64_local(counter, f);
        schema.release_i32_local(done, f);
        output.clear(f);
        pending.clear(f);
        index.clear(f);
        value.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_iterator_terminal_find(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let callback = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &callback, f);
        let value = schema.reserve_value_local(f);
        let index = schema.reserve_value_local(f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let done = schema.reserve_i32_local(f);
        let counter = schema.reserve_f64_local(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        counter.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        self.emit_helper_callback_admission(
            &receiver,
            &callback,
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_FIND_CALLBACK_MUST_BE_CALLABLE,
            &output,
            exit,
            f,
        )?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
        self.emit_helper_abrupt_exit(&pending, &output, exit, f);
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
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
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        pending.set_normal(&value, f);
        self.emit_helper_close_record(&record, &pending, &output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        counter.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        f.instruction(&Instruction::F64Add);
        counter.store(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.set_undefined(f);
        output.set_normal(&value, f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        schema.release_f64_local(counter, f);
        schema.release_i32_local(done, f);
        output.clear(f);
        pending.clear(f);
        index.clear(f);
        value.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
    fn emit_iterator_terminal_reduce(&mut self, f: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let callback = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &callback, f);
        let value = schema.reserve_value_local(f);
        let index = schema.reserve_value_local(f);
        let output = schema.reserve_completion(f);
        output.initialize(f);
        let pending = schema.reserve_completion(f);
        pending.initialize(f);
        let done = schema.reserve_i32_local(f);
        let counter = schema.reserve_f64_local(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(0.0)));
        counter.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_helper_require_object(&receiver, &output, exit, f)?;
        self.emit_helper_callback_admission(
            &receiver,
            &callback,
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_REDUCE_REDUCER_MUST_BE_CALLABLE,
            &output,
            exit,
            f,
        )?;
        let record = self
            .emit_helper_direct_record(&receiver, &output, exit, f)?
            .into_record();
        let accumulator = schema.reserve_value_local(f);
        self.emit_builtin_arg_to_value(1, &accumulator, f);
        self.body_entry_locals()
            .map(|entry| entry.argument_count())
            .ok_or_else(|| EmitError::unsupported("missing actual reducer argument count"))?
            .load(f);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_helper_step(&record, done, Some(&accumulator), &pending, f)?;
        self.emit_helper_abrupt_exit(&pending, &output, exit, f);
        done.load(f);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_PROTOTYPE_REDUCE_OF_EMPTY_ITERATOR_WITH_NO_INITIAL_VALUE,
            &output,
            exit,
            f,
        )?;
        f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        counter.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let finished = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
        self.emit_helper_abrupt_exit(&pending, &output, exit, f);
        done.load(f);
        self.emit_branch_if_to_target(finished, f);
        counter.load(f);
        f.instruction(&Instruction::I64ReinterpretF64);
        index.scalar().store(f);
        index.set_number(index.scalar(), f);
        self.emit_helper_call(&callback, &[&accumulator, &value, &index], &pending, f)?;
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_helper_close_record(&record, &pending, &output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        accumulator.copy_from(pending.value(), f);
        counter.load(f);
        f.instruction(&Instruction::F64Const(Ieee64::from(1.0)));
        f.instruction(&Instruction::F64Add);
        counter.store(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        output.set_normal(&accumulator, f);
        accumulator.clear(f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        schema.release_f64_local(counter, f);
        schema.release_i32_local(done, f);
        output.clear(f);
        pending.clear(f);
        index.clear(f);
        value.clear(f);
        callback.clear(f);
        receiver.clear(f);
        Ok(())
    }
}
