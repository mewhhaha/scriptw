//! Dense and sparse arrays share complete indexed property descriptors.
//!
//! The closed storage mode selects this backing. Its capacity follows occupied indices,
//! never observable length. A distant insertion moves once to sparse storage;
//! the old dense graph stays rooted until every replacement node is complete.
use super::*;
use crate::gc_types::{
    ArrayIndexBucketTable, ArrayIndexEntry, ArrayIndexEntrySchema, ArrayIndexKeyConstruction,
    ArrayIndexedStorageMode, ArrayObject, ArrayObjectSchema, IndexedTable,
};

const DENSE_INDEX_LIMIT: i64 = 1 << 20;

impl FunctionBuilder<'_> {
    fn emit_array_dense_elements(
        &self,
        storage: &GcLocal<ArrayObject>,
        f: &mut Function,
    ) -> GcLocal<IndexedTable> {
        let schema = self.runtime_schema();
        schema.reserve_gc_local(f).initialize(
            schema
                .field(ArrayObjectSchema::DENSE)
                .read(storage, schema, f)
                .reference(),
            f,
        )
    }

    fn emit_array_dense_mode(&self, storage: &GcLocal<ArrayObject>, f: &mut Function) -> I32Local {
        let schema = self.runtime_schema();
        let dense = schema.reserve_i32_local(f);
        schema.array_storage_mode_is(storage, ArrayIndexedStorageMode::DenseDescriptors, f);
        dense.store(f);
        dense
    }

    /// The caller has selected dense mode. Out-of-capacity reads are holes.
    pub(super) fn emit_dense_array_descriptor(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        output: &GcLocal<PropertyDescriptor, Nullable>,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let dense = self.emit_array_dense_elements(storage, f);
        let slot = schema.reserve_i32_local(f);
        index.load(f);
        schema
            .array_type::<IndexedTable>()
            .length(&dense, schema, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        output.replace(
            schema
                .array_type::<IndexedTable>()
                .read(&dense, slot, schema, f)
                .reference(),
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(slot, f);
        dense.clear(f);
    }

    /// Return true when dense publication completed. A distant index first
    /// converts existing descriptors, then returns false to the sparse owner.
    pub(super) fn emit_dense_array_publish(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        descriptor: &GcLocal<PropertyDescriptor>,
        f: &mut Function,
    ) -> I32Local {
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(f);
        handled.set_constant(0, f);
        let mode = self.emit_array_dense_mode(storage, f);
        mode.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let dense = self.emit_array_dense_elements(storage, f);
        let capacity = schema.reserve_i32_local(f);
        schema
            .array_type::<IndexedTable>()
            .length(&dense, schema, f);
        capacity.store(f);
        let count = schema.reserve_i64_local(f);
        schema
            .field(ArrayObjectSchema::COUNT)
            .read(storage, schema, f)
            .store_i64(count, f);
        // Existing capacity can be reused after deletion. New capacity stays
        // close to occupancy, with a fixed absolute ceiling for dense storage.
        index.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        index.load(f);
        f.instruction(&Instruction::I64Const(DENSE_INDEX_LIMIT));
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
        let replacement_capacity = schema.reserve_i32_local(f);
        capacity.load(f);
        replacement_capacity.store(f);
        replacement_capacity.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        replacement_capacity.set_constant(8, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let ready = self.open_frame(ControlFrameKind::Block, f);
        let grow = self.open_frame(ControlFrameKind::Loop, f);
        index.load(f);
        replacement_capacity.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        self.emit_branch_if_to_target(ready, f);
        replacement_capacity.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        replacement_capacity.store(f);
        self.emit_branch_to_target(grow, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let replacement = schema.reserve_gc_local(f).initialize(
            schema.array_type::<IndexedTable>().filled(
                GcOperand::null(schema),
                replacement_capacity,
                f,
            ),
            f,
        );
        schema.array_type::<IndexedTable>().copy_prefix_from(
            &replacement,
            &dense,
            capacity,
            schema,
            f,
        );
        schema.field(ArrayObjectSchema::DENSE).write(
            storage,
            GcOperand::reference(&replacement, schema),
            schema,
            f,
        );
        dense.replace(replacement.load(schema, f), f);
        replacement.clear(f);
        schema.release_i32_local(replacement_capacity, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let slot = schema.reserve_i32_local(f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        schema
            .array_type::<IndexedTable>()
            .read(&dense, slot, schema, f)
            .reference()
            .is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        count.store(f);
        schema.field(ArrayObjectSchema::COUNT).write(
            storage,
            GcOperand::i64_local(count),
            schema,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.array_type::<IndexedTable>().write(
            &dense,
            slot,
            GcOperand::nullable_reference(descriptor, schema),
            schema,
            f,
        );
        schema.release_i32_local(slot, f);
        handled.set_constant(1, f);
        f.instruction(&Instruction::Else);
        self.emit_dense_array_to_sparse(storage, &dense, capacity, count, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i64_local(count, f);
        schema.release_i32_local(capacity, f);
        dense.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(mode, f);
        handled
    }

    fn emit_dense_array_to_sparse(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        dense: &GcLocal<IndexedTable>,
        dense_capacity: I32Local,
        count: I64Local,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let capacity = schema.reserve_i32_local(f);
        capacity.set_constant(8, f);
        let ready = self.open_frame(ControlFrameKind::Block, f);
        let grow = self.open_frame(ControlFrameKind::Loop, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(4));
        f.instruction(&Instruction::I64Mul);
        capacity.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Const(3));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64LeU);
        self.emit_branch_if_to_target(ready, f);
        capacity.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        capacity.store(f);
        self.emit_branch_to_target(grow, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let buckets = schema.reserve_gc_local(f).initialize(
            schema.array_type::<ArrayIndexBucketTable>().filled(
                GcOperand::null(schema),
                capacity,
                f,
            ),
            f,
        );
        let slot = schema.reserve_i32_local(f);
        slot.set_constant(0, f);
        let bucket = schema.reserve_i32_local(f);
        let index = schema.reserve_i64_local(f);
        let descriptor = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(f)
            .initialize_null(schema, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        slot.load(f);
        dense_capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        descriptor.replace(
            schema
                .array_type::<IndexedTable>()
                .read(dense, slot, schema, f)
                .reference(),
            f,
        );
        descriptor.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        slot.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        index.store(f);
        Self::emit_array_index_hash(index, capacity, bucket, f);
        let head = schema.reserve_gc_local(f).initialize(
            schema
                .array_type::<ArrayIndexBucketTable>()
                .read(&buckets, bucket, schema, f)
                .reference(),
            f,
        );
        let present = schema
            .reserve_gc_local(f)
            .initialize(descriptor.load(schema, f).require_non_null(f), f);
        let node = schema.reserve_gc_local(f).initialize(
            schema.struct_type::<ArrayIndexEntry>().construct(
                (
                    GcOperand::i64_local(index),
                    GcOperand::reference(&present, schema),
                    GcOperand::reference(&head, schema),
                ),
                f,
            ),
            f,
        );
        schema.array_type::<ArrayIndexBucketTable>().write(
            &buckets,
            bucket,
            GcOperand::nullable_reference(&node, schema),
            schema,
            f,
        );
        node.clear(f);
        present.clear(f);
        head.clear(f);
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
        // No allocation or JS callback separates these two publication writes.
        // The dense local keeps old descriptors live until the sparse graph owns them.
        let empty = schema
            .reserve_gc_local(f)
            .initialize(schema.empty_array_elements(f), f);
        schema.field(ArrayObjectSchema::DENSE).write(
            storage,
            GcOperand::reference(&empty, schema),
            schema,
            f,
        );
        schema.field(ArrayObjectSchema::BUCKETS).write(
            storage,
            GcOperand::reference(&buckets, schema),
            schema,
            f,
        );
        schema.set_array_storage_mode(storage, ArrayIndexedStorageMode::SparseDescriptors, f);
        empty.clear(f);
        descriptor.clear(f);
        schema.release_i64_local(index, f);
        schema.release_i32_local(bucket, f);
        schema.release_i32_local(slot, f);
        buckets.clear(f);
        schema.release_i32_local(capacity, f);
    }

    /// Dense deletion leaves a hole and adjusts occupancy only for a live slot.
    pub(super) fn emit_dense_array_delete(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        index: I64Local,
        f: &mut Function,
    ) -> I32Local {
        let schema = self.runtime_schema();
        let handled = self.emit_array_dense_mode(storage, f);
        handled.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let dense = self.emit_array_dense_elements(storage, f);
        let slot = schema.reserve_i32_local(f);
        index.load(f);
        schema
            .array_type::<IndexedTable>()
            .length(&dense, schema, f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        self.open_frame(ControlFrameKind::If, f);
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        slot.store(f);
        schema
            .array_type::<IndexedTable>()
            .read(&dense, slot, schema, f)
            .reference()
            .is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        schema
            .array_type::<IndexedTable>()
            .write(&dense, slot, GcOperand::null(schema), schema, f);
        let count = schema.reserve_i64_local(f);
        schema
            .field(ArrayObjectSchema::COUNT)
            .read(storage, schema, f)
            .store_i64(count, f);
        count.load(f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        count.store(f);
        schema.field(ArrayObjectSchema::COUNT).write(
            storage,
            GcOperand::i64_local(count),
            schema,
            f,
        );
        count.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, f);
        // No live descriptor remains. Retire the entire backing instead of
        // retaining its high-water capacity behind an empty JavaScript array.
        let empty = schema
            .reserve_gc_local(f)
            .initialize(schema.empty_array_elements(f), f);
        schema.field(ArrayObjectSchema::DENSE).write(
            storage,
            GcOperand::reference(&empty, schema),
            schema,
            f,
        );
        empty.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i64_local(count, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(slot, f);
        dense.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled
    }

    /// Capacity is bounded and every occupied slot is visited once, in order.
    pub(super) fn emit_dense_array_keys(
        &mut self,
        storage: &GcLocal<ArrayObject>,
        keys: &ArrayIndexKeyConstruction,
        output: I32Local,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let dense = self.emit_array_dense_elements(storage, f);
        let capacity = schema.reserve_i32_local(f);
        schema
            .array_type::<IndexedTable>()
            .length(&dense, schema, f);
        capacity.store(f);
        let slot = schema.reserve_i32_local(f);
        slot.set_constant(0, f);
        let index = schema.reserve_i64_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        slot.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        schema
            .array_type::<IndexedTable>()
            .read(&dense, slot, schema, f)
            .reference()
            .is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        slot.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        index.store(f);
        keys.write(output, index, schema, f);
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
        schema.release_i64_local(index, f);
        schema.release_i32_local(slot, f);
        schema.release_i32_local(capacity, f);
        dense.clear(f);
    }
}
