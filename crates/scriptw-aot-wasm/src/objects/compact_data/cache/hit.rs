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

    /// A missing compact own key can be cached only across these two closed
    /// prototype tails. No object/Realm edge is retained in cache metadata.
    /// Registration additionally proves absence in the sealed intrinsic rows;
    /// hits need only revalidate that exact immutable layout and null parent.
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
