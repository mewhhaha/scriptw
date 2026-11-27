use super::*;

impl FunctionBuilder<'_> {
    /// This entry is for consumers that call the resolving functions only
    /// through the capability. User constructors still receive the executor.
    pub(super) fn emit_new_internal_promise_capability(
        &mut self,
        executor: &PromiseInternalFunctionMaterializationContext,
        constructor: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcLocal<PromiseCapability>, EmitError> {
        let schema = self.runtime_schema();
        let selected = schema
            .reserve_gc_local::<PromiseCapability, Nullable>(function)
            .initialize_null(schema, function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        constructor.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let callable = schema.reserve_gc_local(function).initialize(
            constructor.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CONTEXT)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionContextSchema::REALM)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        let canonical = self.emit_load_non_array_realm_intrinsic_stored(
            &realm,
            NonArrayRealmIntrinsicSlot::PromiseConstructor,
            function,
        );
        let value = schema.reserve_value_local(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&canonical, &value, schema, function);
        constructor.reference().load(function);
        value.reference().load(function);
        function.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, function);
        let capability = self.emit_alloc_internal_intrinsic_promise_capability(&realm, function)?;
        selected.replace(capability.load(schema, function).nullable(), function);
        capability.clear(function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        value.clear(function);
        canonical.clear(function);
        realm.clear(function);
        context.clear(function);
        callable.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let ordinary = self.emit_new_promise_capability(executor, constructor, function)?;
        selected.replace(ordinary.load(schema, function).nullable(), function);
        ordinary.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        let capability = schema.reserve_gc_local(function).initialize(
            selected.load(schema, function).require_non_null(function),
            function,
        );
        selected.clear(function);
        Ok(capability)
    }

    pub(in crate::builtins) fn emit_new_internal_current_realm_promise_capability(
        &mut self,
        function: &mut Function,
    ) -> Result<GcLocal<PromiseCapability>, EmitError> {
        let schema = self.runtime_schema();
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        let capability = self.emit_alloc_internal_intrinsic_promise_capability(&realm, function)?;
        realm.clear(function);
        Ok(capability)
    }

    fn emit_alloc_internal_intrinsic_promise_capability(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<GcLocal<PromiseCapability>, EmitError> {
        let schema = self.runtime_schema();
        // The authentic constructor's prototype property is fixed. Constructing
        // it with the private capability executor performs no other user code.
        let promise = self.emit_alloc_promise_in_realm(realm, function)?;
        let value = schema.reserve_value_local(function);
        value.set_reference(&promise, schema, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        let unused = schema
            .reserve_gc_local(function)
            .initialize(schema.stored_undefined(function), function);
        let capability = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PromiseCapability>().construct(
                (
                    GcOperand::reference(&stored, schema),
                    GcOperand::reference(&unused, schema),
                    GcOperand::reference(&unused, schema),
                    GcOperand::nullable_reference(&promise, schema),
                    GcOperand::boolean(false),
                ),
                function,
            ),
            function,
        );
        unused.clear(function);
        stored.clear(function);
        value.clear(function);
        promise.clear(function);
        Ok(capability)
    }

    pub(in crate::builtins) fn emit_call_internal_promise_capability(
        &mut self,
        capability: &GcLocal<PromiseCapability>,
        settlement: PromiseSettlement,
        argument: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let intrinsic = schema.reserve_gc_local(function).initialize(
            schema
                .field(PromiseCapabilitySchema::INTRINSIC_PROMISE)
                .read(capability, schema, function)
                .reference(),
            function,
        );
        intrinsic.load(schema, function).is_null(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_call_promise_capability(capability, settlement, argument, result, function)?;
        function.instruction(&Instruction::Else);
        let _ = schema
            .field(PromiseCapabilitySchema::ALREADY_RESOLVED)
            .read(capability, schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        // Publish the latch before any observable then Get. The Promise can
        // remain pending until a thenable job calls its own resolving pair.
        schema
            .field(PromiseCapabilitySchema::ALREADY_RESOLVED)
            .write(capability, GcOperand::boolean(true), schema, function);
        let promise = schema.reserve_gc_local(function).initialize(
            intrinsic.load(schema, function).require_non_null(function),
            function,
        );
        let realm = schema.reserve_gc_local(function).initialize(
            schema
                .field(PromiseObjectSchema::REALM)
                .read(&promise, schema, function)
                .reference(),
            function,
        );
        let previous = self.load_current_realm(function);
        self.replace_current_realm(&realm, function);
        match settlement {
            PromiseSettlement::Fulfill => {
                self.emit_resolve_promise_record_in_realm(&promise, argument, &realm, function)?
            }
            PromiseSettlement::Reject => self.emit_settle_promise_record(
                &promise,
                PromiseSettlement::Reject,
                argument,
                function,
            )?,
        }
        self.replace_current_realm(&previous, function);
        previous.clear(function);
        realm.clear(function);
        promise.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.initialize(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        intrinsic.clear(function);
        Ok(())
    }
}
