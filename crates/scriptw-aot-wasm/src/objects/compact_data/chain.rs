//! A call-free walk along a prototype chain of objects that keep every named
//! own property in their ordinary header.
//!
//! Reads that no site record answers, computed-key reads and the chain proof
//! of computed-key adds need what OrdinaryGet observes along the chain: the
//! first object that has the key, or that none does. The walk asks each
//! object's live storage directly and retains nothing between uses:
//!
//! - a compact shape is scanned, or probed through its derived key index,
//!   comparing keys by identity; a different key whose prepared hash differs
//!   cannot have equal contents, and an equal hash declines;
//! - a dictionary first tries the position the String remembers, then probes
//!   its key index, whose empty bucket ends every probe for the key; an entry
//!   with an equal hash and another String object declines, and a Symbol that
//!   has never been hashed is the key of no entry;
//! - an intrinsic layout is immutable static data: its String index proves
//!   absence the same way, and a layout without Symbol rows lacks every
//!   Symbol; a row that may match declines, leaving its value to the general
//!   Get, which materializes it.
//!
//! Only ordinary objects, functions, Arrays and primitive wrappers are walked
//! (and, as the first object, the other composites whose named properties all
//! live in their header). Arrays and String wrappers own their indices and
//! "length" outside the header, so their headers answer only for a key that
//! can be neither. Proxies and every other exotic object decline, as do
//! accessors and values still held as recipes. Nothing can run between the
//! storage reads, so the answer is exactly OrdinaryGet's.
use super::*;
use crate::emit::ControlTarget;
use crate::gc_types::{
    ArgumentsObject, ArgumentsObjectSchema, ArrayObject, ArrayObjectSchema, FunctionObject,
    FunctionObjectSchema, JavaScriptObject, MapObject, MapObjectSchema, PrimitiveBox,
    PrimitiveBoxSchema, PromiseObject, PromiseObjectSchema, SetObject, SetObjectSchema,
    SymbolValue, SymbolValueSchema, WeakMapObject, WeakMapObjectSchema, WeakSetObject,
    WeakSetObjectSchema,
};

/// What a walk may answer.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChainAnswer {
    /// A data value found on any object of the chain, or undefined when every
    /// object lacks the key.
    Get,
    /// Only that every object of the chain lacks the key.
    Absence,
}

/// The key of a walk.
#[derive(Clone, Copy)]
pub(crate) enum ChainKey<'a> {
    /// A String and its raw content hash, prepared (nonzero) by the caller.
    String {
        name: &'a GcLocal<StringValue>,
        hash: I64Local,
    },
    Symbol(&'a GcLocal<SymbolValue>),
}

/// The raw content hash of an ASCII literal, as `emit_string_key_hash`
/// computes it over UTF-16 code units.
const fn raw_ascii_hash(text: &str) -> i64 {
    let bytes = text.as_bytes();
    let mut hash = 14695981039346656037u64;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(1099511628211);
        index += 1;
    }
    hash as i64
}

/// "length", which Arrays and String wrappers own outside their header.
const LENGTH_HASH: i64 = raw_ascii_hash("length");

/// Bytes per intrinsic layout row; the raw key hash is its last word.
const LAYOUT_ROW_BYTES: i32 = crate::data::RPropertyLayoutRows::ROW_BYTES as i32;
const LAYOUT_ROW_HASH_OFFSET: u64 = 32;

impl FunctionBuilder<'_> {
    /// Leaves on the stack whether a String key cannot be an array index:
    /// its first code unit is no decimal digit, as every array index's is.
    pub(crate) fn emit_index_free_key_i32(&self, name: &GcLocal<StringValue>, f: &mut Function) {
        let s = self.runtime_schema();
        let units = s.reserve_gc_local(f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(name, s, f)
                .reference(),
            f,
        );
        let first = s.reserve_i32_local(f);
        first.set_constant(0, f);
        s.array_type::<CodeUnitArray>().length(&units, s, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        let zero = s.reserve_i32_local(f);
        zero.set_constant(0, f);
        s.array_type::<CodeUnitArray>()
            .read(&units, zero, s, f)
            .store(first, f);
        s.release_i32_local(zero, f);
        f.instruction(&Instruction::End);
        units.clear(f);
        first.load(f);
        f.instruction(&Instruction::I32Const(i32::from(b'0')));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32GeU);
        s.release_i32_local(first, f);
    }

    /// Leaves on the stack whether the headers of Arrays and String wrappers
    /// answer for this String key: it cannot be an array index and its hash
    /// differs from "length"'s.
    pub(crate) fn emit_header_named_key_i32(
        &self,
        name: &GcLocal<StringValue>,
        hash: I64Local,
        f: &mut Function,
    ) {
        self.emit_index_free_key_i32(name, f);
        hash.load(f);
        f.instruction(&Instruction::I64Const(LENGTH_HASH));
        f.instruction(&Instruction::I64Ne);
        f.instruction(&Instruction::I32And);
    }

    /// A Get whose String or Symbol key the walk answers from an object's
    /// live chain: sets `result` and branches to `done`. Every other case,
    /// including a key that still needs conversion, falls through without
    /// effect.
    pub(crate) fn emit_computed_read_walk(
        &mut self,
        target: &ValueLocals,
        key: &ValueLocals,
        result: &CompletionLocals,
        done: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let value = s.reserve_value_local(f);
        let index_free = s.reserve_i32_local(f);
        let named = s.reserve_i32_local(f);
        let skip = self.open_frame(ControlFrameKind::Block, f);
        let symbol = self.open_frame(ControlFrameKind::Block, f);
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(symbol, f);
        let name = s
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<StringValue>(s, f), f);
        let hash = s.reserve_i64_local(f);
        self.emit_string_key_hash(&name, hash, f);
        self.emit_index_free_key_i32(&name, f);
        index_free.store(f);
        self.emit_header_named_key_i32(&name, hash, f);
        named.store(f);
        let header = self.emit_walk_receiver_header_or_branch(target, index_free, named, skip, f);
        self.emit_chain_walk(
            &header,
            ChainKey::String { name: &name, hash },
            named,
            ChainAnswer::Get,
            &value,
            skip,
            f,
        );
        result.set_normal(&value, f);
        self.emit_branch_to_target(done, f);
        header.clear(f);
        s.release_i64_local(hash, f);
        name.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        key.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(skip, f);
        let symbol = s
            .reserve_gc_local(f)
            .initialize(key.cast_reference::<SymbolValue>(s, f), f);
        // A Symbol is never an index or "length".
        index_free.set_constant(1, f);
        named.set_constant(1, f);
        let header = self.emit_walk_receiver_header_or_branch(target, index_free, named, skip, f);
        self.emit_chain_walk(
            &header,
            ChainKey::Symbol(&symbol),
            named,
            ChainAnswer::Get,
            &value,
            skip,
            f,
        );
        result.set_normal(&value, f);
        self.emit_branch_to_target(done, f);
        header.clear(f);
        symbol.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(named, f);
        s.release_i32_local(index_free, f);
        value.clear(f);
    }

    /// The ordinary header of a receiver whose named own properties all live
    /// there: an ordinary object, a function, a Map, WeakMap, Set, WeakSet or
    /// Promise; an arguments object, whose header holds every property but
    /// its indices, when `index_free` is set; and an Array or primitive
    /// wrapper when `named` is set as well. Every other value branches to
    /// `other`.
    pub(crate) fn emit_walk_receiver_header_or_branch(
        &mut self,
        value: &ValueLocals,
        index_free: I32Local,
        named: I32Local,
        other: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject> {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Block, f);
        header.replace(
            value
                .sealed_reference_or_branch::<OrdinaryObject>(s, self.local_branch_label(next), f)
                .nullable(),
            f,
        );
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        macro_rules! composite {
            ($layout:ty, $field:expr) => {{
                let next = self.open_frame(ControlFrameKind::Block, f);
                let composite = s.reserve_gc_local(f).initialize(
                    value.sealed_reference_or_branch::<$layout>(
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
        composite!(FunctionObject, FunctionObjectSchema::OBJECT);
        composite!(MapObject, MapObjectSchema::OBJECT);
        composite!(WeakMapObject, WeakMapObjectSchema::OBJECT);
        composite!(SetObject, SetObjectSchema::OBJECT);
        composite!(WeakSetObject, WeakSetObjectSchema::OBJECT);
        composite!(PromiseObject, PromiseObjectSchema::OBJECT);
        index_free.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(other, f);
        composite!(ArgumentsObject, ArgumentsObjectSchema::OBJECT);
        named.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(other, f);
        composite!(ArrayObject, ArrayObjectSchema::OBJECT);
        composite!(PrimitiveBox, PrimitiveBoxSchema::OBJECT);
        self.emit_branch_to_target(other, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let object = s
            .reserve_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        header.clear(f);
        object
    }

    /// The ordinary header of a prototype that is an ordinary object or a
    /// function, or an Array or primitive wrapper when `named` is set. Every
    /// other object branches to `other`.
    pub(super) fn emit_walk_prototype_header_or_branch(
        &mut self,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        named: I32Local,
        other: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject> {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Block, f);
        header.replace(
            prototype
                .sealed_subtype_or_branch::<OrdinaryObject>(s, self.local_branch_label(next), f)
                .nullable(),
            f,
        );
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        macro_rules! composite {
            ($layout:ty, $field:expr) => {{
                let next = self.open_frame(ControlFrameKind::Block, f);
                let composite = s.reserve_gc_local(f).initialize(
                    prototype.sealed_subtype_or_branch::<$layout>(
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
        composite!(FunctionObject, FunctionObjectSchema::OBJECT);
        named.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(other, f);
        composite!(ArrayObject, ArrayObjectSchema::OBJECT);
        composite!(PrimitiveBox, PrimitiveBoxSchema::OBJECT);
        self.emit_branch_to_target(other, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let object = s
            .reserve_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        header.clear(f);
        object
    }

    /// Walks from the object whose header is `first` along [[Prototype]].
    /// Falls through with `value` holding the answer (undefined when the
    /// chain ends without the key); branches to `decline` for every case it
    /// cannot prove, before any effect.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_chain_walk(
        &mut self,
        first: &GcLocal<OrdinaryObject>,
        key: ChainKey<'_>,
        named: I32Local,
        answer: ChainAnswer,
        value: &ValueLocals,
        decline: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize(first.load(s, f).nullable(), f);
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        let answered = self.open_frame(ControlFrameKind::Block, f);
        let hop = self.open_frame(ControlFrameKind::Loop, f);
        let current = s
            .reserve_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        let found = match answer {
            ChainAnswer::Get => answered,
            ChainAnswer::Absence => decline,
        };
        self.emit_header_own_lookup(&current, key, value, found, decline, f);
        // This object lacks the key.
        prototype.replace(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(&current, s, f)
                .reference(),
            f,
        );
        current.clear(f);
        prototype.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        value.set_undefined(f);
        self.emit_branch_to_target(answered, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let next = self.emit_walk_prototype_header_or_branch(&prototype, named, decline, f);
        header.replace(next.load(s, f).nullable(), f);
        next.clear(f);
        self.emit_branch_to_target(hop, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        prototype.clear(f);
        header.clear(f);
    }

    /// One object's own lookup: branches to `found` with its data value in
    /// `value`, falls through when the object lacks the key, and branches to
    /// `decline` otherwise.
    fn emit_header_own_lookup(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: ChainKey<'_>,
        value: &ValueLocals,
        found: ControlTarget,
        decline: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(object, s, f)
                .reference(),
            f,
        );
        let absent = self.open_frame(ControlFrameKind::Block, f);
        // Compact storage, either sealed layout. Shapes hold String keys only.
        let other = self.open_frame(ControlFrameKind::Block, f);
        let inline = s.reserve_gc_local(f).initialize(
            base.sealed_nested_subtype_or_branch::<CompactDataStorage, InlineCompactDataStorage>(
                s,
                self.local_branch_label(other),
                f,
            ),
            f,
        );
        if let ChainKey::String { name, hash } = key {
            let shape = s.reserve_gc_local(f).initialize(
                s.field(InlineCompactDataStorageSchema::SHAPE)
                    .read(&inline, s, f)
                    .reference(),
                f,
            );
            let slot = s.reserve_i32_local(f);
            self.emit_walk_shape_slot_or_branch(&shape, name, hash, slot, absent, decline, f);
            shape.clear(f);
            self.emit_inline_compact_value_at(&inline, slot, value, f);
            s.release_i32_local(slot, f);
            self.emit_branch_to_target(found, f);
        }
        inline.clear(f);
        self.emit_branch_to_target(absent, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let other = self.open_frame(ControlFrameKind::Block, f);
        let array = s.reserve_gc_local(f).initialize(
            base.sealed_nested_subtype_or_branch::<CompactDataStorage, ArrayCompactDataStorage>(
                s,
                self.local_branch_label(other),
                f,
            ),
            f,
        );
        if let ChainKey::String { name, hash } = key {
            let shape = s.reserve_gc_local(f).initialize(
                s.field(ArrayCompactDataStorageSchema::SHAPE)
                    .read(&array, s, f)
                    .reference(),
                f,
            );
            let slot = s.reserve_i32_local(f);
            self.emit_walk_shape_slot_or_branch(&shape, name, hash, slot, absent, decline, f);
            shape.clear(f);
            self.emit_array_compact_value_at(&array, slot, value, f);
            s.release_i32_local(slot, f);
            self.emit_branch_to_target(found, f);
        }
        array.clear(f);
        self.emit_branch_to_target(absent, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // A dictionary.
        let other = self.open_frame(ControlFrameKind::Block, f);
        let dictionary = s.reserve_gc_local(f).initialize(
            base.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(
                s,
                self.local_branch_label(other),
                f,
            ),
            f,
        );
        let entry = s
            .reserve_gc_local::<PropertyEntry, Nullable>(f)
            .initialize_null(s, f);
        self.emit_walk_dictionary_entry_or_branch(&dictionary, key, &entry, absent, decline, f);
        dictionary.clear(f);
        let present = s
            .reserve_gc_local(f)
            .initialize(entry.load(s, f).require_non_null(f), f);
        entry.clear(f);
        let descriptor = s.reserve_gc_local(f).initialize(
            s.field(PropertyEntrySchema::DESCRIPTOR)
                .read(&present, s, f)
                .reference(),
            f,
        );
        present.clear(f);
        s.field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, s, f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        self.emit_branch_if_to_target(decline, f);
        let data = s.reserve_gc_local(f).initialize(
            s.field(PropertyDescriptorSchema::VALUE)
                .read(&descriptor, s, f)
                .reference(),
            f,
        );
        descriptor.clear(f);
        // A value still held as a recipe is left to the general Get.
        let stored = s.reserve_gc_local(f).initialize(
            data.sealed_subtype_or_branch::<StoredValue>(s, self.local_branch_label(decline), f),
            f,
        );
        data.clear(f);
        self.emit_stored_value_to_locals(&stored, value, f);
        stored.clear(f);
        self.emit_branch_to_target(found, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // An immutable intrinsic layout; any other storage declines.
        let intrinsic = s.reserve_gc_local(f).initialize(
            base.sealed_subtype_or_branch::<IntrinsicPropertyStorage>(
                s,
                self.local_branch_label(decline),
                f,
            ),
            f,
        );
        self.emit_walk_layout_absence_or_branch(&intrinsic, key, decline, f);
        intrinsic.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        base.clear(f);
    }

    /// The slot of `name` in an immutable shape: falls through with `slot`
    /// set when the shape holds this very String object, branches to
    /// `absent` when it provably holds no equal key, and to `decline` when a
    /// key with an equal (or unprepared) hash would need its contents read.
    /// A shape with three or more keys is probed through its index, which a
    /// shape builds on its first complete probe; without it, `decline`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_walk_shape_slot_or_branch(
        &mut self,
        shape: &GcLocal<CompactDataShape>,
        name: &GcLocal<StringValue>,
        hash: I64Local,
        slot: I32Local,
        absent: ControlTarget,
        decline: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let keys = s.reserve_gc_local(f).initialize(
            s.field(CompactDataShapeSchema::KEYS)
                .read(shape, s, f)
                .reference(),
            f,
        );
        let length = s.reserve_i32_local(f);
        s.array_type::<CompactShapeKeys>().length(&keys, s, f);
        length.store(f);
        let candidate = s
            .reserve_gc_local::<StringValue, Nullable>(f)
            .initialize_null(s, f);
        let matched = self.open_frame(ControlFrameKind::Block, f);
        length.load(f);
        f.instruction(&Instruction::I32Const(INDEXED_PROBE_MIN_KEYS));
        f.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, f);
        let derived = s
            .reserve_gc_local::<OrdinaryPropertyIndex, Nullable>(f)
            .initialize(
                s.field(CompactDataShapeSchema::INDEX)
                    .read(shape, s, f)
                    .reference(),
                f,
            );
        let index = s.reserve_gc_local(f).initialize(
            derived.non_null_or_branch(self.local_branch_label(decline), f),
            f,
        );
        derived.clear(f);
        let mask = s.reserve_i32_local(f);
        s.array_type::<OrdinaryPropertyIndex>().length(&index, s, f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        mask.store(f);
        let bucket = s.reserve_i32_local(f);
        hash.load(f);
        f.instruction(&Instruction::I32WrapI64);
        mask.load(f);
        f.instruction(&Instruction::I32And);
        bucket.store(f);
        let probe = self.open_frame(ControlFrameKind::Loop, f);
        s.array_type::<OrdinaryPropertyIndex>()
            .read(&index, bucket, s, f)
            .store(slot, f);
        slot.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(absent, f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        slot.store(f);
        self.emit_walk_shape_key_step(&keys, slot, &candidate, name, hash, matched, decline, f);
        bucket.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        mask.load(f);
        f.instruction(&Instruction::I32And);
        bucket.store(f);
        self.emit_branch_to_target(probe, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        s.release_i32_local(bucket, f);
        s.release_i32_local(mask, f);
        index.clear(f);
        f.instruction(&Instruction::Else);
        slot.set_constant(0, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        slot.load(f);
        length.load(f);
        f.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(absent, f);
        self.emit_walk_shape_key_step(&keys, slot, &candidate, name, hash, matched, decline, f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        slot.store(f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        // Both probes leave only by a branch.
        self.emit_branch_to_target(decline, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        candidate.clear(f);
        s.release_i32_local(length, f);
        keys.clear(f);
    }

    /// Compares the shape key at `slot` with `name`: identity branches to
    /// `matched`; a prepared hash that differs continues; anything else
    /// declines.
    #[allow(clippy::too_many_arguments)]
    fn emit_walk_shape_key_step(
        &self,
        keys: &GcLocal<CompactShapeKeys>,
        slot: I32Local,
        candidate: &GcLocal<StringValue, Nullable>,
        name: &GcLocal<StringValue>,
        hash: I64Local,
        matched: ControlTarget,
        decline: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        candidate.replace(
            s.array_type::<CompactShapeKeys>()
                .read(keys, slot, s, f)
                .reference()
                .nullable(),
            f,
        );
        let _ = candidate.load(s, f);
        let _ = name.load(s, f);
        f.instruction(&Instruction::RefEq);
        self.emit_branch_if_to_target(matched, f);
        let present = s
            .reserve_gc_local(f)
            .initialize(candidate.load(s, f).require_non_null(f), f);
        let prepared = s.reserve_i64_local(f);
        s.field(StringValueSchema::RAW_KEY_HASH)
            .read(&present, s, f)
            .store_i64(prepared, f);
        present.clear(f);
        prepared.load(f);
        hash.load(f);
        f.instruction(&Instruction::I64Eq);
        prepared.load(f);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Or);
        s.release_i64_local(prepared, f);
        self.emit_branch_if_to_target(decline, f);
    }

    /// Sets `entry` to the dictionary's live entry for the key, branching to
    /// `absent` when its key index proves there is none and to `decline`
    /// when an entry with an equal hash holds another String object.
    #[allow(clippy::too_many_arguments)]
    fn emit_walk_dictionary_entry_or_branch(
        &mut self,
        dictionary: &GcLocal<OrdinaryPropertyStorage>,
        key: ChainKey<'_>,
        entry: &GcLocal<PropertyEntry, Nullable>,
        absent: ControlTarget,
        decline: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let entries = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(dictionary, s, f)
                .reference(),
            f,
        );
        let hash = s.reserve_i64_local(f);
        let position = s.reserve_i32_local(f);
        let matched = self.open_frame(ControlFrameKind::Block, f);
        let tag = match key {
            ChainKey::String { name, hash: raw } => {
                // The position this String object last held, confirmed by
                // the live entry's own key.
                s.field(StringValueSchema::ENTRY_HINT)
                    .read(name, s, f)
                    .store(position, f);
                position.load(f);
                s.field(OrdinaryPropertyStorageSchema::LENGTH)
                    .read(dictionary, s, f);
                f.instruction(&Instruction::I32LtU);
                self.open_frame(ControlFrameKind::If, f);
                entry.replace(
                    s.array_type::<PropertyTable>()
                        .read(&entries, position, s, f)
                        .reference(),
                    f,
                );
                self.emit_walk_entry_key_is_i32(entry, key, f);
                self.emit_branch_if_to_target(matched, f);
                self.pop_control(ControlFrameKind::If);
                f.instruction(&Instruction::End);
                raw.load(f);
                hash.store(f);
                WasmRuntimeValueTag::String
            }
            ChainKey::Symbol(symbol) => {
                // Every entry stores its key's hash, and a Symbol receives its
                // hash identity the first time it is hashed.
                s.field(SymbolValueSchema::KEY_HASH_ID)
                    .read(symbol, s, f)
                    .store_i64(hash, f);
                hash.load(f);
                f.instruction(&Instruction::I64Eqz);
                self.emit_branch_if_to_target(absent, f);
                WasmRuntimeValueTag::Symbol
            }
        };
        self.emit_key_hash_constant_tag_mix(tag, hash, f);
        let buckets = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryPropertyStorageSchema::INDEX)
                .read(dictionary, s, f)
                .reference(),
            f,
        );
        let mask = s.reserve_i32_local(f);
        s.array_type::<OrdinaryPropertyIndex>()
            .length(&buckets, s, f);
        mask.store(f);
        mask.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(absent, f);
        mask.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        mask.store(f);
        let bucket = s.reserve_i32_local(f);
        hash.load(f);
        f.instruction(&Instruction::I32WrapI64);
        mask.load(f);
        f.instruction(&Instruction::I32And);
        bucket.store(f);
        let probe = self.open_frame(ControlFrameKind::Loop, f);
        s.array_type::<OrdinaryPropertyIndex>()
            .read(&buckets, bucket, s, f)
            .store(position, f);
        position.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(absent, f);
        position.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        position.store(f);
        entry.replace(
            s.array_type::<PropertyTable>()
                .read(&entries, position, s, f)
                .reference(),
            f,
        );
        // A deleted entry is a null tombstone that the probe passes over.
        entry.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let present = s
            .reserve_gc_local(f)
            .initialize(entry.load(s, f).require_non_null(f), f);
        s.field(PropertyEntrySchema::HASH).read(&present, s, f);
        present.clear(f);
        hash.load(f);
        f.instruction(&Instruction::I64Eq);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_walk_entry_key_is_i32(entry, key, f);
        self.emit_branch_if_to_target(matched, f);
        if let ChainKey::String { .. } = key {
            // Another String object may have the same contents. A Symbol
            // is equal only to itself.
            self.emit_branch_to_target(decline, f);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        bucket.load(f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Add);
        mask.load(f);
        f.instruction(&Instruction::I32And);
        bucket.store(f);
        self.emit_branch_to_target(probe, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        // The probe leaves only by a branch.
        self.emit_branch_to_target(decline, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(bucket, f);
        s.release_i32_local(mask, f);
        buckets.clear(f);
        s.release_i32_local(position, f);
        s.release_i64_local(hash, f);
        entries.clear(f);
    }

    /// Leaves on the stack whether `entry` is non-null and keyed by this
    /// very String or Symbol object.
    fn emit_walk_entry_key_is_i32(
        &self,
        entry: &GcLocal<PropertyEntry, Nullable>,
        key: ChainKey<'_>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        entry.load(s, f).is_null(f);
        f.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::Else);
        let present = s
            .reserve_gc_local(f)
            .initialize(entry.load(s, f).require_non_null(f), f);
        let stored = s.reserve_gc_local(f).initialize(
            s.field(PropertyEntrySchema::KEY)
                .read(&present, s, f)
                .reference(),
            f,
        );
        present.clear(f);
        s.field(StoredValueSchema::REFERENCE).read(&stored, s, f);
        stored.clear(f);
        match key {
            ChainKey::String { name, .. } => {
                let _ = name.load(s, f);
            }
            ChainKey::Symbol(symbol) => {
                let _ = symbol.load(s, f);
            }
        }
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::End);
    }

    /// Whether the call-free probe proves that an immutable layout lacks a
    /// String or Symbol key; zero proves nothing, and the caller then asks
    /// the complete layout search.
    pub(crate) fn emit_intrinsic_layout_lacks_key_i32(
        &mut self,
        intrinsic: &GcLocal<IntrinsicPropertyStorage>,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let lacks = s.reserve_i32_local(f);
        lacks.set_constant(0, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let unproven = self.open_frame(ControlFrameKind::Block, f);
        let symbol = self.open_frame(ControlFrameKind::Block, f);
        key.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(symbol, f);
        let name = s
            .reserve_gc_local(f)
            .initialize(key.value().cast_reference::<StringValue>(s, f), f);
        let hash = s.reserve_i64_local(f);
        self.emit_string_key_hash(&name, hash, f);
        self.emit_walk_layout_absence_or_branch(
            intrinsic,
            ChainKey::String { name: &name, hash },
            unproven,
            f,
        );
        lacks.set_constant(1, f);
        self.emit_branch_to_target(done, f);
        s.release_i64_local(hash, f);
        name.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        key.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(unproven, f);
        let symbol = s
            .reserve_gc_local(f)
            .initialize(key.value().cast_reference::<SymbolValue>(s, f), f);
        self.emit_walk_layout_absence_or_branch(intrinsic, ChainKey::Symbol(&symbol), unproven, f);
        lacks.set_constant(1, f);
        self.emit_branch_to_target(done, f);
        symbol.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        lacks
    }

    /// Branches to `decline` unless the immutable layout provably lacks the
    /// key; falls through when it does.
    fn emit_walk_layout_absence_or_branch(
        &mut self,
        intrinsic: &GcLocal<IntrinsicPropertyStorage>,
        key: ChainKey<'_>,
        decline: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let address = s.reserve_i32_local(f);
        s.field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(intrinsic, s, f)
            .store(address, f);
        let row = s.reserve_i32_local(f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        match key {
            ChainKey::String { hash, .. } => {
                // Every named row is in the String index, which always keeps
                // an empty bucket. A row whose hash differs has other contents.
                let mask = s.reserve_i32_local(f);
                s.field(IntrinsicPropertyStorageSchema::INDEX_MASK)
                    .read(intrinsic, s, f)
                    .store(mask, f);
                let index = s.reserve_i32_local(f);
                s.field(IntrinsicPropertyStorageSchema::INDEX_ADDRESS)
                    .read(intrinsic, s, f)
                    .store(index, f);
                let bucket = s.reserve_i32_local(f);
                hash.load(f);
                f.instruction(&Instruction::I32WrapI64);
                mask.load(f);
                f.instruction(&Instruction::I32And);
                bucket.store(f);
                let probe = self.open_frame(ControlFrameKind::Loop, f);
                index.load(f);
                bucket.load(f);
                f.instruction(&Instruction::I32Const(4));
                f.instruction(&Instruction::I32Mul);
                f.instruction(&Instruction::I32Add);
                f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                row.store(f);
                row.load(f);
                f.instruction(&Instruction::I32Eqz);
                self.emit_branch_if_to_target(done, f);
                address.load(f);
                row.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Sub);
                f.instruction(&Instruction::I32Const(LAYOUT_ROW_BYTES));
                f.instruction(&Instruction::I32Mul);
                f.instruction(&Instruction::I32Add);
                f.instruction(&Instruction::I64Load(wasm_encoder::MemArg {
                    offset: LAYOUT_ROW_HASH_OFFSET,
                    align: 0,
                    memory_index: 0,
                }));
                hash.load(f);
                f.instruction(&Instruction::I64Eq);
                self.emit_branch_if_to_target(decline, f);
                bucket.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Add);
                mask.load(f);
                f.instruction(&Instruction::I32And);
                bucket.store(f);
                self.emit_branch_to_target(probe, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                s.release_i32_local(bucket, f);
                s.release_i32_local(index, f);
                s.release_i32_local(mask, f);
            }
            ChainKey::Symbol(_) => {
                // Symbol rows are not indexed; a layout without any lacks
                // every Symbol.
                let count = s.reserve_i32_local(f);
                s.field(IntrinsicPropertyStorageSchema::COUNT)
                    .read(intrinsic, s, f)
                    .store(count, f);
                row.set_constant(0, f);
                let scan = self.open_frame(ControlFrameKind::Loop, f);
                row.load(f);
                count.load(f);
                f.instruction(&Instruction::I32GeU);
                self.emit_branch_if_to_target(done, f);
                address.load(f);
                row.load(f);
                f.instruction(&Instruction::I32Const(LAYOUT_ROW_BYTES));
                f.instruction(&Instruction::I32Mul);
                f.instruction(&Instruction::I32Add);
                f.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                    offset: 0,
                    align: 0,
                    memory_index: 0,
                }));
                self.emit_branch_if_to_target(decline, f);
                row.load(f);
                f.instruction(&Instruction::I32Const(1));
                f.instruction(&Instruction::I32Add);
                row.store(f);
                self.emit_branch_to_target(scan, f);
                self.pop_control(ControlFrameKind::Loop);
                f.instruction(&Instruction::End);
                s.release_i32_local(count, f);
            }
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(row, f);
        s.release_i32_local(address, f);
    }
}
