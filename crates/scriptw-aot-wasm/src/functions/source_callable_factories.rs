//! Defining captures and Realm identities use the complete allocation producer.

use super::*;
use crate::gc_types::{
    BuiltinClosureCapture, FunctionObject, FunctionObjectSchema, GcLocal, GcOperand,
    GcStackReference, Nullable, OrdinaryObject, RealmRecord, StoredValue, TemplateSource,
    ValueLocals,
};

impl FunctionBuilder<'_> {
    /// Bootstrap supplies the constructor's completed public prototype.
    /// Internal Function.prototype and element-kind captures are already set.
    pub(crate) fn emit_realm_constructor_function_record(
        &mut self,
        meta: &WasmFunctionMeta,
        context: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        self.emit_validated_realm_constructor_record(meta, context, function)
    }

    pub(crate) fn emit_function_identity_payload(
        &mut self,
        id: &FunctionId,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let canonical = StandardBuiltinId::from_function_id(id)
            .and_then(crate::module::standard_builtin_function_realm_slot)
            .or_else(|| {
                HostBuiltinId::from_function_id(id)
                    .and_then(crate::module::canonical_host_function_realm_slot)
            })
            .or_else(|| match DynamicSourceIntrinsic::from_function_id(id) {
                Some(DynamicSourceIntrinsic::Function(kind)) => Some(match kind {
                    DynamicFunctionKind::Ordinary => {
                        NonArrayRealmIntrinsicSlot::FunctionConstructor
                    }
                    DynamicFunctionKind::Generator => {
                        NonArrayRealmIntrinsicSlot::GeneratorFunctionConstructor
                    }
                    DynamicFunctionKind::Async => {
                        NonArrayRealmIntrinsicSlot::AsyncFunctionConstructor
                    }
                    DynamicFunctionKind::AsyncGenerator => {
                        NonArrayRealmIntrinsicSlot::AsyncGeneratorFunctionConstructor
                    }
                }),
                Some(DynamicSourceIntrinsic::RealmEvalScript) | None => None,
            });
        if let Some(slot) = canonical {
            let schema = self.runtime_schema();
            let realm = self.load_current_realm(function);
            let value = schema.reserve_value_local(function);
            self.emit_load_non_array_realm_intrinsic(&realm, slot, &value, function);
            let callable = value.cast_reference::<FunctionObject>(schema, function);
            value.clear(function);
            realm.clear(function);
            return Ok(callable);
        }
        let meta = self.functions.get(id).cloned().ok_or_else(|| {
            EmitError::unsupported(format!("unknown planned function identity {id}"))
        })?;
        self.emit_function_value_payload(&meta, function)
    }

    pub(crate) fn emit_function_value_payload(
        &mut self,
        meta: &WasmFunctionMeta,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        self.emit_function_value_payload_with_prototype_materialization(
            meta,
            FunctionPrototypeMaterialization::Automatic,
            function,
        )
    }

    pub(crate) fn emit_function_value_payload_with_prototype_materialization(
        &mut self,
        meta: &WasmFunctionMeta,
        policy: FunctionPrototypeMaterialization,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let captured = match self.inherited_template_source(meta)? {
            Some(owner) => schema
                .reserve_gc_local::<TemplateSource, Nullable>(function)
                .initialize(owner.capture(schema, function).nullable(), function),
            None => schema
                .reserve_gc_local::<TemplateSource, Nullable>(function)
                .initialize_null(schema, function),
        };
        let realm = self.load_current_realm(function);
        let result = self.emit_source_function_record(meta, &realm, &captured, policy, function)?;
        realm.clear(function);
        captured.clear(function);
        Ok(result)
    }

    pub(crate) fn emit_function_value_payload_with_template_source(
        &mut self,
        meta: &WasmFunctionMeta,
        policy: FunctionPrototypeMaterialization,
        owner: Option<&crate::emit::TemplateSourceExecution>,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let realm = self.load_current_realm(function);
        let result = self.emit_source_function_in_realm_with_template_source(
            meta, &realm, policy, owner, function,
        )?;
        realm.clear(function);
        Ok(result)
    }

    pub(crate) fn emit_source_function_in_realm_with_template_source(
        &mut self,
        meta: &WasmFunctionMeta,
        realm: &GcLocal<RealmRecord>,
        policy: FunctionPrototypeMaterialization,
        owner: Option<&crate::emit::TemplateSourceExecution>,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        if meta.template_source != owner.map(|owner| owner.source()) {
            return Err(EmitError::unsupported(
                "function template source must match its captured source execution",
            ));
        }
        let schema = self.runtime_schema();
        let captured = match owner {
            Some(owner) => schema
                .reserve_gc_local::<TemplateSource, Nullable>(function)
                .initialize(owner.capture(schema, function).nullable(), function),
            None => schema
                .reserve_gc_local::<TemplateSource, Nullable>(function)
                .initialize_null(schema, function),
        };
        let result = self.emit_source_function_record(meta, realm, &captured, policy, function)?;
        captured.clear(function);
        Ok(result)
    }

    pub(crate) fn emit_function_value_payload_in_realm(
        &mut self,
        meta: &WasmFunctionMeta,
        context: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        self.emit_realm_builtin_function_record(
            meta,
            context,
            FunctionPrototypeMaterialization::Automatic,
            function,
        )
    }

    pub(crate) fn emit_function_value_payload_in_realm_with_capture(
        &mut self,
        meta: &WasmFunctionMeta,
        context: &RealmFunctionMaterializationContext,
        capture: &GcLocal<BuiltinClosureCapture, Nullable>,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        self.emit_realm_builtin_function_record_with_capture(
            meta,
            context,
            FunctionPrototypeMaterialization::Automatic,
            capture,
            function,
        )
    }

    pub(crate) fn emit_realm_function_materialization_context_from_realm(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> RealmFunctionMaterializationContext {
        let schema = self.runtime_schema();
        let defining_realm = schema
            .reserve_gc_local(function)
            .initialize(realm.load(schema, function), function);
        let value = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::FunctionPrototype,
            &value,
            function,
        );
        let function_prototype = schema.reserve_gc_local(function).initialize(
            value.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        value.clear(function);
        RealmFunctionMaterializationContext {
            realm: defining_realm,
            function_prototype,
        }
    }

    pub(crate) fn emit_initialize_realm_function_materialization_context(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<RealmFunctionMaterializationContext, EmitError> {
        let schema = self.runtime_schema();
        let meta = self
            .functions
            .get(&StandardBuiltinId::FunctionPrototype.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported(
                    "Realm bootstrap requires Function.prototype's planned native entry",
                )
            })?;
        let function_prototype = schema.reserve_gc_local(function).initialize(
            self.emit_bootstrap_function_prototype_record(
                &meta,
                realm,
                object_prototype,
                function,
            )?,
            function,
        );
        let realm = schema
            .reserve_gc_local(function)
            .initialize(realm.load(schema, function), function);
        Ok(RealmFunctionMaterializationContext {
            realm,
            function_prototype,
        })
    }

    pub(crate) fn emit_store_realm_function_prototype(
        &mut self,
        context: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_reference(&context.function_prototype, schema, function);
        self.emit_store_non_array_realm_intrinsic(
            &context.realm,
            NonArrayRealmIntrinsicSlot::FunctionPrototype,
            &value,
            function,
        );
        value.clear(function);
    }

    pub(crate) fn emit_define_realm_function_prototype_data(
        &mut self,
        context: &RealmFunctionMaterializationContext,
        key: &crate::operations::PropertyKeyLocals,
        value: &ValueLocals,
        writable: bool,
        configurable: bool,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::OBJECT)
                .read(&context.function_prototype, schema, function)
                .reference(),
            function,
        );
        self.emit_object_append_data_property_with_flags(
            &header,
            key,
            value,
            writable,
            false,
            configurable,
            function,
        )?;
        header.clear(function);
        Ok(())
    }

    pub(crate) fn emit_bind_realm_function_constructor_prototype(
        &mut self,
        constructor: &GcLocal<FunctionObject>,
        context: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let constructor_header = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::OBJECT)
                .read(constructor, schema, function)
                .reference(),
            function,
        );
        let prototype_header = schema
            .reserve_gc_local::<OrdinaryObject, _>(function)
            .initialize(
                schema
                    .struct_type::<FunctionObject>()
                    .field(FunctionObjectSchema::OBJECT)
                    .read(&context.function_prototype, schema, function)
                    .reference(),
                function,
            );
        let value = schema.reserve_value_local(function);
        value.set_reference(&context.function_prototype, schema, function);
        let key = self.emit_function_string_key("prototype", function)?;
        self.emit_object_append_data_property_with_flags(
            &constructor_header,
            &key,
            &value,
            false,
            false,
            false,
            function,
        )?;
        key.clear(function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(&value, function),
            function,
        );
        schema
            .struct_type::<FunctionObject>()
            .field(FunctionObjectSchema::PUBLIC_PROTOTYPE_CACHE)
            .write(
                constructor,
                GcOperand::reference(&stored, schema),
                schema,
                function,
            );
        stored.clear(function);
        value.set_reference(constructor, schema, function);
        let key = self.emit_function_string_key("constructor", function)?;
        self.emit_object_append_data_property_with_flags(
            &prototype_header,
            &key,
            &value,
            true,
            false,
            true,
            function,
        )?;
        key.clear(function);
        value.clear(function);
        prototype_header.clear(function);
        constructor_header.clear(function);
        Ok(())
    }

    pub(crate) fn release_realm_function_materialization_context(
        &mut self,
        context: RealmFunctionMaterializationContext,
        function: &mut Function,
    ) {
        context.function_prototype.clear(function);
        context.realm.clear(function);
    }

    pub(crate) fn emit_function_or_proxy_construct_with_argv(
        &mut self,
        callee: &ValueLocals,
        new_target: &ValueLocals,
        arguments: &GcLocal<crate::gc_types::ValueArray>,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let caller_realm = self.load_current_realm(function);
        schema
            .call_helper(
                crate::runtime_helpers::ProxyConstructArguments::new(
                    callee,
                    new_target,
                    arguments,
                    &caller_realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        self.replace_current_realm(&caller_realm, function);
        caller_realm.clear(function);
        Ok(())
    }

    pub(crate) fn emit_function_handle_construct_with_argv(
        &mut self,
        callee: &ValueLocals,
        new_target: &ValueLocals,
        arguments: &GcLocal<crate::gc_types::ValueArray>,
        result: &crate::gc_types::CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_function_or_proxy_construct_with_argv(
            callee, new_target, arguments, result, function,
        )
    }
}

impl FunctionBuilder<'_> {
    /// A Function constructor captures its own Realm's global environment,
    /// independently of the Realm selected for newTarget's internal prototype.
    pub(crate) fn emit_dynamic_source_function_record(
        &mut self,
        meta: &WasmFunctionMeta,
        realm: &GcLocal<RealmRecord>,
        internal_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        use crate::gc_types::{
            Environment, PrivateElementTable, PrivateEnvironment, RealmRecordSchema,
        };
        if meta.standard_builtin.is_some()
            || meta.host_builtin.is_some()
            || meta.captures_private_environment
            || meta.is_named_expression
        {
            return Err(EmitError::unsupported(
                "dynamic source must own a planned anonymous source callable",
            ));
        }
        let schema = self.runtime_schema();
        let defining_program = self.capture_current_program(function);
        let global_environment = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<RealmRecord>()
                    .field(RealmRecordSchema::GLOBAL_ENVIRONMENT)
                    .read(realm, schema, function)
                    .reference()
                    .require_non_null(function)
                    .nullable(),
                function,
            );
        let private_environment = schema
            .reserve_gc_local::<PrivateEnvironment, Nullable>(function)
            .initialize_null(schema, function);
        let home = schema.reserve_value_local(function);
        home.set_undefined(function);
        let field_keys = schema
            .reserve_gc_local::<crate::gc_types::ValueArray, Nullable>(function)
            .initialize_null(schema, function);
        let private_methods = schema
            .reserve_gc_local::<PrivateElementTable, Nullable>(function)
            .initialize_null(schema, function);
        let builtin_capture = schema
            .reserve_gc_local::<BuiltinClosureCapture, Nullable>(function)
            .initialize_null(schema, function);
        let template = schema
            .reserve_gc_local::<TemplateSource, Nullable>(function)
            .initialize_null(schema, function);
        if let Some(source) = meta.template_source {
            let owner = self.allocate_template_source(source, realm, function);
            template.replace(owner.capture(schema, function).nullable(), function);
            owner.clear(function);
        }
        let result = self.emit_source_function_record_from_captures(
            meta,
            FunctionAllocationInputs {
                program: crate::gc_types::FunctionProgramOwner::Defining(&defining_program),
                realm,
                lexical_environment: &global_environment,
                private_environment: &private_environment,
                home_object: &home,
                field_keys: &field_keys,
                instance_private_methods: &private_methods,
                template_source: &template,
                builtin_capture: &builtin_capture,
                internal_prototype,
                typed_array_element_kind: None,
            },
            FunctionPrototypeMaterialization::Automatic,
            function,
        )?;
        template.clear(function);
        builtin_capture.clear(function);
        private_methods.clear(function);
        field_keys.clear(function);
        home.clear(function);
        private_environment.clear(function);
        global_environment.clear(function);
        defining_program.clear(function);
        Ok(result)
    }
}
