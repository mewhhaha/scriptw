use super::*;
use crate::gc_types::ProgramStringDescriptorTable;
use crate::runtime_helpers::{HelperParameters, ProgramLiteralResolveParameters};

impl FunctionBuilder<'_> {
    /// Publication allocates no literal String: it copies the descriptor image
    /// and an all-null table of the exact P literal count. Each slot is filled
    /// by [`Self::compile_program_literal_resolve_hook`] at its first use.
    pub(super) fn initialize_program_string_roots<'image, 'pool>(
        &self,
        image: &'image ProgramInstanceImage<'pool>,
        function: &mut Function,
    ) -> ProgramLiteralRoots<'image, 'pool> {
        let descriptor_image = self.strings.program_string_descriptor_image();
        assert_eq!(descriptor_image.count(), image.literal_count());
        let descriptors =
            ProgramStringDescriptorTable::from_image(self.schema, &descriptor_image, function);
        ProgramLiteralRoots::unresolved(self.schema, image, descriptors, function)
    }

    /// The P-owned resolver: publish one literal of the owner it is given, or
    /// return the String a previous call published. Each slot is written at
    /// most once, with a String built from this P's own passive UTF-16 image,
    /// so a literal keeps one identity per ProgramInstance however it is
    /// reached. The body allocates, but runs no JavaScript and polls no
    /// deadline between the allocation and the publication.
    pub(crate) fn compile_program_literal_resolve_hook(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ProgramLiteralResolve);
        let parameters = self.helper_parameters::<ProgramLiteralResolveParameters>(&mut function);
        let schema = self.schema;
        let program = self.current_program();
        let string = program.cached_literal_at(schema, parameters.ordinal, &mut function);
        string.load(schema, &mut function).is_null(&mut function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let fresh =
            program.publish_literal_from_descriptor(schema, parameters.ordinal, &mut function);
        string.replace(fresh.load(schema, &mut function).nullable(), &mut function);
        fresh.clear(&mut function);
        function.instruction(&Instruction::End);
        let _ = string
            .load(schema, &mut function)
            .require_non_null(&mut function);
        string.clear(&mut function);
        self.clear_entry_program(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
