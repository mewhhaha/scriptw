//! Canonical constructors built on their first canonical read. Each is
//! covered by a complete constructor layout and named by its eagerly built
//! prototype's layout row, so later construction changes no property order.
//! The canonical slot holds the Undefined placeholder until then.

use super::*;
use crate::heap::{
    DemandBoxedPrototype, DemandCallable, DemandConstructor, DemandDynamicFunction, DemandGlobalRecipe,
    DemandHostFunction, DemandNamespace, DemandPrototype,
};
use crate::intrinsics::property_layouts::IntrinsicPropertyRecipe;
use crate::runtime_helpers::{HelperParameters, RealmEnsureConstructorParameters};

impl FunctionBuilder<'_> {
    /// Deferring a constructor that adds its own `prototype.constructor` data
    /// property would append that property after the prototype's others.
    fn validate_demand_constructor(&self, demand: DemandConstructor) -> Result<(), EmitError> {
        let builtin = demand.builtin();
        let meta = self.functions.get(&builtin.function_id()).ok_or_else(|| {
            EmitError::unsupported("demand constructor lacks its planned native entry")
        })?;
        if meta.standard_builtin != Some(builtin)
            || !builtin.constructable()
            || standard_builtin_constructor_realm_slot(builtin)
                .map(NonArrayRealmIntrinsicSlot::gc_index)
                != Some(demand.slot().gc_index())
            || IntrinsicPropertyLayoutId::for_constructor(meta)?.is_none()
        {
            return Err(EmitError::unsupported(
                "demand constructor requires a complete constructor layout",
            ));
        }
        if matches!(
            demand,
            DemandConstructor::Proxy | DemandConstructor::Iterator
        ) {
            // Proxy has no prototype property; Iterator.prototype.constructor
            // is an accessor whose native getter performs a canonical read.
            return Ok(());
        }
        let layout = match demand.prototype_slot() {
            Some(slot) => IntrinsicPropertyLayoutId::for_prototype(slot).ok_or_else(|| {
                EmitError::unsupported("demand constructor prototype requires a complete layout")
            })?,
            None => IntrinsicPropertyLayoutId::ArrayPrototype,
        }
        .layout()?;
        let named = layout.properties().iter().any(|property| {
            matches!(property.key, IntrinsicKey::Name("constructor"))
                && matches!(property.recipe, IntrinsicPropertyRecipe::ReadyRealmSlot(slot)
                    if slot.gc_index() == demand.slot().gc_index())
        });
        if !named {
            return Err(EmitError::unsupported(
                "demand constructor prototype must name it through a layout row",
            ));
        }
        Ok(())
    }

    /// Builds one canonical constructor in its defining Realm. Every caller
    /// has observed the placeholder; a slot outside the closed set traps.
    pub(crate) fn compile_realm_ensure_constructor_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RealmEnsureConstructor);
        let parameters = self.helper_parameters::<RealmEnsureConstructorParameters>(&mut function);
        let schema = self.runtime_schema();
        let context = self.emit_realm_function_materialization_context_from_realm(
            &parameters.realm,
            &mut function,
        );
        for demand in DemandConstructor::ALL {
            if self
                .functions
                .get(&demand.builtin().function_id())
                .is_none()
            {
                continue;
            }
            self.validate_demand_constructor(demand)?;
            parameters.slot.load(&mut function);
            function.instruction(&Instruction::I32Const(demand.slot().gc_index() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, &mut function);
            match demand.prototype_slot() {
                Some(slot) => self.emit_bootstrap_constructor_from_slot(
                    demand.builtin(),
                    &context,
                    slot,
                    &mut function,
                )?,
                None => self.emit_initialize_array_constructor(&context, &mut function)?,
            }
            function.instruction(&Instruction::Return);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for demand in DemandCallable::ALL {
            let Some(meta) = self.functions.get(&demand.builtin().function_id()).cloned() else {
                continue;
            };
            parameters.slot.load(&mut function);
            function.instruction(&Instruction::I32Const(demand.slot().gc_index() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, &mut function);
            let callable = schema.reserve_gc_local(&mut function).initialize(
                self.emit_function_value_payload_in_realm(&meta, &context, &mut function)?,
                &mut function,
            );
            let value = schema.reserve_value_local(&mut function);
            value.set_reference(&callable, schema, &mut function);
            self.emit_store_non_array_realm_intrinsic(
                &parameters.realm,
                demand.slot(),
                &value,
                &mut function,
            );
            value.clear(&mut function);
            callable.clear(&mut function);
            function.instruction(&Instruction::Return);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for demand in DemandPrototype::ALL {
            parameters.slot.load(&mut function);
            function.instruction(&Instruction::I32Const(demand.slot().gc_index() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, &mut function);
            let parent = schema.reserve_value_local(&mut function);
            self.emit_load_non_array_realm_intrinsic(
                &parameters.realm,
                demand.parent_slot(),
                &parent,
                &mut function,
            );
            self.emit_bootstrap_plain_prototype(&context, demand.slot(), &parent, &mut function)?;
            parent.clear(&mut function);
            function.instruction(&Instruction::Return);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for demand in DemandBoxedPrototype::ALL {
            parameters.slot.load(&mut function);
            function.instruction(&Instruction::I32Const(demand.slot().gc_index() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, &mut function);
            let parent = schema.reserve_value_local(&mut function);
            self.emit_load_non_array_realm_intrinsic(
                &parameters.realm,
                NonArrayRealmIntrinsicSlot::ObjectPrototype,
                &parent,
                &mut function,
            );
            let primitive = schema.reserve_value_local(&mut function);
            match demand {
                DemandBoxedPrototype::Number => {
                    primitive.set_scalar(ScalarValue::NumberBits(0), &mut function);
                }
                DemandBoxedPrototype::Boolean => {
                    primitive.set_scalar(ScalarValue::Boolean(false), &mut function);
                }
            }
            let prototype = self.emit_bootstrap_boxed_prototype(
                &context,
                demand.slot(),
                &parent,
                &primitive,
                &mut function,
            )?;
            self.emit_publish_bootstrap_boxed_prototype(
                &parameters.realm,
                demand.slot(),
                prototype,
                &mut function,
            );
            primitive.clear(&mut function);
            parent.clear(&mut function);
            function.instruction(&Instruction::Return);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for demand in DemandDynamicFunction::ALL {
            if self
                .functions
                .get(&demand.builtin().function_id())
                .is_none()
            {
                continue;
            }
            parameters.slot.load(&mut function);
            function.instruction(&Instruction::I32Const(demand.slot().gc_index() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, &mut function);
            self.emit_bootstrap_dynamic_function_intrinsic(
                demand.builtin(),
                demand.prototype_slot(),
                demand.slot(),
                &context,
                &mut function,
            )?;
            function.instruction(&Instruction::Return);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for demand in DemandHostFunction::ALL {
            if self
                .functions
                .get(&demand.builtin().function_id())
                .is_none()
            {
                continue;
            }
            parameters.slot.load(&mut function);
            function.instruction(&Instruction::I32Const(demand.slot().gc_index() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, &mut function);
            let callable = self.emit_intrinsic_canonical_host_callable(
                demand.builtin(),
                &context,
                &mut function,
            )?;
            callable.clear(&mut function);
            function.instruction(&Instruction::Return);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        for demand in DemandNamespace::ALL {
            if !(self.runtime_bootstrap_plan.full_standard_globals
                || self.strings.has_intrinsic_property_layout(demand.layout())?)
            {
                continue;
            }
            parameters.slot.load(&mut function);
            function.instruction(&Instruction::I32Const(demand.slot().gc_index() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, &mut function);
            let object_prototype = schema.reserve_value_local(&mut function);
            self.emit_load_non_array_realm_intrinsic(
                &parameters.realm,
                NonArrayRealmIntrinsicSlot::ObjectPrototype,
                &object_prototype,
                &mut function,
            );
            let namespace = match demand {
                DemandNamespace::Reflect => {
                    self.init_reflect_object(&context, &object_prototype, &mut function)?
                }
                DemandNamespace::Math => {
                    self.init_math_object(&context, &object_prototype, &mut function)?
                }
                DemandNamespace::Json => {
                    self.init_json_object(&context, &object_prototype, &mut function)?
                }
                DemandNamespace::Atomics => {
                    self.init_atomics_object(&context, &object_prototype, &mut function)?
                }
            };
            self.emit_store_non_array_realm_intrinsic(
                &parameters.realm,
                demand.slot(),
                &namespace,
                &mut function,
            );
            namespace.clear(&mut function);
            object_prototype.clear(&mut function);
            function.instruction(&Instruction::Return);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        // The slot operand is a closed compiler-owned domain.
        function.instruction(&Instruction::Unreachable);
        self.release_realm_function_materialization_context(context, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// The recipe a property publishes at its original position and
    /// attributes. Its first semantic read performs the canonical read.
    pub(super) fn emit_deferred_demand_recipe_value(
        &mut self,
        demand: DemandGlobalRecipe,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<GcLocal<PropertyValue>, EmitError> {
        let schema = self.runtime_schema();
        let recipe = schema.reserve_gc_local(function).initialize(
            schema.struct_type::<DeferredDemandConstructor>().construct(
                (
                    GcOperand::reference(realm.realm(), schema),
                    GcOperand::constant(demand),
                ),
                function,
            ),
            function,
        );
        let deferred = schema
            .reserve_gc_local(function)
            .initialize(recipe.load(schema, function).upcast(), function);
        recipe.clear(function);
        Ok(deferred)
    }
}
