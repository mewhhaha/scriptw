//! The hit entries of static reads and assignments.
//!
//! A static site calls a small entry that answers from the site's record when
//! the live receiver proves it, and otherwise tail-calls the complete miss
//! kernel with the same operands. A read entry makes no other call and
//! registers, resolves and allocates nothing. Each answer re-proves exactly
//! what the miss kernel's use of the same record proves: an own compact slot,
//! or absence before a closed tail, by the receiver's immutable shape; an own
//! dictionary entry by its key at the recorded position; an own intrinsic row
//! by the immutable layout address; and a place on the prototype by the
//! receiver's absence rule and the live prototype's layout, shape or entry.
//! An assignment stores into an existing writable own compact slot, or proves
//! an add by the recorded shape transition and tail-calls the kernel that
//! extends the storage. Every other case, including a value still held as a
//! recipe or a dictionary holder whose key index cannot prove absence,
//! misses.
use super::holder::{HolderReceiverKind, RecipeValues};
use super::write::DictionaryAbsence;
use super::*;
use crate::emit::ControlTarget;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, FunctionObject, FunctionObjectSchema, JavaScriptObject,
    MapObject, MapObjectSchema, PrimitiveBox, PrimitiveBoxSchema, PromiseObject,
    PromiseObjectSchema, RealmRecord, SetObject, SetObjectSchema, WeakMapObject,
    WeakMapObjectSchema, WeakSetObject, WeakSetObjectSchema,
};

/// A compact storage of one of its two sealed layouts.
enum CompactStorage {
    Array(GcLocal<ArrayCompactDataStorage>),
    Inline(GcLocal<InlineCompactDataStorage>),
}

/// The sealed compact layouts, in the order an entry tests them.
const COMPACT_LAYOUTS: [CompactLayout; 2] = [CompactLayout::Array, CompactLayout::Inline];

#[derive(Clone, Copy)]
enum CompactLayout {
    Array,
    Inline,
}

impl CompactStorage {
    fn shape(
        &self,
        schema: &RuntimeSchema,
        f: &mut Function,
    ) -> GcStackReference<CompactDataShape> {
        match self {
            Self::Array(storage) => schema
                .field(ArrayCompactDataStorageSchema::SHAPE)
                .read(storage, schema, f)
                .reference(),
            Self::Inline(storage) => schema
                .field(InlineCompactDataStorageSchema::SHAPE)
                .read(storage, schema, f)
                .reference(),
        }
    }

    fn base(
        &self,
        schema: &RuntimeSchema,
        f: &mut Function,
    ) -> GcStackReference<CompactDataStorage> {
        match self {
            Self::Array(storage) => storage.load(schema, f).upcast(),
            Self::Inline(storage) => storage.load(schema, f).upcast(),
        }
    }

    fn clear(self, f: &mut Function) {
        match self {
            Self::Array(storage) => storage.clear(f),
            Self::Inline(storage) => storage.clear(f),
        }
    }
}

/// The primitive receivers whose records name their Realm's prototype.
const PRIMITIVE_RECEIVERS: [(
    WasmRuntimeValueTag,
    HolderReceiverKind,
    NonArrayRealmIntrinsicSlot,
); 2] = [
    (
        WasmRuntimeValueTag::String,
        HolderReceiverKind::String,
        NonArrayRealmIntrinsicSlot::StringPrototype,
    ),
    (
        WasmRuntimeValueTag::Number,
        HolderReceiverKind::Number,
        NonArrayRealmIntrinsicSlot::NumberPrototype,
    ),
];

/// The primitives a walk boxes with the current Realm's prototype.
const WALKED_PRIMITIVES: [(WasmRuntimeValueTag, NonArrayRealmIntrinsicSlot); 5] = [
    (
        WasmRuntimeValueTag::String,
        NonArrayRealmIntrinsicSlot::StringPrototype,
    ),
    (
        WasmRuntimeValueTag::Number,
        NonArrayRealmIntrinsicSlot::NumberPrototype,
    ),
    (
        WasmRuntimeValueTag::Boolean,
        NonArrayRealmIntrinsicSlot::BooleanPrototype,
    ),
    (
        WasmRuntimeValueTag::Symbol,
        NonArrayRealmIntrinsicSlot::SymbolPrototype,
    ),
    (
        WasmRuntimeValueTag::BigInt,
        NonArrayRealmIntrinsicSlot::BigIntPrototype,
    ),
];

impl FunctionBuilder<'_> {
    pub(crate) fn compile_cached_property_read_helper(&mut self) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::CachedPropertyRead);
        let p = self
            .helper_parameters::<crate::runtime_helpers::CachedPropertyReadParameters>(&mut body);
        let f = &mut body;
        let s = self.runtime_schema();
        let key = PropertyKeyLocals::from_string(s, &p.key, f);
        let value = s.reserve_value_local(f);
        let found = s.reserve_i32_local(f);
        // The prototype that holds the recorded place, and its header.
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        let holder = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let miss = self.open_frame(ControlFrameKind::Block, f);
        let record = self.emit_site_record_or_branch(p.site, &key, miss, f);
        // A record that does not apply leaves the live chain to the walk.
        let walk = self.open_frame(ControlFrameKind::Block, f);
        let place = self.open_frame(ControlFrameKind::Block, f);
        let edge = self.open_frame(ControlFrameKind::Block, f);

        // A String or Number has no own property for a recorded key: its own
        // keys are indices and "length", which never get a record. ToObject
        // would box it with the current Realm's prototype.
        for (tag, kind, slot) in PRIMITIVE_RECEIVERS {
            p.target.tag().load(f);
            f.instruction(&Instruction::I32Const(tag as i32));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            self.emit_receiver_rule_or_branch(&record, kind, None, walk, f);
            let realm: GcLocal<RealmRecord> = self.load_current_realm(f);
            let intrinsic = s.reserve_value_local(f);
            self.emit_load_non_array_realm_intrinsic(&realm, slot, &intrinsic, f);
            realm.clear(f);
            let header = self.emit_realm_prototype_header_or_branch(&intrinsic, walk, f);
            intrinsic.clear(f);
            holder.replace(header.load(s, f).nullable(), f);
            header.clear(f);
            self.emit_branch_to_target(place, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }

        // An actual ordinary object or function: its own records, then its
        // absence rule.
        let not_own = self.open_frame(ControlFrameKind::Block, f);
        let object = self.emit_own_header_or_branch(&p.target, not_own, f);
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        for layout in COMPACT_LAYOUTS {
            let other = self.open_frame(ControlFrameKind::Block, f);
            let storage = self.emit_compact_storage_or_branch(layout, &base, other, f);
            let shape = s.reserve_gc_local(f).initialize(storage.shape(s, f), f);
            let _ = shape.load(s, f);
            s.field(OwnDataReadCacheSchema::SHAPE).read(&record, s, f);
            f.instruction(&Instruction::RefEq);
            self.open_frame(ControlFrameKind::If, f);
            // The recorded shape has the key at a nonnegative slot; a
            // negative slot is the closed tail beyond a shape that lacks it.
            let slot = s.reserve_i32_local(f);
            s.field(OwnDataReadCacheSchema::SLOT)
                .read(&record, s, f)
                .store(slot, f);
            slot.load(f);
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32GeS);
            self.open_frame(ControlFrameKind::If, f);
            match &storage {
                CompactStorage::Array(array) => {
                    self.emit_array_compact_value_at(array, slot, &value, f)
                }
                CompactStorage::Inline(inline) => {
                    self.emit_inline_compact_value_at(inline, slot, &value, f)
                }
            }
            emit_return_normal(&value, f);
            f.instruction(&Instruction::Else);
            let mode = self.emit_negative_cache_mode(&p.target, &object, None, f)?;
            mode.load(f);
            slot.load(f);
            f.instruction(&Instruction::I32Eq);
            mode.load(f);
            f.instruction(&Instruction::I32Eqz);
            f.instruction(&Instruction::I32Eqz);
            f.instruction(&Instruction::I32And);
            s.release_i32_local(mode, f);
            f.instruction(&Instruction::If(BlockType::Empty));
            value.set_undefined(f);
            emit_return_normal(&value, f);
            f.instruction(&Instruction::End);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            s.release_i32_local(slot, f);
            self.emit_branch_to_target(walk, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            // The live shape lacks the key.
            self.emit_receiver_rule_or_branch(
                &record,
                HolderReceiverKind::Object,
                Some(&shape),
                walk,
                f,
            );
            self.emit_prototype_edge(&object, &prototype, edge, f);
            shape.clear(f);
            storage.clear(f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        let other = self.open_frame(ControlFrameKind::Block, f);
        let dictionary = s.reserve_gc_local(f).initialize(
            base.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(
                s,
                self.local_branch_label(other),
                f,
            ),
            f,
        );
        let position = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::DICTIONARY_POSITION)
            .read(&record, s, f)
            .store(position, f);
        self.emit_dictionary_position_read_with(
            &dictionary,
            position,
            &key,
            &value,
            found,
            RecipeValues::Decline,
            f,
        );
        s.release_i32_local(position, f);
        found.load(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        emit_return_normal(&value, f);
        f.instruction(&Instruction::End);
        self.emit_empty_dictionary_or_branch(&dictionary, walk, f);
        dictionary.clear(f);
        self.emit_receiver_rule_or_branch(&record, HolderReceiverKind::Object, None, walk, f);
        self.emit_prototype_edge(&object, &prototype, edge, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // An intrinsic layout is immutable: storage at the recorded address
        // has the key's data row in the recorded value group.
        let intrinsic = s.reserve_gc_local(f).initialize(
            base.sealed_subtype_or_branch::<IntrinsicPropertyStorage>(
                s,
                self.local_branch_label(walk),
                f,
            ),
            f,
        );
        s.field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(&intrinsic, s, f);
        s.field(OwnDataReadCacheSchema::LAYOUT).read(&record, s, f);
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(walk, f);
        let group = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::GROUP)
            .read(&record, s, f)
            .store(group, f);
        found.set_constant(0, f);
        self.emit_materialized_intrinsic_group(&intrinsic, group, &value, found, f);
        s.release_i32_local(group, f);
        intrinsic.clear(f);
        self.emit_found_return_or_miss(found, &value, walk, f);
        base.clear(f);
        object.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        // An Array, primitive wrapper, Map, WeakMap, Set, WeakSet or
        // Promise: only its absence rule. An Array's or String wrapper's
        // indices and "length" are not in its header, and never get a record.
        let object = self.emit_composite_header_or_branch(&p.target, walk, f);
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        for layout in COMPACT_LAYOUTS {
            let other = self.open_frame(ControlFrameKind::Block, f);
            let storage = self.emit_compact_storage_or_branch(layout, &base, other, f);
            let shape = s.reserve_gc_local(f).initialize(storage.shape(s, f), f);
            self.emit_receiver_rule_or_branch(
                &record,
                HolderReceiverKind::Object,
                Some(&shape),
                walk,
                f,
            );
            self.emit_prototype_edge(&object, &prototype, edge, f);
            shape.clear(f);
            storage.clear(f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        let dictionary = s.reserve_gc_local(f).initialize(
            base.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(
                s,
                self.local_branch_label(walk),
                f,
            ),
            f,
        );
        self.emit_empty_dictionary_or_branch(&dictionary, walk, f);
        dictionary.clear(f);
        self.emit_receiver_rule_or_branch(&record, HolderReceiverKind::Object, None, walk, f);
        self.emit_prototype_edge(&object, &prototype, edge, f);
        base.clear(f);
        object.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);

        // The receiver's live [[Prototype]] holds the place.
        let header = self.emit_prototype_header_or_branch(&prototype, walk, f);
        holder.replace(header.load(s, f).nullable(), f);
        header.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let present = s
            .reserve_gc_local(f)
            .initialize(holder.load(s, f).require_non_null(f), f);
        self.emit_recorded_place_hit(&record, &present, &key, &value, found, walk, f)?;
        present.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        record.clear(f);
        self.emit_static_read_walk(&p.target, &p.key, &value, miss, f);
        emit_return_normal(&value, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.return_call_helper(
            crate::runtime_helpers::CachedPropertyReadMissArguments::new(
                &p.target,
                p.site,
                &p.key,
                self.current_environment(),
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            f,
        );
        holder.clear(f);
        prototype.clear(f);
        s.release_i32_local(found, f);
        value.clear(f);
        key.clear(f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }

    pub(crate) fn compile_cached_property_write_helper(&mut self) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::CachedPropertyWrite);
        let p = self
            .helper_parameters::<crate::runtime_helpers::CachedPropertyWriteParameters>(&mut body);
        let f = &mut body;
        let s = self.runtime_schema();
        let shape = s
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(s, f);
        let compact = s
            .reserve_gc_local::<CompactDataStorage, Nullable>(f)
            .initialize_null(s, f);
        let miss = self.open_frame(ControlFrameKind::Block, f);
        // OrdinarySet(O, P, V, O) on an actual OrdinaryObject, which answers
        // [[GetOwnProperty]], [[GetPrototypeOf]] and [[DefineOwnProperty]]
        // from its header.
        p.target.reference().load(f);
        p.receiver.reference().load(f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
        let record = self.emit_site_record_or_branch(p.site, &p.key, miss, f);
        let object = s.reserve_gc_local(f).initialize(
            p.target.sealed_reference_or_branch::<OrdinaryObject>(
                s,
                self.local_branch_label(miss),
                f,
            ),
            f,
        );
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        let add = self.open_frame(ControlFrameKind::Block, f);
        for layout in COMPACT_LAYOUTS {
            let other = self.open_frame(ControlFrameKind::Block, f);
            let storage = self.emit_compact_storage_or_branch(layout, &base, other, f);
            shape.replace(storage.shape(s, f).nullable(), f);
            compact.replace(storage.base(s, f).nullable(), f);
            // An existing own data property: the recorded shape has the key
            // at the recorded slot and its one attribute word is writable
            // data, so OrdinarySet only replaces [[Value]] on O itself.
            let _ = shape.load(s, f);
            s.field(OwnDataReadCacheSchema::SHAPE).read(&record, s, f);
            f.instruction(&Instruction::RefEq);
            self.open_frame(ControlFrameKind::If, f);
            let present = s
                .reserve_gc_local(f)
                .initialize(shape.load(s, f).require_non_null(f), f);
            s.field(CompactDataShapeSchema::ATTRIBUTES)
                .read(&present, s, f);
            present.clear(f);
            f.instruction(&Instruction::I32Const(DescriptorMask::WRITABLE.as_i32()));
            f.instruction(&Instruction::I32And);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(miss, f);
            let slot = s.reserve_i32_local(f);
            s.field(OwnDataReadCacheSchema::SLOT)
                .read(&record, s, f)
                .store(slot, f);
            slot.load(f);
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32LtS);
            self.emit_branch_if_to_target(miss, f);
            match &storage {
                CompactStorage::Array(array) => {
                    self.emit_array_compact_backed_store_or_branch(array, slot, &p.value, miss, f)
                }
                CompactStorage::Inline(inline) => {
                    self.emit_inline_compact_store_at(inline, slot, &p.value, f)
                }
            }
            s.release_i32_local(slot, f);
            emit_return_true(s, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            storage.clear(f);
            self.emit_branch_to_target(add, f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        self.emit_branch_to_target(miss, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // An add: the parent shape lacks the key, so O has no own property,
        // and the chain lacks it, so no setter, read-only property or Proxy
        // intervenes. CreateDataProperty then defines a default data
        // property on the extensible O, which is the recorded canonical child.
        let parent = s
            .reserve_gc_local(f)
            .initialize(shape.load(s, f).require_non_null(f), f);
        let _ = parent.load(s, f);
        s.field(OwnDataReadCacheSchema::ADD_PARENT)
            .read(&record, s, f);
        f.instruction(&Instruction::RefEq);
        s.field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(&object, s, f);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
        // A dictionary holder is proved by its key index alone: the entry
        // makes no call.
        let tail = self.emit_recorded_add_chain_tail(
            &p.target,
            &object,
            &p.key,
            &record,
            DictionaryAbsence::IndexOnly,
            f,
        )?;
        tail.load(f);
        s.field(OwnDataReadCacheSchema::ADD_TAIL)
            .read(&record, s, f);
        f.instruction(&Instruction::I32Ne);
        tail.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::I32Or);
        s.release_i32_local(tail, f);
        self.emit_branch_if_to_target(miss, f);
        let recorded = s
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize(
                s.field(OwnDataReadCacheSchema::ADD_CHILD)
                    .read(&record, s, f)
                    .reference(),
                f,
            );
        let child = s.reserve_gc_local(f).initialize(
            recorded.non_null_or_branch(self.local_branch_label(miss), f),
            f,
        );
        recorded.clear(f);
        let keys = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(&parent, s, f)
                .reference(),
            f,
        );
        let count = s.reserve_i32_local(f);
        s.array_type::<CompactShapeKeys>().length(&keys, s, f);
        count.store(f);
        keys.clear(f);
        let storage = s
            .reserve_gc_local(f)
            .initialize(compact.load(s, f).require_non_null(f), f);
        // Every precondition is proved; no code runs before the extension.
        s.return_call_helper(
            crate::runtime_helpers::CachedPropertyAddArguments::new(
                &object, &storage, &child, count, &p.value,
            ),
            self.runtime_helper_base()?,
            f,
        );
        storage.clear(f);
        s.release_i32_local(count, f);
        child.clear(f);
        parent.clear(f);
        base.clear(f);
        object.clear(f);
        record.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.return_call_helper(
            crate::runtime_helpers::CachedPropertyWriteMissArguments::new(
                &p.target,
                &p.receiver,
                &p.key,
                &p.value,
                p.site,
                self.current_environment(),
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            f,
        );
        compact.clear(f);
        shape.clear(f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }

    /// Extends a compact storage by one key whose add the write entry has
    /// proved, and completes the [[Set]] with true.
    pub(crate) fn compile_cached_property_add_helper(&mut self) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::CachedPropertyAdd);
        let p = self
            .helper_parameters::<crate::runtime_helpers::CachedPropertyAddParameters>(&mut body);
        let f = &mut body;
        let s = self.runtime_schema();
        self.emit_compact_storage_extend(&p.object, &p.storage, &p.child, p.count, &p.value, f);
        emit_return_true(s, f);
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }

    /// A static read that no record answers: the call-free walk from the
    /// live receiver, or from the current Realm's prototype for a primitive,
    /// which ToObject would box with it. Falls through with the value of the
    /// read; branches to `miss` for every case the walk cannot prove.
    fn emit_static_read_walk(
        &mut self,
        target: &ValueLocals,
        key: &GcLocal<StringValue>,
        value: &ValueLocals,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        // The miss kernel prepares the key's hash.
        let hash = s.reserve_i64_local(f);
        s.field(StringValueSchema::RAW_KEY_HASH)
            .read(key, s, f)
            .store_i64(hash, f);
        hash.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(miss, f);
        let index_free = s.reserve_i32_local(f);
        self.emit_index_free_key_i32(key, f);
        index_free.store(f);
        let named = s.reserve_i32_local(f);
        self.emit_header_named_key_i32(key, hash, f);
        named.store(f);
        let start = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let begin = self.open_frame(ControlFrameKind::Block, f);
        for (tag, slot) in WALKED_PRIMITIVES {
            target.tag().load(f);
            f.instruction(&Instruction::I32Const(tag as i32));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            if tag == WasmRuntimeValueTag::String {
                // A String owns its indices and "length".
                named.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.emit_branch_if_to_target(miss, f);
            }
            let realm: GcLocal<RealmRecord> = self.load_current_realm(f);
            let intrinsic = s.reserve_value_local(f);
            self.emit_load_non_array_realm_intrinsic(&realm, slot, &intrinsic, f);
            realm.clear(f);
            let header = self.emit_realm_prototype_header_or_branch(&intrinsic, miss, f);
            intrinsic.clear(f);
            start.replace(header.load(s, f).nullable(), f);
            header.clear(f);
            self.emit_branch_to_target(begin, f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        let header = self.emit_walk_receiver_header_or_branch(target, index_free, named, miss, f);
        start.replace(header.load(s, f).nullable(), f);
        header.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let first = s
            .reserve_gc_local(f)
            .initialize(start.load(s, f).require_non_null(f), f);
        start.clear(f);
        self.emit_chain_walk(
            &first,
            ChainKey::String { name: key, hash },
            named,
            ChainAnswer::Get,
            value,
            miss,
            f,
        );
        first.clear(f);
        s.release_i32_local(named, f);
        s.release_i32_local(index_free, f);
        s.release_i64_local(hash, f);
    }

    /// The site's record, after branching to `miss` when the site has none
    /// yet. A record is made for its site's key; the key reference proves it.
    fn emit_site_record_or_branch(
        &mut self,
        site: I32Local,
        key: &PropertyKeyLocals,
        miss: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<OwnDataReadCache> {
        let s = self.runtime_schema();
        let label = self.local_branch_label(miss);
        let records = self.current_program().own_data_records(s, f);
        let table = s
            .reserve_gc_local(f)
            .initialize(records.non_null_or_branch(label, f), f);
        records.clear(f);
        site.load(f);
        s.array_type::<OwnDataReadCacheTable>().length(&table, s, f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(miss, f);
        let entry = s.reserve_gc_local(f).initialize(
            s.array_type::<OwnDataReadCacheTable>()
                .read(&table, site, s, f)
                .reference(),
            f,
        );
        table.clear(f);
        let record = s
            .reserve_gc_local(f)
            .initialize(entry.non_null_or_branch(label, f), f);
        entry.clear(f);
        s.field(OwnDataReadCacheSchema::KEY).read(&record, s, f);
        key.value().reference().load(f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
        record
    }

    /// The ordinary header of an actual ordinary object or function; every
    /// other value branches to `other`.
    fn emit_own_header_or_branch(
        &mut self,
        target: &ValueLocals,
        other: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject> {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let callable = self.open_frame(ControlFrameKind::Block, f);
        header.replace(
            target
                .sealed_reference_or_branch::<OrdinaryObject>(
                    s,
                    self.local_branch_label(callable),
                    f,
                )
                .nullable(),
            f,
        );
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let function = s.reserve_gc_local(f).initialize(
            target.sealed_reference_or_branch::<FunctionObject>(
                s,
                self.local_branch_label(other),
                f,
            ),
            f,
        );
        header.replace(
            s.field(FunctionObjectSchema::OBJECT)
                .read(&function, s, f)
                .reference()
                .nullable(),
            f,
        );
        function.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let object = s
            .reserve_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        header.clear(f);
        object
    }

    /// The ordinary header of an Array, primitive wrapper, Map, WeakMap,
    /// Set, WeakSet or Promise, the composites whose named own properties
    /// all live in that header; every other value branches to `other`.
    fn emit_composite_header_or_branch(
        &mut self,
        target: &ValueLocals,
        other: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject> {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        macro_rules! composite {
            ($layout:ty, $field:expr) => {{
                let next = self.open_frame(ControlFrameKind::Block, f);
                let composite = s.reserve_gc_local(f).initialize(
                    target.sealed_reference_or_branch::<$layout>(
                        s,
                        self.local_branch_label(next),
                        f,
                    ),
                    f,
                );
                header.replace(
                    s.field($field)
                        .read(&composite, s, f)
                        .reference()
                        .nullable(),
                    f,
                );
                composite.clear(f);
                self.emit_branch_to_target(done, f);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
            }};
        }
        composite!(ArrayObject, ArrayObjectSchema::OBJECT);
        composite!(PrimitiveBox, PrimitiveBoxSchema::OBJECT);
        composite!(MapObject, MapObjectSchema::OBJECT);
        composite!(WeakMapObject, WeakMapObjectSchema::OBJECT);
        composite!(SetObject, SetObjectSchema::OBJECT);
        composite!(WeakSetObject, WeakSetObjectSchema::OBJECT);
        composite!(PromiseObject, PromiseObjectSchema::OBJECT);
        self.emit_branch_to_target(other, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let object = s
            .reserve_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        header.clear(f);
        object
    }

    /// The ordinary header of a prototype that is an actual ordinary
    /// object, function or Array; any other prototype, or null, branches to
    /// `miss`. Each keeps all its named own properties, other than an
    /// Array's indices and "length", in that header.
    fn emit_prototype_header_or_branch(
        &mut self,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        miss: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject> {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let other = self.open_frame(ControlFrameKind::Block, f);
        header.replace(
            prototype
                .sealed_subtype_or_branch::<OrdinaryObject>(s, self.local_branch_label(other), f)
                .nullable(),
            f,
        );
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let other = self.open_frame(ControlFrameKind::Block, f);
        let callable = s.reserve_gc_local(f).initialize(
            prototype.sealed_subtype_or_branch::<FunctionObject>(
                s,
                self.local_branch_label(other),
                f,
            ),
            f,
        );
        header.replace(
            s.field(FunctionObjectSchema::OBJECT)
                .read(&callable, s, f)
                .reference()
                .nullable(),
            f,
        );
        callable.clear(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let array = s.reserve_gc_local(f).initialize(
            prototype.sealed_subtype_or_branch::<ArrayObject>(s, self.local_branch_label(miss), f),
            f,
        );
        header.replace(
            s.field(ArrayObjectSchema::OBJECT)
                .read(&array, s, f)
                .reference()
                .nullable(),
            f,
        );
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let object = s
            .reserve_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        header.clear(f);
        object
    }

    /// The ordinary header of a Realm's String or Number prototype, a
    /// primitive wrapper or an ordinary object; anything else branches to
    /// `miss`.
    fn emit_realm_prototype_header_or_branch(
        &mut self,
        prototype: &ValueLocals,
        miss: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject> {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let other = self.open_frame(ControlFrameKind::Block, f);
        let boxed = s.reserve_gc_local(f).initialize(
            prototype.sealed_reference_or_branch::<PrimitiveBox>(
                s,
                self.local_branch_label(other),
                f,
            ),
            f,
        );
        header.replace(
            s.field(PrimitiveBoxSchema::OBJECT)
                .read(&boxed, s, f)
                .reference()
                .nullable(),
            f,
        );
        boxed.clear(f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        header.replace(
            prototype
                .sealed_reference_or_branch::<OrdinaryObject>(s, self.local_branch_label(miss), f)
                .nullable(),
            f,
        );
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let object = s
            .reserve_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        header.clear(f);
        object
    }

    /// The storage as `layout`, after branching to `other` for any other
    /// storage.
    fn emit_compact_storage_or_branch(
        &mut self,
        layout: CompactLayout,
        base: &GcLocal<ObjectPropertyStorage>,
        other: ControlTarget,
        f: &mut Function,
    ) -> CompactStorage {
        let s = self.runtime_schema();
        let label = self.local_branch_label(other);
        match layout {
            CompactLayout::Array => CompactStorage::Array(
                s.reserve_gc_local(f).initialize(
                    base.sealed_nested_subtype_or_branch::<
                        CompactDataStorage,
                        ArrayCompactDataStorage,
                    >(s, label, f),
                    f,
                ),
            ),
            CompactLayout::Inline => CompactStorage::Inline(
                s.reserve_gc_local(f).initialize(
                    base.sealed_nested_subtype_or_branch::<
                        CompactDataStorage,
                        InlineCompactDataStorage,
                    >(s, label, f),
                    f,
                ),
            ),
        }
    }

    /// Branches to `miss` unless the dictionary has no own property: every
    /// live entry sits below the insertion extent.
    fn emit_empty_dictionary_or_branch(
        &self,
        dictionary: &GcLocal<OrdinaryPropertyStorage>,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(dictionary, s, f);
        self.emit_branch_if_to_target(miss, f);
    }

    /// Branches to `miss` unless the record's prototype rule is the live
    /// receiver's: its kind, and the compact `shape` that proves an
    /// ordinary receiver lacks the key (None for an empty dictionary and
    /// for primitives).
    fn emit_receiver_rule_or_branch(
        &self,
        record: &GcLocal<OwnDataReadCache>,
        kind: HolderReceiverKind,
        shape: Option<&GcLocal<CompactDataShape>>,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        s.field(OwnDataReadCacheSchema::RECEIVER_KIND)
            .read(record, s, f);
        f.instruction(&Instruction::I32Const(kind.word()));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(miss, f);
        s.field(OwnDataReadCacheSchema::RECEIVER_SHAPE)
            .read(record, s, f);
        match shape {
            Some(shape) => {
                let _ = shape.load(s, f);
                f.instruction(&Instruction::RefEq);
            }
            None => {
                f.instruction(&Instruction::RefIsNull);
            }
        }
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
    }

    /// Takes the receiver's live [[Prototype]] to `edge`.
    fn emit_prototype_edge(
        &self,
        object: &GcLocal<OrdinaryObject>,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        edge: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        prototype.replace(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(object, s, f)
                .reference(),
            f,
        );
        self.emit_branch_to_target(edge, f);
    }

    /// Returns the value at the record's place on `holder`, the receiver's
    /// prototype, or branches to `miss`.
    #[allow(clippy::too_many_arguments)]
    fn emit_recorded_place_hit(
        &mut self,
        record: &GcLocal<OwnDataReadCache>,
        holder: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        miss: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let holder_base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(holder, s, f)
                .reference(),
            f,
        );
        let slot = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::HOLDER_SLOT)
            .read(record, s, f)
            .store(slot, f);
        found.set_constant(0, f);
        // An intrinsic layout is immutable: storage at the recorded address
        // has the same rows, and keeps each materialized value in its group.
        let other = self.open_frame(ControlFrameKind::Block, f);
        let intrinsic = s.reserve_gc_local(f).initialize(
            holder_base.sealed_subtype_or_branch::<IntrinsicPropertyStorage>(
                s,
                self.local_branch_label(other),
                f,
            ),
            f,
        );
        s.field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(&intrinsic, s, f);
        s.field(OwnDataReadCacheSchema::HOLDER_LAYOUT)
            .read(record, s, f);
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(miss, f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.emit_materialized_intrinsic_group(&intrinsic, slot, value, found, f);
        f.instruction(&Instruction::Else);
        self.emit_recorded_absence(holder, slot, value, found, f)?;
        f.instruction(&Instruction::End);
        intrinsic.clear(f);
        self.emit_found_return_or_miss(found, value, miss, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // A compact shape is immutable: the same shape has the key at the
        // same slot, or lacks it.
        for layout in COMPACT_LAYOUTS {
            let other = self.open_frame(ControlFrameKind::Block, f);
            let storage = self.emit_compact_storage_or_branch(layout, &holder_base, other, f);
            let _ = storage.shape(s, f);
            s.field(OwnDataReadCacheSchema::HOLDER_SHAPE)
                .read(record, s, f);
            f.instruction(&Instruction::RefEq);
            f.instruction(&Instruction::I32Eqz);
            self.emit_branch_if_to_target(miss, f);
            slot.load(f);
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32GeS);
            f.instruction(&Instruction::If(BlockType::Empty));
            match &storage {
                CompactStorage::Array(array) => {
                    self.emit_array_compact_value_at(array, slot, value, f)
                }
                CompactStorage::Inline(inline) => {
                    self.emit_inline_compact_value_at(inline, slot, value, f)
                }
            }
            found.set_constant(1, f);
            f.instruction(&Instruction::Else);
            self.emit_recorded_absence(holder, slot, value, found, f)?;
            f.instruction(&Instruction::End);
            storage.clear(f);
            self.emit_found_return_or_miss(found, value, miss, f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        // A dictionary entry is confirmed by its own key at the position.
        let dictionary = s.reserve_gc_local(f).initialize(
            holder_base.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(
                s,
                self.local_branch_label(miss),
                f,
            ),
            f,
        );
        self.emit_dictionary_position_read_with(
            &dictionary,
            slot,
            key,
            value,
            found,
            RecipeValues::Decline,
            f,
        );
        dictionary.clear(f);
        self.emit_found_return_or_miss(found, value, miss, f);
        s.release_i32_local(slot, f);
        holder_base.clear(f);
        Ok(())
    }

    fn emit_found_return_or_miss(
        &self,
        found: I32Local,
        value: &ValueLocals,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        found.load(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        emit_return_normal(value, f);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(miss, f);
    }
}

/// Returns `value` as the helper's normal completion.
fn emit_return_normal(value: &ValueLocals, f: &mut Function) {
    value.emit(f);
    f.instruction(&Instruction::I32Const(crate::COMPLETION_KIND_NORMAL as i32));
    f.instruction(&Instruction::I32Const(0));
    f.instruction(&Instruction::Return);
}

/// Returns the normal completion `true` of a successful [[Set]].
fn emit_return_true(schema: &RuntimeSchema, f: &mut Function) {
    let stored = schema.reserve_value_local(f);
    stored.set_scalar(crate::gc_types::ScalarValue::Boolean(true), f);
    emit_return_normal(&stored, f);
    stored.clear(f);
}
