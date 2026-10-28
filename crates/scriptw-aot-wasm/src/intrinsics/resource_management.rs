//! DisposableStack disposal aliases share one completed native function.

use super::super::*;
use super::{IntrinsicInstall, IntrinsicKey};
use crate::objects::{AccessorDescriptor, AccessorGetter};

impl FunctionBuilder<'_> {
    pub(crate) fn install_disposable_stack_constructor_intrinsics(
        &mut self,
        context: &IntrinsicInstall<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_install_intrinsic_methods(
            context.prototype,
            crate::intrinsics::IntrinsicMethodPlan::DisposableStackPrototype,
            context.realm,
            function,
        )?;
        self.emit_install_intrinsic_method_aliases(
            context.prototype,
            &[
                IntrinsicKey::Name("dispose"),
                IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::Dispose),
            ],
            StandardBuiltinId::DisposableStackPrototypeDispose,
            context.realm,
            true,
            true,
            function,
        )?;
        self.emit_install_intrinsic_accessor(
            context.prototype,
            IntrinsicKey::Name("disposed"),
            AccessorDescriptor::Getter(AccessorGetter::new(
                StandardBuiltinId::DisposableStackPrototypeDisposedGetter,
            )),
            context.realm,
            true,
            function,
        )?;
        self.emit_install_intrinsic_string(
            context.prototype,
            IntrinsicKey::Symbol(scriptw_ir::WellKnownSymbol::ToStringTag),
            "DisposableStack",
            false,
            false,
            true,
            function,
        )?;
        Ok(())
    }
}
