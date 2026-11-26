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
        schema.call_helper(
            RealmInitializeIntrinsicsArguments::new(realm, self.current_environment()),
            self.runtime_helper_base()?,
            function,
        );
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
        bootstrap.clear(self, &mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
