use self::promise_internal_function_materialization::{
    PromiseInternalFunction, PromiseInternalFunctionMaterializationContext,
};
use super::super::*;
use crate::functions::{FunctionRealmRevokedRoute, OrdinaryDefaultPrototype};
use crate::gc_types::*;
use crate::operations::PropertyKeyLocals;

mod async_from_sync_iterator;
mod current_function_realm_intrinsic_promise_capability;
mod promise_combinator_algorithm_error_realm;
mod promise_combinator_element_materialization;
mod promise_combinator_reaction_pair;
mod promise_finally_completion;
mod promise_internal_function_materialization;
mod promise_job_to_enqueue;
mod promise_keyed_combinator_mode;
mod promise_keyed_element_projection;
mod promise_prototype_receiver_type_error;
mod promise_prototype_then_invocation;
mod promise_resolve_realm_context;
mod promise_settlement_record_allocation;
mod promise_species_realm_context;
mod promise_try_callback_type_error;
mod promise_with_resolvers_result_allocation;
mod scheduling_helpers;

pub(crate) enum AsyncGeneratorCompleteStepKind {
    Yielded,
    Completed,
}

#[must_use = "Promise allocation consumes its selected prototype and Realm"]
pub(crate) struct PromiseAllocationContext {
    prototype: ValueLocals,
    realm: GcLocal<RealmRecord>,
}

#[must_use = "async execution Realm roots must be released"]
pub(crate) struct AsyncExecutionRealmContext {
    realm: GcLocal<RealmRecord>,
}
impl AsyncExecutionRealmContext {
    pub(crate) fn realm(&self) -> &GcLocal<RealmRecord> {
        &self.realm
    }
}

enum PromiseResolveRealmAuthority<'a> {
    CurrentFunction,
    AsyncExecution(&'a AsyncExecutionRealmContext),
    ExplicitRealm(&'a GcLocal<RealmRecord>),
}

pub(crate) enum ModuleReactionContinuation<'a> {
    Body(&'a GcLocal<ModuleRecord>),
    Join(&'a GcLocal<ModuleJoin>),
}

enum PromiseReactionInitialization<'a> {
    Default {
        handler: &'a ValueLocals,
        capability: &'a GcLocal<PromiseCapability, Nullable>,
    },
    AsyncFunction {
        activation: &'a GcLocal<AsyncActivation>,
        realm: &'a GcLocal<RealmRecord>,
    },
    AsyncGenerator {
        activation: &'a GcLocal<AsyncGeneratorActivation>,
        realm: &'a GcLocal<RealmRecord>,
        continuation: AsyncGeneratorAwaitContinuation,
    },
    AsyncFromSync {
        context: &'a GcLocal<AsyncFromSyncIteratorContinuation>,
        realm: &'a GcLocal<RealmRecord>,
    },
    Module {
        realm: &'a GcLocal<RealmRecord>,
        continuation: ModuleReactionContinuation<'a>,
    },
}

pub(crate) enum AsyncGeneratorAwaitContinuation {
    Body,
    AwaitReturn,
    Yield,
    YieldReturn,
}
impl AsyncGeneratorAwaitContinuation {
    fn callback_kind(&self) -> PromiseReactionCallbackKind {
        match self {
            Self::Body => PromiseReactionCallbackKind::AsyncGeneratorAwait,
            Self::AwaitReturn => PromiseReactionCallbackKind::AsyncGeneratorAwaitReturn,
            Self::Yield => PromiseReactionCallbackKind::AsyncGeneratorYield,
            Self::YieldReturn => PromiseReactionCallbackKind::AsyncGeneratorYieldReturn,
        }
    }
}

#[derive(Clone, Copy)]
enum PromiseCombinatorMode {
    Values,
    SettledRecords,
    FirstFulfillment,
    Race,
}
impl PromiseCombinatorMode {
    const fn resolve_error(self) -> RuntimeErrorMessage {
        match self {
            Self::Race => {
                RuntimeErrorMessage::PROMISE_RACE_CONSTRUCTOR_RESOLVE_PROPERTY_IS_NOT_CALLABLE
            }
            Self::Values | Self::SettledRecords | Self::FirstFulfillment => {
                RuntimeErrorMessage::PROMISE_ALL_CONSTRUCTOR_RESOLVE_PROPERTY_IS_NOT_CALLABLE
            }
        }
    }
    const fn input_error(self) -> RuntimeErrorMessage {
        match self {
            Self::Race => RuntimeErrorMessage::PROMISE_RACE_INPUT_IS_NOT_ITERABLE,
            Self::Values | Self::SettledRecords | Self::FirstFulfillment => {
                RuntimeErrorMessage::PROMISE_ALL_INPUT_IS_NOT_ITERABLE
            }
        }
    }
    const fn method_error(self) -> RuntimeErrorMessage {
        match self {
            Self::Race => RuntimeErrorMessage::PROMISE_RACE_ITERATOR_METHOD_IS_NOT_CALLABLE,
            Self::Values | Self::SettledRecords | Self::FirstFulfillment => {
                RuntimeErrorMessage::PROMISE_ALL_ITERATOR_METHOD_IS_NOT_CALLABLE
            }
        }
    }
    const fn method_result_error(self) -> RuntimeErrorMessage {
        match self {
            Self::Race => RuntimeErrorMessage::PROMISE_RACE_ITERATOR_METHOD_MUST_RETURN_AN_OBJECT,
            Self::Values | Self::SettledRecords | Self::FirstFulfillment => {
                RuntimeErrorMessage::PROMISE_ALL_ITERATOR_METHOD_MUST_RETURN_AN_OBJECT
            }
        }
    }
    const fn next_result_error(self) -> RuntimeErrorMessage {
        match self {
            Self::Race => RuntimeErrorMessage::PROMISE_RACE_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT,
            Self::Values | Self::SettledRecords | Self::FirstFulfillment => {
                RuntimeErrorMessage::PROMISE_ALL_ITERATOR_NEXT_RESULT_MUST_BE_AN_OBJECT
            }
        }
    }
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn emit_async_execution_realm_context_from_function(
        &mut self,
        function_object: &GcLocal<FunctionObject>,
        function: &mut Function,
    ) -> AsyncExecutionRealmContext {
        let schema = self.runtime_schema();
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::CONTEXT)
                .read(function_object, schema, function)
                .reference(),
            function,
        );
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionContext>()
                .field(FunctionContextSchema::REALM)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        context.clear(function);
        AsyncExecutionRealmContext { realm }
    }

    pub(crate) fn emit_async_function_execution_realm_context_from_activation(
        &mut self,
        activation: &GcLocal<AsyncActivation>,
        function: &mut Function,
    ) -> AsyncExecutionRealmContext {
        let schema = self.runtime_schema();
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncActivation>()
                .field(AsyncActivationSchema::REALM)
                .read(activation, schema, function)
                .reference(),
            function,
        );
        AsyncExecutionRealmContext { realm }
    }

    pub(crate) fn emit_async_generator_execution_realm_context_from_activation(
        &mut self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        function: &mut Function,
    ) -> AsyncExecutionRealmContext {
        let schema = self.runtime_schema();
        let frame = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncGeneratorActivation>()
                .field(AsyncGeneratorActivationSchema::FRAME)
                .read(activation, schema, function)
                .reference(),
            function,
        );
        let callable = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<InvocationFrame>()
                .field(InvocationFrameSchema::FUNCTION)
                .read(&frame, schema, function)
                .reference(),
            function,
        );
        let realm = self.emit_async_execution_realm_context_from_function(&callable, function);
        callable.clear(function);
        frame.clear(function);
        realm
    }

    pub(crate) fn release_async_execution_realm_context(
        &mut self,
        context: AsyncExecutionRealmContext,
        function: &mut Function,
    ) {
        context.realm.clear(function);
    }

    pub(crate) fn emit_async_execution_intrinsic_promise_allocation_context(
        &mut self,
        context: &AsyncExecutionRealmContext,
        function: &mut Function,
    ) -> PromiseAllocationContext {
        self.emit_intrinsic_promise_allocation_context(context.realm(), function)
    }

    fn emit_intrinsic_promise_allocation_context(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> PromiseAllocationContext {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::PromisePrototype,
            &prototype,
            function,
        );
        let realm = schema
            .reserve_gc_local(function)
            .initialize(realm.load(schema, function), function);
        PromiseAllocationContext { prototype, realm }
    }

    pub(crate) fn emit_current_function_realm_intrinsic_promise_allocation_context(
        &mut self,
        function: &mut Function,
    ) -> PromiseAllocationContext {
        let schema = self.runtime_schema();
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        let context = self.emit_intrinsic_promise_allocation_context(&realm, function);
        realm.clear(function);
        context
    }

    pub(crate) fn emit_alloc_promise_in_realm(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<GcLocal<PromiseObject>, EmitError> {
        let context = self.emit_intrinsic_promise_allocation_context(realm, function);
        self.emit_alloc_promise_with_prototype(context, function)
    }

    fn emit_current_function_realm_promise_allocation_context(
        &mut self,
        prototype: &ValueLocals,
        function: &mut Function,
    ) -> PromiseAllocationContext {
        let schema = self.runtime_schema();
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        let copied = schema.reserve_value_local(function);
        copied.copy_from(prototype, function);
        PromiseAllocationContext {
            prototype: copied,
            realm,
        }
    }

    pub(crate) fn emit_alloc_promise_with_prototype(
        &mut self,
        context: PromiseAllocationContext,
        function: &mut Function,
    ) -> Result<GcLocal<PromiseObject>, EmitError> {
        let schema = self.runtime_schema();
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&context.prototype), function)?,
            function,
        );
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let result = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&undefined, function),
            function,
        );
        let promise = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PromiseObject>().construct(
                (
                    GcOperand::reference(&object, schema),
                    GcOperand::reference(&result, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::constant(PromiseState::Pending),
                    GcOperand::boolean(false),
                    GcOperand::reference(&context.realm, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            function,
        );
        result.clear(function);
        undefined.clear(function);
        object.clear(function);
        context.realm.clear(function);
        context.prototype.clear(function);
        Ok(promise)
    }

    fn emit_create_promise_resolving_functions(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        context: &PromiseInternalFunctionMaterializationContext,
        resolve: &ValueLocals,
        reject: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let shared = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PromiseResolvingContext>().construct(
                (
                    GcOperand::reference(promise, schema),
                    GcOperand::boolean(false),
                ),
                function,
            ),
            function,
        );
        for (entry, out) in [
            (PromiseInternalFunction::Resolve(&shared), resolve),
            (PromiseInternalFunction::Reject(&shared), reject),
        ] {
            let callable = self.emit_promise_internal_function_value(entry, context, function)?;
            out.set_reference(&callable, schema, function);
            callable.clear(function);
        }
        shared.clear(function);
        Ok(())
    }

    pub(crate) fn emit_load_promise_state_strict(
        &self,
        promise: &GcLocal<PromiseObject>,
        state: I32Local,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::STATE)
            .read(promise, schema, function)
            .store(state, function);
        function.instruction(&Instruction::I32Const(0));
        for kind in PromiseState::ALL {
            state.load(function);
            function.instruction(&Instruction::I32Const(GcI32Constant::encode(kind)));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
        }
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
    }

    fn emit_enqueue_promise_reaction_list(
        &mut self,
        head: &GcLocal<PromiseReaction, Nullable>,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let current = schema
            .reserve_gc_local(function)
            .initialize(head.load(schema, function), function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        current.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::BrIf(1));
        let reaction = schema.reserve_gc_local(function).initialize(
            current.load(schema, function).require_non_null(function),
            function,
        );
        let next = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseReaction>()
                .field(PromiseReactionSchema::NEXT)
                .read(&reaction, schema, function)
                .reference(),
            function,
        );
        self.emit_enqueue_promise_reaction_job(&reaction, argument, function)?;
        current.replace(next.load(schema, function), function);
        next.clear(function);
        reaction.clear(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        current.clear(function);
        Ok(())
    }

    fn emit_promise_job_callback_realm(
        &mut self,
        callback: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcLocal<RealmRecord>, EmitError> {
        let schema = self.runtime_schema();
        let fallback = self.load_current_realm(function);
        let obtained = self.emit_get_function_realm(callback, function);
        let resolved = self.emit_route_function_realm_result(
            obtained,
            FunctionRealmRevokedRoute::UseCurrentRealm { realm: &fallback },
            function,
        )?;
        let realm = schema
            .reserve_gc_local(function)
            .initialize(resolved.realm().load(schema, function), function);
        self.release_resolved_function_realm_local(resolved, function);
        fallback.clear(function);
        Ok(realm)
    }

    fn emit_promise_reaction_job_realm(
        &mut self,
        reaction: &GcLocal<PromiseReaction>,
        function: &mut Function,
    ) -> Result<GcLocal<RealmRecord, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let kind = schema.reserve_i32_local(function);
        let realm = schema
            .reserve_gc_local::<RealmRecord, Nullable>(function)
            .initialize_null(schema, function);
        schema
            .struct_type::<PromiseReaction>()
            .field(PromiseReactionSchema::CALLBACK_KIND)
            .read(reaction, schema, function)
            .store(kind, function);
        let mut arms = 0;
        for callback_kind in PromiseReactionCallbackKind::ALL {
            kind.load(function);
            function.instruction(&Instruction::I32Const(GcI32Constant::encode(callback_kind)));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            match callback_kind.realm_source() {
                PromiseReactionRealmSource::HandlerOrNull => {
                    let stored = schema.reserve_gc_local(function).initialize(
                        schema
                            .struct_type::<PromiseReaction>()
                            .field(PromiseReactionSchema::HANDLER)
                            .read(reaction, schema, function)
                            .reference(),
                        function,
                    );
                    let handler = schema.reserve_value_local(function);
                    schema
                        .struct_type::<StoredValue>()
                        .read_into(&stored, &handler, schema, function);
                    self.emit_is_callable_i32(&handler, function)?;
                    function.instruction(&Instruction::If(BlockType::Empty));
                    let selected = self.emit_promise_job_callback_realm(&handler, function)?;
                    realm.replace(selected.load(schema, function).nullable(), function);
                    selected.clear(function);
                    function.instruction(&Instruction::End);
                    handler.clear(function);
                    stored.clear(function);
                }
                PromiseReactionRealmSource::Captured => {
                    realm.replace(
                        schema
                            .struct_type::<PromiseReaction>()
                            .field(PromiseReactionSchema::REALM)
                            .read(reaction, schema, function)
                            .reference(),
                        function,
                    );
                    realm.load(schema, function);
                    function.instruction(&Instruction::RefIsNull);
                    function.instruction(&Instruction::If(BlockType::Empty));
                    function.instruction(&Instruction::Unreachable);
                    function.instruction(&Instruction::End);
                }
            }
            function.instruction(&Instruction::Else);
            arms += 1;
        }
        function.instruction(&Instruction::Unreachable);
        for _ in 0..arms {
            function.instruction(&Instruction::End);
        }
        schema.release_i32_local(kind, function);
        Ok(realm)
    }

    pub(crate) fn emit_resolve_promise_record(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        // Direct intrinsic resolutions stand in for resolving functions created
        // with this Promise. Escaping resolving functions supply their own Realm
        // below: a thenable job can create them in a different Realm.
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseObject>()
                .field(PromiseObjectSchema::REALM)
                .read(promise, schema, function)
                .reference(),
            function,
        );
        self.emit_resolve_promise_record_in_realm(promise, value, &realm, function)?;
        realm.clear(function);
        Ok(())
    }

    fn emit_resolve_promise_record_in_realm(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        value: &ValueLocals,
        resolving_realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let then = schema.reserve_completion(function);
        then.initialize(function);
        self.emit_is_heap_object_like_tag_i32(value.tag(), function);
        function.instruction(&Instruction::If(BlockType::Empty));
        value.reference().load(function);
        promise.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            resolving_realm,
            NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            &prototype,
            function,
        );
        self.emit_throw_runtime_error_with_prototype(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::PROMISE_CANNOT_RESOLVE_TO_ITSELF,
            &prototype,
            &then,
            function,
        )?;
        self.emit_settle_promise_record(
            promise,
            PromiseSettlement::Reject,
            then.value(),
            function,
        )?;
        prototype.clear(function);
        function.instruction(&Instruction::Else);
        let key_string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("then", function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &key_string, function);
        self.emit_object_read_with_throw_routing(
            value,
            value,
            &key,
            &then,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        then.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_settle_promise_record(
            promise,
            PromiseSettlement::Reject,
            then.value(),
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_is_callable_i32(then.value(), function)?;
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_enqueue_promise_thenable_job(promise, value, then.value(), function)?;
        function.instruction(&Instruction::Else);
        self.emit_settle_promise_record(promise, PromiseSettlement::Fulfill, value, function)?;
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        key.clear(function);
        key_string.clear(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        self.emit_settle_promise_record(promise, PromiseSettlement::Fulfill, value, function)?;
        function.instruction(&Instruction::End);
        then.clear(function);
        Ok(())
    }

    pub(crate) fn emit_settle_promise_record(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        settlement: PromiseSettlement,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let state = schema.reserve_i32_local(function);
        self.emit_load_promise_state_strict(promise, state, function);
        state.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            PromiseState::Pending,
        )));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let fulfill = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseObject>()
                .field(PromiseObjectSchema::FULFILL_REACTIONS)
                .read(promise, schema, function)
                .reference(),
            function,
        );
        let reject = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseObject>()
                .field(PromiseObjectSchema::REJECT_REACTIONS)
                .read(promise, schema, function)
                .reference(),
            function,
        );
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(value, function),
            function,
        );
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::RESULT)
            .write(
                promise,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::FULFILL_REACTIONS)
            .write(promise, GcOperand::null(schema), schema, function);
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::REJECT_REACTIONS)
            .write(promise, GcOperand::null(schema), schema, function);
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::STATE)
            .write(
                promise,
                GcOperand::constant(settlement.state()),
                schema,
                function,
            );
        match settlement {
            PromiseSettlement::Fulfill => {
                self.emit_enqueue_promise_reaction_list(&fulfill, value, function)?
            }
            PromiseSettlement::Reject => {
                self.emit_track_unhandled_rejection(promise, function);
                self.emit_enqueue_promise_reaction_list(&reject, value, function)?;
            }
        }
        stored.clear(function);
        reject.clear(function);
        fulfill.clear(function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(state, function);
        Ok(())
    }

    fn emit_track_unhandled_rejection(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(function);
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::HANDLED)
            .read(promise, schema, function)
            .store(handled, function);
        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::UNHANDLED_NEXT)
            .write(promise, GcOperand::null(schema), schema, function);
        let tail = schema.load_unhandled_promise_queue(RuntimeQueueEnd::Tail, function);
        tail.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema.replace_unhandled_promise_queue(RuntimeQueueEnd::Head, promise, function);
        function.instruction(&Instruction::Else);
        let nonnull = schema.reserve_gc_local(function).initialize(
            tail.load(schema, function).require_non_null(function),
            function,
        );
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::UNHANDLED_NEXT)
            .write(
                &nonnull,
                GcOperand::nullable_reference(promise, schema),
                schema,
                function,
            );
        nonnull.clear(function);
        function.instruction(&Instruction::End);
        schema.replace_unhandled_promise_queue(RuntimeQueueEnd::Tail, promise, function);
        tail.clear(function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(handled, function);
    }

    pub(crate) fn emit_complete_async_entry_invocation(
        &mut self,
        activation: &GcLocal<AsyncActivation>,
        completion: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let promise = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncActivation>()
                .field(AsyncActivationSchema::PROMISE)
                .read(activation, schema, function)
                .reference(),
            function,
        );
        let done = schema.reserve_i32_local(function);
        schema
            .struct_type::<AsyncActivation>()
            .field(AsyncActivationSchema::COMPLETED)
            .read(activation, schema, function)
            .store(done, function);
        done.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        completion.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_settle_promise_record(
            &promise,
            PromiseSettlement::Reject,
            completion.value(),
            function,
        )?;
        function.instruction(&Instruction::I32Const(1));
        done.store(function);
        function.instruction(&Instruction::Else);
        completion.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Return.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_resolve_promise_record(&promise, completion.value(), function)?;
        function.instruction(&Instruction::I32Const(1));
        done.store(function);
        function.instruction(&Instruction::Else);
        completion.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        completion.target().load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        self.emit_resolve_promise_record(&promise, &undefined, function)?;
        undefined.clear(function);
        function.instruction(&Instruction::I32Const(1));
        done.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        done.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let frame = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncActivation>()
                .field(AsyncActivationSchema::FRAME)
                .read(activation, schema, function)
                .reference(),
            function,
        );
        let lists = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PrivateArgumentListTable>()
                .fixed(std::iter::empty(), function),
            function,
        );
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::PRIVATE_ARGUMENT_LISTS)
            .write(
                &frame,
                GcOperand::reference(&lists, schema),
                schema,
                function,
            );
        lists.clear(function);
        frame.clear(function);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<AsyncActivation>()
            .field(AsyncActivationSchema::COMPLETED)
            .write(activation, GcOperand::boolean_local(done), schema, function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(done, function);
        promise.clear(function);
        Ok(())
    }

    /// Detach the finite checkpoint before diagnostic conversions can enqueue
    /// more rejections. Existing abrupt completion always remains primary.
    pub(crate) fn emit_report_unhandled_rejection(
        &mut self,
        policy: PromiseRejectionPolicy,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        match policy {
            PromiseRejectionPolicy::Ignore => {
                schema.clear_unhandled_promise_queue(RuntimeQueueEnd::Head, function);
                schema.clear_unhandled_promise_queue(RuntimeQueueEnd::Tail, function);
                return Ok(());
            }
            PromiseRejectionPolicy::FailRun => {}
        }
        let saved = schema.reserve_completion(function);
        saved.copy_from(self.completion(), function);
        let saved_name = schema.load_throw_diagnostic(ThrowDiagnosticRole::Name, function);
        let saved_message = schema.load_throw_diagnostic(ThrowDiagnosticRole::Message, function);
        let saved_constructor =
            schema.load_throw_diagnostic(ThrowDiagnosticRole::ConstructorName, function);
        let current = schema.load_unhandled_promise_queue(RuntimeQueueEnd::Head, function);
        let tail = schema.load_unhandled_promise_queue(RuntimeQueueEnd::Tail, function);
        let oldest = schema
            .reserve_gc_local::<PromiseObject, Nullable>(function)
            .initialize_null(schema, function);
        schema.clear_unhandled_promise_queue(RuntimeQueueEnd::Head, function);
        schema.clear_unhandled_promise_queue(RuntimeQueueEnd::Tail, function);
        tail.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let last = schema.reserve_gc_local(function).initialize(
            tail.load(schema, function).require_non_null(function),
            function,
        );
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::UNHANDLED_NEXT)
            .write(&last, GcOperand::null(schema), schema, function);
        last.clear(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        current.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::BrIf(1));
        let promise = schema.reserve_gc_local(function).initialize(
            current.load(schema, function).require_non_null(function),
            function,
        );
        let state = schema.reserve_i32_local(function);
        let handled = schema.reserve_i32_local(function);
        self.emit_load_promise_state_strict(&promise, state, function);
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::HANDLED)
            .read(&promise, schema, function)
            .store(handled, function);
        state.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            PromiseState::Rejected,
        )));
        function.instruction(&Instruction::I32Eq);
        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        oldest.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        oldest.replace(promise.load(schema, function).nullable(), function);
        function.instruction(&Instruction::End);
        saved.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Ne);
        oldest.load(schema, function);
        promise.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseObject>()
                .field(PromiseObjectSchema::RESULT)
                .read(&promise, schema, function)
                .reference(),
            function,
        );
        let rejection = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &rejection, schema, function);
        let printed = schema
            .reserve_gc_local::<StringValue, Nullable>(function)
            .initialize_null(schema, function);
        rejection.tag().load(function);
        function.instruction(&Instruction::I32Const(ValueKind::Symbol.tag() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let symbol = schema.reserve_gc_local(function).initialize(
            rejection.cast_reference::<SymbolValue>(schema, function),
            function,
        );
        printed.replace(
            self.emit_symbol_descriptive_string(&symbol, function)?
                .nullable(),
            function,
        );
        symbol.clear(function);
        function.instruction(&Instruction::Else);
        let conversion = schema.reserve_completion(function);
        conversion.initialize(function);
        self.emit_value_to_string_payload(&rejection, &conversion, function)?;
        conversion.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        printed.replace(
            self.emit_interned_string_reference(
                UNHANDLED_REJECTION_TOSTRING_THROWN_MESSAGE,
                function,
            )?
            .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        printed.replace(
            conversion
                .value()
                .cast_reference::<StringValue>(schema, function)
                .nullable(),
            function,
        );
        function.instruction(&Instruction::End);
        conversion.clear(function);
        function.instruction(&Instruction::End);
        let output = schema.reserve_gc_local(function).initialize(
            printed.load(schema, function).require_non_null(function),
            function,
        );
        self.emit_host_print_string(&output, function)?;
        output.clear(function);
        printed.clear(function);
        rejection.clear(function);
        stored.clear(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        current.replace(
            schema
                .struct_type::<PromiseObject>()
                .field(PromiseObjectSchema::UNHANDLED_NEXT)
                .read(&promise, schema, function)
                .reference(),
            function,
        );
        schema.release_i32_local(handled, function);
        schema.release_i32_local(state, function);
        promise.clear(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&saved, function);
        schema.replace_throw_diagnostic(ThrowDiagnosticRole::Name, &saved_name, function);
        schema.replace_throw_diagnostic(ThrowDiagnosticRole::Message, &saved_message, function);
        schema.replace_throw_diagnostic(
            ThrowDiagnosticRole::ConstructorName,
            &saved_constructor,
            function,
        );
        oldest.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        saved.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        let promise = schema.reserve_gc_local(function).initialize(
            oldest.load(schema, function).require_non_null(function),
            function,
        );
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseObject>()
                .field(PromiseObjectSchema::RESULT)
                .read(&promise, schema, function)
                .reference(),
            function,
        );
        let value = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &value, schema, function);
        self.completion().set_throw(&value, function);
        schema.clear_throw_diagnostic(ThrowDiagnosticRole::Name, function);
        self.emit_capture_throw_error_name(&value, function)?;
        value.clear(function);
        stored.clear(function);
        promise.clear(function);
        function.instruction(&Instruction::End);
        oldest.clear(function);
        tail.clear(function);
        current.clear(function);
        saved_constructor.clear(function);
        saved_message.clear(function);
        saved_name.clear(function);
        saved.clear(function);
        Ok(())
    }

    pub(crate) fn emit_promise_constructor(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let executor = schema.reserve_value_local(function);
        let new_target = schema.reserve_value_local(function);
        new_target.copy_from(
            self.body_entry_locals()
                .expect("Promise constructor ordinary entry")
                .new_target(),
            function,
        );
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        new_target.tag().load(function);
        function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::PROMISE_CONSTRUCTOR_REQUIRES_NEW,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_builtin_arg_to_value(0, &executor, function);
        self.emit_is_callable_i32(&executor, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::PROMISE_EXECUTOR_IS_NOT_CALLABLE,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_get_prototype_from_constructor(
            &new_target,
            OrdinaryDefaultPrototype::Promise,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let context =
            self.emit_current_function_realm_promise_allocation_context(pending.value(), function);
        let promise = self.emit_alloc_promise_with_prototype(context, function)?;
        let resolve = schema.reserve_value_local(function);
        let reject = schema.reserve_value_local(function);
        let resolving_context =
            self.emit_current_function_promise_internal_function_materialization_context(function);
        self.emit_create_promise_resolving_functions(
            &promise,
            &resolving_context,
            &resolve,
            &reject,
            function,
        )?;
        self.release_promise_internal_function_materialization_context(resolving_context, function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[&resolve, &reject], function);
        self.emit_function_or_proxy_call_with_argv(
            &executor, &undefined, &arguments, &pending, function,
        )?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let rejection_arguments = self.emit_pre_evaluated_arg_vector(&[pending.value()], function);
        let ignored = schema.reserve_completion(function);
        ignored.initialize(function);
        self.emit_function_or_proxy_call_with_argv(
            &reject,
            &undefined,
            &rejection_arguments,
            &ignored,
            function,
        )?;
        ignored.clear(function);
        rejection_arguments.clear(function);
        function.instruction(&Instruction::End);
        let value = schema.reserve_value_local(function);
        value.set_reference(&promise, schema, function);
        self.completion().set_normal(&value, function);
        value.clear(function);
        arguments.clear(function);
        undefined.clear(function);
        reject.clear(function);
        resolve.clear(function);
        promise.clear(function);
        pending.clear(function);
        new_target.clear(function);
        executor.clear(function);
        Ok(())
    }

    /// Publish an immutable capability only after the exposed executor's
    /// mutable cells have been initialized by the actual user Construct call.
    pub(crate) fn emit_new_promise_capability(
        &mut self,
        executor_context: &PromiseInternalFunctionMaterializationContext,
        constructor: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PromiseCapability>, EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        self.emit_is_constructor_i32(constructor, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::PROMISE_CAPABILITY_CONSTRUCTOR_IS_NOT_A_CONSTRUCTOR,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let initial = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&undefined, function),
            function,
        );
        let cells = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseCapabilityExecutorContext>()
                .construct(
                    (
                        GcOperand::reference(&initial, schema),
                        GcOperand::reference(&initial, schema),
                    ),
                    function,
                ),
            function,
        );
        let executor = self.emit_promise_internal_function_value(
            PromiseInternalFunction::CapabilityExecutor(&cells),
            executor_context,
            function,
        )?;
        let executor_value = schema.reserve_value_local(function);
        executor_value.set_reference(&executor, schema, function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[&executor_value], function);
        self.emit_function_or_proxy_construct_with_argv(
            constructor,
            constructor,
            &arguments,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let resolve = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseCapabilityExecutorContext>()
                .field(PromiseCapabilityExecutorContextSchema::RESOLVE)
                .read(&cells, schema, function)
                .reference(),
            function,
        );
        let reject = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseCapabilityExecutorContext>()
                .field(PromiseCapabilityExecutorContextSchema::REJECT)
                .read(&cells, schema, function)
                .reference(),
            function,
        );
        let resolved_value = schema.reserve_value_local(function);
        let rejected_value = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&resolve, &resolved_value, schema, function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&reject, &rejected_value, schema, function);
        self.emit_is_callable_i32(&resolved_value, function)?;
        self.emit_is_callable_i32(&rejected_value, function)?;
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let failure = schema.reserve_completion(function);
        failure.initialize(function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::PROMISE_CAPABILITY_DID_NOT_INITIALIZE_CALLABLE_RESOLVING_FUNCTIONS,
            &failure,
            function,
        )?;
        self.completion().copy_from(&failure, function);
        self.emit_propagate_current_throw(function);
        failure.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let promise = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(pending.value(), function),
            function,
        );
        let capability = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PromiseCapability>().construct(
                (
                    GcOperand::reference(&promise, schema),
                    GcOperand::reference(&resolve, schema),
                    GcOperand::reference(&reject, schema),
                ),
                function,
            ),
            function,
        );
        promise.clear(function);
        rejected_value.clear(function);
        resolved_value.clear(function);
        reject.clear(function);
        resolve.clear(function);
        arguments.clear(function);
        executor_value.clear(function);
        executor.clear(function);
        cells.clear(function);
        initial.clear(function);
        undefined.clear(function);
        pending.clear(function);
        Ok(capability)
    }

    fn emit_initialize_promise_reaction(
        &mut self,
        reaction_type: PromiseReactionType,
        initialization: &PromiseReactionInitialization<'_>,
        function: &mut Function,
    ) -> GcLocal<PromiseReaction> {
        let schema = self.runtime_schema();
        let empty = schema.reserve_value_local(function);
        empty.set_undefined(function);
        let handler = match initialization {
            PromiseReactionInitialization::Default { handler, .. } => *handler,
            PromiseReactionInitialization::AsyncFunction { .. }
            | PromiseReactionInitialization::AsyncGenerator { .. }
            | PromiseReactionInitialization::AsyncFromSync { .. }
            | PromiseReactionInitialization::Module { .. } => &empty,
        };
        let handler = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(handler, function),
            function,
        );
        let (kind, capability, realm, activation, generator, module, join, sync) =
            match initialization {
                PromiseReactionInitialization::Default { capability, .. } => (
                    PromiseReactionCallbackKind::Default,
                    GcOperand::reference(*capability, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                PromiseReactionInitialization::AsyncFunction { activation, realm } => (
                    PromiseReactionCallbackKind::AsyncFunction,
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(*realm, schema),
                    GcOperand::nullable_reference(*activation, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                PromiseReactionInitialization::AsyncGenerator {
                    activation,
                    realm,
                    continuation,
                } => (
                    continuation.callback_kind(),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(*realm, schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(*activation, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                PromiseReactionInitialization::AsyncFromSync { context, realm } => (
                    PromiseReactionCallbackKind::AsyncFromSyncIterator,
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(*realm, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::nullable_reference(*context, schema),
                ),
                PromiseReactionInitialization::Module {
                    realm,
                    continuation,
                } => match continuation {
                    ModuleReactionContinuation::Body(record) => (
                        PromiseReactionCallbackKind::ModuleBody,
                        GcOperand::null(schema),
                        GcOperand::nullable_reference(*realm, schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::nullable_reference(*record, schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                    ),
                    ModuleReactionContinuation::Join(record) => (
                        PromiseReactionCallbackKind::ModuleJoin,
                        GcOperand::null(schema),
                        GcOperand::nullable_reference(*realm, schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::null(schema),
                        GcOperand::nullable_reference(*record, schema),
                        GcOperand::null(schema),
                    ),
                },
            };
        let reaction = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PromiseReaction>().construct(
                (
                    GcOperand::constant(reaction_type),
                    GcOperand::constant(kind),
                    GcOperand::reference(&handler, schema),
                    capability,
                    realm,
                    activation,
                    generator,
                    module,
                    join,
                    sync,
                    GcOperand::null(schema),
                ),
                function,
            ),
            function,
        );
        handler.clear(function);
        empty.clear(function);
        reaction
    }

    fn emit_initialize_default_promise_reaction(
        &mut self,
        capability: &GcLocal<PromiseCapability, Nullable>,
        handler: &ValueLocals,
        reaction_type: PromiseReactionType,
        function: &mut Function,
    ) -> GcLocal<PromiseReaction> {
        self.emit_initialize_promise_reaction(
            reaction_type,
            &PromiseReactionInitialization::Default {
                handler,
                capability,
            },
            function,
        )
    }

    /// PerformPromiseThen for an already-owned Promise. Internal algorithms
    /// attach reactions without observing mutable `then`, `constructor`, or
    /// species properties. The public method selects its species beforehand.
    pub(crate) fn emit_perform_promise_then(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        on_fulfilled: &ValueLocals,
        on_rejected: &ValueLocals,
        capability: &GcLocal<PromiseCapability>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let nullable_capability = schema
            .reserve_gc_local::<PromiseCapability, Nullable>(function)
            .initialize(capability.load(schema, function).nullable(), function);
        let fulfill = self.emit_initialize_default_promise_reaction(
            &nullable_capability,
            on_fulfilled,
            PromiseReactionType::Fulfill,
            function,
        );
        let reject = self.emit_initialize_default_promise_reaction(
            &nullable_capability,
            on_rejected,
            PromiseReactionType::Reject,
            function,
        );
        self.emit_route_promise_reaction_pair(promise, &fulfill, &reject, function)?;
        reject.clear(function);
        fulfill.clear(function);
        nullable_capability.clear(function);
        Ok(())
    }

    fn emit_append_promise_reaction(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        reaction_type: PromiseReactionType,
        reaction: &GcLocal<PromiseReaction>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let field = match reaction_type {
            PromiseReactionType::Fulfill => PromiseObjectSchema::FULFILL_REACTIONS,
            PromiseReactionType::Reject => PromiseObjectSchema::REJECT_REACTIONS,
        };
        let head = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseObject>()
                .field(field)
                .read(promise, schema, function)
                .reference(),
            function,
        );
        head.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema.struct_type::<PromiseObject>().field(field).write(
            promise,
            GcOperand::nullable_reference(reaction, schema),
            schema,
            function,
        );
        function.instruction(&Instruction::Else);
        let current = schema.reserve_gc_local(function).initialize(
            head.load(schema, function).require_non_null(function),
            function,
        );
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        let next = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseReaction>()
                .field(PromiseReactionSchema::NEXT)
                .read(&current, schema, function)
                .reference(),
            function,
        );
        next.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .struct_type::<PromiseReaction>()
            .field(PromiseReactionSchema::NEXT)
            .write(
                &current,
                GcOperand::nullable_reference(reaction, schema),
                schema,
                function,
            );
        function.instruction(&Instruction::Br(2));
        function.instruction(&Instruction::End);
        current.replace(
            next.load(schema, function).require_non_null(function),
            function,
        );
        next.clear(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        current.clear(function);
        function.instruction(&Instruction::End);
        head.clear(function);
    }

    /// Closed PerformPromiseThen dispatch; no invalid state becomes rejection.
    fn emit_route_promise_reaction_pair(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        fulfill: &GcLocal<PromiseReaction>,
        reject: &GcLocal<PromiseReaction>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let state = schema.reserve_i32_local(function);
        self.emit_load_promise_state_strict(promise, state, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseObject>()
                .field(PromiseObjectSchema::RESULT)
                .read(promise, schema, function)
                .reference(),
            function,
        );
        let value = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &value, schema, function);
        let mut arms = 0;
        for selected in PromiseState::ALL {
            state.load(function);
            function.instruction(&Instruction::I32Const(GcI32Constant::encode(selected)));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            match selected {
                PromiseState::Pending => {
                    self.emit_append_promise_reaction(
                        promise,
                        PromiseReactionType::Fulfill,
                        fulfill,
                        function,
                    );
                    self.emit_append_promise_reaction(
                        promise,
                        PromiseReactionType::Reject,
                        reject,
                        function,
                    );
                }
                PromiseState::Fulfilled => {
                    self.emit_enqueue_promise_reaction_job(fulfill, &value, function)?
                }
                PromiseState::Rejected => {
                    self.emit_enqueue_promise_reaction_job(reject, &value, function)?
                }
            }
            function.instruction(&Instruction::Else);
            arms += 1;
        }
        function.instruction(&Instruction::Unreachable);
        for _ in 0..arms {
            function.instruction(&Instruction::End);
        }
        schema
            .struct_type::<PromiseObject>()
            .field(PromiseObjectSchema::HANDLED)
            .write(promise, GcOperand::boolean(true), schema, function);
        value.clear(function);
        stored.clear(function);
        schema.release_i32_local(state, function);
        Ok(())
    }

    pub(crate) fn emit_read_promise_capability_promise(
        &self,
        capability: &GcLocal<PromiseCapability>,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseCapability>()
                .field(PromiseCapabilitySchema::PROMISE)
                .read(capability, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, output, schema, function);
        stored.clear(function);
    }

    pub(crate) fn emit_call_promise_capability(
        &mut self,
        capability: &GcLocal<PromiseCapability>,
        settlement: PromiseSettlement,
        argument: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let field = match settlement {
            PromiseSettlement::Fulfill => PromiseCapabilitySchema::RESOLVE,
            PromiseSettlement::Reject => PromiseCapabilitySchema::REJECT,
        };
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseCapability>()
                .field(field)
                .read(capability, schema, function)
                .reference(),
            function,
        );
        let callback = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &callback, schema, function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[argument], function);
        self.emit_function_or_proxy_call_with_argv(
            &callback, &undefined, &arguments, result, function,
        )?;
        arguments.clear(function);
        undefined.clear(function);
        callback.clear(function);
        stored.clear(function);
        Ok(())
    }

    fn emit_owned_promise_record_reactions(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        initialization: PromiseReactionInitialization<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let fulfill = self.emit_initialize_promise_reaction(
            PromiseReactionType::Fulfill,
            &initialization,
            function,
        );
        let reject = self.emit_initialize_promise_reaction(
            PromiseReactionType::Reject,
            &initialization,
            function,
        );
        self.emit_route_promise_reaction_pair(promise, &fulfill, &reject, function)?;
        reject.clear(function);
        fulfill.clear(function);
        Ok(())
    }

    pub(crate) fn emit_module_promise_reactions(
        &mut self,
        promise: &GcLocal<PromiseObject>,
        realm: &AsyncExecutionRealmContext,
        continuation: ModuleReactionContinuation<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_owned_promise_record_reactions(
            promise,
            PromiseReactionInitialization::Module {
                realm: realm.realm(),
                continuation,
            },
            function,
        )
    }
    pub(crate) fn emit_module_import_await_reactions(
        &mut self,
        activation: &GcLocal<AsyncActivation>,
        promise: &GcLocal<PromiseObject>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let frame = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncActivation>()
                .field(AsyncActivationSchema::FRAME)
                .read(activation, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::LEXICAL_ENVIRONMENT)
            .write(
                &frame,
                GcOperand::reference(self.current_environment(), schema),
                schema,
                function,
            );
        let realm =
            self.emit_async_function_execution_realm_context_from_activation(activation, function);
        self.emit_owned_promise_record_reactions(
            promise,
            PromiseReactionInitialization::AsyncFunction {
                activation,
                realm: realm.realm(),
            },
            function,
        )?;
        self.release_async_execution_realm_context(realm, function);
        frame.clear(function);
        Ok(())
    }
    pub(crate) fn emit_module_execution_realm_context(
        &mut self,
        module: &GcLocal<ModuleRecord>,
        function: &mut Function,
    ) -> AsyncExecutionRealmContext {
        let schema = self.runtime_schema();
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ModuleRecord>()
                .field(ModuleRecordSchema::REALM)
                .read(module, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        AsyncExecutionRealmContext { realm }
    }

    fn emit_async_await_reactions_inner(
        &mut self,
        activation: &GcLocal<AsyncActivation>,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let frame = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncActivation>()
                .field(AsyncActivationSchema::FRAME)
                .read(activation, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<InvocationFrame>()
            .field(InvocationFrameSchema::LEXICAL_ENVIRONMENT)
            .write(
                &frame,
                GcOperand::reference(self.current_environment(), schema),
                schema,
                function,
            );
        let realm =
            self.emit_async_function_execution_realm_context_from_activation(activation, function);
        let initialization = PromiseReactionInitialization::AsyncFunction {
            activation,
            realm: realm.realm(),
        };
        self.emit_intrinsic_await_reactions(
            value,
            &initialization,
            &initialization,
            PromiseResolveRealmAuthority::AsyncExecution(&realm),
            function,
        )?;
        self.release_async_execution_realm_context(realm, function);
        frame.clear(function);
        Ok(())
    }
    fn emit_async_generator_await_with_continuation_inner(
        &mut self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        value: &ValueLocals,
        continuation: AsyncGeneratorAwaitContinuation,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let realm =
            self.emit_async_generator_execution_realm_context_from_activation(activation, function);
        let initialization = PromiseReactionInitialization::AsyncGenerator {
            activation,
            realm: realm.realm(),
            continuation,
        };
        let emitted = self.emit_intrinsic_await_reactions(
            value,
            &initialization,
            &initialization,
            PromiseResolveRealmAuthority::AsyncExecution(&realm),
            function,
        );
        self.release_async_execution_realm_context(realm, function);
        emitted
    }
    pub(crate) fn emit_intrinsic_await_with_handlers(
        &mut self,
        value: &ValueLocals,
        fulfilled: &ValueLocals,
        rejected: &ValueLocals,
        capability: &GcLocal<PromiseCapability>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let capability = schema
            .reserve_gc_local::<PromiseCapability, Nullable>(function)
            .initialize(capability.load(schema, function).nullable(), function);
        let fulfill = PromiseReactionInitialization::Default {
            handler: fulfilled,
            capability: &capability,
        };
        let reject = PromiseReactionInitialization::Default {
            handler: rejected,
            capability: &capability,
        };
        let emitted = self.emit_intrinsic_await_reactions(
            value,
            &fulfill,
            &reject,
            PromiseResolveRealmAuthority::CurrentFunction,
            function,
        );
        capability.clear(function);
        emitted
    }

    fn emit_intrinsic_await_reactions(
        &mut self,
        value: &ValueLocals,
        fulfill: &PromiseReactionInitialization<'_>,
        reject: &PromiseReactionInitialization<'_>,
        authority: PromiseResolveRealmAuthority<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let context = self.emit_intrinsic_promise_resolve_realm_context(authority, function)?;
        let resolved = schema.reserve_completion(function);
        self.emit_intrinsic_promise_resolve_to_locals(&context, value, &resolved, function)?;
        // Await propagates synchronous PromiseResolve failure before it suspends.
        // The caller owns catch/finally routing and activation/status commits.
        self.completion().copy_from(&resolved, function);
        resolved.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let selected = schema.reserve_gc_local(function).initialize(
            resolved
                .value()
                .cast_reference::<PromiseObject>(schema, function),
            function,
        );
        let fulfill_reaction =
            self.emit_initialize_promise_reaction(PromiseReactionType::Fulfill, fulfill, function);
        let reject_reaction =
            self.emit_initialize_promise_reaction(PromiseReactionType::Reject, reject, function);
        self.emit_route_promise_reaction_pair(
            &selected,
            &fulfill_reaction,
            &reject_reaction,
            function,
        )?;
        reject_reaction.clear(function);
        fulfill_reaction.clear(function);
        selected.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        resolved.clear(function);
        self.release_intrinsic_promise_resolve_realm_context(context, function);
        Ok(())
    }

    fn emit_async_generator_await_return_reactions_inner(
        &mut self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm =
            self.emit_async_generator_execution_realm_context_from_activation(activation, function);
        let context = self.emit_intrinsic_promise_resolve_realm_context(
            PromiseResolveRealmAuthority::AsyncExecution(&realm),
            function,
        )?;
        self.emit_intrinsic_promise_resolve_to_locals(&context, value, result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        let promise = schema.reserve_gc_local(function).initialize(
            result
                .value()
                .cast_reference::<PromiseObject>(schema, function),
            function,
        );
        let initialization = PromiseReactionInitialization::AsyncGenerator {
            activation,
            realm: realm.realm(),
            continuation: AsyncGeneratorAwaitContinuation::AwaitReturn,
        };
        self.emit_owned_promise_record_reactions(&promise, initialization, function)?;
        promise.clear(function);
        function.instruction(&Instruction::End);
        self.release_intrinsic_promise_resolve_realm_context(context, function);
        self.release_async_execution_realm_context(realm, function);
        Ok(())
    }

    pub(crate) fn emit_promise_prototype_then(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        receiver.copy_from(
            self.body_entry_locals()
                .expect("Promise then ordinary entry")
                .this_value(),
            function,
        );
        let pending = schema.reserve_completion(function);
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<PromiseObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let prototype = self.emit_load_promise_prototype_receiver_type_error_prototype(function);
        self.emit_throw_promise_then_incompatible_receiver_error(prototype, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let promise = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<PromiseObject>(schema, function),
            function,
        );
        let on_fulfilled = schema.reserve_value_local(function);
        let on_rejected = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &on_fulfilled, function);
        self.emit_builtin_arg_to_value(1, &on_rejected, function);
        let species = self.emit_current_function_promise_species_realm_context(function);
        self.emit_promise_species_constructor(species, &receiver, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let executor =
            self.emit_current_function_promise_internal_function_materialization_context(function);
        let capability = self.emit_new_promise_capability(&executor, pending.value(), function)?;
        self.release_promise_internal_function_materialization_context(executor, function);
        self.emit_perform_promise_then(
            &promise,
            &on_fulfilled,
            &on_rejected,
            &capability,
            function,
        )?;
        let result = schema.reserve_value_local(function);
        self.emit_read_promise_capability_promise(&capability, &result, function);
        self.completion().set_normal(&result, function);
        result.clear(function);
        capability.clear(function);
        on_rejected.clear(function);
        on_fulfilled.clear(function);
        promise.clear(function);
        pending.clear(function);
        receiver.clear(function);
        Ok(())
    }

    pub(crate) fn emit_promise_prototype_catch(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        receiver.copy_from(
            self.body_entry_locals()
                .expect("Promise catch ordinary entry")
                .this_value(),
            function,
        );
        let on_rejected = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &on_rejected, function);
        let pending = schema.reserve_completion(function);
        self.emit_value_to_object_locals(&receiver, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let target = schema.reserve_value_local(function);
        target.copy_from(pending.value(), function);
        let key_string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("then", function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &key_string, function);
        self.emit_object_read_with_throw_routing(
            &target,
            &receiver,
            &key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let invocation = self.emit_validate_promise_prototype_then_invocation(
            pending.value(),
            &receiver,
            function,
        )?;
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        self.emit_call_validated_promise_prototype_then_invocation(
            invocation,
            &undefined,
            &on_rejected,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        undefined.clear(function);
        key.clear(function);
        key_string.clear(function);
        target.clear(function);
        pending.clear(function);
        on_rejected.clear(function);
        receiver.clear(function);
        Ok(())
    }

    pub(crate) fn emit_promise_prototype_finally(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        receiver.copy_from(
            self.body_entry_locals()
                .expect("Promise finally ordinary entry")
                .this_value(),
            function,
        );
        let pending = schema.reserve_completion(function);
        self.emit_is_heap_object_like_tag_i32(receiver.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let prototype = self.emit_load_promise_prototype_receiver_type_error_prototype(function);
        self.emit_throw_promise_finally_non_object_receiver_error(prototype, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let on_finally = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &on_finally, function);
        let species = self.emit_current_function_promise_species_realm_context(function);
        self.emit_promise_species_constructor(species, &receiver, &pending, function)?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let constructor = schema.reserve_value_local(function);
        constructor.copy_from(pending.value(), function);
        let then_finally = schema.reserve_value_local(function);
        let catch_finally = schema.reserve_value_local(function);
        then_finally.copy_from(&on_finally, function);
        catch_finally.copy_from(&on_finally, function);
        self.emit_is_callable_i32(&on_finally, function)?;
        function.instruction(&Instruction::If(BlockType::Empty));
        let callback = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&on_finally, function),
            function,
        );
        let captured_constructor = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&constructor, function),
            function,
        );
        let context = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PromiseFinallyContext>().construct(
                (
                    GcOperand::reference(&callback, schema),
                    GcOperand::reference(&captured_constructor, schema),
                ),
                function,
            ),
            function,
        );
        let materialization =
            self.emit_current_function_promise_internal_function_materialization_context(function);
        let fulfilled = self.emit_promise_internal_function_value(
            PromiseInternalFunction::ThenFinally(&context),
            &materialization,
            function,
        )?;
        then_finally.set_reference(&fulfilled, schema, function);
        fulfilled.clear(function);
        let rejected = self.emit_promise_internal_function_value(
            PromiseInternalFunction::CatchFinally(&context),
            &materialization,
            function,
        )?;
        catch_finally.set_reference(&rejected, schema, function);
        rejected.clear(function);
        self.release_promise_internal_function_materialization_context(materialization, function);
        context.clear(function);
        captured_constructor.clear(function);
        callback.clear(function);
        function.instruction(&Instruction::End);
        let key_string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("then", function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &key_string, function);
        self.emit_object_read_with_throw_routing(
            &receiver,
            &receiver,
            &key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        let invocation = self.emit_validate_promise_prototype_then_invocation(
            pending.value(),
            &receiver,
            function,
        )?;
        self.emit_call_validated_promise_prototype_then_invocation(
            invocation,
            &then_finally,
            &catch_finally,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw_if_needed(function);
        key.clear(function);
        key_string.clear(function);
        catch_finally.clear(function);
        then_finally.clear(function);
        constructor.clear(function);
        on_finally.clear(function);
        pending.clear(function);
        receiver.clear(function);
        Ok(())
    }

    fn emit_run_async_continuation_job(
        &mut self,
        reaction: &GcLocal<PromiseReaction>,
        rejected: I32Local,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let activation = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseReaction>()
                .field(PromiseReactionSchema::ASYNC_ACTIVATION)
                .read(reaction, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        schema
            .struct_type::<AsyncActivation>()
            .field(AsyncActivationSchema::COMPLETED)
            .read(&activation, schema, function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(argument, function),
            function,
        );
        schema
            .struct_type::<AsyncActivation>()
            .field(AsyncActivationSchema::RESUME_VALUE)
            .write(
                &activation,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
        rejected.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .struct_type::<AsyncActivation>()
            .field(AsyncActivationSchema::RESUME_COMPLETION)
            .write(
                &activation,
                GcOperand::constant(AwaitCompletionKind::Throw),
                schema,
                function,
            );
        function.instruction(&Instruction::Else);
        schema
            .struct_type::<AsyncActivation>()
            .field(AsyncActivationSchema::RESUME_COMPLETION)
            .write(
                &activation,
                GcOperand::constant(AwaitCompletionKind::Normal),
                schema,
                function,
            );
        function.instruction(&Instruction::End);
        let body = schema.reserve_completion(function);
        body.initialize(function);
        self.emit_saved_async_body_call(&activation, &body, function);
        self.emit_complete_async_entry_invocation(&activation, &body, function)?;
        body.clear(function);
        stored.clear(function);
        function.instruction(&Instruction::End);
        activation.clear(function);
        self.completion().initialize(function);
        Ok(())
    }

    fn emit_remove_async_generator_queue_head(
        &self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        request: &GcLocal<AsyncGeneratorRequest>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let next = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncGeneratorRequest>()
                .field(AsyncGeneratorRequestSchema::NEXT)
                .read(request, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::REQUEST_HEAD)
            .write(
                activation,
                GcOperand::reference(&next, schema),
                schema,
                function,
            );
        next.load(schema, function).is_null(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::REQUEST_TAIL)
            .write(activation, GcOperand::null(schema), schema, function);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<AsyncGeneratorRequest>()
            .field(AsyncGeneratorRequestSchema::NEXT)
            .write(request, GcOperand::null(schema), schema, function);
        next.clear(function);
    }

    pub(crate) fn emit_complete_async_generator_step(
        &mut self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        completion: &CompletionLocals,
        kind: AsyncGeneratorCompleteStepKind,
        realm: Option<&GcLocal<RealmRecord>>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let request = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncGeneratorActivation>()
                .field(AsyncGeneratorActivationSchema::ACTIVE_REQUEST)
                .read(activation, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let capability = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncGeneratorRequest>()
                .field(AsyncGeneratorRequestSchema::CAPABILITY)
                .read(&request, schema, function)
                .reference(),
            function,
        );
        self.emit_remove_async_generator_queue_head(activation, &request, function);
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::ACTIVE_REQUEST)
            .write(activation, GcOperand::null(schema), schema, function);
        let settled = schema.reserve_completion(function);
        settled.initialize(function);
        completion.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_call_promise_capability(
            &capability,
            PromiseSettlement::Reject,
            completion.value(),
            &settled,
            function,
        )?;
        function.instruction(&Instruction::Else);
        completion.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        // CompleteStep allocates in the explicitly supplied previous context,
        // or the current execution context. A capability's Promise is arbitrary.
        let current_realm = realm.is_none().then(|| self.load_current_realm(function));
        let realm = realm
            .or(current_realm.as_ref())
            .expect("iterator result Realm is selected");
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            function,
        );
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
            function,
        );
        for (name, value) in [("value", completion.value())] {
            let string = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
            let key = PropertyKeyLocals::from_string(schema, &string, function);
            self.emit_object_append_data_property_with_flags(
                &object, &key, value, true, true, true, function,
            )?;
            key.clear(function);
            string.clear(function);
        }
        let done = schema.reserve_value_local(function);
        done.set_scalar(
            ScalarValue::Boolean(matches!(kind, AsyncGeneratorCompleteStepKind::Completed)),
            function,
        );
        let done_string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("done", function)?,
            function,
        );
        let done_key = PropertyKeyLocals::from_string(schema, &done_string, function);
        self.emit_object_append_data_property_with_flags(
            &object, &done_key, &done, true, true, true, function,
        )?;
        let result = schema.reserve_value_local(function);
        result.set_reference(&object, schema, function);
        self.emit_call_promise_capability(
            &capability,
            PromiseSettlement::Fulfill,
            &result,
            &settled,
            function,
        )?;
        result.clear(function);
        done_key.clear(function);
        done_string.clear(function);
        done.clear(function);
        object.clear(function);
        prototype.clear(function);
        if let Some(realm) = current_realm {
            realm.clear(function);
        }
        function.instruction(&Instruction::End);
        if matches!(kind, AsyncGeneratorCompleteStepKind::Completed) {
            let frame = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<AsyncGeneratorActivation>()
                    .field(AsyncGeneratorActivationSchema::FRAME)
                    .read(activation, schema, function)
                    .reference(),
                function,
            );
            let lists = schema.reserve_gc_local(function).initialize(
                schema
                    .array_type::<PrivateArgumentListTable>()
                    .fixed(std::iter::empty(), function),
                function,
            );
            schema
                .struct_type::<InvocationFrame>()
                .field(InvocationFrameSchema::PRIVATE_ARGUMENT_LISTS)
                .write(
                    &frame,
                    GcOperand::reference(&lists, schema),
                    schema,
                    function,
                );
            lists.clear(function);
            frame.clear(function);
        }
        self.completion().initialize(function);
        settled.clear(function);
        capability.clear(function);
        request.clear(function);
        Ok(())
    }

    fn emit_drain_async_generator_queue_inner(
        &mut self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let stopped = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        stopped.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        let head = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncGeneratorActivation>()
                .field(AsyncGeneratorActivationSchema::REQUEST_HEAD)
                .read(activation, schema, function)
                .reference(),
            function,
        );
        head.load(schema, function).is_null(function);
        function.instruction(&Instruction::BrIf(1));
        let request = schema.reserve_gc_local(function).initialize(
            head.load(schema, function).require_non_null(function),
            function,
        );
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::ACTIVE_REQUEST)
            .write(
                activation,
                GcOperand::nullable_reference(&request, schema),
                schema,
                function,
            );
        let value = schema.reserve_value_local(function);
        let record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncGeneratorRequest>()
                .field(AsyncGeneratorRequestSchema::COMPLETION_VALUE)
                .read(&request, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&record, &value, schema, function);
        let kind = schema.reserve_i32_local(function);
        schema
            .struct_type::<AsyncGeneratorRequest>()
            .field(AsyncGeneratorRequestSchema::COMPLETION_KIND)
            .read(&request, schema, function)
            .store(kind, function);
        let step = schema.reserve_completion(function);
        step.initialize(function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorRequestCompletionKind::Normal,
        )));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        step.value().set_undefined(function);
        self.emit_complete_async_generator_step(
            activation,
            &step,
            AsyncGeneratorCompleteStepKind::Completed,
            None,
            function,
        )?;
        function.instruction(&Instruction::Else);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorRequestCompletionKind::Throw,
        )));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        step.set_throw(&value, function);
        self.emit_complete_async_generator_step(
            activation,
            &step,
            AsyncGeneratorCompleteStepKind::Completed,
            None,
            function,
        )?;
        function.instruction(&Instruction::Else);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorRequestCompletionKind::Return,
        )));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        self.emit_async_generator_await_return_reactions(activation, &value, &step, function)?;
        step.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_complete_async_generator_step(
            activation,
            &step,
            AsyncGeneratorCompleteStepKind::Completed,
            None,
            function,
        )?;
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(1));
        stopped.store(function);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        step.clear(function);
        schema.release_i32_local(kind, function);
        record.clear(function);
        value.clear(function);
        request.clear(function);
        head.clear(function);
        stopped.load(function);
        function.instruction(&Instruction::BrIf(1));
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        stopped.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::ACTIVE_REQUEST)
            .write(activation, GcOperand::null(schema), schema, function);
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::EXECUTION_STATE)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorExecutionState::Completed),
                schema,
                function,
            );
        function.instruction(&Instruction::End);
        schema.release_i32_local(stopped, function);
        self.completion().initialize(function);
        Ok(())
    }

    fn emit_async_generator_reaction_activation(
        &self,
        reaction: &GcLocal<PromiseReaction>,
        require_await: bool,
        function: &mut Function,
    ) -> GcLocal<AsyncGeneratorActivation> {
        let schema = self.runtime_schema();
        let activation = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseReaction>()
                .field(PromiseReactionSchema::ASYNC_GENERATOR)
                .read(reaction, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        if require_await {
            for (field, expected) in [(
                AsyncGeneratorActivationSchema::EXECUTION_STATE,
                GcI32Constant::encode(AsyncGeneratorExecutionState::Executing),
            )] {
                schema
                    .struct_type::<AsyncGeneratorActivation>()
                    .field(field)
                    .read(&activation, schema, function);
                function.instruction(&Instruction::I32Const(expected));
                function.instruction(&Instruction::I32Ne);
                function.instruction(&Instruction::If(BlockType::Empty));
                function.instruction(&Instruction::Unreachable);
                function.instruction(&Instruction::End);
            }
            schema
                .struct_type::<AsyncGeneratorActivation>()
                .field(AsyncGeneratorActivationSchema::BODY_STATUS)
                .read(&activation, schema, function);
            function.instruction(&Instruction::I32Const(GcI32Constant::encode(
                AsyncGeneratorBodyStatus::Await,
            )));
            function.instruction(&Instruction::I32Ne);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::Unreachable);
            function.instruction(&Instruction::End);
            let active = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<AsyncGeneratorActivation>()
                    .field(AsyncGeneratorActivationSchema::ACTIVE_REQUEST)
                    .read(&activation, schema, function)
                    .reference()
                    .require_non_null(function),
                function,
            );
            let head = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<AsyncGeneratorActivation>()
                    .field(AsyncGeneratorActivationSchema::REQUEST_HEAD)
                    .read(&activation, schema, function)
                    .reference(),
                function,
            );
            active.load(schema, function);
            head.load(schema, function);
            function.instruction(&Instruction::RefEq);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::Unreachable);
            function.instruction(&Instruction::End);
            head.clear(function);
            active.clear(function);
        }
        activation
    }

    pub(crate) fn emit_async_generator_resume_value(
        &self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        value: &ValueLocals,
        rejected: I32Local,
        fulfilled: AsyncGeneratorResumeKind,
        rejection: AsyncGeneratorResumeKind,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(value, function),
            function,
        );
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::RESUME_VALUE)
            .write(
                activation,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
        rejected.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::RESUME_KIND)
            .write(activation, GcOperand::constant(rejection), schema, function);
        function.instruction(&Instruction::Else);
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::RESUME_KIND)
            .write(activation, GcOperand::constant(fulfilled), schema, function);
        function.instruction(&Instruction::End);
        stored.clear(function);
    }

    fn emit_run_async_generator_await_job(
        &mut self,
        reaction: &GcLocal<PromiseReaction>,
        rejected: I32Local,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let activation = self.emit_async_generator_reaction_activation(reaction, true, function);
        self.emit_async_generator_resume_value(
            &activation,
            argument,
            rejected,
            AsyncGeneratorResumeKind::Fulfill,
            AsyncGeneratorResumeKind::Reject,
            function,
        );
        self.emit_start_async_generator_body(&activation, function)?;
        activation.clear(function);
        Ok(())
    }

    fn emit_run_async_generator_await_return_job(
        &mut self,
        reaction: &GcLocal<PromiseReaction>,
        rejected: I32Local,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let activation = self.emit_async_generator_reaction_activation(reaction, false, function);
        let completion = schema.reserve_completion(function);
        completion.initialize(function);
        rejected.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        completion.set_throw(argument, function);
        function.instruction(&Instruction::Else);
        completion.set_normal(argument, function);
        function.instruction(&Instruction::End);
        self.emit_complete_async_generator_step(
            &activation,
            &completion,
            AsyncGeneratorCompleteStepKind::Completed,
            None,
            function,
        )?;
        self.emit_drain_async_generator_queue(&activation, function)?;
        completion.clear(function);
        activation.clear(function);
        Ok(())
    }

    fn emit_run_async_generator_yield_return_job(
        &mut self,
        reaction: &GcLocal<PromiseReaction>,
        rejected: I32Local,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let activation = self.emit_async_generator_reaction_activation(reaction, true, function);
        self.emit_async_generator_resume_value(
            &activation,
            argument,
            rejected,
            AsyncGeneratorResumeKind::Return,
            AsyncGeneratorResumeKind::Throw,
            function,
        );
        self.emit_start_async_generator_body(&activation, function)?;
        activation.clear(function);
        Ok(())
    }

    fn emit_run_async_generator_yield_job(
        &mut self,
        reaction: &GcLocal<PromiseReaction>,
        rejected: I32Local,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let activation = self.emit_async_generator_reaction_activation(reaction, false, function);
        rejected.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_async_generator_resume_value(
            &activation,
            argument,
            rejected,
            AsyncGeneratorResumeKind::Fulfill,
            AsyncGeneratorResumeKind::Reject,
            function,
        );
        self.emit_start_async_generator_body(&activation, function)?;
        function.instruction(&Instruction::Else);
        let resume = schema.reserve_i32_local(function);
        self.emit_complete_async_generator_yield(&activation, argument, resume, None, function)?;
        resume.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_start_async_generator_body(&activation, function)?;
        function.instruction(&Instruction::End);
        schema.release_i32_local(resume, function);
        function.instruction(&Instruction::End);
        activation.clear(function);
        Ok(())
    }

    pub(crate) fn emit_complete_async_generator_yield(
        &mut self,
        activation: &GcLocal<AsyncGeneratorActivation>,
        value: &ValueLocals,
        resume: I32Local,
        realm: Option<&GcLocal<RealmRecord>>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let incoming = schema.reserve_completion(function);
        incoming.copy_from(self.completion(), function);
        function.instruction(&Instruction::I32Const(0));
        resume.store(function);
        let completion = schema.reserve_completion(function);
        completion.initialize(function);
        completion.set_normal(value, function);
        self.emit_complete_async_generator_step(
            activation,
            &completion,
            AsyncGeneratorCompleteStepKind::Yielded,
            realm,
            function,
        )?;
        let head = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncGeneratorActivation>()
                .field(AsyncGeneratorActivationSchema::REQUEST_HEAD)
                .read(activation, schema, function)
                .reference(),
            function,
        );
        head.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let request = schema.reserve_gc_local(function).initialize(
            head.load(schema, function).require_non_null(function),
            function,
        );
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::ACTIVE_REQUEST)
            .write(
                activation,
                GcOperand::nullable_reference(&request, schema),
                schema,
                function,
            );
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<AsyncGeneratorRequest>()
                .field(AsyncGeneratorRequestSchema::COMPLETION_VALUE)
                .read(&request, schema, function)
                .reference(),
            function,
        );
        schema.struct_type::<StoredValue>().read_into(
            &stored,
            completion.value(),
            schema,
            function,
        );
        let kind = schema.reserve_i32_local(function);
        schema
            .struct_type::<AsyncGeneratorRequest>()
            .field(AsyncGeneratorRequestSchema::COMPLETION_KIND)
            .read(&request, schema, function)
            .store(kind, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorRequestCompletionKind::Return,
        )));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_async_generator_yield_return_reactions(activation, completion.value(), function)?;
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let rejected = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(1));
        rejected.store(function);
        self.emit_async_generator_resume_value(
            activation,
            self.completion().value(),
            rejected,
            AsyncGeneratorResumeKind::Return,
            AsyncGeneratorResumeKind::Throw,
            function,
        );
        function.instruction(&Instruction::I32Const(1));
        resume.store(function);
        schema.release_i32_local(rejected, function);
        function.instruction(&Instruction::Else);
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::BODY_STATUS)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorBodyStatus::Await),
                schema,
                function,
            );
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::EXECUTION_STATE)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorExecutionState::Executing),
                schema,
                function,
            );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        let rejected = schema.reserve_i32_local(function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorRequestCompletionKind::Throw,
        )));
        function.instruction(&Instruction::I32Eq);
        rejected.store(function);
        rejected.load(function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            AsyncGeneratorRequestCompletionKind::Normal,
        )));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        self.emit_async_generator_resume_value(
            activation,
            completion.value(),
            rejected,
            AsyncGeneratorResumeKind::Normal,
            AsyncGeneratorResumeKind::Throw,
            function,
        );
        function.instruction(&Instruction::I32Const(1));
        resume.store(function);
        schema.release_i32_local(rejected, function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(kind, function);
        stored.clear(function);
        request.clear(function);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::EXECUTION_STATE)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorExecutionState::SuspendedYield),
                schema,
                function,
            );
        // A queued Return stays Executing while its Await reactions are pending.
        resume.load(function);
        head.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema
            .struct_type::<AsyncGeneratorActivation>()
            .field(AsyncGeneratorActivationSchema::EXECUTION_STATE)
            .write(
                activation,
                GcOperand::constant(AsyncGeneratorExecutionState::Executing),
                schema,
                function,
            );
        function.instruction(&Instruction::End);
        head.clear(function);
        self.completion().copy_from(&incoming, function);
        completion.clear(function);
        incoming.clear(function);
        Ok(())
    }

    fn emit_run_promise_reaction_callback(
        &mut self,
        kind: PromiseReactionCallbackKind,
        reaction: &GcLocal<PromiseReaction>,
        rejected: I32Local,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        match kind {
            PromiseReactionCallbackKind::Default => {
                self.emit_run_default_promise_reaction_job(reaction, rejected, argument, function)
            }
            PromiseReactionCallbackKind::AsyncFunction => {
                self.emit_run_async_continuation_job(reaction, rejected, argument, function)
            }
            PromiseReactionCallbackKind::AsyncGeneratorAwait => {
                self.emit_run_async_generator_await_job(reaction, rejected, argument, function)
            }
            PromiseReactionCallbackKind::AsyncGeneratorAwaitReturn => self
                .emit_run_async_generator_await_return_job(reaction, rejected, argument, function),
            PromiseReactionCallbackKind::AsyncGeneratorYield => {
                self.emit_run_async_generator_yield_job(reaction, rejected, argument, function)
            }
            PromiseReactionCallbackKind::AsyncGeneratorYieldReturn => self
                .emit_run_async_generator_yield_return_job(reaction, rejected, argument, function),
            PromiseReactionCallbackKind::AsyncFromSyncIterator => {
                self.emit_run_async_from_sync_iterator_job(reaction, rejected, argument, function)
            }
            PromiseReactionCallbackKind::ModuleBody => {
                let module = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<PromiseReaction>()
                        .field(PromiseReactionSchema::MODULE)
                        .read(reaction, schema, function)
                        .reference()
                        .require_non_null(function),
                    function,
                );
                let result =
                    self.emit_run_module_body_reaction(&module, rejected, argument, function);
                module.clear(function);
                result
            }
            PromiseReactionCallbackKind::ModuleJoin => {
                let join = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<PromiseReaction>()
                        .field(PromiseReactionSchema::MODULE_JOIN)
                        .read(reaction, schema, function)
                        .reference()
                        .require_non_null(function),
                    function,
                );
                let result =
                    self.emit_run_module_join_reaction(&join, rejected, argument, function);
                join.clear(function);
                result
            }
        }
    }

    fn emit_run_promise_reaction_job(
        &mut self,
        reaction: &GcLocal<PromiseReaction>,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let kind = schema.reserve_i32_local(function);
        let reaction_type = schema.reserve_i32_local(function);
        let rejected = schema.reserve_i32_local(function);
        schema
            .struct_type::<PromiseReaction>()
            .field(PromiseReactionSchema::TYPE)
            .read(reaction, schema, function)
            .store(reaction_type, function);
        let mut arms = 0;
        for selected in PromiseReactionType::ALL {
            reaction_type.load(function);
            function.instruction(&Instruction::I32Const(GcI32Constant::encode(selected)));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::I32Const(i32::from(selected.is_rejected())));
            rejected.store(function);
            function.instruction(&Instruction::Else);
            arms += 1;
        }
        function.instruction(&Instruction::Unreachable);
        for _ in 0..arms {
            function.instruction(&Instruction::End);
        }
        schema
            .struct_type::<PromiseReaction>()
            .field(PromiseReactionSchema::CALLBACK_KIND)
            .read(reaction, schema, function)
            .store(kind, function);
        let mut arms = 0;
        for selected in PromiseReactionCallbackKind::ALL {
            kind.load(function);
            function.instruction(&Instruction::I32Const(GcI32Constant::encode(selected)));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            self.emit_run_promise_reaction_callback(
                selected, reaction, rejected, argument, function,
            )?;
            function.instruction(&Instruction::Else);
            arms += 1;
        }
        function.instruction(&Instruction::Unreachable);
        for _ in 0..arms {
            function.instruction(&Instruction::End);
        }
        schema.release_i32_local(rejected, function);
        schema.release_i32_local(reaction_type, function);
        schema.release_i32_local(kind, function);
        Ok(())
    }

    fn emit_run_default_promise_reaction_job(
        &mut self,
        reaction: &GcLocal<PromiseReaction>,
        rejected: I32Local,
        argument: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let stored_handler = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseReaction>()
                .field(PromiseReactionSchema::HANDLER)
                .read(reaction, schema, function)
                .reference(),
            function,
        );
        let handler = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored_handler, &handler, schema, function);
        let capability = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseReaction>()
                .field(PromiseReactionSchema::CAPABILITY)
                .read(reaction, schema, function)
                .reference(),
            function,
        );
        let handled = schema.reserve_completion(function);
        rejected.load(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        handled.set_throw(argument, function);
        function.instruction(&Instruction::Else);
        handled.set_normal(argument, function);
        function.instruction(&Instruction::End);
        self.emit_is_callable_i32(&handler, function)?;
        function.instruction(&Instruction::If(BlockType::Empty));
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[argument], function);
        self.emit_function_or_proxy_call_with_argv(
            &handler, &undefined, &arguments, &handled, function,
        )?;
        arguments.clear(function);
        undefined.clear(function);
        function.instruction(&Instruction::End);
        capability.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let selected = schema.reserve_gc_local(function).initialize(
            capability.load(schema, function).require_non_null(function),
            function,
        );
        let settled = schema.reserve_completion(function);
        handled.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_call_promise_capability(
            &selected,
            PromiseSettlement::Reject,
            handled.value(),
            &settled,
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_call_promise_capability(
            &selected,
            PromiseSettlement::Fulfill,
            handled.value(),
            &settled,
            function,
        )?;
        function.instruction(&Instruction::End);
        settled.clear(function);
        selected.clear(function);
        function.instruction(&Instruction::End);
        handled.clear(function);
        capability.clear(function);
        handler.clear(function);
        stored_handler.clear(function);
        self.completion().initialize(function);
        Ok(())
    }

    fn emit_run_promise_thenable_job(
        &mut self,
        job: &GcLocal<PromiseThenableJob>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let promise = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseThenableJob>()
                .field(PromiseThenableJobSchema::PROMISE)
                .read(job, schema, function)
                .reference(),
            function,
        );
        let stored_value = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseThenableJob>()
                .field(PromiseThenableJobSchema::THENABLE)
                .read(job, schema, function)
                .reference(),
            function,
        );
        let stored_method = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseThenableJob>()
                .field(PromiseThenableJobSchema::THEN_METHOD)
                .read(job, schema, function)
                .reference(),
            function,
        );
        let thenable = schema.reserve_value_local(function);
        let method = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored_value, &thenable, schema, function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored_method, &method, schema, function);
        let resolve = schema.reserve_value_local(function);
        let reject = schema.reserve_value_local(function);
        // NewPromiseResolveThenableJob creates these functions in the running
        // job's Realm, selected from GetFunctionRealm(then), not the Promise.
        let job_realm = self.load_current_realm(function);
        let resolving_context = self
            .emit_promise_internal_function_materialization_context_from_realm(
                &job_realm, function,
            );
        job_realm.clear(function);
        self.emit_create_promise_resolving_functions(
            &promise,
            &resolving_context,
            &resolve,
            &reject,
            function,
        )?;
        self.release_promise_internal_function_materialization_context(resolving_context, function);
        let arguments = self.emit_pre_evaluated_arg_vector(&[&resolve, &reject], function);
        let called = schema.reserve_completion(function);
        self.emit_function_or_proxy_call_with_argv(
            &method, &thenable, &arguments, &called, function,
        )?;
        called.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        // Invoke the same shared already-resolved guard as user rejection.
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let rejection_arguments = self.emit_pre_evaluated_arg_vector(&[called.value()], function);
        let ignored = schema.reserve_completion(function);
        self.emit_function_or_proxy_call_with_argv(
            &reject,
            &undefined,
            &rejection_arguments,
            &ignored,
            function,
        )?;
        ignored.clear(function);
        rejection_arguments.clear(function);
        undefined.clear(function);
        function.instruction(&Instruction::End);
        called.clear(function);
        arguments.clear(function);
        reject.clear(function);
        resolve.clear(function);
        method.clear(function);
        thenable.clear(function);
        stored_method.clear(function);
        stored_value.clear(function);
        promise.clear(function);
        self.completion().initialize(function);
        Ok(())
    }

    fn emit_drain_promise_jobs_inner(&mut self, function: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let saved = schema.reserve_completion(function);
        saved.copy_from(self.completion(), function);
        let saved_name = schema.load_throw_diagnostic(ThrowDiagnosticRole::Name, function);
        let saved_message = schema.load_throw_diagnostic(ThrowDiagnosticRole::Message, function);
        let saved_constructor =
            schema.load_throw_diagnostic(ThrowDiagnosticRole::ConstructorName, function);
        let saved_realm = self.load_current_realm(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        let head = schema.load_pending_job_queue(RuntimeQueueEnd::Head, function);
        head.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::BrIf(1));
        let job = schema.reserve_gc_local(function).initialize(
            head.load(schema, function).require_non_null(function),
            function,
        );
        let next = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PendingJob>()
                .field(PendingJobSchema::NEXT)
                .read(&job, schema, function)
                .reference(),
            function,
        );
        schema.replace_pending_job_queue(RuntimeQueueEnd::Head, &next, function);
        next.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema.clear_pending_job_queue(RuntimeQueueEnd::Tail, function);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<PendingJob>()
            .field(PendingJobSchema::NEXT)
            .write(&job, GcOperand::null(schema), schema, function);
        self.replace_current_realm(&saved_realm, function);
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PendingJob>()
                .field(PendingJobSchema::REALM)
                .read(&job, schema, function)
                .reference(),
            function,
        );
        realm.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let selected = schema.reserve_gc_local(function).initialize(
            realm.load(schema, function).require_non_null(function),
            function,
        );
        self.replace_current_realm(&selected, function);
        selected.clear(function);
        function.instruction(&Instruction::End);
        let kind = schema.reserve_i32_local(function);
        schema
            .struct_type::<PendingJob>()
            .field(PendingJobSchema::KIND)
            .read(&job, schema, function)
            .store(kind, function);
        let mut arms = 0;
        for selected in PromiseJobKind::ALL {
            kind.load(function);
            function.instruction(&Instruction::I32Const(GcI32Constant::encode(selected)));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::If(BlockType::Empty));
            match selected {
                PromiseJobKind::FinalizationCleanup => {
                    let registry = schema.reserve_gc_local(function).initialize(
                        schema
                            .field(PendingJobSchema::REGISTRY)
                            .read(&job, schema, function)
                            .reference()
                            .require_non_null(function),
                        function,
                    );
                    self.emit_run_finalization_job(&registry, function)?;
                    registry.clear(function);
                }
                PromiseJobKind::Reaction => {
                    let reaction = schema.reserve_gc_local(function).initialize(
                        schema
                            .struct_type::<PendingJob>()
                            .field(PendingJobSchema::REACTION)
                            .read(&job, schema, function)
                            .reference()
                            .require_non_null(function),
                        function,
                    );
                    let stored = schema.reserve_gc_local(function).initialize(
                        schema
                            .struct_type::<PendingJob>()
                            .field(PendingJobSchema::ARGUMENT)
                            .read(&job, schema, function)
                            .reference(),
                        function,
                    );
                    let argument = schema.reserve_value_local(function);
                    schema
                        .struct_type::<StoredValue>()
                        .read_into(&stored, &argument, schema, function);
                    self.emit_run_promise_reaction_job(&reaction, &argument, function)?;
                    argument.clear(function);
                    stored.clear(function);
                    reaction.clear(function);
                }
                PromiseJobKind::ResolveThenable => {
                    let thenable = schema.reserve_gc_local(function).initialize(
                        schema
                            .struct_type::<PendingJob>()
                            .field(PendingJobSchema::THENABLE)
                            .read(&job, schema, function)
                            .reference()
                            .require_non_null(function),
                        function,
                    );
                    self.emit_run_promise_thenable_job(&thenable, function)?;
                    thenable.clear(function);
                }
            }
            function.instruction(&Instruction::Else);
            arms += 1;
        }
        function.instruction(&Instruction::Unreachable);
        for _ in 0..arms {
            function.instruction(&Instruction::End);
        }
        schema.release_i32_local(kind, function);
        realm.clear(function);
        next.clear(function);
        job.clear(function);
        head.clear(function);
        self.emit_finish_weak_job(function)?;
        if self
            .functions
            .monotonic_clock_nanos_import_function_index()
            .is_some()
        {
            self.emit_poll_atomics_wait_async_timeouts(function)?;
        }
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        self.replace_current_realm(&saved_realm, function);
        self.completion().copy_from(&saved, function);
        schema.replace_throw_diagnostic(ThrowDiagnosticRole::Name, &saved_name, function);
        schema.replace_throw_diagnostic(ThrowDiagnosticRole::Message, &saved_message, function);
        schema.replace_throw_diagnostic(
            ThrowDiagnosticRole::ConstructorName,
            &saved_constructor,
            function,
        );
        saved_realm.clear(function);
        saved_constructor.clear(function);
        saved_message.clear(function);
        saved_name.clear(function);
        saved.clear(function);
        Ok(())
    }

    fn emit_promise_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_promise_capability_executor(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let context = self.emit_promise_capability_executor_context(function);
        let resolve = schema.reserve_value_local(function);
        let reject = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        for (field, out) in [
            (PromiseCapabilityExecutorContextSchema::RESOLVE, &resolve),
            (PromiseCapabilityExecutorContextSchema::REJECT, &reject),
        ] {
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<PromiseCapabilityExecutorContext>()
                    .field(field)
                    .read(&context, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<StoredValue>()
                .read_into(&stored, out, schema, function);
            stored.clear(function);
        }
        let exit = self.open_frame(ControlFrameKind::Block, function);
        resolve.tag().load(function);
        function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        function.instruction(&Instruction::I32Ne);
        reject.tag().load(function);
        function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::PROMISE_CAPABILITY_EXECUTOR_CALLED_MORE_THAN_ONCE,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_builtin_arg_to_value(0, &resolve, function);
        self.emit_builtin_arg_to_value(1, &reject, function);
        for (field, value) in [
            (PromiseCapabilityExecutorContextSchema::RESOLVE, &resolve),
            (PromiseCapabilityExecutorContextSchema::REJECT, &reject),
        ] {
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(value, function),
                function,
            );
            schema
                .struct_type::<PromiseCapabilityExecutorContext>()
                .field(field)
                .write(
                    &context,
                    GcOperand::reference(&stored, schema),
                    schema,
                    function,
                );
            stored.clear(function);
        }
        self.completion().initialize(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        reject.clear(function);
        resolve.clear(function);
        context.clear(function);
        Ok(())
    }

    pub(crate) fn emit_promise_static_settle(
        &mut self,
        settlement: PromiseSettlement,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let constructor = schema.reserve_value_local(function);
        constructor.copy_from(
            self.body_entry_locals()
                .expect("Promise static entry")
                .this_value(),
            function,
        );
        let value = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &value, function);
        self.emit_promise_settle_value(settlement, &constructor, &value, function)?;
        value.clear(function);
        constructor.clear(function);
        Ok(())
    }

    /// PromiseResolve and the rejecting static operation use their live
    /// constructor/value operands without reading an observable resolve method.
    fn emit_promise_settle_value(
        &mut self,
        settlement: PromiseSettlement,
        constructor: &ValueLocals,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        if matches!(settlement, PromiseSettlement::Fulfill) {
            self.emit_is_heap_object_like_tag_i32(constructor.tag(), function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_throw_current_function_realm_type_error(
                RuntimeErrorMessage::PROMISE_RESOLVE_RECEIVER_IS_NOT_AN_OBJECT,
                &pending,
                function,
            )?;
            self.completion().copy_from(&pending, function);
            self.emit_branch_to_target(exit, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            value.reference().load(function);
            function.instruction(&Instruction::RefTestNonNull(
                schema
                    .reference_type::<PromiseObject>(GcNullability::NonNullable)
                    .heap_type,
            ));
            self.open_frame(ControlFrameKind::If, function);
            let name = schema.reserve_gc_local(function).initialize(
                self.emit_interned_string_reference("constructor", function)?,
                function,
            );
            let key = PropertyKeyLocals::from_string(schema, &name, function);
            self.emit_object_read_with_throw_routing(
                value,
                value,
                &key,
                &pending,
                AccessorThrowRouting::LeaveInCompletion,
                function,
            )?;
            self.emit_promise_abrupt_exit(&pending, exit, function);
            self.emit_tagged_payload_same_value_i32(constructor, pending.value(), function)?;
            self.open_frame(ControlFrameKind::If, function);
            self.completion().set_normal(value, function);
            self.emit_branch_to_target(exit, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            key.clear(function);
            name.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        let context =
            self.emit_current_function_promise_internal_function_materialization_context(function);
        let capability = self.emit_new_promise_capability(&context, constructor, function)?;
        self.emit_call_promise_capability(&capability, settlement, value, &pending, function)?;
        self.emit_promise_abrupt_exit(&pending, exit, function);
        let promise = schema.reserve_value_local(function);
        self.emit_read_promise_capability_promise(&capability, &promise, function);
        self.completion().set_normal(&promise, function);
        promise.clear(function);
        capability.clear(function);
        self.release_promise_internal_function_materialization_context(context, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        pending.clear(function);
        Ok(())
    }

    pub(crate) fn emit_promise_with_resolvers(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let constructor = schema.reserve_value_local(function);
        constructor.copy_from(
            self.body_entry_locals()
                .expect("withResolvers entry")
                .this_value(),
            function,
        );
        let context =
            self.emit_current_function_promise_internal_function_materialization_context(function);
        let capability = self.emit_new_promise_capability(&context, &constructor, function)?;
        let allocation =
            self.emit_current_function_promise_with_resolvers_result_allocation_context(function);
        let object =
            self.emit_alloc_promise_with_resolvers_result(allocation, &capability, function)?;
        let value = schema.reserve_value_local(function);
        value.set_reference(&object, schema, function);
        self.completion().set_normal(&value, function);
        value.clear(function);
        object.clear(function);
        capability.clear(function);
        self.release_promise_internal_function_materialization_context(context, function);
        constructor.clear(function);
        Ok(())
    }

    pub(crate) fn emit_promise_try(&mut self, function: &mut Function) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let constructor = schema.reserve_value_local(function);
        constructor.copy_from(
            self.body_entry_locals()
                .expect("Promise.try entry")
                .this_value(),
            function,
        );
        let callback = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &callback, function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        // sec-promise.try: only the receiver's Object check precedes Call.
        // Constructor validation and capability effects follow the callback.
        self.emit_is_heap_object_like_tag_i32(constructor.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::PROMISE_TRY_RECEIVER_IS_NOT_AN_OBJECT,
            &pending,
            function,
        )?;
        self.completion().copy_from(&pending, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_callable_i32(&callback, function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let prototype = self.emit_load_promise_try_callback_type_error_prototype(function);
        self.emit_throw_promise_try_non_callable_callback(prototype, &pending, function)?;
        function.instruction(&Instruction::Else);
        let arguments = self.emit_builtin_argument_vector_tail(1, function);
        self.emit_function_or_proxy_call_with_argv(
            &callback, &undefined, &arguments, &pending, function,
        )?;
        arguments.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_promise_settle_value(
            PromiseSettlement::Reject,
            &constructor,
            pending.value(),
            function,
        )?;
        function.instruction(&Instruction::Else);
        self.emit_promise_settle_value(
            PromiseSettlement::Fulfill,
            &constructor,
            pending.value(),
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        undefined.clear(function);
        callback.clear(function);
        constructor.clear(function);
        Ok(())
    }

    fn emit_promise_property_get(
        &mut self,
        target: &ValueLocals,
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
        self.emit_object_read_with_throw_routing(
            target,
            target,
            &key,
            result,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        key.clear(function);
        string.clear(function);
        Ok(())
    }

    fn emit_promise_combinator_reject_current_throw(
        &mut self,
        capability: &GcLocal<PromiseCapability>,
        iterator: &GcLocal<IteratorRecord, Nullable>,
        pending: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        iterator.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let record = schema.reserve_gc_local(function).initialize(
            iterator.load(schema, function).require_non_null(function),
            function,
        );
        schema
            .struct_type::<IteratorRecord>()
            .field(IteratorRecordSchema::DONE)
            .read(&record, schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let raw = schema.reserve_value_local(function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<IteratorRecord>()
                .field(IteratorRecordSchema::ITERATOR)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &raw, schema, function);
        self.emit_iterator_close_with_completion(&raw, pending, pending, function)?;
        stored.clear(function);
        raw.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        record.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let settled = schema.reserve_completion(function);
        settled.initialize(function);
        self.emit_call_promise_capability(
            capability,
            PromiseSettlement::Reject,
            pending.value(),
            &settled,
            function,
        )?;
        self.completion().copy_from(&settled, function);
        settled.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        let promise = schema.reserve_value_local(function);
        self.emit_read_promise_capability_promise(capability, &promise, function);
        self.completion().set_normal(&promise, function);
        promise.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        settled.clear(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_promise_combinator_iterator(
        &mut self,
        input: &ValueLocals,
        errors: &promise_combinator_algorithm_error_realm::PromiseCombinatorAlgorithmErrorRealmContext,
        mode: PromiseCombinatorMode,
        pending: &CompletionLocals,
        function: &mut Function,
    ) -> Result<GcLocal<IteratorRecord, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let result = schema
            .reserve_gc_local::<IteratorRecord, Nullable>(function)
            .initialize_null(schema, function);
        let boxed = schema.reserve_completion(function);
        boxed.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.compile_nullish_tagged_i32(input.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_promise_combinator_type_error(
            errors,
            mode.input_error(),
            pending,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_value_to_object_locals(input, &boxed, function)?;
        boxed.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        pending.copy_from(&boxed, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let symbol = schema.reserve_gc_local(function).initialize(
            self.emit_well_known_symbol_reference(WellKnownSymbol::Iterator, function)?,
            function,
        );
        let key = PropertyKeyLocals::from_symbol(schema, &symbol, function);
        self.emit_object_read_with_throw_routing(
            boxed.value(),
            input,
            &key,
            pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_callable_i32(pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_promise_combinator_type_error(
            errors,
            mode.method_error(),
            pending,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let method = schema.reserve_value_local(function);
        method.copy_from(pending.value(), function);
        let args = self.emit_pre_evaluated_arg_vector(&[], function);
        self.emit_function_or_proxy_call_with_argv(&method, input, &args, pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_heap_object_like_tag_i32(pending.value().tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_promise_combinator_type_error(
            errors,
            mode.method_result_error(),
            pending,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let iterator = schema.reserve_value_local(function);
        iterator.copy_from(pending.value(), function);
        self.emit_promise_property_get(&iterator, "next", pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let stored_iterator = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&iterator, function),
            function,
        );
        let stored_method = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(pending.value(), function),
            function,
        );
        result.replace(
            schema
                .struct_type::<IteratorRecord>()
                .construct(
                    (
                        GcOperand::reference(&stored_iterator, schema),
                        GcOperand::reference(&stored_method, schema),
                        GcOperand::boolean(false),
                    ),
                    function,
                )
                .nullable(),
            function,
        );
        stored_method.clear(function);
        stored_iterator.clear(function);
        iterator.clear(function);
        args.clear(function);
        method.clear(function);
        key.clear(function);
        symbol.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        boxed.clear(function);
        Ok(result)
    }

    fn emit_promise_combinator_step(
        &mut self,
        record: &GcLocal<IteratorRecord>,
        errors: &promise_combinator_algorithm_error_realm::PromiseCombinatorAlgorithmErrorRealmContext,
        mode: PromiseCombinatorMode,
        value: &ValueLocals,
        done: I32Local,
        pending: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let iterator = schema.reserve_value_local(function);
        let next = schema.reserve_value_local(function);
        for (field, out) in [
            (IteratorRecordSchema::ITERATOR, &iterator),
            (IteratorRecordSchema::NEXT_METHOD, &next),
        ] {
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<IteratorRecord>()
                    .field(field)
                    .read(record, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<StoredValue>()
                .read_into(&stored, out, schema, function);
            stored.clear(function);
        }
        function.instruction(&Instruction::I32Const(1));
        done.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let args = self.emit_pre_evaluated_arg_vector(&[], function);
        self.emit_function_or_proxy_call_with_argv(&next, &iterator, &args, pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_is_heap_object_like_tag_i32(pending.value().tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_promise_combinator_type_error(
            errors,
            mode.next_result_error(),
            pending,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let result = schema.reserve_value_local(function);
        result.copy_from(pending.value(), function);
        self.emit_promise_property_get(&result, "done", pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_promise_property_get(&result, "value", pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.copy_from(pending.value(), function);
        function.instruction(&Instruction::I32Const(0));
        done.store(function);
        result.clear(function);
        args.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<IteratorRecord>()
            .field(IteratorRecordSchema::DONE)
            .write(record, GcOperand::boolean_local(done), schema, function);
        next.clear(function);
        iterator.clear(function);
        Ok(())
    }

    fn emit_finish_promise_combinator_list(
        &mut self,
        shared: &GcLocal<PromiseCombinatorShared>,
        mode: PromiseCombinatorMode,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let list = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseCombinatorShared>()
                .field(PromiseCombinatorSharedSchema::VALUES)
                .read(shared, schema, function)
                .reference(),
            function,
        );
        let array = self.emit_array_from_argument_list(&list, function)?;
        let argument = schema.reserve_value_local(function);
        argument.set_reference(&array, schema, function);
        match mode {
            PromiseCombinatorMode::FirstFulfillment => {
                let realm = schema
                    .reserve_gc_local(function)
                    .initialize(self.emit_current_function_realm(function), function);
                self.emit_promise_any_aggregate_error(&argument, &realm, result, function)?;
                argument.copy_from(result.value(), function);
                realm.clear(function);
            }
            PromiseCombinatorMode::Values
            | PromiseCombinatorMode::SettledRecords
            | PromiseCombinatorMode::Race => result.set_normal(&argument, function),
        }
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let settle = schema.reserve_value_local(function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseCombinatorShared>()
                .field(PromiseCombinatorSharedSchema::SETTLE)
                .read(shared, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &settle, schema, function);
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let args = self.emit_pre_evaluated_arg_vector(&[&argument], function);
        self.emit_function_or_proxy_call_with_argv(&settle, &undefined, &args, result, function)?;
        args.clear(function);
        undefined.clear(function);
        stored.clear(function);
        settle.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        argument.clear(function);
        array.clear(function);
        list.clear(function);
        Ok(())
    }

    pub(crate) fn emit_promise_all_resolve_element(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_promise_combinator_element(
            PromiseCombinatorMode::Values,
            PromiseSettlement::Fulfill,
            function,
        )
    }
    pub(crate) fn emit_promise_all_settled_element(
        &mut self,
        settlement: PromiseSettlement,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_promise_combinator_element(
            PromiseCombinatorMode::SettledRecords,
            settlement,
            function,
        )
    }
    pub(crate) fn emit_promise_any_reject_element(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_promise_combinator_element(
            PromiseCombinatorMode::FirstFulfillment,
            PromiseSettlement::Reject,
            function,
        )
    }

    fn emit_promise_combinator_element(
        &mut self,
        mode: PromiseCombinatorMode,
        settlement: PromiseSettlement,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let element = self.emit_promise_element_context(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        schema
            .struct_type::<PromiseElementContext>()
            .field(PromiseElementContextSchema::ALREADY_CALLED)
            .read(&element, schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<PromiseElementContext>()
            .field(PromiseElementContextSchema::ALREADY_CALLED)
            .write(&element, GcOperand::boolean(true), schema, function);
        let shared = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseElementContext>()
                .field(PromiseElementContextSchema::SHARED)
                .read(&element, schema, function)
                .reference(),
            function,
        );
        let index = schema.reserve_i64_local(function);
        schema
            .struct_type::<PromiseElementContext>()
            .field(PromiseElementContextSchema::INDEX)
            .read(&element, schema, function)
            .store_i64(index, function);
        let value = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &value, function);
        match mode {
            PromiseCombinatorMode::SettledRecords => {
                let allocation =
                    self.emit_self_backed_promise_settlement_record_allocation_context(function);
                let record = self.emit_alloc_promise_settlement_record(
                    allocation, settlement, &value, function,
                )?;
                value.set_reference(&record, schema, function);
                record.clear(function);
            }
            PromiseCombinatorMode::Values
            | PromiseCombinatorMode::FirstFulfillment
            | PromiseCombinatorMode::Race => {}
        }
        self.emit_store_promise_combinator_list_entry(&shared, index, &value, function);
        let remaining = schema.reserve_i64_local(function);
        schema
            .struct_type::<PromiseCombinatorShared>()
            .field(PromiseCombinatorSharedSchema::REMAINING)
            .read(&shared, schema, function)
            .store_i64(remaining, function);
        remaining.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        remaining.store(function);
        schema
            .struct_type::<PromiseCombinatorShared>()
            .field(PromiseCombinatorSharedSchema::REMAINING)
            .write(&shared, GcOperand::i64_local(remaining), schema, function);
        remaining.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_finish_promise_combinator_list(&shared, mode, &pending, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(remaining, function);
        value.clear(function);
        schema.release_i64_local(index, function);
        shared.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&pending, function);
        function.instruction(&Instruction::Else);
        self.completion().initialize(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        element.clear(function);
        Ok(())
    }

    pub(crate) fn emit_promise_race(&mut self, function: &mut Function) -> Result<(), EmitError> {
        self.emit_promise_combinator(PromiseCombinatorMode::Race, function)
    }
    pub(crate) fn emit_promise_all(&mut self, function: &mut Function) -> Result<(), EmitError> {
        self.emit_promise_combinator(PromiseCombinatorMode::Values, function)
    }
    pub(crate) fn emit_promise_all_settled(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_promise_combinator(PromiseCombinatorMode::SettledRecords, function)
    }
    pub(crate) fn emit_promise_any(&mut self, function: &mut Function) -> Result<(), EmitError> {
        self.emit_promise_combinator(PromiseCombinatorMode::FirstFulfillment, function)
    }

    fn emit_promise_combinator(
        &mut self,
        mode: PromiseCombinatorMode,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let constructor = schema.reserve_value_local(function);
        constructor.copy_from(
            self.body_entry_locals()
                .expect("Promise combinator entry")
                .this_value(),
            function,
        );
        let executor =
            self.emit_current_function_promise_internal_function_materialization_context(function);
        let capability = self.emit_new_promise_capability(&executor, &constructor, function)?;
        let errors = self.emit_promise_combinator_algorithm_error_realm_context(function);
        let elements =
            self.emit_current_function_promise_combinator_element_materialization_context(function);
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        let iterator = schema
            .reserve_gc_local::<IteratorRecord, Nullable>(function)
            .initialize_null(schema, function);
        let resolve = schema.reserve_value_local(function);
        let reject = schema.reserve_value_local(function);
        for (field, out) in [
            (PromiseCapabilitySchema::RESOLVE, &resolve),
            (PromiseCapabilitySchema::REJECT, &reject),
        ] {
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<PromiseCapability>()
                    .field(field)
                    .read(&capability, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<StoredValue>()
                .read_into(&stored, out, schema, function);
            stored.clear(function);
        }
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_promise_property_get(&constructor, "resolve", &pending, function)?;
        self.emit_promise_combinator_reject_current_throw(
            &capability,
            &iterator,
            &pending,
            exit,
            function,
        )?;
        self.emit_is_callable_i32(pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_promise_combinator_type_error(
            &errors,
            mode.resolve_error(),
            &pending,
            function,
        )?;
        self.emit_promise_combinator_reject_current_throw(
            &capability,
            &iterator,
            &pending,
            exit,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let promise_resolve = schema.reserve_value_local(function);
        promise_resolve.copy_from(pending.value(), function);
        let input = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &input, function);
        let acquired =
            self.emit_promise_combinator_iterator(&input, &errors, mode, &pending, function)?;
        iterator.replace(acquired.load(schema, function), function);
        acquired.clear(function);
        self.emit_promise_combinator_reject_current_throw(
            &capability,
            &iterator,
            &pending,
            exit,
            function,
        )?;
        let record = schema.reserve_gc_local(function).initialize(
            iterator.load(schema, function).require_non_null(function),
            function,
        );
        let shared = match mode {
            PromiseCombinatorMode::Race => None,
            PromiseCombinatorMode::Values
            | PromiseCombinatorMode::SettledRecords
            | PromiseCombinatorMode::FirstFulfillment => {
                let list = schema.reserve_gc_local(function).initialize(
                    schema
                        .array_type::<ValueArray>()
                        .fixed(std::iter::empty(), function),
                    function,
                );
                let settle = match mode {
                    PromiseCombinatorMode::FirstFulfillment => &reject,
                    PromiseCombinatorMode::Values
                    | PromiseCombinatorMode::SettledRecords
                    | PromiseCombinatorMode::Race => &resolve,
                };
                let stored = schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(settle, function),
                    function,
                );
                let shared = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<PromiseCombinatorShared>().construct(
                        (
                            GcOperand::i64(1),
                            GcOperand::reference(&list, schema),
                            GcOperand::reference(&stored, schema),
                        ),
                        function,
                    ),
                    function,
                );
                stored.clear(function);
                list.clear(function);
                Some(shared)
            }
        };
        let index = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        let done = schema.reserve_i32_local(function);
        let next_value = schema.reserve_value_local(function);
        let next_promise = schema.reserve_value_local(function);
        let then = schema.reserve_value_local(function);
        let resolve_element = schema.reserve_value_local(function);
        let reject_element = schema.reserve_value_local(function);
        resolve_element.set_undefined(function);
        reject_element.set_undefined(function);
        let loop_end = self.open_frame(ControlFrameKind::Block, function);
        let again = self.open_frame(ControlFrameKind::Loop, function);
        self.emit_promise_combinator_step(
            &record,
            &errors,
            mode,
            &next_value,
            done,
            &pending,
            function,
        )?;
        self.emit_promise_combinator_reject_current_throw(
            &capability,
            &iterator,
            &pending,
            exit,
            function,
        )?;
        done.load(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(loop_end, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        if let Some(shared) = &shared {
            index.load(function);
            function.instruction(&Instruction::I64Const(9_007_199_254_740_991));
            function.instruction(&Instruction::I64GeU);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_throw_promise_combinator_range_error(
                &errors,
                RuntimeErrorMessage::PROMISE_ALL_ITERABLE_CONTAINS_TOO_MANY_VALUES,
                &pending,
                function,
            )?;
            self.emit_promise_combinator_reject_current_throw(
                &capability,
                &iterator,
                &pending,
                exit,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.emit_reserve_promise_combinator_list_entry(shared, function);
        }
        let args = self.emit_pre_evaluated_arg_vector(&[&next_value], function);
        self.emit_function_or_proxy_call_with_argv(
            &promise_resolve,
            &constructor,
            &args,
            &pending,
            function,
        )?;
        args.clear(function);
        self.emit_promise_combinator_reject_current_throw(
            &capability,
            &iterator,
            &pending,
            exit,
            function,
        )?;
        next_promise.copy_from(pending.value(), function);
        if let Some(shared) = &shared {
            let element = schema.reserve_gc_local(function).initialize(
                schema.struct_type::<PromiseElementContext>().construct(
                    (
                        GcOperand::i64_local(index),
                        GcOperand::reference(shared, schema),
                        GcOperand::boolean(false),
                    ),
                    function,
                ),
                function,
            );
            match mode {
                PromiseCombinatorMode::Values => {
                    let callable = self.emit_promise_combinator_element_function_value(
                        PromiseInternalFunction::AllElement(&element),
                        &elements,
                        function,
                    )?;
                    resolve_element.set_reference(&callable, schema, function);
                    callable.clear(function);
                }
                PromiseCombinatorMode::SettledRecords => {
                    let fulfilled = self.emit_promise_combinator_element_function_value(
                        PromiseInternalFunction::SettledFulfillElement(&element),
                        &elements,
                        function,
                    )?;
                    let rejected = self.emit_promise_combinator_element_function_value(
                        PromiseInternalFunction::SettledRejectElement(&element),
                        &elements,
                        function,
                    )?;
                    resolve_element.set_reference(&fulfilled, schema, function);
                    reject_element.set_reference(&rejected, schema, function);
                    rejected.clear(function);
                    fulfilled.clear(function);
                }
                PromiseCombinatorMode::FirstFulfillment => {
                    let callable = self.emit_promise_combinator_element_function_value(
                        PromiseInternalFunction::AnyRejectElement(&element),
                        &elements,
                        function,
                    )?;
                    reject_element.set_reference(&callable, schema, function);
                    callable.clear(function);
                }
                PromiseCombinatorMode::Race => unreachable!("Race owns no values List"),
            }
            let remaining = schema.reserve_i64_local(function);
            schema
                .struct_type::<PromiseCombinatorShared>()
                .field(PromiseCombinatorSharedSchema::REMAINING)
                .read(shared, schema, function)
                .store_i64(remaining, function);
            remaining.load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Add);
            remaining.store(function);
            schema
                .struct_type::<PromiseCombinatorShared>()
                .field(PromiseCombinatorSharedSchema::REMAINING)
                .write(shared, GcOperand::i64_local(remaining), schema, function);
            schema.release_i64_local(remaining, function);
            element.clear(function);
        }
        self.emit_promise_property_get(&next_promise, "then", &pending, function)?;
        self.emit_promise_combinator_reject_current_throw(
            &capability,
            &iterator,
            &pending,
            exit,
            function,
        )?;
        then.copy_from(pending.value(), function);
        self.emit_invoke_promise_combinator_reaction_pair(
            mode,
            &then,
            &next_promise,
            &resolve_element,
            &reject,
            &reject_element,
            &resolve,
            &pending,
            function,
        )?;
        self.emit_promise_combinator_reject_current_throw(
            &capability,
            &iterator,
            &pending,
            exit,
            function,
        )?;
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        self.emit_branch_to_target(again, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if let Some(shared) = &shared {
            let remaining = schema.reserve_i64_local(function);
            schema
                .struct_type::<PromiseCombinatorShared>()
                .field(PromiseCombinatorSharedSchema::REMAINING)
                .read(shared, schema, function)
                .store_i64(remaining, function);
            remaining.load(function);
            function.instruction(&Instruction::I64Const(1));
            function.instruction(&Instruction::I64Sub);
            remaining.store(function);
            schema
                .struct_type::<PromiseCombinatorShared>()
                .field(PromiseCombinatorSharedSchema::REMAINING)
                .write(shared, GcOperand::i64_local(remaining), schema, function);
            remaining.load(function);
            function.instruction(&Instruction::I64Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_finish_promise_combinator_list(shared, mode, &pending, function)?;
            self.emit_promise_combinator_reject_current_throw(
                &capability,
                &iterator,
                &pending,
                exit,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            schema.release_i64_local(remaining, function);
        }
        let promise = schema.reserve_value_local(function);
        self.emit_read_promise_capability_promise(&capability, &promise, function);
        self.completion().set_normal(&promise, function);
        promise.clear(function);
        reject_element.clear(function);
        resolve_element.clear(function);
        then.clear(function);
        next_promise.clear(function);
        next_value.clear(function);
        schema.release_i32_local(done, function);
        schema.release_i64_local(index, function);
        if let Some(shared) = shared {
            shared.clear(function);
        }
        record.clear(function);
        input.clear(function);
        promise_resolve.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        reject.clear(function);
        resolve.clear(function);
        iterator.clear(function);
        pending.clear(function);
        self.release_promise_combinator_element_function_materialization_context(
            elements, function,
        );
        self.release_promise_combinator_algorithm_error_realm_context(errors, function);
        capability.clear(function);
        self.release_promise_internal_function_materialization_context(executor, function);
        constructor.clear(function);
        Ok(())
    }

    pub(crate) fn emit_promise_resolving_function(
        &mut self,
        settlement: PromiseSettlement,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let context = self.emit_promise_resolving_context(function);
        self.completion().initialize(function);
        schema
            .struct_type::<PromiseResolvingContext>()
            .field(PromiseResolvingContextSchema::ALREADY_RESOLVED)
            .read(&context, schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .struct_type::<PromiseResolvingContext>()
            .field(PromiseResolvingContextSchema::ALREADY_RESOLVED)
            .write(&context, GcOperand::boolean(true), schema, function);
        let promise = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseResolvingContext>()
                .field(PromiseResolvingContextSchema::PROMISE)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        let value = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &value, function);
        match settlement {
            PromiseSettlement::Fulfill => {
                let resolving = self
                    .emit_current_function_promise_internal_function_materialization_context(
                        function,
                    );
                self.emit_resolve_promise_record_in_realm(
                    &promise,
                    &value,
                    resolving.realm(),
                    function,
                )?;
                self.release_promise_internal_function_materialization_context(resolving, function);
            }
            PromiseSettlement::Reject => self.emit_settle_promise_record(
                &promise,
                PromiseSettlement::Reject,
                &value,
                function,
            )?,
        }
        value.clear(function);
        promise.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        context.clear(function);
        self.completion().initialize(function);
        Ok(())
    }
}
