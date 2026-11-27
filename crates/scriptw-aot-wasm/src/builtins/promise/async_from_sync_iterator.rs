//! Async-from-Sync methods and their intrinsic Promise continuation.
//!
//! The wrapper Promise exists before any underlying protocol observation. Its
//! reaction owns the retained iterator, completion policy and wrapper Realm.

use super::*;
use crate::emit::ControlTarget;

impl FunctionBuilder<'_> {
    fn emit_async_from_sync_property(
        &mut self,
        receiver: &ValueLocals,
        name: &str,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(name, function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &string, function);
        let emitted = self.emit_object_read_with_throw_routing(
            receiver,
            receiver,
            &key,
            result,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        );
        key.clear(function);
        string.clear(function);
        emitted
    }

    fn emit_async_from_sync_type_error(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            &prototype,
            function,
        );
        let emitted = self.emit_throw_runtime_error_with_prototype(
            NativeErrorKind::TypeError,
            message,
            &prototype,
            result,
            function,
        );
        prototype.clear(function);
        emitted
    }

    fn emit_async_from_sync_reject_abrupt(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        result: &CompletionLocals,
        finish: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_settle_promise_record(
            promise,
            PromiseSettlement::Reject,
            result.value(),
            function,
        )?;
        self.emit_branch_to_target(finish, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_async_from_sync_close_rejection(
        &mut self,
        iterator: &ValueLocals,
        done: I32Local,
        close_on_rejection: I32Local,
        pending: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        done.load(function);
        function.instruction(&Instruction::I32Eqz);
        close_on_rejection.load(function);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        // IteratorClose observes return even though the incoming Throw wins.
        self.emit_iterator_close_with_completion(iterator, pending, pending, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_async_from_sync_continuation(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        record: &GcLocal<IteratorRecord>,
        iterator: &ValueLocals,
        sync_result: &ValueLocals,
        realm: &GcLocal<RealmRecord>,
        close_on_rejection: bool,
        pending: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let done = schema.reserve_i32_local(function);
        let close = schema.reserve_i32_local(function);
        let value = schema.reserve_value_local(function);
        function.instruction(&Instruction::I32Const(i32::from(close_on_rejection)));
        close.store(function);
        let finish = self.open_frame(ControlFrameKind::Block, function);
        self.emit_async_from_sync_property(sync_result, "done", pending, function)?;
        self.emit_async_from_sync_reject_abrupt(promise, pending, finish, function)?;
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        done.store(function);
        // AsyncFromSyncIteratorContinuation reads value even when done is true.
        self.emit_async_from_sync_property(sync_result, "value", pending, function)?;
        self.emit_async_from_sync_reject_abrupt(promise, pending, finish, function)?;
        value.copy_from(pending.value(), function);
        let resolve = self.emit_intrinsic_promise_resolve_realm_context(
            PromiseResolveRealmAuthority::ExplicitRealm(realm),
            function,
        );
        self.emit_intrinsic_promise_resolve_to_locals(&resolve, &value, pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_async_from_sync_close_rejection(iterator, done, close, pending, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_async_from_sync_reject_abrupt(promise, pending, finish, function)?;
        let value_wrapper = schema.reserve_gc_local(function).initialize(
            pending
                .value()
                .cast_reference::<PromiseObject>(schema, function),
            function,
        );
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncFromSyncIteratorContinuation>()
                .construct(
                    (
                        GcOperand::reference(promise, schema),
                        GcOperand::reference(record, schema),
                        GcOperand::reference(realm, schema),
                        GcOperand::boolean_local(done),
                        GcOperand::boolean(close_on_rejection),
                    ),
                    function,
                ),
            function,
        );
        self.emit_owned_promise_record_reactions(
            &value_wrapper,
            PromiseReactionInitialization::AsyncFromSync {
                context: &context,
                realm,
            },
            function,
        )?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        context.clear(function);
        value_wrapper.clear(function);
        self.release_intrinsic_promise_resolve_realm_context(resolve, function);
        value.clear(function);
        schema.release_i32_local(close, function);
        schema.release_i32_local(done, function);
        Ok(())
    }

    pub(crate) fn emit_async_from_sync_iterator_method(
        &mut self,
        record: &GcLocal<IteratorRecord>,
        method: AsyncFromSyncIteratorMethod,
        argument: Option<&ValueLocals>,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<GcLocal<PromiseObject>, EmitError> {
        let schema = self.runtime_schema();
        // Allocate the intrinsic wrapper before Get/Call can run user code.
        let promise = self.emit_alloc_promise_in_realm(realm, function)?;
        let previous = schema.reserve_completion(function);
        previous.copy_from(self.completion(), function);
        let previous_name = schema.load_throw_diagnostic(ThrowDiagnosticRole::Name, function);
        let previous_message = schema.load_throw_diagnostic(ThrowDiagnosticRole::Message, function);
        let previous_constructor =
            schema.load_throw_diagnostic(ThrowDiagnosticRole::ConstructorName, function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        let iterator = schema.reserve_value_local(function);
        let callable = schema.reserve_value_local(function);
        let sync_result = schema.reserve_value_local(function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::ITERATOR)
                .read(record, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &iterator, schema, function);
        stored.clear(function);
        let arguments = match argument {
            Some(value) => self.emit_pre_evaluated_arg_vector(&[value], function),
            None => self.emit_pre_evaluated_arg_vector(&[], function),
        };
        let finish = self.open_frame(ControlFrameKind::Block, function);
        match method {
            AsyncFromSyncIteratorMethod::Next => {
                let next = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<IteratorRecord>()
                        .field(IteratorRecordSchema::NEXT_METHOD)
                        .read(record, schema, function)
                        .reference(),
                    function,
                );
                schema
                    .struct_type::<StoredValue>()
                    .read_into(&next, &callable, schema, function);
                next.clear(function);
            }
            AsyncFromSyncIteratorMethod::Return | AsyncFromSyncIteratorMethod::Throw => {
                let name = match method {
                    AsyncFromSyncIteratorMethod::Return => "return",
                    AsyncFromSyncIteratorMethod::Throw => "throw",
                    AsyncFromSyncIteratorMethod::Next => unreachable!(),
                };
                self.emit_async_from_sync_property(&iterator, name, &pending, function)?;
                self.emit_async_from_sync_reject_abrupt(&promise, &pending, finish, function)?;
                self.compile_nullish_tagged_i32(pending.value().tag(), function)?;
                self.open_frame(ControlFrameKind::If, function);
                match method {
                    AsyncFromSyncIteratorMethod::Return => {
                        let done = schema.reserve_i32_local(function);
                        function.instruction(&Instruction::I32Const(1));
                        done.store(function);
                        let result = self.emit_iterator_result_object_in_realm(
                            realm,
                            argument.unwrap_or(&undefined),
                            done,
                            function,
                        )?;
                        sync_result.set_reference(&result, schema, function);
                        // The absent return path does not await its supplied value.
                        self.emit_resolve_promise_record(&promise, &sync_result, function)?;
                        result.clear(function);
                        schema.release_i32_local(done, function);
                    }
                    AsyncFromSyncIteratorMethod::Throw => {
                        pending.initialize(function);
                        self.emit_iterator_close_with_completion(
                            &iterator, &pending, &pending, function,
                        )?;
                        self.emit_async_from_sync_reject_abrupt(
                            &promise, &pending, finish, function,
                        )?;
                        self.emit_async_from_sync_type_error(
                            realm,
                            RuntimeErrorMessage::YIELD_ITERATOR_HAS_NO_THROW_METHOD,
                            &pending,
                            function,
                        )?;
                        self.emit_settle_promise_record(
                            &promise,
                            PromiseSettlement::Reject,
                            pending.value(),
                            function,
                        )?;
                    }
                    AsyncFromSyncIteratorMethod::Next => unreachable!(),
                }
                self.emit_branch_to_target(finish, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                callable.copy_from(pending.value(), function);
            }
        }
        // Call checks the cached next only when invoked, not during acquisition.
        self.emit_is_callable_i32(&callable, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_async_from_sync_type_error(
            realm,
            RuntimeErrorMessage::VALUE_IS_NOT_CALLABLE,
            &pending,
            function,
        )?;
        self.emit_async_from_sync_reject_abrupt(&promise, &pending, finish, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_function_or_proxy_call_with_argv(
            &callable, &iterator, &arguments, &pending, function,
        )?;
        self.emit_async_from_sync_reject_abrupt(&promise, &pending, finish, function)?;
        self.emit_is_heap_object_like_tag_i32(pending.value().tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_async_from_sync_type_error(
            realm,
            RuntimeErrorMessage::YIELD_ITERATOR_RESULT_MUST_BE_OBJECT,
            &pending,
            function,
        )?;
        self.emit_async_from_sync_reject_abrupt(&promise, &pending, finish, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        sync_result.copy_from(pending.value(), function);
        let close = match method {
            AsyncFromSyncIteratorMethod::Next | AsyncFromSyncIteratorMethod::Throw => true,
            AsyncFromSyncIteratorMethod::Return => false,
        };
        self.emit_async_from_sync_continuation(
            &promise,
            record,
            &iterator,
            &sync_result,
            realm,
            close,
            &pending,
            function,
        )?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        arguments.clear(function);
        undefined.clear(function);
        sync_result.clear(function);
        callable.clear(function);
        iterator.clear(function);
        pending.clear(function);
        self.completion().copy_from(&previous, function);
        schema.replace_throw_diagnostic(ThrowDiagnosticRole::Name, &previous_name, function);
        schema.replace_throw_diagnostic(ThrowDiagnosticRole::Message, &previous_message, function);
        schema.replace_throw_diagnostic(
            ThrowDiagnosticRole::ConstructorName,
            &previous_constructor,
            function,
        );
        previous_constructor.clear(function);
        previous_message.clear(function);
        previous_name.clear(function);
        previous.clear(function);
        Ok(promise)
    }

    pub(super) fn emit_run_async_from_sync_iterator_job(
        &mut self,
        reaction: &GcLocal<PromiseReaction>,
        rejected: I32Local,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseReaction>()
                .field(PromiseReactionSchema::ASYNC_FROM_SYNC)
                .read(reaction, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let promise = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncFromSyncIteratorContinuation>()
                .field(AsyncFromSyncIteratorContinuationSchema::PROMISE)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncFromSyncIteratorContinuation>()
                .field(AsyncFromSyncIteratorContinuationSchema::REALM)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        let done = schema.reserve_i32_local(function);
        schema
            .struct_type::<AsyncFromSyncIteratorContinuation>()
            .field(AsyncFromSyncIteratorContinuationSchema::DONE)
            .read(&context, schema, function)
            .store(done, function);
        rejected.load(function);
        self.open_frame(ControlFrameKind::If, function);
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncFromSyncIteratorContinuation>()
                .field(AsyncFromSyncIteratorContinuationSchema::ITERATOR)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::ITERATOR)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        let iterator = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &iterator, schema, function);
        let close = schema.reserve_i32_local(function);
        schema
            .struct_type::<AsyncFromSyncIteratorContinuation>()
            .field(AsyncFromSyncIteratorContinuationSchema::CLOSE_ON_REJECTION)
            .read(&context, schema, function)
            .store(close, function);
        let pending = schema.reserve_completion(function);
        pending.set_throw(argument, function);
        self.emit_async_from_sync_close_rejection(&iterator, done, close, &pending, function)?;
        self.emit_settle_promise_record(
            &promise,
            PromiseSettlement::Reject,
            pending.value(),
            function,
        )?;
        pending.clear(function);
        schema.release_i32_local(close, function);
        iterator.clear(function);
        stored.clear(function);
        record.clear(function);
        function.instruction(&Instruction::Else);
        let result = self.emit_iterator_result_object_in_realm(&realm, argument, done, function)?;
        let value = schema.reserve_value_local(function);
        value.set_reference(&result, schema, function);
        // Resolve, rather than fulfill, so the result object's then is observed.
        self.emit_resolve_promise_record(&promise, &value, function)?;
        value.clear(function);
        result.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(done, function);
        realm.clear(function);
        promise.clear(function);
        context.clear(function);
        self.completion().initialize(function);
        Ok(())
    }
}
