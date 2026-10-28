//! Fresh intrinsic properties retain completed values and defining Realm.

use super::super::*;
use super::{IntrinsicInstall, IntrinsicKey};

impl FunctionBuilder<'_> {
    pub(crate) fn install_function_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::FunctionPrototype,
            context.realm,
            function,
        )?;
        self.emit_install_intrinsic_method(
            context.prototype,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::HasInstance),
            StandardBuiltinId::FunctionPrototypeSymbolHasInstance,
            context.realm,
            false,
            false,
            function,
        )?;
        Ok(())
    }
}
