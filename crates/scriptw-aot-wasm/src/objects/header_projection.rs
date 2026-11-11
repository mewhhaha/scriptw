//! All generic Value owners consume one registered GC header projection.

use super::*;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_object_header_projection(
        &self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> GcStackReference<OrdinaryObject> {
        let schema = self.runtime_schema();
        let base = self
            .runtime_helper_base()
            .expect("object-header projection requires the registered Wasm-GC helper plan");
        schema.emit_ordinary_header_or_registered_projection(value, function, |function| {
            let _ = schema.helper_reference_on_stack(schema.call_helper(
                crate::runtime_helpers::ObjectHeaderProjectionArguments::new(value),
                base,
                function,
            ));
        })
    }
}
