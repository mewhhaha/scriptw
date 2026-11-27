use super::*;

/// Each callback job selects its payload and Realm policy together.
enum PromiseJobToEnqueue<'a> {
    Callback {
        callback: &'a ValueLocals,
    },
    FinalizationCleanup {
        registry: &'a GcLocal<FinalizationRegistryObject>,
    },
    ResolveThenable {
        thenable: &'a GcLocal<PromiseThenableJob>,
        method: &'a ValueLocals,
    },
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_enqueue_microtask_callback(
        &mut self,
        callback: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_enqueue_promise_job(PromiseJobToEnqueue::Callback { callback }, function)
    }
    pub(crate) fn emit_enqueue_finalization_job(
        &mut self,
        registry: &GcLocal<FinalizationRegistryObject>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_enqueue_promise_job(
            PromiseJobToEnqueue::FinalizationCleanup { registry },
            function,
        )
    }

    pub(in crate::builtins) fn emit_enqueue_stream_fulfilled_job(
        &mut self,
        controller: &GcLocal<ReadableByteStreamControllerObject>,
        fulfillment: StreamFulfillment,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .field(ReadableByteStreamControllerObjectSchema::REALM)
                .read(controller, schema, function)
                .reference(),
            function,
        );
        let job = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StreamFulfillmentJob>()
                .construct(
                    (
                        GcOperand::null(schema),
                        GcOperand::nullable_reference(&realm, schema),
                        GcOperand::reference(controller, schema),
                        GcOperand::boolean(matches!(fulfillment, StreamFulfillment::Start)),
                    ),
                    function,
                )
                .upcast(),
            function,
        );
        self.emit_append_promise_job(&job, function);
        job.clear(function);
        realm.clear(function);
        Ok(())
    }

    /// The same entry moves from a pending Promise's list onto the job queue.
    /// Its argument shares the immutable StoredValue published at settlement.
    pub(super) fn emit_queue_selected_promise_reaction(
        &mut self,
        job: &GcLocal<PendingJob>,
        settlement: PromiseSettlement,
        argument: &GcLocal<StoredValue>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let rejected = matches!(settlement, PromiseSettlement::Reject);
        job.is_subtype::<PromiseAwaitContinuation>(schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let continuation = schema.reserve_gc_local(function).initialize(
            job.load(schema, function)
                .cast_subtype::<PromiseAwaitContinuation>(schema, function),
            function,
        );
        schema
            .field(PromiseAwaitContinuationSchema::ARGUMENT)
            .write(
                &continuation,
                GcOperand::nullable_reference(argument, schema),
                schema,
                function,
            );
        schema
            .field(PromiseAwaitContinuationSchema::REJECTED)
            .write(
                &continuation,
                GcOperand::boolean(rejected),
                schema,
                function,
            );
        continuation.clear(function);
        function.instruction(&Instruction::Else);
        let reaction = schema.reserve_gc_local(function).initialize(
            job.load(schema, function)
                .cast_subtype::<PromiseReaction>(schema, function),
            function,
        );
        schema.field(PromiseReactionSchema::ARGUMENT).write(
            &reaction,
            GcOperand::nullable_reference(argument, schema),
            schema,
            function,
        );
        schema.field(PromiseReactionSchema::REJECTED).write(
            &reaction,
            GcOperand::boolean(rejected),
            schema,
            function,
        );
        let handler = schema.reserve_gc_local(function).initialize(
            schema
                .field(PromiseReactionSchema::REJECT_HANDLER)
                .read(&reaction, schema, function)
                .reference(),
            function,
        );
        if rejected {
            handler.load(schema, function).is_null(function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
            schema.field(PromiseReactionSchema::HANDLER).write(
                &reaction,
                GcOperand::reference(&handler, schema),
                schema,
                function,
            );
            function.instruction(&Instruction::End);
        }
        schema.field(PromiseReactionSchema::REJECT_HANDLER).write(
            &reaction,
            GcOperand::null(schema),
            schema,
            function,
        );
        let realm = self.emit_promise_reaction_job_realm(&reaction, function)?;
        schema.field(PendingJobSchema::REALM).write(
            job,
            GcOperand::reference(&realm, schema),
            schema,
            function,
        );
        realm.clear(function);
        handler.clear(function);
        reaction.clear(function);
        function.instruction(&Instruction::End);
        self.emit_append_promise_job(job, function);
        Ok(())
    }

    pub(super) fn emit_enqueue_promise_thenable_job(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        thenable: &ValueLocals,
        method: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(thenable, function),
            function,
        );
        let then = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(method, function),
            function,
        );
        let record = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PromiseThenableJob>().construct(
                (
                    GcOperand::reference(promise, schema),
                    GcOperand::reference(&value, schema),
                    GcOperand::reference(&then, schema),
                ),
                function,
            ),
            function,
        );
        self.emit_enqueue_promise_job(
            PromiseJobToEnqueue::ResolveThenable {
                thenable: &record,
                method,
            },
            function,
        )?;
        record.clear(function);
        then.clear(function);
        value.clear(function);
        Ok(())
    }

    fn emit_enqueue_promise_job(
        &mut self,
        selected: PromiseJobToEnqueue<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let (kind, thenable, registry, callback, realm) = match selected {
            PromiseJobToEnqueue::Callback { callback } => {
                let stored = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(callback, function),
                    function,
                );
                (
                    PromiseJobKind::Callback,
                    None,
                    None,
                    Some(stored),
                    self.emit_promise_job_callback_realm(callback, function)?,
                )
            }
            PromiseJobToEnqueue::FinalizationCleanup { registry } => (
                PromiseJobKind::FinalizationCleanup,
                None,
                Some(registry),
                None,
                schema.reserve_gc_local(function).initialize(
                    schema
                        .field(FinalizationRegistryObjectSchema::REALM)
                        .read(registry, schema, function)
                        .reference(),
                    function,
                ),
            ),
            PromiseJobToEnqueue::ResolveThenable { thenable, method } => (
                PromiseJobKind::ResolveThenable,
                Some(thenable),
                None,
                None,
                self.emit_promise_job_callback_realm(method, function)?,
            ),
        };
        let job = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseCallbackJob>()
                .construct(
                    (
                        GcOperand::null(schema),
                        GcOperand::nullable_reference(&realm, schema),
                        GcOperand::constant(kind),
                        thenable.map_or_else(
                            || GcOperand::null(schema),
                            |value| GcOperand::nullable_reference(value, schema),
                        ),
                        registry.map_or_else(
                            || GcOperand::null(schema),
                            |value| GcOperand::nullable_reference(value, schema),
                        ),
                        callback.as_ref().map_or_else(
                            || GcOperand::null(schema),
                            |value| GcOperand::nullable_reference(value, schema),
                        ),
                    ),
                    function,
                )
                .upcast(),
            function,
        );
        self.emit_append_promise_job(&job, function);
        job.clear(function);
        realm.clear(function);
        if let Some(callback) = callback {
            callback.clear(function);
        }
        Ok(())
    }

    fn emit_append_promise_job(&self, job: &GcLocal<PendingJob>, function: &mut Function) {
        let schema = self.runtime_schema();
        // The sole FIFO append operates on rooted references, including empty.
        let tail = schema.load_pending_job_queue(RuntimeQueueEnd::Tail, function);
        tail.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema.replace_pending_job_queue(RuntimeQueueEnd::Head, job, function);
        function.instruction(&Instruction::Else);
        let nonnull = schema.reserve_gc_local(function).initialize(
            tail.load(schema, function).require_non_null(function),
            function,
        );
        schema
            .struct_type::<PendingJob>()
            .field(PendingJobSchema::NEXT)
            .write(
                &nonnull,
                GcOperand::nullable_reference(job, schema),
                schema,
                function,
            );
        nonnull.clear(function);
        function.instruction(&Instruction::End);
        schema.replace_pending_job_queue(RuntimeQueueEnd::Tail, job, function);
        tail.clear(function);
    }

    pub(super) fn emit_run_pending_job(
        &mut self,
        job: &GcLocal<PendingJob>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        job.is_subtype::<PromiseAwaitContinuation>(schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let continuation = schema.reserve_gc_local(function).initialize(
            job.load(schema, function)
                .cast_subtype::<PromiseAwaitContinuation>(schema, function),
            function,
        );
        self.emit_run_async_continuation_job(&continuation, function)?;
        continuation.clear(function);
        function.instruction(&Instruction::Else);

        job.is_subtype::<PromiseReaction>(schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let reaction = schema.reserve_gc_local(function).initialize(
            job.load(schema, function)
                .cast_subtype::<PromiseReaction>(schema, function),
            function,
        );
        self.emit_run_promise_reaction_job(&reaction, function)?;
        reaction.clear(function);
        function.instruction(&Instruction::Else);

        job.is_subtype::<StreamFulfillmentJob>(schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        if self
            .functions
            .get(&StandardBuiltinId::ReadableStreamPullIfNeeded.function_id())
            .is_some()
        {
            let stream_job = schema.reserve_gc_local(function).initialize(
                job.load(schema, function)
                    .cast_subtype::<StreamFulfillmentJob>(schema, function),
                function,
            );
            let controller = schema.reserve_gc_local(function).initialize(
                schema
                    .field(StreamFulfillmentJobSchema::CONTROLLER)
                    .read(&stream_job, schema, function)
                    .reference(),
                function,
            );
            let _ =
                schema
                    .field(StreamFulfillmentJobSchema::START)
                    .read(&stream_job, schema, function);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_run_stream_fulfilled_job(&controller, StreamFulfillment::Start, function)?;
            function.instruction(&Instruction::Else);
            self.emit_run_stream_fulfilled_job(&controller, StreamFulfillment::Pull, function)?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            controller.clear(function);
            stream_job.clear(function);
            self.completion().initialize(function);
        } else {
            function.instruction(&Instruction::Unreachable);
        }
        function.instruction(&Instruction::Else);

        let callback = schema.reserve_gc_local(function).initialize(
            job.load(schema, function)
                .cast_subtype::<PromiseCallbackJob>(schema, function),
            function,
        );
        self.emit_run_promise_callback_job(&callback, function)?;
        callback.clear(function);

        for _ in 0..3 {
            function.instruction(&Instruction::End);
        }
        Ok(())
    }

    fn emit_run_promise_callback_job(
        &mut self,
        job: &GcLocal<PromiseCallbackJob>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let kind = schema.reserve_i32_local(function);
        schema
            .field(PromiseCallbackJobSchema::KIND)
            .read(job, schema, function)
            .store(kind, function);
        for selected in PromiseJobKind::ALL {
            kind.load(function);
            function.instruction(&Instruction::I32Const(GcI32Constant::encode(selected)));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            match selected {
                PromiseJobKind::Callback => {
                    let stored = schema.reserve_gc_local(function).initialize(
                        schema
                            .field(PromiseCallbackJobSchema::CALLBACK)
                            .read(job, schema, function)
                            .reference()
                            .require_non_null(function),
                        function,
                    );
                    self.emit_run_microtask_callback(&stored, function)?;
                    stored.clear(function);
                }
                PromiseJobKind::FinalizationCleanup => {
                    let registry = schema.reserve_gc_local(function).initialize(
                        schema
                            .field(PromiseCallbackJobSchema::REGISTRY)
                            .read(job, schema, function)
                            .reference()
                            .require_non_null(function),
                        function,
                    );
                    self.emit_run_finalization_job(&registry, function)?;
                    registry.clear(function);
                }
                PromiseJobKind::ResolveThenable => {
                    let thenable = schema.reserve_gc_local(function).initialize(
                        schema
                            .field(PromiseCallbackJobSchema::THENABLE)
                            .read(job, schema, function)
                            .reference()
                            .require_non_null(function),
                        function,
                    );
                    self.emit_run_promise_thenable_job(&thenable, function)?;
                    thenable.clear(function);
                }
                PromiseJobKind::Reaction => {
                    function.instruction(&Instruction::Unreachable);
                }
            }
            function.instruction(&Instruction::Else);
        }
        function.instruction(&Instruction::Unreachable);
        for _ in PromiseJobKind::ALL {
            function.instruction(&Instruction::End);
        }
        schema.release_i32_local(kind, function);
        Ok(())
    }
}
