use super::super::*;
use super::atomics::ATOMICS_PUBLICATION_ORDER;
use crate::functions::{NonArrayRealmIntrinsicSlot, RealmFunctionMaterializationContext};
use crate::gc_types::*;
use crate::intrinsics::IntrinsicKey;
use crate::objects::{
    AccessorDescriptor, AccessorDescriptorLocals, AccessorGetter, AccessorGetterLocals,
    AccessorSetterLocals,
};
use scriptw_ir::StandardBuiltinInstaller;

mod realm_initialization;

/// Bootstrap holds only actual rooted products until the global object owns
/// them. It is consumed once; no namespace or Realm address is exported.
#[must_use]
pub(crate) struct BootstrapRealm {
    realm: RealmFunctionMaterializationContext,
    object_prototype: ValueLocals,
    reflect: Option<ValueLocals>,
    math: Option<ValueLocals>,
    json: Option<ValueLocals>,
    atomics: Option<ValueLocals>,
    temporal: Option<ValueLocals>,
    intl: Option<ValueLocals>,
}
/// The canonical globals of every Realm the runtime creates. The string pool
/// interns these names in its compiler-owned phase, so the bodies that install
/// them never depend on the program's own global bindings.
pub(crate) fn created_realm_global_bindings() -> Vec<scriptw_ir::ScriptGlobalBindingIr> {
    let mut bindings = Vec::new();
    let mut append = |name: &str, initializer| {
        bindings.push(scriptw_ir::ScriptGlobalBindingIr {
            name: name.to_owned(),
            initializer,
            declarations: scriptw_ir::GlobalDeclarationSetIr::None,
        })
    };
    append(GLOBAL_THIS_NAME, GlobalPropertyInitializerIr::Intrinsic);
    append("Infinity", GlobalPropertyInitializerIr::Infinity);
    append("NaN", GlobalPropertyInitializerIr::NaN);
    append("undefined", GlobalPropertyInitializerIr::Undefined);
    append("Reflect", GlobalPropertyInitializerIr::ReflectObject);
    append("Math", GlobalPropertyInitializerIr::MathObject);
    append("JSON", GlobalPropertyInitializerIr::JsonObject);
    append("Atomics", GlobalPropertyInitializerIr::AtomicsObject);
    append("Temporal", GlobalPropertyInitializerIr::TemporalObject);
    append("Intl", GlobalPropertyInitializerIr::IntlObject);
    for builtin in StandardBuiltinId::all_globals() {
        if let Some(name) = builtin.global_name() {
            append(name, GlobalPropertyInitializerIr::BuiltinFunction(*builtin));
        }
    }
    for builtin in HostBuiltinId::every_realm_globals() {
        append(
            builtin
                .global_name()
                .expect("every-Realm host row owns a global name"),
            GlobalPropertyInitializerIr::HostFunction(builtin),
        );
    }
    bindings
}

impl BootstrapRealm {
    fn clear(self, builder: &mut FunctionBuilder<'_>, function: &mut Function) {
        for value in [
            self.intl,
            self.temporal,
            self.atomics,
            self.json,
            self.math,
            self.reflect,
        ]
        .into_iter()
        .flatten()
        {
            value.clear(function);
        }
        builder.release_realm_function_materialization_context(self.realm, function);
        self.object_prototype.clear(function);
    }
}

impl<'a> FunctionBuilder<'a> {
    pub(crate) fn init_builtin_constructor_object(
        &mut self,
        builtin: StandardBuiltinId,
        realm: &crate::functions::RealmFunctionMaterializationContext,
        prototype: &crate::gc_types::ValueLocals,
        function: &mut Function,
    ) -> Result<crate::gc_types::GcLocal<crate::gc_types::FunctionObject>, EmitError> {
        use crate::gc_types::*;
        use crate::intrinsics::IntrinsicKey;
        let meta = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported(format!(
                    "missing planned intrinsic constructor {}",
                    builtin.debug_name()
                ))
            })?;
        let schema = self.runtime_schema();
        let constructor = schema
            .reserve_gc_local::<FunctionObject, NonNullable>(function)
            .initialize(
                self.emit_realm_constructor_function_record(&meta, realm, function)?,
                function,
            );
        let constructor_value = schema.reserve_value_local(function);
        constructor_value.set_reference(&constructor, schema, function);
        let header = schema
            .reserve_gc_local::<OrdinaryObject, NonNullable>(function)
            .initialize(
                self.emit_object_header_projection(&constructor_value, function),
                function,
            );
        let parent_constructor = if matches!(
            builtin,
            StandardBuiltinId::EvalErrorConstructor
                | StandardBuiltinId::AggregateErrorConstructor
                | StandardBuiltinId::SuppressedErrorConstructor
                | StandardBuiltinId::RangeErrorConstructor
                | StandardBuiltinId::SyntaxErrorConstructor
                | StandardBuiltinId::TypeErrorConstructor
                | StandardBuiltinId::URIErrorConstructor
                | StandardBuiltinId::ReferenceErrorConstructor
        ) {
            Some(NonArrayRealmIntrinsicSlot::ErrorConstructor)
        } else if is_typed_array_constructor(builtin) {
            Some(NonArrayRealmIntrinsicSlot::TypedArrayConstructor)
        } else {
            None
        };
        if let Some(slot) = parent_constructor {
            let parent = schema.reserve_value_local(function);
            self.emit_load_non_array_realm_intrinsic(realm.realm(), slot, &parent, function);
            let stored = schema
                .reserve_gc_local(function)
                .initialize(schema.prototype_reference(&parent, function), function);
            schema
                .struct_type::<OrdinaryObject>()
                .field(OrdinaryObjectSchema::PROTOTYPE)
                .write(
                    &header,
                    GcOperand::reference(&stored, schema),
                    schema,
                    function,
                );
            stored.clear(function);
            parent.clear(function);
        }
        if builtin != StandardBuiltinId::ProxyConstructor {
            self.emit_install_intrinsic_data(
                &constructor_value,
                IntrinsicKey::Name("prototype"),
                prototype,
                false,
                false,
                false,
                function,
            )?;
            let stored = schema
                .reserve_gc_local::<StoredValue, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(prototype, function),
                    function,
                );
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::PUBLIC_PROTOTYPE_CACHE)
                .write(
                    &constructor,
                    GcOperand::reference(&stored, schema),
                    schema,
                    function,
                );
            stored.clear(function);
            if builtin != StandardBuiltinId::IteratorConstructor {
                self.emit_install_intrinsic_data(
                    prototype,
                    IntrinsicKey::Name("constructor"),
                    &constructor_value,
                    true,
                    false,
                    true,
                    function,
                )?;
            }
        }
        if is_typed_array_constructor(builtin) {
            let size = typed_array_bytes_per_element(builtin) as f64;
            self.emit_install_intrinsic_number(
                &constructor_value,
                "BYTES_PER_ELEMENT",
                size,
                false,
                false,
                false,
                function,
            )?;
            self.emit_install_intrinsic_number(
                prototype,
                "BYTES_PER_ELEMENT",
                size,
                false,
                false,
                false,
                function,
            )?;
        }
        let slot = standard_builtin_constructor_realm_slot(builtin).ok_or_else(|| {
            EmitError::unsupported("intrinsic constructor has no canonical Realm slot")
        })?;
        self.emit_store_non_array_realm_intrinsic(
            realm.realm(),
            slot,
            &constructor_value,
            function,
        );
        let intrinsic_context = IntrinsicInstall {
            builtin,
            realm,
            constructor: &constructor_value,
            prototype,
        };
        match builtin.intrinsic_installer() {
            StandardBuiltinInstaller::None => {}
            StandardBuiltinInstaller::ShadowRealm => {
                self.install_shadow_realm_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Function => {
                self.install_function_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Promise => {
                self.install_promise_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Map => {
                self.install_map_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::WeakMap => {
                self.install_weak_map_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::WeakSet => {
                self.install_weak_set_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::WeakRef => {
                self.install_weak_ref_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::FinalizationRegistry => self
                .install_finalization_registry_constructor_intrinsics(
                    &intrinsic_context,
                    function,
                )?,
            StandardBuiltinInstaller::AsyncDisposableStack => self
                .install_async_disposable_stack_constructor_intrinsics(
                    &intrinsic_context,
                    function,
                )?,
            StandardBuiltinInstaller::DisposableStack => {
                self.install_disposable_stack_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Set => {
                self.install_set_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Object => {
                self.install_object_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Proxy => {
                self.install_proxy_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::RegExp => {
                self.install_regexp_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Iterator => {
                self.install_iterator_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Array => {
                self.install_array_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::String => {
                self.install_string_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::ArrayBuffer => {
                self.install_array_buffer_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::DataView => {
                self.install_data_view_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Uint8Array => {
                self.install_uint8_array_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::TemporalInstant => {
                self.install_temporal_instant_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::TemporalZonedDateTime => self
                .install_temporal_zoned_date_time_constructor_intrinsics(
                    &intrinsic_context,
                    function,
                )?,
            StandardBuiltinInstaller::TemporalPlainDate => self
                .install_temporal_plain_date_constructor_intrinsics(&intrinsic_context, function)?,
            StandardBuiltinInstaller::TemporalDuration => {
                self.install_temporal_duration_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::TemporalPlainTime => self
                .install_temporal_plain_time_constructor_intrinsics(&intrinsic_context, function)?,
            StandardBuiltinInstaller::TemporalPlainDateTime => self
                .install_temporal_plain_date_time_constructor_intrinsics(
                    &intrinsic_context,
                    function,
                )?,
            StandardBuiltinInstaller::TemporalPlainYearMonth => self
                .install_temporal_plain_year_month_constructor_intrinsics(
                    &intrinsic_context,
                    function,
                )?,
            StandardBuiltinInstaller::TemporalPlainMonthDay => self
                .install_temporal_plain_month_day_constructor_intrinsics(
                    &intrinsic_context,
                    function,
                )?,
            StandardBuiltinInstaller::IntlLocale => {
                self.install_intl_locale_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::IntlDateTimeFormat => self
                .install_intl_date_time_format_constructor_intrinsics(
                    &intrinsic_context,
                    function,
                )?,
            StandardBuiltinInstaller::IntlNumberFormat => self
                .install_intl_number_format_constructor_intrinsics(&intrinsic_context, function)?,
            StandardBuiltinInstaller::IntlPluralRules => {
                self.install_intl_plural_rules_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::IntlListFormat => {
                self.install_intl_list_format_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::IntlCollator => {
                self.install_intl_collator_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::IntlDisplayNames => self
                .install_intl_display_names_constructor_intrinsics(&intrinsic_context, function)?,
            StandardBuiltinInstaller::IntlRelativeTimeFormat => self
                .install_intl_relative_time_constructor_intrinsics(&intrinsic_context, function)?,
            StandardBuiltinInstaller::IntlSegmenter => {
                self.install_intl_segmenter_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::IntlDurationFormat => self
                .install_intl_durationformat_constructor_intrinsics(&intrinsic_context, function)?,
            StandardBuiltinInstaller::Date => {
                self.install_date_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Error => {
                self.install_error_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::BigInt => {
                self.install_big_int_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Symbol => {
                self.install_symbol_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Number => {
                self.install_number_constructor_intrinsics(&intrinsic_context, function)?
            }
            StandardBuiltinInstaller::Boolean => {
                self.install_boolean_constructor_intrinsics(&intrinsic_context, function)?
            }
        }

        header.clear(function);
        constructor_value.clear(function);
        Ok(constructor)
    }

    fn emit_bootstrap_namespace(
        &mut self,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let schema = self.runtime_schema();
        let object = schema
            .reserve_gc_local::<OrdinaryObject, NonNullable>(function)
            .initialize(
                self.emit_alloc_plain_object_with_prototype(Some(object_prototype), function)?,
                function,
            );
        let value = schema.reserve_value_local(function);
        value.set_reference(&object, schema, function);
        object.clear(function);
        Ok(value)
    }

    pub(crate) fn init_throw_type_error_intrinsic(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let thrower =
            self.emit_intrinsic_callable(StandardBuiltinId::ThrowTypeError, realm, function)?;
        let value = schema.reserve_value_local(function);
        value.set_reference(&thrower, schema, function);
        let prototype = schema.reserve_value_local(function);
        prototype.set_reference(realm.function_prototype(), schema, function);
        for name in ["arguments", "caller"] {
            self.emit_install_intrinsic_accessor_values(
                &prototype,
                IntrinsicKey::Name(name),
                AccessorDescriptorLocals::GetterAndSetter {
                    getter: AccessorGetterLocals::new(&value),
                    setter: AccessorSetterLocals::new(&value),
                },
                true,
                function,
            )?;
        }
        prototype.clear(function);
        value.clear(function);
        thrower.clear(function);
        Ok(())
    }

    pub(crate) fn init_reflect_object(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let object = self.emit_bootstrap_namespace(object_prototype, function)?;
        self.emit_install_intrinsic_methods(
            &object,
            crate::intrinsics::IntrinsicMethodPlan::Reflect,
            realm,
            function,
        )?;
        self.emit_install_intrinsic_string(
            &object,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            "Reflect",
            false,
            false,
            true,
            function,
        )?;
        Ok(object)
    }

    fn init_temporal_now_object(
        &mut self,
        members: TemporalNamespaceMembers,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let object = self.emit_bootstrap_namespace(object_prototype, function)?;
        self.emit_install_intrinsic_string(
            &object,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            "Temporal.Now",
            false,
            false,
            true,
            function,
        )?;
        for (name, builtin) in members.now_members_in_installation_order() {
            self.emit_install_intrinsic_method(
                &object,
                IntrinsicKey::Name(name),
                builtin,
                realm,
                true,
                true,
                function,
            )?;
        }
        Ok(object)
    }

    pub(crate) fn init_temporal_object(
        &mut self,
        members: TemporalNamespaceMembers,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let object = self.emit_bootstrap_namespace(object_prototype, function)?;
        let now = self.init_temporal_now_object(members, realm, object_prototype, function)?;
        self.emit_install_intrinsic_data(
            &object,
            IntrinsicKey::Name(TEMPORAL_NOW_NAME),
            &now,
            true,
            false,
            true,
            function,
        )?;
        now.clear(function);
        let value = self.runtime_schema().reserve_value_local(function);
        for (name, builtin) in members.constructors_in_installation_order() {
            let slot = standard_builtin_constructor_realm_slot(builtin).ok_or_else(|| {
                EmitError::unsupported("Temporal constructor has no intrinsic slot")
            })?;
            self.emit_load_non_array_realm_intrinsic(realm.realm(), slot, &value, function);
            self.emit_install_intrinsic_data(
                &object,
                IntrinsicKey::Name(name),
                &value,
                true,
                false,
                true,
                function,
            )?;
        }
        value.clear(function);
        self.emit_install_intrinsic_string(
            &object,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            "Temporal",
            false,
            false,
            true,
            function,
        )?;
        Ok(object)
    }

    pub(crate) fn init_intl_object(
        &mut self,
        members: IntlNamespaceMembers,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let object = self.emit_bootstrap_namespace(object_prototype, function)?;
        for (name, builtin) in members.methods_in_installation_order() {
            self.emit_install_intrinsic_method(
                &object,
                IntrinsicKey::Name(name),
                builtin,
                realm,
                true,
                true,
                function,
            )?;
        }
        let value = self.runtime_schema().reserve_value_local(function);
        for (name, builtin) in members.in_installation_order() {
            let slot = standard_builtin_constructor_realm_slot(builtin)
                .ok_or_else(|| EmitError::unsupported("Intl constructor has no intrinsic slot"))?;
            self.emit_load_non_array_realm_intrinsic(realm.realm(), slot, &value, function);
            self.emit_install_intrinsic_data(
                &object,
                IntrinsicKey::Name(name),
                &value,
                true,
                false,
                true,
                function,
            )?;
        }
        value.clear(function);
        self.emit_install_intrinsic_string(
            &object,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            "Intl",
            false,
            false,
            true,
            function,
        )?;
        Ok(object)
    }

    pub(crate) fn init_math_object(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let object = self.emit_bootstrap_namespace(object_prototype, function)?;
        for (name, value) in [
            ("E", std::f64::consts::E),
            ("LN10", std::f64::consts::LN_10),
            ("LN2", std::f64::consts::LN_2),
            ("LOG10E", std::f64::consts::LOG10_E),
            ("LOG2E", std::f64::consts::LOG2_E),
            ("PI", std::f64::consts::PI),
            ("SQRT1_2", std::f64::consts::FRAC_1_SQRT_2),
            ("SQRT2", std::f64::consts::SQRT_2),
        ] {
            self.emit_install_intrinsic_number(
                &object, name, value, false, false, false, function,
            )?;
        }
        self.emit_install_intrinsic_methods(
            &object,
            crate::intrinsics::IntrinsicMethodPlan::Math,
            realm,
            function,
        )?;
        self.emit_install_intrinsic_string(
            &object,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            "Math",
            false,
            false,
            true,
            function,
        )?;
        Ok(object)
    }

    pub(crate) fn init_json_object(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let object = self.emit_bootstrap_namespace(object_prototype, function)?;
        self.emit_install_intrinsic_string(
            &object,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            JSON_NAME,
            false,
            false,
            true,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            &object,
            crate::intrinsics::IntrinsicMethodPlan::Json,
            realm,
            function,
        )?;
        Ok(object)
    }

    pub(crate) fn init_atomics_object(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let object = self.emit_bootstrap_namespace(object_prototype, function)?;
        self.emit_install_intrinsic_string(
            &object,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            ATOMICS_NAME,
            false,
            false,
            true,
            function,
        )?;
        for builtin in ATOMICS_PUBLICATION_ORDER {
            let name = builtin
                .native_function_name()
                .ok_or_else(|| EmitError::unsupported("Atomics method has no native name"))?;
            self.emit_install_intrinsic_method(
                &object,
                IntrinsicKey::Name(name),
                builtin,
                realm,
                true,
                true,
                function,
            )?;
        }
        Ok(object)
    }

    pub(crate) fn init_typed_array_intrinsic(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm.realm(),
            NonArrayRealmIntrinsicSlot::TypedArrayPrototype,
            &prototype,
            function,
        );
        let constructor = self.init_builtin_constructor_object(
            StandardBuiltinId::TypedArrayConstructor,
            realm,
            &prototype,
            function,
        )?;
        let constructor_value = schema.reserve_value_local(function);
        constructor_value.set_reference(&constructor, schema, function);
        self.emit_install_intrinsic_accessor(
            &constructor_value,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Species),
            AccessorDescriptor::Getter(AccessorGetter::new(
                StandardBuiltinId::TypedArraySpeciesGetter,
            )),
            realm,
            true,
            function,
        )?;
        for (name, builtin) in [
            ("buffer", StandardBuiltinId::TypedArrayPrototypeBufferGetter),
            (
                "byteLength",
                StandardBuiltinId::TypedArrayPrototypeByteLengthGetter,
            ),
            (
                "byteOffset",
                StandardBuiltinId::TypedArrayPrototypeByteOffsetGetter,
            ),
            ("length", StandardBuiltinId::TypedArrayPrototypeLengthGetter),
        ] {
            self.emit_install_intrinsic_accessor(
                &prototype,
                IntrinsicKey::Name(name),
                AccessorDescriptor::Getter(AccessorGetter::new(builtin)),
                realm,
                true,
                function,
            )?;
        }
        self.emit_install_intrinsic_accessor(
            &prototype,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            AccessorDescriptor::Getter(AccessorGetter::new(
                StandardBuiltinId::TypedArrayPrototypeToStringTagGetter,
            )),
            realm,
            true,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            &prototype,
            crate::intrinsics::IntrinsicMethodPlan::TypedArrayPrototypeBeforeAliases,
            realm,
            function,
        )?;
        self.emit_install_intrinsic_method_aliases(
            &prototype,
            &[
                IntrinsicKey::Name("values"),
                IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Iterator),
            ],
            StandardBuiltinId::TypedArrayPrototypeValues,
            realm,
            true,
            true,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            &prototype,
            crate::intrinsics::IntrinsicMethodPlan::TypedArrayPrototypeAfterAliases,
            realm,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            &constructor_value,
            crate::intrinsics::IntrinsicMethodPlan::TypedArrayConstructor,
            realm,
            function,
        )?;
        constructor_value.clear(function);
        constructor.clear(function);
        prototype.clear(function);
        Ok(())
    }

    fn emit_bootstrap_iterator_prototypes(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_value_local(function);
        for (slot, next, tag) in [
            (
                NonArrayRealmIntrinsicSlot::ArrayIteratorPrototype,
                StandardBuiltinId::ArrayIteratorNext,
                "Array Iterator",
            ),
            (
                NonArrayRealmIntrinsicSlot::StringIteratorPrototype,
                StandardBuiltinId::StringIteratorNext,
                "String Iterator",
            ),
            (
                NonArrayRealmIntrinsicSlot::RegExpStringIteratorPrototype,
                StandardBuiltinId::RegExpStringIteratorNext,
                "RegExp String Iterator",
            ),
            (
                NonArrayRealmIntrinsicSlot::MapIteratorPrototype,
                StandardBuiltinId::MapIteratorNext,
                "Map Iterator",
            ),
            (
                NonArrayRealmIntrinsicSlot::SetIteratorPrototype,
                StandardBuiltinId::SetIteratorNext,
                "Set Iterator",
            ),
        ] {
            self.emit_load_non_array_realm_intrinsic(realm.realm(), slot, &prototype, function);
            self.emit_install_intrinsic_method(
                &prototype,
                IntrinsicKey::Name("next"),
                next,
                realm,
                true,
                true,
                function,
            )?;
            if matches!(slot, NonArrayRealmIntrinsicSlot::ArrayIteratorPrototype) {
                self.emit_install_intrinsic_method(
                    &prototype,
                    IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Iterator),
                    StandardBuiltinId::ArrayIteratorIdentity,
                    realm,
                    true,
                    true,
                    function,
                )?;
            }
            self.emit_install_intrinsic_string(
                &prototype,
                IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
                tag,
                false,
                false,
                true,
                function,
            )?;
        }
        let value = schema.reserve_value_local(function);
        for (slot, constructor_slot, next, return_method, throw_method, tag) in [
            (
                NonArrayRealmIntrinsicSlot::GeneratorPrototype,
                NonArrayRealmIntrinsicSlot::GeneratorFunctionPrototype,
                StandardBuiltinId::GeneratorPrototypeNext,
                StandardBuiltinId::GeneratorPrototypeReturn,
                StandardBuiltinId::GeneratorPrototypeThrow,
                "Generator",
            ),
            (
                NonArrayRealmIntrinsicSlot::AsyncGeneratorPrototype,
                NonArrayRealmIntrinsicSlot::AsyncGeneratorFunctionPrototype,
                StandardBuiltinId::AsyncGeneratorPrototypeNext,
                StandardBuiltinId::AsyncGeneratorPrototypeReturn,
                StandardBuiltinId::AsyncGeneratorPrototypeThrow,
                "AsyncGenerator",
            ),
        ] {
            self.emit_load_non_array_realm_intrinsic(realm.realm(), slot, &prototype, function);
            self.emit_load_non_array_realm_intrinsic(
                realm.realm(),
                constructor_slot,
                &value,
                function,
            );
            self.emit_install_intrinsic_data(
                &prototype,
                IntrinsicKey::Name("constructor"),
                &value,
                false,
                false,
                true,
                function,
            )?;
            for (name, builtin) in [
                ("next", next),
                ("return", return_method),
                ("throw", throw_method),
            ] {
                self.emit_install_intrinsic_method(
                    &prototype,
                    IntrinsicKey::Name(name),
                    builtin,
                    realm,
                    true,
                    true,
                    function,
                )?;
            }
            self.emit_install_intrinsic_string(
                &prototype,
                IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
                tag,
                false,
                false,
                true,
                function,
            )?;
        }
        value.clear(function);
        self.emit_load_non_array_realm_intrinsic(
            realm.realm(),
            NonArrayRealmIntrinsicSlot::AsyncIteratorPrototype,
            &prototype,
            function,
        );
        let mut identity_meta = self
            .functions
            .get(&StandardBuiltinId::ArrayIteratorIdentity.function_id())
            .cloned()
            .ok_or_else(|| EmitError::unsupported("missing async iterator identity entry"))?;
        identity_meta.name = "[Symbol.asyncIterator]".to_owned();
        identity_meta.to_string_value =
            "function [Symbol.asyncIterator]() { [native code] }".to_owned();
        let identity = schema
            .reserve_gc_local::<FunctionObject, NonNullable>(function)
            .initialize(
                self.emit_function_value_payload_in_realm(&identity_meta, realm, function)?,
                function,
            );
        let value = schema.reserve_value_local(function);
        value.set_reference(&identity, schema, function);
        self.emit_install_intrinsic_data(
            &prototype,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::AsyncIterator),
            &value,
            true,
            false,
            true,
            function,
        )?;
        value.clear(function);
        identity.clear(function);
        self.emit_install_intrinsic_method(
            &prototype,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::AsyncDispose),
            StandardBuiltinId::AsyncIteratorPrototypeAsyncDispose,
            realm,
            true,
            true,
            function,
        )?;
        prototype.clear(function);
        Ok(())
    }

    fn emit_bootstrap_dynamic_function_intrinsics(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        for (builtin, prototype_slot, constructor_slot, instance_slot, tag) in [
            (
                HostBuiltinId::GeneratorFunctionConstructor,
                NonArrayRealmIntrinsicSlot::GeneratorFunctionPrototype,
                NonArrayRealmIntrinsicSlot::GeneratorFunctionConstructor,
                Some(NonArrayRealmIntrinsicSlot::GeneratorPrototype),
                "GeneratorFunction",
            ),
            (
                HostBuiltinId::AsyncFunctionConstructor,
                NonArrayRealmIntrinsicSlot::AsyncFunctionPrototype,
                NonArrayRealmIntrinsicSlot::AsyncFunctionConstructor,
                None,
                "AsyncFunction",
            ),
            (
                HostBuiltinId::AsyncGeneratorFunctionConstructor,
                NonArrayRealmIntrinsicSlot::AsyncGeneratorFunctionPrototype,
                NonArrayRealmIntrinsicSlot::AsyncGeneratorFunctionConstructor,
                Some(NonArrayRealmIntrinsicSlot::AsyncGeneratorPrototype),
                "AsyncGeneratorFunction",
            ),
        ] {
            let meta = self
                .functions
                .get(&builtin.function_id())
                .cloned()
                .ok_or_else(|| {
                    EmitError::unsupported(format!(
                        "missing dynamic intrinsic {}",
                        builtin.as_str()
                    ))
                })?;
            let prototype = schema.reserve_value_local(function);
            self.emit_load_non_array_realm_intrinsic(
                realm.realm(),
                prototype_slot,
                &prototype,
                function,
            );
            let constructor = schema
                .reserve_gc_local::<FunctionObject, NonNullable>(function)
                .initialize(
                    self.emit_realm_constructor_function_record(&meta, realm, function)?,
                    function,
                );
            let constructor_value = schema.reserve_value_local(function);
            constructor_value.set_reference(&constructor, schema, function);
            let header = schema
                .reserve_gc_local::<OrdinaryObject, NonNullable>(function)
                .initialize(
                    self.emit_object_header_projection(&constructor_value, function),
                    function,
                );
            let function_constructor = schema.reserve_value_local(function);
            self.emit_load_non_array_realm_intrinsic(
                realm.realm(),
                NonArrayRealmIntrinsicSlot::FunctionConstructor,
                &function_constructor,
                function,
            );
            let parent = schema.reserve_gc_local(function).initialize(
                schema.prototype_reference(&function_constructor, function),
                function,
            );
            schema
                .struct_type::<OrdinaryObject>()
                .field(OrdinaryObjectSchema::PROTOTYPE)
                .write(
                    &header,
                    GcOperand::reference(&parent, schema),
                    schema,
                    function,
                );
            parent.clear(function);
            function_constructor.clear(function);
            header.clear(function);
            self.emit_install_intrinsic_data(
                &constructor_value,
                IntrinsicKey::Name("prototype"),
                &prototype,
                false,
                false,
                false,
                function,
            )?;
            let stored = schema
                .reserve_gc_local::<StoredValue, NonNullable>(function)
                .initialize(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(&prototype, function),
                    function,
                );
            schema
                .struct_type::<FunctionObject>()
                .field(FunctionObjectSchema::PUBLIC_PROTOTYPE_CACHE)
                .write(
                    &constructor,
                    GcOperand::reference(&stored, schema),
                    schema,
                    function,
                );
            stored.clear(function);
            self.emit_store_non_array_realm_intrinsic(
                realm.realm(),
                constructor_slot,
                &constructor_value,
                function,
            );
            if let Some(instance_slot) = instance_slot {
                let instance = schema.reserve_value_local(function);
                self.emit_load_non_array_realm_intrinsic(
                    realm.realm(),
                    instance_slot,
                    &instance,
                    function,
                );
                self.emit_install_intrinsic_data(
                    &prototype,
                    IntrinsicKey::Name("prototype"),
                    &instance,
                    false,
                    false,
                    true,
                    function,
                )?;
                instance.clear(function);
            }
            self.emit_install_intrinsic_data(
                &prototype,
                IntrinsicKey::Name("constructor"),
                &constructor_value,
                false,
                false,
                true,
                function,
            )?;
            self.emit_install_intrinsic_string(
                &prototype,
                IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
                tag,
                false,
                false,
                true,
                function,
            )?;
            constructor_value.clear(function);
            constructor.clear(function);
            prototype.clear(function);
        }
        Ok(())
    }

    fn emit_bootstrap_plain_prototype(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        slot: NonArrayRealmIntrinsicSlot,
        parent: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let value = self.emit_bootstrap_namespace(parent, function)?;
        self.emit_store_non_array_realm_intrinsic(realm.realm(), slot, &value, function);
        value.clear(function);
        Ok(())
    }

    fn emit_bootstrap_boxed_prototype(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        slot: NonArrayRealmIntrinsicSlot,
        parent: &ValueLocals,
        primitive: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let header = schema
            .reserve_gc_local::<OrdinaryObject, NonNullable>(function)
            .initialize(
                self.emit_alloc_plain_object_with_prototype(Some(parent), function)?,
                function,
            );
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(primitive, function),
                function,
            );
        let object = schema
            .reserve_gc_local::<PrimitiveBox, NonNullable>(function)
            .initialize(
                schema.struct_type::<PrimitiveBox>().construct(
                    (
                        GcOperand::reference(&header, schema),
                        GcOperand::reference(&stored, schema),
                    ),
                    function,
                ),
                function,
            );
        let value = schema.reserve_value_local(function);
        value.set_reference(&object, schema, function);
        if slot.gc_index() == NonArrayRealmIntrinsicSlot::StringPrototype.gc_index() {
            self.emit_install_intrinsic_number(
                &value, "length", 0.0, false, false, false, function,
            )?;
        }
        self.emit_store_non_array_realm_intrinsic(realm.realm(), slot, &value, function);
        value.clear(function);
        object.clear(function);
        stored.clear(function);
        header.clear(function);
        Ok(())
    }

    fn emit_bootstrap_constructor_from_slot(
        &mut self,
        builtin: StandardBuiltinId,
        realm: &RealmFunctionMaterializationContext,
        slot: NonArrayRealmIntrinsicSlot,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let prototype = self.runtime_schema().reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(realm.realm(), slot, &prototype, function);
        let constructor =
            self.init_builtin_constructor_object(builtin, realm, &prototype, function)?;
        constructor.clear(function);
        prototype.clear(function);
        Ok(())
    }

    fn emit_initialize_realm_intrinsics_inner(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<BootstrapRealm, EmitError> {
        let schema = self.runtime_schema();
        let object = schema
            .reserve_gc_local::<OrdinaryObject, NonNullable>(function)
            .initialize(
                self.emit_alloc_immutable_prototype_object(function)?,
                function,
            );
        let object_prototype = schema.reserve_value_local(function);
        object_prototype.set_reference(&object, schema, function);
        object.clear(function);
        self.emit_store_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &object_prototype,
            function,
        );
        let context = self.emit_initialize_realm_function_materialization_context(
            realm,
            &object_prototype,
            function,
        )?;
        self.emit_store_realm_function_prototype(&context, function);
        let function_prototype = schema.reserve_value_local(function);
        function_prototype.set_reference(context.function_prototype(), schema, function);
        let reserved = self.reserve_realm_array_prototype_local(function);
        let array =
            self.emit_initialize_realm_array_prototype(reserved, &object_prototype, function)?;
        self.emit_store_realm_array_prototype(realm, array.array(), function);
        self.release_realm_array_prototype_local(array, function);
        self.emit_bootstrap_plain_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::IteratorPrototype,
            &object_prototype,
            function,
        )?;
        let parent = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::IteratorPrototype,
            &parent,
            function,
        );
        for slot in [
            NonArrayRealmIntrinsicSlot::IteratorHelperPrototype,
            NonArrayRealmIntrinsicSlot::IteratorFromWrapperPrototype,
            NonArrayRealmIntrinsicSlot::ArrayIteratorPrototype,
            NonArrayRealmIntrinsicSlot::StringIteratorPrototype,
            NonArrayRealmIntrinsicSlot::RegExpStringIteratorPrototype,
            NonArrayRealmIntrinsicSlot::MapIteratorPrototype,
            NonArrayRealmIntrinsicSlot::SetIteratorPrototype,
            NonArrayRealmIntrinsicSlot::GeneratorPrototype,
            NonArrayRealmIntrinsicSlot::IntlSegmentIteratorPrototype,
        ] {
            self.emit_bootstrap_plain_prototype(&context, slot, &parent, function)?;
        }
        for slot in [
            NonArrayRealmIntrinsicSlot::GeneratorFunctionPrototype,
            NonArrayRealmIntrinsicSlot::AsyncFunctionPrototype,
            NonArrayRealmIntrinsicSlot::AsyncGeneratorFunctionPrototype,
        ] {
            self.emit_bootstrap_plain_prototype(&context, slot, &function_prototype, function)?;
        }
        self.emit_bootstrap_plain_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::AsyncIteratorPrototype,
            &object_prototype,
            function,
        )?;
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::AsyncIteratorPrototype,
            &parent,
            function,
        );
        self.emit_bootstrap_plain_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::AsyncGeneratorPrototype,
            &parent,
            function,
        )?;
        for slot in [
            NonArrayRealmIntrinsicSlot::PromisePrototype,
            NonArrayRealmIntrinsicSlot::MapPrototype,
            NonArrayRealmIntrinsicSlot::WeakMapPrototype,
            NonArrayRealmIntrinsicSlot::WeakRefPrototype,
            NonArrayRealmIntrinsicSlot::FinalizationRegistryPrototype,
            NonArrayRealmIntrinsicSlot::WeakSetPrototype,
            NonArrayRealmIntrinsicSlot::AsyncDisposableStackPrototype,
            NonArrayRealmIntrinsicSlot::DisposableStackPrototype,
            NonArrayRealmIntrinsicSlot::SetPrototype,
            NonArrayRealmIntrinsicSlot::SymbolPrototype,
            NonArrayRealmIntrinsicSlot::BigIntPrototype,
            NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
            NonArrayRealmIntrinsicSlot::SharedArrayBufferPrototype,
            NonArrayRealmIntrinsicSlot::DataViewPrototype,
            NonArrayRealmIntrinsicSlot::TypedArrayPrototype,
            NonArrayRealmIntrinsicSlot::RegExpPrototype,
            NonArrayRealmIntrinsicSlot::IntlLocalePrototype,
            NonArrayRealmIntrinsicSlot::IntlDateTimeFormatPrototype,
            NonArrayRealmIntrinsicSlot::IntlNumberFormatPrototype,
            NonArrayRealmIntrinsicSlot::IntlPluralRulesPrototype,
            NonArrayRealmIntrinsicSlot::IntlListFormatPrototype,
            NonArrayRealmIntrinsicSlot::IntlCollatorPrototype,
            NonArrayRealmIntrinsicSlot::IntlDisplayNamesPrototype,
            NonArrayRealmIntrinsicSlot::IntlRelativeTimeFormatPrototype,
            NonArrayRealmIntrinsicSlot::IntlSegmenterPrototype,
            NonArrayRealmIntrinsicSlot::IntlDurationFormatPrototype,
            NonArrayRealmIntrinsicSlot::IntlSegmentsPrototype,
        ] {
            self.emit_bootstrap_plain_prototype(&context, slot, &object_prototype, function)?;
        }
        for family in crate::intrinsics::temporal::TemporalIntrinsicFamily::ALL {
            self.emit_bootstrap_plain_prototype(
                &context,
                family.prototype_slot(),
                &object_prototype,
                function,
            )?;
        }
        let primitive = schema.reserve_value_local(function);
        primitive.set_scalar(ScalarValue::NumberBits(0), function);
        self.emit_bootstrap_boxed_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::NumberPrototype,
            &object_prototype,
            &primitive,
            function,
        )?;
        let empty_string = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(self.emit_interned_string_reference("", function)?, function);
        primitive.set_reference(&empty_string, schema, function);
        self.emit_bootstrap_boxed_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::StringPrototype,
            &object_prototype,
            &primitive,
            function,
        )?;
        empty_string.clear(function);
        primitive.set_scalar(ScalarValue::Boolean(false), function);
        self.emit_bootstrap_boxed_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::BooleanPrototype,
            &object_prototype,
            &primitive,
            function,
        )?;
        primitive.clear(function);
        self.emit_bootstrap_plain_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::DatePrototype,
            &object_prototype,
            function,
        )?;
        for (slot, name) in [
            (NonArrayRealmIntrinsicSlot::ErrorPrototype, "Error"),
            (NonArrayRealmIntrinsicSlot::TypeErrorPrototype, "TypeError"),
            (
                NonArrayRealmIntrinsicSlot::ReferenceErrorPrototype,
                "ReferenceError",
            ),
            (NonArrayRealmIntrinsicSlot::EvalErrorPrototype, "EvalError"),
            (
                NonArrayRealmIntrinsicSlot::AggregateErrorPrototype,
                "AggregateError",
            ),
            (
                NonArrayRealmIntrinsicSlot::SuppressedErrorPrototype,
                "SuppressedError",
            ),
            (
                NonArrayRealmIntrinsicSlot::RangeErrorPrototype,
                "RangeError",
            ),
            (
                NonArrayRealmIntrinsicSlot::SyntaxErrorPrototype,
                "SyntaxError",
            ),
            (NonArrayRealmIntrinsicSlot::URIErrorPrototype, "URIError"),
        ] {
            if slot.gc_index() == NonArrayRealmIntrinsicSlot::ErrorPrototype.gc_index() {
                parent.copy_from(&object_prototype, function);
            } else {
                self.emit_load_non_array_realm_intrinsic(
                    realm,
                    NonArrayRealmIntrinsicSlot::ErrorPrototype,
                    &parent,
                    function,
                );
            }
            // Error prototype objects have no [[ErrorData]] marker.
            let prototype = self.emit_bootstrap_namespace(&parent, function)?;
            self.emit_install_intrinsic_string(
                &prototype,
                IntrinsicKey::Name("name"),
                name,
                true,
                false,
                true,
                function,
            )?;
            self.emit_install_intrinsic_string(
                &prototype,
                IntrinsicKey::Name("message"),
                "",
                true,
                false,
                true,
                function,
            )?;
            self.emit_store_non_array_realm_intrinsic(realm, slot, &prototype, function);
            prototype.clear(function);
        }
        self.emit_bootstrap_constructor_from_slot(
            StandardBuiltinId::FunctionConstructor,
            &context,
            NonArrayRealmIntrinsicSlot::FunctionPrototype,
            function,
        )?;
        self.init_throw_type_error_intrinsic(&context, function)?;
        self.emit_bootstrap_dynamic_function_intrinsics(&context, function)?;
        self.emit_bootstrap_constructor_from_slot(
            StandardBuiltinId::ObjectConstructor,
            &context,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            function,
        )?;
        if self
            .runtime_bootstrap_plan
            .should_initialize_standard_builtin(StandardBuiltinId::AbstractModuleSourceConstructor)
        {
            self.emit_initialize_abstract_module_source_intrinsic(
                &context,
                &object_prototype,
                function,
            )?;
        }
        if self
            .runtime_bootstrap_plan
            .should_initialize_standard_builtin(StandardBuiltinId::ShadowRealmConstructor)
        {
            self.emit_initialize_shadow_realm_intrinsic(&context, &object_prototype, function)?;
        }
        for (builtin, slot) in [
            (
                StandardBuiltinId::ProxyConstructor,
                NonArrayRealmIntrinsicSlot::ObjectPrototype,
            ),
            (
                StandardBuiltinId::IteratorConstructor,
                NonArrayRealmIntrinsicSlot::IteratorPrototype,
            ),
        ] {
            if self
                .runtime_bootstrap_plan
                .should_initialize_standard_builtin(builtin)
            {
                self.emit_bootstrap_constructor_from_slot(builtin, &context, slot, function)?;
            }
        }
        if self
            .runtime_bootstrap_plan
            .should_initialize_standard_builtin(StandardBuiltinId::ArrayConstructor)
        {
            let array = schema
                .reserve_gc_local::<ArrayObject, NonNullable>(function)
                .initialize(
                    self.emit_load_realm_array_prototype(realm, function),
                    function,
                );
            let prototype = schema.reserve_value_local(function);
            prototype.set_reference(&array, schema, function);
            let constructor = self.init_builtin_constructor_object(
                StandardBuiltinId::ArrayConstructor,
                &context,
                &prototype,
                function,
            )?;
            constructor.clear(function);
            prototype.clear(function);
            array.clear(function);
        }
        self.emit_bootstrap_iterator_prototypes(&context, function)?;
        for (builtin, slot) in [
            (
                StandardBuiltinId::ArrayBufferConstructor,
                NonArrayRealmIntrinsicSlot::ArrayBufferPrototype,
            ),
            (
                StandardBuiltinId::SharedArrayBufferConstructor,
                NonArrayRealmIntrinsicSlot::SharedArrayBufferPrototype,
            ),
            (
                StandardBuiltinId::DataViewConstructor,
                NonArrayRealmIntrinsicSlot::DataViewPrototype,
            ),
            (
                StandardBuiltinId::DateConstructor,
                NonArrayRealmIntrinsicSlot::DatePrototype,
            ),
            (
                StandardBuiltinId::TemporalInstantConstructor,
                NonArrayRealmIntrinsicSlot::TemporalInstantPrototype,
            ),
            (
                StandardBuiltinId::TemporalPlainDateConstructor,
                NonArrayRealmIntrinsicSlot::TemporalPlainDatePrototype,
            ),
            (
                StandardBuiltinId::TemporalDurationConstructor,
                NonArrayRealmIntrinsicSlot::TemporalDurationPrototype,
            ),
            (
                StandardBuiltinId::TemporalPlainTimeConstructor,
                NonArrayRealmIntrinsicSlot::TemporalPlainTimePrototype,
            ),
            (
                StandardBuiltinId::TemporalPlainDateTimeConstructor,
                NonArrayRealmIntrinsicSlot::TemporalPlainDateTimePrototype,
            ),
            (
                StandardBuiltinId::TemporalPlainYearMonthConstructor,
                NonArrayRealmIntrinsicSlot::TemporalPlainYearMonthPrototype,
            ),
            (
                StandardBuiltinId::TemporalPlainMonthDayConstructor,
                NonArrayRealmIntrinsicSlot::TemporalPlainMonthDayPrototype,
            ),
            (
                StandardBuiltinId::TemporalZonedDateTimeConstructor,
                NonArrayRealmIntrinsicSlot::TemporalZonedDateTimePrototype,
            ),
            (
                StandardBuiltinId::IntlLocaleConstructor,
                NonArrayRealmIntrinsicSlot::IntlLocalePrototype,
            ),
            (
                StandardBuiltinId::IntlDateTimeFormatConstructor,
                NonArrayRealmIntrinsicSlot::IntlDateTimeFormatPrototype,
            ),
            (
                StandardBuiltinId::IntlNumberFormatConstructor,
                NonArrayRealmIntrinsicSlot::IntlNumberFormatPrototype,
            ),
            (
                StandardBuiltinId::IntlPluralRulesConstructor,
                NonArrayRealmIntrinsicSlot::IntlPluralRulesPrototype,
            ),
            (
                StandardBuiltinId::IntlListFormatConstructor,
                NonArrayRealmIntrinsicSlot::IntlListFormatPrototype,
            ),
            (
                StandardBuiltinId::IntlCollatorConstructor,
                NonArrayRealmIntrinsicSlot::IntlCollatorPrototype,
            ),
            (
                StandardBuiltinId::IntlDisplayNamesConstructor,
                NonArrayRealmIntrinsicSlot::IntlDisplayNamesPrototype,
            ),
            (
                StandardBuiltinId::IntlRelativeTimeFormatConstructor,
                NonArrayRealmIntrinsicSlot::IntlRelativeTimeFormatPrototype,
            ),
            (
                StandardBuiltinId::IntlSegmenterConstructor,
                NonArrayRealmIntrinsicSlot::IntlSegmenterPrototype,
            ),
            (
                StandardBuiltinId::IntlDurationFormatConstructor,
                NonArrayRealmIntrinsicSlot::IntlDurationFormatPrototype,
            ),
            (
                StandardBuiltinId::RegExpConstructor,
                NonArrayRealmIntrinsicSlot::RegExpPrototype,
            ),
        ] {
            if self
                .runtime_bootstrap_plan
                .should_initialize_standard_builtin(builtin)
            {
                self.emit_bootstrap_constructor_from_slot(builtin, &context, slot, function)?;
            }
        }
        if self.runtime_bootstrap_plan.needs_typed_array_intrinsic() {
            self.init_typed_array_intrinsic(&context, function)?;
        }
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::TypedArrayPrototype,
            &parent,
            function,
        );
        for kind in TypedArrayElementKind::ALL {
            let builtin = kind.constructor();
            if self
                .runtime_bootstrap_plan
                .should_initialize_standard_builtin(builtin)
            {
                let slot = NonArrayRealmIntrinsicSlot::prototype_identity(kind);
                self.emit_bootstrap_plain_prototype(&context, slot, &parent, function)?;
                self.emit_bootstrap_constructor_from_slot(builtin, &context, slot, function)?;
            }
        }
        for (builtin, slot) in [
            (
                StandardBuiltinId::NumberConstructor,
                NonArrayRealmIntrinsicSlot::NumberPrototype,
            ),
            (
                StandardBuiltinId::StringConstructor,
                NonArrayRealmIntrinsicSlot::StringPrototype,
            ),
            (
                StandardBuiltinId::BooleanConstructor,
                NonArrayRealmIntrinsicSlot::BooleanPrototype,
            ),
            (
                StandardBuiltinId::PromiseConstructor,
                NonArrayRealmIntrinsicSlot::PromisePrototype,
            ),
            (
                StandardBuiltinId::MapConstructor,
                NonArrayRealmIntrinsicSlot::MapPrototype,
            ),
            (
                StandardBuiltinId::WeakMapConstructor,
                NonArrayRealmIntrinsicSlot::WeakMapPrototype,
            ),
            (
                StandardBuiltinId::WeakRefConstructor,
                NonArrayRealmIntrinsicSlot::WeakRefPrototype,
            ),
            (
                StandardBuiltinId::FinalizationRegistryConstructor,
                NonArrayRealmIntrinsicSlot::FinalizationRegistryPrototype,
            ),
            (
                StandardBuiltinId::WeakSetConstructor,
                NonArrayRealmIntrinsicSlot::WeakSetPrototype,
            ),
            (
                StandardBuiltinId::AsyncDisposableStackConstructor,
                NonArrayRealmIntrinsicSlot::AsyncDisposableStackPrototype,
            ),
            (
                StandardBuiltinId::DisposableStackConstructor,
                NonArrayRealmIntrinsicSlot::DisposableStackPrototype,
            ),
            (
                StandardBuiltinId::SetConstructor,
                NonArrayRealmIntrinsicSlot::SetPrototype,
            ),
            (
                StandardBuiltinId::SymbolConstructor,
                NonArrayRealmIntrinsicSlot::SymbolPrototype,
            ),
            (
                StandardBuiltinId::BigIntConstructor,
                NonArrayRealmIntrinsicSlot::BigIntPrototype,
            ),
        ] {
            if self
                .runtime_bootstrap_plan
                .should_initialize_standard_builtin(builtin)
            {
                self.emit_bootstrap_constructor_from_slot(builtin, &context, slot, function)?;
                if builtin == StandardBuiltinId::StringConstructor {
                    let constructor = schema.reserve_value_local(function);
                    self.emit_load_non_array_realm_intrinsic(
                        realm,
                        NonArrayRealmIntrinsicSlot::StringConstructor,
                        &constructor,
                        function,
                    );
                    for (name, method) in [
                        ("fromCharCode", StandardBuiltinId::StringFromCharCode),
                        ("fromCodePoint", StandardBuiltinId::StringFromCodePoint),
                        ("raw", StandardBuiltinId::StringRaw),
                    ] {
                        self.emit_install_intrinsic_method(
                            &constructor,
                            IntrinsicKey::Name(name),
                            method,
                            &context,
                            true,
                            true,
                            function,
                        )?;
                    }
                    constructor.clear(function);
                }
            }
        }
        for (builtin, slot) in [
            (
                StandardBuiltinId::ErrorConstructor,
                NonArrayRealmIntrinsicSlot::ErrorPrototype,
            ),
            (
                StandardBuiltinId::EvalErrorConstructor,
                NonArrayRealmIntrinsicSlot::EvalErrorPrototype,
            ),
            (
                StandardBuiltinId::AggregateErrorConstructor,
                NonArrayRealmIntrinsicSlot::AggregateErrorPrototype,
            ),
            (
                StandardBuiltinId::SuppressedErrorConstructor,
                NonArrayRealmIntrinsicSlot::SuppressedErrorPrototype,
            ),
            (
                StandardBuiltinId::RangeErrorConstructor,
                NonArrayRealmIntrinsicSlot::RangeErrorPrototype,
            ),
            (
                StandardBuiltinId::SyntaxErrorConstructor,
                NonArrayRealmIntrinsicSlot::SyntaxErrorPrototype,
            ),
            (
                StandardBuiltinId::TypeErrorConstructor,
                NonArrayRealmIntrinsicSlot::TypeErrorPrototype,
            ),
            (
                StandardBuiltinId::URIErrorConstructor,
                NonArrayRealmIntrinsicSlot::URIErrorPrototype,
            ),
            (
                StandardBuiltinId::ReferenceErrorConstructor,
                NonArrayRealmIntrinsicSlot::ReferenceErrorPrototype,
            ),
        ] {
            self.emit_bootstrap_constructor_from_slot(builtin, &context, slot, function)?;
        }
        parent.clear(function);
        function_prototype.clear(function);
        let plan = self.runtime_bootstrap_plan.clone();
        let reflect = (plan.full_standard_globals || plan.reflect_object)
            .then(|| self.init_reflect_object(&context, &object_prototype, function))
            .transpose()?;
        let math = (plan.full_standard_globals || plan.math_object)
            .then(|| self.init_math_object(&context, &object_prototype, function))
            .transpose()?;
        let json = (plan.full_standard_globals || plan.json_object)
            .then(|| self.init_json_object(&context, &object_prototype, function))
            .transpose()?;
        let atomics = (plan.full_standard_globals || plan.atomics_object)
            .then(|| self.init_atomics_object(&context, &object_prototype, function))
            .transpose()?;
        let temporal = plan
            .temporal_namespace_members()
            .map(|members| {
                self.init_temporal_object(members, &context, &object_prototype, function)
            })
            .transpose()?;
        let intl = plan
            .intl_namespace_members()
            .map(|members| self.init_intl_object(members, &context, &object_prototype, function))
            .transpose()?;
        Ok(BootstrapRealm {
            realm: context,
            object_prototype,
            reflect,
            math,
            json,
            atomics,
            temporal,
            intl,
        })
    }

    pub(crate) fn init_runtime_roots(&mut self, function: &mut Function) -> Result<(), EmitError> {
        if !self.is_main() {
            return Ok(());
        }
        if self.bootstrap_realm.is_some() {
            return Err(EmitError::unsupported(
                "Realm bootstrap products already exist",
            ));
        }
        let realm = self.load_current_realm(function);
        let bootstrap = self.emit_initialize_realm_intrinsics(&realm, function)?;
        realm.clear(function);
        self.bootstrap_realm = Some(bootstrap);
        Ok(())
    }

    pub(crate) fn init_script_global_object(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !self.is_main() {
            return Ok(());
        }
        let bootstrap = self.bootstrap_realm.take().ok_or_else(|| {
            EmitError::unsupported("global publication requires completed Realm bootstrap")
        })?;
        let bindings = self
            .script_global_bindings
            .expect("main owns its global plan")
            .iter()
            .filter(|binding| {
                self.runtime_bootstrap_plan
                    .should_install_script_global_binding(&binding.initializer)
            })
            .cloned()
            .collect();
        let object = self.emit_bootstrap_global_object(bootstrap, bindings, function)?;
        if let Some(slot) = self.owned_env_slot(LEXICAL_THIS_NAME) {
            let value = self.runtime_schema().reserve_value_local(function);
            value.set_reference(&object, self.runtime_schema(), function);
            self.write_env_slot_from_locals(slot, 0, &value, function);
            value.clear(function);
        }
        object.clear(function);
        Ok(())
    }

    /// Creates only this Realm's canonical globals. Entry declarations and
    /// entry-only host extensions never become properties of the new global.
    pub(crate) fn emit_created_realm_global_object(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let environment = self.emit_alloc_realm_global_environment(realm, function)?;
        environment.clear(function);
        let bootstrap = self.emit_initialize_realm_intrinsics(realm, function)?;
        let bindings = created_realm_global_bindings();
        self.emit_bootstrap_global_object(bootstrap, bindings, function)
    }

    fn emit_bootstrap_global_object(
        &mut self,
        bootstrap: BootstrapRealm,
        bindings: Vec<scriptw_ir::ScriptGlobalBindingIr>,
        function: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        let object = self.emit_bootstrap_namespace(&bootstrap.object_prototype, function)?;
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&object, function),
                function,
            );
        schema
            .struct_type::<RealmRecord>()
            .field(RealmRecordSchema::GLOBAL_OBJECT)
            .write(
                bootstrap.realm.realm(),
                GcOperand::nullable_reference(&stored, schema),
                schema,
                function,
            );
        schema
            .struct_type::<RealmRecord>()
            .field(RealmRecordSchema::GLOBAL_THIS)
            .write(
                bootstrap.realm.realm(),
                GcOperand::nullable_reference(&stored, schema),
                schema,
                function,
            );
        stored.clear(function);
        let eval_meta = self
            .functions
            .get(&StandardBuiltinId::EvalFunction.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported("every Realm requires its original eval entry")
            })?;
        let eval = schema
            .reserve_gc_local::<FunctionObject, NonNullable>(function)
            .initialize(
                self.emit_function_value_payload_in_realm(&eval_meta, &bootstrap.realm, function)?,
                function,
            );
        self.emit_initialize_realm_eval_intrinsic(bootstrap.realm.realm(), &eval, function);
        eval.clear(function);
        let value = schema.reserve_value_local(function);
        for binding in bindings {
            match &binding.initializer {
                GlobalPropertyInitializerIr::Intrinsic => value.copy_from(&object, function),
                GlobalPropertyInitializerIr::Infinity => value.set_scalar(
                    ScalarValue::NumberBits(f64::INFINITY.to_bits() as i64),
                    function,
                ),
                GlobalPropertyInitializerIr::NaN => {
                    value.set_scalar(ScalarValue::NumberBits(f64::NAN.to_bits() as i64), function)
                }
                GlobalPropertyInitializerIr::Undefined
                | GlobalPropertyInitializerIr::FreshUndefined => value.set_undefined(function),
                GlobalPropertyInitializerIr::SourceFunction(id) => {
                    let meta = self.functions.get(id).cloned().ok_or_else(|| {
                        EmitError::unsupported(format!("missing global source function {id}"))
                    })?;
                    let callable = schema
                        .reserve_gc_local::<FunctionObject, NonNullable>(function)
                        .initialize(self.emit_function_value_payload(&meta, function)?, function);
                    value.set_reference(&callable, schema, function);
                    callable.clear(function);
                }
                GlobalPropertyInitializerIr::ReflectObject => value.copy_from(
                    bootstrap.reflect.as_ref().ok_or_else(|| {
                        EmitError::unsupported("planned reflect namespace was not constructed")
                    })?,
                    function,
                ),
                GlobalPropertyInitializerIr::MathObject => value.copy_from(
                    bootstrap.math.as_ref().ok_or_else(|| {
                        EmitError::unsupported("planned math namespace was not constructed")
                    })?,
                    function,
                ),
                GlobalPropertyInitializerIr::JsonObject => value.copy_from(
                    bootstrap.json.as_ref().ok_or_else(|| {
                        EmitError::unsupported("planned json namespace was not constructed")
                    })?,
                    function,
                ),
                GlobalPropertyInitializerIr::AtomicsObject => value.copy_from(
                    bootstrap.atomics.as_ref().ok_or_else(|| {
                        EmitError::unsupported("planned atomics namespace was not constructed")
                    })?,
                    function,
                ),
                GlobalPropertyInitializerIr::TemporalObject => value.copy_from(
                    bootstrap.temporal.as_ref().ok_or_else(|| {
                        EmitError::unsupported("planned temporal namespace was not constructed")
                    })?,
                    function,
                ),
                GlobalPropertyInitializerIr::IntlObject => value.copy_from(
                    bootstrap.intl.as_ref().ok_or_else(|| {
                        EmitError::unsupported("planned intl namespace was not constructed")
                    })?,
                    function,
                ),
                GlobalPropertyInitializerIr::BuiltinFunction(builtin) => {
                    if *builtin == StandardBuiltinId::EvalFunction {
                        self.emit_load_realm_eval_intrinsic_to_local(
                            bootstrap.realm.realm(),
                            &value,
                            function,
                        );
                    } else if let Some(slot) = standard_builtin_constructor_realm_slot(*builtin) {
                        self.emit_load_non_array_realm_intrinsic(
                            bootstrap.realm.realm(),
                            slot,
                            &value,
                            function,
                        );
                    } else {
                        let callable =
                            self.emit_intrinsic_callable(*builtin, &bootstrap.realm, function)?;
                        value.set_reference(&callable, schema, function);
                        callable.clear(function);
                    }
                }
                GlobalPropertyInitializerIr::HostFunction(builtin) => {
                    if canonical_host_function_realm_slot(*builtin).is_some() {
                        let callable = self.emit_intrinsic_canonical_host_callable(
                            *builtin,
                            &bootstrap.realm,
                            function,
                        )?;
                        value.set_reference(&callable, schema, function);
                        callable.clear(function);
                    } else {
                        let meta = self
                            .functions
                            .get(&builtin.function_id())
                            .cloned()
                            .ok_or_else(|| {
                                EmitError::unsupported("missing global host function entry")
                            })?;
                        let callable = schema
                            .reserve_gc_local::<FunctionObject, NonNullable>(function)
                            .initialize(
                                self.emit_function_value_payload_in_realm(
                                    &meta,
                                    &bootstrap.realm,
                                    function,
                                )?,
                                function,
                            );
                        value.set_reference(&callable, schema, function);
                        callable.clear(function);
                    }
                }
            }
            self.emit_install_intrinsic_data(
                &object,
                IntrinsicKey::Name(&binding.name),
                &value,
                binding.initializer.writable(),
                binding.initializer.enumerable(),
                binding.initializer.configurable(),
                function,
            )?;
        }
        let global = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(&object, function),
            function,
        );
        value.clear(function);
        object.clear(function);
        bootstrap.clear(self, function);
        Ok(global)
    }
}
