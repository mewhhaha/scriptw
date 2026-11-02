use super::*;
use crate::gc_types::{ArrayObject, GcLocal, GcLocalSlot, ValueLocals};

/// Bootstrap cannot publish this storage before ArrayCreate initializes it.
#[must_use]
pub(crate) struct ReservedRealmArrayPrototypeLocal(GcLocalSlot<ArrayObject>);

/// The completed Array exotic prototype remains rooted until Realm bootstrap
/// has published its canonical Realm slot.
#[must_use]
pub(crate) struct RealmArrayPrototypeLocal(GcLocal<ArrayObject>);

impl RealmArrayPrototypeLocal {
    pub(crate) fn array(&self) -> &GcLocal<ArrayObject> {
        &self.0
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn reserve_realm_array_prototype_local(
        &mut self,
        function: &mut Function,
    ) -> ReservedRealmArrayPrototypeLocal {
        ReservedRealmArrayPrototypeLocal(self.runtime_schema().reserve_gc_local(function))
    }

    pub(crate) fn emit_initialize_realm_array_prototype(
        &mut self,
        reserved: ReservedRealmArrayPrototypeLocal,
        object_prototype: &ValueLocals,
        realm: &GcLocal<crate::gc_types::RealmRecord>,
        function: &mut Function,
    ) -> Result<RealmArrayPrototypeLocal, EmitError> {
        let schema = self.runtime_schema();
        let layout = crate::intrinsics::IntrinsicPropertyLayoutId::ArrayPrototype;
        let header_value = if self.runtime_bootstrap_plan.full_standard_globals
            || self.strings.has_intrinsic_property_layout(layout)?
        {
            self.emit_alloc_intrinsic_layout_header(object_prototype, layout, realm, function)?
        } else {
            self.emit_alloc_plain_object_with_prototype(Some(object_prototype), function)?
        };
        let header = schema
            .reserve_gc_local(function)
            .initialize(header_value, function);
        let length = schema.reserve_i64_local(function);
        length.set_constant(0, function);
        let prototype = reserved.0.initialize(
            schema
                .struct_type::<ArrayObject>()
                .with_empty_index_storage(&header, length, true, schema, function),
            function,
        );
        schema.release_i64_local(length, function);
        header.clear(function);
        Ok(RealmArrayPrototypeLocal(prototype))
    }

    pub(crate) fn release_realm_array_prototype_local(
        &mut self,
        prototype: RealmArrayPrototypeLocal,
        function: &mut Function,
    ) {
        prototype.0.clear(function);
    }
}
