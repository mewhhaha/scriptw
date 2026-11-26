//! The Array constructor uses the defining Realm's eager Array prototype.

use super::super::*;
use crate::functions::RealmFunctionMaterializationContext;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_initialize_array_constructor(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let array = schema
            .reserve_gc_local(function)
            .initialize(self.emit_load_realm_array_prototype(realm.realm(), function), function);
        let prototype = schema.reserve_value_local(function);
        prototype.set_reference(&array, schema, function);
        let constructor = self.init_builtin_constructor_object(
            StandardBuiltinId::ArrayConstructor,
            realm,
            &prototype,
            function,
        )?;
        constructor.clear(function);
        prototype.clear(function);
        array.clear(function);
        Ok(())
    }
}
