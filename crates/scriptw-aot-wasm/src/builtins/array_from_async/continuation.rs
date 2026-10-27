use super::*;

/// Only the stage dispatch may choose which already-rooted value transition
/// runs next. The frames are consumed in map-then-commit order.
enum FulfilledValueRoute {
    Map,
    Commit,
}

struct FulfilledDispatchScope {
    map: ControlTarget,
    commit: ControlTarget,
}

struct FulfilledCommitScope;

impl FulfilledDispatchScope {
    fn open(builder: &mut FunctionBuilder<'_>, f: &mut Function) -> Self {
        let commit = builder.open_frame(ControlFrameKind::Block, f);
        let map = builder.open_frame(ControlFrameKind::Block, f);
        Self { map, commit }
    }

    fn route(&self, route: FulfilledValueRoute, builder: &FunctionBuilder<'_>, f: &mut Function) {
        let target = match route {
            FulfilledValueRoute::Map => self.map,
            FulfilledValueRoute::Commit => self.commit,
        };
        builder.emit_branch_to_target(target, f);
    }

    fn into_mapping(
        self,
        builder: &mut FunctionBuilder<'_>,
        f: &mut Function,
    ) -> FulfilledCommitScope {
        builder.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        FulfilledCommitScope
    }
}

impl FulfilledCommitScope {
    fn into_commit(self, builder: &mut FunctionBuilder<'_>, f: &mut Function) {
        builder.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_array_from_async_fulfilled(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let state = self.emit_af_captured_state(f);
        let value = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &value, f);
        self.completion().initialize(f);
        let stage = GcI32DomainLocal::new(s, ArrayFromAsyncStage::InputValue, f);
        s.struct_type::<ArrayFromAsyncState>()
            .field(ArrayFromAsyncStateSchema::STAGE)
            .read(&state, s, f)
            .store_domain(&stage, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let dispatch = FulfilledDispatchScope::open(self, f);
        // Exhaustive Rust dispatch defines every encoded continuation outcome.
        for current in [
            ArrayFromAsyncStage::InputValue,
            ArrayFromAsyncStage::MappedValue,
            ArrayFromAsyncStage::AsyncIteratorResult,
            ArrayFromAsyncStage::AsyncCloseResult,
        ] {
            stage.load(f);
            f.instruction(&Instruction::I32Const(GcI32Constant::encode(current)));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            match current {
                ArrayFromAsyncStage::InputValue => {
                    dispatch.route(FulfilledValueRoute::Map, self, f);
                }
                ArrayFromAsyncStage::MappedValue => {
                    dispatch.route(FulfilledValueRoute::Commit, self, f);
                }
                ArrayFromAsyncStage::AsyncIteratorResult => {
                    self.emit_af_iterator_value(&state, &value, exit, f)?;
                    dispatch.route(FulfilledValueRoute::Map, self, f);
                }
                ArrayFromAsyncStage::AsyncCloseResult => {
                    self.emit_af_reject_saved(&state, exit, f)?
                }
            }
            self.emit_branch_to_target(exit, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::Unreachable);
        let commit = dispatch.into_mapping(self, f);
        self.emit_af_map_or_await(&state, &value, exit, f)?;
        commit.into_commit(self, f);
        self.emit_af_commit(&state, &value, exit, f)?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().initialize(f);
        stage.clear(s, f);
        value.clear(f);
        state.clear(f);
        Ok(())
    }
    pub(crate) fn emit_array_from_async_rejected(
        &mut self,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let state = self.emit_af_captured_state(f);
        let error = s.reserve_value_local(f);
        self.emit_builtin_arg_to_value(0, &error, f);
        self.completion().initialize(f);
        let stage = GcI32DomainLocal::new(s, ArrayFromAsyncStage::InputValue, f);
        s.struct_type::<ArrayFromAsyncState>()
            .field(ArrayFromAsyncStateSchema::STAGE)
            .read(&state, s, f)
            .store_domain(&stage, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        for current in [
            ArrayFromAsyncStage::InputValue,
            ArrayFromAsyncStage::MappedValue,
            ArrayFromAsyncStage::AsyncIteratorResult,
            ArrayFromAsyncStage::AsyncCloseResult,
        ] {
            stage.load(f);
            f.instruction(&Instruction::I32Const(GcI32Constant::encode(current)));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            match current {
                ArrayFromAsyncStage::MappedValue => {
                    self.emit_af_close_or_reject(&state, &error, exit, f)?
                }
                ArrayFromAsyncStage::AsyncCloseResult => {
                    self.emit_af_reject_saved(&state, exit, f)?
                }
                ArrayFromAsyncStage::InputValue | ArrayFromAsyncStage::AsyncIteratorResult => {
                    self.emit_af_reject_value(&state, &error, exit, f)?
                }
            }
            self.emit_branch_to_target(exit, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.completion().initialize(f);
        stage.clear(s, f);
        error.clear(f);
        state.clear(f);
        Ok(())
    }

    pub(super) fn emit_af_drive(
        &mut self,
        state: &GcLocal<ArrayFromAsyncState>,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let mode = self.emit_af_mode(state, f);
        let index = self.emit_af_index(state, f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let capability = self.emit_af_capability(state, f);
        let value = s.reserve_value_local(f);
        mode.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(
            ArrayFromAsyncSourceMode::ArrayLike,
        )));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let length = s.reserve_i64_local(f);
        s.struct_type::<ArrayFromAsyncState>()
            .field(ArrayFromAsyncStateSchema::LENGTH)
            .read(state, s, f)
            .store_i64(length, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_af_finish(state, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let source = s.reserve_value_local(f);
        self.emit_af_value(state, StateValue::Source, &source, f);
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_object_read(&source, &source, &key, &pending, f)?;
        key.clear(f);
        source.clear(f);
        self.emit_af_reject_abrupt(&capability, &pending, exit, f)?;
        value.copy_from(pending.value(), f);
        self.emit_af_await(state, AwaitPhase::Input, &value, f)?;
        self.emit_af_await_abrupt(state, AwaitPhase::Input, exit, f)?;
        s.release_i64_local(length, f);
        f.instruction(&Instruction::Else);
        index.load(f);
        f.instruction(&Instruction::I64Const(9_007_199_254_740_991));
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ARRAY_FROMASYNC_ITERATOR_PRODUCED_TOO_MANY_VALUES,
            &pending,
            f,
        )?;
        self.emit_af_close_or_reject(state, pending.value(), exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let record = self.emit_af_record(state, f);
        mode.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(
            ArrayFromAsyncSourceMode::SyncIterator,
        )));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let realm = self.emit_execution_realm(f);
        let promise = self.emit_async_from_sync_iterator_method(
            &record,
            AsyncFromSyncIteratorMethod::Next,
            None,
            &realm,
            f,
        )?;
        value.set_reference(&promise, s, f);
        promise.clear(f);
        realm.clear(f);
        f.instruction(&Instruction::Else);
        let receiver = s.reserve_value_local(f);
        let next = s.reserve_value_local(f);
        let iterator_stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::ITERATOR)
                .read(&record, s, f)
                .reference(),
            f,
        );
        let next_stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::NEXT_METHOD)
                .read(&record, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&iterator_stored, &receiver, s, f);
        s.struct_type::<StoredValue>()
            .read_into(&next_stored, &next, s, f);
        let argv = self.emit_pre_evaluated_arg_vector(&[], f);
        self.emit_function_or_proxy_call_with_argv(&next, &receiver, &argv, &pending, f)?;
        argv.clear(f);
        next_stored.clear(f);
        iterator_stored.clear(f);
        next.clear(f);
        receiver.clear(f);
        self.emit_af_reject_abrupt(&capability, &pending, exit, f)?;
        value.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_af_await(state, AwaitPhase::Iterator, &value, f)?;
        self.emit_af_await_abrupt(state, AwaitPhase::Iterator, exit, f)?;
        record.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.clear(f);
        capability.clear(f);
        pending.clear(f);
        s.release_i64_local(index, f);
        mode.clear(s, f);
        // Await is the only continuation; no further source Get runs here.
        self.emit_branch_to_target(exit, f);
        Ok(())
    }

    fn emit_af_iterator_value(
        &mut self,
        state: &GcLocal<ArrayFromAsyncState>,
        result: &ValueLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let capability = self.emit_af_capability(state, f);
        self.emit_is_heap_object_like_tag_i32(result.tag(), f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::ARRAY_FROMASYNC_ITERATOR_NEXT_RESULT_MUST_BE_OBJECT,
            &pending,
            f,
        )?;
        self.emit_af_reject_abrupt(&capability, &pending, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_af_result_get(
            result,
            ArrayFromAsyncIteratorResultProperty::Done,
            &pending,
            f,
        )?;
        self.emit_af_reject_abrupt(&capability, &pending, exit, f)?;
        self.compile_truthy_tagged_i32(pending.value(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        self.emit_af_finish(state, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_af_result_get(
            result,
            ArrayFromAsyncIteratorResultProperty::Value,
            &pending,
            f,
        )?;
        self.emit_af_reject_abrupt(&capability, &pending, exit, f)?;
        // No further read of the result object follows the successful value
        // Get. Replace its rooted value only now; async iterator values are
        // retained as-is, because their result alone was awaited.
        result.copy_from(pending.value(), f);
        capability.clear(f);
        pending.clear(f);
        Ok(())
    }
    fn emit_af_map_or_await(
        &mut self,
        state: &GcLocal<ArrayFromAsyncState>,
        value: &ValueLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let mapper = s.reserve_value_local(f);
        self.emit_af_value(state, StateValue::Mapper, &mapper, f);
        mapper.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        let this_argument = s.reserve_value_local(f);
        self.emit_af_value(state, StateValue::ThisArgument, &this_argument, f);
        let index = self.emit_af_index(state, f);
        let number = s.reserve_value_local(f);
        self.emit_af_number(index, &number, f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let argv = self.emit_pre_evaluated_arg_vector(&[value, &number], f);
        self.emit_function_or_proxy_call_with_argv(&mapper, &this_argument, &argv, &pending, f)?;
        argv.clear(f);
        self.emit_af_close_abrupt(state, &pending, exit, f)?;
        self.emit_af_await(state, AwaitPhase::Mapper, pending.value(), f)?;
        self.emit_af_await_abrupt(state, AwaitPhase::Mapper, exit, f)?;
        pending.clear(f);
        number.clear(f);
        s.release_i64_local(index, f);
        this_argument.clear(f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // An absent mapper falls through to the shared commit owner.
        mapper.clear(f);
        Ok(())
    }
    fn emit_af_commit(
        &mut self,
        state: &GcLocal<ArrayFromAsyncState>,
        value: &ValueLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let target = s.reserve_value_local(f);
        self.emit_af_value(state, StateValue::Target, &target, f);
        let index = self.emit_af_index(state, f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let key = self.emit_array_native_index_key(index, f)?;
        self.emit_create_data_property_or_throw(&target, &key, value, &pending, f)?;
        key.clear(f);
        self.emit_af_close_abrupt(state, &pending, exit, f)?;
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        index.store(f);
        s.struct_type::<ArrayFromAsyncState>()
            .field(ArrayFromAsyncStateSchema::INDEX)
            .write(state, GcOperand::i64_local(index), s, f);
        pending.clear(f);
        s.release_i64_local(index, f);
        target.clear(f);
        self.emit_af_drive(state, exit, f)
    }
    fn emit_af_finish(
        &mut self,
        state: &GcLocal<ArrayFromAsyncState>,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let target = s.reserve_value_local(f);
        self.emit_af_value(state, StateValue::Target, &target, f);
        let index = self.emit_af_index(state, f);
        let number = s.reserve_value_local(f);
        self.emit_af_number(index, &number, f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        let capability = self.emit_af_capability(state, f);
        let text = s
            .reserve_gc_local(f)
            .initialize(self.emit_interned_string_reference("length", f)?, f);
        let key = PropertyKeyLocals::from_string(s, &text, f);
        self.emit_object_write_strict(&target, &key, &number, &pending, f)?;
        key.clear(f);
        text.clear(f);
        self.emit_af_reject_abrupt(&capability, &pending, exit, f)?;
        self.emit_af_settle(&capability, PromiseSettlement::Fulfill, &target, f)?;
        capability.clear(f);
        pending.clear(f);
        number.clear(f);
        s.release_i64_local(index, f);
        target.clear(f);
        self.emit_branch_to_target(exit, f);
        Ok(())
    }

    fn emit_af_close_abrupt(
        &mut self,
        state: &GcLocal<ArrayFromAsyncState>,
        pending: &CompletionLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_af_close_or_reject(state, pending.value(), exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }
    fn emit_af_await_abrupt(
        &mut self,
        state: &GcLocal<ArrayFromAsyncState>,
        phase: AwaitPhase,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let pending = s.reserve_completion(f);
        pending.copy_from(self.completion(), f);
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        match phase {
            AwaitPhase::Input | AwaitPhase::Iterator => {
                self.emit_af_reject_value(state, pending.value(), exit, f)?
            }
            AwaitPhase::Mapper => self.emit_af_close_or_reject(state, pending.value(), exit, f)?,
            AwaitPhase::Close => self.emit_af_reject_saved(state, exit, f)?,
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        pending.clear(f);
        Ok(())
    }
    fn emit_af_reject_value(
        &mut self,
        state: &GcLocal<ArrayFromAsyncState>,
        error: &ValueLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let capability = self.emit_af_capability(state, f);
        self.emit_af_settle(&capability, PromiseSettlement::Reject, error, f)?;
        capability.clear(f);
        self.emit_branch_to_target(exit, f);
        Ok(())
    }
    fn emit_af_reject_saved(
        &mut self,
        state: &GcLocal<ArrayFromAsyncState>,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let error = s.reserve_value_local(f);
        self.emit_af_saved_error(state, &error, f);
        self.emit_af_reject_value(state, &error, exit, f)?;
        error.clear(f);
        Ok(())
    }
    fn emit_af_close_or_reject(
        &mut self,
        state: &GcLocal<ArrayFromAsyncState>,
        error: &ValueLocals,
        exit: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let mode = self.emit_af_mode(state, f);
        mode.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(
            ArrayFromAsyncSourceMode::ArrayLike,
        )));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_af_reject_value(state, error, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_af_save_error(state, error, f);
        let record = self.emit_af_record(state, f);
        let value = s.reserve_value_local(f);
        let pending = s.reserve_completion(f);
        pending.initialize(f);
        mode.load(f);
        f.instruction(&Instruction::I32Const(GcI32Constant::encode(
            ArrayFromAsyncSourceMode::SyncIterator,
        )));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let realm = self.emit_execution_realm(f);
        let promise = self.emit_async_from_sync_iterator_method(
            &record,
            AsyncFromSyncIteratorMethod::Return,
            None,
            &realm,
            f,
        )?;
        value.set_reference(&promise, s, f);
        promise.clear(f);
        realm.clear(f);
        f.instruction(&Instruction::Else);
        let receiver = s.reserve_value_local(f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::ITERATOR)
                .read(&record, s, f)
                .reference(),
            f,
        );
        s.struct_type::<StoredValue>()
            .read_into(&stored, &receiver, s, f);
        stored.clear(f);
        self.emit_array_native_get(&receiver, "return", &pending, f)?;
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_af_reject_saved(state, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.compile_nullish_tagged_i32(pending.value().tag(), f)?;
        self.open_frame(ControlFrameKind::If, f);
        self.emit_af_reject_saved(state, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let method = s.reserve_value_local(f);
        method.copy_from(pending.value(), f);
        let argv = self.emit_pre_evaluated_arg_vector(&[], f);
        self.emit_function_or_proxy_call_with_argv(&method, &receiver, &argv, &pending, f)?;
        argv.clear(f);
        method.clear(f);
        receiver.clear(f);
        pending.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_af_reject_saved(state, exit, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.copy_from(pending.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_af_await(state, AwaitPhase::Close, &value, f)?;
        self.emit_af_await_abrupt(state, AwaitPhase::Close, exit, f)?;
        pending.clear(f);
        value.clear(f);
        record.clear(f);
        mode.clear(s, f);
        self.emit_branch_to_target(exit, f);
        Ok(())
    }
}
