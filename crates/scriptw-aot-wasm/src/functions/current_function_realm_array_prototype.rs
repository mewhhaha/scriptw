use super::*;
use crate::gc_types::{
    ArrayObject, GcLocal, GcStackReference, I64Local, PackedValueListConstruction,
};

/// A rooted prototype selected from the executing function's defining Realm.
/// ArrayCreate consumes this owner before publishing its immutable header edge.
#[must_use]
pub(crate) struct CurrentFunctionRealmArrayPrototypeLocal(GcLocal<ArrayObject>);

impl FunctionBuilder<'_> {
    pub(crate) fn emit_load_current_function_realm_array_prototype(
        &mut self,
        function: &mut Function,
    ) -> CurrentFunctionRealmArrayPrototypeLocal {
        let schema = self.runtime_schema();
        let realm = schema
            .reserve_gc_local(function)
            .initialize(self.emit_current_function_realm(function), function);
        let prototype = schema.reserve_gc_local(function).initialize(
            self.emit_load_realm_array_prototype(&realm, function),
            function,
        );
        realm.clear(function);
        CurrentFunctionRealmArrayPrototypeLocal(prototype)
    }

    pub(crate) fn emit_alloc_array_with_current_function_realm_prototype(
        &mut self,
        length: I64Local,
        prototype: CurrentFunctionRealmArrayPrototypeLocal,
        function: &mut Function,
    ) -> Result<GcStackReference<ArrayObject>, EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_reference(&prototype.0, schema, function);
        let result =
            self.emit_alloc_array_payload_with_length_and_prototype(length, &value, function);
        value.clear(function);
        prototype.0.clear(function);
        result
    }

    /// CreateArrayFromList of a private native list: one Array header and
    /// the list's own packed backing, with no per-element definition.
    pub(crate) fn emit_publish_packed_value_list_with_current_function_realm_prototype(
        &mut self,
        list: PackedValueListConstruction,
        prototype: CurrentFunctionRealmArrayPrototypeLocal,
        function: &mut Function,
    ) -> Result<GcStackReference<ArrayObject>, EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_reference(&prototype.0, schema, function);
        let header = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&value), function)?,
            function,
        );
        let result = schema
            .struct_type::<ArrayObject>()
            .with_packed_value_list(&header, list, schema, function);
        header.clear(function);
        value.clear(function);
        prototype.0.clear(function);
        Ok(result)
    }
}
