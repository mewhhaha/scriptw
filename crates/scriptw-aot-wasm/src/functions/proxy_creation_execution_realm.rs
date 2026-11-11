use super::*;
use crate::gc_types::{
    BuiltinClosureCapture, BuiltinClosurePayload, Environment, FunctionObject, GcLocal, GcOperand,
    GcStackReference, Nullable, PrivateEnvironment, ProxyObject, ProxyRevocationContext,
    RealmRecord, TemplateSource, ValueArray, ValueLocals,
};

/// All identities are loaded from one defining Realm before creation starts.
#[must_use]
pub(crate) struct ProxyCreationExecutionRealm {
    realm: GcLocal<RealmRecord>,
    object_prototype: ValueLocals,
    function_prototype: ValueLocals,
    type_error_prototype: ValueLocals,
}

impl ProxyCreationExecutionRealm {
    pub(crate) fn object_prototype(&self) -> &ValueLocals {
        &self.object_prototype
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn emit_proxy_creation_execution_realm(
        &mut self,
        function: &mut Function,
    ) -> ProxyCreationExecutionRealm {
        let schema = self.runtime_schema();
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        let object_prototype = schema.reserve_value_local(function);
        let function_prototype = schema.reserve_value_local(function);
        let type_error_prototype = schema.reserve_value_local(function);
        for (slot, value) in [
            (
                NonArrayRealmIntrinsicSlot::ObjectPrototype,
                &object_prototype,
            ),
            (
                NonArrayRealmIntrinsicSlot::FunctionPrototype,
                &function_prototype,
            ),
            (
                NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
                &type_error_prototype,
            ),
        ] {
            self.emit_load_non_array_realm_intrinsic(&realm, slot, value, function);
        }
        ProxyCreationExecutionRealm {
            realm,
            object_prototype,
            function_prototype,
            type_error_prototype,
        }
    }

    pub(crate) fn emit_throw_proxy_creation_type_error(
        &mut self,
        realm: &ProxyCreationExecutionRealm,
        message: RuntimeErrorMessage,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(function);
        self.emit_throw_runtime_error_with_prototype(
            NativeErrorKind::TypeError,
            message,
            &realm.type_error_prototype,
            &result,
            function,
        )?;
        self.completion().copy_from(&result, function);
        result.clear(function);
        Ok(())
    }

    /// A revoker is a native callable with its own captured Proxy. This value
    /// is independent of caller this and can be cleared exactly once by the body.
    pub(crate) fn emit_proxy_revoke_target_function(
        &mut self,
        realm: &ProxyCreationExecutionRealm,
        proxy: &GcLocal<ProxyObject>,
        function: &mut Function,
    ) -> Result<GcStackReference<FunctionObject>, EmitError> {
        let schema = self.runtime_schema();
        let mut meta = self
            .functions
            .get(&StandardBuiltinId::ProxyRevoke.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported("compiler invariant: missing Proxy revoker entry")
            })?;
        meta.name.clear();
        meta.length = 0;
        meta.strict = true;
        meta.length_name_configurable = true;
        meta.to_string_value = "function () { [native code] }".into();
        let revocation = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ProxyRevocationContext>()
                .construct((GcOperand::nullable_reference(proxy, schema),), function),
            function,
        );
        let capture = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<BuiltinClosureCapture>().publish(
                BuiltinClosurePayload::ProxyRevocation(&revocation),
                schema,
                function,
            ),
            function,
        );
        let captured = schema
            .reserve_gc_local::<BuiltinClosureCapture, Nullable>(function)
            .initialize(capture.load(schema, function).nullable(), function);
        let lexical = schema
            .reserve_gc_local::<Environment, Nullable>(function)
            .initialize_null(schema, function);
        let private = schema
            .reserve_gc_local::<PrivateEnvironment, Nullable>(function)
            .initialize_null(schema, function);
        let field_keys = schema
            .reserve_gc_local::<ValueArray, Nullable>(function)
            .initialize_null(schema, function);
        let instance_private_methods = schema
            .reserve_gc_local::<crate::gc_types::PrivateElementTable, Nullable>(function)
            .initialize_null(schema, function);
        let template = schema
            .reserve_gc_local::<TemplateSource, Nullable>(function)
            .initialize_null(schema, function);
        let home = schema.reserve_value_local(function);
        home.set_undefined(function);
        let published = self.emit_complete_function_record(
            &meta,
            FunctionAllocationInputs {
                program: crate::gc_types::FunctionProgramOwner::InheritCaller,
                realm: &realm.realm,
                lexical_environment: &lexical,
                private_environment: &private,
                home_object: &home,
                field_keys: &field_keys,
                instance_private_methods: &instance_private_methods,
                template_source: &template,
                builtin_capture: &captured,
                internal_prototype: &realm.function_prototype,
                typed_array_element_kind: None,
            },
            FunctionPrototypeMaterialization::BootstrapSupplied,
            function,
        )?;
        home.clear(function);
        template.clear(function);
        instance_private_methods.clear(function);
        field_keys.clear(function);
        private.clear(function);
        lexical.clear(function);
        captured.clear(function);
        capture.clear(function);
        revocation.clear(function);
        Ok(published)
    }

    pub(crate) fn release_proxy_creation_execution_realm(
        &mut self,
        realm: ProxyCreationExecutionRealm,
        function: &mut Function,
    ) {
        realm.type_error_prototype.clear(function);
        realm.function_prototype.clear(function);
        realm.object_prototype.clear(function);
        realm.realm.clear(function);
    }
}
