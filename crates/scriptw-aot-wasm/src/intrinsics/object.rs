//! Intrinsic methods preserve their canonical property installation order.

use super::super::*;
use super::{IntrinsicInstall, IntrinsicKey};
use crate::objects::{AccessorDescriptor, AccessorGetter, AccessorSetter};

impl FunctionBuilder<'_> {
    pub(crate) fn install_object_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_methods(
            context.constructor,
            crate::intrinsics::IntrinsicMethodPlan::ObjectConstructorInitial,
            context.realm,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::ObjectPrototypeBeforeAccessor,
            context.realm,
            function,
        )?;
        self.emit_install_intrinsic_accessor(
            context.prototype,
            IntrinsicKey::Name("__proto__"),
            AccessorDescriptor::GetterAndSetter {
                getter: AccessorGetter::new(StandardBuiltinId::ObjectPrototypeProtoGetter),
                setter: AccessorSetter::new(StandardBuiltinId::ObjectPrototypeProtoSetter),
            },
            context.realm,
            true,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::ObjectPrototypeAfterAccessor,
            context.realm,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            context.constructor,
            crate::intrinsics::IntrinsicMethodPlan::ObjectConstructorRemaining,
            context.realm,
            function,
        )?;
        Ok(())
    }
}
