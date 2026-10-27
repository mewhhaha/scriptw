//! Ordinary data slots share immutable StoredValues; null alone represents a hole.
use super::*;
use crate::gc_types::{PackedValueTable, StoredValue};

impl FunctionBuilder<'_> {
    fn packed_values(
        &self,
        storage: &GcLocal<ArrayIndexStorage>,
        f: &mut Function,
    ) -> GcLocal<PackedValueTable> {
        let s = self.runtime_schema();
        s.reserve_gc_local(f).initialize(
            s.field(ArrayIndexStorageSchema::VALUES)
                .read(storage, s, f)
                .reference()
                .require_non_null(f),
            f,
        )
    }

    pub(super) fn emit_packed_array_probe(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
        index: I64Local,
        output: Option<&ValueLocals>,
        found: I32Local,
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
        if let Some(output) = output {
            let present = s
                .reserve_gc_local(f)
                .initialize(value.load(s, f).require_non_null(f), f);
            s.struct_type::<StoredValue>()
                .read_into(&present, output, s, f);
            present.clear(f);
        }
        found.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        value.clear(f);
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        values.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_packed_number_probe(storage, index, output, found, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    /// Promote only an occupancy-bounded default-data publication. The old
    /// numeric backing remains rooted until all immutable values are allocated.
    fn promote_packed_numbers(&mut self, storage: &GcLocal<ArrayIndexStorage>, f: &mut Function) {
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
        s.field(ArrayIndexStorageSchema::VALUES).write(
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

    /// Shared publication retains Number mode until a nonnumber ordinary value
    /// is actually admitted by the same capacity/occupancy policy.
    pub(super) fn packed_number_write(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
        index: I64Local,
        value: &ValueLocals,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let handled = self.packed_number_only_write(storage, index, value, f);
        handled.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedNumbers, f);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Ne);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        // Promotion is useful only when the eventual write can remain packed.
        let (numbers, presence) = self.packed_backings(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        capacity.store(f);
        let count = s.reserve_i64_local(f);
        s.field(ArrayIndexStorageSchema::COUNT)
            .read(storage, s, f)
            .store_i64(count, f);
        self.emit_packed_capacity_admission(index, capacity, count, f);
        self.open_frame(ControlFrameKind::If, f);
        self.promote_packed_numbers(storage, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(count, f);
        s.release_i32_local(capacity, f);
        presence.clear(f);
        numbers.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedValues, f);
        self.open_frame(ControlFrameKind::If, f);
        self.write_packed_value(storage, index, value, handled, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled
    }

    fn emit_packed_capacity_admission(
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
        storage: &GcLocal<ArrayIndexStorage>,
        index: I64Local,
        value: &ValueLocals,
        handled: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let values = self.packed_values(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        capacity.store(f);
        let count = s.reserve_i64_local(f);
        s.field(ArrayIndexStorageSchema::COUNT)
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
        s.field(ArrayIndexStorageSchema::VALUES).write(
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
        // The immutable wrapper is rooted while the old reference is inspected.
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(value, f), f);
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
        s.field(ArrayIndexStorageSchema::COUNT)
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
        old.clear(f);
        stored.clear(f);
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(count, f);
        s.release_i32_local(capacity, f);
        values.clear(f);
    }

    pub(in crate::objects) fn emit_materialize_packed_array(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
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
        s.field(ArrayIndexStorageSchema::DENSE).write(
            storage,
            GcOperand::reference(&dense, s),
            s,
            f,
        );
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
        storage: &GcLocal<ArrayIndexStorage>,
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
        s.field(ArrayIndexStorageSchema::COUNT)
            .read(storage, s, f)
            .store_i64(count, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        count.store(f);
        s.field(ArrayIndexStorageSchema::COUNT)
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
        storage: &GcLocal<ArrayIndexStorage>,
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
