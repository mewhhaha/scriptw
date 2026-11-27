//! Open-addressed WeakMap storage. Only the registered reference array contains
//! ephemeron edges. Scalar backing never has to be traced or updated by the GC.
use super::*;

const INITIAL_CAPACITY: i32 = 16;

struct Table {
    record: GcLocal<WeakMapTable>,
    hashes: GcLocal<NumberBitsI64Array>,
    references: GcLocal<CompactReferenceArray>,
    capacity: I32Local,
}

#[derive(Clone, Copy)]
enum Probe<'a> {
    Lookup { key: &'a EqRefLocal, present: I32Local },
    Insert { key: &'a EqRefLocal, present: I32Local },
    Rehash,
}

#[derive(Clone, Copy)]
enum PairMember {
    Key,
    Value,
}

fn reference_index(
    slot: I32Local,
    member: PairMember,
    s: &RuntimeSchema,
    f: &mut Function,
) -> I32Local {
    let index = s.reserve_i32_local(f);
    slot.load(f);
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Shl);
    if matches!(member, PairMember::Value) {
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Or);
    }
    index.store(f);
    index
}

fn load_table(
    map: &GcLocal<WeakMapObject>,
    s: &RuntimeSchema,
    f: &mut Function,
) -> GcLocal<WeakMapTable, Nullable> {
    s.reserve_gc_local(f).initialize(
        s.field(WeakMapObjectSchema::TABLE).read(map, s, f).reference(),
        f,
    )
}

impl Table {
    fn hold(table: GcStackReference<WeakMapTable>, s: &RuntimeSchema, f: &mut Function) -> Self {
        let record = s.reserve_gc_local(f).initialize(table, f);
        let hashes = s.reserve_gc_local(f).initialize(
            s.field(WeakMapTableSchema::HASHES).read(&record, s, f).reference(),
            f,
        );
        let references = s.reserve_gc_local(f).initialize(
            s.field(WeakMapTableSchema::REFERENCES).read(&record, s, f).reference(),
            f,
        );
        let capacity = s.reserve_i32_local(f);
        s.array_type::<NumberBitsI64Array>().length(&hashes, s, f);
        capacity.store(f);
        Self { record, hashes, references, capacity }
    }

    fn allocate(
        capacity: I32Local,
        register: DeclaredGcHostImport,
        s: &RuntimeSchema,
        f: &mut Function,
    ) -> Self {
        let length = s.reserve_i32_local(f);
        capacity.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        length.store(f);
        let hashes = s.reserve_gc_local(f).initialize(
            s.array_type::<NumberBitsI64Array>().filled(GcOperand::i64(0), capacity, f),
            f,
        );
        let tags = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactTagArray>().filled(GcOperand::i32(0), capacity, f),
            f,
        );
        let scalars = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactScalarArray>().filled(GcOperand::i64(0), capacity, f),
            f,
        );
        let references = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactReferenceArray>().filled(GcOperand::null_eq(), length, f),
            f,
        );
        let record = s.reserve_gc_local(f).initialize(
            s.struct_type::<WeakMapTable>().construct(
                (
                    GcOperand::reference(&hashes, s),
                    GcOperand::reference(&tags, s),
                    GcOperand::reference(&scalars, s),
                    GcOperand::reference(&references, s),
                    GcOperand::i32(0),
                ),
                f,
            ),
            f,
        );
        scalars.clear(f);
        tags.clear(f);
        s.release_i32_local(length, f);
        register.register_weak_map_table(&references, s, f);
        Self { record, hashes, references, capacity }
    }

    fn publish(&self, map: &GcLocal<WeakMapObject>, s: &RuntimeSchema, f: &mut Function) {
        s.field(WeakMapObjectSchema::TABLE).write(
            map,
            GcOperand::nullable_reference(&self.record, s),
            s,
            f,
        );
    }

    fn replace(&self, new: &Self, s: &RuntimeSchema, f: &mut Function) {
        self.record.replace(new.record.load(s, f), f);
        self.hashes.replace(new.hashes.load(s, f), f);
        self.references.replace(new.references.load(s, f), f);
        new.capacity.load(f);
        self.capacity.store(f);
    }

    fn clear(self, s: &RuntimeSchema, f: &mut Function) {
        self.references.clear(f);
        self.hashes.clear(f);
        self.record.clear(f);
        s.release_i32_local(self.capacity, f);
    }

    fn emit_key_reference(&self, slot: I32Local, s: &RuntimeSchema, f: &mut Function) {
        let index = reference_index(slot, PairMember::Key, s, f);
        let _ = s.array_type::<CompactReferenceArray>().read(&self.references, index, s, f);
        s.release_i32_local(index, f);
    }

    fn emit_hash(&self, slot: I32Local, s: &RuntimeSchema, f: &mut Function) {
        let _ = s.array_type::<NumberBitsI64Array>().read(&self.hashes, slot, s, f);
    }

    // USED never exceeds three quarters of a table. The probe always reaches
    // a virgin slot, even when every earlier slot is a tombstone.
    fn probe(
        &self,
        hash: I64Local,
        mode: Probe<'_>,
        slot: I32Local,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        let mask = s.reserve_i32_local(f);
        self.capacity.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        mask.store(f);
        hash.load(f);
        f.instruction(&Instruction::I64Const(0x9e37_79b9_7f4a_7c15_u64 as i64));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Const(32));
        f.instruction(&Instruction::I64ShrU);
        f.instruction(&Instruction::I32WrapI64);
        mask.load(f);
        f.instruction(&Instruction::I32And);
        slot.store(f);

        let deleted = if matches!(mode, Probe::Insert { .. }) {
            let first = s.reserve_i32_local(f);
            first.set_constant(-1, f);
            Some(first)
        } else {
            None
        };
        let candidate = s.reserve_i64_local(f);
        f.instruction(&Instruction::Block(BlockType::Empty));
        let done = f.label_depth();
        f.instruction(&Instruction::Block(BlockType::Empty));
        let virgin = f.label_depth();
        f.instruction(&Instruction::Loop(BlockType::Empty));
        let again = f.label_depth();
        self.emit_hash(slot, s, f);
        candidate.store(f);
        candidate.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.branch_if_to_label(virgin);

        if let Probe::Lookup { key, present } | Probe::Insert { key, present } = mode {
            candidate.load(f);
            hash.load(f);
            f.instruction(&Instruction::I64Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            self.emit_key_reference(slot, s, f);
            key.load(f);
            f.instruction(&Instruction::RefEq);
            f.instruction(&Instruction::If(BlockType::Empty));
            present.set_constant(1, f);
            f.branch_to_label(done);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::End);
        }
        if let Some(first) = deleted {
            first.load(f);
            f.instruction(&Instruction::I32Const(-1));
            f.instruction(&Instruction::I32Eq);
            f.instruction(&Instruction::If(BlockType::Empty));
            self.emit_key_reference(slot, s, f);
            f.instruction(&Instruction::RefIsNull);
            f.instruction(&Instruction::If(BlockType::Empty));
            slot.load(f);
            first.store(f);
            f.instruction(&Instruction::End);
            f.instruction(&Instruction::End);
        }
        increment(slot, f);
        slot.load(f);
        mask.load(f);
        f.instruction(&Instruction::I32And);
        slot.store(f);
        f.branch_to_label(again);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);

        if let Some(first) = deleted {
            first.load(f);
            f.instruction(&Instruction::I32Const(-1));
            f.instruction(&Instruction::I32Ne);
            f.instruction(&Instruction::If(BlockType::Empty));
            first.load(f);
            slot.store(f);
            f.instruction(&Instruction::End);
            s.release_i32_local(first, f);
        }
        f.instruction(&Instruction::End);
        s.release_i64_local(candidate, f);
        s.release_i32_local(mask, f);
    }

    fn read_value_scalars(
        &self,
        slot: I32Local,
        value: &ValueLocals,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        let tags = s.field(WeakMapTableSchema::TAGS).read(&self.record, s, f).reference();
        s.array_type::<CompactTagArray>().read_top(tags, slot, f).store(value.tag(), f);
        let scalars = s.field(WeakMapTableSchema::SCALARS).read(&self.record, s, f).reference();
        s.array_type::<CompactScalarArray>()
            .read_top(scalars, slot, f)
            .store_i64(value.scalar(), f);
    }

    fn read_value(&self, slot: I32Local, value: &ValueLocals, s: &RuntimeSchema, f: &mut Function) {
        self.read_value_scalars(slot, value, s, f);
        let index = reference_index(slot, PairMember::Value, s, f);
        s.array_type::<CompactReferenceArray>()
            .read(&self.references, index, s, f)
            .store_eq(value.reference(), f);
        s.release_i32_local(index, f);
    }

    fn write_value_scalars(
        &self,
        slot: I32Local,
        value: &ValueLocals,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        let tags = s.field(WeakMapTableSchema::TAGS).read(&self.record, s, f).reference();
        s.array_type::<CompactTagArray>().write_top(tags, slot, GcOperand::i32_local(value.tag()), f);
        let scalars = s.field(WeakMapTableSchema::SCALARS).read(&self.record, s, f).reference();
        s.array_type::<CompactScalarArray>().write_top(
            scalars, slot, GcOperand::i64_local(value.scalar()), f,
        );
    }

    fn write_value(&self, slot: I32Local, value: &ValueLocals, s: &RuntimeSchema, f: &mut Function) {
        self.write_value_scalars(slot, value, s, f);
        let index = reference_index(slot, PairMember::Value, s, f);
        s.array_type::<CompactReferenceArray>().write(
            &self.references, index, GcOperand::eq_reference(value.reference()), s, f,
        );
        s.release_i32_local(index, f);
    }

    fn write_key(
        &self,
        slot: I32Local,
        hash: I64Local,
        key: &EqRefLocal,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        s.array_type::<NumberBitsI64Array>().write(
            &self.hashes, slot, GcOperand::i64_local(hash), s, f,
        );
        let index = reference_index(slot, PairMember::Key, s, f);
        s.array_type::<CompactReferenceArray>().write(
            &self.references, index, GcOperand::eq_reference(key), s, f,
        );
        s.release_i32_local(index, f);
    }

    fn delete(&self, slot: I32Local, s: &RuntimeSchema, f: &mut Function) {
        for member in [PairMember::Key, PairMember::Value] {
            let index = reference_index(slot, member, s, f);
            s.array_type::<CompactReferenceArray>().write(
                &self.references, index, GcOperand::null_eq(), s, f,
            );
            s.release_i32_local(index, f);
        }
    }

    fn used(&self, used: I32Local, s: &RuntimeSchema, f: &mut Function) {
        s.field(WeakMapTableSchema::USED).read(&self.record, s, f).store(used, f);
    }

    fn set_used(&self, used: I32Local, s: &RuntimeSchema, f: &mut Function) {
        s.field(WeakMapTableSchema::USED).write(&self.record, GcOperand::i32_local(used), s, f);
    }

    fn visit_live(
        &self,
        s: &RuntimeSchema,
        f: &mut Function,
        mut visit: impl FnMut(I32Local, &mut Function),
    ) {
        let slot = s.reserve_i32_local(f);
        slot.set_constant(0, f);
        f.instruction(&Instruction::Loop(BlockType::Empty));
        let again = f.label_depth();
        self.emit_key_reference(slot, s, f);
        f.instruction(&Instruction::RefIsNull);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        visit(slot, f);
        f.instruction(&Instruction::End);
        increment(slot, f);
        slot.load(f);
        self.capacity.load(f);
        f.instruction(&Instruction::I32LtU);
        f.branch_if_to_label(again);
        f.instruction(&Instruction::End);
        s.release_i32_local(slot, f);
    }

    fn rebuild(
        &self,
        map: &GcLocal<WeakMapObject>,
        register: DeclaredGcHostImport,
        s: &RuntimeSchema,
        f: &mut Function,
    ) {
        let used = s.reserve_i32_local(f);
        used.set_constant(0, f);
        self.visit_live(s, f, |_, f| increment(used, f));

        // Reclaim tombstones at the same capacity when they make up enough
        // of the used slots. Otherwise grow to keep the probe load bounded.
        let capacity = s.reserve_i32_local(f);
        self.capacity.load(f);
        capacity.store(f);
        used.load(f);
        self.capacity.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.capacity.load(f);
        // Each bucket needs two reference elements. Never let their length
        // wrap to zero, even if the Store permits unusually large arrays.
        f.instruction(&Instruction::I32Const(1 << 30));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
        self.capacity.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        capacity.store(f);
        f.instruction(&Instruction::End);
        let replacement = Self::allocate(capacity, register, s, f);

        // Any allocation above may have collected keys. Re-read the original
        // slots after allocation, and count exactly the pairs copied.
        used.set_constant(0, f);
        let destination = s.reserve_i32_local(f);
        let hash = s.reserve_i64_local(f);
        let value = s.reserve_value_local(f);
        let pair_length = s.reserve_i32_local(f);
        pair_length.set_constant(2, f);
        self.visit_live(s, f, |source, f| {
            self.emit_hash(source, s, f);
            hash.store(f);
            replacement.probe(hash, Probe::Rehash, destination, s, f);
            s.array_type::<NumberBitsI64Array>().write(
                &replacement.hashes, destination, GcOperand::i64_local(hash), s, f,
            );
            self.read_value_scalars(source, &value, s, f);
            replacement.write_value_scalars(destination, &value, s, f);
            let from = reference_index(source, PairMember::Key, s, f);
            let to = reference_index(destination, PairMember::Key, s, f);
            s.array_type::<CompactReferenceArray>().copy_range_from(
                &replacement.references, to, &self.references, from, pair_length, s, f,
            );
            s.release_i32_local(to, f);
            s.release_i32_local(from, f);
            increment(used, f);
        });
        s.release_i32_local(pair_length, f);
        value.clear(f);
        s.release_i64_local(hash, f);
        s.release_i32_local(destination, f);
        replacement.set_used(used, s, f);
        s.release_i32_local(used, f);
        replacement.publish(map, s, f);

        // This array may already be old. Remove the obsolete edges now, so
        // the next minor collection cannot treat them as old-table edges.
        self.visit_live(s, f, |slot, f| self.delete(slot, s, f));
        self.replace(&replacement, s, f);
        replacement.clear(s, f);
    }
}

fn increment(local: I32Local, f: &mut Function) {
    local.load(f);
    f.instruction(&Instruction::I32Const(1));
    f.instruction(&Instruction::I32Add);
    local.store(f);
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_weak_map_lookup(
        &self,
        op: MapOperation,
        map: &GcLocal<WeakMapObject>,
        key: &ValueLocals,
        hash: I64Local,
        value: &ValueLocals,
        present: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        f.instruction(&Instruction::Block(BlockType::Empty));
        let miss = f.label_depth();
        hash.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.branch_if_to_label(miss);
        let nullable = load_table(map, s, f);
        let table = Table::hold(nullable.non_null_or_branch(miss, f), s, f);
        nullable.clear(f);
        let slot = s.reserve_i32_local(f);
        table.probe(hash, Probe::Lookup { key: key.reference(), present }, slot, s, f);
        match op {
            MapOperation::Has => {}
            MapOperation::Delete
            | MapOperation::Get
            | MapOperation::GetOrInsert
            | MapOperation::GetOrInsertComputed => {
                present.load(f);
                f.instruction(&Instruction::If(BlockType::Empty));
                if matches!(op, MapOperation::Delete) {
                    table.delete(slot, s, f);
                } else {
                    table.read_value(slot, value, s, f);
                }
                f.instruction(&Instruction::End);
            }
            MapOperation::Set => unreachable!(),
        }
        s.release_i32_local(slot, f);
        table.clear(s, f);
        f.instruction(&Instruction::End);
    }

    pub(super) fn emit_weak_map_put(
        &self,
        map: &GcLocal<WeakMapObject>,
        key: &ValueLocals,
        hash: I64Local,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let register = self.weak_import(GcHostImport::WeakMapRegisterTable)?;
        let nullable = load_table(map, s, f);
        nullable.load(s, f).is_null(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        let capacity = s.reserve_i32_local(f);
        capacity.set_constant(INITIAL_CAPACITY, f);
        let initial = Table::allocate(capacity, register, s, f);
        initial.publish(map, s, f);
        nullable.replace(initial.record.load(s, f).nullable(), f);
        initial.clear(s, f);
        f.instruction(&Instruction::End);
        let table = Table::hold(nullable.load(s, f).require_non_null(f), s, f);
        nullable.clear(f);

        let slot = s.reserve_i32_local(f);
        let present = s.reserve_i32_local(f);
        present.set_constant(0, f);
        table.probe(hash, Probe::Insert { key: key.reference(), present }, slot, s, f);
        present.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        table.emit_hash(slot, s, f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        let used = s.reserve_i32_local(f);
        table.used(used, s, f);
        used.load(f);
        table.capacity.load(f);
        table.capacity.load(f);
        f.instruction(&Instruction::I32Const(2));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::If(BlockType::Empty));
        table.rebuild(map, register, s, f);
        table.probe(hash, Probe::Rehash, slot, s, f);
        table.used(used, s, f);
        f.instruction(&Instruction::End);
        increment(used, f);
        table.set_used(used, s, f);
        s.release_i32_local(used, f);
        f.instruction(&Instruction::End);
        table.write_key(slot, hash, key.reference(), s, f);
        f.instruction(&Instruction::End);
        table.write_value(slot, value, s, f);
        s.release_i32_local(present, f);
        s.release_i32_local(slot, f);
        table.clear(s, f);
        Ok(())
    }
}
