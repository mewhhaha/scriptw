//! R-only shared literal roots and one private literal/data image for every P.

use super::*;
use crate::data::{PooledLiteralDomain, PooledStringIndex};
use crate::gc_types::{ProgramInstanceImage, ProgramLiteralRoots, ProgramMetadataBytes};
mod program_strings;
mod runtime_strings;

impl FunctionBuilder<'_> {
    /// Only genuine runtime constants and the shared Symbol tables are gated
    /// by R initialization. P publication has a separate unconditional producer.
    pub(in crate::emit) fn initialize_runtime_literal_roots(
        &self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if !(self.is_main() || self.is_program_initialize()) || !self.uses_heap {
            return Ok(());
        }
        let schema = self.schema;
        schema.pooled_strings_initialized(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        schema.call_helper(
            crate::runtime_helpers::PooledStringsInitializeArguments::new(),
            self.runtime_helper_base()?,
            function,
        );
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// The dedicated P bootstrap constructs resources before any source function.
    /// The checked image and both returned roots go directly to publish; no
    /// global P table or initialized bit can suppress a later P instance.
    pub(in crate::emit) fn produce_program_literal_roots<'image, 'pool>(
        &self,
        image: &'image ProgramInstanceImage<'pool>,
        function: &mut Function,
    ) -> Result<
        (
            ProgramLiteralRoots<'image, 'pool>,
            ProgramMetadataBytes<'image, 'pool>,
        ),
        EmitError,
    > {
        if !self.is_program_initialize() || !self.uses_heap {
            return Err(EmitError::unsupported(
                "P literal publication requires a linked heap program entry",
            ));
        }
        image.require_pool(self.strings)?;
        let literals = self.initialize_program_string_roots(image, function);
        let metadata = ProgramMetadataBytes::from_program_segment(self.schema, image, function);
        Ok((literals, metadata))
    }

    /// Both source literals and source-specific diagnostic strings use this
    /// route. Native R bodies resolve their R prefix without needing a P frame.
    pub(crate) fn emit_pooled_string_reference(
        &self,
        index: PooledStringIndex,
        function: &mut Function,
    ) -> Result<GcStackReference<StringValue>, EmitError> {
        match self.strings.pooled_string_domain(index)? {
            PooledLiteralDomain::Runtime(index) => Ok(self.schema.pooled_string_reference(
                index,
                self.runtime_helper_base()?,
                function,
            )),
            PooledLiteralDomain::Program(index) => {
                Ok(self.current_program().literal(self.schema, index, function))
            }
        }
    }
}
