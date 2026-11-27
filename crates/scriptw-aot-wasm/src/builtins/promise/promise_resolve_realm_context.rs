use super::*;

#[must_use = "PromiseResolve entry root must be released"]
pub(super) struct PromiseResolveOperationRealmContext {
    resolve: ValueLocals,
}
#[must_use = "intrinsic PromiseResolve Realm context must be released"]
pub(super) struct IntrinsicPromiseResolveRealmContext {
    realm: GcLocal<RealmRecord>,
}

impl FunctionBuilder<'_> {
    fn emit_promise_resolve_realm(
        &mut self,
        authority: PromiseResolveRealmAuthority<'_>,
        function: &mut Function,
    ) -> GcLocal<RealmRecord> {
        let schema = self.runtime_schema();
        let realm = match authority {
            PromiseResolveRealmAuthority::CurrentFunction => {
                self.emit_current_function_realm(function)
            }
            PromiseResolveRealmAuthority::ExplicitRealm(realm) => realm.load(schema, function),
            PromiseResolveRealmAuthority::AsyncExecution(context) => {
                context.realm().load(schema, function)
            }
        };
        schema
            .reserve_gc_local(function)
            .initialize(realm, function)
    }
    fn emit_promise_resolve_operation_in_context(
        &mut self,
        context: &PromiseInternalFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<PromiseResolveOperationRealmContext, EmitError> {
        let schema = self.runtime_schema();
        let meta = self
            .functions
            .get(&StandardBuiltinId::PromiseResolve.function_id())
            .cloned()
            .ok_or_else(|| EmitError::unsupported("missing Promise.resolve builtin"))?;
        let callable = schema.reserve_gc_local(function).initialize(
            self.emit_function_value_payload_in_realm(&meta, context.materialization(), function)?,
            function,
        );
        let resolve = schema.reserve_value_local(function);
        resolve.set_reference(&callable, schema, function);
        callable.clear(function);
        Ok(PromiseResolveOperationRealmContext { resolve })
    }
    pub(super) fn emit_promise_resolve_operation_realm_context(
        &mut self,
        authority: PromiseResolveRealmAuthority<'_>,
        function: &mut Function,
    ) -> Result<PromiseResolveOperationRealmContext, EmitError> {
        let realm = self.emit_promise_resolve_realm(authority, function);
        let context = self
            .emit_promise_internal_function_materialization_context_from_realm(&realm, function);
        let result = self.emit_promise_resolve_operation_in_context(&context, function);
        self.release_promise_internal_function_materialization_context(context, function);
        realm.clear(function);
        result
    }
    pub(super) fn emit_intrinsic_promise_resolve_realm_context(
        &mut self,
        authority: PromiseResolveRealmAuthority<'_>,
        function: &mut Function,
    ) -> IntrinsicPromiseResolveRealmContext {
        IntrinsicPromiseResolveRealmContext {
            realm: self.emit_promise_resolve_realm(authority, function),
        }
    }
    pub(super) fn emit_call_promise_resolve_operation(
        &mut self,
        context: &PromiseResolveOperationRealmContext,
        constructor: &ValueLocals,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let arguments = self.emit_pre_evaluated_arg_vector(&[value], function);
        let emitted = self.emit_function_or_proxy_call_with_argv(
            &context.resolve,
            constructor,
            &arguments,
            result,
            function,
        );
        arguments.clear(function);
        emitted
    }
    pub(super) fn release_promise_resolve_operation_realm_context(
        &mut self,
        context: PromiseResolveOperationRealmContext,
        function: &mut Function,
    ) {
        context.resolve.clear(function);
    }
    pub(super) fn release_intrinsic_promise_resolve_realm_context(
        &mut self,
        context: IntrinsicPromiseResolveRealmContext,
        function: &mut Function,
    ) {
        context.realm.clear(function);
    }
    pub(super) fn emit_intrinsic_promise_resolve_to_locals(
        &mut self,
        context: &IntrinsicPromiseResolveRealmContext,
        value: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let done = self.open_frame(ControlFrameKind::Block, function);
        self.emit_is_heap_object_like_tag_i32(value.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        // The intrinsic constructor is fixed and primitive resolution does
        // not consult JavaScript properties.
        let fresh = self.emit_alloc_promise_in_realm(&context.realm, function)?;
        self.emit_settle_promise_record(&fresh, PromiseSettlement::Fulfill, value, function)?;
        result.value().set_reference(&fresh, schema, function);
        result.set_kind(CompletionKind::Normal, function);
        result.target().set_constant(0, function);
        fresh.clear(function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let slow = self.open_frame(ControlFrameKind::Block, function);
        let promise = schema.reserve_gc_local(function).initialize(
            value.sealed_reference_or_branch::<PromiseObject>(
                schema,
                self.local_branch_label(slow),
                function,
            ),
            function,
        );
        self.emit_intrinsic_promise_constructor_or_branch(
            &promise,
            &context.realm,
            slow,
            function,
        )?;
        result.set_normal(value, function);
        promise.clear(function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        // The existing call owns its defining Realm and the observable
        // constructor/then Gets for custom Promises and other objects.
        let materialization = self
            .emit_promise_internal_function_materialization_context_from_realm(
                &context.realm,
                function,
            );
        let operation =
            self.emit_promise_resolve_operation_in_context(&materialization, function)?;
        let constructor = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &context.realm,
            NonArrayRealmIntrinsicSlot::PromiseConstructor,
            &constructor,
            function,
        );
        self.emit_call_promise_resolve_operation(
            &operation,
            &constructor,
            value,
            result,
            function,
        )?;
        constructor.clear(function);
        self.release_promise_resolve_operation_realm_context(operation, function);
        self.release_promise_internal_function_materialization_context(materialization, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        Ok(())
    }
}
