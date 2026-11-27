//! Packed ordinary data uses Number bits/presence or nullable immutable value
//! slots. No JavaScript value is a hole sentinel. Cold consumers retire the backing
//! before receiving any mutable descriptor reference.
use super::*;
use crate::gc_types::{
    ArrayIndexKeyConstruction, ArrayIndexedStorageMode, ArrayObject, ArrayObjectSchema,
    IndexedTable, NumberBitsI64Array, NumberPresenceU64Array, PackedValueTable, StoredValue,
};
use crate::heap::StoredPropertyAttributes;
use crate::runtime_helpers::{
    ArrayIndexedPublishDefaultDataArguments, ArrayIndexedPublishDefaultDataParameters,
    HelperParameters,
};
use values::PackedArrayWriteValue;

const PACKED_INDEX_LIMIT: i64 = 1 << 20;

impl FunctionBuilder<'_> {
    /// Packed entries are always configurable. Canonical missing indices are
    /// successful deletions too; named keys/proxies retain generic semantics.
    pub(crate) fn emit_packed_array_delete_property(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        target.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(target.cast_reference::<ArrayObject>(s, f), f);
        let storage = self.emit_array_index_storage(&array, f);
        s.array_storage_is_packed(&storage, f);
        self.open_frame(ControlFrameKind::If, f);
        let index32 = s.reserve_i32_local(f);
        let indexed = s.reserve_i32_local(f);
        self.emit_property_key_array_index(key, index32, indexed, f)?;
        indexed.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let index = s.reserve_i64_local(f);
        index32.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        index.store(f);
        let deleted = self.emit_packed_array_delete(&storage, index, f);
        deleted.load(f);
        handled.store(f);
        s.release_i32_local(deleted, f);
        s.release_i64_local(index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(indexed, f);
        s.release_i32_local(index32, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        storage.clear(f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(handled)
    }
    /// Ordinary Set can bypass descriptor/prototype work only for a present
    /// own packed data slot on the exact target/receiver reference.
    pub(crate) fn emit_packed_array_existing_data_write(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        target.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<ArrayObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        target.reference().load(f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(target.cast_reference::<ArrayObject>(s, f), f);
        let storage = self.emit_array_index_storage(&array, f);
        s.array_storage_is_packed(&storage, f);
        self.open_frame(ControlFrameKind::If, f);
        let index32 = s.reserve_i32_local(f);
        let indexed = s.reserve_i32_local(f);
        self.emit_property_key_array_index(key, index32, indexed, f)?;
        indexed.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let index = s.reserve_i64_local(f);
        index32.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        index.store(f);
        let found = s.reserve_i32_local(f);
        found.set_constant(0, f);
        self.emit_packed_array_present(&storage, index, found, f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let published =
            self.emit_packed_array_write(&storage, index, PackedArrayWriteValue::Raw(value), f);
        published.load(f);
        handled.store(f);
        s.release_i32_local(published, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(found, f);
        s.release_i64_local(index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(indexed, f);
        s.release_i32_local(index32, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        storage.clear(f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(handled)
    }
    /// OrdinarySet(A, index, V, A) on a concrete packed Array with an array
    /// index below 2^32 - 1. A present element is writable default data, so
    /// it takes the value. An absent one has no own property, so the Set
    /// creates it on A when no prototype has the index (each Array
    /// prototype without indexed elements, each intrinsic layout without
    /// index keys, and each other ordinary prototype by lookup), with A
    /// extensible and, past its length, a writable "length" that then
    /// becomes index + 1: ArrayDefineOwnProperty's steps. Anything else
    /// branches to `general` before any effect.
    pub(in crate::objects) fn emit_packed_array_index_set_or_branch(
        &mut self,
        array: &GcLocal<ArrayObject>,
        index: I64Local,
        value: &ValueLocals,
        general: crate::emit::ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        s.array_storage_is_packed(array, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let length = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::LENGTH)
            .read(array, s, f)
            .store_i64(length, f);

        let absent = self.open_frame(ControlFrameKind::Block, f);
        // No own array index can be at or beyond the live length.
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(absent, f);
        let found = s.reserve_i32_local(f);
        found.set_constant(0, f);
        self.emit_packed_array_present(array, index, found, f);
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(absent, f);
        s.release_i32_local(found, f);
        let published =
            self.emit_packed_array_write(array, index, PackedArrayWriteValue::Raw(value), f);
        published.load(f);
        f.instruction(&Instruction::I32Eqz);
        s.release_i32_local(published, f);
        self.emit_branch_if_to_target(general, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        s.field(ArrayObjectSchema::LENGTH_WRITABLE)
            .read(array, s, f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        self.emit_branch_if_to_target(general, f);
        let header = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(array, s, f)
                .reference(),
            f,
        );
        s.field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(&header, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        let end = s.reserve_i64_local(f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        end.store(f);

        let publish = self.open_frame(ControlFrameKind::Block, f);
        let walk = self.open_frame(ControlFrameKind::Block, f);
        // A dense packed array with its protected prototype can append
        // directly. Growth or promotion still uses the shared publication.
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64Eq);
        s.field(ArrayObjectSchema::COUNT).read(array, s, f);
        length.load(f);
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(walk, f);
        let prototype = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, s, f)
                .reference(),
            f,
        );
        header.clear(f);
        self.emit_array_prototype_chain_without_indices_or_branch(&prototype, walk, f);
        prototype.clear(f);
        self.emit_packed_append_within_capacity_or_branch(array, index, value, publish, f);
        s.field(ArrayObjectSchema::COUNT)
            .write(array, GcOperand::i64_local(end), s, f);
        s.field(ArrayObjectSchema::LENGTH)
            .write(array, GcOperand::i64_local(end), s, f);
        self.emit_branch_to_target(done, f);

        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let permitted = self.emit_packed_push_prototypes_permit_append(array, index, end, f)?;
        permitted.load(f);
        f.instruction(&Instruction::I32Eqz);
        s.release_i32_local(permitted, f);
        self.emit_branch_if_to_target(general, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        self.emit_array_indexed_publish_default_data(array, index, value, f)?;
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I64GeU);
        f.instruction(&Instruction::If(BlockType::Empty));
        s.field(ArrayObjectSchema::LENGTH)
            .write(array, GcOperand::i64_local(end), s, f);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i64_local(end, f);
        s.release_i64_local(length, f);
        Ok(())
    }

    /// Fast path for `Array.prototype.fill(value, 0, end)` on an empty
    /// extensible packed `ArrayObject` whose length is at least `end` and
    /// whose prototype chain has no indexed properties. Allocates one exact-size
    /// `PackedValueTable` filled with a single shared immutable `StoredValue`.
    pub(crate) fn emit_packed_array_fill_empty_prefix_or_branch(
        &mut self,
        receiver: &ValueLocals,
        start: I64Local,
        end: I64Local,
        value: &ValueLocals,
        general: crate::emit::ControlTarget,
        f: &mut Function,
    ) {
        const PACKED_FILL_MAX_ELEMENTS: i64 = 1 << 24;
        let s = self.runtime_schema();
        start.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        end.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(general, f);
        end.load(f);
        f.instruction(&Instruction::I64Const(PACKED_FILL_MAX_ELEMENTS));
        f.instruction(&Instruction::I64GtU);
        self.emit_branch_if_to_target(general, f);
        let array = s.reserve_gc_local(f).initialize(
            receiver.sealed_reference_or_branch::<ArrayObject>(
                s,
                self.local_branch_label(general),
                f,
            ),
            f,
        );
        s.array_storage_is_packed(&array, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        s.field(ArrayObjectSchema::COUNT).read(&array, s, f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        s.field(ArrayObjectSchema::LENGTH).read(&array, s, f);
        end.load(f);
        f.instruction(&Instruction::I64LtU);
        self.emit_branch_if_to_target(general, f);
        let header = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference(),
            f,
        );
        s.field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(&header, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(general, f);
        let prototype = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&header, s, f)
                .reference(),
            f,
        );
        header.clear(f);
        self.emit_array_prototype_chain_without_indices_or_branch(&prototype, general, f);
        prototype.clear(f);
        let stored = self.emit_packed_value_for_store(value, f);
        let fill_len = s.reserve_i32_local(f);
        end.load(f);
        f.instruction(&Instruction::I32WrapI64);
        fill_len.store(f);
        let values = s.reserve_gc_local(f).initialize(
            s.array_type::<PackedValueTable>().filled(
                GcOperand::nullable_reference(&stored, s),
                fill_len,
                f,
            ),
            f,
        );
        s.release_i32_local(fill_len, f);
        stored.clear(f);
        self.clear_packed_backings(&array, f);
        s.field(ArrayObjectSchema::VALUES).write(
            &array,
            GcOperand::nullable_reference(&values, s),
            s,
            f,
        );
        s.field(ArrayObjectSchema::COUNT)
            .write(&array, GcOperand::i64_local(end), s, f);
        s.set_array_storage_mode(&array, ArrayIndexedStorageMode::PackedValues, f);
        values.clear(f);
        array.clear(f);
    }

    /// Appends to a result whose existing elements form a dense prefix.
    /// The caller proves an array index and owns length publication.
    pub(crate) fn emit_dense_packed_value_append_or_branch(
        &mut self,
        array: &GcLocal<ArrayObject>,
        index: I64Local,
        value: &ValueLocals,
        fallback: crate::emit::ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.array_storage_mode_is(array, ArrayIndexedStorageMode::PackedValues, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(fallback, f);
        s.field(ArrayObjectSchema::COUNT).read(array, s, f);
        index.load(f);
        f.instruction(&Instruction::I64Ne);
        self.emit_branch_if_to_target(fallback, f);
        let slot = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        self.emit_packed_value_store_within_capacity_or_branch(array, slot, value, fallback, f);
        s.release_i32_local(slot, f);
        let end = s.reserve_i64_local(f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        end.store(f);
        s.field(ArrayObjectSchema::COUNT)
            .write(array, GcOperand::i64_local(end), s, f);
        s.release_i64_local(end, f);
    }

    fn emit_packed_value_store_within_capacity_or_branch(
        &mut self,
        array: &GcLocal<ArrayObject>,
        slot: I32Local,
        value: &ValueLocals,
        publish: crate::emit::ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let values = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::VALUES)
                .read(array, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        slot.load(f);
        s.array_type::<PackedValueTable>().length(&values, s, f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(publish, f);
        let stored = self.emit_packed_value_for_store(value, f);
        s.array_type::<PackedValueTable>().write(
            &values,
            slot,
            GcOperand::nullable_reference(&stored, s),
            s,
            f,
        );
        stored.clear(f);
        values.clear(f);
    }

    /// The caller has proved that index is the first unoccupied slot and
    /// owns publication of length and count after the store.
    fn emit_packed_append_within_capacity_or_branch(
        &mut self,
        array: &GcLocal<ArrayObject>,
        index: I64Local,
        value: &ValueLocals,
        publish: crate::emit::ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let slot = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        s.array_storage_mode_is(array, ArrayIndexedStorageMode::PackedValues, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_packed_value_store_within_capacity_or_branch(array, slot, value, publish, f);
        f.instruction(&Instruction::Else);
        // The packed admission left only PackedNumbers. Changing its value
        // type requires the existing promotion path.
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(publish, f);
        let (numbers, presence) = self.packed_backings(array, f);
        slot.load(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(publish, f);
        let word = s.reserve_i32_local(f);
        let mask = s.reserve_i64_local(f);
        let bits = s.reserve_i64_local(f);
        self.packed_presence(&presence, slot, word, mask, bits, f);
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
        s.release_i64_local(bits, f);
        s.release_i64_local(mask, f);
        s.release_i32_local(word, f);
        presence.clear(f);
        numbers.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(slot, f);
    }

    fn packed_backings(
        &self,
        storage: &GcLocal<ArrayObject>,
        f: &mut Function,
    ) -> (GcLocal<NumberBitsI64Array>, GcLocal<NumberPresenceU64Array>) {
        let s = self.runtime_schema();
        let numbers = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::NUMBERS)
                .read(storage, s, f)
                .reference(),
            f,
        );
        let presence = s.reserve_gc_local(f).initialize(
            s.field(ArrayObjectSchema::NUMBER_PRESENT)
                .read(storage, s, f)
                .reference(),
            f,
        );
        (numbers, presence)
    }

    /// Caller proves the index is in the Number backing's capacity. Presence
    /// words cover exactly ceil(capacity/64), including capacity below 64.
    fn packed_presence(
        &self,
        presence: &GcLocal<NumberPresenceU64Array>,
        index: I32Local,
        word: I32Local,
        mask: I64Local,
        bits: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        index.load(f);
        f.instruction(&Instruction::I32Const(6));
        f.instruction(&Instruction::I32ShrU);
        word.store(f);
        f.instruction(&Instruction::I64Const(1));
        index.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Shl);
        mask.store(f);
        s.array_type::<NumberPresenceU64Array>()
            .read(presence, word, s, f)
            .store_i64(bits, f);
    }

    pub(super) fn emit_packed_array_value(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        output: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) {
        self.emit_packed_array_probe(storage, index, Some(output), found, f);
    }

    pub(super) fn emit_packed_array_present(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        found: I32Local,
        f: &mut Function,
    ) {
        self.emit_packed_array_probe(storage, index, None, found, f);
    }

    fn emit_packed_number_probe(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        output: Option<&ValueLocals>,
        found: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let (numbers, presence) = self.packed_backings(storage, f);
        index.load(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        let slot = s.reserve_i32_local(f);
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
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        if let Some(output) = output {
            s.array_type::<NumberBitsI64Array>()
                .read(&numbers, slot, s, f)
                .store_i64(bits, f);
            output.set_number(bits, f);
        }
        found.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(bits, f);
        s.release_i64_local(mask, f);
        s.release_i32_local(word, f);
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        presence.clear(f);
        numbers.clear(f);
    }

    /// The caller proved Number storage and a Number payload.
    fn emit_packed_number_write(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        bits: I64Local,
        handled: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let (numbers, presence) = self.packed_backings(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        capacity.store(f);
        let count = s.reserve_i64_local(f);
        s.field(ArrayObjectSchema::COUNT)
            .read(storage, s, f)
            .store_i64(count, f);
        // Existing capacity may retain holes; new capacity is occupancy driven
        // and bounded independently of Array.length and distant index values.
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
        let new_numbers = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberBitsI64Array>()
                .filled(GcOperand::i64(0), grown, f),
            f,
        );
        s.array_type::<NumberBitsI64Array>().copy_prefix_from(
            &new_numbers,
            &numbers,
            capacity,
            s,
            f,
        );
        let words = s.reserve_i32_local(f);
        grown.load(f);
        f.instruction(&Instruction::I32Const(63));
        f.instruction(&Instruction::I32Add);
        f.instruction(&Instruction::I32Const(6));
        f.instruction(&Instruction::I32ShrU);
        words.store(f);
        let new_presence = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberPresenceU64Array>()
                .filled(GcOperand::i64(0), words, f),
            f,
        );
        s.array_type::<NumberPresenceU64Array>()
            .length(&presence, s, f);
        words.store(f);
        s.array_type::<NumberPresenceU64Array>().copy_prefix_from(
            &new_presence,
            &presence,
            words,
            s,
            f,
        );
        // Both complete allocations exist before publication. All old/new
        // backing references remain strong roots across the second allocation.
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
        s.release_i32_local(words, f);
        s.release_i32_local(grown, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let slot = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        let word = s.reserve_i32_local(f);
        let mask = s.reserve_i64_local(f);
        let occupied = s.reserve_i64_local(f);
        self.packed_presence(&presence, slot, word, mask, occupied, f);
        occupied.load(f);
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
            GcOperand::i64_local(bits),
            s,
            f,
        );
        occupied.load(f);
        mask.load(f);
        f.instruction(&Instruction::I64Or);
        occupied.store(f);
        s.array_type::<NumberPresenceU64Array>().write(
            &presence,
            word,
            GcOperand::i64_local(occupied),
            s,
            f,
        );
        handled.set_constant(1, f);
        s.release_i64_local(occupied, f);
        s.release_i64_local(mask, f);
        s.release_i32_local(word, f);
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(count, f);
        s.release_i32_local(capacity, f);
        presence.clear(f);
        numbers.clear(f);
    }

    /// All descriptor consumers must call this before obtaining a reference.
    /// Returned descriptors therefore remain the actual mutable store owners.
    fn emit_materialize_packed_numbers(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedNumbers, f);
        self.open_frame(ControlFrameKind::If, f);
        let (numbers, presence) = self.packed_backings(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        capacity.store(f);
        let dense = s.reserve_gc_local(f).initialize(
            s.array_type::<IndexedTable>()
                .filled(GcOperand::null(s), capacity, f),
            f,
        );
        let slot = s.reserve_i32_local(f);
        slot.set_constant(0, f);
        let word = s.reserve_i32_local(f);
        let mask = s.reserve_i64_local(f);
        let bits = s.reserve_i64_local(f);
        let value = s.reserve_value_local(f);
        let undefined = s.reserve_value_local(f);
        undefined.set_undefined(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        slot.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        self.packed_presence(&presence, slot, word, mask, bits, f);
        bits.load(f);
        mask.load(f);
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.array_type::<NumberBitsI64Array>()
            .read(&numbers, slot, s, f)
            .store_i64(bits, f);
        value.set_number(bits, f);
        let descriptor = self.emit_alloc_property_descriptor(
            StoredPropertyAttributes::Data {
                writable: true,
                enumerable: true,
                configurable: true,
            },
            &value,
            &undefined,
            &undefined,
            f,
        );
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
        slot.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        slot.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // No allocation or user code occurs between these publication writes.
        s.field(ArrayObjectSchema::DENSE)
            .write(storage, GcOperand::reference(&dense, s), s, f);
        self.clear_packed_backings(storage, f);
        s.set_array_storage_mode(storage, ArrayIndexedStorageMode::DenseDescriptors, f);
        undefined.clear(f);
        value.clear(f);
        s.release_i64_local(bits, f);
        s.release_i64_local(mask, f);
        s.release_i32_local(word, f);
        s.release_i32_local(slot, f);
        s.release_i32_local(capacity, f);
        dense.clear(f);
        presence.clear(f);
        numbers.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    fn clear_packed_backings(&self, storage: &GcLocal<ArrayObject>, f: &mut Function) {
        let s = self.runtime_schema();
        let numbers = s.reserve_gc_local(f).initialize(s.empty_number_bits(f), f);
        let presence = s
            .reserve_gc_local(f)
            .initialize(s.empty_number_presence(f), f);
        s.field(ArrayObjectSchema::NUMBERS)
            .write(storage, GcOperand::reference(&numbers, s), s, f);
        s.field(ArrayObjectSchema::NUMBER_PRESENT).write(
            storage,
            GcOperand::reference(&presence, s),
            s,
            f,
        );
        s.field(ArrayObjectSchema::VALUES)
            .write(storage, GcOperand::null(s), s, f);
        presence.clear(f);
        numbers.clear(f);
    }

    pub(super) fn emit_packed_array_publish_descriptor(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        descriptor: &GcLocal<PropertyDescriptor>,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        s.array_storage_is_packed(storage, f);
        self.open_frame(ControlFrameKind::If, f);
        let flags = s.reserve_i32_local(f);
        s.field(PropertyDescriptorSchema::FLAGS)
            .read(descriptor, s, f)
            .store(flags, f);
        flags.load(f);
        f.instruction(&Instruction::I32Const(
            DescriptorMask::WRITABLE
                .union(DescriptorMask::ENUMERABLE)
                .union(DescriptorMask::CONFIGURABLE)
                .as_i32(),
        ));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(PropertyDescriptorSchema::VALUE)
                .read(descriptor, s, f)
                .reference(),
            f,
        );
        // Growing default-data backing comes through this descriptor path.
        // Its immutable value edge can become the element directly.
        let boxed = self.open_reference_block(s, f);
        let recipe = self.open_frame(ControlFrameKind::Block, f);
        boxed.branch_with(
            stored.sealed_subtype_or_branch::<StoredValue>(s, self.local_branch_label(recipe), f),
            f,
        );
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        boxed.branch_with(
            s.helper_reference_on_stack(
                s.call_helper(
                    crate::runtime_helpers::PropertyValueResolveArguments::new(&stored),
                    self.runtime_helper_base()
                        .expect("packed descriptor publication requires runtime helpers"),
                    f,
                ),
            ),
            f,
        );
        let reuse = s.reserve_gc_local(f).initialize(boxed.close(f), f);
        self.pop_control(ControlFrameKind::Block);
        let published =
            self.emit_packed_array_write(storage, index, PackedArrayWriteValue::Stored(&reuse), f);
        published.load(f);
        handled.store(f);
        s.release_i32_local(published, f);
        reuse.clear(f);
        stored.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_materialize_packed_array(storage, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(flags, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled
    }

    /// This method is for proven CreateDataProperty contexts, never ordinary
    /// Set on a missing index (which may invoke an inherited indexed setter).
    /// The producer retains its own length/extensibility/error ordering.
    pub(crate) fn emit_array_indexed_publish_default_data(
        &mut self,
        array: &GcLocal<ArrayObject>,
        index: I64Local,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // Keep geometric growth/copy and cold descriptor publication in one
        // shared R body. Repeating that CFG for every short source Array can
        // otherwise overwhelm native compilation of the containing P body.
        s.call_helper(
            ArrayIndexedPublishDefaultDataArguments::new(array, index, value),
            self.runtime_helper_base()?,
            f,
        );
        Ok(())
    }

    pub(crate) fn compile_array_indexed_publish_default_data_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::ArrayIndexedPublishDefaultData);
        let p = self.helper_parameters::<ArrayIndexedPublishDefaultDataParameters>(&mut f);
        let s = self.runtime_schema();
        // This body is the publication of every literal element and append,
        // so it holds only the store that needs no copy. The descriptor
        // publication repeats the packed admission with growth and
        // promotion before it retires a packed backing.
        let handled =
            self.emit_packed_array_store_within_capacity(&p.array, p.index, &p.value, &mut f);
        handled.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let absent = s.reserve_value_local(&mut f);
        absent.set_undefined(&mut f);
        let descriptor = self.emit_alloc_property_descriptor(
            StoredPropertyAttributes::Data {
                writable: true,
                enumerable: true,
                configurable: true,
            },
            &p.value,
            &absent,
            &absent,
            &mut f,
        );
        self.emit_array_indexed_publish_descriptor(&p.array, p.index, &descriptor, &mut f)?;
        descriptor.clear(&mut f);
        absent.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(handled, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    fn emit_packed_number_delete(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedNumbers, f);
        handled.store(f);
        handled.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let (numbers, presence) = self.packed_backings(storage, f);
        index.load(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        let slot = s.reserve_i32_local(f);
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
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        bits.load(f);
        mask.load(f);
        f.instruction(&Instruction::I64Const(-1));
        f.instruction(&Instruction::I64Xor);
        f.instruction(&Instruction::I64And);
        bits.store(f);
        s.array_type::<NumberPresenceU64Array>().write(
            &presence,
            word,
            GcOperand::i64_local(bits),
            s,
            f,
        );
        s.array_type::<NumberBitsI64Array>()
            .write(&numbers, slot, GcOperand::i64(0), s, f);
        s.field(ArrayObjectSchema::COUNT)
            .read(storage, s, f)
            .store_i64(bits, f);
        bits.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        bits.store(f);
        s.field(ArrayObjectSchema::COUNT)
            .write(storage, GcOperand::i64_local(bits), s, f);
        bits.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.clear_packed_backings(storage, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(bits, f);
        s.release_i64_local(mask, f);
        s.release_i32_local(word, f);
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        presence.clear(f);
        numbers.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled
    }

    fn emit_packed_number_keys(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        keys: &ArrayIndexKeyConstruction,
        output: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let (numbers, presence) = self.packed_backings(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        capacity.store(f);
        let slot = s.reserve_i32_local(f);
        slot.set_constant(0, f);
        let index = s.reserve_i64_local(f);
        let word = s.reserve_i32_local(f);
        let mask = s.reserve_i64_local(f);
        let bits = s.reserve_i64_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        slot.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        self.packed_presence(&presence, slot, word, mask, bits, f);
        bits.load(f);
        mask.load(f);
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Eqz);
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
        slot.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        slot.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        for local in [bits, mask, index] {
            s.release_i64_local(local, f);
        }
        for local in [word, slot, capacity] {
            s.release_i32_local(local, f);
        }
        presence.clear(f);
        numbers.clear(f);
    }
}

mod values;
