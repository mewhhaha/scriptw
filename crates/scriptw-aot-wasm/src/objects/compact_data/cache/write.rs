//! A static assignment site's write record: an existing own data property
//! of a compact shape, and the add of the key by that shape's transition.
//!
//! The record shares the site's own-data entry and, like a read record,
//! holds only the key, shapes and integers. Nothing invalidates it from
//! outside. A use proves again that the receiver is the target and an actual
//! OrdinaryObject on compact storage whose shape is the recorded one. An add
//! further proves that the object is extensible and that its prototype
//! chain still lacks the key: the receiver's prototype is the recorded
//! closed tail, or one ordinary holder that lacks the key and whose own
//! chain is that tail. Every other case runs OrdinarySet unchanged; after a
//! successful one on a compact receiver the record is refreshed from facts
//! that the immutable shapes and layouts alone prove.
use super::*;

/// How an add record proves that the receiver's [[Prototype]] lacks the key
/// before the closed tail recorded in ADD_TAIL.
#[derive(Clone, Copy)]
pub(super) enum AddHolder {
    /// The prototype is the closed tail itself.
    None,
    /// An actual OrdinaryObject on compact storage with the recorded shape,
    /// which lacks the key.
    Compact,
    /// An actual OrdinaryObject on dictionary storage in which a live lookup
    /// of the key misses.
    Dictionary,
}

impl AddHolder {
    pub(super) const fn word(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Compact => 1,
            Self::Dictionary => 2,
        }
    }
}

/// How a use proves that a dictionary holder lacks the key.
#[derive(Clone, Copy)]
pub(super) enum DictionaryAbsence {
    /// The holder's key index, then a complete lookup when the index
    /// cannot tell.
    Lookup,
    /// The holder's key index alone, which makes no call; when it cannot
    /// tell, the absence stays unproven.
    IndexOnly,
}

impl FunctionBuilder<'_> {
    /// The complete static assignment after the hit entry declined: the
    /// record's add, OrdinarySet and the record refresh.
    pub(crate) fn compile_cached_property_write_miss_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::CachedPropertyWriteMiss);
        let p = self
            .helper_parameters::<crate::runtime_helpers::CachedPropertyWriteMissParameters>(
                &mut body,
            );
        let f = &mut body;
        let s = self.runtime_schema();
        let result = s.reserve_completion(f);
        // Only compiler-owned static String sites call this kernel. Keep a
        // value-tag guard so no Symbol or object can ever enter a record.
        p.key.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let cache = self.emit_own_data_cache_site(p.site, p.key.value(), f);
        let handled = s.reserve_i32_local(f);
        let previous = s
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(s, f);
        self.emit_recorded_write(
            &p.target,
            &p.receiver,
            &p.key,
            &p.value,
            &cache,
            handled,
            &previous,
            f,
        )?;
        handled.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_object_boolean_result(handled, &result, f);
        f.instruction(&Instruction::Else);
        self.emit_cached_write_general_set(&p, &result, f)?;
        // Only a normal, successful Set on a receiver that was compact
        // refreshes the record.
        previous.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        result.kind().load(f);
        f.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        result.value().scalar().load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let parent = s
            .reserve_gc_local(f)
            .initialize(previous.load(s, f).require_non_null(f), f);
        self.emit_write_record_refresh(&p.target, &p.key, &cache, &parent, f)?;
        parent.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        previous.clear(f);
        s.release_i32_local(handled, f);
        cache.clear(f);
        f.instruction(&Instruction::Else);
        self.emit_cached_write_general_set(&p, &result, f)?;
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        result.emit(f);
        result.clear(f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }

    /// The complete OrdinarySet with the site's original operands.
    fn emit_cached_write_general_set(
        &mut self,
        p: &crate::runtime_helpers::CachedPropertyWriteMissParameters,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.runtime_schema()
            .call_helper(
                crate::runtime_helpers::OrdinarySetArguments::new(
                    &p.target,
                    &p.receiver,
                    &p.key,
                    &p.value,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                f,
            )
            .store(result, f);
        Ok(())
    }

    /// Performs the write and sets `handled` when the record applies to the
    /// live receiver. Otherwise, for a compact receiver that is the target,
    /// leaves its current shape in `previous` for the refresh.
    #[allow(clippy::too_many_arguments)]
    fn emit_recorded_write(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        cache: &GcLocal<OwnDataReadCache>,
        handled: I32Local,
        previous: &GcLocal<CompactDataShape, Nullable>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        handled.set_constant(0, f);
        // OrdinarySet(O, P, V, O): an actual OrdinaryObject answers
        // [[GetOwnProperty]], [[GetPrototypeOf]] and [[DefineOwnProperty]]
        // ordinarily from its header.
        target.reference().load(f);
        receiver.reference().load(f);
        f.instruction(&Instruction::RefEq);
        target.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<OrdinaryObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let object = s
            .reserve_gc_local(f)
            .initialize(target.cast_reference::<OrdinaryObject>(s, f), f);
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        base.is_subtype::<CompactDataStorage>(s, f);
        self.open_frame(ControlFrameKind::If, f);
        let storage = s
            .reserve_gc_local(f)
            .initialize(base.load(s, f).cast_subtype::<CompactDataStorage>(s, f), f);
        let shape = s.reserve_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&storage, s, f)
                .reference(),
            f,
        );
        s.field(OwnDataReadCacheSchema::KEY).read(cache, s, f);
        key.value().reference().load(f);
        f.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, f);
        // Existing own data: the shape has the key at the recorded slot and
        // its one attribute word is writable data, so OrdinarySet performs
        // [[DefineOwnProperty]] with only [[Value]] on O itself.
        let _ = shape.load(s, f);
        s.field(OwnDataReadCacheSchema::SHAPE).read(cache, s, f);
        f.instruction(&Instruction::RefEq);
        self.open_frame(ControlFrameKind::If, f);
        let slot = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::SLOT)
            .read(cache, s, f)
            .store(slot, f);
        self.emit_compact_store_at(&storage, slot, value, f);
        s.release_i32_local(slot, f);
        handled.set_constant(1, f);
        f.instruction(&Instruction::Else);
        // Add: the parent shape lacks the key, so O has no own property, and
        // the chain lacks it, so no setter, read-only property or Proxy
        // intervenes. CreateDataProperty then defines a default data property
        // on the extensible O, which is exactly the recorded canonical child.
        let _ = shape.load(s, f);
        s.field(OwnDataReadCacheSchema::ADD_PARENT)
            .read(cache, s, f);
        f.instruction(&Instruction::RefEq);
        s.field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(&object, s, f);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let tail = self.emit_recorded_add_chain_tail(
            target,
            &object,
            key,
            cache,
            DictionaryAbsence::Lookup,
            f,
        )?;
        tail.load(f);
        s.field(OwnDataReadCacheSchema::ADD_TAIL).read(cache, s, f);
        f.instruction(&Instruction::I32Eq);
        tail.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        s.release_i32_local(tail, f);
        self.open_frame(ControlFrameKind::If, f);
        let recorded = s
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize(
                s.field(OwnDataReadCacheSchema::ADD_CHILD)
                    .read(cache, s, f)
                    .reference(),
                f,
            );
        let child = s
            .reserve_gc_local(f)
            .initialize(recorded.load(s, f).require_non_null(f), f);
        recorded.clear(f);
        let keys = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(&shape, s, f)
                .reference(),
            f,
        );
        let count = s.reserve_i32_local(f);
        s.array_type::<CompactShapeKeys>().length(&keys, s, f);
        count.store(f);
        keys.clear(f);
        self.emit_compact_storage_extend(&object, &storage, &child, count, value, f);
        s.release_i32_local(count, f);
        child.clear(f);
        handled.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        previous.replace(shape.load(s, f).nullable(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        shape.clear(f);
        storage.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        base.clear(f);
        object.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// The live closed-tail word that ends the receiver's chain by the
    /// record's holder rule, or 0 when the live chain does not follow it.
    pub(super) fn emit_recorded_add_chain_tail(
        &mut self,
        target: &ValueLocals,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        cache: &GcLocal<OwnDataReadCache>,
        absence: DictionaryAbsence,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let tail = s.reserve_i32_local(f);
        let rule = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::ADD_HOLDER)
            .read(cache, s, f)
            .store(rule, f);
        rule.load(f);
        f.instruction(&Instruction::I32Const(AddHolder::None.word()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let mode = self.emit_negative_cache_mode(target, object, None, f)?;
        mode.load(f);
        tail.store(f);
        s.release_i32_local(mode, f);
        f.instruction(&Instruction::Else);
        tail.set_constant(0, f);
        let holder = self.emit_ordinary_prototype(object, f);
        holder.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let present = s
            .reserve_gc_local(f)
            .initialize(holder.load(s, f).require_non_null(f), f);
        let lacks = s.reserve_i32_local(f);
        lacks.set_constant(0, f);
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&present, s, f)
                .reference(),
            f,
        );
        rule.load(f);
        f.instruction(&Instruction::I32Const(AddHolder::Compact.word()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        // An immutable shape lacks the key for as long as the holder has it.
        // Its shape is read through the sealed layouts: a cast to the open
        // compact base would keep a host subtype query in this path.
        let shape = self.emit_compact_storage_shape(&base, f);
        shape.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let _ = shape.load(s, f);
        s.field(OwnDataReadCacheSchema::ADD_HOLDER_SHAPE)
            .read(cache, s, f);
        f.instruction(&Instruction::RefEq);
        lacks.store(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        shape.clear(f);
        f.instruction(&Instruction::Else);
        // A dictionary holder answers by a live lookup of its own entries.
        base.is_subtype::<OrdinaryPropertyStorage>(s, f);
        self.open_frame(ControlFrameKind::If, f);
        let absent = self.emit_dictionary_holder_lacks_key_i32(&present, &base, key, absence, f)?;
        absent.load(f);
        lacks.store(f);
        s.release_i32_local(absent, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        base.clear(f);
        lacks.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let holder_value = s.reserve_value_local(f);
        holder_value.set_reference(&present, s, f);
        let mode = self.emit_negative_cache_mode(&holder_value, &present, None, f)?;
        holder_value.clear(f);
        mode.load(f);
        tail.store(f);
        s.release_i32_local(mode, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(lacks, f);
        present.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        holder.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(rule, f);
        Ok(tail)
    }

    /// Whether `holder`, whose storage `base` is a dictionary, has no own
    /// entry for `key`. An empty index bucket proves it without the call;
    /// otherwise the complete lookup decides, unless `absence` declines it.
    fn emit_dictionary_holder_lacks_key_i32(
        &mut self,
        holder: &GcLocal<OrdinaryObject>,
        base: &GcLocal<ObjectPropertyStorage>,
        key: &PropertyKeyLocals,
        absence: DictionaryAbsence,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let absent = s.reserve_i32_local(f);
        let dictionary = s.reserve_gc_local(f).initialize(
            base.load(s, f)
                .cast_subtype::<OrdinaryPropertyStorage>(s, f),
            f,
        );
        self.emit_ordinary_property_index_lacks_key_i32(&dictionary, key, f);
        absent.store(f);
        dictionary.clear(f);
        match absence {
            DictionaryAbsence::IndexOnly => {}
            DictionaryAbsence::Lookup => {
                absent.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                let entry = self.emit_ordinary_property_entry(holder, key, f)?;
                entry.load(s, f).is_null(f);
                absent.store(f);
                entry.clear(f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
            }
        }
        Ok(absent)
    }

    /// The receiver's [[Prototype]] when it is an actual OrdinaryObject;
    /// null for a null prototype and for every other kind of object.
    fn emit_ordinary_prototype(
        &self,
        object: &GcLocal<OrdinaryObject>,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject, Nullable> {
        let s = self.runtime_schema();
        let holder = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let prototype = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(object, s, f)
                .reference(),
            f,
        );
        prototype.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        prototype.is_subtype::<OrdinaryObject>(s, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        holder.replace(
            prototype
                .load(s, f)
                .cast_subtype::<OrdinaryObject>(s, f)
                .nullable(),
            f,
        );
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        prototype.clear(f);
        holder
    }

    /// Records, after a successful general Set, what immutable shapes and
    /// layouts prove about the key: the slot of a writable own data property
    /// in the object's current shape, and the canonical child of `parent`,
    /// the shape the object had before, when `parent` lacks the key, holds
    /// default data attributes and the chain beyond it lacks the key.
    fn emit_write_record_refresh(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        cache: &GcLocal<OwnDataReadCache>,
        parent: &GcLocal<CompactDataShape>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // The write front leaves `parent` only for an actual OrdinaryObject
        // target whose record key is this key.
        let object = s
            .reserve_gc_local(f)
            .initialize(target.cast_reference::<OrdinaryObject>(s, f), f);
        let name = s
            .reserve_gc_local(f)
            .initialize(key.value().cast_reference::<StringValue>(s, f), f);
        let hash = s.reserve_i64_local(f);
        self.emit_string_key_hash(&name, hash, f);
        let slot = s.reserve_i32_local(f);
        let found = s.reserve_i32_local(f);
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        base.is_subtype::<CompactDataStorage>(s, f);
        self.open_frame(ControlFrameKind::If, f);
        let storage = s
            .reserve_gc_local(f)
            .initialize(base.load(s, f).cast_subtype::<CompactDataStorage>(s, f), f);
        let current = s.reserve_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&storage, s, f)
                .reference(),
            f,
        );
        storage.clear(f);
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&current, s, f);
        f.instruction(&Instruction::I32Const(DescriptorMask::WRITABLE.as_i32()));
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_compact_shape_probe_prepared(&current, &name, hash, slot, found, f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        s.field(OwnDataReadCacheSchema::SHAPE).write(
            cache,
            GcOperand::nullable_reference(&current, s),
            s,
            f,
        );
        s.field(OwnDataReadCacheSchema::SLOT)
            .write(cache, GcOperand::i32_local(slot), s, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        current.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        base.clear(f);

        let rule = s.reserve_i32_local(f);
        rule.set_constant(AddHolder::None.word(), f);
        let holder_shape = s
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(s, f);
        let tail = s.reserve_i32_local(f);
        tail.set_constant(0, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(parent, s, f);
        f.instruction(&Instruction::I32Const(
            super::super::transition::default_data_attributes().as_i32(),
        ));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(done, f);
        self.emit_compact_shape_probe_prepared(parent, &name, hash, slot, found, f);
        found.load(f);
        self.emit_branch_if_to_target(done, f);
        let keys = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(parent, s, f)
                .reference(),
            f,
        );
        let count = s.reserve_i32_local(f);
        s.array_type::<CompactShapeKeys>().length(&keys, s, f);
        count.store(f);
        keys.clear(f);
        let child = self.emit_compact_transition_lookup(parent, count, &name, hash, f);
        s.release_i32_local(count, f);
        child.load(s, f).is_null(f);
        self.emit_branch_if_to_target(done, f);
        let mode = self.emit_negative_cache_mode(target, &object, Some(key), f)?;
        mode.load(f);
        tail.store(f);
        s.release_i32_local(mode, f);
        tail.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        // One ordinary holder between the receiver and a closed tail.
        let holder = self.emit_ordinary_prototype(&object, f);
        holder.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let present = s
            .reserve_gc_local(f)
            .initialize(holder.load(s, f).require_non_null(f), f);
        let holder_base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&present, s, f)
                .reference(),
            f,
        );
        holder_base.is_subtype::<CompactDataStorage>(s, f);
        self.open_frame(ControlFrameKind::If, f);
        let compact = s.reserve_gc_local(f).initialize(
            holder_base
                .load(s, f)
                .cast_subtype::<CompactDataStorage>(s, f),
            f,
        );
        let shape = s.reserve_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&compact, s, f)
                .reference(),
            f,
        );
        compact.clear(f);
        self.emit_compact_shape_probe_prepared(&shape, &name, hash, slot, found, f);
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        rule.set_constant(AddHolder::Compact.word(), f);
        holder_shape.replace(shape.load(s, f).nullable(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        shape.clear(f);
        f.instruction(&Instruction::Else);
        holder_base.is_subtype::<OrdinaryPropertyStorage>(s, f);
        self.open_frame(ControlFrameKind::If, f);
        let absent = self.emit_dictionary_holder_lacks_key_i32(
            &present,
            &holder_base,
            key,
            DictionaryAbsence::Lookup,
            f,
        )?;
        absent.load(f);
        self.open_frame(ControlFrameKind::If, f);
        rule.set_constant(AddHolder::Dictionary.word(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(absent, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        holder_base.clear(f);
        rule.load(f);
        f.instruction(&Instruction::I32Const(AddHolder::None.word()));
        f.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, f);
        let holder_value = s.reserve_value_local(f);
        holder_value.set_reference(&present, s, f);
        let mode = self.emit_negative_cache_mode(&holder_value, &present, Some(key), f)?;
        holder_value.clear(f);
        mode.load(f);
        tail.store(f);
        s.release_i32_local(mode, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        present.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        holder.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        tail.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, f);
        s.field(OwnDataReadCacheSchema::ADD_PARENT).write(
            cache,
            GcOperand::nullable_reference(parent, s),
            s,
            f,
        );
        s.field(OwnDataReadCacheSchema::ADD_CHILD).write(
            cache,
            GcOperand::reference(&child, s),
            s,
            f,
        );
        s.field(OwnDataReadCacheSchema::ADD_HOLDER)
            .write(cache, GcOperand::i32_local(rule), s, f);
        s.field(OwnDataReadCacheSchema::ADD_HOLDER_SHAPE).write(
            cache,
            GcOperand::reference(&holder_shape, s),
            s,
            f,
        );
        s.field(OwnDataReadCacheSchema::ADD_TAIL)
            .write(cache, GcOperand::i32_local(tail), s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // These clears also run after a guard branch skipped initialization.
        child.clear(f);
        s.release_i32_local(tail, f);
        holder_shape.clear(f);
        s.release_i32_local(rule, f);
        s.release_i32_local(found, f);
        s.release_i32_local(slot, f);
        s.release_i64_local(hash, f);
        name.clear(f);
        object.clear(f);
        Ok(())
    }
}
