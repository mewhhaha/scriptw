use super::super::*;
use crate::functions::{NonArrayRealmIntrinsicSlot, RealmFunctionMaterializationContext};
use crate::gc_types::*;
use crate::intrinsics::IntrinsicKey;
use crate::intrinsics::IntrinsicPropertyLayoutId;
use crate::objects::{AccessorDescriptor, AccessorGetter};
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
    ) -> Result<GcLocal<PrimitiveBox>, EmitError> {
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
        // Narrow pools retain String's standalone virtual length.
        if slot.gc_index() == NonArrayRealmIntrinsicSlot::StringPrototype.gc_index()
            && layout.is_none()
        {
            let value = schema.reserve_value_local(function);
            value.set_reference(&object, schema, function);
            self.emit_install_intrinsic_number(
                &value, "length", 0.0, false, false, false, function,
            )?;
            value.clear(function);
        }
        stored.clear(function);
        header.clear(function);
        Ok(object)
    }

    pub(super) fn emit_publish_bootstrap_boxed_prototype(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        slot: NonArrayRealmIntrinsicSlot,
        object: GcLocal<PrimitiveBox>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_reference(&object, schema, function);
        self.emit_store_non_array_realm_intrinsic(realm, slot, &value, function);
        value.clear(function);
        object.clear(function);
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
        // Demand prototypes (Iterator*, Generator*, Async*, Promise, Map, Set,
        // Weak*, Disposable*, Abort*, Event*, Text*, Symbol, BigInt, Number,
        // Boolean, ShadowRealm, AbstractModuleSource, ModuleSource,
        // ReadableStream*), dynamic function constructors, DemandConstructors
        // (including Array, Object, Function, String), canonical callables,
        // parseInt/parseFloat,
        // Reflect/Math/JSON/Atomics namespaces, Error/DOMException, Date,
        // RegExp, BinaryData, Temporal and Intl are built on their first
        // canonical read (see demand_constructors). %String.prototype% stays
        // eager because Realm protector arming records its storage at birth.
        let primitive = schema.reserve_value_local(function);
        let empty_string = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(self.emit_interned_string_reference("", function)?, function);
        primitive.set_reference(&empty_string, schema, function);
        let string = self.emit_bootstrap_boxed_prototype(
            &context,
            NonArrayRealmIntrinsicSlot::StringPrototype,
            &object_prototype,
            &primitive,
            function,
        )?;
        empty_string.clear(function);
        primitive.clear(function);
        self.emit_arm_realm_protectors(
            realm,
            &object,
            context.function_prototype(),
            array.array(),
            &string,
            function,
        )?;
        self.emit_publish_bootstrap_boxed_prototype(
            realm,
            NonArrayRealmIntrinsicSlot::StringPrototype,
            string,
            function,
        );
        object.clear(function);
        self.release_realm_array_prototype_local(array, function);
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
        let object = self.emit_bootstrap_global_object(bootstrap, true, function)?;
        self.emit_entry_global_declarations(&object, function)?;
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
        self.emit_bootstrap_global_object(bootstrap, false, function)
    }

    fn emit_bootstrap_global_object(
        &mut self,
        bootstrap: BootstrapRealm,
        entry: bool,
        function: &mut Function,
    ) -> Result<GcLocal<OrdinaryObject>, EmitError> {
        let schema = self.runtime_schema();
        let mut deleted = [0_u64; 2];
        if entry {
            let plan = self.script_global_bindings.expect("main owns its global plan");
            for (index, row) in StringPool::global_builtin_catalog(true).iter().enumerate() {
                let live = plan.get(&row.name).is_some_and(|binding| {
                    self.runtime_bootstrap_plan.should_install_script_global_binding(&binding.initializer)
                });
                if !live {
                    deleted[index / 64] |= 1 << (index % 64);
                }
            }
        }
        let span = self.strings.global_builtin_layout_span(entry)?;
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_global_builtin_object(
                &bootstrap.object_prototype, bootstrap.realm.realm(), span, deleted, function,
            ),
            function,
        );
        let value = schema.reserve_value_local(function);
        value.set_reference(&object, schema, function);
        let stored = schema
            .reserve_gc_local::<StoredValue, NonNullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&value, function),
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
        if entry {
            self.emit_entry_global_initializer_overrides(
                &object, &stored, &bootstrap.realm, function,
            )?;
        }
        stored.clear(function);
        value.clear(function);
        bootstrap.clear(self, function);
        Ok(object)
    }
}
