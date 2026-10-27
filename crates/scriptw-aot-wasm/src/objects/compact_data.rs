//! Bounded fresh data literals share immutable string-only shapes. Mutable
//! values remain strongly rooted in tag/scalar/reference arrays. Structural
//! mutation replaces the complete storage once; no PropertyEntry is fabricated.
use super::property_storage::PreparedOrdinaryPropertyKey;
use super::*;
use crate::gc_types::*;

mod cache;
mod native_function;
mod shape;

impl FunctionBuilder<'_> {
    /// Dictionary-entry consumers cannot observe a compact storage as an entry.
    /// The normal dictionary arm is an inline type check and cast; only a real
    /// transition calls the optimized runtime conversion kernel.
    pub(super) fn emit_dictionary_property_storage(
        &self,
        object: &GcLocal<OrdinaryObject>,
        f: &mut Function,
    ) -> GcLocal<OrdinaryPropertyStorage> {
        let schema = self.runtime_schema();
        let storage = schema.reserve_gc_local(f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(object, schema, f)
                .reference(),
            f,
        );
        let result = schema
            .reserve_gc_local::<OrdinaryPropertyStorage, Nullable>(f)
            .initialize_null(schema, f);
        storage.is_subtype::<OrdinaryPropertyStorage>(schema, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        result.replace(
            storage
                .load(schema, f)
                .cast_subtype::<OrdinaryPropertyStorage>(schema, f)
                .nullable(),
            f,
        );
        f.instruction(&Instruction::Else);
        let converted = schema.helper_reference_on_stack(
            schema.call_helper(
                crate::runtime_helpers::OrdinaryPropertyStorageEnsureArguments::new(object),
                self.runtime_helper_base()
                    .expect("ordinary storage requires registered runtime kernels"),
                f,
            ),
        );
        result.replace(converted.nullable(), f);
        f.instruction(&Instruction::End);
        let concrete = schema
            .reserve_gc_local(f)
            .initialize(result.load(schema, f).require_non_null(f), f);
        result.clear(f);
        storage.clear(f);
        concrete
    }

    pub(crate) fn compile_ordinary_property_storage_ensure_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::OrdinaryPropertyStorageEnsure);
        let p = self
            .helper_parameters::<crate::runtime_helpers::OrdinaryPropertyStorageEnsureParameters>(
                &mut f,
            );
        let schema = self.runtime_schema();
        let base = schema.reserve_gc_local(&mut f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&p.object, schema, &mut f)
                .reference(),
            &mut f,
        );
        let result = schema
            .reserve_gc_local::<OrdinaryPropertyStorage, Nullable>(&mut f)
            .initialize_null(schema, &mut f);
        base.is_subtype::<OrdinaryPropertyStorage>(schema, &mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        result.replace(
            base.load(schema, &mut f)
                .cast_subtype::<OrdinaryPropertyStorage>(schema, &mut f)
                .nullable(),
            &mut f,
        );
        f.instruction(&Instruction::Else);
        let compact = schema.reserve_gc_local(&mut f).initialize(
            base.load(schema, &mut f)
                .cast_subtype::<CompactDataStorage>(schema, &mut f),
            &mut f,
        );
        let entries = schema
            .reserve_gc_local(&mut f)
            .initialize(schema.empty_property_table(&mut f), &mut f);
        let buckets = schema
            .reserve_gc_local(&mut f)
            .initialize(schema.empty_property_index(&mut f), &mut f);
        let dictionary = schema.reserve_gc_local(&mut f).initialize(
            schema.struct_type::<OrdinaryPropertyStorage>().construct(
                (
                    GcOperand::reference(&entries, schema),
                    GcOperand::i32(0),
                    GcOperand::reference(&buckets, schema),
                ),
                &mut f,
            ),
            &mut f,
        );
        let keys = self.emit_compact_shape_keys(&compact, &mut f);
        let attributes = self.emit_compact_data_attributes(&compact, &mut f);
        let count = schema.reserve_i32_local(&mut f);
        let index = schema.reserve_i32_local(&mut f);
        schema
            .array_type::<CompactShapeKeys>()
            .length(&keys, schema, &mut f);
        count.store(&mut f);
        f.instruction(&Instruction::I32Const(0));
        index.store(&mut f);
        let value = schema.reserve_value_local(&mut f);
        let exit = self.open_frame(ControlFrameKind::Block, &mut f);
        let repeat = self.open_frame(ControlFrameKind::Loop, &mut f);
        index.load(&mut f);
        count.load(&mut f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, &mut f);
        let name = schema.reserve_gc_local(&mut f).initialize(
            schema
                .array_type::<CompactShapeKeys>()
                .read(&keys, index, schema, &mut f)
                .reference(),
            &mut f,
        );
        let key = PropertyKeyLocals::from_string(schema, &name, &mut f);
        self.emit_compact_value_at(&compact, index, &value, &mut f);
        let descriptor = self.emit_alloc_data_property_descriptor_with_word(
            attributes.operand(),
            &value,
            &mut f,
        );
        let hash = schema.reserve_i64_local(&mut f);
        self.emit_property_key_hash(&key, hash, &mut f);
        self.emit_ordinary_insert_in_dictionary(
            &dictionary,
            &PreparedOrdinaryPropertyKey { key: &key, hash },
            &descriptor,
            &mut f,
        );
        schema.release_i64_local(hash, &mut f);
        descriptor.clear(&mut f);
        key.clear(&mut f);
        name.clear(&mut f);
        index.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        index.store(&mut f);
        self.emit_branch_to_target(repeat, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // Publish only after the complete replacement graph owns every value.
        schema.field(OrdinaryObjectSchema::PROPERTIES).write(
            &p.object,
            GcOperand::subtype_reference(&dictionary, schema),
            schema,
            &mut f,
        );
        result.replace(dictionary.load(schema, &mut f).nullable(), &mut f);
        attributes.clear(schema, &mut f);
        value.clear(&mut f);
        keys.clear(&mut f);
        dictionary.clear(&mut f);
        buckets.clear(&mut f);
        entries.clear(&mut f);
        compact.clear(&mut f);
        schema.release_i32_local(index, &mut f);
        schema.release_i32_local(count, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        result.load(schema, &mut f).require_non_null(&mut f);
        result.clear(&mut f);
        base.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    pub(super) fn emit_compact_shape_keys(
        &self,
        storage: &GcLocal<CompactDataStorage>,
        f: &mut Function,
    ) -> GcLocal<CompactShapeKeys> {
        let schema = self.runtime_schema();
        let shape = schema.reserve_gc_local(f).initialize(
            schema
                .field(CompactDataStorageSchema::SHAPE)
                .read(storage, schema, f)
                .reference(),
            f,
        );
        let keys = schema.reserve_gc_local(f).initialize(
            schema
                .field(CompactDataShapeSchema::KEYS)
                .read(&shape, schema, f)
                .reference(),
            f,
        );
        shape.clear(f);
        keys
    }

    /// Immutable uniform data attributes travel with the shape. Reading the
    /// declared word retains its typed domain through descriptor publication.
    fn emit_compact_data_attributes(
        &self,
        storage: &GcLocal<CompactDataStorage>,
        f: &mut Function,
    ) -> GcI32DomainLocal<crate::heap::DescriptorWord> {
        let schema = self.runtime_schema();
        let shape = schema.reserve_gc_local(f).initialize(
            schema
                .field(CompactDataStorageSchema::SHAPE)
                .read(storage, schema, f)
                .reference(),
            f,
        );
        let attributes = GcI32DomainLocal::new(
            schema,
            StoredPropertyAttributes::Data {
                writable: true,
                enumerable: true,
                configurable: true,
            }
            .descriptor_word(),
            f,
        );
        schema
            .field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&shape, schema, f)
            .store_domain(&attributes, f);
        shape.clear(f);
        attributes
    }

    /// A canonical string key and the actual immutable shape decide the slot.
    /// Symbols never compare equal and cannot be retained in shape metadata.
    fn emit_compact_slot_probe(
        &mut self,
        storage: &GcLocal<CompactDataStorage>,
        key: &PropertyKeyLocals,
        slot: I32Local,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        f.instruction(&Instruction::I32Const(0));
        found.store(f);
        key.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let keys = self.emit_compact_shape_keys(storage, f);
        let length = schema.reserve_i32_local(f);
        schema
            .array_type::<CompactShapeKeys>()
            .length(&keys, schema, f);
        length.store(f);
        f.instruction(&Instruction::I32Const(0));
        slot.store(f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        let repeat = self.open_frame(ControlFrameKind::Loop, f);
        slot.load(f);
        length.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, f);
        let name = schema.reserve_gc_local(f).initialize(
            schema
                .array_type::<CompactShapeKeys>()
                .read(&keys, slot, schema, f)
                .reference(),
            f,
        );
        let actual = schema.reserve_value_local(f);
        actual.set_reference(&name, schema, f);
        self.emit_tagged_payload_same_value_i32(&actual, key.value(), f)?;
        found.store(f);
        actual.clear(f);
        name.clear(f);
        found.load(f);
        self.emit_branch_if_to_target(exit, f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        slot.store(f);
        self.emit_branch_to_target(repeat, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema.release_i32_local(length, f);
        keys.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_compact_value_at(
        &self,
        storage: &GcLocal<CompactDataStorage>,
        slot: I32Local,
        value: &ValueLocals,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let tags = schema.reserve_gc_local(f).initialize(
            schema
                .field(CompactDataStorageSchema::TAGS)
                .read(storage, schema, f)
                .reference(),
            f,
        );
        let scalars = schema.reserve_gc_local(f).initialize(
            schema
                .field(CompactDataStorageSchema::SCALARS)
                .read(storage, schema, f)
                .reference(),
            f,
        );
        let references = schema.reserve_gc_local(f).initialize(
            schema
                .field(CompactDataStorageSchema::REFERENCES)
                .read(storage, schema, f)
                .reference(),
            f,
        );
        schema
            .array_type::<CompactTagArray>()
            .read(&tags, slot, schema, f)
            .store(value.tag(), f);
        value.scalar().set_constant(0, f);
        scalars.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        let present = schema
            .reserve_gc_local(f)
            .initialize(scalars.load(schema, f).require_non_null(f), f);
        schema
            .array_type::<CompactScalarArray>()
            .read(&present, slot, schema, f)
            .store_i64(value.scalar(), f);
        present.clear(f);
        f.instruction(&Instruction::End);
        value.reference().clear(f);
        references.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        let present = schema
            .reserve_gc_local(f)
            .initialize(references.load(schema, f).require_non_null(f), f);
        schema
            .array_type::<CompactReferenceArray>()
            .read(&present, slot, schema, f)
            .store_eq(value.reference(), f);
        present.clear(f);
        f.instruction(&Instruction::End);
        references.clear(f);
        scalars.clear(f);
        tags.clear(f);
    }

    pub(super) fn emit_compact_store_at(
        &self,
        storage: &GcLocal<CompactDataStorage>,
        slot: I32Local,
        value: &ValueLocals,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let tags = schema.reserve_gc_local(f).initialize(
            schema
                .field(CompactDataStorageSchema::TAGS)
                .read(storage, schema, f)
                .reference(),
            f,
        );
        let scalars = schema.reserve_gc_local(f).initialize(
            schema
                .field(CompactDataStorageSchema::SCALARS)
                .read(storage, schema, f)
                .reference(),
            f,
        );
        let references = schema.reserve_gc_local(f).initialize(
            schema
                .field(CompactDataStorageSchema::REFERENCES)
                .read(storage, schema, f)
                .reference(),
            f,
        );
        schema.array_type::<CompactTagArray>().write(
            &tags,
            slot,
            GcOperand::i32_local(value.tag()),
            schema,
            f,
        );
        // Missing scalar backing encodes zero bits for every present slot.
        // Allocate only when one slot first needs a nonzero payload; negative
        // zero and NaN have nonzero bits and therefore take this path too.
        scalars.load(schema, f).is_null(f);
        value.scalar().load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::If(BlockType::Empty));
        let length = schema.reserve_i32_local(f);
        schema
            .array_type::<CompactTagArray>()
            .length(&tags, schema, f);
        length.store(f);
        let allocated = schema.reserve_gc_local(f).initialize(
            schema
                .array_type::<CompactScalarArray>()
                .filled(GcOperand::i64(0), length, f),
            f,
        );
        schema.field(CompactDataStorageSchema::SCALARS).write(
            storage,
            GcOperand::nullable_reference(&allocated, schema),
            schema,
            f,
        );
        scalars.replace(allocated.load(schema, f).nullable(), f);
        allocated.clear(f);
        schema.release_i32_local(length, f);
        f.instruction(&Instruction::End);
        scalars.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        let present = schema
            .reserve_gc_local(f)
            .initialize(scalars.load(schema, f).require_non_null(f), f);
        schema.array_type::<CompactScalarArray>().write(
            &present,
            slot,
            GcOperand::i64_local(value.scalar()),
            schema,
            f,
        );
        present.clear(f);
        f.instruction(&Instruction::End);
        // Missing reference backing encodes null for every present slot. The
        // tag still distinguishes undefined/null/scalars from missing properties.
        references.load(schema, f).is_null(f);
        value.reference().load(f);
        f.instruction(&Instruction::RefIsNull);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::If(BlockType::Empty));
        let length = schema.reserve_i32_local(f);
        schema
            .array_type::<CompactTagArray>()
            .length(&tags, schema, f);
        length.store(f);
        let undefined = schema.reserve_value_local(f);
        undefined.set_undefined(f);
        let allocated = schema.reserve_gc_local(f).initialize(
            schema.array_type::<CompactReferenceArray>().filled(
                GcOperand::eq_reference(undefined.reference()),
                length,
                f,
            ),
            f,
        );
        schema.field(CompactDataStorageSchema::REFERENCES).write(
            storage,
            GcOperand::nullable_reference(&allocated, schema),
            schema,
            f,
        );
        references.replace(allocated.load(schema, f).nullable(), f);
        allocated.clear(f);
        undefined.clear(f);
        schema.release_i32_local(length, f);
        f.instruction(&Instruction::End);
        // Once allocated, every write updates the backing, including null
        // overwrites that must retire the old strong edge.
        references.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        let present = schema
            .reserve_gc_local(f)
            .initialize(references.load(schema, f).require_non_null(f), f);
        schema.array_type::<CompactReferenceArray>().write(
            &present,
            slot,
            GcOperand::eq_reference(value.reference()),
            schema,
            f,
        );
        present.clear(f);
        f.instruction(&Instruction::End);
        references.clear(f);
        scalars.clear(f);
        tags.clear(f);
    }

    /// Allocation-free own data probe for the sole ordinary object model.
    /// Accessors and absence both miss, leaving full Get/descriptor consumers
    /// free to preserve their required semantics.
    pub(crate) fn emit_ordinary_own_data_value(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        output: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        f.instruction(&Instruction::I32Const(0));
        found.store(f);
        let base = schema.reserve_gc_local(f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(object, schema, f)
                .reference(),
            f,
        );
        base.is_subtype::<CompactDataStorage>(schema, f);
        self.open_frame(ControlFrameKind::If, f);
        let compact = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<CompactDataStorage>(schema, f),
            f,
        );
        let slot = schema.reserve_i32_local(f);
        self.emit_compact_slot_probe(&compact, key, slot, found, f)?;
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_compact_value_at(&compact, slot, output, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(slot, f);
        compact.clear(f);
        f.instruction(&Instruction::Else);
        let entry = self.emit_ordinary_property_entry(object, key, f)?;
        let descriptor = self.emit_ordinary_entry_descriptor_reference(&entry, f);
        descriptor.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let flags = schema.reserve_i32_local(f);
        schema
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, schema, f)
            .store(flags, f);
        flags.load(f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let stored = schema.reserve_gc_local(f).initialize(
            schema
                .field(PropertyDescriptorSchema::VALUE)
                .read(&descriptor, schema, f)
                .reference(),
            f,
        );
        self.emit_property_value_into(&stored, output, f);
        f.instruction(&Instruction::I32Const(1));
        found.store(f);
        stored.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(flags, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        descriptor.clear(f);
        entry.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        base.clear(f);
        Ok(())
    }

    /// Successful existing own-data Set only when target and receiver are the
    /// same concrete ordinary object. Misses perform no mutation or completion.
    pub(super) fn emit_compact_existing_data_write(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(f);
        f.instruction(&Instruction::I32Const(0));
        handled.store(f);
        target.reference().load(f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_try_compact_set(receiver, key, value, handled, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(handled)
    }

    fn emit_try_compact_set(
        &mut self,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        f.instruction(&Instruction::I32Const(0));
        found.store(f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<OrdinaryObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let object = schema
            .reserve_gc_local(f)
            .initialize(receiver.cast_reference::<OrdinaryObject>(schema, f), f);
        let base = schema.reserve_gc_local(f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        base.is_subtype::<CompactDataStorage>(schema, f);
        self.open_frame(ControlFrameKind::If, f);
        let compact = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<CompactDataStorage>(schema, f),
            f,
        );
        let slot = schema.reserve_i32_local(f);
        self.emit_compact_slot_probe(&compact, key, slot, found, f)?;
        let attributes = self.emit_compact_data_attributes(&compact, f);
        found.load(f);
        attributes.load(f);
        f.instruction(&Instruction::I32Const(DescriptorMask::WRITABLE.as_i32()));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        found.store(f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_compact_store_at(&compact, slot, value, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(slot, f);
        attributes.clear(schema, f);
        compact.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        base.clear(f);
        object.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    pub(crate) fn compile_ordinary_own_descriptor_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::OrdinaryOwnDescriptor);
        let p = self
            .helper_parameters::<crate::runtime_helpers::OrdinaryOwnDescriptorParameters>(&mut f);
        let schema = self.runtime_schema();
        let output = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(&mut f)
            .initialize_null(schema, &mut f);
        let base = schema.reserve_gc_local(&mut f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&p.object, schema, &mut f)
                .reference(),
            &mut f,
        );
        base.is_subtype::<CompactDataStorage>(schema, &mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let compact = schema.reserve_gc_local(&mut f).initialize(
            base.load(schema, &mut f)
                .cast_subtype::<CompactDataStorage>(schema, &mut f),
            &mut f,
        );
        let found = schema.reserve_i32_local(&mut f);
        let slot = schema.reserve_i32_local(&mut f);
        self.emit_compact_slot_probe(&compact, &p.key, slot, found, &mut f)?;
        found.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let value = schema.reserve_value_local(&mut f);
        self.emit_compact_value_at(&compact, slot, &value, &mut f);
        let attributes = self.emit_compact_data_attributes(&compact, &mut f);
        let snapshot = self.emit_alloc_data_property_descriptor_with_word(
            attributes.operand(),
            &value,
            &mut f,
        );
        output.replace(snapshot.load(schema, &mut f).nullable(), &mut f);
        snapshot.clear(&mut f);
        attributes.clear(schema, &mut f);
        value.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(slot, &mut f);
        schema.release_i32_local(found, &mut f);
        compact.clear(&mut f);
        f.instruction(&Instruction::Else);
        let entry = self.emit_ordinary_property_entry(&p.object, &p.key, &mut f)?;
        let descriptor = self.emit_ordinary_entry_descriptor_reference(&entry, &mut f);
        output.replace(descriptor.load(schema, &mut f), &mut f);
        descriptor.clear(&mut f);
        entry.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        output.load(schema, &mut f);
        output.clear(&mut f);
        base.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
