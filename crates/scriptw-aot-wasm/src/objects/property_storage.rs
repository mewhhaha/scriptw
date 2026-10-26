//! Capacity growth shared by the named and Arguments descriptor tables.
use super::*;

mod index;

/// The borrowed canonical key remains rooted until probe and insertion finish.
/// Each private publication/definition path owns and releases its prepared hash.
pub(super) struct PreparedOrdinaryPropertyKey<'key> {
    pub(super) key: &'key PropertyKeyLocals,
    pub(super) hash: I64Local,
}

/// One ordinary definition owns its object/key pairing, lookup result and
/// optional hash. Descriptor validation/merge cannot invoke JavaScript, so the
/// lookup remains valid until publication. Empty rejected definitions never hash.
#[must_use]
pub(super) struct PreparedOrdinaryDefinition<'key> {
    object: &'key GcLocal<OrdinaryObject>,
    prepared: PreparedOrdinaryPropertyKey<'key>,
    hash_ready: I32Local,
    current: GcLocal<PropertyDescriptor, Nullable>,
}

impl PreparedOrdinaryDefinition<'_> {
    pub(super) fn current(&self) -> &GcLocal<PropertyDescriptor, Nullable> {
        &self.current
    }

    pub(super) fn release(self, schema: &crate::gc_types::RuntimeSchema, function: &mut Function) {
        self.current.clear(function);
        schema.release_i32_local(self.hash_ready, function);
        schema.release_i64_local(self.prepared.hash, function);
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn emit_prepare_ordinary_definition<'key>(
        &mut self,
        object: &'key GcLocal<OrdinaryObject>,
        key: &'key PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<PreparedOrdinaryDefinition<'key>, EmitError> {
        let schema = self.runtime_schema();
        let prepared = PreparedOrdinaryPropertyKey {
            key,
            hash: schema.reserve_i64_local(function),
        };
        let hash_ready = schema.reserve_i32_local(function);
        function.instruction(&Instruction::I32Const(0));
        hash_ready.store(function);
        let entry =
            self.emit_captured_ordinary_property_entry(object, &prepared, hash_ready, function)?;
        let current = self.emit_ordinary_entry_descriptor_reference(&entry, function);
        entry.clear(function);
        Ok(PreparedOrdinaryDefinition {
            object,
            prepared,
            hash_ready,
            current,
        })
    }

    /// Called only after descriptor compatibility succeeds. This ordinary path
    /// has no observable callbacks between its initial lookup and publication.
    pub(super) fn emit_publish_ordinary_definition(
        &mut self,
        lookup: &PreparedOrdinaryDefinition<'_>,
        descriptor: &GcLocal<PropertyDescriptor>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        lookup.current.load(schema, function).is_null(function);
        self.open_frame(ControlFrameKind::If, function);
        lookup.hash_ready.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_property_key_hash(lookup.prepared.key, lookup.prepared.hash, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.emit_ordinary_insert_prepared_property(
            lookup.object,
            &lookup.prepared,
            descriptor,
            function,
        );
        function.instruction(&Instruction::Else);
        self.emit_copy_complete_descriptor(&lookup.current, descriptor, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_ordinary_append_property_entry(
        &self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        descriptor: &GcLocal<PropertyDescriptor>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema().call_helper(
            crate::runtime_helpers::OrdinaryPropertyAppendArguments::new(object, key, descriptor),
            self.runtime_helper_base()?,
            function,
        );
        Ok(())
    }

    pub(crate) fn compile_ordinary_property_append_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut function =
            self.begin_helper_body(crate::runtime_helpers::RuntimeHelperId::OrdinaryPropertyAppend);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::OrdinaryPropertyAppendParameters>(
                &mut function,
            );
        self.emit_ordinary_append_property_entry_inner(
            &parameters.object,
            &parameters.key,
            &parameters.descriptor,
            &mut function,
        )?;
        parameters.release(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Appends the actual GC entry in insertion order. Full tables compact
    /// live entries before growing, so delete/reinsert history stays bounded.
    fn emit_ordinary_append_property_entry_inner(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        descriptor: &GcLocal<PropertyDescriptor>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        // Every publication path, including intrinsic installation, keeps one
        // entry per key. Replacement preserves the first insertion position.
        let hash = schema.reserve_i64_local(function);
        self.emit_property_key_hash(key, hash, function);
        let prepared = PreparedOrdinaryPropertyKey { key, hash };
        let previous = self.emit_prepared_ordinary_property_entry(object, &prepared, function)?;
        previous.load(schema, function).is_null(function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_ordinary_insert_prepared_property(object, &prepared, descriptor, function);
        function.instruction(&Instruction::Else);
        let present = schema.reserve_gc_local(function).initialize(
            previous.load(schema, function).require_non_null(function),
            function,
        );
        schema.field(PropertyEntrySchema::DESCRIPTOR).write(
            &present,
            GcOperand::reference(descriptor, schema),
            schema,
            function,
        );
        present.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        previous.clear(function);
        schema.release_i64_local(prepared.hash, function);
        Ok(())
    }

    /// The caller has already proved absence without invoking user code.
    fn emit_ordinary_insert_prepared_property(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        prepared: &PreparedOrdinaryPropertyKey<'_>,
        descriptor: &GcLocal<PropertyDescriptor>,
        function: &mut Function,
    ) {
        let storage = self.emit_dictionary_property_storage(object, function);
        self.emit_ordinary_insert_in_dictionary(&storage, prepared, descriptor, function);
        storage.clear(function);
    }

    pub(super) fn emit_ordinary_insert_in_dictionary(
        &mut self,
        storage: &GcLocal<OrdinaryPropertyStorage>,
        prepared: &PreparedOrdinaryPropertyKey<'_>,
        descriptor: &GcLocal<PropertyDescriptor>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let key = prepared.key;
        let hash = prepared.hash;
        let length = schema.reserve_i32_local(function);
        let new_length = schema.reserve_i32_local(function);
        schema
            .field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(storage, schema, function)
            .store(length, function);
        let table = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(storage, schema, function)
                .reference(),
            function,
        );
        let entries = schema.array_type::<PropertyTable>();
        length.load(function);
        entries.length(&table, schema, function);
        function.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .call_helper(
                crate::runtime_helpers::OrdinaryPropertyPrepareArguments::new(storage, length),
                self.runtime_helper_base()
                    .expect("dictionary mutation requires registered runtime kernels"),
                function,
            )
            .store(length, function);
        // Preparation may compact or replace storage. Retain the original
        // rooted table when there is room; reload only after that mutation.
        table.replace(
            schema
                .field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(storage, schema, function)
                .reference(),
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        let buckets = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryPropertyStorageSchema::INDEX)
                .read(storage, schema, function)
                .reference(),
            function,
        );
        length.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        new_length.store(function);
        let stored_key_slot = schema.reserve_gc_local(function);
        let stored_key = stored_key_slot.initialize(
            schema
                .struct_type::<StoredValue>()
                .from_value(key.value(), function),
            function,
        );
        let entry_slot = schema.reserve_gc_local(function);
        let entry = entry_slot.initialize(
            schema.struct_type::<PropertyEntry>().construct(
                (
                    GcOperand::reference(&stored_key, schema),
                    GcOperand::reference(descriptor, schema),
                    GcOperand::i64_local(hash),
                    GcOperand::i32_local(length),
                ),
                function,
            ),
            function,
        );
        entries.write(
            &table,
            length,
            GcOperand::nullable_reference(&entry, schema),
            schema,
            function,
        );
        self.emit_ordinary_property_index_publish(&buckets, &table, hash, length, function);
        schema.field(OrdinaryPropertyStorageSchema::LENGTH).write(
            storage,
            GcOperand::i32_local(new_length),
            schema,
            function,
        );
        entry.clear(function);
        stored_key.clear(function);
        schema.release_i32_local(new_length, function);
        schema.release_i32_local(length, function);
        buckets.clear(function);
        table.clear(function);
    }

    /// Sole physical table-compaction body. No JS callback can observe its
    /// unpublished replacement, and immutable entry positions are rebuilt.
    pub(crate) fn compile_ordinary_property_prepare_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self
            .begin_helper_body(crate::runtime_helpers::RuntimeHelperId::OrdinaryPropertyPrepare);
        let p = self
            .helper_parameters::<crate::runtime_helpers::OrdinaryPropertyPrepareParameters>(&mut f);
        let schema = self.runtime_schema();
        let capacity = schema.reserve_i32_local(&mut f);
        let index_capacity = schema.reserve_i32_local(&mut f);
        let source = schema.reserve_i32_local(&mut f);
        let destination = schema.reserve_i32_local(&mut f);
        let live = schema.reserve_i32_local(&mut f);
        let hash = schema.reserve_i64_local(&mut f);
        let old = schema.reserve_gc_local(&mut f).initialize(
            schema
                .field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(&p.storage, schema, &mut f)
                .reference(),
            &mut f,
        );
        schema
            .array_type::<PropertyTable>()
            .length(&old, schema, &mut f);
        capacity.store(&mut f);
        p.count.load(&mut f);
        capacity.load(&mut f);
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, &mut f);
        f.instruction(&Instruction::I32Const(0));
        source.store(&mut f);
        f.instruction(&Instruction::I32Const(0));
        live.store(&mut f);
        let counted = self.open_frame(ControlFrameKind::Block, &mut f);
        let counting = self.open_frame(ControlFrameKind::Loop, &mut f);
        source.load(&mut f);
        p.count.load(&mut f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(counted, &mut f);
        schema
            .array_type::<PropertyTable>()
            .read(&old, source, schema, &mut f)
            .reference()
            .is_null(&mut f);
        f.instruction(&Instruction::I32Eqz);
        live.load(&mut f);
        f.instruction(&Instruction::I32Add);
        live.store(&mut f);
        source.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        source.store(&mut f);
        self.emit_branch_to_target(counting, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.emit_dictionary_storage_capacity(live, capacity, index_capacity, &mut f);
        let table = schema.reserve_gc_local(&mut f).initialize(
            schema
                .array_type::<PropertyTable>()
                .filled(GcOperand::null(schema), capacity, &mut f),
            &mut f,
        );
        let buckets = schema.reserve_gc_local(&mut f).initialize(
            schema.array_type::<OrdinaryPropertyIndex>().filled(
                GcOperand::i32(0),
                index_capacity,
                &mut f,
            ),
            &mut f,
        );
        f.instruction(&Instruction::I32Const(0));
        source.store(&mut f);
        f.instruction(&Instruction::I32Const(0));
        destination.store(&mut f);
        let copied = self.open_frame(ControlFrameKind::Block, &mut f);
        let copying = self.open_frame(ControlFrameKind::Loop, &mut f);
        source.load(&mut f);
        p.count.load(&mut f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(copied, &mut f);
        let maybe = schema.reserve_gc_local(&mut f).initialize(
            schema
                .array_type::<PropertyTable>()
                .read(&old, source, schema, &mut f)
                .reference(),
            &mut f,
        );
        maybe.load(schema, &mut f).is_null(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let entry = schema
            .reserve_gc_local(&mut f)
            .initialize(maybe.load(schema, &mut f).require_non_null(&mut f), &mut f);
        let key = schema.reserve_gc_local(&mut f).initialize(
            schema
                .field(PropertyEntrySchema::KEY)
                .read(&entry, schema, &mut f)
                .reference(),
            &mut f,
        );
        let descriptor = schema.reserve_gc_local(&mut f).initialize(
            schema
                .field(PropertyEntrySchema::DESCRIPTOR)
                .read(&entry, schema, &mut f)
                .reference(),
            &mut f,
        );
        schema
            .field(PropertyEntrySchema::HASH)
            .read(&entry, schema, &mut f)
            .store_i64(hash, &mut f);
        // A dense growth keeps its existing immutable entry records. Only a
        // moved live property needs a new POSITION; descriptor/key records
        // remain shared in both cases.
        let replacement = schema
            .reserve_gc_local(&mut f)
            .initialize(entry.load(schema, &mut f), &mut f);
        source.load(&mut f);
        destination.load(&mut f);
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, &mut f);
        replacement.replace(
            schema.struct_type::<PropertyEntry>().construct(
                (
                    GcOperand::reference(&key, schema),
                    GcOperand::reference(&descriptor, schema),
                    GcOperand::i64_local(hash),
                    GcOperand::i32_local(destination),
                ),
                &mut f,
            ),
            &mut f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.array_type::<PropertyTable>().write(
            &table,
            destination,
            GcOperand::nullable_reference(&replacement, schema),
            schema,
            &mut f,
        );
        self.emit_ordinary_property_index_publish(&buckets, &table, hash, destination, &mut f);
        destination.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        destination.store(&mut f);
        replacement.clear(&mut f);
        descriptor.clear(&mut f);
        key.clear(&mut f);
        entry.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        maybe.clear(&mut f);
        source.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        source.store(&mut f);
        self.emit_branch_to_target(copying, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema.field(OrdinaryPropertyStorageSchema::ENTRIES).write(
            &p.storage,
            GcOperand::reference(&table, schema),
            schema,
            &mut f,
        );
        schema.field(OrdinaryPropertyStorageSchema::INDEX).write(
            &p.storage,
            GcOperand::reference(&buckets, schema),
            schema,
            &mut f,
        );
        schema.field(OrdinaryPropertyStorageSchema::LENGTH).write(
            &p.storage,
            GcOperand::i32_local(destination),
            schema,
            &mut f,
        );
        destination.load(&mut f);
        p.count.store(&mut f);
        buckets.clear(&mut f);
        table.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        old.clear(&mut f);
        schema.release_i64_local(hash, &mut f);
        schema.release_i32_local(live, &mut f);
        schema.release_i32_local(destination, &mut f);
        schema.release_i32_local(source, &mut f);
        schema.release_i32_local(index_capacity, &mut f);
        schema.release_i32_local(capacity, &mut f);
        p.count.load(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    /// Mask-based probes need power-of-two capacities; reserving twice the live
    /// extent leaves spare slots between compactions. Clamp that reservation
    /// before rounding, and produce the ordered/index pair at one allocation
    /// seam so both invariants hold even near the private address limit.
    fn emit_dictionary_storage_capacity(
        &self,
        live: I32Local,
        ordered: I32Local,
        index: I32Local,
        function: &mut Function,
    ) {
        // At this live extent the maximum ordered capacity has no append slot.
        // Below it, doubling live itself cannot wrap an unsigned i32.
        live.load(function);
        function.instruction(&Instruction::I32Const(1 << 30));
        function.instruction(&Instruction::I32GeU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        live.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Shl);
        ordered.store(function);
        // An unrepresentable 2x reservation must not reject a representable
        // live extent plus its next property: retain the maximum capacity.
        ordered.load(function);
        function.instruction(&Instruction::I32Const(1 << 30));
        function.instruction(&Instruction::I32GtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(1 << 30));
        ordered.store(function);
        function.instruction(&Instruction::End);
        ordered.load(function);
        function.instruction(&Instruction::I32Const(4));
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(4));
        ordered.store(function);
        function.instruction(&Instruction::End);
        // ceil(log2(reserved)) = 32 - clz(reserved - 1). The reservation is
        // already within [4, 2^30], so neither subtraction nor shift can wrap.
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Const(32));
        ordered.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Sub);
        function.instruction(&Instruction::I32Clz);
        function.instruction(&Instruction::I32Sub);
        function.instruction(&Instruction::I32Shl);
        ordered.store(function);
        ordered.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Shl);
        index.store(function);
    }

    /// Chooses max(4, required, 2 * capacity), without unsigned i32 wrap.
    /// An unrepresentable required extent is a private storage failure, never
    /// a fabricated JavaScript exception or silently truncated property list.
    pub(super) fn emit_property_table_growth_capacity(
        &self,
        capacity: I32Local,
        required: I32Local,
        output: I32Local,
        function: &mut Function,
    ) {
        required.load(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::Unreachable);
        function.instruction(&Instruction::End);
        capacity.load(function);
        function.instruction(&Instruction::I32Const(i32::MAX));
        function.instruction(&Instruction::I32LeU);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        capacity.load(function);
        function.instruction(&Instruction::I32Const(2));
        function.instruction(&Instruction::I32Mul);
        function.instruction(&Instruction::Else);
        required.load(function);
        function.instruction(&Instruction::End);
        output.store(function);
        output.load(function);
        required.load(function);
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        required.load(function);
        output.store(function);
        function.instruction(&Instruction::End);
        output.load(function);
        function.instruction(&Instruction::I32Const(4));
        function.instruction(&Instruction::I32LtU);
        function.instruction(&Instruction::If(BlockType::Empty));
        function.instruction(&Instruction::I32Const(4));
        output.store(function);
        function.instruction(&Instruction::End);
    }
}
