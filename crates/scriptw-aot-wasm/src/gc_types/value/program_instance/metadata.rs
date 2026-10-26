//! Frame-rooted reads from one actual P's immutable metadata and literal tables.

use super::*;
use crate::data::{
    ProgramMetadataSpan, POOLED_LITERAL_PROGRAM_DOMAIN, POOLED_LITERAL_RUNTIME_DOMAIN,
};

/// The owner, start and extent travel together. No linear load API is exposed.
pub(crate) struct ProgramMetadataView {
    bytes: GcLocal<ImmutableByteArray>,
    start: I32Local,
    length: I32Local,
}

impl ProgramInstanceLocals {
    pub(crate) fn metadata_span(
        &self,
        schema: &RuntimeSchema,
        span: ProgramMetadataSpan,
        function: &mut Function,
    ) -> ProgramMetadataView {
        let start = schema.reserve_i32_local(function);
        let length = schema.reserve_i32_local(function);
        start.set_constant(span.start() as i32, function);
        length.set_constant(span.length() as i32, function);
        ProgramMetadataView::bind(self, schema, start, length, function)
    }

    /// Only a checked Program-domain candidate row supplies this packed span.
    /// Bounds are checked against this actual P before publication reads bytes.
    pub(crate) fn metadata_payload(
        &self,
        schema: &RuntimeSchema,
        payload: I64Local,
        function: &mut Function,
    ) -> ProgramMetadataView {
        let start = schema.reserve_i32_local(function);
        let length = schema.reserve_i32_local(function);
        payload.load(function);
        function.instruction(&Instruction::I32WrapI64);
        length.store(function);
        payload.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I32WrapI64);
        start.store(function);
        ProgramMetadataView::bind(self, schema, start, length, function)
    }

    /// Embedded recipe/candidate operands identify a closed domain and an
    /// ordinal relative to that domain, never the old combined mutable table.
    pub(crate) fn literal_operand(
        &self,
        schema: &RuntimeSchema,
        operand: I64Local,
        base: crate::runtime_helpers::RuntimeHelperFunctionBase,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        let ordinal = schema.reserve_i32_local(function);
        let domain = schema.reserve_i32_local(function);
        operand.load(function);
        function.instruction(&Instruction::I32WrapI64);
        ordinal.store(function);
        operand.load(function);
        function.instruction(&Instruction::I64Const(32));
        function.instruction(&Instruction::I64ShrU);
        function.instruction(&Instruction::I32WrapI64);
        domain.store(function);
        let string = schema
            .reserve_gc_local::<StringValue, Nullable>(function)
            .initialize_null(schema, function);
        domain.load(function);
        function.instruction(&Instruction::I32Const(POOLED_LITERAL_RUNTIME_DOMAIN as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        let runtime_limit = schema.reserve_i32_local(function);
        schema
            .struct_type::<ProgramInstance>()
            .field(ProgramInstanceSchema::LITERAL_BASE)
            .read(&self.owner, schema, function)
            .store(runtime_limit, function);
        ordinal.load(function);
        runtime_limit.load(function);
        function.instruction(&Instruction::I32GeU);
        trap_if_true(function);
        schema.release_i32_local(runtime_limit, function);
        string.replace(
            schema
                .resolve_pooled_string_reference(ordinal, base, function)
                .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        domain.load(function);
        function.instruction(&Instruction::I32Const(POOLED_LITERAL_PROGRAM_DOMAIN as i32));
        function.instruction(&Instruction::I32Ne);
        trap_if_true(function);
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<ProgramInstance>()
                .field(ProgramInstanceSchema::LITERAL_STRINGS)
                .read(&self.owner, schema, function)
                .reference(),
            function,
        );
        ordinal.load(function);
        schema
            .array_type::<PooledStringTable>()
            .length(&table, schema, function);
        function.instruction(&Instruction::I32GeU);
        trap_if_true(function);
        string.replace(
            schema
                .array_type::<PooledStringTable>()
                .read(&table, ordinal, schema, function)
                .reference(),
            function,
        );
        table.clear(function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(domain, function);
        schema.release_i32_local(ordinal, function);
        let result = string.load(schema, function).require_non_null(function);
        string.clear(function);
        result
    }
}

impl ProgramMetadataView {
    fn bind(
        owner: &ProgramInstanceLocals,
        schema: &RuntimeSchema,
        start: I32Local,
        length: I32Local,
        function: &mut Function,
    ) -> Self {
        let bytes = schema
            .reserve_gc_local(function)
            .initialize(owner.metadata(schema, function), function);
        // Unsigned extension makes start+length nonwrapping even at u32::MAX.
        start.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        length.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Add);
        schema
            .array_type::<ImmutableByteArray>()
            .length(&bytes, schema, function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64GtU);
        trap_if_true(function);
        Self {
            bytes,
            start,
            length,
        }
    }

    pub(crate) fn length(&self, function: &mut Function) {
        self.length.load(function);
    }

    fn checked_index(
        &self,
        cursor: I32Local,
        offset: u32,
        width: u32,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> I32Local {
        cursor.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64Const(i64::from(offset) + i64::from(width)));
        function.instruction(&Instruction::I64Add);
        self.length.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        function.instruction(&Instruction::I64GtU);
        trap_if_true(function);
        let index = schema.reserve_i32_local(function);
        self.start.load(function);
        cursor.load(function);
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::I32Const(offset as i32));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        index
    }

    pub(crate) fn read_byte(
        &self,
        cursor: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let index = self.checked_index(cursor, 0, 1, schema, function);
        let byte = schema.reserve_i32_local(function);
        schema
            .array_type::<ImmutableByteArray>()
            .read(&self.bytes, index, schema, function)
            .store(byte, function);
        byte.load(function);
        schema.release_i32_local(byte, function);
        schema.release_i32_local(index, function);
    }

    fn read_word(
        &self,
        cursor: I32Local,
        offset: u32,
        width: u32,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let index = self.checked_index(cursor, offset, width, schema, function);
        let byte = schema.reserve_i32_local(function);
        let result = schema.reserve_i64_local(function);
        result.set_constant(0, function);
        for part in 0..width {
            schema
                .array_type::<ImmutableByteArray>()
                .read(&self.bytes, index, schema, function)
                .store(byte, function);
            result.load(function);
            byte.load(function);
            function.instruction(&Instruction::I64ExtendI32U);
            if part != 0 {
                function.instruction(&Instruction::I64Const(i64::from(part * 8)));
                function.instruction(&Instruction::I64Shl);
            }
            function.instruction(&Instruction::I64Or);
            result.store(function);
            if part + 1 != width {
                index.load(function);
                function.instruction(&Instruction::I32Const(1));
                function.instruction(&Instruction::I32Add);
                index.store(function);
            }
        }
        result.load(function);
        schema.release_i64_local(result, function);
        schema.release_i32_local(byte, function);
        schema.release_i32_local(index, function);
    }

    pub(crate) fn read_u32(
        &self,
        cursor: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.read_word(cursor, 0, 4, schema, function);
        function.instruction(&Instruction::I32WrapI64);
    }

    pub(crate) fn read_u64(
        &self,
        cursor: I32Local,
        offset: u32,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.read_word(cursor, offset, 8, schema, function);
    }

    pub(crate) fn clear(self, schema: &RuntimeSchema, function: &mut Function) {
        schema.release_i32_local(self.length, function);
        schema.release_i32_local(self.start, function);
        self.bytes.clear(function);
    }
}
