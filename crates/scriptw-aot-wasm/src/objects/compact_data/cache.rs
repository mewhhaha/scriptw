//! A site record owns only immutable shape/key metadata, slots and positions.
//! Every use reloads the receiver's live storage and SoA values after
//! intervening code.
//!
//! A static read site that meets several receivers keeps a bounded chain of
//! records from the one in its global. The miss kernel fills one record of
//! the chain a visit: the first that proves nothing yet, a new last record
//! while the chain is short, and otherwise the last record again. Each
//! record's proofs stand alone, so the hit entry may answer from any of them,
//! and no record depends on its place in the chain.
use super::*;
mod entry;
mod hit;
mod holder;
mod site;
mod write;

/// The slot of a static site's record that the read miss kernel has not
/// visited yet. The hit entry sends such a record to the miss kernel, which
/// registers what it can on a site's first use; only a visited record lets
/// the entry answer through the call-free walk what the record does not.
/// Every other reader treats it as a negative slot that proves nothing.
const UNVISITED_SLOT: i32 = i32::MIN;

/// The most records a static read site chains. A site that meets more
/// receivers than these can prove answers the rest by probing the live
/// shape and through the call-free walk, as a megamorphic site does.
const MAX_SITE_RECORDS: i32 = 8;

/// The visits a site's miss kernel may spend without recording a new proof
/// before the hit entry stops asking it to learn an unrecorded receiver and
/// walks instead. A receiver the kernel cannot record (an accessor, a deep
/// chain, an exotic object) then costs the walk, not the kernel.
const SITE_LEARNING_BUDGET: i32 = 8;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_own_data_cache_site(
        &mut self,
        site: I32Local,
        key: &ValueLocals,
        f: &mut Function,
    ) -> GcLocal<OwnDataReadCache> {
        let schema = self.runtime_schema();
        let table = self
            .current_program()
            .own_data_records_covering(site, schema, f);
        let found = schema.reserve_gc_local(f).initialize(
            schema
                .array_type::<OwnDataReadCacheTable>()
                .read(&table, site, schema, f)
                .reference(),
            f,
        );
        found.load(schema, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let name = schema
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<StringValue>(schema, f), f);
        let cache = self.emit_new_site_record(&name, 0, f);
        schema.array_type::<OwnDataReadCacheTable>().write(
            &table,
            site,
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
        result
    }

    /// A fresh record for a site whose key is `name`, which proves nothing
    /// until the miss kernel fills it.
    fn emit_new_site_record(
        &self,
        name: &GcLocal<StringValue>,
        slot: i32,
        f: &mut Function,
    ) -> GcLocal<OwnDataReadCache> {
        let schema = self.runtime_schema();
        schema.reserve_gc_local(f).initialize(
            schema.struct_type::<OwnDataReadCache>().construct(
                (
                    GcOperand::reference(name, schema),
                    GcOperand::null(schema),
                    GcOperand::i32(slot),
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
                    GcOperand::null(schema),
                    GcOperand::i32(entry::ReceiverLayout::Ordinary.word()),
                    GcOperand::i32(SITE_LEARNING_BUDGET),
                ),
                f,
            ),
            f,
        )
    }

    /// Leaves on the stack whether a record proves nothing: no own compact
    /// shape, dictionary position or intrinsic row, and no receiver rule.
    fn emit_site_record_is_blank_i32<N: GcFieldNullability>(
        &self,
        record: &GcLocal<OwnDataReadCache, N>,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        schema
            .field(OwnDataReadCacheSchema::SHAPE)
            .read(record, schema, f)
            .reference()
            .is_null(f);
        schema
            .field(OwnDataReadCacheSchema::RECEIVER_KIND)
            .read(record, schema, f);
        f.instruction(&Instruction::I32Const(
            holder::HolderReceiverKind::None.word(),
        ));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        schema
            .field(OwnDataReadCacheSchema::DICTIONARY_POSITION)
            .read(record, schema, f);
        f.instruction(&Instruction::I32Const(-1));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
        schema
            .field(OwnDataReadCacheSchema::LAYOUT)
            .read(record, schema, f);
        f.instruction(&Instruction::I32Const(holder::NO_INTRINSIC_LAYOUT));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::I32And);
    }

    /// The record of a read site's chain that this visit of the miss kernel
    /// fills: the first that proves nothing, else a new last record made by
    /// R's record helper while the chain is short, else the last record,
    /// whose proofs this visit may replace. `fresh` is set unless the chain
    /// is full. Every record's proofs stand alone, so which record holds a
    /// proof never matters to its validity.
    fn emit_select_site_record(
        &mut self,
        head: &GcLocal<OwnDataReadCache>,
        name: &GcLocal<StringValue>,
        fresh: I32Local,
        f: &mut Function,
    ) -> Result<GcLocal<OwnDataReadCache>, EmitError> {
        let schema = self.runtime_schema();
        let selected = schema
            .reserve_gc_local::<OwnDataReadCache, Nullable>(f)
            .initialize(head.load(schema, f).nullable(), f);
        let following = schema
            .reserve_gc_local::<OwnDataReadCache, Nullable>(f)
            .initialize_null(schema, f);
        let count = schema.reserve_i32_local(f);
        count.set_constant(1, f);
        fresh.set_constant(1, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        self.emit_site_record_is_blank_i32(&selected, f);
        self.emit_branch_if_to_target(done, f);
        following.replace(
            schema
                .field(OwnDataReadCacheSchema::NEXT)
                .read(&selected, schema, f)
                .reference(),
            f,
        );
        following.load(schema, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        count.load(f);
        f.instruction(&Instruction::I32Const(MAX_SITE_RECORDS));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        fresh.set_constant(0, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let made = schema
            .reserve_gc_local::<OwnDataReadCache, NonNullable>(f)
            .initialize(
                schema.helper_reference_on_stack(schema.call_helper(
                    crate::runtime_helpers::CachedPropertyRecordArguments::new(name),
                    self.runtime_helper_base()?,
                    f,
                )),
                f,
            );
        schema
            .field(OwnDataReadCacheSchema::SLOT)
            .write(&made, GcOperand::i32(0), schema, f);
        schema.field(OwnDataReadCacheSchema::NEXT).write(
            &selected,
            GcOperand::nullable_reference(&made, schema),
            schema,
            f,
        );
        selected.replace(made.load(schema, f).nullable(), f);
        made.clear(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        selected.replace(following.load(schema, f), f);
        count.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        count.store(f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema.release_i32_local(count, f);
        following.clear(f);
        let record = schema
            .reserve_gc_local(f)
            .initialize(selected.load(schema, f).require_non_null(f), f);
        selected.clear(f);
        Ok(record)
    }

    /// Ends a visit of the read miss kernel that filled `record` of the
    /// chain from `head`. A visit that recorded no new proof spends one unit
    /// of the site's learning budget. An own compact value proof moves to
    /// the site's first record, the only one the site reads inline, when
    /// that record names no own value: the two records exchange their shape
    /// and slot, which each prove the same fact wherever they are held.
    fn emit_settle_site_record(
        &mut self,
        head: &GcLocal<OwnDataReadCache>,
        record: &GcLocal<OwnDataReadCache>,
        fresh: I32Local,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        self.emit_site_record_is_blank_i32(record, f);
        fresh.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        let budget = schema.reserve_i32_local(f);
        schema
            .field(OwnDataReadCacheSchema::BUDGET)
            .read(head, schema, f)
            .store(budget, f);
        budget.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GtS);
        self.open_frame(ControlFrameKind::If, f);
        budget.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        budget.store(f);
        schema.field(OwnDataReadCacheSchema::BUDGET).write(
            head,
            GcOperand::i32_local(budget),
            schema,
            f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(budget, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        let keep = self.open_frame(ControlFrameKind::Block, f);
        let _ = record.load(schema, f);
        let _ = head.load(schema, f);
        f.instruction(&Instruction::RefEq);
        self.emit_branch_if_to_target(keep, f);
        let found = schema
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize(
                schema
                    .field(OwnDataReadCacheSchema::SHAPE)
                    .read(record, schema, f)
                    .reference(),
                f,
            );
        found.load(schema, f).is_null(f);
        self.emit_branch_if_to_target(keep, f);
        let slot = schema.reserve_i32_local(f);
        schema
            .field(OwnDataReadCacheSchema::SLOT)
            .read(record, schema, f)
            .store(slot, f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32LtS);
        self.emit_branch_if_to_target(keep, f);
        let displaced = schema
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize(
                schema
                    .field(OwnDataReadCacheSchema::SHAPE)
                    .read(head, schema, f)
                    .reference(),
                f,
            );
        let displaced_slot = schema.reserve_i32_local(f);
        schema
            .field(OwnDataReadCacheSchema::SLOT)
            .read(head, schema, f)
            .store(displaced_slot, f);
        // The first record already names an own value.
        displaced.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        displaced_slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        f.instruction(&Instruction::I32And);
        self.emit_branch_if_to_target(keep, f);
        schema.field(OwnDataReadCacheSchema::SHAPE).write(
            head,
            GcOperand::reference(&found, schema),
            schema,
            f,
        );
        schema.field(OwnDataReadCacheSchema::SLOT).write(
            head,
            GcOperand::i32_local(slot),
            schema,
            f,
        );
        schema.field(OwnDataReadCacheSchema::SHAPE).write(
            record,
            GcOperand::reference(&displaced, schema),
            schema,
            f,
        );
        schema.field(OwnDataReadCacheSchema::SLOT).write(
            record,
            GcOperand::i32_local(displaced_slot),
            schema,
            f,
        );
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema.release_i32_local(displaced_slot, f);
        displaced.clear(f);
        schema.release_i32_local(slot, f);
        found.clear(f);
    }

    /// Makes the record of a static property site, which the site keeps in
    /// its own global: R alone allocates and fills records. The record starts
    /// unvisited, so its first use reaches the miss kernel.
    pub(crate) fn compile_cached_property_record_helper(&mut self) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::CachedPropertyRecord);
        let p = self
            .helper_parameters::<crate::runtime_helpers::CachedPropertyRecordParameters>(&mut f);
        let schema = self.runtime_schema();
        let record = self.emit_new_site_record(&p.key, UNVISITED_SLOT, &mut f);
        let _ = record.load(schema, &mut f);
        record.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    /// The complete static read after the hit entry declined: registration,
    /// the own and prototype stages, and the general Get. A static read
    /// passes its raw base as both target and receiver.
    pub(crate) fn compile_cached_property_read_miss_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::CachedPropertyReadMiss);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::CachedPropertyReadMissParameters>(&mut f);
        let schema = self.runtime_schema();
        // The site's record names its key: a compiler-owned static String.
        let name = schema.reserve_gc_local(&mut f).initialize(
            schema
                .field(OwnDataReadCacheSchema::KEY)
                .read(&parameters.record, schema, &mut f)
                .reference(),
            &mut f,
        );
        let key = PropertyKeyLocals::from_string(schema, &name, &mut f);
        let p = CachedReadOperands {
            target: &parameters.target,
            receiver: &parameters.target,
            key: &key,
        };
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
        // Only compiler-owned static String sites have records, and a
        // record's key is typed as a String: no Symbol or object can ever
        // enter a strong record. One record serves the own compact slot,
        // the own dictionary position, the own intrinsic row and the
        // prototype stage. It holds only the key, shapes, integers and the
        // site's next record; every value is reloaded from live storage.
        let head = &parameters.record;
        // The kernel's first visit to a site's record makes it registered:
        // from then on the hit entry answers what no record of the site
        // does through the call-free walk, once the site has stopped
        // learning.
        schema
            .field(OwnDataReadCacheSchema::SLOT)
            .read(head, schema, &mut f)
            .store(handled, &mut f);
        handled.load(&mut f);
        f.instruction(&Instruction::I32Const(UNVISITED_SLOT));
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        schema
            .field(OwnDataReadCacheSchema::SLOT)
            .write(head, GcOperand::i32(0), schema, &mut f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::I32Const(0));
        handled.store(&mut f);
        // The entry tests the layout this visit met before any other.
        self.emit_record_receiver_layout_hint(&p.target, head, &mut f);
        // This visit fills one record of the site's chain, and uses no
        // other: the entry has already tried them all.
        let fresh = schema.reserve_i32_local(&mut f);
        let selected = self.emit_select_site_record(head, &name, fresh, &mut f)?;
        let cache = &selected;
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
            &p.target, &p.key, &object, &storage, cache, &result, handled, &mut f,
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
            cache,
            GcOperand::nullable_reference(&shape, schema),
            schema,
            &mut f,
        );
        schema.field(OwnDataReadCacheSchema::SLOT).write(
            cache,
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
            .read(cache, schema, &mut f)
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
        schema
            .field(OwnDataReadCacheSchema::DICTIONARY_POSITION)
            .write(cache, GcOperand::i32_local(position), schema, &mut f);
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
            &intrinsic, &p.key, cache, &own_value, handled, &mut f,
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
        self.emit_prototype_record_read(&p.target, &p.key, cache, &result, handled, &mut f)?;
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
        self.emit_settle_site_record(head, &selected, fresh, &mut f);
        selected.clear(&mut f);
        schema.release_i32_local(fresh, &mut f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        result.emit(&mut f);
        result.clear(&mut f);
        schema.release_i32_local(handled, &mut f);
        key.clear(&mut f);
        name.clear(&mut f);
        parameters.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }
}

/// A static read's operands inside the miss kernel.
struct CachedReadOperands<'a> {
    target: &'a ValueLocals,
    receiver: &'a ValueLocals,
    key: &'a PropertyKeyLocals,
}
