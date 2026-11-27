//! Immutable global row recipes and declaration publication.
use super::*;
use crate::heap::{DemandGlobalRecipe, DemandNamespace, StoredPropertyAttributes};
use crate::intrinsics::property_layouts::IntrinsicPropertyRecipeKind;
use crate::objects::PropertyKeyLocals;
use crate::runtime_helpers::RuntimeHelperPropertyLayoutRow;

// The low recipe kinds are owned by IntrinsicPropertyRecipeKind.
#[derive(Clone, Copy)]
#[repr(u32)]
enum GlobalBuiltinRecipe {
    This = 128,
    Undefined,
    Namespace,
    Temporal,
    Intl,
    StandardFunction,
    HostFunction,
}

impl FunctionBuilder<'_> {
    pub(crate) fn encode_global_builtin_recipe(
        initializer: &GlobalPropertyInitializerIr,
    ) -> Result<[u32; 3], EmitError> {
        use GlobalBuiltinRecipe as R;
        use GlobalPropertyInitializerIr as I;
        let (kind, payload) = match initializer {
            I::Infinity | I::NaN => {
                let bits = if matches!(initializer, I::Infinity) {
                    f64::INFINITY
                } else {
                    f64::NAN
                }
                .to_bits();
                return Ok([
                    IntrinsicPropertyRecipeKind::Number as u32,
                    bits as u32,
                    (bits >> 32) as u32,
                ]);
            }
            I::Intrinsic => (R::This, 0),
            I::Undefined => (R::Undefined, 0),
            I::ReflectObject => (R::Namespace, DemandNamespace::Reflect.word() as u32),
            I::MathObject => (R::Namespace, DemandNamespace::Math.word() as u32),
            I::JsonObject => (R::Namespace, DemandNamespace::Json.word() as u32),
            I::AtomicsObject => (R::Namespace, DemandNamespace::Atomics.word() as u32),
            I::TemporalObject => (R::Temporal, 0),
            I::IntlObject => (R::Intl, 0),
            I::BuiltinFunction(builtin) => (R::StandardFunction, *builtin as u32),
            I::HostFunction(builtin) => (R::HostFunction, *builtin as u32),
            I::FreshUndefined | I::SourceFunction(_) => {
                return Err(EmitError::unsupported(
                    "source declaration is not a global builtin recipe",
                ));
            }
        };
        Ok([kind as u32, payload, 0])
    }

    pub(crate) fn emit_global_builtin_layout_recipe_value(
        &mut self,
        row: &RuntimeHelperPropertyLayoutRow,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyValue>, EmitError> {
        use GlobalBuiltinRecipe as R;
        let schema = self.runtime_schema();
        let result = schema
            .reserve_gc_local::<PropertyValue, Nullable>(function)
            .initialize_null(schema, function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        for kind in [
            R::This,
            R::Undefined,
            R::Namespace,
            R::Temporal,
            R::Intl,
            R::StandardFunction,
            R::HostFunction,
        ] {
            row.kind().load(function);
            function.instruction(&Instruction::I32Const(kind as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            match kind {
                R::This => {
                    result.replace(
                        schema
                            .field(RealmRecordSchema::GLOBAL_THIS)
                            .read(realm, schema, function)
                            .reference()
                            .upcast(),
                        function,
                    );
                }
                R::Undefined => result.replace(
                    schema.stored_undefined(function).upcast().nullable(),
                    function,
                ),
                R::Namespace => {
                    for demand in DemandNamespace::ALL {
                        row.payload0().load(function);
                        function.instruction(&Instruction::I32Const(demand.word() as i32));
                        function.instruction(&Instruction::I32Eq);
                        self.open_frame(ControlFrameKind::If, function);
                        let recipe = schema.reserve_gc_local(function).initialize(
                            schema.struct_type::<DeferredDemandConstructor>().construct(
                                (
                                    GcOperand::reference(realm, schema),
                                    GcOperand::constant(DemandGlobalRecipe::Namespace(demand)),
                                ),
                                function,
                            ),
                            function,
                        );
                        result.replace(recipe.load(schema, function).upcast().nullable(), function);
                        recipe.clear(function);
                        self.emit_branch_to_target(done, function);
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                    }
                    function.instruction(&Instruction::Unreachable);
                }
                R::Temporal => {
                    let recipe = schema.reserve_gc_local(function).initialize(
                        schema
                            .struct_type::<DeferredTemporalNamespace>()
                            .construct((GcOperand::reference(realm, schema),), function),
                        function,
                    );
                    result.replace(recipe.load(schema, function).upcast().nullable(), function);
                    recipe.clear(function);
                }
                R::Intl => {
                    let recipe = schema.reserve_gc_local(function).initialize(
                        schema
                            .struct_type::<DeferredIntlNamespace>()
                            .construct((GcOperand::reference(realm, schema),), function),
                        function,
                    );
                    result.replace(recipe.load(schema, function).upcast().nullable(), function);
                    recipe.clear(function);
                }
                R::StandardFunction => {
                    for &builtin in StandardBuiltinId::all_globals() {
                        if self.functions.get(&builtin.function_id()).is_none() {
                            continue;
                        }
                        row.payload0().load(function);
                        function.instruction(&Instruction::I32Const(builtin as i32));
                        function.instruction(&Instruction::I32Eq);
                        self.open_frame(ControlFrameKind::If, function);
                        let context = self.emit_realm_function_materialization_context_from_realm(
                            realm, function,
                        );
                        let property =
                            self.emit_standard_global_value(builtin, &context, function)?;
                        result.replace(property.load(schema, function).nullable(), function);
                        property.clear(function);
                        self.release_realm_function_materialization_context(context, function);
                        self.emit_branch_to_target(done, function);
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                    }
                    function.instruction(&Instruction::Unreachable);
                }
                R::HostFunction => {
                    for builtin in HostBuiltinId::global_builtins() {
                        if self.functions.get(&builtin.function_id()).is_none() {
                            continue;
                        }
                        row.payload0().load(function);
                        function.instruction(&Instruction::I32Const(builtin as i32));
                        function.instruction(&Instruction::I32Eq);
                        self.open_frame(ControlFrameKind::If, function);
                        let context = self.emit_realm_function_materialization_context_from_realm(
                            realm, function,
                        );
                        let property = self.emit_host_global_value(builtin, &context, function)?;
                        result.replace(property.load(schema, function).nullable(), function);
                        property.clear(function);
                        self.release_realm_function_materialization_context(context, function);
                        self.emit_branch_to_target(done, function);
                        self.pop_control(ControlFrameKind::If);
                        function.instruction(&Instruction::End);
                    }
                    function.instruction(&Instruction::Unreachable);
                }
            }
            self.emit_branch_to_target(done, function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        function.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        let value = schema.reserve_gc_local(function).initialize(
            result.load(schema, function).require_non_null(function),
            function,
        );
        result.clear(function);
        Ok(value)
    }

    pub(crate) fn emit_is_global_builtin_layout_recipe(
        row: &RuntimeHelperPropertyLayoutRow,
        function: &mut Function,
    ) {
        row.kind().load(function);
        function.instruction(&Instruction::I32Const(GlobalBuiltinRecipe::This as i32));
        function.instruction(&Instruction::I32GeU);
    }

    fn emit_standard_global_value(
        &mut self,
        builtin: StandardBuiltinId,
        context: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyValue>, EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let deferred = {
            if builtin == StandardBuiltinId::RegExpConstructor {
                Some(self.emit_deferred_regexp_constructor_value(context, function)?)
            } else if builtin == StandardBuiltinId::DateConstructor {
                Some(self.emit_deferred_date_constructor_value(context, function)?)
            } else if let Some(constructor) =
                crate::heap::ErrorIntrinsicConstructor::from_builtin(builtin)
            {
                Some(self.emit_deferred_error_constructor_value(constructor, context, function)?)
            } else if let Some(demand) = crate::heap::DemandConstructor::from_builtin(builtin) {
                Some(self.emit_deferred_demand_recipe_value(
                    crate::heap::DemandGlobalRecipe::Constructor(demand),
                    context,
                    function,
                )?)
            } else if let Some(constructor) =
                crate::heap::BinaryDataConstructor::from_builtin(builtin)
            {
                Some(self.emit_deferred_binary_data_constructor_value(
                    constructor,
                    context,
                    function,
                )?)
            } else if let Some(method) = match self.functions.get(&builtin.function_id()) {
                Some(meta) => {
                    crate::intrinsics::DeferredIntrinsicMethod::validated_global(builtin, meta)?
                }
                None => None,
            } {
                Some(self.emit_deferred_global_function_value(method, context, function))
            } else if let Some(slot) = if builtin == StandardBuiltinId::EvalFunction {
                Some(NonArrayRealmIntrinsicSlot::EvalFunction)
            } else {
                standard_builtin_constructor_realm_slot(builtin)
            } {
                let canonical = self.emit_load_non_array_realm_intrinsic_stored(
                    context.realm(),
                    slot,
                    function,
                );
                let property = schema
                    .reserve_gc_local(function)
                    .initialize(canonical.load(schema, function).upcast(), function);
                canonical.clear(function);
                Some(property)
            } else {
                let callable = self.emit_intrinsic_callable(builtin, context, function)?;
                value.set_reference(&callable, schema, function);
                callable.clear(function);
                None
            }
        };
        let property = deferred.unwrap_or_else(|| {
            schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&value, function)
                    .upcast(),
                function,
            )
        });
        value.clear(function);
        Ok(property)
    }

    fn emit_host_global_value(
        &mut self,
        builtin: HostBuiltinId,
        context: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyValue>, EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        let deferred = {
            if let Some(demand) = crate::heap::DemandHostFunction::from_builtin(builtin) {
                Some(self.emit_deferred_demand_recipe_value(
                    crate::heap::DemandGlobalRecipe::HostFunction(demand),
                    context,
                    function,
                )?)
            } else if canonical_host_function_realm_slot(builtin).is_some() {
                let callable =
                    self.emit_intrinsic_canonical_host_callable(builtin, context, function)?;
                value.set_reference(&callable, schema, function);
                callable.clear(function);
                None
            } else {
                let meta = self
                    .functions
                    .get(&builtin.function_id())
                    .cloned()
                    .ok_or_else(|| EmitError::unsupported("missing global host function entry"))?;
                let callable = schema
                    .reserve_gc_local::<FunctionObject, NonNullable>(function)
                    .initialize(
                        self.emit_function_value_payload_in_realm(&meta, context, function)?,
                        function,
                    );
                value.set_reference(&callable, schema, function);
                callable.clear(function);
                None
            }
        };
        let property = deferred.unwrap_or_else(|| {
            schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&value, function)
                    .upcast(),
                function,
            )
        });
        value.clear(function);
        Ok(property)
    }

    // Lowered source uses the catalog and the separate declaration plan.
    // An explicit IR plan can also supply a different initial property.
    pub(super) fn emit_entry_global_initializer_overrides(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        global_this: &GcLocal<StoredValue>,
        context: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        use GlobalPropertyInitializerIr as I;
        let catalog = StringPool::global_builtin_catalog(true);
        let bindings = self
            .script_global_bindings
            .expect("main owns its global plan");
        let schema = self.runtime_schema();
        for binding in bindings.iter() {
            let initializer = &binding.initializer;
            if matches!(initializer, I::FreshUndefined | I::SourceFunction(_))
                || !self
                    .runtime_bootstrap_plan
                    .should_install_script_global_binding(initializer)
                || catalog
                    .binary_search_by(|row| row.name.cmp(&binding.name))
                    .is_ok_and(|index| &catalog[index].initializer == initializer)
            {
                continue;
            }
            let value = match *initializer {
                I::BuiltinFunction(builtin) => {
                    self.emit_standard_global_value(builtin, context, function)?
                }
                I::HostFunction(builtin) => {
                    self.emit_host_global_value(builtin, context, function)?
                }
                I::ReflectObject | I::MathObject | I::JsonObject | I::AtomicsObject => {
                    let namespace = match initializer {
                        I::ReflectObject => DemandNamespace::Reflect,
                        I::MathObject => DemandNamespace::Math,
                        I::JsonObject => DemandNamespace::Json,
                        _ => DemandNamespace::Atomics,
                    };
                    self.emit_deferred_demand_recipe_value(
                        DemandGlobalRecipe::Namespace(namespace),
                        context,
                        function,
                    )?
                }
                I::TemporalObject => {
                    let recipe = schema
                        .struct_type::<DeferredTemporalNamespace>()
                        .construct((GcOperand::reference(context.realm(), schema),), function);
                    schema
                        .reserve_gc_local(function)
                        .initialize(recipe.upcast(), function)
                }
                I::IntlObject => {
                    let recipe = schema
                        .struct_type::<DeferredIntlNamespace>()
                        .construct((GcOperand::reference(context.realm(), schema),), function);
                    schema
                        .reserve_gc_local(function)
                        .initialize(recipe.upcast(), function)
                }
                I::Intrinsic => schema
                    .reserve_gc_local(function)
                    .initialize(global_this.load(schema, function).upcast(), function),
                I::Infinity | I::NaN | I::Undefined => {
                    let value = schema.reserve_value_local(function);
                    match initializer {
                        I::Infinity => value.set_scalar(
                            ScalarValue::NumberBits(f64::INFINITY.to_bits() as i64),
                            function,
                        ),
                        I::NaN => value.set_scalar(
                            ScalarValue::NumberBits(f64::NAN.to_bits() as i64),
                            function,
                        ),
                        _ => value.set_undefined(function),
                    }
                    let data = schema.reserve_gc_local(function).initialize(
                        schema
                            .struct_type::<StoredValue>()
                            .from_value(&value, function)
                            .upcast(),
                        function,
                    );
                    value.clear(function);
                    data
                }
                I::FreshUndefined | I::SourceFunction(_) => {
                    unreachable!("source declaration was excluded")
                }
            };
            self.emit_initial_global_property(object, binding, &value, function)?;
            value.clear(function);
        }
        Ok(())
    }

    fn emit_initial_global_property(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        binding: &scriptw_ir::ScriptGlobalBindingIr,
        value: &GcLocal<PropertyValue>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let key = schema.reserve_gc_local(function).initialize(
            self.emit_interned_string_reference(&binding.name, function)?,
            function,
        );
        let key_locals = PropertyKeyLocals::from_string(schema, &key, function);
        let absent = schema
            .reserve_gc_local(function)
            .initialize(schema.stored_undefined(function), function);
        let attributes = StoredPropertyAttributes::Data {
            writable: binding.initializer.writable(),
            enumerable: binding.initializer.enumerable(),
            configurable: binding.initializer.configurable(),
        };
        let descriptor = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::descriptor_word(attributes.descriptor_word()),
                    GcOperand::reference(value, schema),
                    GcOperand::reference(&absent, schema),
                    GcOperand::reference(&absent, schema),
                ),
                function,
            ),
            function,
        );
        self.emit_ordinary_append_property_entry(object, &key_locals, &descriptor, function)?;
        descriptor.clear(function);
        absent.clear(function);
        key_locals.clear(function);
        key.clear(function);
        Ok(())
    }

    pub(super) fn emit_entry_global_declarations(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let bindings = self
            .script_global_bindings
            .expect("main owns its global plan");
        let schema = self.runtime_schema();
        for binding in bindings.entry_declarations() {
            let value = schema.reserve_value_local(function);
            match &binding.initializer {
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
                GlobalPropertyInitializerIr::FreshUndefined => value.set_undefined(function),
                _ => unreachable!("entry declaration plan owns source declarations"),
            }
            let data = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(&value, function)
                    .upcast(),
                function,
            );
            self.emit_initial_global_property(object, binding, &data, function)?;
            data.clear(function);
            value.clear(function);
        }
        Ok(())
    }
}
