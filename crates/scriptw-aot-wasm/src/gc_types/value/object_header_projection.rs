//! The sole physical projection from the exhaustive actual GC layout owner.

use super::*;
use crate::emit::FunctionBuilder;
use crate::runtime_helpers::{HelperParameters, RuntimeHelperId};
use crate::EmitError;

/// Tested first: the layouts that most object operations actually meet.
const FREQUENT_OBJECT_LAYOUTS: [GcLayout; 3] = [
    GcLayout::OrdinaryObject,
    GcLayout::ArrayObject,
    GcLayout::FunctionObject,
];

impl RuntimeSchema {
    /// An ordinary header is its own projection. Every other layout takes
    /// `registered`, which must leave the projected header on the stack.
    pub(crate) fn emit_ordinary_header_or_registered_projection(
        &self,
        value: &ValueLocals,
        function: &mut Function,
        registered: impl FnOnce(&mut Function),
    ) -> GcStackReference<OrdinaryObject> {
        let output = self.reference_type::<OrdinaryObject>(GcNullability::NonNullable);
        value.reference.load(function);
        function.instruction(&Instruction::RefTestNonNull(output.heap_type));
        function.instruction(&Instruction::If(BlockType::Result(ValType::Ref(output))));
        value.reference.load(function);
        function.instruction(&Instruction::RefCastNonNull(output.heap_type));
        function.instruction(&Instruction::Else);
        registered(function);
        function.instruction(&Instruction::End);
        GcStackReference::new()
    }

    /// Composite records share their declared OBJECT field, never a guessed
    /// cast. Only this module's registered body may emit the complete dispatch.
    fn emit_object_header_projection_kernel(
        &self,
        value: &ValueLocals,
        function: &mut Function,
    ) -> GcStackReference<OrdinaryObject> {
        let output = self.reference_type::<OrdinaryObject>(GcNullability::NonNullable);
        let mut branches = 0usize;
        let ordered = FREQUENT_OBJECT_LAYOUTS.iter().chain(
            GcLayout::ALL
                .iter()
                .filter(|layout| !FREQUENT_OBJECT_LAYOUTS.contains(layout)),
        );
        for layout in ordered {
            let Some(projection) = layout.object_projection() else {
                continue;
            };
            let heap_type = self
                .types
                .layouts
                .reference(*layout, GcNullability::NonNullable)
                .heap_type;
            value.reference.load(function);
            function.instruction(&Instruction::RefTestNonNull(heap_type));
            function.instruction(&Instruction::If(BlockType::Result(ValType::Ref(output))));
            value.reference.load(function);
            function.instruction(&Instruction::RefCastNonNull(heap_type));
            match projection {
                ObjectHeaderProjection::Own => {}
                ObjectHeaderProjection::Field(field) => {
                    function.instruction(&Instruction::StructGet {
                        struct_type_index: self.types.layouts.layout_index(*layout),
                        field_index: field.raw(),
                    });
                }
            }
            function.instruction(&Instruction::Else);
            branches += 1;
        }
        function.instruction(&Instruction::Unreachable);
        for _ in 0..branches {
            function.instruction(&Instruction::End);
        }
        GcStackReference::new()
    }
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_object_header_projection_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::ObjectHeaderProjection);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::ObjectHeaderProjectionParameters>(
                &mut function,
            );
        let schema = self.runtime_schema();
        // Unknown/non-object references retain the original invariant trap.
        let header = schema.reserve_gc_local(&mut function).initialize(
            schema.emit_object_header_projection_kernel(&parameters.value, &mut function),
            &mut function,
        );
        let _ = header.load(schema, &mut function);
        header.clear(&mut function);
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }
}
