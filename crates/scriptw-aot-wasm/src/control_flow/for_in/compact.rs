//! A compact own-key phase borrows immutable shape keys. Structural mutation
//! promotes storage, so identity is the complete live-descriptor guard.
use super::enumeration::ForInVisitedKeys;
use super::*;
use crate::operations::PropertyKeyLocals;

impl FunctionBuilder<'_> {
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
        let layout = crate::intrinsics::IntrinsicPropertyLayoutId::ObjectPrototype.layout()?;
        if layout.properties().iter().any(|property| {
            property.flags.bits() & crate::heap::DescriptorBit::Enumerable.word() != 0
        }) {
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
        let stored = s.reserve_gc_local(f).initialize(
            s.field(ForInEnumerationRecordSchema::CURRENT)
                .read(record, s, f)
                .reference(),
            f,
        );
        let current = s.reserve_value_local(f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &current, s, f);
        current.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Null.tag()));
        f.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(complete, f);
        current.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<OrdinaryObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        let object = s
            .reserve_gc_local(f)
            .initialize(current.cast_reference::<OrdinaryObject>(s, f), f);
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
        base.is_subtype::<CompactDataStorage>(s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        let candidate = s
            .reserve_gc_local(f)
            .initialize(base.load(s, f).cast_subtype::<CompactDataStorage>(s, f), f);
        let shape = s.reserve_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&candidate, s, f)
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
        s.field(ForInEnumerationRecordSchema::COMPACT_STORAGE)
            .write(record, GcOperand::nullable_reference(&candidate, s), s, f);
        captured.replace(candidate.load(s, f).nullable(), f);
        shape.clear(f);
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
        let name = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeKeys>()
                .read(&keys, index, s, f)
                .reference(),
            f,
        );
        let value = s.reserve_value_local(f);
        value.set_reference(&name, s, f);
        output.set_normal(&value, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(f);
        s.field(ForInEnumerationRecordSchema::NEXT_KEY).write(
            record,
            GcOperand::i32_local(index),
            s,
            f,
        );
        value.clear(f);
        name.clear(f);
        self.emit_branch_to_target(complete, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
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
