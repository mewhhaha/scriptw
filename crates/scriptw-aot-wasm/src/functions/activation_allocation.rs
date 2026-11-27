//! Call activation factories publish actual rooted frames and family records.

use super::*;
use crate::gc_types::{
    AsyncActivation, AsyncGeneratorActivation, AsyncGeneratorActivationSchema,
    AsyncGeneratorObject, AsyncGeneratorReturnStage, AwaitCompletionKind, CompletionLocals,
    ExecutableCode, ExecutableCodeKind, ExecutableCodeSchema, FunctionContext,
    FunctionContextSchema, FunctionObject, FunctionObjectSchema, FunctionProtocolCode,
    GcI32Constant, GcLocal, GcOperand, GeneratorActivation, GeneratorActivationSchema,
    GeneratorObject, InvocationFrame, RealmRecord, RealmRecordSchema, StoredValue, ValueArray,
    ValueLocals,
};

impl FunctionBuilder<'_> {
    /// The callback sees only a successfully bound receiver in the callee
    /// Realm. All JavaScript abrupt paths remain completions until restoration.
    fn emit_with_resumable_call_context(
        &mut self,
        callable: &GcLocal<FunctionObject>,
        supplied_this: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
        consume: impl FnOnce(
            &mut Self,
            &GcLocal<RealmRecord>,
            &ValueLocals,
            &mut Function,
        ) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::CONTEXT)
                .read(callable, schema, function)
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
        let caller_realm = self.load_current_realm(function);
        self.replace_current_realm(&realm, function);
        let receiver = schema.reserve_value_local(function);
        receiver.copy_from(supplied_this, function);
        let pending = schema.reserve_completion(function);
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::CODE)
                .read(callable, schema, function)
                .reference(),
            function,
        );
        let protocol = schema.reserve_i32_local(function);
        let strict = schema.reserve_i32_local(function);
        let kind = schema.reserve_i32_local(function);
        schema
            .struct_type::<ExecutableCode>()
            .field(ExecutableCodeSchema::PROTOCOL)
            .read(&code, schema, function)
            .store(protocol, function);
        schema
            .struct_type::<ExecutableCode>()
            .field(ExecutableCodeSchema::KIND)
            .read(&code, schema, function)
            .store(kind, function);
        schema
            .struct_type::<FunctionObject>()
            .field(FunctionObjectSchema::STRICT)
            .read(callable, schema, function)
            .store(strict, function);
        result.initialize(function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(
            ExecutableCodeKind::JavaScript.encode(),
        ));
        function.instruction(&Instruction::I32Eq);
        strict.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        // The same closed protocol authority used at code publication owns
        // lexical-this behavior. Async arrows cannot accidentally be boxed.
        crate::gc_types::emit_i32_in(
            protocol,
            FunctionProtocolCode::ALL
                .iter()
                .copied()
                .filter(|code| code.protocol().flavor() == FunctionFlavor::Ordinary)
                .map(|code| code.encoding()),
            function,
        );
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.compile_nullish_tagged_i32(supplied_this.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let global_this = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<RealmRecord>()
                .field(RealmRecordSchema::GLOBAL_THIS)
                .read(&realm, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&global_this, &receiver, schema, function);
        global_this.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_value_to_object_locals(supplied_this, &pending, function)?;
        self.emit_bind_metadata_abrupt_exit(&pending, result, done, function);
        receiver.copy_from(pending.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        consume(self, &realm, &receiver, function)?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.replace_current_realm(&caller_realm, function);
        schema.release_i32_local(kind, function);
        schema.release_i32_local(strict, function);
        schema.release_i32_local(protocol, function);
        code.clear(function);
        pending.clear(function);
        receiver.clear(function);
        caller_realm.clear(function);
        realm.clear(function);
        context.clear(function);
        Ok(())
    }

    pub(crate) fn emit_alloc_invocation_frame(
        &self,
        callable: &GcLocal<FunctionObject>,
        this_value: &ValueLocals,
        new_target: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        function: &mut Function,
    ) -> GcLocal<InvocationFrame> {
        let schema = self.runtime_schema();
        let stored_this = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(this_value, function),
            function,
        );
        let stored_new_target = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(new_target, function),
            function,
        );
        // Stored Undefined is immutable. The shared empty array has no
        // writable element, and a growing frame replaces its LOCALS reference.
        let empty = schema
            .reserve_gc_local(function)
            .initialize(schema.stored_undefined(function), function);
        let locals = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_value_array(function), function);
        let private_argument_lists = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<crate::gc_types::PrivateArgumentListTable>()
                .fixed([], function),
            function,
        );
        let frame = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<InvocationFrame>().construct(
                (
                    GcOperand::reference(callable, schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::reference(&stored_this, schema),
                    GcOperand::reference(&stored_new_target, schema),
                    GcOperand::reference(arguments, schema),
                    GcOperand::reference(&locals, schema),
                    GcOperand::reference(&private_argument_lists, schema),
                    GcOperand::null(schema),
                    GcOperand::i64(0),
                    GcOperand::reference(&empty, schema),
                    GcOperand::reference(&empty, schema),
                    GcOperand::boolean(false),
                    GcOperand::null(schema),
                ),
                function,
            ),
            function,
        );
        private_argument_lists.clear(function);
        locals.clear(function);
        empty.clear(function);
        stored_new_target.clear(function);
        stored_this.clear(function);
        frame
    }

    pub(super) fn emit_generator_function_entry_call(
        &mut self,
        entry: &crate::function_entry::RuntimeGeneratorBodyEntry,
        supplied_this: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_with_resumable_call_context(
            entry.function_object(),
            supplied_this,
            result,
            function,
            |builder, _realm, this_value, function| {
                let undefined = schema.reserve_value_local(function);
                undefined.set_undefined(function);
                let frame = builder.emit_alloc_invocation_frame(
                    entry.function_object(),
                    this_value,
                    &undefined,
                    arguments,
                    function,
                );
                let initial = schema
                    .reserve_gc_local(function)
                    .initialize(schema.stored_undefined(function), function);
                let activation = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<GeneratorActivation>().construct(
                        (
                            GcOperand::reference(&frame, schema),
                            GcOperand::reference(&initial, schema),
                            GcOperand::constant(GeneratorResumeKind::Normal),
                            GcOperand::null(schema),
                            GcOperand::i32(crate::gc_types::INITIALIZING_RESUME_POINT),
                            GcOperand::constant(GeneratorState::SuspendedStart),
                        ),
                        function,
                    ),
                    function,
                );
                let pending = schema.reserve_completion(function);
                let done = builder.open_frame(ControlFrameKind::Block, function);
                pending.store_call(
                    entry.emit_call(
                        GeneratorBodyInputs::new(
                            this_value,
                            &activation,
                            EntryArguments::new(arguments),
                            builder.current_program(),
                        ),
                        schema,
                        function,
                    ),
                    function,
                );
                builder.emit_bind_metadata_abrupt_exit(&pending, result, done, function);
                schema
                    .struct_type::<GeneratorActivation>()
                    .field(GeneratorActivationSchema::RESUME_POINT)
                    .write(&activation, GcOperand::i32(0), schema, function);
                let function_value = schema.reserve_value_local(function);
                function_value.set_reference(entry.function_object(), schema, function);
                builder.emit_generator_instance_prototype(
                    GeneratorInstanceFamily::Generator,
                    &function_value,
                    &pending,
                    function,
                )?;
                function_value.clear(function);
                builder.emit_bind_metadata_abrupt_exit(&pending, result, done, function);
                let header = schema.reserve_gc_local(function).initialize(
                    builder
                        .emit_alloc_plain_object_with_prototype(Some(pending.value()), function)?,
                    function,
                );
                let object = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<GeneratorObject>().construct(
                        (
                            GcOperand::reference(&header, schema),
                            GcOperand::reference(&activation, schema),
                        ),
                        function,
                    ),
                    function,
                );
                undefined.set_reference(&object, schema, function);
                result.set_normal(&undefined, function);
                object.clear(function);
                header.clear(function);
                builder.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                pending.clear(function);
                activation.clear(function);
                initial.clear(function);
                frame.clear(function);
                undefined.clear(function);
                Ok(())
            },
        )
    }

    pub(super) fn emit_async_generator_function_entry_call(
        &mut self,
        entry: &crate::function_entry::RuntimeAsyncGeneratorBodyEntry,
        supplied_this: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_with_resumable_call_context(
            entry.function_object(),
            supplied_this,
            result,
            function,
            |builder, _realm, this_value, function| {
                let undefined = schema.reserve_value_local(function);
                undefined.set_undefined(function);
                let frame = builder.emit_alloc_invocation_frame(
                    entry.function_object(),
                    this_value,
                    &undefined,
                    arguments,
                    function,
                );
                let initial = schema
                    .reserve_gc_local(function)
                    .initialize(schema.stored_undefined(function), function);
                let activation = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<AsyncGeneratorActivation>().construct(
                        (
                            GcOperand::reference(&frame, schema),
                            GcOperand::reference(&initial, schema),
                            GcOperand::constant(AsyncGeneratorResumeKind::Normal),
                            GcOperand::null(schema),
                            GcOperand::null(schema),
                            GcOperand::null(schema),
                            GcOperand::null(schema),
                            GcOperand::i32(crate::gc_types::INITIALIZING_RESUME_POINT),
                            GcOperand::constant(AsyncGeneratorExecutionState::SuspendedStart),
                            GcOperand::constant(AsyncGeneratorBodyStatus::Idle),
                            GcOperand::reference(&initial, schema),
                            GcOperand::constant(AsyncGeneratorReturnStage::Unawaited),
                        ),
                        function,
                    ),
                    function,
                );
                let pending = schema.reserve_completion(function);
                let done = builder.open_frame(ControlFrameKind::Block, function);
                pending.store_call(
                    entry.emit_call(
                        AsyncGeneratorBodyInputs::new(
                            this_value,
                            &activation,
                            EntryArguments::new(arguments),
                            builder.current_program(),
                        ),
                        schema,
                        function,
                    ),
                    function,
                );
                builder.emit_bind_metadata_abrupt_exit(&pending, result, done, function);
                schema
                    .struct_type::<AsyncGeneratorActivation>()
                    .field(AsyncGeneratorActivationSchema::RESUME_POINT)
                    .write(&activation, GcOperand::i32(0), schema, function);
                let function_value = schema.reserve_value_local(function);
                function_value.set_reference(entry.function_object(), schema, function);
                builder.emit_generator_instance_prototype(
                    GeneratorInstanceFamily::AsyncGenerator,
                    &function_value,
                    &pending,
                    function,
                )?;
                function_value.clear(function);
                builder.emit_bind_metadata_abrupt_exit(&pending, result, done, function);
                let header = schema.reserve_gc_local(function).initialize(
                    builder
                        .emit_alloc_plain_object_with_prototype(Some(pending.value()), function)?,
                    function,
                );
                let object = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<AsyncGeneratorObject>().construct(
                        (
                            GcOperand::reference(&header, schema),
                            GcOperand::reference(&activation, schema),
                        ),
                        function,
                    ),
                    function,
                );
                undefined.set_reference(&object, schema, function);
                result.set_normal(&undefined, function);
                object.clear(function);
                header.clear(function);
                builder.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                pending.clear(function);
                activation.clear(function);
                initial.clear(function);
                frame.clear(function);
                undefined.clear(function);
                Ok(())
            },
        )
    }

    pub(super) fn emit_async_function_entry_call(
        &mut self,
        entry: &crate::function_entry::RuntimeAsyncBodyEntry,
        supplied_this: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_with_resumable_call_context(
            entry.function_object(),
            supplied_this,
            result,
            function,
            |builder, realm, this_value, function| {
                let undefined = schema.reserve_value_local(function);
                undefined.set_undefined(function);
                let promise = builder.emit_alloc_promise_in_realm(realm, function)?;
                let frame = builder.emit_alloc_invocation_frame(
                    entry.function_object(),
                    this_value,
                    &undefined,
                    arguments,
                    function,
                );
                let initial = schema
                    .reserve_gc_local(function)
                    .initialize(schema.stored_undefined(function), function);
                let activation = schema.reserve_gc_local(function).initialize(
                    schema.struct_type::<AsyncActivation>().construct(
                        (
                            GcOperand::reference(&frame, schema),
                            GcOperand::reference(&initial, schema),
                            GcOperand::constant(AwaitCompletionKind::Normal),
                            GcOperand::reference(&promise, schema),
                            GcOperand::i32(0),
                            GcOperand::boolean(false),
                            GcOperand::reference(realm, schema),
                            GcOperand::constant(AsyncModuleEntryMode::Execute),
                        ),
                        function,
                    ),
                    function,
                );
                let pending = schema.reserve_completion(function);
                pending.store_call(
                    entry.emit_call(
                        AsyncBodyInputs::new(
                            this_value,
                            &activation,
                            EntryArguments::new(arguments),
                            builder.current_program(),
                        ),
                        schema,
                        function,
                    ),
                    function,
                );
                builder.emit_complete_async_entry_invocation(&activation, &pending, function)?;
                undefined.set_reference(&promise, schema, function);
                result.set_normal(&undefined, function);
                pending.clear(function);
                activation.clear(function);
                initial.clear(function);
                frame.clear(function);
                promise.clear(function);
                undefined.clear(function);
                Ok(())
            },
        )
    }
}
