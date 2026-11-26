use super::*;
impl ProgramInstanceLocals {
    pub(super) fn absent_native_payload_slot(
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcLocal<NativeSourcePayloadArray, Nullable> {
        // Native templates replace this typed null with their payload argument.
        // A raw ProgramInstance never needs the template's source image.
        function.native_program_payload_slot();
        function.instruction(&Instruction::RefNull(
            schema
                .reference_type::<NativeSourcePayloadArray>(GcNullability::Nullable)
                .heap_type,
        ));
        schema
            .reserve_gc_local(function)
            .initialize(GcStackReference::new(), function)
    }

    pub(crate) const fn native_payload_field() -> u32 {
        ProgramInstanceSchema::NATIVE_SOURCE_PAYLOAD.ordinal().raw()
    }
    fn native_context_schema(schema: &RuntimeSchema) -> crate::native_program_contexts::Schema {
        crate::native_program_contexts::Schema {
            program_type: schema.struct_type::<ProgramInstance>().type_index().raw(),
            payload_type: schema
                .array_type::<NativeSourcePayloadArray>()
                .type_index()
                .raw(),
            payload_field: ProgramInstanceSchema::NATIVE_SOURCE_PAYLOAD.ordinal().raw(),
        }
    }

    pub(crate) fn mark_native_context(&self, schema: &RuntimeSchema, function: &mut Function) {
        function.bind_native_program_context(Self::native_context_schema(schema), self.owner.index);
    }

    pub(crate) fn mark_native_bootstrap(schema: &RuntimeSchema, function: &mut Function) {
        function.bind_native_bootstrap_context(Self::native_context_schema(schema));
    }
}
