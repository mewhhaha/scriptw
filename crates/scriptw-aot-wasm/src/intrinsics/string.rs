//! Fresh intrinsic properties retain completed values and defining Realm.

use super::super::*;
use super::{IntrinsicInstall, IntrinsicKey};

impl FunctionBuilder<'_> {
    pub(crate) fn install_string_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::StringPrototypeBeforeAliases,
            context.realm,
            function,
        )?;
        for (keys, builtin) in [
            (
                [
                    IntrinsicKey::Name("trimStart"),
                    IntrinsicKey::Name("trimLeft"),
                ],
                StandardBuiltinId::StringPrototypeTrimStart,
            ),
            (
                [
                    IntrinsicKey::Name("trimEnd"),
                    IntrinsicKey::Name("trimRight"),
                ],
                StandardBuiltinId::StringPrototypeTrimEnd,
            ),
        ] {
            self.emit_install_intrinsic_method_aliases(
                context.prototype,
                &keys,
                builtin,
                context.realm,
                true,
                true,
                function,
            )?;
        }
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::StringPrototypeAfterAliases,
            context.realm,
            function,
        )?;
        Ok(())
    }
}
