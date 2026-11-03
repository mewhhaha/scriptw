use super::*;

/// A queue append must carry the exact target, full argument and Realm policy.
/// Reaction and thenable payloads cannot occupy each other's target fields.
enum PromiseJobToEnqueue<'a> {
    Callback {
        callback: &'a ValueLocals,
    },
    FinalizationCleanup {
        registry: &'a GcLocal<FinalizationRegistryObject>,
    },
    Reaction {
        reaction: &'a GcLocal<PromiseReaction>,
        argument: &'a ValueLocals,
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

    pub(super) fn emit_enqueue_promise_reaction_job(
        &mut self,
        reaction: &GcLocal<PromiseReaction>,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_enqueue_promise_job(
            PromiseJobToEnqueue::Reaction { reaction, argument },
            function,
        )
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
        let job: GcLocal<PendingJob> = match selected {
            PromiseJobToEnqueue::Callback { callback } => {
                let value = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(callback, function),
                    function,
                );
                let realm = self.emit_promise_job_callback_realm(callback, function)?;
                let job = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<PendingJob>().construct(
                        (
                            GcOperand::constant(PromiseJobKind::Callback),
                            GcOperand::null(schema),
                            GcOperand::null(schema),
                            GcOperand::null(schema),
                            GcOperand::reference(&value, schema),
                            GcOperand::nullable_reference(&realm, schema),
                            GcOperand::null(schema),
                        ),
                        function,
                    ),
                    function,
                );
                realm.clear(function);
                value.clear(function);
                job
            }
            PromiseJobToEnqueue::FinalizationCleanup { registry } => {
                let undefined = schema.reserve_value_local(function);
                undefined.set_undefined(function);
                let value = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(&undefined, function),
                    function,
                );
                let realm = schema.reserve_gc_local(function).initialize(
                    schema
                        .field(FinalizationRegistryObjectSchema::REALM)
                        .read(registry, schema, function)
                        .reference(),
                    function,
                );
                let job = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<PendingJob>().construct(
                        (
                            GcOperand::constant(PromiseJobKind::FinalizationCleanup),
                            GcOperand::null(schema),
                            GcOperand::null(schema),
                            GcOperand::nullable_reference(registry, schema),
                            GcOperand::reference(&value, schema),
                            GcOperand::nullable_reference(&realm, schema),
                            GcOperand::null(schema),
                        ),
                        function,
                    ),
                    function,
                );
                realm.clear(function);
                value.clear(function);
                undefined.clear(function);
                job
            }

            PromiseJobToEnqueue::Reaction { reaction, argument } => {
                let value = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(argument, function),
                    function,
                );
                let realm = self.emit_promise_reaction_job_realm(reaction, function)?;
                let job = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<PendingJob>().construct(
                        (
                            GcOperand::constant(PromiseJobKind::Reaction),
                            GcOperand::nullable_reference(reaction, schema),
                            GcOperand::null(schema),
                            GcOperand::null(schema),
                            GcOperand::reference(&value, schema),
                            GcOperand::reference(&realm, schema),
                            GcOperand::null(schema),
                        ),
                        function,
                    ),
                    function,
                );
                realm.clear(function);
                value.clear(function);
                job
            }
            PromiseJobToEnqueue::ResolveThenable { thenable, method } => {
                let undefined = schema.reserve_value_local(function);
                undefined.set_undefined(function);
                let value = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(&undefined, function),
                    function,
                );
                let realm = self.emit_promise_job_callback_realm(method, function)?;
                let job = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<PendingJob>().construct(
                        (
                            GcOperand::constant(PromiseJobKind::ResolveThenable),
                            GcOperand::null(schema),
                            GcOperand::nullable_reference(thenable, schema),
                            GcOperand::null(schema),
                            GcOperand::reference(&value, schema),
                            GcOperand::nullable_reference(&realm, schema),
                            GcOperand::null(schema),
                        ),
                        function,
                    ),
                    function,
                );
                realm.clear(function);
                value.clear(function);
                undefined.clear(function);
                job
            }
        };
        // The sole FIFO append operates on rooted references, including empty.
        let tail = schema.load_pending_job_queue(RuntimeQueueEnd::Tail, function);
        tail.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema.replace_pending_job_queue(RuntimeQueueEnd::Head, &job, function);
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
                GcOperand::nullable_reference(&job, schema),
                schema,
                function,
            );
        nonnull.clear(function);
        function.instruction(&Instruction::End);
        schema.replace_pending_job_queue(RuntimeQueueEnd::Tail, &job, function);
        tail.clear(function);
        job.clear(function);
        Ok(())
    }
}
