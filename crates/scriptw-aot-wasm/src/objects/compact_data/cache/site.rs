//! Static read and assignment sites in P.
//!
//! Each site keeps its record in a global of the module that compiles it, so
//! the site reaches the record with one global read; R allocates the record
//! on the site's first use and alone fills it. A site whose base may be an
//! actual OrdinaryObject answers inline what is commonest and cheapest to
//! prove from the record's shape: an own data value or a store to an existing
//! writable own slot. Absence and prototype places go to the hit entry.
//! Measured on the retained RSC request, inline proofs of those cases would
//! add more code to every site than their hits save.
use super::*;
use crate::emit::ControlTarget;
use crate::gc_types::SiteRecordGlobal;

/// The key a site's record is made for.
enum SiteKey<'a> {
    /// The site's static key, materialized only when R makes the record.
    Literal(&'a str),
    /// The site's static key, already a String value.
    String(&'a ValueLocals),
}

impl FunctionBuilder<'_> {
    /// The site's record, from its global; the site's first use asks R to
    /// make it for the site's key and keeps it in the global.
    fn emit_site_record(
        &mut self,
        global: SiteRecordGlobal,
        key: SiteKey<'_>,
        f: &mut Function,
    ) -> Result<GcLocal<OwnDataReadCache>, EmitError> {
        let s = self.runtime_schema();
        // A Collect-mode P counts the site's executions here, where every
        // execution of it loads its record.
        if let Some(counter) = self.functions.profile_site_counter(global) {
            counter.emit_increment(f);
        }
        f.instruction(&Instruction::Block(BlockType::Result(ValType::Ref(
            s.reference_type::<OwnDataReadCache>(GcNullability::NonNullable),
        ))));
        let _ = global.load(s, f);
        // Null only until the site's first use.
        f.likely(&Instruction::BrOnNonNull(0));
        let name = s.reserve_gc_local(f).initialize(
            match key {
                SiteKey::Literal(name) => self.emit_interned_string_reference(name, f)?,
                SiteKey::String(value) => value.cast_reference::<StringValue>(s, f),
            },
            f,
        );
        let made = s.helper_reference_on_stack(s.call_helper(
            crate::runtime_helpers::CachedPropertyRecordArguments::new(&name),
            self.runtime_helper_base()?,
            f,
        ));
        name.clear(f);
        global.store(made, f);
        let record = global.load(s, f).require_non_null(f);
        f.instruction(&Instruction::End);
        Ok(s.reserve_gc_local(f).initialize(record, f))
    }

    /// Reads `base.<name>` into `value` through the site's record. Only a
    /// site whose base may be an actual OrdinaryObject answers inline, and a
    /// site the execution profile marks hot first proves its dominant kind
    /// of answer inline. The hit entry's Throw propagates from its own
    /// result before it joins the inline hits, so falling through always
    /// has the property's value.
    pub(crate) fn emit_static_read_site(
        &mut self,
        name: &str,
        may_be_ordinary: bool,
        base: &ValueLocals,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let (global, site) =
            self.next_profiled_site_record(crate::profile::SiteKind::PropertyRead)?;
        // A site whose base may be ordinary already proves an own slot of
        // either compact layout inline, right after the guard: guarding those
        // kinds too would only repeat the proof.
        let specialisation = self.functions.read_specialisation(&site).filter(|kind| {
            !(may_be_ordinary && matches!(kind, crate::profile::ReadSpecialisation::OwnCompact(_)))
        });
        let record = self.emit_site_record(global, SiteKey::Literal(name), f)?;
        let done = (may_be_ordinary || specialisation.is_some())
            .then(|| self.open_frame(ControlFrameKind::Block, f));
        if let (Some(specialisation), Some(done)) = (specialisation, done) {
            self.emit_profiled_read(specialisation, base, &record, value, done, f)?;
        }
        if let (true, Some(done)) = (may_be_ordinary, done) {
            let miss = self.open_frame(ControlFrameKind::Block, f);
            self.emit_own_compact_record_hit(base, &record, value, miss, f);
            self.emit_branch_to_target(done, f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        let result = s.reserve_completion(f);
        s.call_helper(
            crate::runtime_helpers::CachedPropertyReadArguments::new(
                base,
                &record,
                self.current_environment(),
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            f,
        )
        .store(&result, f);
        self.emit_propagate_pending_throw(&result, &[result.value()], f);
        value.copy_from(result.value(), f);
        result.clear(f);
        if done.is_some() {
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        record.clear(f);
        Ok(())
    }

    /// The [[Set]] of PutValue at a static assignment site, its Boolean
    /// into `succeeded`: an existing writable own data property that the
    /// site's record proves is stored inline, everything else is the R
    /// write entry's with the record. `key` is the site's String key,
    /// `target` the base after ToObject and `receiver` the original base.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_static_write_site(
        &mut self,
        global: SiteRecordGlobal,
        target: &ValueLocals,
        receiver: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        succeeded: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let done = self.open_frame(ControlFrameKind::Block, f);
        let stored = self.open_frame(ControlFrameKind::Block, f);
        let record = self.emit_static_write_record_hit(
            global,
            SiteKey::String(key.value()),
            target,
            receiver,
            value,
            stored,
            f,
        )?;
        self.emit_static_write_call(&record, target, receiver, value, succeeded, f)?;
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // The [[Set]] of an existing writable own data property is true.
        succeeded.set_scalar(crate::gc_types::ScalarValue::Boolean(true), f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        record.clear(f);
        Ok(())
    }

    /// A plain assignment `base.name = value` at a static write site, after
    /// its base and value. When the site's record proves that the base is an
    /// object whose existing writable own data property PutValue would only
    /// update, stores it and branches to `done`: ToObject of an object and
    /// ToPropertyKey of the site's String key observe nothing. Otherwise
    /// falls through with the site's record for the complete PutValue.
    pub(crate) fn emit_static_assignment_hit(
        &mut self,
        global: SiteRecordGlobal,
        name: &str,
        base: &ValueLocals,
        value: &ValueLocals,
        done: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<OwnDataReadCache>, EmitError> {
        self.emit_static_write_record_hit(
            global,
            SiteKey::Literal(name),
            base,
            base,
            value,
            done,
            f,
        )
    }

    /// The site's record, after storing and branching to `done` when the
    /// record proves the store.
    #[allow(clippy::too_many_arguments)]
    fn emit_static_write_record_hit(
        &mut self,
        global: SiteRecordGlobal,
        key: SiteKey<'_>,
        target: &ValueLocals,
        receiver: &ValueLocals,
        value: &ValueLocals,
        done: ControlTarget,
        f: &mut Function,
    ) -> Result<GcLocal<OwnDataReadCache>, EmitError> {
        let record = self.emit_site_record(global, key, f)?;
        let miss = self.open_frame(ControlFrameKind::Block, f);
        self.emit_own_compact_record_store(target, receiver, value, &record, miss, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        Ok(record)
    }

    /// The R write entry for a static site's [[Set]], its Boolean into
    /// `succeeded`; the entry's Throw propagates from its own result. The
    /// record names the site's key.
    pub(crate) fn emit_static_write_call(
        &mut self,
        record: &GcLocal<OwnDataReadCache>,
        target: &ValueLocals,
        receiver: &ValueLocals,
        value: &ValueLocals,
        succeeded: &ValueLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let result = s.reserve_completion(f);
        s.call_helper(
            crate::runtime_helpers::CachedPropertyWriteArguments::new(
                target,
                receiver,
                value,
                record,
                self.current_environment(),
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            f,
        )
        .store(&result, f);
        self.emit_propagate_pending_throw(&result, &[result.value()], f);
        succeeded.copy_from(result.value(), f);
        result.clear(f);
        Ok(())
    }

    /// Falls through with `value` when the record proves an own data
    /// property of `base` as the hit entry does: an actual OrdinaryObject
    /// whose current storage is inline- or array-layout compact with the
    /// record's shape, which has the key at the record's nonnegative slot. A
    /// shape is immutable, so that slot holds the property's value whatever
    /// happened since the record was made. Every other record or receiver is
    /// the entry's: this branches to `miss` having observed nothing, written
    /// no part of `value` and retaining no temporary root.
    fn emit_own_compact_record_hit(
        &mut self,
        base: &ValueLocals,
        record: &GcLocal<OwnDataReadCache>,
        value: &ValueLocals,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        // A negative slot proves no own property. A null recorded shape
        // cannot equal either concrete storage's non-null shape.
        let slot = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::SLOT)
            .read(record, s, f)
            .store(slot, f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32LtS);
        self.emit_branch_if_to_target(miss, f);
        self.emit_own_compact_storage_or_branch(
            base,
            miss,
            f,
            |builder, inline, hit, f| {
                let s = builder.runtime_schema();
                s.field(InlineCompactDataStorageSchema::SHAPE)
                    .read(inline, s, f);
                s.field(OwnDataReadCacheSchema::SHAPE).read(record, s, f);
                f.instruction(&Instruction::RefEq);
                f.instruction(&Instruction::If(BlockType::Empty));
                builder.emit_inline_compact_recorded_value_at(inline, slot, value, f);
                hit.set_constant(1, f);
                f.instruction(&Instruction::End);
            },
            |builder, array, hit, f| {
                let s = builder.runtime_schema();
                s.field(ArrayCompactDataStorageSchema::SHAPE)
                    .read(array, s, f);
                s.field(OwnDataReadCacheSchema::SHAPE).read(record, s, f);
                f.instruction(&Instruction::RefEq);
                f.instruction(&Instruction::If(BlockType::Empty));
                builder.emit_array_compact_value_at(array, slot, value, f);
                hit.set_constant(1, f);
                f.instruction(&Instruction::End);
            },
        );
        s.release_i32_local(slot, f);
    }

    /// Runs `inline_answer` or `array_answer` on the compact storage of `base`,
    /// an actual OrdinaryObject, which sets the flag it is given when it
    /// answers. Branches to `miss` for any other value or storage and when
    /// the handler leaves the flag clear, in each case retaining no temporary
    /// root.
    fn emit_own_compact_storage_or_branch(
        &mut self,
        base: &ValueLocals,
        miss: ControlTarget,
        f: &mut Function,
        inline_answer: impl FnOnce(
            &mut Self,
            &GcLocal<InlineCompactDataStorage>,
            I32Local,
            &mut Function,
        ),
        array_answer: impl FnOnce(&mut Self, &GcLocal<ArrayCompactDataStorage>, I32Local, &mut Function),
    ) {
        let s = self.runtime_schema();
        let hit = s.reserve_i32_local(f);
        hit.set_constant(0, f);
        let object =
            base.sealed_reference_or_branch::<OrdinaryObject>(s, self.local_branch_label(miss), f);
        let storage = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read_top(object, f)
                .reference(),
            f,
        );
        let other = self.open_frame(ControlFrameKind::Block, f);
        let not_inline = self.open_frame(ControlFrameKind::Block, f);
        let inline = s.reserve_gc_local(f).initialize(
            storage
                .sealed_nested_subtype_or_branch::<CompactDataStorage, InlineCompactDataStorage>(
                    s,
                    self.local_branch_label(not_inline),
                    f,
                ),
            f,
        );
        inline_answer(self, &inline, hit, f);
        inline.clear(f);
        self.emit_branch_to_target(other, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let array = s.reserve_gc_local(f).initialize(
            storage.sealed_nested_subtype_or_branch::<CompactDataStorage, ArrayCompactDataStorage>(
                s,
                self.local_branch_label(other),
                f,
            ),
            f,
        );
        array_answer(self, &array, hit, f);
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        storage.clear(f);
        hit.load(f);
        f.instruction(&Instruction::I32Eqz);
        s.release_i32_local(hit, f);
        self.emit_branch_if_to_target(miss, f);
    }

    /// Stores `value` and falls through when the record proves that
    /// OrdinarySet(target, key, value, receiver) only replaces the value of
    /// an existing own data property: the receiver is the target, an actual
    /// OrdinaryObject whose current storage is inline- or array-layout compact
    /// with the record's shape, which has the key at the record's nonnegative
    /// slot under a writable attribute word, and (for array storage) already
    /// has every backing the value needs. Every other case branches to `miss`
    /// having written nothing and retaining no temporary root.
    fn emit_own_compact_record_store(
        &mut self,
        target: &ValueLocals,
        receiver: &ValueLocals,
        value: &ValueLocals,
        record: &GcLocal<OwnDataReadCache>,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        // A plain assignment passes its base as both.
        if !std::ptr::eq(target, receiver) {
            target.reference().load(f);
            receiver.reference().load(f);
            f.instruction(&Instruction::RefEq);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(miss, f);
        }
        // A write record has a nonnegative slot only with the shape it was
        // found in.
        let slot = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::SLOT)
            .read(record, s, f)
            .store(slot, f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32LtS);
        self.emit_branch_if_to_target(miss, f);
        let shape = s
            .field(OwnDataReadCacheSchema::SHAPE)
            .read(record, s, f)
            .reference()
            .non_null_or_branch(self.local_branch_label(miss), f);
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read_top(shape, f);
        f.instruction(&Instruction::I32Const(DescriptorMask::WRITABLE.as_i32()));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
        self.emit_own_compact_storage_or_branch(
            target,
            miss,
            f,
            |builder, inline, hit, f| {
                let s = builder.runtime_schema();
                s.field(InlineCompactDataStorageSchema::SHAPE)
                    .read(inline, s, f);
                s.field(OwnDataReadCacheSchema::SHAPE).read(record, s, f);
                f.instruction(&Instruction::RefEq);
                f.instruction(&Instruction::If(BlockType::Empty));
                builder.emit_inline_compact_recorded_store_at(inline, slot, value, f);
                hit.set_constant(1, f);
                f.instruction(&Instruction::End);
            },
            |builder, array, hit, f| {
                let s = builder.runtime_schema();
                // The live shape is the record's, and its one attribute word is
                // writable data.
                s.field(ArrayCompactDataStorageSchema::SHAPE)
                    .read(array, s, f);
                s.field(OwnDataReadCacheSchema::SHAPE).read(record, s, f);
                f.instruction(&Instruction::RefEq);
                f.instruction(&Instruction::If(BlockType::Empty));
                let stored = builder.emit_array_compact_backed_store_i32(array, slot, value, f);
                stored.load(f);
                hit.store(f);
                s.release_i32_local(stored, f);
                f.instruction(&Instruction::End);
            },
        );
        s.release_i32_local(slot, f);
    }
}
