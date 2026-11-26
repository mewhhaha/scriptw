use super::super::*;
use crate::functions::{NonArrayRealmIntrinsicSlot, RealmFunctionMaterializationContext};
use crate::gc_types::*;
use crate::intrinsics::IntrinsicKey;
use crate::intrinsics::IntrinsicPropertyLayoutId;
use crate::objects::{AccessorDescriptor, AccessorGetter, FreshDataAttributes};
use scriptw_ir::StandardBuiltinInstaller;

mod error_initialization;
pub(crate) use error_initialization::ErrorIntrinsicBuild;
mod date_initialization;
pub(crate) use date_initialization::DateIntrinsicBuild;
mod regexp_initialization;
pub(crate) use regexp_initialization::RegExpIntrinsicBuild;
mod binary_data_initialization;
pub(crate) use binary_data_initialization::BinaryDataIntrinsicBuild;
mod demand_constructors;
mod global_publication;
mod intl_initialization;
use global_publication::GlobalBindingPublication;
mod realm_initialization;
pub(crate) use intl_initialization::IntlIntrinsicBuild;
mod temporal_initialization;
pub(crate) use temporal_initialization::TemporalIntrinsicBuild;

/// Bootstrap holds only actual rooted products until the global object owns
/// them. It is consumed once; no namespace or Realm address is exported.
#[must_use]
pub(crate) struct BootstrapRealm {
    realm: RealmFunctionMaterializationContext,
    object_prototype: ValueLocals,
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
        let schema = self.runtime_schema();
        let parent_slot = if matches!(
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
        let parent = parent_slot.map(|slot| {
            let value = schema.reserve_value_local(function);
            self.emit_load_non_array_realm_intrinsic(realm.realm(), slot, &value, function);
            value
        });
        let constructor = self.init_builtin_constructor_object_with_parent(
            builtin,
            realm,
            prototype,
            parent.as_ref(),
            function,
        )?;
        if let Some(parent) = parent {
            parent.clear(function);
        }
        Ok(constructor)
    }

    // Lazy-family builders pass completed parents explicitly. An ordinary
    // canonical parent read here would reenter their Building state.
    fn init_builtin_constructor_object_with_parent(
        &mut self,
        builtin: StandardBuiltinId,
        realm: &RealmFunctionMaterializationContext,
        prototype: &ValueLocals,
        parent: Option<&ValueLocals>,
        function: &mut Function,
    ) -> Result<GcLocal<FunctionObject>, EmitError> {
        if builtin == StandardBuiltinId::RegExpConstructor {
            return Err(EmitError::unsupported(
                "RegExp family installation requires its build capability",
            ));
        }
        let constructor = self.emit_builtin_constructor_shell_with_parent(
            builtin, realm, prototype, parent, function,
        )?;
        let schema = self.runtime_schema();
        let constructor_value = schema.reserve_value_local(function);
        constructor_value.set_reference(&constructor, schema, function);
        let intrinsic_context = IntrinsicInstall {
            builtin,
            realm,
            constructor: &constructor_value,
            prototype,
        };
        match builtin.intrinsic_installer() {
            StandardBuiltinInstaller::None => {}
            StandardBuiltinInstaller::ShadowRealm
            | StandardBuiltinInstaller::Function
            | StandardBuiltinInstaller::Promise
            | StandardBuiltinInstaller::Map
            | StandardBuiltinInstaller::WeakMap
            | StandardBuiltinInstaller::WeakSet
            | StandardBuiltinInstaller::WeakRef
            | StandardBuiltinInstaller::FinalizationRegistry
            | StandardBuiltinInstaller::AsyncDisposableStack
            | StandardBuiltinInstaller::DisposableStack
            | StandardBuiltinInstaller::TextEncoder
            | StandardBuiltinInstaller::TextDecoder
            | StandardBuiltinInstaller::Set
            | StandardBuiltinInstaller::Object
            | StandardBuiltinInstaller::Proxy
            | StandardBuiltinInstaller::Iterator
            | StandardBuiltinInstaller::Array
            | StandardBuiltinInstaller::String
            | StandardBuiltinInstaller::BigInt
            | StandardBuiltinInstaller::Symbol
            | StandardBuiltinInstaller::Number
            | StandardBuiltinInstaller::Boolean => {}
            StandardBuiltinInstaller::RegExp => {
                return Err(EmitError::unsupported(
                    "RegExp family installation requires its build capability",
                ));
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
        }

        constructor_value.clear(function);
        Ok(constructor)
    }

    // Build and root the canonical constructor/prototype cycle without invoking
    // a family installer. Lazy families supply their own installation capability.
    fn emit_builtin_constructor_shell_with_parent(
        &mut self,
        builtin: StandardBuiltinId,
        realm: &RealmFunctionMaterializationContext,
        prototype: &ValueLocals,
        parent: Option<&ValueLocals>,
        function: &mut Function,
    ) -> Result<GcLocal<FunctionObject>, EmitError> {
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
        let covered = IntrinsicPropertyLayoutId::for_constructor(&meta)?.is_some();
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
        if let Some(parent) = parent {
            let stored = schema
                .reserve_gc_local(function)
                .initialize(schema.prototype_reference(parent, function), function);
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
        }
        if builtin != StandardBuiltinId::ProxyConstructor {
            if !covered {
                self.emit_install_intrinsic_data(
                    &constructor_value,
                    IntrinsicKey::Name("prototype"),
                    prototype,
                    false,
                    false,
                    false,
                    function,
                )?;
            }
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
            if builtin != StandardBuiltinId::IteratorConstructor && !covered {
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
        let thrower =
            self.emit_intrinsic_callable(StandardBuiltinId::ThrowTypeError, realm, function)?;
        thrower.clear(function);
        Ok(())
    }

    fn emit_bootstrap_intrinsic_namespace(
        &mut self,
        layout: IntrinsicPropertyLayoutId,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        let schema = self.runtime_schema();
        let object = schema
            .reserve_gc_local::<OrdinaryObject, NonNullable>(function)
            .initialize(
                self.emit_alloc_intrinsic_layout_header(
                    object_prototype,
                    layout,
                    realm.realm(),
                    function,
                )?,
                function,
            );
        let value = schema.reserve_value_local(function);
        value.set_reference(&object, schema, function);
        object.clear(function);
        Ok(value)
    }

    pub(crate) fn init_reflect_object(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        self.emit_bootstrap_intrinsic_namespace(
            IntrinsicPropertyLayoutId::Reflect,
            realm,
            object_prototype,
            function,
        )
    }

    pub(crate) fn init_math_object(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        self.emit_bootstrap_intrinsic_namespace(
            IntrinsicPropertyLayoutId::Math,
            realm,
            object_prototype,
            function,
        )
    }

    pub(crate) fn init_json_object(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        self.emit_bootstrap_intrinsic_namespace(
            IntrinsicPropertyLayoutId::Json,
            realm,
            object_prototype,
            function,
        )
    }

    pub(crate) fn init_atomics_object(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<ValueLocals, EmitError> {
        self.emit_bootstrap_intrinsic_namespace(
            IntrinsicPropertyLayoutId::Atomics,
            realm,
            object_prototype,
            function,
        )
    }

    pub(super) fn emit_bootstrap_dynamic_function_intrinsic(
        &mut self,
        builtin: HostBuiltinId,
        prototype_slot: NonArrayRealmIntrinsicSlot,
        constructor_slot: NonArrayRealmIntrinsicSlot,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let meta = self
            .functions
            .get(&builtin.function_id())
            .cloned()
            .ok_or_else(|| {
                EmitError::unsupported(format!("missing dynamic intrinsic {}", builtin.as_str()))
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
        constructor_value.clear(function);
        constructor.clear(function);
        prototype.clear(function);
        Ok(())
    }

    pub(super) fn emit_bootstrap_plain_prototype(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        slot: NonArrayRealmIntrinsicSlot,
        parent: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let layout = match IntrinsicPropertyLayoutId::for_prototype(slot) {
            Some(layout)
                if self.runtime_bootstrap_plan.full_standard_globals
                    || self.strings.has_intrinsic_property_layout(layout)? =>
            {
                Some(layout)
            }
            _ => None,
        };
        let value = if let Some(layout) = layout {
            self.emit_bootstrap_intrinsic_namespace(layout, realm, parent, function)?
        } else {
            self.emit_bootstrap_namespace(parent, function)?
        };
        self.emit_store_non_array_realm_intrinsic(realm.realm(), slot, &value, function);
        value.clear(function);
        Ok(())
    }

    pub(super) fn emit_bootstrap_boxed_prototype(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        slot: NonArrayRealmIntrinsicSlot,
        parent: &ValueLocals,
        primitive: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let layout = IntrinsicPropertyLayoutId::for_prototype(slot);
        let layout = match layout {
            Some(layout)
                if self.runtime_bootstrap_plan.full_standard_globals
                    || self.strings.has_intrinsic_property_layout(layout)? =>
            {
                Some(layout)
            }
            _ => None,
        };
        let header_value = if let Some(layout) = layout {
            self.emit_alloc_intrinsic_layout_header(parent, layout, realm.realm(), function)?
        } else {
            self.emit_alloc_plain_object_with_prototype(Some(parent), function)?
        };
        let header = schema
            .reserve_gc_local::<OrdinaryObject, NonNullable>(function)
            .initialize(header_value, function);
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
        // Narrow pools retain String's standalone virtual length.
        if slot.gc_index() == NonArrayRealmIntrinsicSlot::StringPrototype.gc_index()
            && layout.is_none()
        {
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

    pub(super) fn emit_bootstrap_constructor_from_slot(
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
                self.emit_alloc_immutable_prototype_object(realm, function)?,
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
        let reserved = self.reserve_realm_array_prototype_local(function);
        let array = self.emit_initialize_realm_array_prototype(
            reserved,
            &object_prototype,
            realm,
            function,
        )?;
        self.emit_store_realm_array_prototype(realm, array.array(), function);
        self.release_realm_array_prototype_local(array, function);
        // Demand prototypes (Iterator*, Generator*, Async*, Promise, Map, Set,
        // Weak*, Disposable*, Abort*, Event*, Text*, Symbol, BigInt, Number,
        // Boolean, ShadowRealm, AbstractModuleSource, ModuleSource,
        // ReadableStream*), dynamic function constructors, DemandConstructors
        // (including Object, Function, String), parseInt/parseFloat,
        // Reflect/Math/JSON/Atomics namespaces, Error/DOMException, Date,
        // RegExp, BinaryData, Temporal and Intl are built on their first
        // canonical read (see demand_constructors). %String.prototype% stays
        // eager because Realm protector arming records its storage at birth.
        let primitive = schema.reserve_value_local(function);
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
        primitive.clear(function);
        self.init_throw_type_error_intrinsic(&context, function)?;
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
            self.emit_initialize_array_canonical_callables(&context, function)?;
            constructor.clear(function);
            prototype.clear(function);
            array.clear(function);
        }
        self.emit_arm_realm_protectors(
            realm,
            &object_prototype,
            context.function_prototype(),
            function,
        )?;
        Ok(BootstrapRealm {
            realm: context,
            object_prototype,
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
        // A Realm's global object and `globalThis` are written here alone.
        schema.emit_advance_builtin_fact_epoch(function);
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
        // Every binding's value exists before the global object receives its
        // first property; computing one runs no JavaScript and reads no
        // global property, so publishing them together is unobservable.
        let mut publication = GlobalBindingPublication::new(
            self,
            bindings.iter().map(|binding| binding.name.as_str()),
            function,
        )?;
        let value = schema.reserve_value_local(function);
        for binding in bindings {
            // Deferred recipes keep the binding's original position and data
            // attributes; a read resolves them in their defining Realm.
            let deferred = match &binding.initializer {
                GlobalPropertyInitializerIr::Intrinsic => {
                    value.copy_from(&object, function);
                    None
                }
                GlobalPropertyInitializerIr::Infinity => {
                    value.set_scalar(
                        ScalarValue::NumberBits(f64::INFINITY.to_bits() as i64),
                        function,
                    );
                    None
                }
                GlobalPropertyInitializerIr::NaN => {
                    value.set_scalar(ScalarValue::NumberBits(f64::NAN.to_bits() as i64), function);
                    None
                }
                GlobalPropertyInitializerIr::Undefined
                | GlobalPropertyInitializerIr::FreshUndefined => {
                    value.set_undefined(function);
                    None
                }
                GlobalPropertyInitializerIr::SourceFunction(id) => {
                    let meta = self.functions.get(id).cloned().ok_or_else(|| {
                        EmitError::unsupported(format!("missing global source function {id}"))
                    })?;
                    let callable = schema
                        .reserve_gc_local::<FunctionObject, NonNullable>(function)
                        .initialize(self.emit_function_value_payload(&meta, function)?, function);
                    value.set_reference(&callable, schema, function);
                    callable.clear(function);
                    None
                }
                GlobalPropertyInitializerIr::ReflectObject => Some(
                    self.emit_deferred_demand_recipe_value(
                        crate::heap::DemandGlobalRecipe::Namespace(
                            crate::heap::DemandNamespace::Reflect,
                        ),
                        &bootstrap.realm,
                        function,
                    )?,
                ),
                GlobalPropertyInitializerIr::MathObject => Some(
                    self.emit_deferred_demand_recipe_value(
                        crate::heap::DemandGlobalRecipe::Namespace(
                            crate::heap::DemandNamespace::Math,
                        ),
                        &bootstrap.realm,
                        function,
                    )?,
                ),
                GlobalPropertyInitializerIr::JsonObject => Some(
                    self.emit_deferred_demand_recipe_value(
                        crate::heap::DemandGlobalRecipe::Namespace(
                            crate::heap::DemandNamespace::Json,
                        ),
                        &bootstrap.realm,
                        function,
                    )?,
                ),
                GlobalPropertyInitializerIr::AtomicsObject => Some(
                    self.emit_deferred_demand_recipe_value(
                        crate::heap::DemandGlobalRecipe::Namespace(
                            crate::heap::DemandNamespace::Atomics,
                        ),
                        &bootstrap.realm,
                        function,
                    )?,
                ),
                GlobalPropertyInitializerIr::TemporalObject => {
                    // Deleting or replacing the property cannot recreate it.
                    let recipe = schema.reserve_gc_local(function).initialize(
                        schema.struct_type::<DeferredTemporalNamespace>().construct(
                            (GcOperand::reference(bootstrap.realm.realm(), schema),),
                            function,
                        ),
                        function,
                    );
                    let deferred = schema
                        .reserve_gc_local(function)
                        .initialize(recipe.load(schema, function).upcast(), function);
                    recipe.clear(function);
                    Some(deferred)
                }
                GlobalPropertyInitializerIr::IntlObject => {
                    // Private first use cannot recreate a deleted or replaced
                    // public property.
                    let recipe = schema.reserve_gc_local(function).initialize(
                        schema.struct_type::<DeferredIntlNamespace>().construct(
                            (GcOperand::reference(bootstrap.realm.realm(), schema),),
                            function,
                        ),
                        function,
                    );
                    let deferred = schema
                        .reserve_gc_local(function)
                        .initialize(recipe.load(schema, function).upcast(), function);
                    recipe.clear(function);
                    Some(deferred)
                }
                GlobalPropertyInitializerIr::BuiltinFunction(builtin) => {
                    if *builtin == StandardBuiltinId::RegExpConstructor {
                        Some(
                            self.emit_deferred_regexp_constructor_value(
                                &bootstrap.realm,
                                function,
                            )?,
                        )
                    } else if *builtin == StandardBuiltinId::DateConstructor {
                        Some(self.emit_deferred_date_constructor_value(&bootstrap.realm, function)?)
                    } else if let Some(constructor) =
                        crate::heap::ErrorIntrinsicConstructor::from_builtin(*builtin)
                    {
                        Some(self.emit_deferred_error_constructor_value(
                            constructor,
                            &bootstrap.realm,
                            function,
                        )?)
                    } else if let Some(demand) =
                        crate::heap::DemandConstructor::from_builtin(*builtin)
                    {
                        Some(self.emit_deferred_demand_recipe_value(
                            crate::heap::DemandGlobalRecipe::Constructor(demand),
                            &bootstrap.realm,
                            function,
                        )?)
                    } else if let Some(constructor) =
                        crate::heap::BinaryDataConstructor::from_builtin(*builtin)
                    {
                        Some(self.emit_deferred_binary_data_constructor_value(
                            constructor,
                            &bootstrap.realm,
                            function,
                        )?)
                    } else if let Some(method) = match self.functions.get(&builtin.function_id()) {
                        Some(meta) => crate::intrinsics::DeferredIntrinsicMethod::validated_global(
                            *builtin, meta,
                        )?,
                        None => None,
                    } {
                        Some(self.emit_deferred_global_function_value(
                            method,
                            &bootstrap.realm,
                            function,
                        ))
                    } else {
                        if *builtin == StandardBuiltinId::EvalFunction {
                            self.emit_load_realm_eval_intrinsic_to_local(
                                bootstrap.realm.realm(),
                                &value,
                                function,
                            );
                        } else if let Some(slot) = standard_builtin_constructor_realm_slot(*builtin)
                        {
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
                        None
                    }
                }
                GlobalPropertyInitializerIr::HostFunction(builtin) => {
                    if let Some(demand) = crate::heap::DemandHostFunction::from_builtin(*builtin) {
                        Some(self.emit_deferred_demand_recipe_value(
                            crate::heap::DemandGlobalRecipe::HostFunction(demand),
                            &bootstrap.realm,
                            function,
                        )?)
                    } else if canonical_host_function_realm_slot(*builtin).is_some() {
                        let callable = self.emit_intrinsic_canonical_host_callable(
                            *builtin,
                            &bootstrap.realm,
                            function,
                        )?;
                        value.set_reference(&callable, schema, function);
                        callable.clear(function);
                        None
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
                        None
                    }
                }
            };
            let property_value = match deferred {
                Some(deferred) => deferred,
                None => schema.reserve_gc_local(function).initialize(
                    schema
                        .struct_type::<StoredValue>()
                        .from_value(&value, function)
                        .upcast(),
                    function,
                ),
            };
            publication.push(
                self,
                &binding.name,
                &property_value,
                FreshDataAttributes {
                    writable: binding.initializer.writable(),
                    enumerable: binding.initializer.enumerable(),
                    configurable: binding.initializer.configurable(),
                },
                function,
            )?;
            property_value.clear(function);
        }
        publication.publish(self, &object, function)?;
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
