//! Zip captures complete direct records before padding or helper publication.
use super::*;
use crate::functions::{ArgumentListConstruction, NativeObjectAlgorithm};

#[derive(Clone, Copy)]
pub(in crate::builtins::standard) enum IteratorZipInput {
    Iterable,
    Keyed,
}

#[derive(Clone, Copy)]
enum ZipRecordSource {
    Iterable,
    Flattenable,
}

impl FunctionBuilder<'_> {
    fn emit_zip_is_throw(&self, completion: &CompletionLocals, f: &mut Function) {
        completion.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
    }
    fn emit_zip_increment(&self, index: I32Local, f: &mut Function) {
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
    }
    fn emit_zip_decrement(&self, index: I32Local, f: &mut Function) {
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        index.store(f);
    }

    /// Failure leaves a nullable root and a whole Throw; the caller owns which
    /// already acquired records need closing before it publishes that Throw.
    fn emit_zip_acquire_record(
        &mut self,
        source: &ValueLocals,
        kind: ZipRecordSource,
        output: &CompletionLocals,
        f: &mut Function,
    ) -> Result<GcLocal<IteratorRecord, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let result = s
            .reserve_gc_local::<IteratorRecord, Nullable>(f)
            .initialize_null(s, f);
        output.initialize(f);
        let finish = self.open_frame(ControlFrameKind::Block, f);
        match kind {
            ZipRecordSource::Flattenable => {
                let record = self
                    .emit_helper_flattenable_record(source, output, finish, f)?
                    .into_record();
                result.replace(record.load(s, f).nullable(), f);
                record.clear(f);
            }
            ZipRecordSource::Iterable => {
                let symbol = s.reserve_gc_local(f).initialize(
                    self.emit_well_known_symbol_reference(
                        scriptw_ir::WellKnownSymbol::Iterator,
                        f,
                    )?,
                    f,
                );
                let key = PropertyKeyLocals::from_symbol(s, &symbol, f);
                symbol.clear(f);
                self.emit_object_read(source, source, &key, output, f)?;
                key.clear(f);
                self.emit_zip_is_throw(output, f);
                self.emit_branch_if_to_target(finish, f);
                let method = s.reserve_value_local(f);
                method.copy_from(output.value(), f);
                self.emit_is_callable_i32(&method, f)?;
                f.instruction(&Instruction::I32Eqz);
                self.emit_helper_type_error_if(
                    RuntimeErrorMessage::ITERATOR_ZIP_ITERATOR_METHOD_MUST_BE_CALLABLE,
                    output,
                    finish,
                    f,
                )?;
                let argv = self.emit_pre_evaluated_arg_vector(&[], f);
                self.emit_function_or_proxy_call_with_argv(&method, source, &argv, output, f)?;
                argv.clear(f);
                method.clear(f);
                self.emit_zip_is_throw(output, f);
                self.emit_branch_if_to_target(finish, f);
                let receiver = s.reserve_value_local(f);
                receiver.copy_from(output.value(), f);
                self.emit_is_heap_object_like_tag_i32(receiver.tag(), f);
                f.instruction(&Instruction::I32Eqz);
                self.emit_helper_type_error_if(
                    RuntimeErrorMessage::ITERATOR_ZIP_ITERATOR_METHOD_MUST_RETURN_OBJECT,
                    output,
                    finish,
                    f,
                )?;
                let record = self
                    .emit_helper_direct_record(&receiver, output, finish, f)?
                    .into_record();
                result.replace(record.load(s, f).nullable(), f);
                record.clear(f);
                receiver.clear(f);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Ok(result)
    }
    fn emit_zip_append_acquisition(
        &mut self,
        head: &GcLocal<IteratorZipAcquisition, Nullable>,
        count: I32Local,
        record: &GcLocal<IteratorRecord>,
        key: Option<&PropertyKeyLocals>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        count.load(f);
        f.instruction(&Instruction::I32Const(-1));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        let stored = s
            .reserve_gc_local::<StoredValue, Nullable>(f)
            .initialize_null(s, f);
        if let Some(key) = key {
            stored.replace(
                s.struct_type::<StoredValue>()
                    .from_value(key.value(), f)
                    .nullable(),
                f,
            );
        }
        let node = s.reserve_gc_local(f).initialize(
            s.struct_type::<IteratorZipAcquisition>().construct(
                (
                    GcOperand::reference(record, s),
                    GcOperand::reference(&stored, s),
                    GcOperand::reference(head, s),
                ),
                f,
            ),
            f,
        );
        head.replace(node.load(s, f).nullable(), f);
        self.emit_zip_increment(count, f);
        node.clear(f);
        stored.clear(f);
    }
    fn emit_zip_close_acquisitions(
        &mut self,
        head: &GcLocal<IteratorZipAcquisition, Nullable>,
        outer: Option<&GcLocal<IteratorRecord>>,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        output.copy_from(pending, f);
        let cursor = s.reserve_gc_local(f).initialize(head.load(s, f), f);
        let closed = s.reserve_completion(f);
        closed.initialize(f);
        let finish = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(s, f).is_null(f);
        self.emit_branch_if_to_target(finish, f);
        let node = s
            .reserve_gc_local(f)
            .initialize(cursor.load(s, f).require_non_null(f), f);
        let record = s.reserve_gc_local(f).initialize(
            s.field(IteratorZipAcquisitionSchema::RECORD)
                .read(&node, s, f)
                .reference(),
            f,
        );
        self.emit_helper_close_record(&record, output, &closed, f)?;
        output.copy_from(&closed, f);
        cursor.replace(
            s.field(IteratorZipAcquisitionSchema::PREVIOUS)
                .read(&node, s, f)
                .reference(),
            f,
        );
        record.clear(f);
        node.clear(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        if let Some(outer) = outer {
            self.emit_helper_close_record(outer, output, &closed, f)?;
            output.copy_from(&closed, f);
        }
        closed.clear(f);
        cursor.clear(f);
        Ok(())
    }
    fn emit_zip_acquisition_abrupt(
        &mut self,
        head: &GcLocal<IteratorZipAcquisition, Nullable>,
        outer: Option<&GcLocal<IteratorRecord>>,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_zip_is_throw(pending, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_zip_close_acquisitions(head, outer, pending, output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_zip_mode_and_padding(
        &mut self,
        options: &ValueLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(GcI32DomainLocal<IteratorZipMode>, ValueLocals), EmitError> {
        let s = self.runtime_schema();
        let mode = GcI32DomainLocal::new(s, IteratorZipMode::Shortest, f);
        let padding = s.reserve_value_local(f);
        padding.set_undefined(f);
        options.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_heap_object_like_tag_i32(options.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_ZIP_OPTIONS_MUST_BE_AN_OBJECT_OR_UNDEFINED,
            output,
            exit,
            f,
        )?;
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let key = self.emit_helper_key("mode", f)?;
        self.emit_object_read(options, options, &key, &pending, f)?;
        key.clear(f);
        self.emit_helper_abrupt_exit(&pending, output, exit, f);
        pending.value().tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        pending.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_ZIP_MODE_MUST_BE_A_STRING_OR_UNDEFINED,
            output,
            exit,
            f,
        )?;
        let text = s
            .reserve_gc_local(f)
            .initialize(pending.value().cast_reference::<StringValue>(s, f), f);
        let matched = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        matched.store(f);
        for (name, constant) in [
            ("shortest", IteratorZipMode::Shortest),
            ("longest", IteratorZipMode::Longest),
            ("strict", IteratorZipMode::Strict),
        ] {
            let candidate = s
                .reserve_gc_local(f)
                .initialize(self.emit_interned_string_reference(name, f)?, f);
            self.emit_string_payload_equality_i32(&text, &candidate, f);
            self.open_frame(ControlFrameKind::If, f);
            mode.set_constant(constant, f);
            f.instruction(&Instruction::I32Const(1));
            matched.store(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            candidate.clear(f);
        }
        matched.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_ZIP_MODE_MUST_BE_SHORTEST_LONGEST_OR_STRICT,
            output,
            exit,
            f,
        )?;
        s.release_i32_local(matched, f);
        text.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        mode.load(f);
        f.instruction(&Instruction::I32Const(IteratorZipMode::Longest.encode()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let key = self.emit_helper_key("padding", f)?;
        self.emit_object_read(options, options, &key, &pending, f)?;
        key.clear(f);
        self.emit_helper_abrupt_exit(&pending, output, exit, f);
        padding.copy_from(pending.value(), f);
        padding.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_is_heap_object_like_tag_i32(padding.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_ZIP_PADDING_MUST_BE_AN_OBJECT_OR_UNDEFINED,
            output,
            exit,
            f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pending.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok((mode, padding))
    }
    fn emit_zip_collect_iterable(
        &mut self,
        input: &ValueLocals,
        head: &GcLocal<IteratorZipAcquisition, Nullable>,
        count: I32Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let acquired =
            self.emit_zip_acquire_record(input, ZipRecordSource::Iterable, &pending, f)?;
        self.emit_helper_abrupt_exit(&pending, output, exit, f);
        let outer = s
            .reserve_gc_local(f)
            .initialize(acquired.load(s, f).require_non_null(f), f);
        acquired.clear(f);
        let done = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        let finish = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_helper_step(&outer, done, Some(&value), &pending, f)?;
        // A failed outer step is done; only previously collected inner records close.
        self.emit_zip_acquisition_abrupt(head, None, &pending, output, exit, f)?;
        done.load(f);
        self.emit_branch_if_to_target(finish, f);
        let acquired =
            self.emit_zip_acquire_record(&value, ZipRecordSource::Flattenable, &pending, f)?;
        // Failed inner acquisition closes the collected inners, then the outer input.
        self.emit_zip_acquisition_abrupt(head, Some(&outer), &pending, output, exit, f)?;
        let record = s
            .reserve_gc_local(f)
            .initialize(acquired.load(s, f).require_non_null(f), f);
        acquired.clear(f);
        self.emit_zip_append_acquisition(head, count, &record, None, f);
        record.clear(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.clear(f);
        s.release_i32_local(done, f);
        outer.clear(f);
        pending.clear(f);
        Ok(())
    }
    fn emit_zip_collect_keyed(
        &mut self,
        input: &ValueLocals,
        head: &GcLocal<IteratorZipAcquisition, Nullable>,
        count: I32Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // OwnKeys runs before any acquisition, so this owner's direct abrupt route is safe.
        let keys = self.emit_object_own_property_keys(input, f)?;
        let length = s.reserve_i32_local(f);
        keys.length(length, s, f);
        let index = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        index.store(f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let descriptor = s.reserve_value_local(f);
        let value = s.reserve_value_local(f);
        let finish = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(finish, f);
        let key = keys.read_key(index, self, f)?;
        self.emit_native_object_algorithm_call(
            NativeObjectAlgorithm::GetOwnPropertyDescriptor,
            &[input, key.value()],
            &pending,
            f,
        )?;
        self.emit_zip_acquisition_abrupt(head, None, &pending, output, exit, f)?;
        pending.value().tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        descriptor.copy_from(pending.value(), f);
        let enumerable = self.emit_helper_key("enumerable", f)?;
        self.emit_object_read(&descriptor, &descriptor, &enumerable, &pending, f)?;
        enumerable.clear(f);
        self.emit_zip_acquisition_abrupt(head, None, &pending, output, exit, f)?;
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        self.emit_object_read(input, input, &key, &pending, f)?;
        self.emit_zip_acquisition_abrupt(head, None, &pending, output, exit, f)?;
        value.copy_from(pending.value(), f);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        let acquired =
            self.emit_zip_acquire_record(&value, ZipRecordSource::Flattenable, &pending, f)?;
        self.emit_zip_acquisition_abrupt(head, None, &pending, output, exit, f)?;
        let record = s
            .reserve_gc_local(f)
            .initialize(acquired.load(s, f).require_non_null(f), f);
        acquired.clear(f);
        self.emit_zip_append_acquisition(head, count, &record, Some(&key), f);
        record.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        key.clear(f);
        self.emit_zip_increment(index, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        value.clear(f);
        descriptor.clear(f);
        pending.clear(f);
        s.release_i32_local(index, f);
        s.release_i32_local(length, f);
        keys.clear(f);
        Ok(())
    }
    fn emit_zip_key_snapshot(
        &mut self,
        head: &GcLocal<IteratorZipAcquisition, Nullable>,
        count: I32Local,
        f: &mut Function,
    ) -> Result<GcLocal<PropertyKeyTable>, EmitError> {
        let s = self.runtime_schema();
        let construction = PropertyKeyConstruction::allocate(s, s.reserve_gc_local(f), count, f);
        let cursor = s.reserve_gc_local(f).initialize(head.load(s, f), f);
        let index = s.reserve_i32_local(f);
        count.load(f);
        index.store(f);
        let value = s.reserve_value_local(f);
        let finish = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(s, f).is_null(f);
        self.emit_branch_if_to_target(finish, f);
        self.emit_zip_decrement(index, f);
        let node = s
            .reserve_gc_local(f)
            .initialize(cursor.load(s, f).require_non_null(f), f);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(IteratorZipAcquisitionSchema::KEY)
                .read(&node, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &value, s, f);
        let key = self.emit_value_to_property_key_locals(&value, f)?;
        construction.write(index, &key, s, f);
        key.clear(f);
        cursor.replace(
            s.field(IteratorZipAcquisitionSchema::PREVIOUS)
                .read(&node, s, f)
                .reference(),
            f,
        );
        stored.clear(f);
        node.clear(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let keys = s
            .reserve_gc_local(f)
            .initialize(construction.publish(s, f), f);
        value.clear(f);
        s.release_i32_local(index, f);
        cursor.clear(f);
        Ok(keys)
    }
    fn emit_zip_read_key(
        &mut self,
        keys: &GcLocal<PropertyKeyTable>,
        index: I32Local,
        f: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let s = self.runtime_schema();
        let stored = s.reserve_gc_local(f).initialize(
            s.array_type::<PropertyKeyTable>()
                .read(keys, index, s, f)
                .reference(),
            f,
        );
        let value = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &value, s, f);
        let key = self.emit_value_to_property_key_locals(&value, f)?;
        value.clear(f);
        stored.clear(f);
        Ok(key)
    }
    fn emit_zip_padding_list(
        &mut self,
        kind: IteratorZipInput,
        padding: &ValueLocals,
        keys: &GcLocal<PropertyKeyTable, Nullable>,
        head: &GcLocal<IteratorZipAcquisition, Nullable>,
        count: I32Local,
        output: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<ValueArray>, EmitError> {
        let s = self.runtime_schema();
        let list = ArgumentListConstruction::new(self, f);
        let index = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        index.store(f);
        let value = s.reserve_value_local(f);
        value.set_undefined(f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let record = s
            .reserve_gc_local::<IteratorRecord, Nullable>(f)
            .initialize_null(s, f);
        let using = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        using.store(f);
        padding.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        match kind {
            IteratorZipInput::Iterable => {
                let acquired =
                    self.emit_zip_acquire_record(padding, ZipRecordSource::Iterable, &pending, f)?;
                self.emit_zip_acquisition_abrupt(head, None, &pending, output, exit, f)?;
                record.replace(acquired.load(s, f), f);
                acquired.clear(f);
                f.instruction(&Instruction::I32Const(1));
                using.store(f);
            }
            IteratorZipInput::Keyed => {
                f.instruction(&Instruction::I32Const(1));
                using.store(f);
            }
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let finish = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(finish, f);
        value.set_undefined(f);
        using.load(f);
        self.open_frame(ControlFrameKind::If, f);
        match kind {
            IteratorZipInput::Iterable => {
                let iterator = s
                    .reserve_gc_local(f)
                    .initialize(record.load(s, f).require_non_null(f), f);
                let done = s.reserve_i32_local(f);
                self.emit_helper_step(&iterator, done, Some(&value), &pending, f)?;
                self.emit_zip_acquisition_abrupt(head, None, &pending, output, exit, f)?;
                done.load(f);
                f.instruction(&Instruction::I32Eqz);
                using.store(f);
                s.release_i32_local(done, f);
                iterator.clear(f);
            }
            IteratorZipInput::Keyed => {
                let table = s
                    .reserve_gc_local(f)
                    .initialize(keys.load(s, f).require_non_null(f), f);
                let key = self.emit_zip_read_key(&table, index, f)?;
                self.emit_object_read(padding, padding, &key, &pending, f)?;
                key.clear(f);
                table.clear(f);
                self.emit_zip_acquisition_abrupt(head, None, &pending, output, exit, f)?;
                value.copy_from(pending.value(), f);
            }
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        list.append(&value, s, f);
        self.emit_zip_increment(index, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        if let IteratorZipInput::Iterable = kind {
            using.load(f);
            self.open_frame(ControlFrameKind::If, f);
            let iterator = s
                .reserve_gc_local(f)
                .initialize(record.load(s, f).require_non_null(f), f);
            let normal = s.reserve_completion(f);
            normal.initialize(f);
            self.emit_helper_close_record(&iterator, &normal, &pending, f)?;
            self.emit_zip_acquisition_abrupt(head, None, &pending, output, exit, f)?;
            normal.clear(f);
            iterator.clear(f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        let result = list.finish(self, f);
        s.release_i32_local(using, f);
        record.clear(f);
        pending.clear(f);
        value.clear(f);
        s.release_i32_local(index, f);
        Ok(result)
    }
    fn emit_zip_entries(
        &mut self,
        head: &GcLocal<IteratorZipAcquisition, Nullable>,
        count: I32Local,
        padding: &GcLocal<ValueArray>,
        f: &mut Function,
    ) -> GcLocal<IteratorZipEntries> {
        let s = self.runtime_schema();
        let entries = s.reserve_gc_local(f).initialize(
            s.array_type::<IteratorZipEntries>()
                .fixed(std::iter::empty(), f),
            f,
        );
        count.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let first = s
            .reserve_gc_local(f)
            .initialize(head.load(s, f).require_non_null(f), f);
        let record = s.reserve_gc_local(f).initialize(
            s.field(IteratorZipAcquisitionSchema::RECORD)
                .read(&first, s, f)
                .reference(),
            f,
        );
        let value = s.reserve_value_local(f);
        value.set_undefined(f);
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&value, f), f);
        let seed = s.reserve_gc_local(f).initialize(
            s.struct_type::<IteratorZipEntry>().construct(
                (
                    GcOperand::reference(&record, s),
                    GcOperand::boolean(true),
                    GcOperand::reference(&stored, s),
                ),
                f,
            ),
            f,
        );
        let construction = IteratorZipEntriesConstruction::allocate(s, &seed, count, f);
        seed.clear(f);
        stored.clear(f);
        record.clear(f);
        first.clear(f);
        let cursor = s.reserve_gc_local(f).initialize(head.load(s, f), f);
        let index = s.reserve_i32_local(f);
        count.load(f);
        index.store(f);
        let finish = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(s, f).is_null(f);
        self.emit_branch_if_to_target(finish, f);
        self.emit_zip_decrement(index, f);
        let node = s
            .reserve_gc_local(f)
            .initialize(cursor.load(s, f).require_non_null(f), f);
        let record = s.reserve_gc_local(f).initialize(
            s.field(IteratorZipAcquisitionSchema::RECORD)
                .read(&node, s, f)
                .reference(),
            f,
        );
        self.emit_argument_vector_entry_to_value(padding, index, &value, f);
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&value, f), f);
        let entry = s.reserve_gc_local(f).initialize(
            s.struct_type::<IteratorZipEntry>().construct(
                (
                    GcOperand::reference(&record, s),
                    GcOperand::boolean(true),
                    GcOperand::reference(&stored, s),
                ),
                f,
            ),
            f,
        );
        construction.write(index, &entry, s, f);
        cursor.replace(
            s.field(IteratorZipAcquisitionSchema::PREVIOUS)
                .read(&node, s, f)
                .reference(),
            f,
        );
        entry.clear(f);
        stored.clear(f);
        record.clear(f);
        node.clear(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        entries.replace(construction.publish(s, f), f);
        s.release_i32_local(index, f);
        cursor.clear(f);
        value.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        entries
    }
    pub(in crate::builtins::standard) fn emit_native_iterator_zip_create(
        &mut self,
        kind: IteratorZipInput,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let input = s.reserve_value_local(f);
        let options = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &input, f);
        self.emit_builtin_arg_to_value(1, &options, f);
        let output = s.reserve_completion(f);
        output.initialize(f);
        let head = s
            .reserve_gc_local::<IteratorZipAcquisition, Nullable>(f)
            .initialize_null(s, f);
        let count = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        count.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        self.emit_is_heap_object_like_tag_i32(input.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_helper_type_error_if(
            RuntimeErrorMessage::ITERATOR_ZIP_CALLED_WITH_A_NON_OBJECT_ITERABLES_VALUE,
            &output,
            exit,
            f,
        )?;
        let (mode, padding) = self.emit_zip_mode_and_padding(&options, &output, exit, f)?;
        match kind {
            IteratorZipInput::Iterable => {
                self.emit_zip_collect_iterable(&input, &head, count, &output, exit, f)?
            }
            IteratorZipInput::Keyed => {
                self.emit_zip_collect_keyed(&input, &head, count, &output, exit, f)?
            }
        }
        let keys = s
            .reserve_gc_local::<PropertyKeyTable, Nullable>(f)
            .initialize_null(s, f);
        if let IteratorZipInput::Keyed = kind {
            let complete = self.emit_zip_key_snapshot(&head, count, f)?;
            keys.replace(complete.load(s, f).nullable(), f);
            complete.clear(f);
        }
        let values =
            self.emit_zip_padding_list(kind, &padding, &keys, &head, count, &output, exit, f)?;
        let entries = self.emit_zip_entries(&head, count, &values, f);
        values.clear(f);
        let realm = s
            .reserve_gc_local(f)
            .initialize(self.emit_current_function_realm(f), f);
        let header = self.emit_helper_header(&realm, f)?;
        let helper = s.reserve_gc_local(f).initialize(
            s.struct_type::<IteratorZipHelper>().construct(
                (
                    GcOperand::reference(&header, s),
                    GcOperand::reference(&entries, s),
                    GcOperand::reference(&keys, s),
                    mode.operand(),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::boolean(false),
                    GcOperand::reference(&realm, s),
                ),
                f,
            ),
            f,
        );
        output.value().set_reference(&helper, s, f);
        output.set_kind(CompletionKind::Normal, f);
        helper.clear(f);
        header.clear(f);
        realm.clear(f);
        entries.clear(f);
        keys.clear(f);
        padding.clear(f);
        mode.clear(s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        s.release_i32_local(count, f);
        head.clear(f);
        output.clear(f);
        options.clear(f);
        input.clear(f);
        Ok(())
    }

    /// OPEN is the private copy of openIters; removal precedes every close or
    /// completed/failed step. CloseAll preserves the first whole Throw.
    fn emit_zip_close_entries(
        &mut self,
        entries: &GcLocal<IteratorZipEntries>,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        output.copy_from(pending, f);
        let index = s.reserve_i32_local(f);
        s.array_type::<IteratorZipEntries>().length(entries, s, f);
        index.store(f);
        let open = s.reserve_i32_local(f);
        let closed = s.reserve_completion(f);
        closed.initialize(f);
        let finish = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(finish, f);
        self.emit_zip_decrement(index, f);
        let entry = s.reserve_gc_local(f).initialize(
            s.array_type::<IteratorZipEntries>()
                .read(entries, index, s, f)
                .reference(),
            f,
        );
        s.field(IteratorZipEntrySchema::OPEN)
            .read(&entry, s, f)
            .store(open, f);
        open.load(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(IteratorZipEntrySchema::OPEN)
            .write(&entry, GcOperand::boolean(false), s, f);
        let record = s.reserve_gc_local(f).initialize(
            s.field(IteratorZipEntrySchema::RECORD)
                .read(&entry, s, f)
                .reference(),
            f,
        );
        self.emit_helper_close_record(&record, output, &closed, f)?;
        output.copy_from(&closed, f);
        record.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        entry.clear(f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        closed.clear(f);
        s.release_i32_local(open, f);
        s.release_i32_local(index, f);
        Ok(())
    }
    fn emit_zip_step_abrupt(
        &mut self,
        entry: &GcLocal<IteratorZipEntry>,
        entries: &GcLocal<IteratorZipEntries>,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        stop: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.emit_zip_is_throw(pending, f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(IteratorZipEntrySchema::OPEN)
            .write(entry, GcOperand::boolean(false), s, f);
        self.emit_zip_close_entries(entries, pending, output, f)?;
        self.emit_branch_to_target(stop, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_zip_finish(
        &mut self,
        entries: &GcLocal<IteratorZipEntries>,
        output: &CompletionLocals,
        stop: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        self.emit_zip_close_entries(entries, &pending, output, f)?;
        self.emit_zip_is_throw(output, f);
        self.emit_branch_if_to_target(stop, f);
        self.emit_helper_done_result(output, f)?;
        self.emit_branch_to_target(stop, f);
        pending.clear(f);
        Ok(())
    }
    fn emit_zip_strict_finish(
        &mut self,
        entries: &GcLocal<IteratorZipEntries>,
        index: I32Local,
        count: I32Local,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        stop: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        index.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ITERATOR_ZIP_STRICT_MODE_HAS_ITERATORS_OF_DIFFERENT_LENGTHS,
            pending,
            f,
        )?;
        self.emit_zip_close_entries(entries, pending, output, f)?;
        self.emit_branch_to_target(stop, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let probe = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(1));
        probe.store(f);
        let done = s.reserve_i32_local(f);
        let finish = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        probe.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(finish, f);
        let entry = s.reserve_gc_local(f).initialize(
            s.array_type::<IteratorZipEntries>()
                .read(entries, probe, s, f)
                .reference(),
            f,
        );
        let record = s.reserve_gc_local(f).initialize(
            s.field(IteratorZipEntrySchema::RECORD)
                .read(&entry, s, f)
                .reference(),
            f,
        );
        // Strict completion checks IteratorStep, not IteratorStepValue.
        self.emit_helper_step(&record, done, None, pending, f)?;
        self.emit_zip_step_abrupt(&entry, entries, pending, output, stop, f)?;
        done.load(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(IteratorZipEntrySchema::OPEN)
            .write(&entry, GcOperand::boolean(false), s, f);
        f.instruction(&Instruction::Else);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ITERATOR_ZIP_STRICT_MODE_HAS_ITERATORS_OF_DIFFERENT_LENGTHS,
            pending,
            f,
        )?;
        self.emit_zip_close_entries(entries, pending, output, f)?;
        self.emit_branch_to_target(stop, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        record.clear(f);
        entry.clear(f);
        self.emit_zip_increment(probe, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(done, f);
        s.release_i32_local(probe, f);
        self.emit_helper_done_result(output, f)?;
        self.emit_branch_to_target(stop, f);
        Ok(())
    }
    fn emit_zip_finish_row(
        &mut self,
        keys: &GcLocal<PropertyKeyTable, Nullable>,
        values: &GcLocal<ValueArray>,
        count: I32Local,
        output: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        keys.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let row = self.emit_array_from_argument_list(values, f)?;
        output.set_reference(&row, s, f);
        row.clear(f);
        f.instruction(&Instruction::Else);
        let prototype = s.reserve_value_local(f);
        prototype.set_scalar(ScalarValue::Null, f);
        let row = s.reserve_gc_local(f).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), f)?,
            f,
        );
        prototype.clear(f);
        let table = s
            .reserve_gc_local(f)
            .initialize(keys.load(s, f).require_non_null(f), f);
        let index = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        index.store(f);
        let value = s.reserve_value_local(f);
        let finish = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(finish, f);
        let key = self.emit_zip_read_key(&table, index, f)?;
        self.emit_argument_vector_entry_to_value(values, index, &value, f);
        self.emit_object_append_data_property_with_flags(&row, &key, &value, true, true, true, f)?;
        key.clear(f);
        self.emit_zip_increment(index, f);
        self.emit_branch_to_target(next, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        output.set_reference(&row, s, f);
        value.clear(f);
        s.release_i32_local(index, f);
        table.clear(f);
        row.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    pub(in crate::builtins::standard) fn emit_native_iterator_zip_resume(
        &mut self,
        operation: IteratorHelperOperation,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let receiver = s.reserve_value_local(f);
        self.compile_this_to_locals(&receiver, f)?;
        let output = s.reserve_completion(f);
        output.initialize(f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let flag = s.reserve_i32_local(f);
        let done = s.reserve_i32_local(f);
        let count = s.reserve_i32_local(f);
        let index = s.reserve_i32_local(f);
        let active = s.reserve_i32_local(f);
        let yielded = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        f.instruction(&Instruction::I32Const(0));
        yielded.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        HelperKind::Zip.test(&receiver, s, f);
        f.instruction(&Instruction::I32Eqz);
        let message = match operation {
            IteratorHelperOperation::Next => {
                RuntimeErrorMessage::ITERATOR_ZIP_HELPER_NEXT_CALLED_ON_INCOMPATIBLE_RECEIVER
            }
            IteratorHelperOperation::Return => {
                RuntimeErrorMessage::ITERATOR_ZIP_HELPER_RETURN_CALLED_ON_INCOMPATIBLE_RECEIVER
            }
        };
        self.emit_helper_type_error_if(message, &output, exit, f)?;
        let helper = s
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<IteratorZipHelper>(s, f), f);
        let state = HelperState::Zip(s.reserve_gc_local(f).initialize(helper.load(s, f), f));
        state.read_flag(HelperFlag::Executing, flag, s, f);
        flag.load(f);
        self.emit_helper_type_error_if(HelperKind::Zip.running_message(), &output, exit, f)?;
        state.read_flag(HelperFlag::Done, flag, s, f);
        flag.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_helper_done_result(&output, f)?;
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let entries = s.reserve_gc_local(f).initialize(
            s.field(IteratorZipHelperSchema::ENTRIES)
                .read(&helper, s, f)
                .reference(),
            f,
        );
        let keys = s.reserve_gc_local(f).initialize(
            s.field(IteratorZipHelperSchema::KEYS)
                .read(&helper, s, f)
                .reference(),
            f,
        );
        let mode = GcI32DomainLocal::new(s, IteratorZipMode::Shortest, f);
        s.field(IteratorZipHelperSchema::MODE)
            .read(&helper, s, f)
            .store_domain(&mode, f);
        s.array_type::<IteratorZipEntries>().length(&entries, s, f);
        count.store(f);
        let stop = self.open_frame(ControlFrameKind::Block, f);
        match operation {
            IteratorHelperOperation::Return => {
                state.read_flag(HelperFlag::Started, flag, s, f);
                flag.load(f);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Executing, true, s, f);
                f.instruction(&Instruction::Else);
                state.write_flag(HelperFlag::Done, true, s, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                self.emit_zip_finish(&entries, &output, stop, f)?;
            }
            IteratorHelperOperation::Next => {
                state.write_flag(HelperFlag::Executing, true, s, f);
                state.write_flag(HelperFlag::Started, true, s, f);
                f.instruction(&Instruction::I32Const(0));
                active.store(f);
                f.instruction(&Instruction::I32Const(0));
                index.store(f);
                let counted = self.open_frame(ControlFrameKind::Block, f);
                let scan = self.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                count.load(f);
                f.instruction(&Instruction::I32GeU);
                self.emit_branch_if_to_target(counted, f);
                let entry = s.reserve_gc_local(f).initialize(
                    s.array_type::<IteratorZipEntries>()
                        .read(&entries, index, s, f)
                        .reference(),
                    f,
                );
                s.field(IteratorZipEntrySchema::OPEN)
                    .read(&entry, s, f)
                    .store(flag, f);
                active.load(f);
                flag.load(f);
                f.instruction(&Instruction::I32Add);
                active.store(f);
                entry.clear(f);
                self.emit_zip_increment(index, f);
                self.emit_branch_to_target(scan, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                active.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_helper_done_result(&output, f)?;
                self.emit_branch_to_target(stop, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                let list = ArgumentListConstruction::new(self, f);
                f.instruction(&Instruction::I32Const(0));
                index.store(f);
                let collected = self.open_frame(ControlFrameKind::Block, f);
                let next = self.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                count.load(f);
                f.instruction(&Instruction::I32GeU);
                self.emit_branch_if_to_target(collected, f);
                let entry = s.reserve_gc_local(f).initialize(
                    s.array_type::<IteratorZipEntries>()
                        .read(&entries, index, s, f)
                        .reference(),
                    f,
                );
                s.field(IteratorZipEntrySchema::OPEN)
                    .read(&entry, s, f)
                    .store(flag, f);
                let padding = s.reserve_gc_local(f).initialize(
                    s.field(IteratorZipEntrySchema::PADDING)
                        .read(&entry, s, f)
                        .reference(),
                    f,
                );
                s.struct_type::<StoredValue>()
                    .read_into(&padding, &value, s, f);
                padding.clear(f);
                flag.load(f);
                self.open_frame(ControlFrameKind::If, f);
                let record = s.reserve_gc_local(f).initialize(
                    s.field(IteratorZipEntrySchema::RECORD)
                        .read(&entry, s, f)
                        .reference(),
                    f,
                );
                self.emit_helper_step(&record, done, Some(&value), &pending, f)?;
                self.emit_zip_step_abrupt(&entry, &entries, &pending, &output, stop, f)?;
                done.load(f);
                self.open_frame(ControlFrameKind::If, f);
                s.field(IteratorZipEntrySchema::OPEN).write(
                    &entry,
                    GcOperand::boolean(false),
                    s,
                    f,
                );
                self.emit_zip_decrement(active, f);
                mode.load(f);
                f.instruction(&Instruction::I32Const(IteratorZipMode::Shortest.encode()));
                f.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_zip_finish(&entries, &output, stop, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                mode.load(f);
                f.instruction(&Instruction::I32Const(IteratorZipMode::Strict.encode()));
                f.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_zip_strict_finish(&entries, index, count, &pending, &output, stop, f)?;
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                // Longest terminates without yielding an all-padding final row.
                active.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                self.emit_helper_done_result(&output, f)?;
                self.emit_branch_to_target(stop, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                let padding = s.reserve_gc_local(f).initialize(
                    s.field(IteratorZipEntrySchema::PADDING)
                        .read(&entry, s, f)
                        .reference(),
                    f,
                );
                s.struct_type::<StoredValue>()
                    .read_into(&padding, &value, s, f);
                padding.clear(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                record.clear(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                list.append(&value, s, f);
                entry.clear(f);
                self.emit_zip_increment(index, f);
                self.emit_branch_to_target(next, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
                let values = list.finish(self, f);
                self.emit_zip_finish_row(&keys, &values, count, &value, f)?;
                values.clear(f);
                self.emit_iterator_result_object_from_locals(&value, false, &output, f)?;
                f.instruction(&Instruction::I32Const(1));
                yielded.store(f);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        state.write_flag(HelperFlag::Executing, false, s, f);
        match operation {
            IteratorHelperOperation::Return => state.write_flag(HelperFlag::Done, true, s, f),
            IteratorHelperOperation::Next => {
                // Only a completed Yield keeps the helper resumable. Terminal
                // completion is a private lifecycle fact, not another result Get.
                yielded.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                state.write_flag(HelperFlag::Done, true, s, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        mode.clear(s, f);
        keys.clear(f);
        entries.clear(f);
        state.clear(f);
        helper.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().copy_from(&output, f);
        value.clear(f);
        s.release_i32_local(yielded, f);
        s.release_i32_local(active, f);
        s.release_i32_local(index, f);
        s.release_i32_local(count, f);
        s.release_i32_local(done, f);
        s.release_i32_local(flag, f);
        pending.clear(f);
        output.clear(f);
        receiver.clear(f);
        Ok(())
    }
}
