use super::*;
use crate::runtime_helpers::HelperParameters;

/// A raw hash stays paired with an owned, immutable local snapshot of its key. Primitive contents
/// and assigned identity hashes are stable even across user callbacks/GC.
/// Never retain table/index state here: every lookup/update uses current buckets.
#[must_use]
pub(super) struct PreparedCollectionKey {
    value: ValueLocals,
    hash: I64Local,
}

impl PreparedCollectionKey {
    pub(super) fn release(self, builder: &FunctionBuilder<'_>, f: &mut Function) {
        builder.runtime_schema().release_i64_local(self.hash, f);
        self.value.clear(f);
    }
}

// Physical positions may compact. Stable insertion ordinals let live cursors
// recover their position without retaining old tables or registering cursors.
pub(super) struct CollectionCursorLocals {
    pub(super) index: I64Local,
    pub(super) next_ordinal: I64Local,
    pub(super) version: I64Local,
}
impl CollectionCursorLocals {
    pub(super) fn reserve(schema: &RuntimeSchema, f: &mut Function) -> Self {
        let cursor = Self {
            index: schema.reserve_i64_local(f),
            next_ordinal: schema.reserve_i64_local(f),
            version: schema.reserve_i64_local(f),
        };
        f.instruction(&Instruction::I64Const(0));
        cursor.index.store(f);
        f.instruction(&Instruction::I64Const(0));
        cursor.next_ordinal.store(f);
        f.instruction(&Instruction::I64Const(-1));
        cursor.version.store(f);
        cursor
    }
    pub(super) fn release(self, schema: &RuntimeSchema, f: &mut Function) {
        schema.release_i64_local(self.version, f);
        schema.release_i64_local(self.next_ordinal, f);
        schema.release_i64_local(self.index, f);
    }
}
impl FunctionBuilder<'_> {
    /// These are private bounded-resource counters, never observable JS sizes.
    fn emit_collection_advance_counter(&self, counter: I64Local, f: &mut Function) {
        counter.load(f);
        f.instruction(&Instruction::I64Const(-1));
        f.instruction(&Instruction::I64Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        self.emit_increment_local(counter, 1, f);
    }

    pub(super) fn emit_collection_prepare_key(
        &mut self,
        key: &ValueLocals,
        f: &mut Function,
    ) -> PreparedCollectionKey {
        let value = self.runtime_schema().reserve_value_local(f);
        value.copy_from(key, f);
        let hash = self.runtime_schema().reserve_i64_local(f);
        self.emit_collection_key_hash(&value, hash, f);
        PreparedCollectionKey { value, hash }
    }

    pub(super) fn emit_collection_alloc_map(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        f: &mut Function,
    ) -> GcLocal<MapObject> {
        let s = self.runtime_schema();
        let zero = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        zero.store(f);
        let entries = s.reserve_gc_local(f).initialize(
            s.array_type::<MapEntryTable>()
                .filled(GcOperand::null(s), zero, f),
            f,
        );
        f.instruction(&Instruction::I32Const(4));
        zero.store(f);
        let buckets = s.reserve_gc_local(f).initialize(
            s.array_type::<CollectionHashTable>()
                .filled(GcOperand::i64(0), zero, f),
            f,
        );
        let ordinals = s.reserve_gc_local(f).initialize(s.empty_number_bits(f), f);
        let record = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>().construct(
                (
                    GcOperand::reference(header, s),
                    GcOperand::reference(&entries, s),
                    GcOperand::i64(0),
                    GcOperand::i64(0),
                    GcOperand::reference(&buckets, s),
                    GcOperand::reference(&ordinals, s),
                    GcOperand::i64(0),
                    GcOperand::i64(0),
                ),
                f,
            ),
            f,
        );
        ordinals.clear(f);
        buckets.clear(f);
        entries.clear(f);
        s.release_i32_local(zero, f);
        record
    }
    pub(super) fn emit_collection_find_map(
        &mut self,
        record: &GcLocal<MapObject>,
        key: &ValueLocals,
        index: I64Local,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let prepared = self.emit_collection_prepare_key(key, f);
        let result = self.emit_collection_find_prepared_map(record, &prepared, index, found, f);
        prepared.release(self, f);
        result
    }

    pub(super) fn emit_collection_find_prepared_map(
        &mut self,
        record: &GcLocal<MapObject>,
        prepared: &PreparedCollectionKey,
        index: I64Local,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let key = &prepared.value;
        let hash = prepared.hash;
        let cursor = s.reserve_i32_local(f);
        let bucket = s.reserve_i32_local(f);
        let chain = s.reserve_i64_local(f);
        let retained = s.reserve_value_local(f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>()
                .field(MapObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        let buckets = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>()
                .field(MapObjectSchema::HASH_INDEX)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.emit_collection_hash_bucket(&buckets, hash, bucket, f);
        s.array_type::<CollectionHashTable>()
            .read(&buckets, bucket, s, f)
            .store_i64(chain, f);
        f.instruction(&Instruction::I32Const(0));
        found.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        chain.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(exit, f);
        chain.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        index.store(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let entry = s.reserve_gc_local(f).initialize(
            s.array_type::<MapEntryTable>()
                .read(&table, cursor, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapEntry>()
                .field(MapEntrySchema::KEY)
                .read(&entry, s, f)
                .reference(),
            f,
        );
        self.emit_stored_value_to_locals(&stored, &retained, f);
        self.emit_tagged_payload_same_value_zero_i32(&retained, key, f)?;
        found.store(f);
        s.struct_type::<MapEntry>()
            .field(MapEntrySchema::HASH_NEXT)
            .read(&entry, s, f)
            .store_i64(chain, f);
        stored.clear(f);
        entry.clear(f);
        found.load(f);
        self.emit_branch_if_to_target(exit, f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.struct_type::<MapObject>()
            .field(MapObjectSchema::HISTORY_LENGTH)
            .read(record, s, f)
            .store_i64(index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        retained.clear(f);
        buckets.clear(f);
        table.clear(f);
        s.release_i64_local(chain, f);
        s.release_i32_local(bucket, f);
        s.release_i32_local(cursor, f);
        Ok(())
    }

    fn emit_collection_grow_map(
        &mut self,
        record: &GcLocal<MapObject>,
        table: &GcLocal<MapEntryTable>,
        count: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // Physical history, rather than live occupancy, owns the next slot.
        // Below this u32 array length the helper's address-limit trap is inert.
        count.load(f);
        s.array_type::<MapEntryTable>().length(table, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        s.call_helper(
            crate::runtime_helpers::CollectionPrepareMapArguments::new(record, count),
            self.runtime_helper_base()?,
            f,
        )
        .store(count, f);
        table.replace(
            s.field(MapObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_collection_prepare_map_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::CollectionPrepareMap);
        let p = self
            .helper_parameters::<crate::runtime_helpers::CollectionPrepareMapParameters>(&mut f);
        let s = self.runtime_schema();
        let capacity = s.reserve_i32_local(&mut f);
        let source = s.reserve_i32_local(&mut f);
        let destination = s.reserve_i32_local(&mut f);
        let live = s.reserve_i64_local(&mut f);
        let version = s.reserve_i64_local(&mut f);
        let table = s.reserve_gc_local(&mut f).initialize(
            s.field(MapObjectSchema::ENTRIES)
                .read(&p.record, s, &mut f)
                .reference(),
            &mut f,
        );
        s.array_type::<MapEntryTable>().length(&table, s, &mut f);
        capacity.store(&mut f);
        p.count.load(&mut f);
        capacity.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, &mut f);
        // Bound physical history by live occupancy, preserving insertion order.
        s.field(MapObjectSchema::LIVE_COUNT)
            .read(&p.record, s, &mut f)
            .store_i64(live, &mut f);
        live.load(&mut f);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64Mul);
        live.store(&mut f);
        live.load(&mut f);
        f.instruction(&Instruction::I64Const(4));
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::I64Const(4));
        live.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        live.load(&mut f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        live.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        live.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        capacity.store(&mut f);
        let old_ordinals = s.reserve_gc_local(&mut f).initialize(
            s.field(MapObjectSchema::ORDINALS)
                .read(&p.record, s, &mut f)
                .reference(),
            &mut f,
        );
        let replacement = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<MapEntryTable>()
                .filled(GcOperand::null(s), capacity, &mut f),
            &mut f,
        );
        let ordinals = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<NumberBitsI64Array>()
                .filled(GcOperand::i64(0), capacity, &mut f),
            &mut f,
        );
        f.instruction(&Instruction::I32Const(0));
        source.store(&mut f);
        f.instruction(&Instruction::I32Const(0));
        destination.store(&mut f);
        let exit = self.open_frame(ControlFrameKind::Block, &mut f);
        let again = self.open_frame(ControlFrameKind::Loop, &mut f);
        source.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        p.count.load(&mut f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, &mut f);
        let entry = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<MapEntryTable>()
                .read(&table, source, s, &mut f)
                .reference(),
            &mut f,
        );
        entry.load(s, &mut f).is_null(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        s.array_type::<MapEntryTable>().write(
            &replacement,
            destination,
            GcOperand::reference(&entry, s),
            s,
            &mut f,
        );
        s.array_type::<NumberBitsI64Array>()
            .read(&old_ordinals, source, s, &mut f)
            .store_i64(live, &mut f);
        s.array_type::<NumberBitsI64Array>().write(
            &ordinals,
            destination,
            GcOperand::i64_local(live),
            s,
            &mut f,
        );
        destination.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        destination.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        entry.clear(&mut f);
        source.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        source.store(&mut f);
        self.emit_branch_to_target(again, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        destination.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        p.count.store(&mut f);
        s.field(MapObjectSchema::ENTRIES).write(
            &p.record,
            GcOperand::reference(&replacement, s),
            s,
            &mut f,
        );
        s.field(MapObjectSchema::ORDINALS).write(
            &p.record,
            GcOperand::reference(&ordinals, s),
            s,
            &mut f,
        );
        s.field(MapObjectSchema::HISTORY_LENGTH).write(
            &p.record,
            GcOperand::i64_local(p.count),
            s,
            &mut f,
        );
        s.field(MapObjectSchema::VERSION)
            .read(&p.record, s, &mut f)
            .store_i64(version, &mut f);
        self.emit_collection_advance_counter(version, &mut f);
        s.field(MapObjectSchema::VERSION).write(
            &p.record,
            GcOperand::i64_local(version),
            s,
            &mut f,
        );
        self.emit_collection_rehash_map(&p.record, capacity, &mut f);
        ordinals.clear(&mut f);
        replacement.clear(&mut f);
        old_ordinals.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // This private array address limit is a resource trap, not a JS error.
        p.count.load(&mut f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        table.clear(&mut f);
        s.release_i64_local(version, &mut f);
        s.release_i64_local(live, &mut f);
        s.release_i32_local(destination, &mut f);
        s.release_i32_local(source, &mut f);
        s.release_i32_local(capacity, &mut f);
        p.count.load(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
    pub(super) fn emit_collection_clear_map(
        &mut self,
        record: &GcLocal<MapObject>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let count = s.reserve_i64_local(f);
        s.field(MapObjectSchema::HISTORY_LENGTH)
            .read(record, s, f)
            .store_i64(count, f);
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let capacity = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        capacity.store(f);
        let entries = s.reserve_gc_local(f).initialize(
            s.array_type::<MapEntryTable>()
                .filled(GcOperand::null(s), capacity, f),
            f,
        );
        f.instruction(&Instruction::I32Const(4));
        capacity.store(f);
        let buckets = s.reserve_gc_local(f).initialize(
            s.array_type::<CollectionHashTable>()
                .filled(GcOperand::i64(0), capacity, f),
            f,
        );
        let ordinals = s.reserve_gc_local(f).initialize(s.empty_number_bits(f), f);
        s.field(MapObjectSchema::ENTRIES)
            .write(record, GcOperand::reference(&entries, s), s, f);
        s.field(MapObjectSchema::HASH_INDEX)
            .write(record, GcOperand::reference(&buckets, s), s, f);
        s.field(MapObjectSchema::ORDINALS)
            .write(record, GcOperand::reference(&ordinals, s), s, f);
        s.field(MapObjectSchema::LIVE_COUNT)
            .write(record, GcOperand::i64(0), s, f);
        s.field(MapObjectSchema::HISTORY_LENGTH)
            .write(record, GcOperand::i64(0), s, f);
        s.field(MapObjectSchema::VERSION)
            .read(record, s, f)
            .store_i64(count, f);
        self.emit_collection_advance_counter(count, f);
        s.field(MapObjectSchema::VERSION)
            .write(record, GcOperand::i64_local(count), s, f);
        // NEXT_ORDINAL survives: existing cursors still observe later inserts.
        ordinals.clear(f);
        buckets.clear(f);
        entries.clear(f);
        s.release_i32_local(capacity, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(count, f);
    }
    pub(super) fn emit_collection_delete_map(
        &mut self,
        record: &GcLocal<MapObject>,
        key: &ValueLocals,
        removed: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let prepared = self.emit_collection_prepare_key(key, f);
        let result = self.emit_collection_delete_prepared_map(record, &prepared, removed, f);
        prepared.release(self, f);
        result
    }
    pub(super) fn emit_collection_delete_prepared_map(
        &mut self,
        record: &GcLocal<MapObject>,
        prepared: &PreparedCollectionKey,
        removed: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i64_local(f);
        let cursor = s.reserve_i32_local(f);
        let live = s.reserve_i64_local(f);
        self.emit_collection_find_prepared_map(record, prepared, index, removed, f)?;
        removed.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>()
                .field(MapObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.emit_collection_unlink_map(record, prepared, index, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        s.array_type::<MapEntryTable>()
            .write(&table, cursor, GcOperand::null(s), s, f);
        s.struct_type::<MapObject>()
            .field(MapObjectSchema::LIVE_COUNT)
            .read(record, s, f)
            .store_i64(live, f);
        self.emit_increment_local(live, -1, f);
        s.struct_type::<MapObject>()
            .field(MapObjectSchema::LIVE_COUNT)
            .write(record, GcOperand::i64_local(live), s, f);
        table.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(live, f);
        s.release_i32_local(cursor, f);
        s.release_i64_local(index, f);
        Ok(())
    }
    pub(super) fn emit_collection_put_map(
        &mut self,
        record: &GcLocal<MapObject>,
        key: &ValueLocals,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let prepared = self.emit_collection_prepare_key(key, f);
        let result = self.emit_collection_put_prepared_map(record, &prepared, value, f);
        prepared.release(self, f);
        result
    }

    pub(super) fn emit_collection_put_prepared_map(
        &mut self,
        record: &GcLocal<MapObject>,
        prepared: &PreparedCollectionKey,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let key = &prepared.value;
        let index = s.reserve_i64_local(f);
        let found = s.reserve_i32_local(f);
        let cursor = s.reserve_i32_local(f);
        let live = s.reserve_i64_local(f);
        self.emit_collection_find_prepared_map(record, prepared, index, found, f)?;
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>()
                .field(MapObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let entry = s.reserve_gc_local(f).initialize(
            s.array_type::<MapEntryTable>()
                .read(&table, cursor, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let stored = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(value, f), f);
        s.struct_type::<MapEntry>()
            .field(MapEntrySchema::VALUE)
            .write(&entry, GcOperand::reference(&stored, s), s, f);
        stored.clear(f);
        entry.clear(f);
        table.clear(f);
        f.instruction(&Instruction::Else);
        // Append never reuses a tombstone: deletion/reinsertion moves to the end.
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>()
                .field(MapObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.emit_collection_grow_map(record, &table, index, f)?;
        let hash = prepared.hash;
        let head = s.reserve_i64_local(f);
        let bucket = s.reserve_i32_local(f);
        let buckets = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>()
                .field(MapObjectSchema::HASH_INDEX)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.emit_collection_hash_bucket(&buckets, hash, bucket, f);
        s.array_type::<CollectionHashTable>()
            .read(&buckets, bucket, s, f)
            .store_i64(head, f);
        let stored_value = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(value, f), f);
        let stored_key = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(key, f), f);
        let entry = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapEntry>().construct(
                (
                    GcOperand::reference(&stored_key, s),
                    GcOperand::reference(&stored_value, s),
                    GcOperand::i64_local(head),
                ),
                f,
            ),
            f,
        );
        stored_key.clear(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        s.array_type::<MapEntryTable>().write(
            &table,
            cursor,
            GcOperand::nullable_reference(&entry, s),
            s,
            f,
        );
        let ordinals = s.reserve_gc_local(f).initialize(
            s.field(MapObjectSchema::ORDINALS)
                .read(record, s, f)
                .reference(),
            f,
        );
        let ordinal = s.reserve_i64_local(f);
        s.field(MapObjectSchema::NEXT_ORDINAL)
            .read(record, s, f)
            .store_i64(ordinal, f);
        s.array_type::<NumberBitsI64Array>().write(
            &ordinals,
            cursor,
            GcOperand::i64_local(ordinal),
            s,
            f,
        );
        self.emit_collection_advance_counter(ordinal, f);
        s.field(MapObjectSchema::NEXT_ORDINAL)
            .write(record, GcOperand::i64_local(ordinal), s, f);
        s.release_i64_local(ordinal, f);
        ordinals.clear(f);
        self.emit_increment_local(index, 1, f);
        s.array_type::<CollectionHashTable>().write(
            &buckets,
            bucket,
            GcOperand::i64_local(index),
            s,
            f,
        );
        s.struct_type::<MapObject>()
            .field(MapObjectSchema::HISTORY_LENGTH)
            .write(record, GcOperand::i64_local(index), s, f);
        s.struct_type::<MapObject>()
            .field(MapObjectSchema::LIVE_COUNT)
            .read(record, s, f)
            .store_i64(live, f);
        self.emit_increment_local(live, 1, f);
        s.struct_type::<MapObject>()
            .field(MapObjectSchema::LIVE_COUNT)
            .write(record, GcOperand::i64_local(live), s, f);
        buckets.clear(f);
        s.release_i32_local(bucket, f);
        s.release_i64_local(head, f);
        entry.clear(f);
        stored_value.clear(f);
        table.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(live, f);
        s.release_i32_local(cursor, f);
        s.release_i32_local(found, f);
        s.release_i64_local(index, f);
        Ok(())
    }
    pub(super) fn emit_collection_read_map(
        &mut self,
        record: &GcLocal<MapObject>,
        index: I64Local,
        key: &ValueLocals,
        value: &ValueLocals,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let cursor = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>()
                .field(MapObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        let entry = s.reserve_gc_local(f).initialize(
            s.array_type::<MapEntryTable>()
                .read(&table, cursor, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapEntry>()
                .field(MapEntrySchema::KEY)
                .read(&entry, s, f)
                .reference(),
            f,
        );
        self.emit_stored_value_to_locals(&stored, key, f);
        stored.clear(f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapEntry>()
                .field(MapEntrySchema::VALUE)
                .read(&entry, s, f)
                .reference(),
            f,
        );
        self.emit_stored_value_to_locals(&stored, value, f);
        stored.clear(f);
        entry.clear(f);
        table.clear(f);
        s.release_i32_local(cursor, f);
    }
    // Advances through a fresh table and length on every step, including after callbacks.
    pub(super) fn emit_collection_next_map(
        &mut self,
        record: &GcLocal<MapObject>,
        cursor: &CollectionCursorLocals,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::CollectionNextMapArguments::new(
                    record,
                    cursor.index,
                    cursor.next_ordinal,
                    cursor.version,
                ),
                self.runtime_helper_base()?,
                f,
            )
            .store(cursor.index, cursor.next_ordinal, cursor.version, found, f);
        Ok(())
    }

    pub(crate) fn compile_collection_next_map_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::CollectionNextMap);
        let p =
            self.helper_parameters::<crate::runtime_helpers::CollectionNextMapParameters>(&mut f);
        let s = self.runtime_schema();
        let count = s.reserve_i64_local(&mut f);
        let version = s.reserve_i64_local(&mut f);
        let low = s.reserve_i64_local(&mut f);
        let high = s.reserve_i64_local(&mut f);
        let middle = s.reserve_i64_local(&mut f);
        let index = s.reserve_i32_local(&mut f);
        let found = s.reserve_i32_local(&mut f);
        let table = s.reserve_gc_local(&mut f).initialize(
            s.field(MapObjectSchema::ENTRIES)
                .read(&p.record, s, &mut f)
                .reference(),
            &mut f,
        );
        let ordinals = s.reserve_gc_local(&mut f).initialize(
            s.field(MapObjectSchema::ORDINALS)
                .read(&p.record, s, &mut f)
                .reference(),
            &mut f,
        );
        s.field(MapObjectSchema::HISTORY_LENGTH)
            .read(&p.record, s, &mut f)
            .store_i64(count, &mut f);
        s.field(MapObjectSchema::VERSION)
            .read(&p.record, s, &mut f)
            .store_i64(version, &mut f);
        version.load(&mut f);
        p.version.load(&mut f);
        f.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, &mut f);
        // Only storage replacement invalidates the O(1) physical cursor. The
        // strictly increasing ordinal array admits a lower-bound search.
        f.instruction(&Instruction::I64Const(0));
        low.store(&mut f);
        count.load(&mut f);
        high.store(&mut f);
        let searched = self.open_frame(ControlFrameKind::Block, &mut f);
        let search = self.open_frame(ControlFrameKind::Loop, &mut f);
        low.load(&mut f);
        high.load(&mut f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(searched, &mut f);
        low.load(&mut f);
        high.load(&mut f);
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64ShrU);
        middle.store(&mut f);
        middle.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        index.store(&mut f);
        s.array_type::<NumberBitsI64Array>()
            .read(&ordinals, index, s, &mut f)
            .store_i64(p.version, &mut f);
        p.version.load(&mut f);
        p.next_ordinal.load(&mut f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        middle.load(&mut f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        low.store(&mut f);
        f.instruction(&Instruction::Else);
        middle.load(&mut f);
        high.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(search, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        low.load(&mut f);
        p.index.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(0));
        found.store(&mut f);
        let exit = self.open_frame(ControlFrameKind::Block, &mut f);
        let again = self.open_frame(ControlFrameKind::Loop, &mut f);
        p.index.load(&mut f);
        count.load(&mut f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, &mut f);
        p.index.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        index.store(&mut f);
        s.array_type::<MapEntryTable>()
            .read(&table, index, s, &mut f)
            .reference()
            .is_null(&mut f);
        f.instruction(&Instruction::I32Eqz);
        found.store(&mut f);
        found.load(&mut f);
        self.emit_branch_if_to_target(exit, &mut f);
        self.emit_increment_local(p.index, 1, &mut f);
        self.emit_branch_to_target(again, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        found.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        s.array_type::<NumberBitsI64Array>()
            .read(&ordinals, index, s, &mut f)
            .store_i64(p.next_ordinal, &mut f);
        self.emit_collection_advance_counter(p.next_ordinal, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        ordinals.clear(&mut f);
        table.clear(&mut f);
        s.release_i32_local(index, &mut f);
        s.release_i64_local(middle, &mut f);
        s.release_i64_local(high, &mut f);
        s.release_i64_local(low, &mut f);
        s.release_i64_local(count, &mut f);
        p.index.load(&mut f);
        p.next_ordinal.load(&mut f);
        version.load(&mut f);
        found.load(&mut f);
        s.release_i32_local(found, &mut f);
        s.release_i64_local(version, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
    pub(super) fn emit_collection_alloc_set(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        f: &mut Function,
    ) -> GcLocal<SetObject> {
        let s = self.runtime_schema();
        let zero = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        zero.store(f);
        let entries = s.reserve_gc_local(f).initialize(
            s.array_type::<SetEntryTable>()
                .filled(GcOperand::null(s), zero, f),
            f,
        );
        f.instruction(&Instruction::I32Const(4));
        zero.store(f);
        let buckets = s.reserve_gc_local(f).initialize(
            s.array_type::<CollectionHashTable>()
                .filled(GcOperand::i64(0), zero, f),
            f,
        );
        let ordinals = s.reserve_gc_local(f).initialize(s.empty_number_bits(f), f);
        let record = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetObject>().construct(
                (
                    GcOperand::reference(header, s),
                    GcOperand::reference(&entries, s),
                    GcOperand::i64(0),
                    GcOperand::i64(0),
                    GcOperand::reference(&buckets, s),
                    GcOperand::reference(&ordinals, s),
                    GcOperand::i64(0),
                    GcOperand::i64(0),
                ),
                f,
            ),
            f,
        );
        ordinals.clear(f);
        buckets.clear(f);
        entries.clear(f);
        s.release_i32_local(zero, f);
        record
    }
    pub(super) fn emit_collection_find_set(
        &mut self,
        record: &GcLocal<SetObject>,
        key: &ValueLocals,
        index: I64Local,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let prepared = self.emit_collection_prepare_key(key, f);
        let result = self.emit_collection_find_prepared_set(record, &prepared, index, found, f);
        prepared.release(self, f);
        result
    }

    pub(super) fn emit_collection_find_prepared_set(
        &mut self,
        record: &GcLocal<SetObject>,
        prepared: &PreparedCollectionKey,
        index: I64Local,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let key = &prepared.value;
        let hash = prepared.hash;
        let cursor = s.reserve_i32_local(f);
        let bucket = s.reserve_i32_local(f);
        let chain = s.reserve_i64_local(f);
        let retained = s.reserve_value_local(f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetObject>()
                .field(SetObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        let buckets = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetObject>()
                .field(SetObjectSchema::HASH_INDEX)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.emit_collection_hash_bucket(&buckets, hash, bucket, f);
        s.array_type::<CollectionHashTable>()
            .read(&buckets, bucket, s, f)
            .store_i64(chain, f);
        f.instruction(&Instruction::I32Const(0));
        found.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        chain.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(exit, f);
        chain.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        index.store(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let entry = s.reserve_gc_local(f).initialize(
            s.array_type::<SetEntryTable>()
                .read(&table, cursor, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetEntry>()
                .field(SetEntrySchema::VALUE)
                .read(&entry, s, f)
                .reference(),
            f,
        );
        self.emit_stored_value_to_locals(&stored, &retained, f);
        self.emit_tagged_payload_same_value_zero_i32(&retained, key, f)?;
        found.store(f);
        s.struct_type::<SetEntry>()
            .field(SetEntrySchema::HASH_NEXT)
            .read(&entry, s, f)
            .store_i64(chain, f);
        stored.clear(f);
        entry.clear(f);
        found.load(f);
        self.emit_branch_if_to_target(exit, f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.struct_type::<SetObject>()
            .field(SetObjectSchema::HISTORY_LENGTH)
            .read(record, s, f)
            .store_i64(index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        retained.clear(f);
        buckets.clear(f);
        table.clear(f);
        s.release_i64_local(chain, f);
        s.release_i32_local(bucket, f);
        s.release_i32_local(cursor, f);
        Ok(())
    }

    fn emit_collection_grow_set(
        &mut self,
        record: &GcLocal<SetObject>,
        table: &GcLocal<SetEntryTable>,
        count: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // Physical history, rather than live occupancy, owns the next slot.
        // Below this u32 array length the helper's address-limit trap is inert.
        count.load(f);
        s.array_type::<SetEntryTable>().length(table, s, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, f);
        s.call_helper(
            crate::runtime_helpers::CollectionPrepareSetArguments::new(record, count),
            self.runtime_helper_base()?,
            f,
        )
        .store(count, f);
        table.replace(
            s.field(SetObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_collection_prepare_set_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::CollectionPrepareSet);
        let p = self
            .helper_parameters::<crate::runtime_helpers::CollectionPrepareSetParameters>(&mut f);
        let s = self.runtime_schema();
        let capacity = s.reserve_i32_local(&mut f);
        let source = s.reserve_i32_local(&mut f);
        let destination = s.reserve_i32_local(&mut f);
        let live = s.reserve_i64_local(&mut f);
        let version = s.reserve_i64_local(&mut f);
        let table = s.reserve_gc_local(&mut f).initialize(
            s.field(SetObjectSchema::ENTRIES)
                .read(&p.record, s, &mut f)
                .reference(),
            &mut f,
        );
        s.array_type::<SetEntryTable>().length(&table, s, &mut f);
        capacity.store(&mut f);
        p.count.load(&mut f);
        capacity.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, &mut f);
        // Bound physical history by live occupancy, preserving insertion order.
        s.field(SetObjectSchema::LIVE_COUNT)
            .read(&p.record, s, &mut f)
            .store_i64(live, &mut f);
        live.load(&mut f);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64Mul);
        live.store(&mut f);
        live.load(&mut f);
        f.instruction(&Instruction::I64Const(4));
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::I64Const(4));
        live.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        live.load(&mut f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        live.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        live.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        capacity.store(&mut f);
        let old_ordinals = s.reserve_gc_local(&mut f).initialize(
            s.field(SetObjectSchema::ORDINALS)
                .read(&p.record, s, &mut f)
                .reference(),
            &mut f,
        );
        let replacement = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<SetEntryTable>()
                .filled(GcOperand::null(s), capacity, &mut f),
            &mut f,
        );
        let ordinals = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<NumberBitsI64Array>()
                .filled(GcOperand::i64(0), capacity, &mut f),
            &mut f,
        );
        f.instruction(&Instruction::I32Const(0));
        source.store(&mut f);
        f.instruction(&Instruction::I32Const(0));
        destination.store(&mut f);
        let exit = self.open_frame(ControlFrameKind::Block, &mut f);
        let again = self.open_frame(ControlFrameKind::Loop, &mut f);
        source.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        p.count.load(&mut f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, &mut f);
        let entry = s.reserve_gc_local(&mut f).initialize(
            s.array_type::<SetEntryTable>()
                .read(&table, source, s, &mut f)
                .reference(),
            &mut f,
        );
        entry.load(s, &mut f).is_null(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        s.array_type::<SetEntryTable>().write(
            &replacement,
            destination,
            GcOperand::reference(&entry, s),
            s,
            &mut f,
        );
        s.array_type::<NumberBitsI64Array>()
            .read(&old_ordinals, source, s, &mut f)
            .store_i64(live, &mut f);
        s.array_type::<NumberBitsI64Array>().write(
            &ordinals,
            destination,
            GcOperand::i64_local(live),
            s,
            &mut f,
        );
        destination.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        destination.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        entry.clear(&mut f);
        source.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        source.store(&mut f);
        self.emit_branch_to_target(again, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        destination.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        p.count.store(&mut f);
        s.field(SetObjectSchema::ENTRIES).write(
            &p.record,
            GcOperand::reference(&replacement, s),
            s,
            &mut f,
        );
        s.field(SetObjectSchema::ORDINALS).write(
            &p.record,
            GcOperand::reference(&ordinals, s),
            s,
            &mut f,
        );
        s.field(SetObjectSchema::HISTORY_LENGTH).write(
            &p.record,
            GcOperand::i64_local(p.count),
            s,
            &mut f,
        );
        s.field(SetObjectSchema::VERSION)
            .read(&p.record, s, &mut f)
            .store_i64(version, &mut f);
        self.emit_collection_advance_counter(version, &mut f);
        s.field(SetObjectSchema::VERSION).write(
            &p.record,
            GcOperand::i64_local(version),
            s,
            &mut f,
        );
        self.emit_collection_rehash_set(&p.record, capacity, &mut f);
        ordinals.clear(&mut f);
        replacement.clear(&mut f);
        old_ordinals.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // This private array address limit is a resource trap, not a JS error.
        p.count.load(&mut f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        f.instruction(&Instruction::I64GeU);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        table.clear(&mut f);
        s.release_i64_local(version, &mut f);
        s.release_i64_local(live, &mut f);
        s.release_i32_local(destination, &mut f);
        s.release_i32_local(source, &mut f);
        s.release_i32_local(capacity, &mut f);
        p.count.load(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
    pub(super) fn emit_collection_clear_set(
        &mut self,
        record: &GcLocal<SetObject>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let count = s.reserve_i64_local(f);
        s.field(SetObjectSchema::HISTORY_LENGTH)
            .read(record, s, f)
            .store_i64(count, f);
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let capacity = s.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        capacity.store(f);
        let entries = s.reserve_gc_local(f).initialize(
            s.array_type::<SetEntryTable>()
                .filled(GcOperand::null(s), capacity, f),
            f,
        );
        f.instruction(&Instruction::I32Const(4));
        capacity.store(f);
        let buckets = s.reserve_gc_local(f).initialize(
            s.array_type::<CollectionHashTable>()
                .filled(GcOperand::i64(0), capacity, f),
            f,
        );
        let ordinals = s.reserve_gc_local(f).initialize(s.empty_number_bits(f), f);
        s.field(SetObjectSchema::ENTRIES)
            .write(record, GcOperand::reference(&entries, s), s, f);
        s.field(SetObjectSchema::HASH_INDEX)
            .write(record, GcOperand::reference(&buckets, s), s, f);
        s.field(SetObjectSchema::ORDINALS)
            .write(record, GcOperand::reference(&ordinals, s), s, f);
        s.field(SetObjectSchema::LIVE_COUNT)
            .write(record, GcOperand::i64(0), s, f);
        s.field(SetObjectSchema::HISTORY_LENGTH)
            .write(record, GcOperand::i64(0), s, f);
        s.field(SetObjectSchema::VERSION)
            .read(record, s, f)
            .store_i64(count, f);
        self.emit_collection_advance_counter(count, f);
        s.field(SetObjectSchema::VERSION)
            .write(record, GcOperand::i64_local(count), s, f);
        // NEXT_ORDINAL survives: existing cursors still observe later inserts.
        ordinals.clear(f);
        buckets.clear(f);
        entries.clear(f);
        s.release_i32_local(capacity, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(count, f);
    }
    pub(super) fn emit_collection_delete_set(
        &mut self,
        record: &GcLocal<SetObject>,
        key: &ValueLocals,
        removed: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let prepared = self.emit_collection_prepare_key(key, f);
        let result = self.emit_collection_delete_prepared_set(record, &prepared, removed, f);
        prepared.release(self, f);
        result
    }
    pub(super) fn emit_collection_delete_prepared_set(
        &mut self,
        record: &GcLocal<SetObject>,
        prepared: &PreparedCollectionKey,
        removed: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i64_local(f);
        let cursor = s.reserve_i32_local(f);
        let live = s.reserve_i64_local(f);
        self.emit_collection_find_prepared_set(record, prepared, index, removed, f)?;
        removed.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetObject>()
                .field(SetObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.emit_collection_unlink_set(record, prepared, index, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        s.array_type::<SetEntryTable>()
            .write(&table, cursor, GcOperand::null(s), s, f);
        s.struct_type::<SetObject>()
            .field(SetObjectSchema::LIVE_COUNT)
            .read(record, s, f)
            .store_i64(live, f);
        self.emit_increment_local(live, -1, f);
        s.struct_type::<SetObject>()
            .field(SetObjectSchema::LIVE_COUNT)
            .write(record, GcOperand::i64_local(live), s, f);
        table.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(live, f);
        s.release_i32_local(cursor, f);
        s.release_i64_local(index, f);
        Ok(())
    }
    pub(super) fn emit_collection_put_set(
        &mut self,
        record: &GcLocal<SetObject>,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let prepared = self.emit_collection_prepare_key(value, f);
        let result = self.emit_collection_put_prepared_set(record, &prepared, f);
        prepared.release(self, f);
        result
    }
    pub(super) fn emit_collection_put_prepared_set(
        &mut self,
        record: &GcLocal<SetObject>,
        prepared: &PreparedCollectionKey,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i64_local(f);
        let found = s.reserve_i32_local(f);
        let cursor = s.reserve_i32_local(f);
        let live = s.reserve_i64_local(f);
        let value = &prepared.value;
        self.emit_collection_find_prepared_set(record, prepared, index, found, f)?;
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        // Append never reuses a tombstone: deletion/reinsertion moves to the end.
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetObject>()
                .field(SetObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.emit_collection_grow_set(record, &table, index, f)?;
        let hash = prepared.hash;
        let head = s.reserve_i64_local(f);
        let bucket = s.reserve_i32_local(f);
        let buckets = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetObject>()
                .field(SetObjectSchema::HASH_INDEX)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.emit_collection_hash_bucket(&buckets, hash, bucket, f);
        s.array_type::<CollectionHashTable>()
            .read(&buckets, bucket, s, f)
            .store_i64(head, f);
        let stored_value = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(value, f), f);
        let entry = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetEntry>().construct(
                (
                    GcOperand::reference(&stored_value, s),
                    GcOperand::i64_local(head),
                ),
                f,
            ),
            f,
        );
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        s.array_type::<SetEntryTable>().write(
            &table,
            cursor,
            GcOperand::nullable_reference(&entry, s),
            s,
            f,
        );
        let ordinals = s.reserve_gc_local(f).initialize(
            s.field(SetObjectSchema::ORDINALS)
                .read(record, s, f)
                .reference(),
            f,
        );
        let ordinal = s.reserve_i64_local(f);
        s.field(SetObjectSchema::NEXT_ORDINAL)
            .read(record, s, f)
            .store_i64(ordinal, f);
        s.array_type::<NumberBitsI64Array>().write(
            &ordinals,
            cursor,
            GcOperand::i64_local(ordinal),
            s,
            f,
        );
        self.emit_collection_advance_counter(ordinal, f);
        s.field(SetObjectSchema::NEXT_ORDINAL)
            .write(record, GcOperand::i64_local(ordinal), s, f);
        s.release_i64_local(ordinal, f);
        ordinals.clear(f);
        self.emit_increment_local(index, 1, f);
        s.array_type::<CollectionHashTable>().write(
            &buckets,
            bucket,
            GcOperand::i64_local(index),
            s,
            f,
        );
        s.struct_type::<SetObject>()
            .field(SetObjectSchema::HISTORY_LENGTH)
            .write(record, GcOperand::i64_local(index), s, f);
        s.struct_type::<SetObject>()
            .field(SetObjectSchema::LIVE_COUNT)
            .read(record, s, f)
            .store_i64(live, f);
        self.emit_increment_local(live, 1, f);
        s.struct_type::<SetObject>()
            .field(SetObjectSchema::LIVE_COUNT)
            .write(record, GcOperand::i64_local(live), s, f);
        buckets.clear(f);
        s.release_i32_local(bucket, f);
        s.release_i64_local(head, f);
        entry.clear(f);
        stored_value.clear(f);
        table.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(live, f);
        s.release_i32_local(cursor, f);
        s.release_i32_local(found, f);
        s.release_i64_local(index, f);
        Ok(())
    }
    pub(super) fn emit_collection_read_set(
        &mut self,
        record: &GcLocal<SetObject>,
        index: I64Local,
        value: &ValueLocals,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let cursor = s.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetObject>()
                .field(SetObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        let entry = s.reserve_gc_local(f).initialize(
            s.array_type::<SetEntryTable>()
                .read(&table, cursor, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetEntry>()
                .field(SetEntrySchema::VALUE)
                .read(&entry, s, f)
                .reference(),
            f,
        );
        self.emit_stored_value_to_locals(&stored, value, f);
        stored.clear(f);
        entry.clear(f);
        table.clear(f);
        s.release_i32_local(cursor, f);
    }
    // Advances through a fresh table and length on every step, including after callbacks.
    pub(super) fn emit_collection_next_set(
        &mut self,
        record: &GcLocal<SetObject>,
        cursor: &CollectionCursorLocals,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::CollectionNextSetArguments::new(
                    record,
                    cursor.index,
                    cursor.next_ordinal,
                    cursor.version,
                ),
                self.runtime_helper_base()?,
                f,
            )
            .store(cursor.index, cursor.next_ordinal, cursor.version, found, f);
        Ok(())
    }

    pub(crate) fn compile_collection_next_set_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::CollectionNextSet);
        let p =
            self.helper_parameters::<crate::runtime_helpers::CollectionNextSetParameters>(&mut f);
        let s = self.runtime_schema();
        let count = s.reserve_i64_local(&mut f);
        let version = s.reserve_i64_local(&mut f);
        let low = s.reserve_i64_local(&mut f);
        let high = s.reserve_i64_local(&mut f);
        let middle = s.reserve_i64_local(&mut f);
        let index = s.reserve_i32_local(&mut f);
        let found = s.reserve_i32_local(&mut f);
        let table = s.reserve_gc_local(&mut f).initialize(
            s.field(SetObjectSchema::ENTRIES)
                .read(&p.record, s, &mut f)
                .reference(),
            &mut f,
        );
        let ordinals = s.reserve_gc_local(&mut f).initialize(
            s.field(SetObjectSchema::ORDINALS)
                .read(&p.record, s, &mut f)
                .reference(),
            &mut f,
        );
        s.field(SetObjectSchema::HISTORY_LENGTH)
            .read(&p.record, s, &mut f)
            .store_i64(count, &mut f);
        s.field(SetObjectSchema::VERSION)
            .read(&p.record, s, &mut f)
            .store_i64(version, &mut f);
        version.load(&mut f);
        p.version.load(&mut f);
        f.instruction(&Instruction::I64Ne);
        self.open_frame(ControlFrameKind::If, &mut f);
        // Only storage replacement invalidates the O(1) physical cursor. The
        // strictly increasing ordinal array admits a lower-bound search.
        f.instruction(&Instruction::I64Const(0));
        low.store(&mut f);
        count.load(&mut f);
        high.store(&mut f);
        let searched = self.open_frame(ControlFrameKind::Block, &mut f);
        let search = self.open_frame(ControlFrameKind::Loop, &mut f);
        low.load(&mut f);
        high.load(&mut f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(searched, &mut f);
        low.load(&mut f);
        high.load(&mut f);
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64ShrU);
        middle.store(&mut f);
        middle.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        index.store(&mut f);
        s.array_type::<NumberBitsI64Array>()
            .read(&ordinals, index, s, &mut f)
            .store_i64(p.version, &mut f);
        p.version.load(&mut f);
        p.next_ordinal.load(&mut f);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        middle.load(&mut f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        low.store(&mut f);
        f.instruction(&Instruction::Else);
        middle.load(&mut f);
        high.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(search, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        low.load(&mut f);
        p.index.store(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(0));
        found.store(&mut f);
        let exit = self.open_frame(ControlFrameKind::Block, &mut f);
        let again = self.open_frame(ControlFrameKind::Loop, &mut f);
        p.index.load(&mut f);
        count.load(&mut f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, &mut f);
        p.index.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        index.store(&mut f);
        s.array_type::<SetEntryTable>()
            .read(&table, index, s, &mut f)
            .reference()
            .is_null(&mut f);
        f.instruction(&Instruction::I32Eqz);
        found.store(&mut f);
        found.load(&mut f);
        self.emit_branch_if_to_target(exit, &mut f);
        self.emit_increment_local(p.index, 1, &mut f);
        self.emit_branch_to_target(again, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        found.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        s.array_type::<NumberBitsI64Array>()
            .read(&ordinals, index, s, &mut f)
            .store_i64(p.next_ordinal, &mut f);
        self.emit_collection_advance_counter(p.next_ordinal, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        ordinals.clear(&mut f);
        table.clear(&mut f);
        s.release_i32_local(index, &mut f);
        s.release_i64_local(middle, &mut f);
        s.release_i64_local(high, &mut f);
        s.release_i64_local(low, &mut f);
        s.release_i64_local(count, &mut f);
        p.index.load(&mut f);
        p.next_ordinal.load(&mut f);
        version.load(&mut f);
        found.load(&mut f);
        s.release_i32_local(found, &mut f);
        s.release_i64_local(version, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
    fn emit_collection_rehash_map(
        &mut self,
        record: &GcLocal<MapObject>,
        capacity: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let count = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let cursor = s.reserve_i32_local(f);
        let bucket = s.reserve_i32_local(f);
        let hash = s.reserve_i64_local(f);
        let head = s.reserve_i64_local(f);
        let key = s.reserve_value_local(f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>()
                .field(MapObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        let buckets = s.reserve_gc_local(f).initialize(
            s.array_type::<CollectionHashTable>()
                .filled(GcOperand::i64(0), capacity, f),
            f,
        );
        s.struct_type::<MapObject>()
            .field(MapObjectSchema::HISTORY_LENGTH)
            .read(record, s, f)
            .store_i64(count, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let maybe = s.reserve_gc_local(f).initialize(
            s.array_type::<MapEntryTable>()
                .read(&table, cursor, s, f)
                .reference(),
            f,
        );
        maybe.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let entry = s
            .reserve_gc_local(f)
            .initialize(maybe.load(s, f).require_non_null(f), f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapEntry>()
                .field(MapEntrySchema::KEY)
                .read(&entry, s, f)
                .reference(),
            f,
        );
        self.emit_stored_value_to_locals(&stored, &key, f);
        self.emit_collection_key_hash(&key, hash, f);
        self.emit_collection_hash_bucket(&buckets, hash, bucket, f);
        s.array_type::<CollectionHashTable>()
            .read(&buckets, bucket, s, f)
            .store_i64(head, f);
        s.struct_type::<MapEntry>()
            .field(MapEntrySchema::HASH_NEXT)
            .write(&entry, GcOperand::i64_local(head), s, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        head.store(f);
        s.array_type::<CollectionHashTable>().write(
            &buckets,
            bucket,
            GcOperand::i64_local(head),
            s,
            f,
        );
        stored.clear(f);
        entry.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        maybe.clear(f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.struct_type::<MapObject>()
            .field(MapObjectSchema::HASH_INDEX)
            .write(record, GcOperand::reference(&buckets, s), s, f);
        key.clear(f);
        buckets.clear(f);
        table.clear(f);
        s.release_i64_local(head, f);
        s.release_i64_local(hash, f);
        s.release_i32_local(bucket, f);
        s.release_i32_local(cursor, f);
        s.release_i64_local(index, f);
        s.release_i64_local(count, f);
    }
    fn emit_collection_unlink_map(
        &mut self,
        record: &GcLocal<MapObject>,
        prepared: &PreparedCollectionKey,
        index: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let cursor = s.reserve_i32_local(f);
        let bucket = s.reserve_i32_local(f);
        let hash = prepared.hash;
        let chain = s.reserve_i64_local(f);
        let previous = s.reserve_i64_local(f);
        let next = s.reserve_i64_local(f);
        let wanted = s.reserve_i64_local(f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>()
                .field(MapObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        let buckets = s.reserve_gc_local(f).initialize(
            s.struct_type::<MapObject>()
                .field(MapObjectSchema::HASH_INDEX)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.emit_collection_hash_bucket(&buckets, hash, bucket, f);
        s.array_type::<CollectionHashTable>()
            .read(&buckets, bucket, s, f)
            .store_i64(chain, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        wanted.store(f);
        f.instruction(&Instruction::I64Const(0));
        previous.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        chain.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        chain.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let entry = s.reserve_gc_local(f).initialize(
            s.array_type::<MapEntryTable>()
                .read(&table, cursor, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        s.struct_type::<MapEntry>()
            .field(MapEntrySchema::HASH_NEXT)
            .read(&entry, s, f)
            .store_i64(next, f);
        entry.clear(f);
        chain.load(f);
        wanted.load(f);
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        previous.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.array_type::<CollectionHashTable>().write(
            &buckets,
            bucket,
            GcOperand::i64_local(next),
            s,
            f,
        );
        f.instruction(&Instruction::Else);
        previous.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let parent = s.reserve_gc_local(f).initialize(
            s.array_type::<MapEntryTable>()
                .read(&table, cursor, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        s.struct_type::<MapEntry>()
            .field(MapEntrySchema::HASH_NEXT)
            .write(&parent, GcOperand::i64_local(next), s, f);
        parent.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        chain.load(f);
        previous.store(f);
        next.load(f);
        chain.store(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        buckets.clear(f);
        table.clear(f);
        s.release_i64_local(wanted, f);
        s.release_i64_local(next, f);
        s.release_i64_local(previous, f);
        s.release_i64_local(chain, f);
        s.release_i32_local(bucket, f);
        s.release_i32_local(cursor, f);
    }

    fn emit_collection_rehash_set(
        &mut self,
        record: &GcLocal<SetObject>,
        capacity: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let count = s.reserve_i64_local(f);
        let index = s.reserve_i64_local(f);
        let cursor = s.reserve_i32_local(f);
        let bucket = s.reserve_i32_local(f);
        let hash = s.reserve_i64_local(f);
        let head = s.reserve_i64_local(f);
        let key = s.reserve_value_local(f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetObject>()
                .field(SetObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        let buckets = s.reserve_gc_local(f).initialize(
            s.array_type::<CollectionHashTable>()
                .filled(GcOperand::i64(0), capacity, f),
            f,
        );
        s.struct_type::<SetObject>()
            .field(SetObjectSchema::HISTORY_LENGTH)
            .read(record, s, f)
            .store_i64(count, f);
        f.instruction(&Instruction::I64Const(0));
        index.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        count.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let maybe = s.reserve_gc_local(f).initialize(
            s.array_type::<SetEntryTable>()
                .read(&table, cursor, s, f)
                .reference(),
            f,
        );
        maybe.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let entry = s
            .reserve_gc_local(f)
            .initialize(maybe.load(s, f).require_non_null(f), f);
        let stored = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetEntry>()
                .field(SetEntrySchema::VALUE)
                .read(&entry, s, f)
                .reference(),
            f,
        );
        self.emit_stored_value_to_locals(&stored, &key, f);
        self.emit_collection_key_hash(&key, hash, f);
        self.emit_collection_hash_bucket(&buckets, hash, bucket, f);
        s.array_type::<CollectionHashTable>()
            .read(&buckets, bucket, s, f)
            .store_i64(head, f);
        s.struct_type::<SetEntry>()
            .field(SetEntrySchema::HASH_NEXT)
            .write(&entry, GcOperand::i64_local(head), s, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        head.store(f);
        s.array_type::<CollectionHashTable>().write(
            &buckets,
            bucket,
            GcOperand::i64_local(head),
            s,
            f,
        );
        stored.clear(f);
        entry.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        maybe.clear(f);
        self.emit_increment_local(index, 1, f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.struct_type::<SetObject>()
            .field(SetObjectSchema::HASH_INDEX)
            .write(record, GcOperand::reference(&buckets, s), s, f);
        key.clear(f);
        buckets.clear(f);
        table.clear(f);
        s.release_i64_local(head, f);
        s.release_i64_local(hash, f);
        s.release_i32_local(bucket, f);
        s.release_i32_local(cursor, f);
        s.release_i64_local(index, f);
        s.release_i64_local(count, f);
    }
    fn emit_collection_unlink_set(
        &mut self,
        record: &GcLocal<SetObject>,
        prepared: &PreparedCollectionKey,
        index: I64Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let cursor = s.reserve_i32_local(f);
        let bucket = s.reserve_i32_local(f);
        let hash = prepared.hash;
        let chain = s.reserve_i64_local(f);
        let previous = s.reserve_i64_local(f);
        let next = s.reserve_i64_local(f);
        let wanted = s.reserve_i64_local(f);
        let table = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetObject>()
                .field(SetObjectSchema::ENTRIES)
                .read(record, s, f)
                .reference(),
            f,
        );
        let buckets = s.reserve_gc_local(f).initialize(
            s.struct_type::<SetObject>()
                .field(SetObjectSchema::HASH_INDEX)
                .read(record, s, f)
                .reference(),
            f,
        );
        self.emit_collection_hash_bucket(&buckets, hash, bucket, f);
        s.array_type::<CollectionHashTable>()
            .read(&buckets, bucket, s, f)
            .store_i64(chain, f);
        index.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        wanted.store(f);
        f.instruction(&Instruction::I64Const(0));
        previous.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let again = self.open_frame(ControlFrameKind::Loop, f);
        chain.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        chain.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let entry = s.reserve_gc_local(f).initialize(
            s.array_type::<SetEntryTable>()
                .read(&table, cursor, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        s.struct_type::<SetEntry>()
            .field(SetEntrySchema::HASH_NEXT)
            .read(&entry, s, f)
            .store_i64(next, f);
        entry.clear(f);
        chain.load(f);
        wanted.load(f);
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        previous.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        s.array_type::<CollectionHashTable>().write(
            &buckets,
            bucket,
            GcOperand::i64_local(next),
            s,
            f,
        );
        f.instruction(&Instruction::Else);
        previous.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        f.instruction(&Instruction::I32WrapI64);
        cursor.store(f);
        let parent = s.reserve_gc_local(f).initialize(
            s.array_type::<SetEntryTable>()
                .read(&table, cursor, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        s.struct_type::<SetEntry>()
            .field(SetEntrySchema::HASH_NEXT)
            .write(&parent, GcOperand::i64_local(next), s, f);
        parent.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        chain.load(f);
        previous.store(f);
        next.load(f);
        chain.store(f);
        self.emit_branch_to_target(again, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        buckets.clear(f);
        table.clear(f);
        s.release_i64_local(wanted, f);
        s.release_i64_local(next, f);
        s.release_i64_local(previous, f);
        s.release_i64_local(chain, f);
        s.release_i32_local(bucket, f);
        s.release_i32_local(cursor, f);
    }
}
