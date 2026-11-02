//! Constructor methods and the complete canonical Object prototype layout.

use super::super::*;
use super::IntrinsicInstall;
use super::IntrinsicPropertyLayoutId;

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
        let schema = self.runtime_schema();
        let prototype = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(context.prototype, function),
            function,
        );
        self.emit_install_intrinsic_property_layout(
            &prototype,
            IntrinsicPropertyLayoutId::ObjectPrototype,
            context.realm.realm(),
            function,
        )?;
        prototype.clear(function);
        self.emit_install_intrinsic_methods(
            context.constructor,
            crate::intrinsics::IntrinsicMethodPlan::ObjectConstructorRemaining,
            context.realm,
            function,
        )?;
        Ok(())
    }
}
