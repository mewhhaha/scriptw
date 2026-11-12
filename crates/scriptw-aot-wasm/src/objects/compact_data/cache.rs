//! A monomorphic cache owns only immutable shape/key metadata and a slot. Every
//! use reloads the receiver's live storage and SoA values after intervening code.
use super::*;
mod hit;
mod holder;
mod write;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_own_data_cache_site(
        &mut self,
        site: I32Local,
        key: &ValueLocals,
        f: &mut Function,
    ) -> GcLocal<OwnDataReadCache> {
        let schema = self.runtime_schema();
        let (table, slot) = self.current_program().own_data_read_caches(site, schema, f);
        let found = schema.reserve_gc_local(f).initialize(
            schema
                .array_type::<OwnDataReadCacheTable>()
                .read(&table, slot, schema, f)
                .reference(),
            f,
        );
        found.load(schema, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let name = schema
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<StringValue>(schema, f), f);
        let cache = schema.reserve_gc_local(f).initialize(
            schema.struct_type::<OwnDataReadCache>().construct(
                (
                    GcOperand::reference(&name, schema),
                    GcOperand::null(schema),
                    GcOperand::i32(0),
                    GcOperand::i32(-1),
                    GcOperand::i32(holder::NO_INTRINSIC_LAYOUT),
                    GcOperand::i32(0),
                    GcOperand::i32(holder::HolderReceiverKind::None.word()),
                    GcOperand::null(schema),
                    GcOperand::i32(holder::NO_INTRINSIC_LAYOUT),
                    GcOperand::null(schema),
                    GcOperand::i32(0),
                    GcOperand::null(schema),
                    GcOperand::null(schema),
                    GcOperand::i32(write::AddHolder::None.word()),
                    GcOperand::null(schema),
                    GcOperand::i32(0),
                ),
                f,
            ),
            f,
        );
        schema.array_type::<OwnDataReadCacheTable>().write(
            &table,
            slot,
            GcOperand::nullable_reference(&cache, schema),
            schema,
            f,
        );
        found.replace(cache.load(schema, f).nullable(), f);
        cache.clear(f);
        name.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let result = schema
            .reserve_gc_local(f)
            .initialize(found.load(schema, f).require_non_null(f), f);
        found.clear(f);
        table.clear(f);
        schema.release_i32_local(slot, f);
        result
    }

    pub(crate) fn compile_cached_property_read_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::CachedPropertyRead);
        let p =
            self.helper_parameters::<crate::runtime_helpers::CachedPropertyReadParameters>(&mut f);
        let schema = self.runtime_schema();
        let result = schema.reserve_completion(&mut f);
        let handled = schema.reserve_i32_local(&mut f);
        let exit = self.open_frame(ControlFrameKind::Block, &mut f);
        // PropertyRead retains its raw receiver, while canonical GetV has
        // already performed ToObject. Keep the raw nullish error on the former.
        self.compile_nullish_tagged_i32(p.target.tag(), &mut f)?;
        self.open_frame(ControlFrameKind::If, &mut f);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::TypeError,
            RuntimeErrorMessage::CANNOT_READ_PROPERTIES_OF_NULL_OR_UNDEFINED,
            &result,
            &mut f,
        )?;
        self.emit_branch_to_target(exit, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(0));
        handled.store(&mut f);
        // Only compiler-owned static string read sites call this kernel. Keep a
        // value-tag guard here so no Symbol/object can ever enter a strong cache.
        p.key.value().tag().load(&mut f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, &mut f);
        // One record per site serves the own compact slot, the own dictionary
        // position and the prototype stage. It holds only the key, shapes
        // and integers; every value is reloaded from live storage.
        let cache = self.emit_own_data_cache_site(p.site, p.key.value(), &mut f);
        let header = self.emit_simple_data_header(&p.target, &mut f);
        header.load(schema, &mut f).is_null(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let object = schema
            .reserve_gc_local(&mut f)
            .initialize(header.load(schema, &mut f).require_non_null(&mut f), &mut f);
        let base = schema.reserve_gc_local(&mut f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, schema, &mut f)
                .reference(),
            &mut f,
        );
        base.is_subtype::<CompactDataStorage>(schema, &mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let storage = schema.reserve_gc_local(&mut f).initialize(
            base.load(schema, &mut f)
                .cast_subtype::<CompactDataStorage>(schema, &mut f),
            &mut f,
        );
        // Native callable shapes have primitive keys/attributes only; cached
        // values are always reloaded from this callable's current own storage.
        self.emit_cached_compact_read_hit(
            &p.target, &p.key, &object, &storage, &cache, &result, handled, &mut f,
        )?;
        handled.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let slot = schema.reserve_i32_local(&mut f);
        self.emit_compact_slot_probe(&storage, &p.key, slot, handled, &mut f)?;
        handled.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let mode = self.emit_negative_cache_mode(&p.target, &object, Some(&p.key), &mut f)?;
        mode.load(&mut f);
        slot.store(&mut f);
        mode.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Eqz);
        handled.store(&mut f);
        schema.release_i32_local(mode, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled.load(&mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        let shape = schema.reserve_gc_local(&mut f).initialize(
            schema
                .field(CompactDataStorageSchema::SHAPE)
                .read(&storage, schema, &mut f)
                .reference(),
            &mut f,
        );
        schema.field(OwnDataReadCacheSchema::SHAPE).write(
            &cache,
            GcOperand::nullable_reference(&shape, schema),
            schema,
            &mut f,
        );
        schema.field(OwnDataReadCacheSchema::SLOT).write(
            &cache,
            GcOperand::i32_local(slot),
            schema,
            &mut f,
        );
        shape.clear(&mut f);
        self.emit_cached_compact_value(&storage, slot, &result, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(slot, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        storage.clear(&mut f);
        f.instruction(&Instruction::Else);
        // A dictionary or intrinsic receiver answers its own data property
        // here. Absence, accessors and exotic receivers keep the general Get.
        let own_value = schema.reserve_value_local(&mut f);
        base.is_subtype::<OrdinaryPropertyStorage>(schema, &mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        // The site remembers where the entry was; the entry's own key object
        // confirms it. Only a miss pays the hash lookup, which also updates
        // the remembered position.
        let dictionary = schema.reserve_gc_local(&mut f).initialize(
            base.load(schema, &mut f)
                .cast_subtype::<OrdinaryPropertyStorage>(schema, &mut f),
            &mut f,
        );
        let position = schema.reserve_i32_local(&mut f);
        schema
            .field(OwnDataReadCacheSchema::DICTIONARY_POSITION)
            .read(&cache, schema, &mut f)
            .store(position, &mut f);
        self.emit_dictionary_position_read(
            &dictionary,
            position,
            &p.key,
            &own_value,
            handled,
            &mut f,
        );
        handled.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let entry = self.emit_ordinary_property_entry(&object, &p.key, &mut f)?;
        entry.load(schema, &mut f).is_null(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        let present = schema
            .reserve_gc_local(&mut f)
            .initialize(entry.load(schema, &mut f).require_non_null(&mut f), &mut f);
        schema
            .field(PropertyEntrySchema::POSITION)
            .read(&present, schema, &mut f)
            .store(position, &mut f);
        present.clear(&mut f);
        schema.field(OwnDataReadCacheSchema::DICTIONARY_POSITION).write(
            &cache,
            GcOperand::i32_local(position),
            schema,
            &mut f,
        );
        self.emit_dictionary_position_read(
            &dictionary,
            position,
            &p.key,
            &own_value,
            handled,
            &mut f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        entry.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(position, &mut f);
        dictionary.clear(&mut f);
        f.instruction(&Instruction::Else);
        base.is_subtype::<IntrinsicPropertyStorage>(schema, &mut f);
        self.open_frame(ControlFrameKind::If, &mut f);
        // The site remembers the layout row's value group; the layout
        // address confirms it. Only a miss searches the layout.
        let intrinsic = schema.reserve_gc_local(&mut f).initialize(
            base.load(schema, &mut f)
                .cast_subtype::<IntrinsicPropertyStorage>(schema, &mut f),
            &mut f,
        );
        self.emit_own_intrinsic_record_read(
            &intrinsic, &p.key, &cache, &own_value, handled, &mut f,
        )?;
        intrinsic.clear(&mut f);
        f.instruction(&Instruction::Else);
        let own =
            self.emit_ordinary_header_own_data_read_i32(&object, &p.key, &own_value, &mut f)?;
        own.load(&mut f);
        handled.store(&mut f);
        schema.release_i32_local(own, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled.load(&mut f);
        f.instruction(&Instruction::If(BlockType::Empty));
        result.set_normal(&own_value, &mut f);
        f.instruction(&Instruction::End);
        own_value.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        base.clear(&mut f);
        object.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        header.clear(&mut f);
        handled.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        self.emit_prototype_record_read(&p.target, &p.key, &cache, &result, handled, &mut f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        cache.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        self.emit_dynamic_property_read_with_key_locals(
            &p.target,
            &p.receiver,
            &p.key,
            &result,
            &mut f,
        )?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        result.emit(&mut f);
        result.clear(&mut f);
        schema.release_i32_local(handled, &mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}
