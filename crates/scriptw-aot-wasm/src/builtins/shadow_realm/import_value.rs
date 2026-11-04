//! Realm-origin module loading and the caller-Realm export boundary use the
//! existing module registry, evaluation Promise, and intrinsic reaction jobs.

use crate::emit::AccessorThrowRouting;
use crate::gc_types::*;
use crate::operations::PropertyKeyLocals;
use crate::*;

enum ImportReaction<'a> {
    Export(&'a GcLocal<ShadowRealmImportContext>),
    Rejection,
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_shadow_realm_import_value_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let argument = schema.reserve_value_local(function);
        let export_name = schema.reserve_value_local(function);
        let namespace = schema.reserve_value_local(function);
        let inner_value = schema.reserve_value_local(function);
        let fulfilled = schema.reserve_value_local(function);
        let rejected = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let output = schema.reserve_completion(function);
        namespace.set_undefined(function);
        output.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);

        // Receiver validation precedes every observable argument conversion.
        let realm = self.emit_shadow_realm_receiver_realm(&output, exit, function)?;
        self.emit_builtin_arg_to_value(0, &argument, function);
        self.emit_value_to_string_payload(&argument, &pending, function)?;
        self.emit_shadow_realm_import_abrupt_exit(&pending, &output, exit, function);
        let specifier = schema.reserve_gc_local(function).initialize(
            pending
                .value()
                .cast_reference::<StringValue>(schema, function),
            function,
        );
        self.emit_builtin_arg_to_value(1, &export_name, function);
        export_name.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::SHADOW_REALM_EXPORT_NAME_STRING,
            &output,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        // The native method's Realm owns its fresh returned Promise and both
        // continuations, including when the method is borrowed by another Realm.
        let caller = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        self.emit_realm_module_import(&realm, &specifier, &namespace, &pending, function)?;
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        // Loading/linking failure is asynchronous at this boundary. Retain its
        // original value internally; only the rejection job produces TypeError.
        let failed = self.emit_alloc_promise_in_realm(&caller, function)?;
        self.emit_settle_promise_record(
            &failed,
            PromiseSettlement::Reject,
            pending.value(),
            function,
        )?;
        inner_value.set_reference(&failed, schema, function);
        failed.clear(function);
        function.instruction(&Instruction::Else);
        inner_value.copy_from(pending.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let stored_namespace = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&namespace, function),
            function,
        );
        let name = schema.reserve_gc_local(function).initialize(
            export_name.cast_reference::<StringValue>(schema, function),
            function,
        );
        let context = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<ShadowRealmImportContext>().construct(
                (
                    GcOperand::reference(&stored_namespace, schema),
                    GcOperand::reference(&name, schema),
                ),
                function,
            ),
            function,
        );
        let on_fulfilled =
            self.emit_shadow_realm_import_reaction(ImportReaction::Export(&context), function)?;
        fulfilled.set_reference(&on_fulfilled, schema, function);
        let on_rejected =
            self.emit_shadow_realm_import_reaction(ImportReaction::Rejection, function)?;
        rejected.set_reference(&on_rejected, schema, function);
        let constructor = self.emit_current_function_realm_intrinsic_promise_constructor(function);
        let capability = self
            .emit_new_current_function_realm_intrinsic_promise_capability(constructor, function)?;
        let inner = schema.reserve_gc_local(function).initialize(
            inner_value.cast_reference::<PromiseObject>(schema, function),
            function,
        );
        self.emit_perform_promise_then(&inner, &fulfilled, &rejected, &capability, function)?;
        self.emit_read_promise_capability_promise(&capability, &argument, function);
        output.set_normal(&argument, function);
        inner.clear(function);
        capability.clear(function);
        on_rejected.clear(function);
        on_fulfilled.clear(function);
        context.clear(function);
        name.clear(function);
        stored_namespace.clear(function);
        caller.clear(function);
        specifier.clear(function);
        realm.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        output.clear(function);
        pending.clear(function);
        rejected.clear(function);
        fulfilled.clear(function);
        inner_value.clear(function);
        namespace.clear(function);
        export_name.clear(function);
        argument.clear(function);
        Ok(())
    }

    fn emit_shadow_realm_import_reaction(
        &mut self,
        reaction: ImportReaction<'_>,
        function: &mut Function,
    ) -> Result<GcLocal<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let (builtin, capture) = match reaction {
            ImportReaction::Export(context) => (
                StandardBuiltinId::ShadowRealmImportFulfilled,
                schema
                    .reserve_gc_local::<BuiltinClosureCapture, Nullable>(function)
                    .initialize(
                        schema
                            .struct_type::<BuiltinClosureCapture>()
                            .publish(
                                BuiltinClosurePayload::ShadowRealmImport(context),
                                schema,
                                function,
                            )
                            .nullable(),
                        function,
                    ),
            ),
            ImportReaction::Rejection => (
                StandardBuiltinId::ShadowRealmImportRejected,
                schema
                    .reserve_gc_local::<BuiltinClosureCapture, Nullable>(function)
                    .initialize_null(schema, function),
            ),
        };
        let meta = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| EmitError::unsupported("missing ShadowRealm import reaction entry"))?;
        let result = schema.reserve_gc_local(function).initialize(
            self.emit_current_builtin_realm_closure_value(&meta, &capture, function)?,
            function,
        );
        capture.clear(function);
        Ok(result)
    }

    pub(crate) fn compile_shadow_realm_import_fulfilled_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let namespace = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        let pending = schema.reserve_completion(function);
        let context = self.emit_shadow_realm_import_context(function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ShadowRealmImportContext>()
                .field(ShadowRealmImportContextSchema::NAMESPACE)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, &namespace, schema, function);
        let name = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ShadowRealmImportContext>()
                .field(ShadowRealmImportContextSchema::EXPORT_NAME)
                .read(&context, schema, function)
                .reference(),
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &name, function);
        output.initialize(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let descriptor = self.emit_object_get_own_property(&namespace, &key, &pending, function)?;
        self.emit_shadow_realm_import_abrupt_exit(&pending, &output, exit, function);
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        descriptor.clear(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::SHADOW_REALM_MISSING_EXPORT,
            &output,
            function,
        )?;
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_object_read_with_throw_routing(
            &namespace,
            &namespace,
            &key,
            &pending,
            AccessorThrowRouting::LeaveInCompletion,
            function,
        )?;
        self.emit_shadow_realm_import_abrupt_exit(&pending, &output, exit, function);
        let caller = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        self.emit_shadow_realm_wrapped_value(pending.value(), &caller, &output, function)?;
        caller.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        key.clear(function);
        name.clear(function);
        stored.clear(function);
        context.clear(function);
        pending.clear(function);
        output.clear(function);
        namespace.clear(function);
        Ok(())
    }

    fn emit_shadow_realm_import_context(
        &self,
        function: &mut Function,
    ) -> GcLocal<ShadowRealmImportContext> {
        let schema = self.runtime_schema();
        let entry = self
            .body_entry_locals()
            .expect("ShadowRealm export getter has native entry");
        let capture = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionContext>()
                .field(FunctionContextSchema::BUILTIN_CAPTURE)
                .read(
                    entry.function_context().expect("export getter context"),
                    schema,
                    function,
                )
                .reference()
                .require_non_null(function),
            function,
        );
        let kind = schema.reserve_i32_local(function);
        schema
            .struct_type::<BuiltinClosureCapture>()
            .field(BuiltinClosureCaptureSchema::KIND)
            .read(&capture, schema, function)
            .store(kind, function);
        kind.load(function);
        function.instruction(&Instruction::I32Const(GcI32Constant::encode(
            BuiltinClosureCaptureKind::ShadowRealmImport,
        )));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        let context = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<BuiltinClosureCapture>()
                .field(BuiltinClosureCaptureSchema::SHADOW_REALM_IMPORT)
                .read(&capture, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        schema.release_i32_local(kind, function);
        capture.clear(function);
        context
    }

    pub(crate) fn compile_shadow_realm_import_rejected_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let output = self.runtime_schema().reserve_completion(function);
        // Copying an error never observes message/name/toString or any other
        // property of the original rejection reason.
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::SHADOW_REALM_ABRUPT,
            &output,
            function,
        )?;
        self.completion().copy_from(&output, function);
        output.clear(function);
        Ok(())
    }

    fn emit_shadow_realm_import_abrupt_exit(
        &mut self,
        pending: &CompletionLocals,
        output: &CompletionLocals,
        exit: ControlTarget,
        function: &mut Function,
    ) {
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal as i32));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        output.copy_from(pending, function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }
}
