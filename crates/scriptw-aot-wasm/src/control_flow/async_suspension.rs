//! One return boundary for an activation with a committed Await continuation.

mod await_operand;

use super::*;
use crate::function_entry::ResumableEntryLocals;

impl FunctionBuilder<'_> {
    /// Return after the actual activation has committed its next resume point.
    /// A plain async Normal completion with target zero means final falloff;
    /// the saved nonzero point keeps its Promise live for the registered job.
    /// Async generators retain their separate BODY_STATUS suspension protocol.
    pub(super) fn emit_return_async_suspension(
        &self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let activation = self
            .body_entry_locals()
            .and_then(|entry| entry.resume_activation())
            .ok_or_else(|| {
                EmitError::unsupported(
                    "compiler invariant: async suspension has no body activation",
                )
            })?;
        match activation {
            ResumableEntryLocals::Async(activation) => {
                self.emit_statement_result(function);
                let schema = self.runtime_schema();
                schema
                    .struct_type::<AsyncActivation>()
                    .field(AsyncActivationSchema::RESUME_POINT)
                    .read(activation, schema, function)
                    .store(self.completion().target(), function);
            }
            ResumableEntryLocals::AsyncGenerator(_) => self.emit_statement_result(function),
            ResumableEntryLocals::Generator(_) => {
                return Err(EmitError::unsupported(
                    "compiler invariant: synchronous generator cannot suspend an Await",
                ));
            }
        }
        self.emit_return_current_completion(function);
        Ok(())
    }
}
