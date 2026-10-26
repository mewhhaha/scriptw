//! Unboxed Number bits with an independent occupied-slot bitmap. No NaN is a
//! hole sentinel. Cold descriptor consumers transition the whole live backing
//! before receiving any mutable descriptor reference.
use super::*;
use crate::gc_types::{
    ArrayIndexKeyConstruction, ArrayIndexStorage, ArrayIndexStorageSchema, ArrayIndexedStorageMode,
    ArrayObject, IndexedTable, NumberBitsI64Array, NumberPresenceU64Array,
};
use crate::heap::StoredPropertyAttributes;
use crate::runtime_helpers::{
    ArrayIndexedPublishDefaultDataArguments, ArrayIndexedPublishDefaultDataParameters,
    HelperParameters,
};

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
        s.array_storage_mode_is(&storage, ArrayIndexedStorageMode::PackedNumbers, f);
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
    /// own packed Number slot on the exact target/receiver reference.
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
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let array = s
            .reserve_gc_local(f)
            .initialize(target.cast_reference::<ArrayObject>(s, f), f);
        let storage = self.emit_array_index_storage(&array, f);
        s.array_storage_mode_is(&storage, ArrayIndexedStorageMode::PackedNumbers, f);
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
        // Presence already proves capacity and unchanged full attributes.
        // No allocation/callback separates that proof from the single write.
        let numbers = s.reserve_gc_local(f).initialize(
            s.field(ArrayIndexStorageSchema::NUMBERS)
                .read(&storage, s, f)
                .reference(),
            f,
        );
        s.array_type::<NumberBitsI64Array>().write(
            &numbers,
            index32,
            GcOperand::i64_local(value.scalar()),
            s,
            f,
        );
        handled.set_constant(1, f);
        numbers.clear(f);
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
    fn packed_backings(
        &self,
        storage: &GcLocal<ArrayIndexStorage>,
        f: &mut Function,
    ) -> (GcLocal<NumberBitsI64Array>, GcLocal<NumberPresenceU64Array>) {
        let s = self.runtime_schema();
        let numbers = s.reserve_gc_local(f).initialize(
            s.field(ArrayIndexStorageSchema::NUMBERS)
                .read(storage, s, f)
                .reference(),
            f,
        );
        let presence = s.reserve_gc_local(f).initialize(
            s.field(ArrayIndexStorageSchema::NUMBER_PRESENT)
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
        storage: &GcLocal<ArrayIndexStorage>,
        index: I64Local,
        output: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) {
        self.emit_packed_array_probe(storage, index, Some(output), found, f);
    }

    pub(super) fn emit_packed_array_present(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
        index: I64Local,
        found: I32Local,
        f: &mut Function,
    ) {
        self.emit_packed_array_probe(storage, index, None, found, f);
    }

    fn emit_packed_array_probe(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
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

    /// Private publication primitive: caller already owns CreateDataProperty
    /// semantics or has proved an existing own writable packed data slot.
    /// Returning zero leaves all storage unchanged for the semantic slow path.
    fn emit_try_packed_array_number_write(
        &mut self,
        array: &GcLocal<ArrayObject>,
        index: I64Local,
        value: &ValueLocals,
        f: &mut Function,
    ) -> I32Local {
        let storage = self.emit_array_index_storage(array, f);
        let result = self.packed_number_write(&storage, index, value, f);
        storage.clear(f);
        result
    }

    fn packed_number_write(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
        index: I64Local,
        value: &ValueLocals,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedNumbers, f);
        value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let (numbers, presence) = self.packed_backings(storage, f);
        let capacity = s.reserve_i32_local(f);
        s.array_type::<NumberBitsI64Array>().length(&numbers, s, f);
        capacity.store(f);
        let count = s.reserve_i64_local(f);
        s.field(ArrayIndexStorageSchema::COUNT)
            .read(storage, s, f)
            .store_i64(count, f);
        // Existing capacity may retain holes; new capacity is occupancy driven
        // and bounded independently of Array.length and distant index values.
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
        grown.set_constant(8, f);
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
        s.field(ArrayIndexStorageSchema::NUMBERS).write(
            storage,
            GcOperand::reference(&new_numbers, s),
            s,
            f,
        );
        s.field(ArrayIndexStorageSchema::NUMBER_PRESENT).write(
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
        s.field(ArrayIndexStorageSchema::COUNT)
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
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(count, f);
        s.release_i32_local(capacity, f);
        presence.clear(f);
        numbers.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled
    }

    /// All descriptor consumers must call this before obtaining a reference.
    /// Returned descriptors therefore remain the actual mutable store owners.
    pub(super) fn emit_materialize_packed_array(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
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
        s.field(ArrayIndexStorageSchema::DENSE).write(
            storage,
            GcOperand::reference(&dense, s),
            s,
            f,
        );
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

    fn clear_packed_backings(&self, storage: &GcLocal<ArrayIndexStorage>, f: &mut Function) {
        let s = self.runtime_schema();
        let numbers = s.reserve_gc_local(f).initialize(s.empty_number_bits(f), f);
        let presence = s
            .reserve_gc_local(f)
            .initialize(s.empty_number_presence(f), f);
        s.field(ArrayIndexStorageSchema::NUMBERS).write(
            storage,
            GcOperand::reference(&numbers, s),
            s,
            f,
        );
        s.field(ArrayIndexStorageSchema::NUMBER_PRESENT).write(
            storage,
            GcOperand::reference(&presence, s),
            s,
            f,
        );
        presence.clear(f);
        numbers.clear(f);
    }

    pub(super) fn emit_packed_array_publish_descriptor(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
        index: I64Local,
        descriptor: &GcLocal<PropertyDescriptor>,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        s.array_storage_mode_is(storage, ArrayIndexedStorageMode::PackedNumbers, f);
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
        let value = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &value, s, f);
        let published = self.packed_number_write(storage, index, &value, f);
        published.load(f);
        handled.store(f);
        s.release_i32_local(published, f);
        value.clear(f);
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
        let handled = self.emit_try_packed_array_number_write(&p.array, p.index, &p.value, &mut f);
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

    pub(super) fn emit_packed_array_delete(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
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
        s.field(ArrayIndexStorageSchema::COUNT)
            .read(storage, s, f)
            .store_i64(bits, f);
        bits.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        bits.store(f);
        s.field(ArrayIndexStorageSchema::COUNT)
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

    pub(super) fn emit_packed_array_keys(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
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
