//! Canonical internal identities precede the complete Array prototype layout.

use super::super::*;
use crate::functions::RealmFunctionMaterializationContext;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_initialize_array_canonical_callables(
        &mut self,
        realm: &RealmFunctionMaterializationContext,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        // Internal consumers require these identities before public observation.
        for builtin in [
            StandardBuiltinId::TypedArrayPrototypeToString,
            StandardBuiltinId::ArrayPrototypeValues,
        ] {
            let callable = self.emit_intrinsic_callable(builtin, realm, function)?;
            callable.clear(function);
        }
        Ok(())
    }
}
