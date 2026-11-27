//! Ordinary data slots share immutable StoredValues; null alone represents a hole.
use super::*;
use crate::gc_types::{PackedValueTable, StoredValue, StoredValueSchema};

/// The capacity that the first growth of an empty packed backing reaches.
const FIRST_PACKED_CAPACITY: i32 = 4;
/// The longest array whose values backing is born covering every index below
/// its length. A backing's capacity is always a power of two, as doubling
/// from [`FIRST_PACKED_CAPACITY`] keeps it and the rooted snapshot requires.
const BORN_VALUES_LIMIT: i64 = 64;

// Existing value reads keep their exact emitted path. Descriptor observations
// request an immutable edge, reusing PackedValues and boxing only Number bits.
#[derive(Clone, Copy)]
enum PackedArrayProbeResult<'a> {
    Value {
        output: Option<&'a ValueLocals>,
        found: I32Local,
    },
    Stored(&'a GcLocal<StoredValue, Nullable>),
}

#[derive(Clone, Copy)]
pub(super) enum PackedArrayWriteValue<'a> {
    Raw(&'a ValueLocals),
    Stored(&'a GcLocal<StoredValue>),
}

impl FunctionBuilder<'_> {
    fn packed_values(
        &self,
        storage: &GcLocal<ArrayObject>,
        f: &mut Function,
    ) -> GcLocal<PackedValueTable> {
        let s = self.runtime_schema();
        s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::VALUES)
                .read(storage, s, f)
                .reference()
                .require_non_null(f),
            f,
        )
    }

    pub(super) fn emit_packed_array_probe(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        output: Option<&ValueLocals>,
        found: I32Local,
        f: &mut Function,
    ) {
        self.emit_packed_array_probe_result(
            storage,
            index,
            PackedArrayProbeResult::Value { output, found },
            f,
        );
    }

    pub(in crate::objects) fn emit_packed_array_read_descriptor(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        f: &mut Function,
    ) -> GcLocal<PropertyDescriptor, Nullable> {
        let s = self.runtime_schema();
        let stored = s
            .reserve_gc_local::<StoredValue, Nullable>(f)
            .initialize_null(s, f);
        let output = s
            .reserve_gc_local::<PropertyDescriptor, Nullable>(f)
            .initialize_null(s, f);
        self.emit_packed_array_probe_result(
            storage,
            index,
            PackedArrayProbeResult::Stored(&stored),
            f,
        );
        stored.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let present = s
            .reserve_gc_local(f)
            .initialize(stored.load(s, f).require_non_null(f), f);
        let absent = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        output.replace(
            s.struct_type::<PropertyDescriptor>()
                .construct(
                    (
                        GcOperand::descriptor_word(
                            StoredPropertyAttributes::Data {
                                writable: true,
                                enumerable: true,
                                configurable: true,
                            }
                            .descriptor_word(),
                        ),
                        GcOperand::subtype_reference(&present, s),
                        GcOperand::reference(&absent, s),
                        GcOperand::reference(&absent, s),
                    ),
                    f,
                )
                .nullable(),
            f,
        );
        absent.clear(f);
        present.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        stored.clear(f);
        output
    }

    fn emit_packed_array_probe_result(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        result: PackedArrayProbeResult<'_>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedValues, f);
        self.open_frame(ControlFrameKind::If, f);
        let values = self.packed_values(storage, f);
        index.load(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        let slot = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let value = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .read(&values, slot, s, f)
                .reference(),
            f,
        );
        value.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        match result {
            PackedArrayProbeResult::Value { output, found } => {
                if let Some(output) = output {
                    let present = s
                        .reserve_gc_local(f)
                        .initialize(value.load(s, f).require_non_null(f), f);
                    s.struct_type::<StoredValue>()
                        .read_into(&present, output, s, f);
                    present.clear(f);
                }
                found.set_constant(1, f);
            }
            PackedArrayProbeResult::Stored(output) => {
                output.replace(value.load(s, f), f);
            }
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.clear(f);
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        values.clear(f);
        f.instruction(&Instruction::Else);
        match result {
            PackedArrayProbeResult::Value { output, found } => {
                self.emit_packed_number_probe(storage, index, output, found, f);
            }
            PackedArrayProbeResult::Stored(output) => {
                let value = s.reserve_value_local(f);
                let found = s.reserve_i32_local(f);
                found.set_constant(0, f);
                self.emit_packed_number_probe(storage, index, Some(&value), found, f);
                found.load(f);
                self.open_frame(ControlFrameKind::If, f);
                output.replace(
                    s.struct_type::<StoredValue>()
                        .from_value(&value, f)
                        .nullable(),
                    f,
                );
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                s.release_i32_local(found, f);
                value.clear(f);
            }
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    /// The publication of default data at `index` when the packed backing
    /// has room for it without a copy, leaving whether it was made (false
    /// having changed nothing). An empty array's first element may create
    /// that backing: four slots as the first growth of either kind does, or,
    /// for a value that is not a Number and an index below a length of at
    /// most [`BORN_VALUES_LIMIT`], the least power-of-two capacity covering
    /// that length, so a fixed literal publishes its elements into one table
    /// that growth by doubling would have reached. The caller
    /// owns CreateDataProperty semantics exactly as for the complete
    /// publication, which keeps growth, promotion and descriptor backings.
    pub(super) fn emit_packed_array_store_within_capacity(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        value: &ValueLocals,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        let slot = s.reserve_i32_local(f);
        let capacity = s.reserve_i32_local(f);
        let count = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::COUNT)
            .read(storage, s, f)
            .store_i64(count, f);
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedNumbers, f);
        self.open_frame(ControlFrameKind::If, f);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let (numbers, presence) = self.packed_backings(storage, f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        capacity.store(f);
        capacity.load(f);
        f.instruction(&Instruction::I32Eqz);
        index.load(f);
        f.instruction(&Instruction::I64Const(i64::from(FIRST_PACKED_CAPACITY)));
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        capacity.set_constant(FIRST_PACKED_CAPACITY, f);
        let new_numbers = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberBitsI64Array>()
                .filled(GcOperand::i64(0), capacity, f),
            f,
        );
        // One presence word covers the first capacity.
        slot.set_constant(1, f);
        let new_presence = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberPresenceU64Array>()
                .filled(GcOperand::i64(0), slot, f),
            f,
        );
        s.field(ArrayObjectSchema::NUMBERS).write(
            storage,
            GcOperand::reference(&new_numbers, s),
            s,
            f,
        );
        s.field(ArrayObjectSchema::NUMBER_PRESENT).write(
            storage,
            GcOperand::reference(&new_presence, s),
            s,
            f,
        );
        numbers.replace(new_numbers.load(s, f), f);
        presence.replace(new_presence.load(s, f), f);
        new_presence.clear(f);
        new_numbers.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        index.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let word = s.reserve_i32_local(f);
        let mask = s.reserve_i64_local(f);
        let bits = s.reserve_i64_local(f);
        self.packed_presence(&presence, slot, word, mask, bits, f);
        bits.load(f);
        mask.load(f);
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        count.store(f);
        s.field(ArrayObjectSchema::COUNT)
            .write(storage, GcOperand::i64_local(count), s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.array_type::<NumberBitsI64Array>().write(
            &numbers,
            slot,
            GcOperand::i64_local(value.scalar()),
            s,
            f,
        );
        bits.load(f);
        mask.load(f);
        f.instruction(&Instruction::I64Or);
        bits.store(f);
        s.array_type::<NumberPresenceU64Array>().write(
            &presence,
            word,
            GcOperand::i64_local(bits),
            s,
            f,
        );
        handled.set_constant(1, f);
        s.release_i64_local(bits, f);
        s.release_i64_local(mask, f);
        s.release_i32_local(word, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        presence.clear(f);
        numbers.clear(f);
        f.instruction(&Instruction::Else);
        // No element exists, so the values backing is born holding none.
        capacity.set_constant(0, f);
        let length = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::LENGTH)
            .read(storage, s, f)
            .store_i64(length, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64LtU);
        length.load(f);
        f.instruction(&Instruction::I64Const(BORN_VALUES_LIMIT));
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        // The least power of two that covers the length, which doubling
        // from the first capacity would reach: 1 << (32 - clz(length - 1)),
        // and no less than that first capacity.
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Const(32));
        length.load(f);
        f.instruction(&Instruction::I32WrapI64);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Clz);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Shl);
        capacity.store(f);
        f.instruction(&Instruction::I32Const(FIRST_PACKED_CAPACITY));
        capacity.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32Const(FIRST_PACKED_CAPACITY));
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::Select);
        capacity.store(f);
        f.instruction(&Instruction::Else);
        index.load(f);
        f.instruction(&Instruction::I64Const(i64::from(FIRST_PACKED_CAPACITY)));
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        capacity.set_constant(FIRST_PACKED_CAPACITY, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(length, f);
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        capacity.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let born = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .filled(GcOperand::null(s), capacity, f),
            f,
        );
        self.clear_packed_backings(storage, f);
        s.field(ArrayObjectSchema::VALUES).write(
            storage,
            GcOperand::nullable_reference(&born, s),
            s,
            f,
        );
        s.set_array_storage_mode(storage, ArrayIndexedStorageMode::PackedValues, f);
        born.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedValues, f);
        self.open_frame(ControlFrameKind::If, f);
        let values = self.packed_values(storage, f);
        index.load(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let stored = self.emit_packed_value_for_store(value, f);
        s.array_type::<PackedValueTable>()
            .read(&values, slot, s, f)
            .reference()
            .is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        count.store(f);
        s.field(ArrayObjectSchema::COUNT)
            .write(storage, GcOperand::i64_local(count), s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.array_type::<PackedValueTable>().write(
            &values,
            slot,
            GcOperand::nullable_reference(&stored, s),
            s,
            f,
        );
        handled.set_constant(1, f);
        stored.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        values.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(count, f);
        s.release_i32_local(capacity, f);
        s.release_i32_local(slot, f);
        handled
    }

    /// Promote only an occupancy-bounded default-data publication. The old
    /// numeric backing remains rooted until all immutable values are allocated.
    fn promote_packed_numbers(&mut self, storage: &GcLocal<ArrayObject>, f: &mut Function) {
        let s = self.runtime_schema();
        let (numbers, presence) = self.packed_backings(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        capacity.store(f);
        let values = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .filled(GcOperand::null(s), capacity, f),
            f,
        );
        let slot = s.reserve_i32_local(f);
        slot.set_constant(0, f);
        let index = s.reserve_i64_local(f);
        let found = s.reserve_i32_local(f);
        let value = s.reserve_value_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        slot.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        slot.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        index.store(f);
        found.set_constant(0, f);
        self.emit_packed_number_probe(storage, index, Some(&value), found, f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&value, f), f);
        s.array_type::<PackedValueTable>().write(
            &values,
            slot,
            GcOperand::nullable_reference(&stored, s),
            s,
            f,
        );
        stored.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        slot.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        slot.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.clear_packed_backings(storage, f);
        s.field(ArrayObjectSchema::VALUES).write(
            storage,
            GcOperand::nullable_reference(&values, s),
            s,
            f,
        );
        s.set_array_storage_mode(storage, ArrayIndexedStorageMode::PackedValues, f);
        value.clear(f);
        s.release_i32_local(found, f);
        s.release_i64_local(index, f);
        s.release_i32_local(slot, f);
        s.release_i32_local(capacity, f);
        values.clear(f);
        presence.clear(f);
        numbers.clear(f);
    }

    /// With either packed mode proved, retain Number mode until a nonnumber
    /// value is admitted by the same capacity and occupancy policy.
    pub(super) fn emit_packed_array_write(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        value: PackedArrayWriteValue<'_>,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let packed_values = self.open_frame(ControlFrameKind::Block, f);
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedValues, f);
        self.emit_branch_if_to_target(packed_values, f);
        match value {
            PackedArrayWriteValue::Raw(value) => value.tag().load(f),
            PackedArrayWriteValue::Stored(value) => {
                s.field(StoredValueSchema::TAG).read(value, s, f);
            }
        }
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let bits = match value {
            PackedArrayWriteValue::Raw(value) => value.scalar(),
            PackedArrayWriteValue::Stored(value) => {
                let stored_bits = s.reserve_i64_local(f);
                s.field(StoredValueSchema::SCALAR)
                    .read(value, s, f)
                    .store_i64(stored_bits, f);
                stored_bits
            }
        };
        self.emit_packed_number_write(storage, index, bits, handled, f);
        if matches!(value, PackedArrayWriteValue::Stored(_)) {
            s.release_i64_local(bits, f);
        }
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Promotion is useful only when the eventual write can remain packed.
        let (numbers, presence) = self.packed_backings(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        capacity.store(f);
        let count = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::COUNT)
            .read(storage, s, f)
            .store_i64(count, f);
        self.emit_packed_capacity_admission(index, capacity, count, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, f);
        self.promote_packed_numbers(storage, f);
        s.release_i64_local(count, f);
        s.release_i32_local(capacity, f);
        presence.clear(f);
        numbers.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.write_packed_value(storage, index, value, handled, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        handled
    }

    pub(super) fn emit_packed_capacity_admission(
        &self,
        index: I64Local,
        capacity: I32Local,
        count: I64Local,
        f: &mut Function,
    ) {
        index.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        index.load(f);
        f.instruction(&Instruction::I64Const(PACKED_INDEX_LIMIT));
        f.instruction(&Instruction::I64LtU);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Const(8));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64LeU);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Or);
    }

    fn write_packed_value(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        value: PackedArrayWriteValue<'_>,
        handled: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let values = self.packed_values(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        capacity.store(f);
        let count = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::COUNT)
            .read(storage, s, f)
            .store_i64(count, f);
        self.emit_packed_capacity_admission(index, capacity, count, f);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        let grown = s.reserve_i32_local(f);
        capacity.load(f);
        grown.store(f);
        grown.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        grown.set_constant(4, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let ready = self.open_frame(ControlFrameKind::Block, f);
        let grow = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        grown.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        self.emit_branch_if_to_target(ready, f);
        grown.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        grown.store(f);
        self.emit_branch_to_target(grow, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let next = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .filled(GcOperand::null(s), grown, f),
            f,
        );
        s.array_type::<PackedValueTable>()
            .copy_prefix_from(&next, &values, capacity, s, f);
        s.field(ArrayObjectSchema::VALUES).write(
            storage,
            GcOperand::nullable_reference(&next, s),
            s,
            f,
        );
        values.replace(next.load(s, f), f);
        next.clear(f);
        s.release_i32_local(grown, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let slot = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let mut fresh = None;
        let edge = match value {
            PackedArrayWriteValue::Raw(value) => {
                fresh.insert(self.emit_packed_value_for_store(value, f))
            }
            PackedArrayWriteValue::Stored(value) => value,
        };
        let old = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .read(&values, slot, s, f)
                .reference(),
            f,
        );
        old.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        count.store(f);
        s.field(ArrayObjectSchema::COUNT)
            .write(storage, GcOperand::i64_local(count), s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.array_type::<PackedValueTable>().write(
            &values,
            slot,
            GcOperand::nullable_reference(edge, s),
            s,
            f,
        );
        handled.set_constant(1, f);
        old.clear(f);
        if let Some(fresh) = fresh {
            fresh.clear(f);
        }
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(count, f);
        s.release_i32_local(capacity, f);
        values.clear(f);
    }

    /// Packed slots use null for holes and share immutable edges for present
    /// values. Explicit undefined may use the canonical immutable edge.
    pub(super) fn emit_packed_value_for_store(
        &mut self,
        value: &ValueLocals,
        f: &mut Function,
    ) -> GcLocal<StoredValue> {
        let s = self.runtime_schema();
        let boxed = self.open_reference_block(s, f);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(
            WasmRuntimeValueTag::Undefined as i32,
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        boxed.branch_with(s.stored_undefined(f), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        boxed.branch_with(s.struct_type::<StoredValue>().from_value(value, f), f);
        let stored = s.reserve_gc_local(f).initialize(boxed.close(f), f);
        self.pop_control(ControlFrameKind::Block);
        stored
    }

    pub(in crate::objects) fn emit_materialize_packed_array(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedValues, f);
        self.open_frame(ControlFrameKind::If, f);
        let values = self.packed_values(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        capacity.store(f);
        let dense = s.reserve_gc_local(f).initialize(
            s.array_type::<IndexedTable>()
                .filled(GcOperand::null(s), capacity, f),
            f,
        );
        let slot = s.reserve_i32_local(f);
        slot.set_constant(0, f);
        let absent = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        slot.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        let stored = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .read(&values, slot, s, f)
                .reference(),
            f,
        );
        stored.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let present = s
            .reserve_gc_local(f)
            .initialize(stored.load(s, f).require_non_null(f), f);
        // Descriptor flags are mutable, but their value/accessor edges are
        // immutable snapshots and can share this exact already-rooted wrapper.
        let descriptor = s.reserve_gc_local(f).initialize(
            s.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::descriptor_word(
                        StoredPropertyAttributes::Data {
                            writable: true,
                            enumerable: true,
                            configurable: true,
                        }
                        .descriptor_word(),
                    ),
                    GcOperand::subtype_reference(&present, s),
                    GcOperand::reference(&absent, s),
                    GcOperand::reference(&absent, s),
                ),
                f,
            ),
            f,
        );
        present.clear(f);
        s.array_type::<IndexedTable>().write(
            &dense,
            slot,
            GcOperand::nullable_reference(&descriptor, s),
            s,
            f,
        );
        descriptor.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        stored.clear(f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        slot.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.field(ArrayObjectSchema::DENSE)
            .write(storage, GcOperand::reference(&dense, s), s, f);
        self.clear_packed_backings(storage, f);
        s.set_array_storage_mode(storage, ArrayIndexedStorageMode::DenseDescriptors, f);
        absent.clear(f);
        s.release_i32_local(slot, f);
        s.release_i32_local(capacity, f);
        dense.clear(f);
        values.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_materialize_packed_numbers(storage, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    pub(in crate::objects) fn emit_packed_array_delete(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedValues, f);
        handled.store(f);
        handled.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let values = self.packed_values(storage, f);
        index.load(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        let slot = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let old = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .read(&values, slot, s, f)
                .reference(),
            f,
        );
        old.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.array_type::<PackedValueTable>()
            .write(&values, slot, GcOperand::null(s), s, f);
        let count = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::COUNT)
            .read(storage, s, f)
            .store_i64(count, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        count.store(f);
        s.field(ArrayObjectSchema::COUNT)
            .write(storage, GcOperand::i64_local(count), s, f);
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        // No live values remain; release capacity and return to initial mode.
        self.clear_packed_backings(storage, f);
        s.set_array_storage_mode(storage, ArrayIndexedStorageMode::PackedNumbers, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(count, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        old.clear(f);
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        values.clear(f);
        f.instruction(&Instruction::Else);
        let numeric = self.emit_packed_number_delete(storage, index, f);
        numeric.load(f);
        handled.store(f);
        s.release_i32_local(numeric, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled
    }

    pub(in crate::objects) fn emit_packed_array_keys(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        keys: &ArrayIndexKeyConstruction,
        output: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedValues, f);
        self.open_frame(ControlFrameKind::If, f);
        let values = self.packed_values(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        capacity.store(f);
        let slot = s.reserve_i32_local(f);
        slot.set_constant(0, f);
        let index = s.reserve_i64_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        slot.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        let value = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>()
                .read(&values, slot, s, f)
                .reference(),
            f,
        );
        value.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        slot.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        index.store(f);
        keys.write(output, index, s, f);
        output.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        output.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.clear(f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        slot.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(index, f);
        s.release_i32_local(slot, f);
        s.release_i32_local(capacity, f);
        values.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_packed_number_keys(storage, keys, output, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }
}
