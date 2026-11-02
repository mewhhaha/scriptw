//! Constructor properties and the complete canonical Array prototype layout.

use super::super::*;
use super::IntrinsicPropertyLayoutId;
use super::{IntrinsicInstall, IntrinsicKey};
use crate::objects::{AccessorDescriptor, AccessorGetter};

impl FunctionBuilder<'_> {
    pub(crate) fn install_array_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_methods(
            context.constructor,
            crate::intrinsics::IntrinsicMethodPlan::ArrayConstructor,
            context.realm,
            function,
        )?;
        self.emit_install_intrinsic_accessor(
            context.constructor,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Species),
            AccessorDescriptor::Getter(AccessorGetter::new(StandardBuiltinId::ArraySpeciesGetter)),
            context.realm,
            true,
            function,
        )?;
        // Internal consumers read these canonical Realm slots before any public
        // property observation. Their identities remain eager, without descriptors.
        for builtin in [
            StandardBuiltinId::TypedArrayPrototypeToString,
            StandardBuiltinId::ArrayPrototypeValues,
        ] {
            let callable = self.emit_intrinsic_callable(builtin, context.realm, function)?;
            callable.clear(function);
        }
        let schema = self.runtime_schema();
        let prototype = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(context.prototype, function),
            function,
        );
        self.emit_install_intrinsic_property_layout(
            &prototype,
            IntrinsicPropertyLayoutId::ArrayPrototype,
            context.realm.realm(),
            function,
        )?;
        prototype.clear(function);
        Ok(())
    }
}
