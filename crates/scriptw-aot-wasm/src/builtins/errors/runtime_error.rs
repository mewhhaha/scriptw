use super::*;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::gc_types::{
    CompletionLocals, Environment, EnvironmentSchema, FunctionContext, FunctionContextSchema,
    GcLocal, GcOperand, GcStackReference, NativeErrorObject, Nullable, RealmRecord, RuntimeSchema,
    StringValue, ValueLocals,
};
use crate::module::ThrowDiagnosticRole;
use crate::operations::PropertyKeyLocals;
use crate::runtime_helpers::{
    HelperParameters, RuntimeErrorForThrowArguments, RuntimeErrorForThrowParameters,
    RuntimeErrorObjectArguments, RuntimeErrorObjectParameters,
};
use scriptw_ir::NativeErrorKind;

pub(crate) enum ActiveBuiltinRealmPrototype {
    TypeError,
    SuppressedError,
}

impl ActiveBuiltinRealmPrototype {
    fn slot(self) -> NonArrayRealmIntrinsicSlot {
        match self {
            Self::TypeError => NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            Self::SuppressedError => NonArrayRealmIntrinsicSlot::SuppressedErrorPrototype,
        }
    }
}

fn prototype_slot(kind: NativeErrorKind) -> NonArrayRealmIntrinsicSlot {
    match kind {
        NativeErrorKind::Error => NonArrayRealmIntrinsicSlot::ErrorPrototype,
        NativeErrorKind::EvalError => NonArrayRealmIntrinsicSlot::EvalErrorPrototype,
        NativeErrorKind::RangeError => NonArrayRealmIntrinsicSlot::RangeErrorPrototype,
        NativeErrorKind::ReferenceError => NonArrayRealmIntrinsicSlot::ReferenceErrorPrototype,
        NativeErrorKind::SyntaxError => NonArrayRealmIntrinsicSlot::SyntaxErrorPrototype,
        NativeErrorKind::TypeError => NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
        NativeErrorKind::URIError => NonArrayRealmIntrinsicSlot::URIErrorPrototype,
        NativeErrorKind::AggregateError => NonArrayRealmIntrinsicSlot::AggregateErrorPrototype,
        NativeErrorKind::SuppressedError => NonArrayRealmIntrinsicSlot::SuppressedErrorPrototype,
    }
}

impl FunctionBuilder<'_> {
    /// Helpers for complete source operations retain the caller's selected Realm.
    /// Callable bodies and transported FunctionContexts retain the defining Realm.
    /// Other helpers use their caller Environment, then the active execution Realm.
    pub(crate) fn emit_execution_realm(&mut self, function: &mut Function) -> GcLocal<RealmRecord> {
        let schema = self.runtime_schema();
        if let Some(realm) = self.helper_execution_realm() {
            return schema
                .reserve_gc_local(function)
                .initialize(realm.load(schema, function), function);
        }
        if let Some(context) = self.current_function_context() {
            return schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<FunctionContext>()
                    .field(FunctionContextSchema::REALM)
                    .read(context, schema, function)
                    .reference(),
                function,
            );
        }
        let cursor = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(self.current_environment().load(schema, function), function);
        let realm = schema
            .reserve_gc_local::<RealmRecord, Nullable>(function)
            .initialize_null(schema, function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        cursor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::BrIf(1));
        realm.replace(
            schema
                .struct_type::<Environment>()
                .field(EnvironmentSchema::DEFINING_REALM)
                .read(&cursor, schema, function)
                .reference(),
            function,
        );
        realm.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::BrIf(1));
        cursor.replace(
            schema
                .struct_type::<Environment>()
                .field(EnvironmentSchema::PARENT)
                .read(&cursor, schema, function)
                .reference(),
            function,
        );
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        realm.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::If(BlockType::Empty));
        let active = self.load_current_realm(function);
        realm.replace(active.load(schema, function).nullable(), function);
        active.clear(function);
        function.instruction(&Instruction::End);
        let resolved = schema.reserve_gc_local(function).initialize(
            realm.load(schema, function).require_non_null(function),
            function,
        );
        realm.clear(function);
        cursor.clear(function);
        resolved
    }

    pub(crate) fn emit_runtime_error_object(
        &mut self,
        kind: NativeErrorKind,
        message: RuntimeErrorMessage,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            prototype_slot(kind),
            &prototype,
            function,
        );
        self.emit_fresh_native_error_object(&prototype, message, value, function)?;
        prototype.clear(function);
        realm.clear(function);
        Ok(())
    }

    fn emit_fresh_native_error_object(
        &mut self,
        prototype: &ValueLocals,
        message: RuntimeErrorMessage,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let message = schema.reserve_gc_local(function).initialize(
            self.emit_runtime_error_message_reference(message, function)?,
            function,
        );
        let base = self.runtime_helper_base.ok_or_else(|| {
            EmitError::unsupported("compiler invariant: native error helper registration is absent")
        })?;
        let error = schema
            .call_helper(
                RuntimeErrorObjectArguments::new(prototype, &message),
                base,
                function,
            )
            .bind(
                schema,
                schema.reserve_gc_local::<NativeErrorObject, _>(function),
                function,
            );
        value.set_reference(&error, schema, function);
        error.clear(function);
        message.clear(function);
        Ok(())
    }

    pub(crate) fn compile_runtime_error_object_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RuntimeErrorObject);
        let schema = self.runtime_schema();
        let parameters = self.helper_parameters::<RuntimeErrorObjectParameters>(&mut function);
        let header = schema.reserve_gc_local(&mut function).initialize(
            self.emit_alloc_plain_object_with_prototype(
                Some(&parameters.prototype),
                &mut function,
            )?,
            &mut function,
        );
        let value = schema.reserve_value_local(&mut function);
        value.set_reference(&parameters.message, schema, &mut function);
        let key = self.emit_runtime_error_key("message", &mut function)?;
        self.emit_object_append_data_property_with_flags(
            &header,
            &key,
            &value,
            true,
            false,
            true,
            &mut function,
        )?;
        key.clear(&mut function);
        value.clear(&mut function);
        let error = schema.reserve_gc_local(&mut function).initialize(
            schema.struct_type::<NativeErrorObject>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&parameters.message, schema),
                ),
                &mut function,
            ),
            &mut function,
        );
        error.load(schema, &mut function);
        error.clear(&mut function);
        header.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// The caller has already selected its prototype and resolved its own
    /// message. Only R's closed error names are selected after allocation.
    pub(crate) fn compile_runtime_error_for_throw_helper(&mut self) -> Result<Function, EmitError> {
        self.strings.require_runtime_only()?;
        let mut function = self.begin_helper_body(RuntimeHelperId::RuntimeErrorForThrow);
        let schema = self.runtime_schema();
        let parameters = self.helper_parameters::<RuntimeErrorForThrowParameters>(&mut function);
        let error = schema
            .call_helper(
                RuntimeErrorObjectArguments::new(&parameters.prototype, &parameters.message),
                self.runtime_helper_base()?,
                &mut function,
            )
            .bind(
                schema,
                schema.reserve_gc_local::<NativeErrorObject, _>(&mut function),
                &mut function,
            );
        self.emit_clear_throw_diagnostic(ThrowDiagnosticRole::Name, &mut function);
        self.emit_clear_throw_diagnostic(ThrowDiagnosticRole::Message, &mut function);
        for kind in NativeErrorKind::ALL {
            parameters.kind.emit_is(kind, &mut function);
            function.instruction(&Instruction::If(BlockType::Empty));
            let name = schema.reserve_gc_local(&mut function).initialize(
                self.emit_interned_string_reference(kind.as_str(), &mut function)?,
                &mut function,
            );
            self.emit_store_throw_diagnostic(ThrowDiagnosticRole::Name, &name, &mut function);
            name.clear(&mut function);
            function.instruction(&Instruction::Else);
        }
        // No raw string index is accepted, and an invalid kind cannot silently
        // select another error name or complete the throwing wrapper.
        function.instruction(&Instruction::Unreachable);
        for _ in NativeErrorKind::ALL {
            function.instruction(&Instruction::End);
        }
        self.emit_store_throw_diagnostic(
            ThrowDiagnosticRole::Message,
            &parameters.message,
            &mut function,
        );
        error.load(schema, &mut function);
        error.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    fn emit_set_thrown_error_text(
        &mut self,
        kind: NativeErrorKind,
        message: Option<RuntimeErrorMessage>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_clear_throw_diagnostic(ThrowDiagnosticRole::Name, function);
        self.emit_clear_throw_diagnostic(ThrowDiagnosticRole::Message, function);
        let schema = self.runtime_schema();
        let name = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(kind.as_str(), function)?,
            function,
        );
        self.emit_store_throw_diagnostic(ThrowDiagnosticRole::Name, &name, function);
        name.clear(function);
        if let Some(message) = message {
            let text = schema.reserve_gc_local(function).initialize(
                self.emit_runtime_error_message_reference(message, function)?,
                function,
            );
            self.emit_store_throw_diagnostic(ThrowDiagnosticRole::Message, &text, function);
            text.clear(function);
        }
        Ok(())
    }

    pub(crate) fn emit_throw_runtime_error(
        &mut self,
        kind: NativeErrorKind,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            prototype_slot(kind),
            &prototype,
            function,
        );
        self.emit_throw_runtime_error_with_prototype(kind, message, &prototype, result, function)?;
        prototype.clear(function);
        realm.clear(function);
        Ok(())
    }

    pub(crate) fn emit_throw_current_function_realm_error(
        &mut self,
        kind: NativeErrorKind,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            prototype_slot(kind),
            &prototype,
            function,
        );
        self.emit_throw_runtime_error_with_prototype(kind, message, &prototype, result, function)?;
        prototype.clear(function);
        realm.clear(function);
        Ok(())
    }

    pub(crate) fn emit_throw_runtime_error_with_prototype(
        &mut self,
        kind: NativeErrorKind,
        message: RuntimeErrorMessage,
        prototype: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let message = schema.reserve_gc_local(function).initialize(
            self.emit_runtime_error_message_reference(message, function)?,
            function,
        );
        let error = schema
            .call_helper(
                RuntimeErrorForThrowArguments::new(kind, prototype, &message),
                self.runtime_helper_base()?,
                function,
            )
            .bind(
                schema,
                schema.reserve_gc_local::<NativeErrorObject, _>(function),
                function,
            );
        let value = schema.reserve_value_local(function);
        value.set_reference(&error, schema, function);
        result.set_throw(&value, function);
        value.clear(function);
        error.clear(function);
        message.clear(function);
        Ok(())
    }

    pub(crate) fn emit_throw_current_function_realm_type_error(
        &mut self,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_throw_current_function_realm_error(
            NativeErrorKind::TypeError,
            message,
            result,
            function,
        )
    }
    pub(crate) fn emit_throw_current_function_realm_range_error(
        &mut self,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_throw_current_function_realm_error(
            NativeErrorKind::RangeError,
            message,
            result,
            function,
        )
    }
    pub(crate) fn emit_throw_current_function_realm_uri_error(
        &mut self,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_throw_current_function_realm_error(
            NativeErrorKind::URIError,
            message,
            result,
            function,
        )
    }

    pub(crate) fn emit_load_active_builtin_realm_type_error_prototype(
        &mut self,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        self.emit_load_active_builtin_realm_prototype(
            ActiveBuiltinRealmPrototype::TypeError,
            value,
            function,
        );
    }
    pub(crate) fn emit_load_active_builtin_realm_prototype(
        &mut self,
        prototype: ActiveBuiltinRealmPrototype,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        self.emit_load_non_array_realm_intrinsic(&realm, prototype.slot(), value, function);
        realm.clear(function);
    }

    fn emit_throw_type_error_without_message_with_prototype(
        &mut self,
        prototype: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(prototype), function)?,
            function,
        );
        let stack = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference("TypeError", function)?,
            function,
        );
        let error = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<NativeErrorObject>().construct(
                (
                    GcOperand::reference(&header, schema),
                    GcOperand::reference(&stack, schema),
                ),
                function,
            ),
            function,
        );
        let value = schema.reserve_value_local(function);
        value.set_reference(&error, schema, function);
        result.set_throw(&value, function);
        self.emit_set_thrown_error_text(NativeErrorKind::TypeError, None, function)?;
        stack.clear(function);
        value.clear(function);
        error.clear(function);
        header.clear(function);
        Ok(())
    }

    pub(crate) fn emit_throw_runtime_type_error_without_message(
        &mut self,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            &prototype,
            function,
        );
        self.emit_throw_type_error_without_message_with_prototype(&prototype, result, function)?;
        prototype.clear(function);
        realm.clear(function);
        Ok(())
    }
    pub(crate) fn emit_throw_current_function_realm_type_error_without_message(
        &mut self,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            &prototype,
            function,
        );
        self.emit_throw_type_error_without_message_with_prototype(&prototype, result, function)?;
        prototype.clear(function);
        realm.clear(function);
        Ok(())
    }

    pub(crate) fn emit_throw_runtime_error_to_active_handler(
        &mut self,
        kind: NativeErrorKind,
        message: RuntimeErrorMessage,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_throw_runtime_error(kind, message, result, function)?;
        self.completion().copy_from(result, function);
        self.emit_propagate_current_throw_if_needed(function);
        Ok(())
    }

    fn emit_runtime_error_key(
        &mut self,
        name: &str,
        function: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let schema = self.runtime_schema();
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(name, function)?,
            function,
        );
        let key = PropertyKeyLocals::from_string(schema, &string, function);
        string.clear(function);
        Ok(key)
    }

    fn emit_store_diagnostic_string_value(
        &mut self,
        role: ThrowDiagnosticRole,
        value: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let string = schema
            .reserve_gc_local::<StringValue, _>(function)
            .initialize(
                value.cast_reference::<StringValue>(schema, function),
                function,
            );
        self.emit_store_throw_diagnostic(role, &string, function);
        string.clear(function);
        function.instruction(&Instruction::End);
    }

    /// Diagnostics use ordinary data fields only. Getter/Proxy code cannot
    /// run after the original completion or replace the value being described.
    pub(crate) fn emit_capture_throw_error_name(
        &mut self,
        thrown: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_clear_throw_diagnostic(ThrowDiagnosticRole::Name, function);
        self.emit_clear_throw_diagnostic(ThrowDiagnosticRole::Message, function);
        self.emit_is_heap_object_like_tag_i32(thrown.tag(), function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let name = schema.reserve_value_local(function);
        let message = schema.reserve_value_local(function);
        let name_key = self.emit_runtime_error_key("name", function)?;
        self.emit_data_property_read_no_call(thrown, &name_key, &name, function)?;
        self.emit_store_diagnostic_string_value(ThrowDiagnosticRole::Name, &name, function);
        let message_key = self.emit_runtime_error_key("message", function)?;
        self.emit_data_property_read_no_call(thrown, &message_key, &message, function)?;
        self.emit_store_diagnostic_string_value(ThrowDiagnosticRole::Message, &message, function);
        name.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::If(BlockType::Empty));
        let constructor = schema.reserve_value_local(function);
        let constructor_key = self.emit_runtime_error_key("constructor", function)?;
        self.emit_data_property_read_no_call(thrown, &constructor_key, &constructor, function)?;
        self.emit_is_heap_object_like_tag_i32(constructor.tag(), function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_data_property_read_no_call(&constructor, &name_key, &name, function)?;
        self.emit_store_diagnostic_string_value(ThrowDiagnosticRole::Name, &name, function);
        function.instruction(&Instruction::End);
        constructor_key.clear(function);
        constructor.clear(function);
        function.instruction(&Instruction::End);
        message_key.clear(function);
        name_key.clear(function);
        message.clear(function);
        name.clear(function);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn emit_capture_final_throw_constructor_name(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        self.emit_clear_throw_diagnostic(ThrowDiagnosticRole::ConstructorName, function);
        self.completion().kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.emit_is_heap_object_like_tag_i32(self.completion().value().tag(), function);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::If(BlockType::Empty));
        let thrown = schema.reserve_value_local(function);
        thrown.copy_from(self.completion().value(), function);
        let constructor = schema.reserve_value_local(function);
        let name = schema.reserve_value_local(function);
        let constructor_key = self.emit_runtime_error_key("constructor", function)?;
        let name_key = self.emit_runtime_error_key("name", function)?;
        self.emit_data_property_read_no_call(&thrown, &constructor_key, &constructor, function)?;
        self.emit_is_heap_object_like_tag_i32(constructor.tag(), function);
        function.instruction(&Instruction::If(BlockType::Empty));
        self.emit_data_property_read_no_call(&constructor, &name_key, &name, function)?;
        self.emit_store_diagnostic_string_value(
            ThrowDiagnosticRole::ConstructorName,
            &name,
            function,
        );
        function.instruction(&Instruction::End);
        name_key.clear(function);
        constructor_key.clear(function);
        name.clear(function);
        constructor.clear(function);
        thrown.clear(function);
        function.instruction(&Instruction::End);
        Ok(())
    }
}
