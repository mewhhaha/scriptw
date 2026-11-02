//! The complete canonical String prototype includes length and alias identities.

use super::super::*;
use super::IntrinsicInstall;
use super::IntrinsicPropertyLayoutId;

impl FunctionBuilder<'_> {
    pub(crate) fn install_string_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let prototype = schema.reserve_gc_local(function).initialize(
            self.emit_object_header_projection(context.prototype, function),
            function,
        );
        self.emit_install_intrinsic_property_layout(
            &prototype,
            IntrinsicPropertyLayoutId::StringPrototype,
            context.realm.realm(),
            function,
        )?;
        prototype.clear(function);
        Ok(())
    }
}
