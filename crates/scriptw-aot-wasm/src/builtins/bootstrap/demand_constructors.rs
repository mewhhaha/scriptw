//! Canonical constructors built on their first canonical read. Each is
//! covered by a complete constructor layout and named by its eagerly built
//! prototype's layout row, so later construction changes no property order.
//! The canonical slot holds the Undefined placeholder until then.

use super::*;
use crate::heap::DemandConstructor;
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
        let layout = IntrinsicPropertyLayoutId::for_prototype(demand.prototype_slot())
            .ok_or_else(|| {
                EmitError::unsupported("demand constructor prototype requires a complete layout")
            })?
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
        let context =
            self.emit_realm_function_materialization_context_from_realm(&parameters.realm, &mut function);
        for demand in DemandConstructor::ALL {
            if self.functions.get(&demand.builtin().function_id()).is_none() {
                continue;
            }
            self.validate_demand_constructor(demand)?;
            parameters.slot.load(&mut function);
            function.instruction(&Instruction::I32Const(demand.slot().gc_index() as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, &mut function);
            let prototype = schema.reserve_value_local(&mut function);
            self.emit_load_non_array_realm_intrinsic(
                &parameters.realm,
                demand.prototype_slot(),
                &prototype,
                &mut function,
            );
            let constructor = self.init_builtin_constructor_object(
                demand.builtin(),
                &context,
                &prototype,
                &mut function,
            )?;
            constructor.clear(&mut function);
            prototype.clear(&mut function);
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

    /// The recipe a global binding publishes at its original position and
    /// attributes. Its first semantic read performs the canonical read.
    pub(super) fn emit_deferred_demand_constructor_value(
        &mut self,
        demand: DemandConstructor,
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
