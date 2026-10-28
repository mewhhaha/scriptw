//! Date prototype publication preserves method order and UTC/GMT identity.

use super::super::*;
use super::{IntrinsicInstall, IntrinsicKey};

impl FunctionBuilder<'_> {
    pub(crate) fn install_date_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_methods(
            context.constructor,
            crate::intrinsics::IntrinsicMethodPlan::DateConstructor,
            context.realm,
            function,
        )?;
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::DatePrototype,
            context.realm,
            function,
        )?;
        self.emit_install_intrinsic_method_aliases(
            context.prototype,
            &[
                IntrinsicKey::Name("toUTCString"),
                IntrinsicKey::Name("toGMTString"),
            ],
            StandardBuiltinId::DatePrototypeToUtcString,
            context.realm,
            true,
            true,
            function,
        )?;
        Ok(())
    }
}
