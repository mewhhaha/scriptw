//! Completed defining captures are installed before a callable is published.

use super::*;
use crate::gc_types::{
    BuiltinClosureCapture, Environment, ExecutableCode, FunctionContext, FunctionContextSchema,
    FunctionObject, FunctionProgramOwner, GcLocal, GcOperand, GcStackReference, NonNullable,
    Nullable, PrivateElementTable, PrivateEnvironment, RealmRecord, StoredValue, StringValue,
    TemplateSource, ValueArray, ValueLocals,
};
use crate::operations::PropertyKeyLocals;

mod metadata_publication;

/// This is consumed by the actual allocation producer. Captures cannot be
/// installed after another operation has obtained the FunctionObject.
pub(super) struct FunctionAllocationInputs<'a> {
    pub(super) program: FunctionProgramOwner<'a>,
    pub(super) realm: &'a GcLocal<RealmRecord>,
    pub(super) lexical_environment: &'a GcLocal<Environment, Nullable>,
    pub(super) private_environment: &'a GcLocal<PrivateEnvironment, Nullable>,
    pub(super) home_object: &'a ValueLocals,
    pub(super) field_keys: &'a GcLocal<ValueArray, Nullable>,
    pub(super) instance_private_methods: &'a GcLocal<PrivateElementTable, Nullable>,
    pub(super) template_source: &'a GcLocal<TemplateSource, Nullable>,
    pub(super) builtin_capture: &'a GcLocal<BuiltinClosureCapture, Nullable>,
    pub(super) internal_prototype: &'a ValueLocals,
    pub(super) typed_array_element_kind: Option<TypedArrayElementKind>,
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_ordinary_function_entry_tail_call(
        &mut self,
        entry: &crate::function_entry::RuntimeOrdinaryBodyEntry,
        this_value: &ValueLocals,
        arguments: &GcLocal<ValueArray>,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use crate::gc_types::{ExecutableCodeSchema, FunctionObjectSchema, FunctionProtocolCode};
        let schema = self.runtime_schema();
        let callable = entry.function_object();
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
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::CODE)
                .read(callable, schema, function)
                .reference(),
            function,
        );
        let protocol = schema.reserve_i32_local(function);
        let native = schema.reserve_i32_local(function);
        let strict = schema.reserve_i32_local(function);
        schema
            .struct_type::<ExecutableCode>()
            .field(ExecutableCodeSchema::PROTOCOL)
            .read(&code, schema, function)
            .store(protocol, function);
        schema
            .struct_type::<ExecutableCode>()
            .field(ExecutableCodeSchema::KIND)
            .read(&code, schema, function)
            .store(native, function);
        native.load(function);
        function.instruction(&Instruction::I32Const(
            crate::gc_types::GcI32Constant::encode(crate::gc_types::ExecutableCodeKind::JavaScript),
        ));
        function.instruction(&Instruction::I32Ne);
        native.store(function);
        schema
            .struct_type::<FunctionObject>()
            .field(FunctionObjectSchema::STRICT)
            .read(callable, schema, function)
            .store(strict, function);
        let call_this = schema.reserve_value_local(function);
        let binding = schema.reserve_completion(function);
        call_this.copy_from(this_value, function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        protocol.load(function);
        function.instruction(&Instruction::I32Const(
            FunctionProtocolCode::ClassConstructor.encoding(),
        ));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let error_prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            &error_prototype,
            function,
        );
        self.emit_throw_runtime_error_with_prototype(
            NativeErrorKind::TypeError,
            RuntimeErrorMessage::CLASS_CONSTRUCTOR_CANNOT_BE_INVOKED_WITHOUT_NEW,
            &error_prototype,
            result,
            function,
        )?;
        error_prototype.clear(function);
        self.emit_branch_to_target(exit, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        native.load(function);
        function.instruction(&Instruction::I32Eqz);
        strict.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        protocol.load(function);
        function.instruction(&Instruction::I32Const(
            FunctionProtocolCode::Arrow.encoding(),
        ));
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        self.compile_nullish_tagged_i32(this_value.tag(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        let global_this = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<RealmRecord>()
                .field(crate::gc_types::RealmRecordSchema::GLOBAL_THIS)
                .read(&realm, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&global_this, &call_this, schema, function);
        global_this.clear(function);
        function.instruction(&Instruction::Else);
        self.emit_value_to_object_locals(this_value, &binding, function)?;
        self.emit_bind_metadata_abrupt_exit(&binding, result, exit, function);
        call_this.copy_from(binding.value(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // The normal Call boundary owns Return normalization and Realm
        // restoration. Forward the body's complete result without a frame.
        entry.emit_return_call(
            crate::function_entry::OrdinaryBodyInputs::call(
                &call_this,
                crate::function_entry::EntryArguments::new(arguments),
                &caller_realm,
                self.current_program(),
            ),
            schema,
            function,
        );
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        binding.clear(function);
        call_this.clear(function);
        schema.release_i32_local(strict, function);
        schema.release_i32_local(native, function);
        schema.release_i32_local(protocol, function);
        code.clear(function);
        caller_realm.clear(function);
        realm.clear(function);
        context.clear(function);
        Ok(())
    }

    pub(super) fn emit_bootstrap_function_prototype_record(
        &mut self,
        meta: &WasmFunctionMeta,
        realm: &GcLocal<RealmRecord>,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let capture = schema
            .reserve_gc_local::<BuiltinClosureCapture, Nullable>(function)
            .initialize_null(schema, function);
        let result = self.emit_native_function_record(
            meta,
            realm,
            object_prototype,
            &capture,
            FunctionPrototypeMaterialization::BootstrapSupplied,
            function,
        );
        capture.clear(function);
        result
    }

    pub(super) fn emit_realm_builtin_function_record(
        &mut self,
        meta: &WasmFunctionMeta,
        context: &RealmFunctionMaterializationContext,
        prototype_materialization: FunctionPrototypeMaterialization,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let capture = schema
            .reserve_gc_local::<BuiltinClosureCapture, Nullable>(function)
            .initialize_null(schema, function);
        let result = self.emit_realm_builtin_function_record_with_capture(
            meta,
            context,
            prototype_materialization,
            &capture,
            function,
        );
        capture.clear(function);
        result
    }

    pub(super) fn emit_realm_builtin_function_record_with_capture(
        &mut self,
        meta: &WasmFunctionMeta,
        context: &RealmFunctionMaterializationContext,
        prototype_materialization: FunctionPrototypeMaterialization,
        builtin_capture: &GcLocal<BuiltinClosureCapture, Nullable>,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        if meta.standard_builtin.is_none() && meta.host_builtin.is_none() {
            return Err(EmitError::unsupported(
                "native Realm allocation requires planned native metadata",
            ));
        }
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        match meta.protocol().execution_kind() {
            FunctionExecutionKind::Ordinary => {
                prototype.set_reference(&context.function_prototype, schema, function)
            }
            FunctionExecutionKind::Generator => self.emit_load_non_array_realm_intrinsic(
                &context.realm,
                NonArrayRealmIntrinsicSlot::GeneratorFunctionPrototype,
                &prototype,
                function,
            ),
            FunctionExecutionKind::Async => self.emit_load_non_array_realm_intrinsic(
                &context.realm,
                NonArrayRealmIntrinsicSlot::AsyncFunctionPrototype,
                &prototype,
                function,
            ),
            FunctionExecutionKind::AsyncGenerator => self.emit_load_non_array_realm_intrinsic(
                &context.realm,
                NonArrayRealmIntrinsicSlot::AsyncGeneratorFunctionPrototype,
                &prototype,
                function,
            ),
        }
        let result = self.emit_native_function_record(
            meta,
            &context.realm,
            &prototype,
            builtin_capture,
            prototype_materialization,
            function,
        )?;
        prototype.clear(function);
        Ok(result)
    }

    pub(super) fn emit_native_function_record(
        &mut self,
        meta: &WasmFunctionMeta,
        realm: &GcLocal<RealmRecord>,
        internal_prototype: &ValueLocals,
        builtin_capture: &GcLocal<BuiltinClosureCapture, Nullable>,
        prototype_materialization: FunctionPrototypeMaterialization,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let lexical_environment = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize_null(schema, function);
        let private_environment = schema
            .reserve_gc_local::<PrivateEnvironment, Nullable>(function)
            .initialize_null(schema, function);
        let home_object = schema.reserve_value_local(function);
        home_object.set_undefined(function);
        let field_keys = schema
            .reserve_gc_local::<ValueArray, Nullable>(function)
            .initialize_null(schema, function);
        let instance_private_methods = schema
            .reserve_gc_local::<PrivateElementTable, Nullable>(function)
            .initialize_null(schema, function);
        let template_source = schema
            .reserve_gc_local::<TemplateSource, Nullable>(function)
            .initialize_null(schema, function);
        let result = self.emit_complete_function_record(
            meta,
            FunctionAllocationInputs {
                program: FunctionProgramOwner::InheritCaller,
                realm,
                lexical_environment: &lexical_environment,
                private_environment: &private_environment,
                home_object: &home_object,
                field_keys: &field_keys,
                instance_private_methods: &instance_private_methods,
                template_source: &template_source,
                builtin_capture,
                internal_prototype,
                typed_array_element_kind: meta
                    .standard_builtin
                    .and_then(TypedArrayElementKind::from_constructor),
            },
            prototype_materialization,
            function,
        )?;
        template_source.clear(function);
        instance_private_methods.clear(function);
        field_keys.clear(function);
        home_object.clear(function);
        private_environment.clear(function);
        lexical_environment.clear(function);
        Ok(result)
    }

    /// Capture an ordinary source expression before allocation. Class and
    /// object methods supply their complete captures to the same producer.
    pub(super) fn emit_source_function_record(
        &mut self,
        meta: &WasmFunctionMeta,
        realm: &GcLocal<RealmRecord>,
        template_source: &GcLocal<TemplateSource, Nullable>,
        prototype_materialization: FunctionPrototypeMaterialization,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let is_source = meta.standard_builtin.is_none() && meta.host_builtin.is_none();
        let defining_program = is_source.then(|| self.capture_current_program(function));
        let lexical_environment = if is_source {
            schema
                .reserve_gc_local::<Environment, Nullable>(function)
                .initialize(self.current_environment().load(schema, function), function)
        } else {
            schema
                .reserve_gc_local::<Environment, Nullable>(function)
                .initialize_null(schema, function)
        };
        let private_environment = if is_source && meta.captures_private_environment {
            schema
                .reserve_gc_local::<PrivateEnvironment, Nullable>(function)
                .initialize(
                    self.current_private_environment().load(schema, function),
                    function,
                )
        } else {
            schema
                .reserve_gc_local::<PrivateEnvironment, Nullable>(function)
                .initialize_null(schema, function)
        };
        let home_object = schema.reserve_value_local(function);
        home_object.set_undefined(function);
        let field_keys = schema
            .reserve_gc_local::<ValueArray, Nullable>(function)
            .initialize_null(schema, function);
        let instance_private_methods = schema
            .reserve_gc_local::<PrivateElementTable, Nullable>(function)
            .initialize_null(schema, function);
        let builtin_capture = schema
            .reserve_gc_local::<BuiltinClosureCapture, Nullable>(function)
            .initialize_null(schema, function);
        let internal_prototype = schema.reserve_value_local(function);
        let slot = match meta.protocol().execution_kind() {
            FunctionExecutionKind::Ordinary => NonArrayRealmIntrinsicSlot::FunctionPrototype,
            FunctionExecutionKind::Generator => {
                NonArrayRealmIntrinsicSlot::GeneratorFunctionPrototype
            }
            FunctionExecutionKind::Async => NonArrayRealmIntrinsicSlot::AsyncFunctionPrototype,
            FunctionExecutionKind::AsyncGenerator => {
                NonArrayRealmIntrinsicSlot::AsyncGeneratorFunctionPrototype
            }
        };
        self.emit_load_non_array_realm_intrinsic(realm, slot, &internal_prototype, function);
        let result = self.emit_source_function_record_from_captures(
            meta,
            FunctionAllocationInputs {
                program: match &defining_program {
                    Some(program) => FunctionProgramOwner::Defining(program),
                    None => FunctionProgramOwner::InheritCaller,
                },
                realm,
                lexical_environment: &lexical_environment,
                private_environment: &private_environment,
                home_object: &home_object,
                field_keys: &field_keys,
                instance_private_methods: &instance_private_methods,
                template_source,
                builtin_capture: &builtin_capture,
                internal_prototype: &internal_prototype,
                typed_array_element_kind: meta
                    .standard_builtin
                    .and_then(TypedArrayElementKind::from_constructor),
            },
            prototype_materialization,
            function,
        )?;
        internal_prototype.clear(function);
        builtin_capture.clear(function);
        instance_private_methods.clear(function);
        field_keys.clear(function);
        home_object.clear(function);
        private_environment.clear(function);
        lexical_environment.clear(function);
        if let Some(program) = defining_program {
            program.clear(function);
        }
        Ok(result)
    }

    /// The actual allocation consumes every capture. Named expressions add
    /// their self cell before publication; no immutable capture is repaired.
    pub(super) fn emit_source_function_record_from_captures(
        &mut self,
        meta: &WasmFunctionMeta,
        inputs: FunctionAllocationInputs<'_>,
        prototype_materialization: FunctionPrototypeMaterialization,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let lexical_environment = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(inputs.lexical_environment.load(schema, function), function);
        let named_cell = if meta.is_named_expression {
            let bindings = [scriptw_ir::OwnedEnvBindingIr {
                mutability: scriptw_ir::EnvironmentBindingMutabilityIr::Immutable { strict: false },
                name: meta.name.clone(),
                slot: 0,
            }];
            let role = scriptw_ir::EvalEnvironmentRoleIr::Declarative {
                kind: scriptw_ir::EvalDeclarativeEnvironmentKindIr::Lexical,
                bindings: vec![scriptw_ir::EvalVisibleBindingIr {
                    source_name: meta.name.clone(),
                    slot: 0,
                    mode: BindingMode::Const,
                    declaration: scriptw_ir::EvalBindingDeclarationIr::NamedFunctionExpression,
                }],
            };
            let cells = self.emit_allocate_environment_cells(&bindings, function);
            let named_environment = self.emit_initialize_named_environment_header(
                &lexical_environment,
                &cells,
                Some(&role),
                function,
            )?;
            let cell = self.emit_environment_cell_local(&named_environment, 0, function);
            lexical_environment.replace(
                named_environment.load(schema, function).nullable(),
                function,
            );
            named_environment.clear(function);
            cells.clear(function);
            Some(cell)
        } else {
            None
        };
        let callable = schema
            .reserve_gc_local::<FunctionObject, NonNullable>(function)
            .initialize(
                self.emit_complete_function_record(
                    meta,
                    FunctionAllocationInputs {
                        program: inputs.program,
                        realm: inputs.realm,
                        lexical_environment: &lexical_environment,
                        private_environment: inputs.private_environment,
                        home_object: inputs.home_object,
                        field_keys: inputs.field_keys,
                        instance_private_methods: inputs.instance_private_methods,
                        template_source: inputs.template_source,
                        builtin_capture: inputs.builtin_capture,
                        internal_prototype: inputs.internal_prototype,
                        typed_array_element_kind: inputs.typed_array_element_kind,
                    },
                    prototype_materialization,
                    function,
                )?,
                function,
            );
        if let Some(cell) = named_cell {
            let value = schema.reserve_value_local(function);
            value.set_reference(&callable, schema, function);
            self.emit_initialize_environment_cell(&cell, &value, function);
            value.clear(function);
            cell.clear(function);
        }
        lexical_environment.clear(function);
        let result = callable.load(schema, function);
        callable.clear(function);
        Ok(result)
    }

    pub(super) fn emit_complete_function_record(
        &mut self,
        meta: &WasmFunctionMeta,
        inputs: FunctionAllocationInputs<'_>,
        prototype_materialization: FunctionPrototypeMaterialization,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let executable_kind = meta.entry.executable_kind()?;
        let program_operand = inputs.program.capture_operand(executable_kind, schema)?;
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(inputs.internal_prototype), function)?,
            function,
        );
        let code = schema
            .reserve_gc_local::<ExecutableCode, NonNullable>(function)
            .initialize(
                meta.entry.emit_executable_code(
                    meta.class_element_execution_kind,
                    schema,
                    function,
                )?,
                function,
            );
        let stored_home = schema.reserve_gc_local(function).initialize(
            match executable_kind {
                crate::gc_types::ExecutableCodeKind::JavaScript => schema
                    .struct_type::<StoredValue>()
                    .from_value(inputs.home_object, function),
                // Native entries have no ECMAScript [[HomeObject]]. Their
                // factories supply undefined, including captured builtins.
                // Source methods/classes retain their actual defining home.
                crate::gc_types::ExecutableCodeKind::StandardBuiltin
                | crate::gc_types::ExecutableCodeKind::HostBuiltin => {
                    schema.stored_undefined(function)
                }
            },
            function,
        );
        let initializer = self.emit_instance_elements_initializer_capture(meta, function)?;
        let context = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<FunctionContext>().construct(
                (
                    GcOperand::reference(inputs.realm, schema),
                    GcOperand::reference(inputs.lexical_environment, schema),
                    GcOperand::reference(inputs.private_environment, schema),
                    GcOperand::null(schema),
                    GcOperand::reference(&stored_home, schema),
                    GcOperand::reference(inputs.field_keys, schema),
                    GcOperand::reference(inputs.template_source, schema),
                    GcOperand::reference(inputs.builtin_capture, schema),
                    GcOperand::reference(inputs.instance_private_methods, schema),
                    program_operand,
                    GcOperand::nullable_function_reference(&initializer),
                ),
                function,
            ),
            function,
        );
        initializer.clear(function);
        let source_text = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(&meta.to_string_value, function)?,
                function,
            );
        // Publication replaces this field's edge when it creates a prototype;
        // the immutable undefined snapshot never acquires object identity.
        let stored_public_prototype = schema
            .reserve_gc_local(function)
            .initialize(schema.stored_undefined(function), function);
        let callable = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<FunctionObject>().construct(
                (
                    GcOperand::reference(&object, schema),
                    GcOperand::reference(&code, schema),
                    GcOperand::reference(&context, schema),
                    GcOperand::reference(&source_text, schema),
                    GcOperand::reference(&stored_public_prototype, schema),
                    GcOperand::boolean(meta.strict),
                    GcOperand::constant(meta.class_heritage_kind),
                    GcOperand::boolean(meta.is_derived_constructor),
                    GcOperand::boolean(meta.is_synthetic_default_derived_constructor),
                    GcOperand::boolean(meta.uses_super),
                    GcOperand::boolean(meta.this_before_super),
                    GcOperand::boolean(meta.host_builtin == Some(HostBuiltinId::HTMLDDA)),
                    GcOperand::constant(inputs.typed_array_element_kind),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                ),
                function,
            ),
            function,
        );
        // This is the sole allocation cycle: all other context fields are
        // immutable, and no source operation can observe the unclosed record.
        schema
            .struct_type::<FunctionContext>()
            .field(FunctionContextSchema::ACTIVE_FUNCTION)
            .write(
                &context,
                GcOperand::nullable_reference(&callable, schema),
                schema,
                function,
            );

        self.emit_publish_function_metadata(
            meta,
            inputs.realm,
            &callable,
            prototype_materialization,
            function,
        )?;
        stored_public_prototype.clear(function);
        source_text.clear(function);
        stored_home.clear(function);
        context.clear(function);
        code.clear(function);
        object.clear(function);
        let published = callable.load(schema, function);
        callable.clear(function);
        Ok(published)
    }

    pub(crate) fn emit_function_string_key(
        &mut self,
        name: &str,
        function: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let schema = self.runtime_schema();
        let string = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
        let key = PropertyKeyLocals::from_string(schema, &string, function);
        string.clear(function);
        Ok(key)
    }
}
