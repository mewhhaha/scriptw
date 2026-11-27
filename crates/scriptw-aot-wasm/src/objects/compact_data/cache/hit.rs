//! R owns compact cache identity, current-value loads and closed negative tails.
use super::*;

// Nonnegative slots are own data; null SHAPE is the uninitialized state.
enum NegativeCacheMode {
    NullPrototype,
    ObjectPrototype,
}

impl NegativeCacheMode {
    fn word(self) -> i32 {
        match self {
            Self::NullPrototype => -1,
            Self::ObjectPrototype => -2,
        }
    }
}

impl FunctionBuilder<'_> {
    /// Leaves on the stack whether `mode` is the unmodified-%Object.prototype%
    /// tail: a prototype with that immutable layout, an immutable null
    /// [[Prototype]] of its own and, when registered with a key, no such row.
    pub(crate) fn emit_is_object_prototype_tail_i32(&self, mode: I32Local, f: &mut Function) {
        mode.load(f);
        f.instruction(&Instruction::I32Const(
            NegativeCacheMode::ObjectPrototype.word(),
        ));
        f.instruction(&Instruction::I32Eq);
    }

    pub(super) fn emit_cached_compact_read_hit(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        object: &GcLocal<OrdinaryObject>,
        storage: &GcLocal<CompactDataStorage>,
        cache: &GcLocal<OwnDataReadCache>,
        result: &CompletionLocals,
        handled: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        self.emit_cached_compact_identity(key, storage, cache, f);
        self.open_frame(ControlFrameKind::If, f);
        let slot = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::SLOT)
            .read(cache, s, f)
            .store(slot, f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        handled.set_constant(1, f);
        f.instruction(&Instruction::Else);
        let mode = self.emit_negative_cache_mode(target, object, None, f)?;
        mode.load(f);
        slot.load(f);
        f.instruction(&Instruction::I32Eq);
        mode.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32And);
        handled.store(f);
        s.release_i32_local(mode, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        handled.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_cached_compact_value(storage, slot, result, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(slot, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(())
    }

    /// Leave the canonical primitive-key/live-shape identity predicate on stack.
    fn emit_cached_compact_identity(
        &self,
        key: &PropertyKeyLocals,
        storage: &GcLocal<CompactDataStorage>,
        cache: &GcLocal<OwnDataReadCache>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.field(OwnDataReadCacheSchema::KEY).read(cache, s, f);
        key.value().reference().load(f);
        f.instruction(&Instruction::RefEq);
        s.field(CompactDataStorageSchema::SHAPE).read(storage, s, f);
        s.field(OwnDataReadCacheSchema::SHAPE).read(cache, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
    }

    pub(super) fn emit_cached_compact_value(
        &mut self,
        storage: &GcLocal<CompactDataStorage>,
        slot: I32Local,
        result: &CompletionLocals,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_compact_value_at(storage, slot, &value, f);
        f.instruction(&Instruction::Else);
        value.set_undefined(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        result.set_normal(&value, f);
        value.clear(f);
    }

    /// Branches to `fail` unless the chain above `object` is the closed
    /// tail a record registered as `mode`: a null [[Prototype]], or an
    /// actual ordinary [[Prototype]] with the immutable %Object.prototype%
    /// layout and count, whose own [[Prototype]] is null and immutable. This
    /// is `emit_negative_cache_mode`'s proof for a header the caller has
    /// already admitted, decided against the record's word instead of
    /// computed: the registration proved that the layout's rows lack the
    /// key, and a use re-proves that the live prototype still has exactly
    /// those rows. Any other word, the unvisited one included, fails.
    /// The current Realm's %Object.prototype% still holding its recorded
    /// rows is that tail by its protector (`realm_protectors.rs`), which
    /// replaces the header, parent and layout tests with two identities.
    pub(super) fn emit_recorded_closed_tail_or_branch(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        mode: I32Local,
        fail: crate::emit::ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let prototype = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(object, s, f)
                .reference(),
            f,
        );
        prototype.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        mode.load(f);
        f.instruction(&Instruction::I32Const(
            NegativeCacheMode::NullPrototype.word(),
        ));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(fail, f);
        f.instruction(&Instruction::Else);
        mode.load(f);
        f.instruction(&Instruction::I32Const(
            NegativeCacheMode::ObjectPrototype.word(),
        ));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(fail, f);
        if self.strings.has_intrinsic_property_layout(
            crate::intrinsics::IntrinsicPropertyLayoutId::ObjectPrototype,
        )? {
            let span = self.strings.intrinsic_property_layout_span(
                crate::intrinsics::IntrinsicPropertyLayoutId::ObjectPrototype,
            )?;
            // The current Realm's protected %Object.prototype% is this
            // tail; any other prototype takes the complete proof below.
            let proved = self.open_frame(ControlFrameKind::Block, f);
            let general = self.open_frame(ControlFrameKind::Block, f);
            self.emit_unchanged_object_prototype_or_branch(&prototype, general, f);
            self.emit_branch_to_target(proved, f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            let parent = s.reserve_gc_local(f).initialize(
                prototype.sealed_subtype_or_branch::<OrdinaryObject>(
                    s,
                    self.local_branch_label(fail),
                    f,
                ),
                f,
            );
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&parent, s, f)
                .reference()
                .is_null(f);
            s.field(OrdinaryObjectSchema::IMMUTABLE_PROTOTYPE)
                .read(&parent, s, f);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(fail, f);
            let base = s.reserve_gc_local(f).initialize(
                s.field(OrdinaryObjectSchema::PROPERTIES)
                    .read(&parent, s, f)
                    .reference(),
                f,
            );
            parent.clear(f);
            // A closed tail requires immutable rows. Global backing can
            // acquire an own property while retaining its storage identity.
            let storage = s.reserve_gc_local(f).initialize(
                base.sealed_subtype_or_branch::<IntrinsicPropertyStorage>(
                    s,
                    self.local_branch_label(fail),
                    f,
                ),
                f,
            );
            base.clear(f);
            for (field, expected) in [
                (IntrinsicPropertyStorageSchema::ADDRESS, span.address()),
                (IntrinsicPropertyStorageSchema::COUNT, span.count()),
            ] {
                s.field(field).read(&storage, s, f);
                f.instruction(&Instruction::I32Const(expected as i32));
                f.instruction(&Instruction::I32Ne);
                self.emit_branch_if_to_target(fail, f);
            }
            storage.clear(f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        } else {
            // Without the layout no record registers this tail.
            self.emit_branch_to_target(fail, f);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        prototype.clear(f);
        Ok(())
    }

    /// A missing compact own key can be cached only across these two closed
    /// prototype tails. No object/Realm edge is retained in cache metadata.
    /// Registration additionally proves absence in the sealed intrinsic rows;
    /// hits need only revalidate that exact immutable layout and null parent,
    /// which the current Realm's %Object.prototype% protector proves alone.
    pub(crate) fn emit_negative_cache_mode(
        &mut self,
        target: &ValueLocals,
        object: &GcLocal<OrdinaryObject>,
        registration_key: Option<&PropertyKeyLocals>,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let mode = s.reserve_i32_local(f);
        mode.set_constant(0, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        target.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<OrdinaryObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        let prototype = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(object, s, f)
                .reference(),
            f,
        );
        prototype.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        mode.set_constant(NegativeCacheMode::NullPrototype.word(), f);
        f.instruction(&Instruction::Else);
        let cleanup = if self.strings.has_intrinsic_property_layout(
            crate::intrinsics::IntrinsicPropertyLayoutId::ObjectPrototype,
        )? {
            let span = self.strings.intrinsic_property_layout_span(
                crate::intrinsics::IntrinsicPropertyLayoutId::ObjectPrototype,
            )?;
            if registration_key.is_none() {
                // The current Realm's protected %Object.prototype% is this
                // tail. A registration still searches the rows for its key
                // below.
                let general = self.open_frame(ControlFrameKind::Block, f);
                self.emit_unchanged_object_prototype_or_branch(&prototype, general, f);
                mode.set_constant(NegativeCacheMode::ObjectPrototype.word(), f);
                self.emit_branch_to_target(exit, f);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
            }
            prototype.is_subtype::<OrdinaryObject>(s, f);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(exit, f);
            let parent = s
                .reserve_gc_local(f)
                .initialize(prototype.load(s, f).cast_subtype::<OrdinaryObject>(s, f), f);
            let tail = s.reserve_gc_local(f).initialize(
                s.field(OrdinaryObjectSchema::PROTOTYPE)
                    .read(&parent, s, f)
                    .reference(),
                f,
            );
            tail.load(s, f).is_null(f);
            let word = s.reserve_i32_local(f);
            s.field(OrdinaryObjectSchema::IMMUTABLE_PROTOTYPE)
                .read(&parent, s, f)
                .store(word, f);
            word.load(f);
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(exit, f);
            let base = s.reserve_gc_local(f).initialize(
                s.field(OrdinaryObjectSchema::PROPERTIES)
                    .read(&parent, s, f)
                    .reference(),
                f,
            );
            base.is_subtype::<IntrinsicPropertyStorage>(s, f);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(exit, f);
            let storage = s.reserve_gc_local(f).initialize(
                base.load(s, f)
                    .cast_subtype::<IntrinsicPropertyStorage>(s, f),
                f,
            );
            for (field, expected) in [
                (IntrinsicPropertyStorageSchema::ADDRESS, span.address()),
                (IntrinsicPropertyStorageSchema::COUNT, span.count()),
            ] {
                s.field(field).read(&storage, s, f).store(word, f);
                word.load(f);
                f.instruction(&Instruction::I32Const(expected as i32));
                f.instruction(&Instruction::I32Ne);
                self.emit_branch_if_to_target(exit, f);
            }
            if let Some(key) = registration_key {
                // The layout's index usually proves absence without the
                // complete search, which decides only what it cannot.
                let lacks = self.emit_intrinsic_layout_lacks_key_i32(&storage, key, f);
                lacks.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, f);
                let index = s.reserve_i32_local(f);
                s.call_helper(
                    crate::runtime_helpers::IntrinsicPropertyFindArguments::new(&storage, key),
                    self.runtime_helper_base()?,
                    f,
                )
                .store(index, f);
                index.load(f);
                f.instruction(&Instruction::I32Const(0));
                f.instruction(&Instruction::I32GeS);
                self.emit_branch_if_to_target(exit, f);
                s.release_i32_local(index, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                s.release_i32_local(lacks, f);
            }
            mode.set_constant(NegativeCacheMode::ObjectPrototype.word(), f);
            s.release_i32_local(word, f);
            Some((storage, base, tail, parent))
        } else {
            None
        };
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // These clears also run after a guard branch skipped initialization.
        if let Some((storage, base, tail, parent)) = cleanup {
            storage.clear(f);
            base.clear(f);
            tail.clear(f);
            parent.clear(f);
        }
        prototype.clear(f);
        Ok(mode)
    }
}
