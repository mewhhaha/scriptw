use super::*;

#[derive(Clone, Copy)]
enum PromiseKeyedCombinatorMode {
    Values,
    SettledRecords,
}
enum PromiseKeyedReflection {
    OwnKeys,
    OwnDescriptor,
}
impl FunctionBuilder<'_> {
    pub(crate) fn emit_promise_all_keyed(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_promise_keyed(PromiseKeyedCombinatorMode::Values, function)
    }
    pub(crate) fn emit_promise_all_settled_keyed(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_promise_keyed(PromiseKeyedCombinatorMode::SettledRecords, function)
    }

    fn emit_promise_keyed_reflection(
        &mut self,
        kind: PromiseKeyedReflection,
        context: &PromiseInternalFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<GcLocal<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let builtin = match kind {
            PromiseKeyedReflection::OwnKeys => StandardBuiltinId::ReflectOwnKeys,
            PromiseKeyedReflection::OwnDescriptor => {
                StandardBuiltinId::ReflectGetOwnPropertyDescriptor
            }
        };
        let meta = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| EmitError::unsupported("missing keyed Promise reflection builtin"))?;
        Ok(schema.reserve_gc_local(function).initialize(
            self.emit_function_value_payload_in_realm(&meta, context.materialization(), function)?,
            function,
        ))
    }

    fn emit_promise_keyed_reject_current_throw(
        &mut self,
        capability: &GcLocal<PromiseCapability>,
        pending: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
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

    fn emit_promise_keyed(
        &mut self,
        mode: PromiseKeyedCombinatorMode,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let constructor = schema.reserve_value_local(function);
        constructor.copy_from(
            self.body_entry_locals()
                .expect("keyed Promise entry")
                .this_value(),
            function,
        );
        let materialization =
            self.emit_current_function_promise_internal_function_materialization_context(function);
        let capability =
            self.emit_new_promise_capability(&materialization, &constructor, function)?;
        let pending = schema.reserve_completion(function);
        pending.initialize(function);
        let resolve = schema.reserve_value_local(function);
        let reject = schema.reserve_value_local(function);
        for (field, out) in [
            (PromiseCapabilitySchema::RESOLVE, &resolve),
            (PromiseCapabilitySchema::REJECT, &reject),
        ] {
            let record = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<PromiseCapability>()
                    .field(field)
                    .read(&capability, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<StoredValue>()
                .read_into(&record, out, schema, function);
            record.clear(function);
        }
        let undefined = schema.reserve_value_local(function);
        undefined.set_undefined(function);
        let input = schema.reserve_value_local(function);
        self.emit_builtin_arg_to_value(0, &input, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        self.emit_promise_property_get(&constructor, "resolve", &pending, function)?;
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        self.emit_is_callable_i32(pending.value(), function)?;
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::PROMISE_KEYED_CONSTRUCTOR_RESOLVE_PROPERTY_IS_NOT_CALLABLE,
            &pending,
            function,
        )?;
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let promise_resolve = schema.reserve_value_local(function);
        promise_resolve.copy_from(pending.value(), function);
        self.emit_is_heap_object_like_tag_i32(input.tag(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::PROMISE_KEYED_INPUT_MUST_BE_AN_OBJECT,
            &pending,
            function,
        )?;
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let own_keys = self.emit_promise_keyed_reflection(
            PromiseKeyedReflection::OwnKeys,
            &materialization,
            function,
        )?;
        let own_descriptor = self.emit_promise_keyed_reflection(
            PromiseKeyedReflection::OwnDescriptor,
            &materialization,
            function,
        )?;
        let native = schema.reserve_value_local(function);
        native.set_reference(&own_keys, schema, function);
        let key_args = self.emit_pre_evaluated_arg_vector(&[&input], function);
        self.emit_function_or_proxy_call_with_argv(
            &native, &undefined, &key_args, &pending, function,
        )?;
        key_args.clear(function);
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        let keys = schema.reserve_gc_local(function).initialize(
            pending
                .value()
                .cast_reference::<ArrayObject>(schema, function),
            function,
        );
        let length = schema.reserve_i32_local(function);
        let index64 = schema.reserve_i64_local(function);
        schema
            .struct_type::<ArrayObject>()
            .field(ArrayObjectSchema::LENGTH)
            .read(&keys, schema, function)
            .store_i64(index64, function);
        index64.load(function);
        function.instruction(&Instruction::I32WrapI64);
        length.store(function);
        let index = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        // Both keyed combinators collect their own keys on a null-prototype
        // result. Settlement records still use the callback Realm's Object.prototype.
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(None, function)?,
            function,
        );
        let result = schema.reserve_value_local(function);
        result.set_reference(&object, schema, function);
        let stored_result = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&result, function),
            function,
        );
        let stored_resolve = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&resolve, function),
            function,
        );
        let shared = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseKeyedCombinatorShared>()
                .construct(
                    (
                        GcOperand::i64(1),
                        GcOperand::reference(&stored_result, schema),
                        GcOperand::reference(&stored_resolve, schema),
                    ),
                    function,
                ),
            function,
        );
        let key_value = schema.reserve_value_local(function);
        let property = schema.reserve_value_local(function);
        let next_promise = schema.reserve_value_local(function);
        let then = schema.reserve_value_local(function);
        let fulfilled = schema.reserve_value_local(function);
        let rejected = schema.reserve_value_local(function);
        let remaining = schema.reserve_i64_local(function);
        let loop_end = self.open_frame(ControlFrameKind::Block, function);
        let again = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        length.load(function);
        function.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(loop_end, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        index64.store(function);
        let nullable_slot = self.emit_array_indexed_descriptor(&keys, index64, function);
        let slot = schema.reserve_gc_local(function).initialize(
            nullable_slot
                .load(schema, function)
                .require_non_null(function),
            function,
        );
        nullable_slot.clear(function);
        let key_record = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PropertyDescriptor>()
                .field(PropertyDescriptorSchema::VALUE)
                .read(&slot, schema, function)
                .reference(),
            function,
        );
        self.emit_property_value_into(&key_record, &key_value, function);
        native.set_reference(&own_descriptor, schema, function);
        let descriptor_args = self.emit_pre_evaluated_arg_vector(&[&input, &key_value], function);
        self.emit_function_or_proxy_call_with_argv(
            &native,
            &undefined,
            &descriptor_args,
            &pending,
            function,
        )?;
        descriptor_args.clear(function);
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        pending.value().tag().load(function);
        function.instruction(&Instruction::I32Const(ValueKind::Undefined.tag() as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        let descriptor = schema.reserve_value_local(function);
        descriptor.copy_from(pending.value(), function);
        self.emit_promise_property_get(&descriptor, "enumerable", &pending, function)?;
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        self.compile_truthy_tagged_i32(pending.value(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let key = self.emit_value_to_property_key_locals(&key_value, function)?;
        self.emit_object_read_with_throw_routing(
            &input,
            &input,
            &key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        property.copy_from(pending.value(), function);
        self.emit_create_data_property_or_throw(&result, &key, &undefined, &pending, function)?;
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        let value_args = self.emit_pre_evaluated_arg_vector(&[&property], function);
        self.emit_function_or_proxy_call_with_argv(
            &promise_resolve,
            &constructor,
            &value_args,
            &pending,
            function,
        )?;
        value_args.clear(function);
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        next_promise.copy_from(pending.value(), function);
        let captured_key = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&key_value, function),
            function,
        );
        let element = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PromiseKeyedElementContext>()
                .construct(
                    (
                        GcOperand::reference(&captured_key, schema),
                        GcOperand::reference(&shared, schema),
                        GcOperand::boolean(false),
                    ),
                    function,
                ),
            function,
        );
        let callable = match mode {
            PromiseKeyedCombinatorMode::Values => self.emit_promise_internal_function_value(
                PromiseInternalFunction::AllKeyedElement(&element),
                &materialization,
                function,
            )?,
            PromiseKeyedCombinatorMode::SettledRecords => self
                .emit_promise_internal_function_value(
                    PromiseInternalFunction::SettledFulfillKeyedElement(&element),
                    &materialization,
                    function,
                )?,
        };
        fulfilled.set_reference(&callable, schema, function);
        callable.clear(function);
        match mode {
            PromiseKeyedCombinatorMode::Values => rejected.copy_from(&reject, function),
            PromiseKeyedCombinatorMode::SettledRecords => {
                let callable = self.emit_promise_internal_function_value(
                    PromiseInternalFunction::SettledRejectKeyedElement(&element),
                    &materialization,
                    function,
                )?;
                rejected.set_reference(&callable, schema, function);
                callable.clear(function);
            }
        }
        schema
            .struct_type::<PromiseKeyedCombinatorShared>()
            .field(PromiseKeyedCombinatorSharedSchema::REMAINING)
            .read(&shared, schema, function)
            .store_i64(remaining, function);
        remaining.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        remaining.store(function);
        schema
            .struct_type::<PromiseKeyedCombinatorShared>()
            .field(PromiseKeyedCombinatorSharedSchema::REMAINING)
            .write(&shared, GcOperand::i64_local(remaining), schema, function);
        self.emit_promise_property_get(&next_promise, "then", &pending, function)?;
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        then.copy_from(pending.value(), function);
        let args = self.emit_pre_evaluated_arg_vector(&[&fulfilled, &rejected], function);
        self.emit_function_or_proxy_call_with_argv(
            &then,
            &next_promise,
            &args,
            &pending,
            function,
        )?;
        args.clear(function);
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        element.clear(function);
        captured_key.clear(function);
        key.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        key_record.clear(function);
        slot.clear(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(again, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema
            .struct_type::<PromiseKeyedCombinatorShared>()
            .field(PromiseKeyedCombinatorSharedSchema::REMAINING)
            .read(&shared, schema, function)
            .store_i64(remaining, function);
        remaining.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Sub);
        remaining.store(function);
        schema
            .struct_type::<PromiseKeyedCombinatorShared>()
            .field(PromiseKeyedCombinatorSharedSchema::REMAINING)
            .write(&shared, GcOperand::i64_local(remaining), schema, function);
        remaining.load(function);
        function.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let args = self.emit_pre_evaluated_arg_vector(&[&result], function);
        self.emit_function_or_proxy_call_with_argv(
            &resolve, &undefined, &args, &pending, function,
        )?;
        args.clear(function);
        self.emit_promise_keyed_reject_current_throw(&capability, &pending, exit, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let promise = schema.reserve_value_local(function);
        self.emit_read_promise_capability_promise(&capability, &promise, function);
        self.completion().set_normal(&promise, function);
        promise.clear(function);
        schema.release_i64_local(remaining, function);
        rejected.clear(function);
        fulfilled.clear(function);
        then.clear(function);
        next_promise.clear(function);
        property.clear(function);
        key_value.clear(function);
        shared.clear(function);
        stored_resolve.clear(function);
        stored_result.clear(function);
        result.clear(function);
        object.clear(function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(length, function);
        schema.release_i64_local(index64, function);
        keys.clear(function);
        native.clear(function);
        own_descriptor.clear(function);
        own_keys.clear(function);
        promise_resolve.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        input.clear(function);
        undefined.clear(function);
        reject.clear(function);
        resolve.clear(function);
        pending.clear(function);
        capability.clear(function);
        self.release_promise_internal_function_materialization_context(materialization, function);
        constructor.clear(function);
        Ok(())
    }
}
