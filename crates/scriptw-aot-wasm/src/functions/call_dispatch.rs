//! Physical helper kernels consume whole values without calling themselves.

use super::*;
use crate::emit::AccessorThrowRouting;
use crate::gc_types::{
    ArrayObject, BoundFunction, CompletionLocals, FunctionObject, GcLocal, GcNullability, ProxyObject, StoredValue, ValueArray, ValueLocals,
};
use crate::objects::ProxyRevocationRoute;

impl FunctionBuilder<'_> {
    pub(super) fn emit_plain_function_construct_dispatch(
        &mut self,
        callee: &ValueLocals,
        new_target: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let current = schema.reserve_value_local(function);
        let actual_new_target = schema.reserve_value_local(function);
        let list = schema
            .reserve_gc_local(function)
            .initialize(arguments.load(schema, function), function);
        current.copy_from(callee, function);
        actual_new_target.copy_from(new_target, function);
        result.initialize(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        for constructor in [&current, &actual_new_target] {
            self.emit_is_constructor_i32(constructor, function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_proxy_execution_realm_type_error(
                RuntimeErrorMessage::TARGET_IS_NOT_A_CONSTRUCTOR,
                result,
                function,
            )?;
            self.emit_branch_to_target(done, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        let next = self.open_frame(ControlFrameKind::Loop, function);
        current.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<BoundFunction>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let bound = schema.reserve_gc_local(function).initialize(
            current.cast_reference::<BoundFunction>(schema, function),
            function,
        );
        let record = self.emit_load_bound_function_record(&bound, function);
        let joined = self.emit_concat_argument_vectors(record.arguments(), &list, function);
        // SameValue is identity for these already-validated constructors.
        // Only this bound exotic substitutes its target for newTarget.
        current.reference().load(function);
        actual_new_target.reference().load(function);
        function.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, function);
        actual_new_target.copy_from(record.target(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.copy_from(record.target(), function);
        list.replace(joined.load(schema, function), function);
        joined.clear(function);
        record.clear(schema, function);
        bound.clear(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        self.emit_function_or_proxy_construct_with_argv(
            &current,
            &actual_new_target,
            &list,
            result,
            function,
        )?;
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let callable = schema.reserve_gc_local(function).initialize(
            current.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let dispatch = RuntimeFunctionEntryDispatch::from_function(&callable, schema, function);
        dispatch.ordinary(self, function, |entry, builder, function| {
            builder.emit_function_constructor_entry_call(
                entry,
                &actual_new_target,
                &list,
                result,
                function,
            )
        })?;
        dispatch.release(function);
        callable.clear(function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        list.clear(function);
        actual_new_target.clear(function);
        current.clear(function);
        Ok(())
    }

    fn emit_function_constructor_entry_call(
        &mut self,
        entry: &crate::function_entry::RuntimeOrdinaryBodyEntry,
        new_target: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::gc_types::{
            ExecutableCode, ExecutableCodeKind, ExecutableCodeSchema, FunctionContext,
            FunctionContextSchema, GcI32Constant,
        };
        let schema = self.runtime_schema();
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(crate::gc_types::FunctionObjectSchema::CONTEXT)
                .read(entry.function_object(), schema, function)
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
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(crate::gc_types::FunctionObjectSchema::CODE)
                .read(entry.function_object(), schema, function)
                .reference(),
            function,
        );
        let native = schema.reserve_i32_local(function);
        let derived = schema.reserve_i32_local(function);
        schema
            .struct_type::<ExecutableCode>()
            .field(ExecutableCodeSchema::KIND)
            .read(&code, schema, function)
            .store(native, function);
        native.load(function);
        function.instruction(&Instruction::I32Const(
            ExecutableCodeKind::JavaScript.encode(),
        ));
        function.instruction(&Instruction::I32Ne);
        native.store(function);
        schema
            .struct_type::<FunctionObject>()
            .field(crate::gc_types::FunctionObjectSchema::DERIVED_CONSTRUCTOR)
            .read(entry.function_object(), schema, function)
            .store(derived, function);
        let receiver = schema.reserve_value_local(function);
        receiver.set_undefined(function);
        let pending = schema.reserve_completion(function);
        let caller_realm = self.load_current_realm(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        native.load(function);
        function.instruction(&Instruction::I32Eqz);
        derived.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        // The observable prototype read happens in the caller context before
        // PrepareForOrdinaryCall switches to the defining Realm.
        self.emit_get_prototype_from_constructor(
            new_target,
            OrdinaryDefaultPrototype::Object,
            &pending,
            function,
        )?;
        self.emit_bind_metadata_abrupt_exit(&pending, result, done, function);
        let instance = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_compact_object_with_prototype(pending.value(), function)?,
            function,
        );
        receiver.set_reference(&instance, schema, function);
        instance.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.replace_current_realm(&realm, function);
        result.store_call(
            entry.emit_call(
                OrdinaryBodyInputs::construct(
                    &receiver,
                    new_target,
                    EntryArguments::new(arguments),
                    &caller_realm,
                    self.current_program(),
                ),
                schema,
                function,
            ),
            function,
        );
        self.replace_current_realm(&caller_realm, function);
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        native.load(function);
        self.open_frame(ControlFrameKind::If, function);
        // A native constructor owns its own allocation and prototype timing.
        // Its declared Construct contract must return an Object normally.
        self.emit_object_value_i32(result.value(), function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        result.set_normal(result.value(), function);
        function.instruction(&Instruction::Else);
        self.emit_object_value_i32(result.value(), function);
        self.open_frame(ControlFrameKind::If, function);
        result.set_normal(result.value(), function);
        function.instruction(&Instruction::Else);
        derived.load(function);
        self.open_frame(ControlFrameKind::If, function);
        // Derived body epilogues resolve GetThisBinding and reject primitive
        // returns while their owned binding remains alive. A normal primitive
        // reaching this boundary violates that compiler contract.
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::Else);
        result.set_normal(&receiver, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        caller_realm.clear(function);
        pending.clear(function);
        receiver.clear(function);
        schema.release_i32_local(derived, function);
        schema.release_i32_local(native, function);
        code.clear(function);
        realm.clear(function);
        context.clear(function);
        Ok(())
    }

    pub(crate) fn emit_plain_function_call_dispatch(
        &mut self,
        callee: &ValueLocals,
        this_value: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let current = schema.reserve_value_local(function);
        let receiver = schema.reserve_value_local(function);
        let list = schema
            .reserve_gc_local(function)
            .initialize(arguments.load(schema, function), function);
        current.copy_from(callee, function);
        receiver.copy_from(this_value, function);
        result.initialize(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        current.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<BoundFunction>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let bound = schema.reserve_gc_local(function).initialize(
            current.cast_reference::<BoundFunction>(schema, function),
            function,
        );
        let record = self.emit_load_bound_function_record(&bound, function);
        let joined = self.emit_concat_argument_vectors(record.arguments(), &list, function);
        current.copy_from(record.target(), function);
        receiver.copy_from(record.this_value(), function);
        list.replace(joined.load(schema, function), function);
        joined.clear(function);
        record.clear(schema, function);
        bound.clear(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        current.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let callable = schema.reserve_gc_local(function).initialize(
            current.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let dispatch = RuntimeFunctionEntryDispatch::from_function(&callable, schema, function);
        let entered = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        entered.store(function);
        dispatch.ordinary(self, function, |entry, builder, function| {
            function.instruction(&Instruction::I32Const(1));
            entered.store(function);
            builder
                .emit_ordinary_function_entry_tail_call(entry, &receiver, &list, result, function)
        })?;
        dispatch.generator(self, function, |entry, builder, function| {
            function.instruction(&Instruction::I32Const(1));
            entered.store(function);
            builder.emit_generator_function_entry_call(entry, &receiver, &list, result, function)
        })?;
        dispatch.asynchronous(self, function, |entry, builder, function| {
            function.instruction(&Instruction::I32Const(1));
            entered.store(function);
            builder.emit_async_function_entry_call(entry, &receiver, &list, result, function)
        })?;
        dispatch.async_generator(self, function, |entry, builder, function| {
            function.instruction(&Instruction::I32Const(1));
            entered.store(function);
            builder
                .emit_async_generator_function_entry_call(entry, &receiver, &list, result, function)
        })?;
        entered.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(entered, function);
        dispatch.release(function);
        callable.clear(function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        self.emit_prepared_tail_call(&current, &receiver, &list, function)?;
        function.instruction(&Instruction::Else);
        self.emit_proxy_execution_realm_type_error(
            RuntimeErrorMessage::VALUE_IS_NOT_CALLABLE,
            result,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        list.clear(function);
        receiver.clear(function);
        current.clear(function);
        Ok(())
    }

    pub(crate) fn emit_proxy_call_dispatch(
        &mut self,
        callee: &ValueLocals,
        this_value: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_proxy_invocation_dispatch(
            callee,
            this_value,
            arguments,
            result,
            ProxyInvocation::Call,
            function,
        )
    }

    pub(crate) fn emit_proxy_construct_dispatch(
        &mut self,
        callee: &ValueLocals,
        new_target: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_proxy_invocation_dispatch(
            callee,
            new_target,
            arguments,
            result,
            ProxyInvocation::Construct,
            function,
        )
    }

    fn emit_proxy_invocation_dispatch(
        &mut self,
        callee: &ValueLocals,
        receiver_or_new_target: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &CompletionLocals,
        operation: ProxyInvocation,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let current = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let trap = schema.reserve_value_local(function);
        current.copy_from(callee, function);
        result.initialize(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        if let ProxyInvocation::Call = operation {
            // A plain FunctionObject is callable and is not a Proxy, so the
            // loop below would only forward it to FunctionCall, whose first
            // iteration enters its ordinary body. Enter that body here. A
            // resumable body has no ordinary entry and keeps the full path.
            current.reference().load(function);
            function.instruction(&Instruction::RefTestNonNull(
                schema
                    .reference_type::<FunctionObject>(GcNullability::NonNullable)
                    .heap_type,
            ));
            self.open_frame(ControlFrameKind::If, function);
            let callable = schema.reserve_gc_local(function).initialize(
                current.cast_reference::<FunctionObject>(schema, function),
                function,
            );
            let dispatch = RuntimeFunctionEntryDispatch::from_function(&callable, schema, function);
            dispatch.ordinary(self, function, |entry, builder, function| {
                // Every exit but the class-constructor TypeError leaves this
                // body; that completion is the whole Call result.
                builder.emit_ordinary_function_entry_tail_call(
                    entry,
                    receiver_or_new_target,
                    arguments,
                    result,
                    function,
                )?;
                builder.emit_branch_to_target(done, function);
                Ok(())
            })?;
            dispatch.release(function);
            callable.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        if let ProxyInvocation::Construct = operation {
            // Construct's public boundary validates the target before
            // newTarget. Retained capabilities are checked before revocation.
            for constructor in [&current, receiver_or_new_target] {
                self.emit_is_constructor_i32(constructor, function);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_proxy_execution_realm_type_error(
                    RuntimeErrorMessage::TARGET_IS_NOT_A_CONSTRUCTOR,
                    result,
                    function,
                )?;
                self.emit_branch_to_target(done, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
            }
        }
        let next = self.open_frame(ControlFrameKind::Loop, function);
        match operation {
            ProxyInvocation::Call => self.emit_is_callable_i32(&current, function)?,
            ProxyInvocation::Construct => self.emit_is_constructor_i32(&current, function),
        }
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_proxy_execution_realm_type_error(operation.invalid_target(), result, function)?;
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        current.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            current.cast_reference::<ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::ProxyExecutionRealmToActiveHandler,
            result,
            function,
            |builder, slots, function| {
                let key = builder.emit_function_string_key(operation.trap_name(), function)?;
                builder.emit_object_read_with_throw_routing(
                    slots.handler(),
                    slots.handler(),
                    &key,
                    &pending,
                    AccessorThrowRouting::LeaveInCompletion,
                    function,
                )?;
                key.clear(function);
                builder.emit_bind_metadata_abrupt_exit(&pending, result, done, function);
                trap.copy_from(pending.value(), function);
                builder.compile_nullish_tagged_i32(trap.tag(), function)?;
                builder.open_frame(ControlFrameKind::If, function);
                current.copy_from(slots.target(), function);
                builder.emit_branch_to_target(next, function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                builder.emit_is_callable_i32(&trap, function)?;
                function.instruction(&Instruction::I32Eqz);
                builder.open_frame(ControlFrameKind::If, function);
                builder.emit_proxy_execution_realm_type_error(
                    operation.invalid_trap(),
                    result,
                    function,
                )?;
                builder.emit_branch_to_target(done, function);
                builder.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                let argument_array = builder.emit_array_from_argument_list(arguments, function)?;
                let argument_array_value = schema.reserve_value_local(function);
                argument_array_value.set_reference(&argument_array, schema, function);
                let trap_arguments = match operation {
                    ProxyInvocation::Call => builder.emit_pre_evaluated_arg_vector(
                        &[
                            slots.target(),
                            receiver_or_new_target,
                            &argument_array_value,
                        ],
                        function,
                    ),
                    ProxyInvocation::Construct => builder.emit_pre_evaluated_arg_vector(
                        &[
                            slots.target(),
                            &argument_array_value,
                            receiver_or_new_target,
                        ],
                        function,
                    ),
                };
                match operation {
                    ProxyInvocation::Call => builder.emit_prepared_tail_call(
                        &trap,
                        slots.handler(),
                        &trap_arguments,
                        function,
                    )?,
                    ProxyInvocation::Construct => builder.emit_function_or_proxy_call_with_argv(
                        &trap,
                        slots.handler(),
                        &trap_arguments,
                        result,
                        function,
                    )?,
                }
                trap_arguments.clear(function);
                argument_array_value.clear(function);
                argument_array.clear(function);
                if let ProxyInvocation::Construct = operation {
                    result.kind().load(function);
                    function.instruction(&Instruction::I32Const(CompletionKind::Throw as i32));
                    function.instruction(&Instruction::I32Ne);
                    builder.open_frame(ControlFrameKind::If, function);
                    builder.emit_object_value_i32(result.value(), function);
                    function.instruction(&Instruction::I32Eqz);
                    builder.open_frame(ControlFrameKind::If, function);
                    builder.emit_proxy_execution_realm_type_error(
                        RuntimeErrorMessage::PROXY_CONSTRUCT_TRAP_RETURNED_NON_OBJECT,
                        result,
                        function,
                    )?;
                    builder.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                    builder.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                Ok(())
            },
        )?;
        proxy.clear(function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        match operation {
            ProxyInvocation::Call => {
                let base = self.runtime_helper_base.ok_or_else(|| {
                    EmitError::unsupported("compiler invariant: FunctionCall helper is absent")
                })?;
                schema.return_call_helper(
                    crate::runtime_helpers::FunctionCallArguments::new(
                        &current,
                        receiver_or_new_target,
                        arguments,
                        self.current_environment(),
                        self.current_program(),
                    ),
                    base,
                    function,
                );
            }
            ProxyInvocation::Construct => self.emit_plain_function_construct_dispatch(
                &current,
                receiver_or_new_target,
                arguments,
                result,
                function,
            )?,
        }
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        trap.clear(function);
        pending.clear(function);
        current.clear(function);
        Ok(())
    }

    pub(super) fn emit_object_value_i32(&self, value: &ValueLocals, function: &mut Function) {
        function.instruction(&Instruction::I32Const(0));
        for tag in [
            WasmRuntimeValueTag::Object,
            WasmRuntimeValueTag::Array,
            WasmRuntimeValueTag::Function,
            WasmRuntimeValueTag::Arguments,
        ] {
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(tag as i32));
            function.instruction(&Instruction::I32Eq);
            function.instruction(&Instruction::I32Or);
        }
    }

    /// CreateArrayFromList creates a JavaScript Array only after the complete
    /// List exists. The private storage remains distinct from that Array.
    pub(crate) fn emit_array_from_argument_list(
        &mut self,
        arguments: &GcLocal<ValueArray>,
        function: &mut Function,
    ) -> Result<GcLocal<ArrayObject>, EmitError> {
        let schema = self.runtime_schema();
        let realm = self.load_current_realm(function);
        let prototype = schema.reserve_gc_local(function).initialize(
            self.emit_load_realm_array_prototype(&realm, function),
            function,
        );
        let prototype_value = schema.reserve_value_local(function);
        prototype_value.set_reference(&prototype, schema, function);
        let length = schema.reserve_i64_local(function);
        let count = schema.reserve_i32_local(function);
        let index = schema.reserve_i32_local(function);
        schema
            .array_type::<ValueArray>()
            .length(arguments, schema, function);
        count.store(function);
        count.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        length.store(function);
        let array = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_array_payload_with_length_and_prototype(
                length,
                &prototype_value,
                function,
            )?,
            function,
        );
        let sparse_index = schema.reserve_i64_local(function);
        let value = schema.reserve_value_local(function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<ValueArray>()
                .read(arguments, index, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &value, schema, function);
        index.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        sparse_index.store(function);
        self.emit_array_indexed_publish_default_data(&array, sparse_index, &value, function)?;
        stored.clear(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        value.clear(function);
        schema.release_i64_local(sparse_index, function);
        schema.release_i32_local(index, function);
        schema.release_i32_local(count, function);
        schema.release_i64_local(length, function);
        prototype_value.clear(function);
        prototype.clear(function);
        realm.clear(function);
        Ok(array)
    }
}

#[derive(Clone, Copy)]
enum ProxyInvocation {
    Call,
    Construct,
}

impl ProxyInvocation {
    const fn trap_name(self) -> &'static str {
        match self {
            Self::Call => "apply",
            Self::Construct => "construct",
        }
    }
    const fn invalid_target(self) -> RuntimeErrorMessage {
        match self {
            Self::Call => RuntimeErrorMessage::VALUE_IS_NOT_CALLABLE,
            Self::Construct => RuntimeErrorMessage::TARGET_IS_NOT_A_CONSTRUCTOR,
        }
    }
    const fn invalid_trap(self) -> RuntimeErrorMessage {
        match self {
            Self::Call => RuntimeErrorMessage::PROXY_APPLY_TRAP_IS_NOT_CALLABLE,
            Self::Construct => RuntimeErrorMessage::PROXY_CONSTRUCT_TRAP_IS_NOT_CALLABLE,
        }
    }
}
