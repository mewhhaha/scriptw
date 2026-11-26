//! CreateArrayFromList for a native producer whose List stays private until
//! it is returned. The producer appends each value once into the very
//! PackedValues backing its Array then owns; publication consumes the sole
//! write owner into one actual Array, with no per-element definition.
use super::*;

/// The capacity the first growth of an empty packed backing reaches; it
/// doubles from there, so every capacity is a power of two covering the
/// count, which the packed owners' own growth would have reached.
const FIRST_PACKED_VALUE_LIST_CAPACITY: i32 = 4;

#[must_use = "a packed value list must be published as its Array"]
pub(crate) struct PackedValueListConstruction {
    values: GcLocal<PackedValueTable>,
    length: I32Local,
}

impl PackedValueListConstruction {
    pub(crate) fn allocate(schema: &RuntimeSchema, function: &mut Function) -> Self {
        let length = schema.reserve_i32_local(function);
        length.set_constant(FIRST_PACKED_VALUE_LIST_CAPACITY, function);
        let values = schema.reserve_gc_local(function).initialize(
            schema.array_type::<PackedValueTable>().filled(
                GcOperand::null(schema),
                length,
                function,
            ),
            function,
        );
        length.set_constant(0, function);
        Self { values, length }
    }

    /// Allocates an exact 1-element packed list containing one present data
    /// value, ready for immediate publication as a singleton Array.
    pub(crate) fn singleton(
        value: &ValueLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> Self {
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(value, function),
            function,
        );
        let values = schema.reserve_gc_local(function).initialize(
            schema
                .array_type::<PackedValueTable>()
                .fixed([GcOperand::nullable_reference(&stored, schema)], function),
            function,
        );
        stored.clear(function);
        let length = schema.reserve_i32_local(function);
        length.set_constant(1, function);
        Self { values, length }
    }

    /// Appends one present ordinary data value. Slots past the count stay
    /// null, which is a hole, so publication needs no fill or copy.
    pub(crate) fn append(
        &self,
        value: &ValueLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let table = schema.array_type::<PackedValueTable>();
        let capacity = schema.reserve_i32_local(function);
        table.length(&self.values, schema, function);
        capacity.store(function);
        self.length.load(function);
        capacity.load(function);
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::If(BlockType::Empty));
        // A capacity past i32 would exceed every engine array limit; the
        // allocation below traps before any value could be lost.
        capacity.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Shl);
        capacity.store(function);
        let grown = schema.reserve_gc_local(function).initialize(
            table.filled(GcOperand::null(schema), capacity, function),
            function,
        );
        table.copy_prefix_from(&grown, &self.values, self.length, schema, function);
        self.values.replace(grown.load(schema, function), function);
        grown.clear(function);
        function.instruction(&Instruction::End);
        schema.release_i32_local(capacity, function);
        let stored = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(value, function),
            function,
        );
        table.write(
            &self.values,
            self.length,
            GcOperand::nullable_reference(&stored, schema),
            schema,
            function,
        );
        stored.clear(function);
        self.length.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        self.length.store(function);
    }
}

impl GcStructType<ArrayObject> {
    /// ArrayCreate(count) followed by CreateDataPropertyOrThrow of every
    /// list element at its ordinal: a fresh extensible Array with a writable
    /// `length` equal to the count and default W/E/C data elements, which is
    /// exactly the PackedValues state over this backing with COUNT = length.
    pub(crate) fn with_packed_value_list(
        self,
        header: &GcLocal<OrdinaryObject>,
        list: PackedValueListConstruction,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<ArrayObject> {
        let PackedValueListConstruction { values, length } = list;
        let count = schema.reserve_i64_local(function);
        length.load(function);
        function.instruction(&Instruction::I64ExtendI32U);
        count.store(function);
        schema.release_i32_local(length, function);
        let buckets = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_arrays.array_buckets.load(function), function);
        let dense = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_array_elements(function), function);
        let numbers = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_number_bits(function), function);
        let presence = schema
            .reserve_gc_local(function)
            .initialize(schema.empty_number_presence(function), function);
        let result = self.construct(
            (
                GcOperand::reference(header, schema),
                GcOperand::boolean(true),
                GcOperand::i64_local(count),
                GcOperand::i64_local(count),
                GcOperand::reference(&buckets, schema),
                GcOperand::reference(&dense, schema),
                GcOperand::constant(ArrayIndexedStorageMode::PackedValues),
                GcOperand::reference(&numbers, schema),
                GcOperand::reference(&presence, schema),
                GcOperand::nullable_reference(&values, schema),
            ),
            function,
        );
        dense.clear(function);
        presence.clear(function);
        numbers.clear(function);
        buckets.clear(function);
        values.clear(function);
        schema.release_i64_local(count, function);
        result
    }
}
