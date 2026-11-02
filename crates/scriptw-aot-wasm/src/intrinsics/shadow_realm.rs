//! Each Realm owns its ShadowRealm constructor, prototype and native methods.

use super::super::*;
use super::IntrinsicPropertyLayoutId;
use crate::functions::{NonArrayRealmIntrinsicSlot, RealmFunctionMaterializationContext};
use crate::gc_types::ValueLocals;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_initialize_shadow_realm_intrinsic(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        object_prototype: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_intrinsic_layout_header(
                object_prototype,
                IntrinsicPropertyLayoutId::ShadowRealmPrototype,
                realm.realm(),
                function,
            )?,
            function,
        );
        let prototype = schema.reserve_value_local(function);
        prototype.set_reference(&object, schema, function);
        self.emit_store_non_array_realm_intrinsic(
            realm.realm(),
            NonArrayRealmIntrinsicSlot::ShadowRealmPrototype,
            &prototype,
            function,
        );
        let constructor = self.init_builtin_constructor_object(
            StandardBuiltinId::ShadowRealmConstructor,
            realm,
            &prototype,
            function,
        )?;
        constructor.clear(function);
        prototype.clear(function);
        object.clear(function);
        Ok(())
    }
}
