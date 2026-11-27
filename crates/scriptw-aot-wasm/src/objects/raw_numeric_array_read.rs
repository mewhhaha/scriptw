//! A raw Number key can read a concrete Array's present packed own data without
//! allocating its decimal property name. Every other case keeps the original
//! lookup, receiver and raw key for the caller's complete property operation.
//! The same key inside a primitive String reads its one code unit directly.
use super::*;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, CodeUnitArray, GcNullability, StringConstruction, StringValue,
    StringValueSchema,
};

impl FunctionBuilder<'_> {
    /// Inline numeric-key probe for `PropertyRead` (`compile_ordinary_property_read_from_locals`):
    /// mirrors `GetV` and `compile_callee_to_value` by evaluating `index_expr` once
    /// (keeping `Int32Expression` bits unboxed until a miss), probing packed Array
    /// elements and primitive String code units directly, and branching to `done`
    /// on a hit without calling `DynamicPropertyRead`.
    ///
    /// Returns `Ok(true)` when `key_value` has been evaluated (and any hit already
    /// branched to `done`), or `Ok(false)` when the key was not evaluated and the
    /// caller should compile it normally.
    pub(super) fn try_emit_inline_indexed_property_read(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        receiver: &ValueLocals,
        key_value: &ValueLocals,
        pending: &CompletionLocals,
        done: crate::emit::ControlTarget,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let (PropertyKeyIr::ArrayIndex(index_expr) | PropertyKeyIr::StringExpr(index_expr)) = key
        else {
            return Ok(false);
        };
        let may_be_array = target.possible_kinds.contains(ValueKind::Array);
        let may_be_string = target.possible_kinds.contains(ValueKind::String);
        if !index_expr.possible_kinds.contains(ValueKind::Number)
            || (!may_be_array && !may_be_string)
        {
            return Ok(false);
        }
        let schema = self.runtime_schema();
        if may_be_array {
            if let Some(int32_key) = crate::operations::Int32Expression::of(index_expr) {
                let bits = schema.reserve_i32_local(function);
                self.compile_int32_bits(&int32_key, function)?;
                bits.store(function);
                let signedness = int32_key.signedness();
                let found = self
                    .emit_int32_array_own_data_read(receiver, bits, signedness, pending, function);
                found.load(function);
                schema.release_i32_local(found, function);
                self.emit_branch_if_to_target(done, function);
                // Materialize the Number key once on miss for String probing and
                // the fallback DynamicPropertyRead.
                bits.load(function);
                signedness.emit_to_f64(function);
                function.instruction(&Instruction::I64ReinterpretF64);
                key_value.scalar().store(function);
                key_value.set_number(key_value.scalar(), function);
                schema.release_i32_local(bits, function);
                if may_be_string {
                    let character = self
                        .emit_raw_numeric_string_index_read(receiver, key_value, pending, function);
                    character.load(function);
                    schema.release_i32_local(character, function);
                    self.emit_branch_if_to_target(done, function);
                }
                return Ok(true);
            }
        }
        self.compile_expr_to_value(index_expr, key_value, function)?;
        if may_be_array {
            let found =
                self.emit_raw_numeric_array_own_data_read(receiver, key_value, pending, function);
            found.load(function);
            schema.release_i32_local(found, function);
            self.emit_branch_if_to_target(done, function);
        }
        if may_be_string {
            let character =
                self.emit_raw_numeric_string_index_read(receiver, key_value, pending, function);
            character.load(function);
            schema.release_i32_local(character, function);
            self.emit_branch_if_to_target(done, function);
        }
        Ok(true)
    }

    pub(crate) fn emit_raw_numeric_array_own_data_read(
        &mut self,
        lookup: &ValueLocals,
        raw_key: &ValueLocals,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let found = s.reserve_i32_local(f);
        found.set_constant(0, f);
        raw_key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Eq);
        // Most named reads (notably TypedArray length) cannot use this probe.
        // Avoid their otherwise redundant GC subtype test entirely.
        self.open_frame(ControlFrameKind::If, f);
        lookup.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        // The unsigned range and round trip admit exactly array indices below
        // 2^32 - 1. NaN, infinities, negatives and fractions fail; numeric -0
        // equals +0.0 and names index 0 (distinct String "-0" never enters here).
        let index = s.reserve_i64_local(f);
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncSatF64S);
        index.store(f);
        index.load(f);
        f.instruction(&Instruction::I64Const(i64::from(u32::MAX)));
        f.instruction(&Instruction::I64LtU);
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        index.load(f);
        f.instruction(&Instruction::F64ConvertI64S);
        f.instruction(&Instruction::F64Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_packed_array_own_data_at(lookup, index, result, found, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        found
    }

    /// The same probe for a raw key that is an int32-domain Number given by
    /// its 32 bits: a nonnegative signed value, or an unsigned value other
    /// than 2^32-1, is an array index as it stands. No binary64 conversion,
    /// range test or integrality test is needed.
    pub(crate) fn emit_int32_array_own_data_read(
        &mut self,
        lookup: &ValueLocals,
        bits: I32Local,
        signedness: crate::operations::Int32Signedness,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let found = s.reserve_i32_local(f);
        found.set_constant(0, f);
        lookup.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        bits.load(f);
        match signedness {
            crate::operations::Int32Signedness::Signed => {
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::I32GeS);
            }
            crate::operations::Int32Signedness::Unsigned => {
                f.instruction(&Instruction::I32Const(-1));
                f.instruction(&Instruction::I32Ne);
            }
        }
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let index = s.reserve_i64_local(f);
        bits.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        index.store(f);
        self.emit_packed_array_own_data_at(lookup, index, result, found, f);
        s.release_i64_local(index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        found
    }

    /// `lookup` is an actual Array and `index` an array index below 2^32-1.
    fn emit_packed_array_own_data_at(
        &mut self,
        lookup: &ValueLocals,
        index: I64Local,
        result: &CompletionLocals,
        found: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let array = s
            .reserve_gc_local(f)
            .initialize(lookup.cast_reference::<ArrayObject>(s, f), f);
        let storage = self.emit_array_index_storage(&array, f);
        s.array_storage_is_packed(&storage, f);
        self.open_frame(ControlFrameKind::If, f);
        let value = s.reserve_value_local(f);
        // The packed probe never allocates, invokes user code or materializes
        // descriptors. A present undefined is distinct from an absent slot.
        self.emit_packed_array_value(&storage, index, &value, found, f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        result.set_normal(&value, f);
        f.instruction(&Instruction::Else);
        // Out-of-bounds read on a dense ArrayObject (COUNT == LENGTH,
        // index >= LENGTH, index < 2^32 - 1): by ECMA-262 10.4.2.4 an Array
        // cannot hold an own array-index property at or beyond its length, so
        // when the prototype chain is the protected %Array.prototype% (no own
        // indices) -> %Object.prototype% (unchanged intrinsic rows, immutable
        // null prototype), the read is guaranteed to return undefined without
        // observable effects or decimal key formatting.
        let miss = self.open_frame(ControlFrameKind::Block, f);
        let length = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::LENGTH)
            .read(&array, s, f)
            .store_i64(length, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64LtU);
        s.field(ArrayObjectSchema::COUNT).read(&array, s, f);
        length.load(f);
        f.instruction(&Instruction::I64Ne);
        f.instruction(&Instruction::I32Or);
        s.release_i64_local(length, f);
        self.emit_branch_if_to_target(miss, f);
        let header = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference(),
            f,
        );
        let prototype = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, s, f)
                .reference(),
            f,
        );
        header.clear(f);
        self.emit_array_prototype_chain_without_indices_or_branch(&prototype, miss, f);
        prototype.clear(f);
        value.set_undefined(f);
        result.set_normal(&value, f);
        found.set_constant(1, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        storage.clear(f);
        array.clear(f);
    }

    /// A raw Number key inside a primitive String reads that String's own
    /// index property, a String of the one code unit, without allocating the
    /// wrapper object ToObject would create or the decimal property name.
    /// The property is own data on every String object, so no other object
    /// or user code is consulted. Every other case is left to the caller.
    pub(crate) fn emit_raw_numeric_string_index_read(
        &mut self,
        lookup: &ValueLocals,
        raw_key: &ValueLocals,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let found = s.reserve_i32_local(f);
        found.set_constant(0, f);
        raw_key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Eq);
        lookup.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        // The unsigned range and round trip admit exactly array indices below
        // 2^32 - 1. NaN, infinities, negatives and fractions fail; numeric -0
        // equals +0.0 and names index 0.
        let index64 = s.reserve_i64_local(f);
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I64TruncSatF64S);
        index64.store(f);
        index64.load(f);
        f.instruction(&Instruction::I64Const(i64::from(u32::MAX)));
        f.instruction(&Instruction::I64LtU);
        raw_key.scalar().load(f);
        f.instruction(&Instruction::F64ReinterpretI64);
        index64.load(f);
        f.instruction(&Instruction::F64ConvertI64S);
        f.instruction(&Instruction::F64Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let index = s.reserve_i32_local(f);
        index64.load(f);
        f.instruction(&Instruction::I32WrapI64);
        index.store(f);
        let string = s
            .reserve_gc_local(f)
            .initialize(lookup.cast_reference::<StringValue>(s, f), f);
        let units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(&string, s, f)
                .reference(),
            f,
        );
        string.clear(f);
        index.load(f);
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        let unit = s.reserve_i32_local(f);
        s.array_type::<CodeUnitArray>()
            .read(&units, index, s, f)
            .store(unit, f);
        let construction = StringConstruction::from_code_unit(s, s.reserve_gc_local(f), unit, f);
        let character = s
            .reserve_gc_local(f)
            .initialize(construction.publish(s, f), f);
        let value = s.reserve_value_local(f);
        value.set_reference(&character, s, f);
        result.set_normal(&value, f);
        found.set_constant(1, f);
        value.clear(f);
        character.clear(f);
        s.release_i32_local(unit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        units.clear(f);
        s.release_i32_local(index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(index64, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        found
    }
}
