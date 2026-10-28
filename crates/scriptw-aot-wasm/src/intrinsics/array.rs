//! Intrinsic methods preserve their canonical property installation order.

use super::super::*;
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
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::ArrayPrototype,
            context.realm,
            function,
        )?;
        self.emit_install_intrinsic_method_aliases(
            context.prototype,
            &[
                IntrinsicKey::Name("values"),
                IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Iterator),
            ],
            StandardBuiltinId::ArrayPrototypeValues,
            context.realm,
            true,
            true,
            function,
        )?;
        // Arguments objects capture the exact installed canonical values function
        // through its strong Realm intrinsic slot, without a public property Get.
        let schema = self.runtime_schema();
        let unscopables = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(None, function)?,
            function,
        );
        let object = schema.reserve_value_local(function);
        object.set_reference(&unscopables, schema, function);
        let value = schema.reserve_value_local(function);
        value.set_scalar(crate::gc_types::ScalarValue::Boolean(true), function);
        for name in [
            "at",
            "copyWithin",
            "entries",
            "fill",
            "find",
            "findIndex",
            "findLast",
            "findLastIndex",
            "flat",
            "flatMap",
            "includes",
            "keys",
            "toReversed",
            "toSorted",
            "toSpliced",
            "values",
        ] {
            self.emit_install_intrinsic_data(
                &object,
                IntrinsicKey::Name(name),
                &value,
                true,
                true,
                true,
                function,
            )?;
        }
        self.emit_install_intrinsic_data(
            context.prototype,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Unscopables),
            &object,
            false,
            false,
            true,
            function,
        )?;
        value.clear(function);
        object.clear(function);
        unscopables.clear(function);
        Ok(())
    }
}
