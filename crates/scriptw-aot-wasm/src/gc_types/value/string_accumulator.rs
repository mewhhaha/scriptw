//! A growable UTF-16 buffer owned by one serialization. Its code units are
//! writable only through this cursor; publication always copies the used
//! prefix, so no published String ever aliases still-writable storage.
use super::*;

/// The live units and used length of one accumulator, held in locals between
/// two calls that may append to the same accumulator. `close` writes the
/// length back; growth writes the replacement backing immediately.
#[must_use = "an accumulator cursor must write its length back"]
pub(crate) struct StringAccumulatorCursor {
    units: GcLocal<CodeUnitArray>,
    length: I32Local,
}

impl StringAccumulatorCursor {
    /// A fresh empty accumulator with `capacity` spare code units.
    pub(crate) fn allocate(
        schema: &RuntimeSchema,
        capacity: i32,
        function: &mut Function,
    ) -> GcStackReference<StringAccumulator> {
        assert!(capacity >= 0, "accumulator capacity is nonnegative");
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32Const(capacity));
        function.instruction(&Instruction::ArrayNew(
            schema.array_type::<CodeUnitArray>().type_index().raw(),
        ));
        function.instruction(&Instruction::I32Const(0));
        schema.struct_type::<StringAccumulator>().emit_new(function);
        GcStackReference::new()
    }

    pub(crate) fn open(
        owner: &GcLocal<StringAccumulator>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> Self {
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .field(StringAccumulatorSchema::UNITS)
                .read(owner, schema, function)
                .reference(),
            function,
        );
        let length = schema.reserve_i32_local(function);
        schema
            .field(StringAccumulatorSchema::LENGTH)
            .read(owner, schema, function)
            .store(length, function);
        Self { units, length }
    }

    /// The used length, usable as a later `truncate` mark.
    pub(crate) fn length(&self) -> I32Local {
        self.length
    }

    /// Forget units appended after `mark`, an earlier `length` of this buffer.
    pub(crate) fn truncate(&self, mark: I32Local, function: &mut Function) {
        mark.load(function);
        self.length.load(function);
        function.instruction(&Instruction::I32GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        mark.load(function);
        self.length.store(function);
    }

    /// Guarantee room for `extra` more units after the used length.
    pub(crate) fn reserve(
        &self,
        owner: &GcLocal<StringAccumulator>,
        extra: I32Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let array = schema.array_type::<CodeUnitArray>();
        let needed = schema.reserve_i32_local(function);
        let capacity = schema.reserve_i32_local(function);
        self.length.load(function);
        extra.load(function);
        function.instruction(&Instruction::I32Add);
        needed.store(function);
        // Lengths stay within the nonnegative I32 GC array domain.
        needed.load(function);
        self.length.load(function);
        function.instruction(&Instruction::I32LtU);
        needed.load(function);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32LtS);
        function.instruction(&Instruction::I32Or);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        array.length(&self.units, schema, function);
        capacity.store(function);
        needed.load(function);
        capacity.load(function);
        function.instruction(&Instruction::I32GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        // Double, or grow exactly to the requirement when doubling is short
        // of it or leaves the I32 domain.
        capacity.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Shl);
        capacity.store(function);
        capacity.load(function);
        needed.load(function);
        function.instruction(&Instruction::I32LtS);
        function.instruction(&Instruction::If(BlockType::Empty));
        needed.load(function);
        capacity.store(function);
        function.instruction(&Instruction::End);
        let grown = schema.reserve_gc_local(function).initialize(
            array.filled(GcOperand::i32(0), capacity, function),
            function,
        );
        emit_code_unit_copy(
            &grown,
            None,
            &self.units,
            None,
            self.length,
            schema,
            function,
        );
        self.units.replace(grown.load(schema, function), function);
        grown.clear(function);
        schema.field(StringAccumulatorSchema::UNITS).write(
            owner,
            GcOperand::reference(&self.units, schema),
            schema,
            function,
        );
        function.instruction(&Instruction::End);
        schema.release_i32_local(capacity, function);
        schema.release_i32_local(needed, function);
    }

    /// Append one unit to room already secured by `reserve`.
    pub(crate) fn push(&self, unit: I32Local, schema: &RuntimeSchema, function: &mut Function) {
        self.units.load(schema, function);
        self.length.load(function);
        unit.load(function);
        schema.array_type::<CodeUnitArray>().emit_set(function);
        self.length.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        self.length.store(function);
    }

    /// Append compiler punctuation such as JSON brackets without a String.
    pub(crate) fn push_literal(
        &self,
        owner: &GcLocal<StringAccumulator>,
        text: &str,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let units: Vec<u16> = text.encode_utf16().collect();
        let extra = schema.reserve_i32_local(function);
        extra.set_constant(
            i32::try_from(units.len()).expect("compiler literal fits the Wasm string domain"),
            function,
        );
        self.reserve(owner, extra, schema, function);
        schema.release_i32_local(extra, function);
        let array = schema.array_type::<CodeUnitArray>();
        for (offset, unit) in units.iter().enumerate() {
            self.units.load(schema, function);
            self.length.load(function);
            if offset != 0 {
                function.instruction(&Instruction::I32Const(
                    i32::try_from(offset).expect("compiler literal offset fits"),
                ));
                function.instruction(&Instruction::I32Add);
            }
            function.instruction(&Instruction::I32Const(i32::from(*unit)));
            array.emit_set(function);
        }
        self.length.load(function);
        function.instruction(&Instruction::I32Const(
            i32::try_from(units.len()).expect("compiler literal fits the Wasm string domain"),
        ));
        function.instruction(&Instruction::I32Add);
        self.length.store(function);
    }

    /// Append every unit of a published String.
    pub(crate) fn append_string(
        &self,
        owner: &GcLocal<StringAccumulator>,
        string: &GcLocal<StringValue>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let source = schema.reserve_gc_local(function).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(string, schema, function)
                .reference(),
            function,
        );
        let count = schema.reserve_i32_local(function);
        schema
            .array_type::<CodeUnitArray>()
            .length(&source, schema, function);
        count.store(function);
        self.reserve(owner, count, schema, function);
        emit_code_unit_copy(
            &self.units,
            Some(self.length),
            &source,
            None,
            count,
            schema,
            function,
        );
        self.length.load(function);
        count.load(function);
        function.instruction(&Instruction::I32Add);
        self.length.store(function);
        schema.release_i32_local(count, function);
        source.clear(function);
    }

    /// Repeat a published String `count` times (JSON indentation).
    pub(crate) fn append_repeated_string(
        &self,
        owner: &GcLocal<StringAccumulator>,
        string: &GcLocal<StringValue>,
        count: I64Local,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let index = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(0));
        index.store(function);
        function.instruction(&Instruction::Block(BlockType::Empty));
        function.instruction(&Instruction::Loop(BlockType::Empty));
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I64GeU);
        function.instruction(&Instruction::BrIf(1));
        self.append_string(owner, string, schema, function);
        index.load(function);
        function.instruction(&Instruction::I64Const(1));
        function.instruction(&Instruction::I64Add);
        index.store(function);
        function.instruction(&Instruction::Br(0));
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::End);
        schema.release_i64_local(index, function);
    }

    /// Write the used length back; the cursor's locals are released.
    pub(crate) fn close(
        self,
        owner: &GcLocal<StringAccumulator>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        schema.field(StringAccumulatorSchema::LENGTH).write(
            owner,
            GcOperand::i32_local(self.length),
            schema,
            function,
        );
        schema.release_i32_local(self.length, function);
        self.units.clear(function);
    }

    /// Copy the used prefix into a new immutable String. The accumulator
    /// keeps its own backing, which is never published.
    pub(crate) fn publish(
        owner: &GcLocal<StringAccumulator>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcStackReference<StringValue> {
        let cursor = Self::open(owner, schema, function);
        let exact = StringConstruction::allocate(
            schema,
            schema.reserve_gc_local(function),
            cursor.length,
            function,
        );
        emit_code_unit_copy(
            &exact.units,
            None,
            &cursor.units,
            None,
            cursor.length,
            schema,
            function,
        );
        let result = schema
            .reserve_gc_local(function)
            .initialize(exact.publish(schema, function), function);
        schema.release_i32_local(cursor.length, function);
        cursor.units.clear(function);
        let published = result.load(schema, function);
        result.clear(function);
        published
    }
}
