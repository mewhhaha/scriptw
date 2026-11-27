//! A compact own-key phase borrows immutable shape keys. Structural mutation
//! promotes storage, so identity is the complete live-descriptor guard.
use super::enumeration::ForInVisitedKeys;
use super::*;
use crate::operations::PropertyKeyLocals;

pub(super) struct LocalCompactForInCursor {
    source: GcLocal<OrdinaryObject, Nullable>,
    storage: GcLocal<CompactDataStorage, Nullable>,
    keys: GcLocal<CompactShapeKeys, Nullable>,
    index: I32Local,
    length: I32Local,
}

impl LocalCompactForInCursor {
    pub(super) fn clear(self, s: &RuntimeSchema, f: &mut Function) {
        s.release_i32_local(self.length, f);
        s.release_i32_local(self.index, f);
        self.keys.clear(f);
        self.storage.clear(f);
        self.source.clear(f);
    }
}

impl FunctionBuilder<'_> {
    /// Eager cursors can borrow shape keys before the first advance. Between
    /// creation and that advance the loop emits only its private bookkeeping.
    pub(super) fn emit_prepare_local_compact_for_in(
        &mut self,
        current: &ValueLocals,
        f: &mut Function,
    ) -> Result<Option<LocalCompactForInCursor>, EmitError> {
        // Resume bodies publish the record before their first advance.
        if self
            .body_entry_locals()
            .and_then(|entry| entry.resume_frame())
            .is_some()
            || !object_prototype_has_no_enumerable_keys()?
        {
            return Ok(None);
        }
        let s = self.runtime_schema();
        let cursor = LocalCompactForInCursor {
            source: s.reserve_gc_local(f).initialize_null(s, f),
            storage: s.reserve_gc_local(f).initialize_null(s, f),
            keys: s.reserve_gc_local(f).initialize_null(s, f),
            index: s.reserve_i32_local(f),
            length: s.reserve_i32_local(f),
        };
        cursor.index.set_constant(0, f);
        cursor.length.set_constant(0, f);
        let declined = self.open_frame(ControlFrameKind::Block, f);
        let source = s.reserve_gc_local(f).initialize(
            current.sealed_reference_or_branch::<OrdinaryObject>(
                s,
                self.local_branch_label(declined),
                f,
            ),
            f,
        );
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&source, s, f)
                .reference(),
            f,
        );
        let storage = self.emit_for_in_compact_storage_or_branch(&base, declined, f);
        base.clear(f);
        self.emit_for_in_local_tail_or_branch(&source, declined, f);
        let keys = self.emit_compact_shape_keys(&storage, f);
        s.array_type::<CompactShapeKeys>().length(&keys, s, f);
        cursor.length.store(f);
        cursor.keys.replace(keys.load(s, f).nullable(), f);
        cursor.storage.replace(storage.load(s, f).nullable(), f);
        cursor.source.replace(source.load(s, f).nullable(), f);
        keys.clear(f);
        storage.clear(f);
        source.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Ok(Some(cursor))
    }

    pub(super) fn emit_advance_local_compact_for_in(
        &mut self,
        record: &GcLocal<ForInEnumerationRecord>,
        cursor: &LocalCompactForInCursor,
        output: &CompletionLocals,
        complete: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let inactive = self.open_frame(ControlFrameKind::Block, f);
        let storage = s.reserve_non_null_gc_local(f).initialize(
            cursor
                .storage
                .non_null_or_branch(self.local_branch_label(inactive), f),
            f,
        );
        let fallback = self.open_frame(ControlFrameKind::Block, f);
        self.rarely_taken(fallback, f);
        let source = s
            .reserve_gc_local(f)
            .initialize(cursor.source.load(s, f).require_non_null(f), f);
        s.field(OrdinaryObjectSchema::PROPERTIES)
            .read(&source, s, f);
        storage.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        // Shape is immutable in a compact storage. Replacement, including a
        // compact append, needs the original shape's unvisited key snapshot.
        self.emit_branch_if_to_target(fallback, f);
        cursor.index.load(f);
        cursor.length.load(f);
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        let keys = s
            .reserve_non_null_gc_local(f)
            .initialize(cursor.keys.load(s, f).require_non_null(f), f);
        self.emit_for_in_compact_key(&keys, cursor.index, output, f);
        keys.clear(f);
        self.emit_branch_to_target(complete, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        // Until own keys are exhausted, prototype mutations do not affect
        // which of those keys to yield. The next observation is the live tail.
        self.emit_for_in_local_tail_or_branch(&source, fallback, f);
        output.initialize(f);
        self.emit_branch_to_target(complete, f);
        source.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        // R sees precisely the original key snapshot and the processed prefix.
        // It either continues that snapshot or seeds the exact visited Set.
        s.field(ForInEnumerationRecordSchema::COMPACT_STORAGE)
            .write(record, GcOperand::nullable_reference(&storage, s), s, f);
        s.field(ForInEnumerationRecordSchema::NEXT_KEY).write(
            record,
            GcOperand::i32_local(cursor.index),
            s,
            f,
        );
        cursor.storage.set_null(s, f);
        cursor.source.set_null(s, f);
        cursor.keys.set_null(s, f);
        storage.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
    }

    fn emit_for_in_local_tail_or_branch(
        &mut self,
        source: &GcLocal<OrdinaryObject>,
        declined: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let prototype = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(source, s, f)
                .reference(),
            f,
        );
        let closed = self.open_frame(ControlFrameKind::Block, f);
        prototype.load(s, f).is_null(f);
        self.emit_branch_if_to_target(closed, f);
        self.emit_unchanged_object_prototype_or_branch(&prototype, declined, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        prototype.clear(f);
    }

    fn emit_for_in_compact_storage_or_branch(
        &mut self,
        base: &GcLocal<ObjectPropertyStorage>,
        declined: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<CompactDataStorage> {
        let s = self.runtime_schema();
        let storage = s.reserve_non_null_gc_local(f).initialize(
            s.emit_compact_storage_or_branch(base, self.local_branch_label(declined), f),
            f,
        );
        let shape = s.reserve_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&storage, s, f)
                .reference(),
            f,
        );
        s.field(CompactDataShapeSchema::OWN_KEYS_ORDERED)
            .read(&shape, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&shape, s, f);
        let attributes = crate::heap::StoredPropertyAttributes::Data {
            writable: true,
            enumerable: true,
            configurable: true,
        }
        .descriptor_word();
        f.instruction(&Instruction::I32Const(attributes.as_i32()));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(declined, f);
        shape.clear(f);
        storage
    }

    fn emit_for_in_compact_key(
        &self,
        keys: &GcLocal<CompactShapeKeys>,
        index: I32Local,
        output: &CompletionLocals,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let name = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeKeys>()
                .read(keys, index, s, f)
                .reference(),
            f,
        );
        // Consumers confirm the ordinal against the exact String reference
        // in live storage. Other enumerations may overwrite the hint safely.
        s.field(StringValueSchema::ENTRY_HINT)
            .write(&name, GcOperand::i32_local(index), s, f);
        output.value().set_reference(&name, s, f);
        output.set_kind(CompletionKind::Normal, f);
        output.target().set_constant(0, f);
        name.clear(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
    }

    pub(super) fn emit_for_in_visited_keys(
        &mut self,
        record: &GcLocal<ForInEnumerationRecord>,
        f: &mut Function,
    ) -> Result<ForInVisitedKeys, EmitError> {
        let s = self.runtime_schema();
        let nullable = s.reserve_gc_local::<SetObject, Nullable>(f).initialize(
            s.field(ForInEnumerationRecordSchema::VISITED_SET)
                .read(record, s, f)
                .reference(),
            f,
        );
        nullable.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let created = ForInVisitedKeys::new(self, f)?;
        s.field(ForInEnumerationRecordSchema::VISITED_SET).write(
            record,
            GcOperand::nullable_reference(&created.set, s),
            s,
            f,
        );
        nullable.replace(created.set.load(s, f).nullable(), f);
        created.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let set = s
            .reserve_gc_local(f)
            .initialize(nullable.load(s, f).require_non_null(f), f);
        nullable.clear(f);
        Ok(ForInVisitedKeys { set })
    }

    /// Valid only after all own keys have been processed. Revalidate the live
    /// prototype: an earlier body may have replaced it or edited its properties.
    pub(super) fn emit_for_in_finish_closed_tail(
        &mut self,
        record: &GcLocal<ForInEnumerationRecord>,
        current: &ValueLocals,
        object: &GcLocal<OrdinaryObject>,
        complete: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        if !object_prototype_has_no_enumerable_keys()? {
            return Ok(());
        }
        let s = self.runtime_schema();
        // The exact immutable Object.prototype rows are all nonenumerable.
        // The shared guard also requires a null ancestor. Structural edits
        // replace intrinsic storage, so the old layout can never authorize one.
        let mode = self.emit_negative_cache_mode(current, object, None, f)?;
        mode.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let null = s.reserve_value_local(f);
        null.set_scalar(ScalarValue::Null, f);
        let done = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(&null, f), f);
        s.field(ForInEnumerationRecordSchema::CURRENT).write(
            record,
            GcOperand::reference(&done, s),
            s,
            f,
        );
        null.clear(f);
        done.clear(f);
        self.emit_branch_to_target(complete, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(mode, f);
        Ok(())
    }

    /// A cursor either stays in this phase or permanently enters the existing
    /// generic phase. No value/descriptor/prototype observation runs ahead of a
    /// selected body, including bodies that suspend or trigger collection.
    pub(super) fn emit_advance_compact_for_in(
        &mut self,
        record: &GcLocal<ForInEnumerationRecord>,
        output: &CompletionLocals,
        complete: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let declined = self.open_frame(ControlFrameKind::Block, f);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(ForInEnumerationRecordSchema::CURRENT)
                .read(record, s, f)
                .reference(),
            f,
        );
        let current = s.reserve_value_local(f);
        s.field(StoredValueSchema::REFERENCE)
            .read(&stored, s, f)
            .store_eq(current.reference(), f);
        let object = s.reserve_gc_local(f).initialize(
            current.sealed_reference_or_branch::<OrdinaryObject>(
                s,
                self.local_branch_label(declined),
                f,
            ),
            f,
        );
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        let captured = s
            .reserve_gc_local::<CompactDataStorage, Nullable>(f)
            .initialize(
                s.field(ForInEnumerationRecordSchema::COMPACT_STORAGE)
                    .read(record, s, f)
                    .reference(),
                f,
            );
        captured.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(ForInEnumerationRecordSchema::VISITED_SET)
            .read(record, s, f)
            .reference()
            .is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        // A published generic snapshot fixes this cursor's initial strategy,
        // even while its small first phase has not needed a visited Set yet.
        s.field(ForInEnumerationRecordSchema::REMAINING_KEYS)
            .read(record, s, f)
            .reference()
            .is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        let candidate = self.emit_for_in_compact_storage_or_branch(&base, declined, f);
        s.field(ForInEnumerationRecordSchema::COMPACT_STORAGE)
            .write(record, GcOperand::nullable_reference(&candidate, s), s, f);
        captured.replace(candidate.load(s, f).nullable(), f);
        candidate.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let storage = s
            .reserve_gc_local(f)
            .initialize(captured.load(s, f).require_non_null(f), f);
        let keys = self.emit_compact_shape_keys(&storage, f);
        let length = s.reserve_i32_local(f);
        s.array_type::<CompactShapeKeys>().length(&keys, s, f);
        length.store(f);
        let index = s.reserve_i32_local(f);
        s.field(ForInEnumerationRecordSchema::NEXT_KEY)
            .read(record, s, f)
            .store(index, f);
        let fallback = self.open_frame(ControlFrameKind::Block, f);
        base.load(s, f);
        storage.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(fallback, f);
        index.load(f);
        length.load(f);
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_for_in_compact_key(&keys, index, output, f);
        s.field(ForInEnumerationRecordSchema::NEXT_KEY).write(
            record,
            GcOperand::i32_local(index),
            s,
            f,
        );
        self.emit_branch_to_target(complete, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // The tail check only uses the reference already admitted above.
        self.emit_for_in_finish_closed_tail(record, &current, &object, complete, f)?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        // Preserve the original OwnKeys snapshot when promotion occurred.
        // Every processed key existed/enumerated while identity held, so only
        // that prefix enters the canonical Set. Remaining keys use live
        // descriptor checks and ordinary lazy prototype traversal below.
        let visited = self.emit_for_in_visited_keys(record, f)?;
        let snapshot = PropertyKeyConstruction::allocate(s, s.reserve_gc_local(f), length, f);
        let cursor = s.reserve_i32_local(f);
        cursor.set_constant(0, f);
        let seeded = self.open_frame(ControlFrameKind::Block, f);
        let seed = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(f);
        length.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(seeded, f);
        let name = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeKeys>()
                .read(&keys, cursor, s, f)
                .reference(),
            f,
        );
        let key = PropertyKeyLocals::from_string(s, &name, f);
        snapshot.write(cursor, &key, s, f);
        cursor.load(f);
        index.load(f);
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        visited.insert(&name, self, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        key.clear(f);
        name.clear(f);
        cursor.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        cursor.store(f);
        self.emit_branch_to_target(seed, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let table = s.reserve_gc_local(f).initialize(snapshot.publish(s, f), f);
        s.field(ForInEnumerationRecordSchema::REMAINING_KEYS).write(
            record,
            GcOperand::nullable_reference(&table, s),
            s,
            f,
        );
        s.field(ForInEnumerationRecordSchema::COMPACT_STORAGE)
            .write(record, GcOperand::null(s), s, f);
        table.clear(f);
        s.release_i32_local(cursor, f);
        visited.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(index, f);
        s.release_i32_local(length, f);
        keys.clear(f);
        storage.clear(f);
        captured.clear(f);
        base.clear(f);
        object.clear(f);
        current.clear(f);
        stored.clear(f);
        Ok(())
    }
}

fn object_prototype_has_no_enumerable_keys() -> Result<bool, EmitError> {
    let layout = crate::intrinsics::IntrinsicPropertyLayoutId::ObjectPrototype.layout()?;
    Ok(layout
        .properties()
        .iter()
        .all(|property| property.flags.bits() & crate::heap::DescriptorBit::Enumerable.word() == 0))
}
