//! Hidden-class transitions keep plain data adds compact.
//!
//! Only a complete default data definition (writable, enumerable and
//! configurable) of an absent non-index String key, on an extensible compact
//! object whose shape holds default data attributes, moves the object to the
//! canonical child shape. Every other definition leaves the object untouched
//! and reports itself unhandled, so the general [[DefineOwnProperty]] runs and
//! performs its existing dictionary promotion exactly where it did before.
use super::*;

/// Keys a shape may reach by transitions. Probes scan keys linearly, but a
/// prepared-hash mismatch rejects each other key without reading code units,
/// so a miss at this bound stays comparable to one dictionary insertion. It
/// matches the plain literal plan bound and admits constructors that assign a
/// few dozen fields; the next add promotes to a dictionary.
pub(crate) const MAX_COMPACT_SHAPE_KEYS: i32 = 64;
/// Distinct one-key extensions one shape retains. Every fresh `{}` and `new F`
/// object shares the empty root, so its fan-out is the number of distinct first
/// keys. The bound keeps the linear transition scan short; a further new key
/// promotes that object instead. `COMPACT_SHAPE_BUDGET` bounds the whole graph.
const MAX_COMPACT_SHAPE_TRANSITIONS: i32 = 64;
const INITIAL_TRANSITION_CAPACITY: i32 = 4;
/// A growing object's SoA arrays carry spare slots, so its next adds publish
/// a new storage record over the same arrays instead of copying them.
const INITIAL_ARRAY_CAPACITY: i32 = 8;

pub(super) fn default_data_attributes() -> crate::heap::DescriptorWord {
    StoredPropertyAttributes::Data {
        writable: true,
        enumerable: true,
        configurable: true,
    }
    .descriptor_word()
}

impl FunctionBuilder<'_> {
    /// Offers a complete default data definition to the transition kernel.
    /// `None` means the descriptor can never qualify; otherwise the returned
    /// local is nonzero exactly when the kernel performed the definition.
    pub(in crate::objects) fn emit_try_compact_data_definition(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        descriptor: &WasmDescriptor<'_>,
        f: &mut Function,
    ) -> Result<Option<I32Local>, EmitError> {
        let fields = descriptor.as_partial();
        let Presence::Present(value) = &fields.value else {
            return Ok(None);
        };
        if !matches!(fields.get, Presence::Absent) || !matches!(fields.set, Presence::Absent) {
            return Ok(None);
        }
        let mut runtime = Vec::new();
        for flag in [&fields.writable, &fields.enumerable, &fields.configurable] {
            match flag {
                Presence::Present(DescriptorFlag::Known(true)) => {}
                Presence::Present(DescriptorFlag::BooleanPayload(local)) => runtime.push(*local),
                _ => return Ok(None),
            }
        }
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(f);
        handled.set_constant(0, f);
        if !runtime.is_empty() {
            f.instruction(&Instruction::I32Const(0));
            for flag in &runtime {
                flag.load(f);
                f.instruction(&Instruction::I32Eqz);
                f.instruction(&Instruction::I32Or);
            }
            f.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, f);
        }
        let called = self.emit_compact_data_define_call(object, key, value, f)?;
        called.load(f);
        handled.store(f);
        schema.release_i32_local(called, f);
        if !runtime.is_empty() {
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        Ok(Some(handled))
    }

    /// Defines `key` as a default data property holding `value` when the
    /// object is eligible; the returned local is zero, with no mutation,
    /// when the general algorithm must run instead.
    pub(in crate::objects) fn emit_compact_data_define_call(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(f);
        schema
            .call_helper(
                crate::runtime_helpers::CompactDataDefineArguments::new(object, key, value),
                self.runtime_helper_base()?,
                f,
            )
            .store(handled, f);
        Ok(handled)
    }

    pub(crate) fn compile_compact_data_define_helper(&mut self) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::CompactDataDefine);
        let p =
            self.helper_parameters::<crate::runtime_helpers::CompactDataDefineParameters>(&mut body);
        let f = &mut body;
        let s = self.runtime_schema();
        let handled = s.reserve_i32_local(f);
        handled.set_constant(0, f);
        let exit = self.open_frame(ControlFrameKind::Block, f);
        p.key.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(exit, f);
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&p.object, s, f)
                .reference(),
            f,
        );
        base.is_subtype::<CompactDataStorage>(s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        let compact = s.reserve_gc_local(f).initialize(
            base.load(s, f).cast_subtype::<CompactDataStorage>(s, f),
            f,
        );
        let shape = s.reserve_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&compact, s, f)
                .reference(),
            f,
        );
        // Native metadata shapes carry other attributes; one word covers
        // every key, so only an all-default shape can take a default key.
        s.field(CompactDataShapeSchema::ATTRIBUTES).read(&shape, s, f);
        f.instruction(&Instruction::I32Const(default_data_attributes().as_i32()));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(exit, f);
        let name = s.reserve_gc_local(f).initialize(
            p.key.value().cast_reference::<StringValue>(s, f),
            f,
        );
        let hash = s.reserve_i64_local(f);
        self.emit_string_key_hash(&name, hash, f);
        let slot = s.reserve_i32_local(f);
        let found = s.reserve_i32_local(f);
        self.emit_compact_slot_probe_prepared(&compact, &name, hash, slot, found, f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        // The present property already has exactly these attributes, so the
        // complete definition replaces only its value.
        self.emit_compact_store_at(&compact, slot, &p.value, f);
        handled.set_constant(1, f);
        self.emit_branch_to_target(exit, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.field(OrdinaryObjectSchema::EXTENSIBLE).read(&p.object, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(exit, f);
        let keys = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(&shape, s, f)
                .reference(),
            f,
        );
        let count = s.reserve_i32_local(f);
        s.array_type::<CompactShapeKeys>().length(&keys, s, f);
        count.store(f);
        let child = self.emit_compact_shape_child(&shape, &keys, count, &name, hash, &p.key, f)?;
        child.load(s, f).is_null(f);
        self.emit_branch_if_to_target(exit, f);
        let target = s
            .reserve_gc_local(f)
            .initialize(child.load(s, f).require_non_null(f), f);
        self.emit_compact_storage_extend(&p.object, &compact, &target, count, &p.value, f);
        handled.set_constant(1, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // These clears also run after a guard branch skipped initialization.
        target.clear(f);
        child.clear(f);
        s.release_i32_local(count, f);
        keys.clear(f);
        s.release_i32_local(found, f);
        s.release_i32_local(slot, f);
        s.release_i64_local(hash, f);
        name.clear(f);
        shape.clear(f);
        compact.clear(f);
        base.clear(f);
        handled.load(f);
        s.release_i32_local(handled, f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }

    /// The canonical child of `shape` for `name`, or null when a bound or an
    /// array-index key refuses it. Lookup is by key identity, then by equal
    /// contents, so objects built by the same adds share one shape.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_compact_shape_child(
        &mut self,
        shape: &GcLocal<CompactDataShape>,
        keys: &GcLocal<CompactShapeKeys>,
        count: I32Local,
        name: &GcLocal<StringValue>,
        hash: I64Local,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<GcLocal<CompactDataShape, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let table_type = s.array_type::<CompactShapeTable>();
        let child = s
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(s, f);
        let table = s.reserve_gc_local::<CompactShapeTable, Nullable>(f).initialize(
            s.field(CompactDataShapeSchema::TRANSITIONS)
                .read(shape, s, f)
                .reference(),
            f,
        );
        let used = s.reserve_i32_local(f);
        let capacity = s.reserve_i32_local(f);
        self.emit_compact_transition_scan(&table, count, name, hash, &child, used, capacity, f);

        child.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let refused = self.open_frame(ControlFrameKind::Block, f);
        count.load(f);
        f.instruction(&Instruction::I32Const(MAX_COMPACT_SHAPE_KEYS));
        f.instruction(&Instruction::I32GeU);
        used.load(f);
        f.instruction(&Instruction::I32Const(MAX_COMPACT_SHAPE_TRANSITIONS));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::I32Or);
        s.compact_shape_budget(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32LeS);
        f.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(refused, f);
        // OwnPropertyKeys orders array indices numerically before other
        // Strings; those keys keep the dictionary that already orders them.
        let index = s.reserve_i32_local(f);
        let canonical = s.reserve_i32_local(f);
        self.emit_property_key_array_index(key, index, canonical, f)?;
        canonical.load(f);
        s.release_i32_local(canonical, f);
        s.release_i32_local(index, f);
        self.emit_branch_if_to_target(refused, f);
        let ordered = s.reserve_i32_local(f);
        s.field(CompactDataShapeSchema::OWN_KEYS_ORDERED)
            .read(shape, s, f)
            .store(ordered, f);
        let extended = s.compact_shape_keys_extended(s.reserve_gc_local(f), keys, count, name, f);
        // The caller admitted only an all-default parent; children inherit it.
        let fresh = s.reserve_gc_local(f).initialize(
            s.struct_type::<CompactDataShape>().construct(
                (
                    GcOperand::reference(&extended, s),
                    GcOperand::descriptor_word(default_data_attributes()),
                    GcOperand::boolean_local(ordered),
                    GcOperand::null(s),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        s.release_i32_local(ordered, f);
        extended.clear(f);
        s.spend_compact_shape_budget(f);
        // Publish the child in its parent's table, growing it geometrically.
        let length = s.reserve_i32_local(f);
        table.load(s, f).is_null(f);
        used.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, f);
        capacity.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        length.store(f);
        length.load(f);
        f.instruction(&Instruction::I32Const(INITIAL_TRANSITION_CAPACITY));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        length.set_constant(INITIAL_TRANSITION_CAPACITY, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let grown = s
            .reserve_gc_local(f)
            .initialize(table_type.filled(GcOperand::null(s), length, f), f);
        table.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let previous = s
            .reserve_gc_local(f)
            .initialize(table.load(s, f).require_non_null(f), f);
        table_type.copy_prefix_from(&grown, &previous, used, s, f);
        previous.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.field(CompactDataShapeSchema::TRANSITIONS).write(
            shape,
            GcOperand::nullable_reference(&grown, s),
            s,
            f,
        );
        table.replace(grown.load(s, f).nullable(), f);
        grown.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(length, f);
        let published = s
            .reserve_gc_local(f)
            .initialize(table.load(s, f).require_non_null(f), f);
        table_type.write(
            &published,
            used,
            GcOperand::nullable_reference(&fresh, s),
            s,
            f,
        );
        published.clear(f);
        child.replace(fresh.load(s, f).nullable(), f);
        fresh.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(capacity, f);
        s.release_i32_local(used, f);
        table.clear(f);
        Ok(child)
    }

    /// The child of `shape`, which has `count` keys, already published for
    /// `name`, or null. Looking up creates and publishes nothing.
    pub(super) fn emit_compact_transition_lookup(
        &mut self,
        shape: &GcLocal<CompactDataShape>,
        count: I32Local,
        name: &GcLocal<StringValue>,
        hash: I64Local,
        f: &mut Function,
    ) -> GcLocal<CompactDataShape, Nullable> {
        let s = self.runtime_schema();
        let child = s
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(s, f);
        let table = s
            .reserve_gc_local::<CompactShapeTable, Nullable>(f)
            .initialize(
                s.field(CompactDataShapeSchema::TRANSITIONS)
                    .read(shape, s, f)
                    .reference(),
                f,
            );
        let used = s.reserve_i32_local(f);
        let capacity = s.reserve_i32_local(f);
        self.emit_compact_transition_scan(&table, count, name, hash, &child, used, capacity, f);
        s.release_i32_local(capacity, f);
        s.release_i32_local(used, f);
        table.clear(f);
        child
    }

    /// Scans a parent's transition `table` for its child for `name` and
    /// leaves it, or null, in `child`. `used` ends at the first entry not
    /// scanned and `capacity` at the table length (both zero without one).
    #[allow(clippy::too_many_arguments)]
    fn emit_compact_transition_scan(
        &mut self,
        table: &GcLocal<CompactShapeTable, Nullable>,
        count: I32Local,
        name: &GcLocal<StringValue>,
        hash: I64Local,
        child: &GcLocal<CompactDataShape, Nullable>,
        used: I32Local,
        capacity: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let table_type = s.array_type::<CompactShapeTable>();
        used.set_constant(0, f);
        capacity.set_constant(0, f);
        let scanned = self.open_frame(ControlFrameKind::Block, f);
        table.load(s, f).is_null(f);
        self.emit_branch_if_to_target(scanned, f);
        let present = s
            .reserve_gc_local(f)
            .initialize(table.load(s, f).require_non_null(f), f);
        table_type.length(&present, s, f);
        capacity.store(f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        used.load(f);
        capacity.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(scanned, f);
        let entry = s
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize(table_type.read(&present, used, s, f).reference(), f);
        entry.load(s, f).is_null(f);
        self.emit_branch_if_to_target(scanned, f);
        let candidate = s
            .reserve_gc_local(f)
            .initialize(entry.load(s, f).require_non_null(f), f);
        let candidate_keys = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(&candidate, s, f)
                .reference(),
            f,
        );
        // Every child extends this exact parent, so its new key is at `count`.
        let candidate_key = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactShapeKeys>()
                .read(&candidate_keys, count, s, f)
                .reference(),
            f,
        );
        self.emit_compact_key_matches_i32(&candidate_key, name, hash, f);
        self.open_frame(ControlFrameKind::If, f);
        child.replace(candidate.load(s, f).nullable(), f);
        self.emit_branch_to_target(scanned, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        used.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        used.store(f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        candidate_key.clear(f);
        candidate_keys.clear(f);
        candidate.clear(f);
        entry.clear(f);
        present.clear(f);
    }

    /// Publishes a new storage for `child` (one key more than `compact`) with
    /// `value` in its last slot. One or two keys stay inline; a SoA storage
    /// with a spare slot shares its arrays with the record it replaces, and
    /// only a full one copies into doubled arrays. The replaced record is no
    /// longer reachable from the object, so the shared arrays have one owner.
    pub(super) fn emit_compact_storage_extend(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        compact: &GcLocal<CompactDataStorage>,
        child: &GcLocal<CompactDataShape>,
        count: I32Local,
        value: &ValueLocals,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let undefined = WasmRuntimeValueTag::Undefined as i32;
        let extended = s
            .reserve_gc_local::<CompactDataStorage, Nullable>(f)
            .initialize_null(s, f);
        compact.is_subtype::<InlineCompactDataStorage>(s, f);
        self.open_frame(ControlFrameKind::If, f);
        let inline = s.reserve_gc_local(f).initialize(
            compact
                .load(s, f)
                .cast_subtype::<InlineCompactDataStorage>(s, f),
            f,
        );
        count.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let first = s.reserve_value_local(f);
        let second = s.reserve_value_local(f);
        s.field(InlineCompactDataStorageSchema::PACKED_TAGS)
            .read(&inline, s, f)
            .store(first.tag(), f);
        s.field(InlineCompactDataStorageSchema::SCALAR0)
            .read(&inline, s, f)
            .store_i64(first.scalar(), f);
        s.field(InlineCompactDataStorageSchema::SCALAR1)
            .read(&inline, s, f)
            .store_i64(second.scalar(), f);
        s.field(InlineCompactDataStorageSchema::REFERENCE0)
            .read(&inline, s, f)
            .store_eq(first.reference(), f);
        s.field(InlineCompactDataStorageSchema::REFERENCE1)
            .read(&inline, s, f)
            .store_eq(second.reference(), f);
        let pair = s.reserve_gc_local(f).initialize(
            s.struct_type::<InlineCompactDataStorage>().construct(
                (
                    GcOperand::reference(child, s),
                    GcOperand::i32_local(first.tag()),
                    GcOperand::i64_local(first.scalar()),
                    GcOperand::i64_local(second.scalar()),
                    GcOperand::eq_reference(first.reference()),
                    GcOperand::eq_reference(second.reference()),
                ),
                f,
            ),
            f,
        );
        second.clear(f);
        first.clear(f);
        extended.replace(pair.load(s, f).upcast::<CompactDataStorage>().nullable(), f);
        pair.clear(f);
        f.instruction(&Instruction::Else);
        let capacity = s.reserve_i32_local(f);
        capacity.set_constant(INITIAL_ARRAY_CAPACITY, f);
        let array = self.emit_alloc_compact_array_storage(child, capacity, f);
        s.release_i32_local(capacity, f);
        let moved = s.reserve_value_local(f);
        let slot = s.reserve_i32_local(f);
        for ordinal in 0..2 {
            slot.set_constant(ordinal, f);
            self.emit_inline_compact_value_at(&inline, slot, &moved, f);
            self.emit_array_compact_store_at(&array, slot, &moved, f);
        }
        s.release_i32_local(slot, f);
        moved.clear(f);
        extended.replace(array.load(s, f).upcast::<CompactDataStorage>().nullable(), f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        inline.clear(f);
        f.instruction(&Instruction::Else);
        let array = s.reserve_gc_local(f).initialize(
            compact
                .load(s, f)
                .cast_subtype::<ArrayCompactDataStorage>(s, f),
            f,
        );
        let tags = s.reserve_gc_local(f).initialize(
            s.field(ArrayCompactDataStorageSchema::TAGS)
                .read(&array, s, f)
                .reference(),
            f,
        );
        count.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let first = s.reserve_gc_local(f).initialize(
            s.struct_type::<InlineCompactDataStorage>().construct(
                (
                    GcOperand::reference(child, s),
                    GcOperand::i32(undefined | (undefined << 8)),
                    GcOperand::i64(0),
                    GcOperand::i64(0),
                    GcOperand::null_eq(),
                    GcOperand::null_eq(),
                ),
                f,
            ),
            f,
        );
        extended.replace(first.load(s, f).upcast::<CompactDataStorage>().nullable(), f);
        first.clear(f);
        f.instruction(&Instruction::Else);
        count.load(f);
        s.array_type::<CompactTagArray>().length(&tags, s, f);
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        let scalars = s
            .reserve_gc_local::<CompactScalarArray, Nullable>(f)
            .initialize(
                s.field(ArrayCompactDataStorageSchema::SCALARS)
                    .read(&array, s, f)
                    .reference(),
                f,
            );
        let references = s
            .reserve_gc_local::<CompactReferenceArray, Nullable>(f)
            .initialize(
                s.field(ArrayCompactDataStorageSchema::REFERENCES)
                    .read(&array, s, f)
                    .reference(),
                f,
            );
        let shared = s.reserve_gc_local(f).initialize(
            s.struct_type::<ArrayCompactDataStorage>().construct(
                (
                    GcOperand::reference(child, s),
                    GcOperand::reference(&tags, s),
                    GcOperand::reference(&scalars, s),
                    GcOperand::reference(&references, s),
                ),
                f,
            ),
            f,
        );
        references.clear(f);
        scalars.clear(f);
        extended.replace(shared.load(s, f).upcast::<CompactDataStorage>().nullable(), f);
        shared.clear(f);
        f.instruction(&Instruction::Else);
        let grown = self.emit_grow_compact_array_storage(&array, &tags, child, count, f);
        extended.replace(grown.load(s, f).upcast::<CompactDataStorage>().nullable(), f);
        grown.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        tags.clear(f);
        array.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let next = s
            .reserve_gc_local(f)
            .initialize(extended.load(s, f).require_non_null(f), f);
        extended.clear(f);
        self.emit_compact_store_at(&next, count, value, f);
        // Publish only after the replacement owns every value.
        s.field(OrdinaryObjectSchema::PROPERTIES).write(
            object,
            GcOperand::subtype_reference(&next, s),
            s,
            f,
        );
        next.clear(f);
    }

    /// A fresh SoA storage whose `capacity` slots all hold undefined tags and
    /// whose scalar/reference backings stay lazily absent.
    fn emit_alloc_compact_array_storage(
        &mut self,
        shape: &GcLocal<CompactDataShape>,
        capacity: I32Local,
        f: &mut Function,
    ) -> GcLocal<ArrayCompactDataStorage> {
        let s = self.runtime_schema();
        let tags = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactTagArray>().filled(
                GcOperand::i32(WasmRuntimeValueTag::Undefined as i32),
                capacity,
                f,
            ),
            f,
        );
        let array = s.reserve_gc_local(f).initialize(
            s.struct_type::<ArrayCompactDataStorage>().construct(
                (
                    GcOperand::reference(shape, s),
                    GcOperand::reference(&tags, s),
                    GcOperand::null(s),
                    GcOperand::null(s),
                ),
                f,
            ),
            f,
        );
        tags.clear(f);
        array
    }

    /// Copies the `count` live slots into arrays of twice that length.
    fn emit_grow_compact_array_storage(
        &mut self,
        array: &GcLocal<ArrayCompactDataStorage>,
        tags: &GcLocal<CompactTagArray>,
        child: &GcLocal<CompactDataShape>,
        count: I32Local,
        f: &mut Function,
    ) -> GcLocal<ArrayCompactDataStorage> {
        let s = self.runtime_schema();
        let capacity = s.reserve_i32_local(f);
        // count < MAX_COMPACT_SHAPE_KEYS, so doubling cannot wrap.
        count.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Shl);
        capacity.store(f);
        capacity.load(f);
        f.instruction(&Instruction::I32Const(INITIAL_ARRAY_CAPACITY));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        capacity.set_constant(INITIAL_ARRAY_CAPACITY, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let grown = self.emit_alloc_compact_array_storage(child, capacity, f);
        let grown_tags = s.reserve_gc_local(f).initialize(
            s.field(ArrayCompactDataStorageSchema::TAGS)
                .read(&grown, s, f)
                .reference(),
            f,
        );
        s.array_type::<CompactTagArray>()
            .copy_prefix_from(&grown_tags, tags, count, s, f);
        grown_tags.clear(f);
        let scalars = s
            .reserve_gc_local::<CompactScalarArray, Nullable>(f)
            .initialize(
                s.field(ArrayCompactDataStorageSchema::SCALARS)
                    .read(array, s, f)
                    .reference(),
                f,
            );
        scalars.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let source = s
            .reserve_gc_local(f)
            .initialize(scalars.load(s, f).require_non_null(f), f);
        let copied = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactScalarArray>()
                .filled(GcOperand::i64(0), capacity, f),
            f,
        );
        s.array_type::<CompactScalarArray>()
            .copy_prefix_from(&copied, &source, count, s, f);
        s.field(ArrayCompactDataStorageSchema::SCALARS).write(
            &grown,
            GcOperand::nullable_reference(&copied, s),
            s,
            f,
        );
        copied.clear(f);
        source.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        scalars.clear(f);
        let references = s
            .reserve_gc_local::<CompactReferenceArray, Nullable>(f)
            .initialize(
                s.field(ArrayCompactDataStorageSchema::REFERENCES)
                    .read(array, s, f)
                    .reference(),
                f,
            );
        references.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let source = s
            .reserve_gc_local(f)
            .initialize(references.load(s, f).require_non_null(f), f);
        let copied = s.reserve_gc_local(f).initialize(
            s.array_type::<CompactReferenceArray>()
                .filled(GcOperand::null_eq(), capacity, f),
            f,
        );
        s.array_type::<CompactReferenceArray>()
            .copy_prefix_from(&copied, &source, count, s, f);
        s.field(ArrayCompactDataStorageSchema::REFERENCES).write(
            &grown,
            GcOperand::nullable_reference(&copied, s),
            s,
            f,
        );
        copied.clear(f);
        source.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        references.clear(f);
        s.release_i32_local(capacity, f);
        grown
    }
}
