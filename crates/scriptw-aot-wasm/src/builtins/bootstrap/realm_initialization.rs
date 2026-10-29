use super::*;
use crate::runtime_helpers::{
    HelperParameters, RealmInitializeIntrinsicsArguments, RealmInitializeIntrinsicsParameters,
};

impl FunctionBuilder<'_> {
    /// Both entry and created Realms call one emitted initialization algorithm.
    /// Global publication remains in the caller, after its own declaration work.
    pub(super) fn emit_initialize_realm_intrinsics(
        &mut self,
        realm: &GcLocal<RealmRecord>,
        function: &mut Function,
    ) -> Result<BootstrapRealm, EmitError> {
        let schema = self.runtime_schema();
        let namespaces = schema
            .call_helper(
                RealmInitializeIntrinsicsArguments::new(realm, self.current_environment()),
                self.runtime_helper_base()?,
                function,
            )
            .bind(schema, function);
        let plan = &self.runtime_bootstrap_plan;
        let present = [
            plan.full_standard_globals || plan.reflect_object,
            plan.full_standard_globals || plan.math_object,
            plan.full_standard_globals || plan.json_object,
            plan.full_standard_globals || plan.atomics_object,
        ];
        let mut namespace_values = namespaces.into_iter().zip(present).map(|(root, present)| {
            let value = present.then(|| {
                let object = schema.reserve_gc_local(function).initialize(
                    root.load(schema, function).require_non_null(function),
                    function,
                );
                let value = schema.reserve_value_local(function);
                value.set_reference(&object, schema, function);
                object.clear(function);
                value
            });
            root.clear(function);
            value
        });
        let [reflect, math, json, atomics] =
            std::array::from_fn(|_| namespace_values.next().expect("four namespace results"));
        let object_prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            realm,
            NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &object_prototype,
            function,
        );
        let context = self.emit_realm_function_materialization_context_from_realm(realm, function);
        Ok(BootstrapRealm {
            realm: context,
            object_prototype,
            reflect,
            math,
            json,
            atomics,
        })
    }

    pub(crate) fn compile_realm_initialize_intrinsics_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::RealmInitializeIntrinsics);
        let parameters =
            self.helper_parameters::<RealmInitializeIntrinsicsParameters>(&mut function);
        let bootstrap =
            self.emit_initialize_realm_intrinsics_inner(&parameters.realm, &mut function)?;
        let schema = self.runtime_schema();
        let absent = schema
            .reserve_gc_local::<OrdinaryObject, Nullable>(&mut function)
            .initialize_null(schema, &mut function);
        for namespace in [
            &bootstrap.reflect,
            &bootstrap.math,
            &bootstrap.json,
            &bootstrap.atomics,
        ] {
            if let Some(value) = namespace {
                value.cast_reference::<OrdinaryObject>(schema, &mut function);
            } else {
                absent.load(schema, &mut function);
            }
        }
        // The four references are now rooted on the Wasm result stack. Releasing
        // local owners cannot make them disappear while the caller binds them.
        absent.clear(&mut function);
        bootstrap.clear(self, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
