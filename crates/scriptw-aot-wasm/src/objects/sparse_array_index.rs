//! The shared indexed store dispatches between bounded dense descriptors and
//! sparse nodes and packed ordinary data. An explicit closed mode selects the
//! active backing. Logical Array length never sizes any backing.
use super::*;
use crate::gc_types::{
    ArrayIndexBucketTable, ArrayIndexEntry, ArrayIndexEntrySchema, ArrayIndexKeyConstruction,
    ArrayIndexKeyTable, ArrayIndexStorage, ArrayIndexStorageSchema, ArrayIndexedStorageMode,
    ArrayObject, ArrayObjectSchema, I64Local,
};
use crate::runtime_helpers::{
    ArrayIndexedDeleteArguments, ArrayIndexedDeleteParameters, ArrayIndexedPublishArguments,
    ArrayIndexedPublishParameters, HelperParameters,
};

impl FunctionBuilder<'_> {
    pub(super) fn emit_array_index_storage(
        &self,
        array: &GcLocal<ArrayObject>,
        f: &mut Function,
    ) -> GcLocal<ArrayIndexStorage> {
        let schema = self.runtime_schema();
        schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<ArrayObject>()
                .field(ArrayObjectSchema::ELEMENTS)
                .read(array, schema, f)
                .reference(),
            f,
        )
    }

    /// Missing occupied entries return null independently of Array.length.
    pub(crate) fn emit_array_indexed_descriptor(
        &mut self,
        array: &GcLocal<ArrayObject>,
        index: I64Local,
        f: &mut Function,
    ) -> GcLocal<PropertyDescriptor, Nullable> {
        let schema = self.runtime_schema();
        let storage = self.emit_array_index_storage(array, f);
        // A query for a missing index must not retire packed storage (Array
        // push and ordinary definition probe a missing index before publishing).
        schema.array_storage_is_packed(&storage, f);
        self.open_frame(ControlFrameKind::If, f);
        let found = schema.reserve_i32_local(f);
        found.set_constant(0, f);
        self.emit_packed_array_present(&storage, index, found, f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_materialize_packed_array(&storage, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(found, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let buckets = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<ArrayIndexStorage>()
                .field(ArrayIndexStorageSchema::BUCKETS)
                .read(&storage, schema, f)
                .reference(),
            f,
        );
        let capacity = schema.reserve_i32_local(f);
        schema
            .array_type::<ArrayIndexBucketTable>()
            .length(&buckets, schema, f);
        capacity.store(f);
        let output = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(f)
            .initialize_null(schema, f);
        schema.array_storage_mode_is(&storage, ArrayIndexedStorageMode::DenseDescriptors, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_dense_array_descriptor(&storage, index, &output, f);
        f.instruction(&Instruction::Else);
        capacity.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let bucket = schema.reserve_i32_local(f);
        Self::emit_array_index_hash(index, capacity, bucket, f);
        let cursor = schema.reserve_gc_local(f).initialize(
            schema
                .array_type::<ArrayIndexBucketTable>()
                .read(&buckets, bucket, schema, f)
                .reference(),
            f,
        );
        let key = schema.reserve_i64_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(schema, f).is_null(f);
        self.emit_branch_if_to_target(exit, f);
        schema
            .struct_type::<ArrayIndexEntry>()
            .field(ArrayIndexEntrySchema::INDEX)
            .read(&cursor, schema, f)
            .store_i64(key, f);
        key.load(f);
        index.load(f);
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        output.replace(
            schema
                .struct_type::<ArrayIndexEntry>()
                .field(ArrayIndexEntrySchema::DESCRIPTOR)
                .read(&cursor, schema, f)
                .reference()
                .nullable(),
            f,
        );
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.replace(
            schema
                .struct_type::<ArrayIndexEntry>()
                .field(ArrayIndexEntrySchema::NEXT)
                .read(&cursor, schema, f)
                .reference(),
            f,
        );
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema.release_i64_local(key, f);
        cursor.clear(f);
        schema.release_i32_local(bucket, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(capacity, f);
        buckets.clear(f);
        storage.clear(f);
        output
    }

    /// Publication never changes observable Array.length. Actual defining
    /// producers perform that operation after the completed descriptor commit.
    pub(crate) fn emit_array_indexed_publish_descriptor(
        &mut self,
        array: &GcLocal<ArrayObject>,
        index: I64Local,
        descriptor: &GcLocal<PropertyDescriptor>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let storage = self.emit_array_index_storage(array, f);
        schema.call_helper(
            ArrayIndexedPublishArguments::new(&storage, index, descriptor),
            self.runtime_helper_base()?,
            f,
        );
        storage.clear(f);
        Ok(())
    }
    pub(crate) fn emit_array_indexed_delete(
        &mut self,
        array: &GcLocal<ArrayObject>,
        index: I64Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let storage = self.emit_array_index_storage(array, f);
        schema.call_helper(
            ArrayIndexedDeleteArguments::new(&storage, index),
            self.runtime_helper_base()?,
            f,
        );
        storage.clear(f);
        Ok(())
    }

    /// A fixed integer mixing function. References and semantic addresses are
    /// absent; equal admitted index words always select the same bucket.
    pub(super) fn emit_array_index_hash(
        index: I64Local,
        capacity: I32Local,
        out: I32Local,
        f: &mut Function,
    ) {
        index.load(f);
        f.instruction(&Instruction::I32WrapI64);
        out.store(f);
        out.load(f);
        out.load(f);
        f.instruction(&Instruction::I32Const(16));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Xor);
        f.instruction(&Instruction::I32Const(0x7feb352d));
        f.instruction(&Instruction::I32Mul);
        out.store(f);
        out.load(f);
        out.load(f);
        f.instruction(&Instruction::I32Const(15));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Xor);
        f.instruction(&Instruction::I32Const(0x846ca68bu32 as i32));
        f.instruction(&Instruction::I32Mul);
        out.store(f);
        out.load(f);
        out.load(f);
        f.instruction(&Instruction::I32Const(16));
        f.instruction(&Instruction::I32ShrU);
        f.instruction(&Instruction::I32Xor);
        capacity.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32And);
        out.store(f);
    }

    /// Rebuild relinks existing nodes after allocating the complete replacement
    /// table. Capturing NEXT before each insertion retains all old chains.
    fn emit_array_index_rehash(
        &mut self,
        storage: &GcLocal<ArrayIndexStorage>,
        capacity: I32Local,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let old = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<ArrayIndexStorage>()
                .field(ArrayIndexStorageSchema::BUCKETS)
                .read(storage, schema, f)
                .reference(),
            f,
        );
        let replacement = schema.reserve_gc_local(f).initialize(
            schema.array_type::<ArrayIndexBucketTable>().filled(
                GcOperand::null(schema),
                capacity,
                f,
            ),
            f,
        );
        let old_capacity = schema.reserve_i32_local(f);
        schema
            .array_type::<ArrayIndexBucketTable>()
            .length(&old, schema, f);
        old_capacity.store(f);
        let slot = schema.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        slot.store(f);
        let bucket = schema.reserve_i32_local(f);
        let key = schema.reserve_i64_local(f);
        let cursor = schema
            .reserve_gc_local::<ArrayIndexEntry, Nullable>(f)
            .initialize_null(schema, f);
        let next = schema
            .reserve_gc_local::<ArrayIndexEntry, Nullable>(f)
            .initialize_null(schema, f);
        let head = schema
            .reserve_gc_local::<ArrayIndexEntry, Nullable>(f)
            .initialize_null(schema, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        slot.load(f);
        old_capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        cursor.replace(
            schema
                .array_type::<ArrayIndexBucketTable>()
                .read(&old, slot, schema, f)
                .reference(),
            f,
        );
        let chain_exit = self.open_frame(ControlFrameKind::Block, f);
        let chain = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(schema, f).is_null(f);
        self.emit_branch_if_to_target(chain_exit, f);
        next.replace(
            schema
                .struct_type::<ArrayIndexEntry>()
                .field(ArrayIndexEntrySchema::NEXT)
                .read(&cursor, schema, f)
                .reference(),
            f,
        );
        schema
            .struct_type::<ArrayIndexEntry>()
            .field(ArrayIndexEntrySchema::INDEX)
            .read(&cursor, schema, f)
            .store_i64(key, f);
        Self::emit_array_index_hash(key, capacity, bucket, f);
        head.replace(
            schema
                .array_type::<ArrayIndexBucketTable>()
                .read(&replacement, bucket, schema, f)
                .reference(),
            f,
        );
        schema
            .struct_type::<ArrayIndexEntry>()
            .field(ArrayIndexEntrySchema::NEXT)
            .write(&cursor, GcOperand::reference(&head, schema), schema, f);
        schema.array_type::<ArrayIndexBucketTable>().write(
            &replacement,
            bucket,
            GcOperand::reference(&cursor, schema),
            schema,
            f,
        );
        cursor.replace(next.load(schema, f), f);
        self.emit_branch_to_target(chain, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
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
        schema
            .struct_type::<ArrayIndexStorage>()
            .field(ArrayIndexStorageSchema::BUCKETS)
            .write(
                storage,
                GcOperand::reference(&replacement, schema),
                schema,
                f,
            );
        head.clear(f);
        next.clear(f);
        cursor.clear(f);
        schema.release_i64_local(key, f);
        schema.release_i32_local(bucket, f);
        schema.release_i32_local(slot, f);
        schema.release_i32_local(old_capacity, f);
        replacement.clear(f);
        old.clear(f);
    }

    /// Only JavaScript Array indices, 0 through 2^32-2, reach node publication.
    fn emit_array_index_admission(index: I64Local, f: &mut Function) {
        index.load(f);
        f.instruction(&Instruction::I64Const(u32::MAX as i64));
        f.instruction(&Instruction::I64GeU);
        f.instruction(&Instruction::If(BlockType::Empty));
        f.instruction(&Instruction::Unreachable);
        f.instruction(&Instruction::End);
    }
    pub(crate) fn compile_array_indexed_publish_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::ArrayIndexedPublish);
        let p = self.helper_parameters::<ArrayIndexedPublishParameters>(&mut f);
        Self::emit_array_index_admission(p.index, &mut f);
        let schema = self.runtime_schema();
        let handled =
            self.emit_packed_array_publish_descriptor(&p.storage, p.index, &p.descriptor, &mut f);
        handled.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let dense_handled =
            self.emit_dense_array_publish(&p.storage, p.index, &p.descriptor, &mut f);
        dense_handled.load(&mut f);
        handled.store(&mut f);
        schema.release_i32_local(dense_handled, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let buckets = schema.reserve_gc_local(&mut f).initialize(
            schema
                .struct_type::<ArrayIndexStorage>()
                .field(ArrayIndexStorageSchema::BUCKETS)
                .read(&p.storage, schema, &mut f)
                .reference(),
            &mut f,
        );
        let capacity = schema.reserve_i32_local(&mut f);
        schema
            .array_type::<ArrayIndexBucketTable>()
            .length(&buckets, schema, &mut f);
        capacity.store(&mut f);
        capacity.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        capacity.set_constant(8, &mut f);
        self.emit_array_index_rehash(&p.storage, capacity, &mut f);
        buckets.replace(
            schema
                .field(ArrayIndexStorageSchema::BUCKETS)
                .read(&p.storage, schema, &mut f)
                .reference(),
            &mut f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let bucket = schema.reserve_i32_local(&mut f);
        Self::emit_array_index_hash(p.index, capacity, bucket, &mut f);
        let cursor = schema.reserve_gc_local(&mut f).initialize(
            schema
                .array_type::<ArrayIndexBucketTable>()
                .read(&buckets, bucket, schema, &mut f)
                .reference(),
            &mut f,
        );
        let key = schema.reserve_i64_local(&mut f);
        let count = schema.reserve_i64_local(&mut f);
        let exit = self.open_frame(ControlFrameKind::Block, &mut f);
        let search_exit = self.open_frame(ControlFrameKind::Block, &mut f);
        let search = self.open_frame(ControlFrameKind::Loop, &mut f);
        cursor.load(schema, &mut f).is_null(&mut f);
        self.emit_branch_if_to_target(search_exit, &mut f);
        schema
            .struct_type::<ArrayIndexEntry>()
            .field(ArrayIndexEntrySchema::INDEX)
            .read(&cursor, schema, &mut f)
            .store_i64(key, &mut f);
        key.load(&mut f);
        p.index.load(&mut f);
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, &mut f);
        schema
            .struct_type::<ArrayIndexEntry>()
            .field(ArrayIndexEntrySchema::DESCRIPTOR)
            .write(
                &cursor,
                GcOperand::reference(&p.descriptor, schema),
                schema,
                &mut f,
            );
        self.emit_branch_to_target(exit, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.replace(
            schema
                .struct_type::<ArrayIndexEntry>()
                .field(ArrayIndexEntrySchema::NEXT)
                .read(&cursor, schema, &mut f)
                .reference(),
            &mut f,
        );
        self.emit_branch_to_target(search, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema
            .struct_type::<ArrayIndexStorage>()
            .field(ArrayIndexStorageSchema::COUNT)
            .read(&p.storage, schema, &mut f)
            .store_i64(count, &mut f);
        // Allocate at 75% load. Capacity overflow is a physical GC resource
        // failure, independent of observable Array length or index magnitude.
        count.load(&mut f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Const(4));
        f.instruction(&Instruction::I64Mul);
        capacity.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Const(3));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        capacity.load(&mut f);
        f.instruction(&Instruction::I32Const(i32::MIN));
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        capacity.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        capacity.store(&mut f);
        self.emit_array_index_rehash(&p.storage, capacity, &mut f);
        buckets.replace(
            schema
                .struct_type::<ArrayIndexStorage>()
                .field(ArrayIndexStorageSchema::BUCKETS)
                .read(&p.storage, schema, &mut f)
                .reference(),
            &mut f,
        );
        Self::emit_array_index_hash(p.index, capacity, bucket, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cursor.replace(
            schema
                .array_type::<ArrayIndexBucketTable>()
                .read(&buckets, bucket, schema, &mut f)
                .reference(),
            &mut f,
        );
        let node = schema.reserve_gc_local(&mut f).initialize(
            schema.struct_type::<ArrayIndexEntry>().construct(
                (
                    GcOperand::i64_local(p.index),
                    GcOperand::reference(&p.descriptor, schema),
                    GcOperand::reference(&cursor, schema),
                ),
                &mut f,
            ),
            &mut f,
        );
        // Complete descriptor and node exist before either entry/count escapes.
        schema.array_type::<ArrayIndexBucketTable>().write(
            &buckets,
            bucket,
            GcOperand::nullable_reference(&node, schema),
            schema,
            &mut f,
        );
        count.load(&mut f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        count.store(&mut f);
        schema
            .struct_type::<ArrayIndexStorage>()
            .field(ArrayIndexStorageSchema::COUNT)
            .write(&p.storage, GcOperand::i64_local(count), schema, &mut f);
        node.clear(&mut f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema.release_i64_local(count, &mut f);
        schema.release_i64_local(key, &mut f);
        cursor.clear(&mut f);
        schema.release_i32_local(bucket, &mut f);
        schema.release_i32_local(capacity, &mut f);
        buckets.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(handled, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    pub(crate) fn compile_array_indexed_delete_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::ArrayIndexedDelete);
        let p = self.helper_parameters::<ArrayIndexedDeleteParameters>(&mut f);
        Self::emit_array_index_admission(p.index, &mut f);
        let schema = self.runtime_schema();
        let handled = self.emit_packed_array_delete(&p.storage, p.index, &mut f);
        handled.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let dense_handled = self.emit_dense_array_delete(&p.storage, p.index, &mut f);
        dense_handled.load(&mut f);
        handled.store(&mut f);
        schema.release_i32_local(dense_handled, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let buckets = schema.reserve_gc_local(&mut f).initialize(
            schema
                .struct_type::<ArrayIndexStorage>()
                .field(ArrayIndexStorageSchema::BUCKETS)
                .read(&p.storage, schema, &mut f)
                .reference(),
            &mut f,
        );
        let capacity = schema.reserve_i32_local(&mut f);
        schema
            .array_type::<ArrayIndexBucketTable>()
            .length(&buckets, schema, &mut f);
        capacity.store(&mut f);
        capacity.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let bucket = schema.reserve_i32_local(&mut f);
        Self::emit_array_index_hash(p.index, capacity, bucket, &mut f);
        let cursor = schema.reserve_gc_local(&mut f).initialize(
            schema
                .array_type::<ArrayIndexBucketTable>()
                .read(&buckets, bucket, schema, &mut f)
                .reference(),
            &mut f,
        );
        let previous = schema
            .reserve_gc_local::<ArrayIndexEntry, Nullable>(&mut f)
            .initialize_null(schema, &mut f);
        let next = schema
            .reserve_gc_local::<ArrayIndexEntry, Nullable>(&mut f)
            .initialize_null(schema, &mut f);
        let key = schema.reserve_i64_local(&mut f);
        let count = schema.reserve_i64_local(&mut f);
        let exit = self.open_frame(ControlFrameKind::Block, &mut f);
        let repeat = self.open_frame(ControlFrameKind::Loop, &mut f);
        cursor.load(schema, &mut f).is_null(&mut f);
        self.emit_branch_if_to_target(exit, &mut f);
        schema
            .struct_type::<ArrayIndexEntry>()
            .field(ArrayIndexEntrySchema::INDEX)
            .read(&cursor, schema, &mut f)
            .store_i64(key, &mut f);
        key.load(&mut f);
        p.index.load(&mut f);
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, &mut f);
        next.replace(
            schema
                .struct_type::<ArrayIndexEntry>()
                .field(ArrayIndexEntrySchema::NEXT)
                .read(&cursor, schema, &mut f)
                .reference(),
            &mut f,
        );
        previous.load(schema, &mut f).is_null(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        schema.array_type::<ArrayIndexBucketTable>().write(
            &buckets,
            bucket,
            GcOperand::reference(&next, schema),
            schema,
            &mut f,
        );
        f.instruction(&Instruction::Else);
        schema
            .struct_type::<ArrayIndexEntry>()
            .field(ArrayIndexEntrySchema::NEXT)
            .write(
                &previous,
                GcOperand::reference(&next, schema),
                schema,
                &mut f,
            );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema
            .struct_type::<ArrayIndexStorage>()
            .field(ArrayIndexStorageSchema::COUNT)
            .read(&p.storage, schema, &mut f)
            .store_i64(count, &mut f);
        count.load(&mut f);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Sub);
        count.store(&mut f);
        schema
            .struct_type::<ArrayIndexStorage>()
            .field(ArrayIndexStorageSchema::COUNT)
            .write(&p.storage, GcOperand::i64_local(count), schema, &mut f);
        // Dropping chain roots before rebuild lets deleted descriptors reclaim.
        cursor.set_null(schema, &mut f);
        previous.set_null(schema, &mut f);
        next.set_null(schema, &mut f);
        count.load(&mut f);
        f.instruction(&Instruction::I64Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let empty = schema
            .reserve_gc_local(&mut f)
            .initialize(schema.empty_array_buckets(&mut f), &mut f);
        schema.field(ArrayIndexStorageSchema::BUCKETS).write(
            &p.storage,
            GcOperand::reference(&empty, schema),
            schema,
            &mut f,
        );
        empty.clear(&mut f);
        f.instruction(&Instruction::Else);
        capacity.load(&mut f);
        f.instruction(&Instruction::I32Const(8));
        f.instruction(&Instruction::I32GtU);
        count.load(&mut f);
        f.instruction(&Instruction::I64Const(4));
        f.instruction(&Instruction::I64Mul);
        capacity.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64LtU);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, &mut f);
        capacity.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32ShrU);
        capacity.store(&mut f);
        self.emit_array_index_rehash(&p.storage, capacity, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(exit, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        previous.replace(cursor.load(schema, &mut f), &mut f);
        cursor.replace(
            schema
                .struct_type::<ArrayIndexEntry>()
                .field(ArrayIndexEntrySchema::NEXT)
                .read(&cursor, schema, &mut f)
                .reference(),
            &mut f,
        );
        self.emit_branch_to_target(repeat, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema.release_i64_local(count, &mut f);
        schema.release_i64_local(key, &mut f);
        next.clear(&mut f);
        previous.clear(&mut f);
        cursor.clear(&mut f);
        schema.release_i32_local(bucket, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(capacity, &mut f);
        buckets.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(handled, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    /// Snapshot traversal visits every occupied entry once; in-place heapsort
    /// gives ascending index order using one exact-size private GC buffer.
    pub(crate) fn emit_array_indexed_keys(
        &mut self,
        array: &GcLocal<ArrayObject>,
        f: &mut Function,
    ) -> GcLocal<ArrayIndexKeyTable> {
        let schema = self.runtime_schema();
        let storage = self.emit_array_index_storage(array, f);
        let buckets = schema.reserve_gc_local(f).initialize(
            schema
                .struct_type::<ArrayIndexStorage>()
                .field(ArrayIndexStorageSchema::BUCKETS)
                .read(&storage, schema, f)
                .reference(),
            f,
        );
        let count64 = schema.reserve_i64_local(f);
        schema
            .struct_type::<ArrayIndexStorage>()
            .field(ArrayIndexStorageSchema::COUNT)
            .read(&storage, schema, f)
            .store_i64(count64, f);
        let count = schema.reserve_i32_local(f);
        count64.load(f);
        f.instruction(&Instruction::I32WrapI64);
        count.store(f);
        let keys = ArrayIndexKeyConstruction::allocate(schema, count, f);
        let capacity = schema.reserve_i32_local(f);
        schema
            .array_type::<ArrayIndexBucketTable>()
            .length(&buckets, schema, f);
        capacity.store(f);
        let bucket = schema.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        bucket.store(f);
        let output = schema.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        output.store(f);
        let key = schema.reserve_i64_local(f);
        let cursor = schema
            .reserve_gc_local::<ArrayIndexEntry, Nullable>(f)
            .initialize_null(schema, f);
        schema.array_storage_is_packed(&storage, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_packed_array_keys(&storage, &keys, output, f);
        f.instruction(&Instruction::Else);
        schema.array_storage_mode_is(&storage, ArrayIndexedStorageMode::DenseDescriptors, f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_dense_array_keys(&storage, &keys, output, f);
        f.instruction(&Instruction::Else);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        bucket.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        cursor.replace(
            schema
                .array_type::<ArrayIndexBucketTable>()
                .read(&buckets, bucket, schema, f)
                .reference(),
            f,
        );
        let chain_exit = self.open_frame(ControlFrameKind::Block, f);
        let chain = self.open_frame(ControlFrameKind::Loop, f);
        cursor.load(schema, f).is_null(f);
        self.emit_branch_if_to_target(chain_exit, f);
        schema
            .struct_type::<ArrayIndexEntry>()
            .field(ArrayIndexEntrySchema::INDEX)
            .read(&cursor, schema, f)
            .store_i64(key, f);
        keys.write(output, key, schema, f);
        output.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        output.store(f);
        cursor.replace(
            schema
                .struct_type::<ArrayIndexEntry>()
                .field(ArrayIndexEntrySchema::NEXT)
                .read(&cursor, schema, f)
                .reference(),
            f,
        );
        self.emit_branch_to_target(chain, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        bucket.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        bucket.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        output.load(f);
        count.load(f);
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        f.instruction(&Instruction::Unreachable);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        capacity.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_array_index_sort(&keys, count, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let result = schema
            .reserve_gc_local(f)
            .initialize(keys.publish(schema, f), f);
        cursor.clear(f);
        schema.release_i64_local(key, f);
        schema.release_i32_local(output, f);
        schema.release_i32_local(bucket, f);
        schema.release_i32_local(capacity, f);
        schema.release_i32_local(count, f);
        schema.release_i64_local(count64, f);
        buckets.clear(f);
        storage.clear(f);
        result
    }

    pub(super) fn emit_array_index_sort(
        &mut self,
        keys: &ArrayIndexKeyConstruction,
        count: I32Local,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let root = schema.reserve_i32_local(f);
        count.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32ShrU);
        root.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        root.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        root.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        root.store(f);
        self.emit_array_index_sift(keys, root, count, f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let remaining = schema.reserve_i32_local(f);
        count.load(f);
        remaining.store(f);
        let zero = schema.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        zero.store(f);
        let left = schema.reserve_i64_local(f);
        let right = schema.reserve_i64_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        remaining.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32LeU);
        self.emit_branch_if_to_target(exit, f);
        remaining.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        remaining.store(f);
        keys.read(zero, left, schema, f);
        keys.read(remaining, right, schema, f);
        keys.write(zero, right, schema, f);
        keys.write(remaining, left, schema, f);
        self.emit_array_index_sift(keys, zero, remaining, f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema.release_i64_local(right, f);
        schema.release_i64_local(left, f);
        schema.release_i32_local(zero, f);
        schema.release_i32_local(remaining, f);
        schema.release_i32_local(root, f);
    }
    fn emit_array_index_sift(
        &mut self,
        keys: &ArrayIndexKeyConstruction,
        start: I32Local,
        count: I32Local,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let root = schema.reserve_i32_local(f);
        start.load(f);
        root.store(f);
        let child = schema.reserve_i32_local(f);
        let best = schema.reserve_i32_local(f);
        let other = schema.reserve_i32_local(f);
        let value = schema.reserve_i64_local(f);
        let best_value = schema.reserve_i64_local(f);
        let other_value = schema.reserve_i64_local(f);
        let child64 = schema.reserve_i64_local(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        root.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Const(2));
        f.instruction(&Instruction::I64Mul);
        f.instruction(&Instruction::I64Const(1));
        f.instruction(&Instruction::I64Add);
        child64.store(f);
        child64.load(f);
        count.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        child64.load(f);
        f.instruction(&Instruction::I32WrapI64);
        child.store(f);
        child.load(f);
        best.store(f);
        keys.read(best, best_value, schema, f);
        child.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        other.store(f);
        other.load(f);
        count.load(f);
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        keys.read(other, other_value, schema, f);
        other_value.load(f);
        best_value.load(f);
        f.instruction(&Instruction::I64GtU);
        self.open_frame(ControlFrameKind::If, f);
        other.load(f);
        best.store(f);
        other_value.load(f);
        best_value.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        keys.read(root, value, schema, f);
        value.load(f);
        best_value.load(f);
        f.instruction(&Instruction::I64GeU);
        self.emit_branch_if_to_target(exit, f);
        keys.write(root, best_value, schema, f);
        keys.write(best, value, schema, f);
        best.load(f);
        root.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema.release_i64_local(child64, f);
        schema.release_i64_local(other_value, f);
        schema.release_i64_local(best_value, f);
        schema.release_i64_local(value, f);
        schema.release_i32_local(other, f);
        schema.release_i32_local(best, f);
        schema.release_i32_local(child, f);
        schema.release_i32_local(root, f);
    }
}
