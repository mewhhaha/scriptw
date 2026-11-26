//! The hit entries of static reads and assignments.
//!
//! A static site calls a small entry that answers from the site's records
//! when the live receiver proves one, and otherwise tail-calls the complete
//! miss kernel with the same operands. A read entry makes no other call and
//! registers, resolves, allocates and writes nothing.
//!
//! A read site keeps a bounded chain of records, each a complete set of
//! proofs for the site's key that stands alone. The entry finds the
//! receiver's ordinary header and current storage once, testing first the
//! layouts the first record hints at, and then asks each record in turn. Each
//! answer re-proves exactly what the miss kernel's use of the same record
//! proves: an own compact slot, or absence before a closed tail, by the
//! receiver's immutable shape; an own dictionary entry by its key at the
//! recorded position; an own intrinsic row by the immutable layout address;
//! and a place on the prototype by the receiver's absence rule and the live
//! prototype's layout, shape or entry. A proof that fails observes nothing
//! and leaves the receiver to the next record. A receiver that no record
//! answers goes to the miss kernel while the site still has learning budget,
//! and through the call-free walk from the live receiver once it has none.
//! Two receivers are first asked for the key themselves, as the walk asks
//! them: one on a dictionary that holds entries, which no shape describes,
//! and, at a site with no learning budget left, one whose compact shape no
//! record names. When that proves the receiver has no own property for the
//! key, what lies beyond it is any record's closed tail or place, re-proved
//! on the live chain.
//!
//! An assignment stores into an existing writable own compact slot, or proves
//! an add by the recorded shape transition and tail-calls the kernel that
//! extends the storage. Every other case, including a value still held as a
//! recipe or a dictionary holder whose key index cannot prove absence,
//! misses.
use super::holder::{HolderReceiverKind, RecipeValues};
#[cfg(test)]
use super::proof_trace::ProofStep;
use super::write::DictionaryAbsence;
use super::*;
use crate::emit::ControlTarget;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, FunctionObject, FunctionObjectSchema, JavaScriptObject,
    MapObject, MapObjectSchema, PrimitiveBox, PrimitiveBoxSchema, PromiseObject,
    PromiseObjectSchema, RealmRecord, ReferenceResultBlock, SetObject, SetObjectSchema,
    WeakMapObject, WeakMapObjectSchema, WeakSetObject, WeakSetObjectSchema,
};

/// A compact storage of one of its two sealed layouts.
pub(super) enum CompactStorage {
    Array(GcLocal<ArrayCompactDataStorage>),
    Inline(GcLocal<InlineCompactDataStorage>),
}

/// The sealed compact layouts, in the order an entry tests them.
const COMPACT_LAYOUTS: [CompactLayout; 2] = [CompactLayout::Array, CompactLayout::Inline];

/// The storage of a prototype that holds a record's place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HolderStorage {
    Intrinsic,
    Compact(CompactLayout),
    Dictionary,
}

/// The holder storages, in the order an entry tests them; a holder on
/// none of them misses.
const HOLDER_STORAGES: [HolderStorage; 4] = [
    HolderStorage::Intrinsic,
    HolderStorage::Compact(COMPACT_LAYOUTS[0]),
    HolderStorage::Compact(COMPACT_LAYOUTS[1]),
    HolderStorage::Dictionary,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CompactLayout {
    Array,
    Inline,
}

impl CompactStorage {
    pub(super) fn shape(
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

    pub(super) fn clear(self, f: &mut Function) {
        match self {
            Self::Array(storage) => storage.clear(f),
            Self::Inline(storage) => storage.clear(f),
        }
    }
}

/// The primitive receivers whose records name their Realm's prototype.
pub(super) const PRIMITIVE_RECEIVERS: [(
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

/// The sealed layouts whose ordinary header holds every named own property a
/// record can name, in the order the entry tests a receiver that the site's
/// hint does not place. A record never names an index or "length", which an
/// Array and a String wrapper own outside the header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReceiverLayout {
    Ordinary,
    Function,
    Array,
    PrimitiveBox,
    Map,
    WeakMap,
    Set,
    WeakSet,
    Promise,
}

pub(super) const RECEIVER_LAYOUTS: [ReceiverLayout; 9] = [
    ReceiverLayout::Ordinary,
    ReceiverLayout::Function,
    ReceiverLayout::Array,
    ReceiverLayout::PrimitiveBox,
    ReceiverLayout::Map,
    ReceiverLayout::WeakMap,
    ReceiverLayout::Set,
    ReceiverLayout::WeakSet,
    ReceiverLayout::Promise,
];

// A site's hint holds the header layout below the storage layout.
const _: () = assert!(RECEIVER_LAYOUTS.len() <= 1 << ReceiverStorage::HINT_SHIFT);

/// Every word a site's hint can hold is below this; the entry's table
/// branch has one entry for each.
const HINT_WORDS: i32 = (RECEIVER_STORAGES.len() as i32) << ReceiverStorage::HINT_SHIFT;

/// The (header, storage) layouts a hint sends straight to their casts:
/// every pair for which some section holds proofs.
fn hinted_pairs() -> Vec<(ReceiverLayout, ReceiverStorage)> {
    RECEIVER_LAYOUTS
        .into_iter()
        .flat_map(|layout| {
            RECEIVER_STORAGES
                .into_iter()
                .filter(move |storage| storage.has_proofs(layout.class().proofs()))
                .map(move |storage| (layout, storage))
        })
        .collect()
}

impl ReceiverLayout {
    /// The word of a site's hint for this layout.
    pub(super) const fn word(self) -> i32 {
        self as i32
    }

    /// The word of a site's hint for this layout on `storage`.
    const fn hint_word(self, storage: ReceiverStorage) -> i32 {
        self.word() | storage.word() << ReceiverStorage::HINT_SHIFT
    }

    const fn class(self) -> ReceiverClass {
        match self {
            Self::Ordinary => ReceiverClass::Ordinary,
            Self::Function => ReceiverClass::Function,
            Self::Array
            | Self::PrimitiveBox
            | Self::Map
            | Self::WeakMap
            | Self::Set
            | Self::WeakSet
            | Self::Promise => ReceiverClass::Composite,
        }
    }
}

/// The sealed storage layouts of an ordinary header, in the order the entry
/// tests a storage that the site's hint does not place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ReceiverStorage {
    Array,
    Inline,
    Dictionary,
    Intrinsic,
}

pub(super) const RECEIVER_STORAGES: [ReceiverStorage; 4] = [
    ReceiverStorage::Array,
    ReceiverStorage::Inline,
    ReceiverStorage::Dictionary,
    ReceiverStorage::Intrinsic,
];

impl ReceiverStorage {
    /// A site's hint holds the storage layout above the header layout.
    pub(super) const HINT_SHIFT: u32 = 4;

    pub(super) const fn word(self) -> i32 {
        self as i32
    }

    /// Whether a section holds proofs for this storage under `proofs`: no
    /// record names a receiver rule for intrinsic storage.
    const fn has_proofs(self, proofs: Proofs) -> bool {
        !matches!((self, proofs), (Self::Intrinsic, Proofs::Composite))
    }
}

/// Which of a record's proofs a receiver's header may use.
#[derive(Clone, Copy)]
enum ReceiverClass {
    /// An actual ordinary object: its own records and its absence rule,
    /// absence before a closed tail included.
    Ordinary,
    /// A function: its own records and its absence rule.
    Function,
    /// Any other layout: only its absence rule.
    Composite,
}

impl ReceiverClass {
    const fn word(self) -> i32 {
        match self {
            Self::Ordinary => 0,
            Self::Function => 1,
            Self::Composite => 2,
        }
    }

    const fn proofs(self) -> Proofs {
        match self {
            Self::Ordinary | Self::Function => Proofs::Object,
            Self::Composite => Proofs::Composite,
        }
    }
}

/// The proofs a storage section emits for the receivers it admits; within
/// an object section, the receiver's `ReceiverClass` still tells an
/// ordinary object from a function.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Proofs {
    /// An actual ordinary object or a function: its own records, its
    /// absence rule and its probed own properties.
    Object,
    /// Any other layout: only its absence rule.
    Composite,
}

/// One of a pair for each kind of `Proofs`.
struct ByProofs<T> {
    object: T,
    composite: T,
}

impl<T> ByProofs<T> {
    fn at(&self, proofs: Proofs) -> &T {
        match proofs {
            Proofs::Object => &self.object,
            Proofs::Composite => &self.composite,
        }
    }
}

/// The blocks that hand each storage section its storage.
struct StorageSections {
    intrinsic: ReferenceResultBlock<IntrinsicPropertyStorage>,
    dictionary: ByProofs<ReferenceResultBlock<OrdinaryPropertyStorage>>,
    inline: ByProofs<ReferenceResultBlock<InlineCompactDataStorage>>,
    array: ByProofs<ReferenceResultBlock<ArrayCompactDataStorage>>,
}

/// What every section of a read entry shares: the site's first record, its
/// key, the value and found word it answers with, and the label for a
/// receiver that no record answers.
#[derive(Clone, Copy)]
struct ReadReceiver<'a> {
    head: &'a GcLocal<OwnDataReadCache>,
    name: &'a GcLocal<StringValue>,
    key: &'a PropertyKeyLocals,
    value: &'a ValueLocals,
    found: I32Local,
    exhausted: ControlTarget,
}

impl FunctionBuilder<'_> {
    pub(crate) fn compile_cached_property_read_helper(&mut self) -> Result<Function, EmitError> {
        proof_step!(ProofStep::ReadEntry);
        let mut body = self.begin_helper_body(RuntimeHelperId::CachedPropertyRead);
        let p = self
            .helper_parameters::<crate::runtime_helpers::CachedPropertyReadParameters>(&mut body);
        let f = &mut body;
        let s = self.runtime_schema();
        self.emit_read_entry(&p.target, &p.record, f)?;
        s.return_call_helper(
            crate::runtime_helpers::CachedPropertyReadMissArguments::new(
                &p.target,
                &p.record,
                self.current_environment(),
                self.current_program(),
            ),
            self.runtime_helper_base()?,
            f,
        );
        p.release(f);
        f.instruction(&Instruction::End);
        proof_step!(ProofStep::ReadEntryEnd);
        Ok(self.finish_function(body))
    }

    /// The read entry's body: returns the read's value when a record of the
    /// site's chain proves it for the live `target`, or when the call-free
    /// walk answers a site that has no learning budget left, and otherwise
    /// falls through, having observed and written nothing, for the caller's
    /// tail call to the miss kernel.
    fn emit_read_entry(
        &mut self,
        target: &ValueLocals,
        record: &GcLocal<OwnDataReadCache>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // Every proof reads the record: one non-null copy is checked once.
        let head = s
            .reserve_non_null_gc_local(f)
            .initialize(record.load(s, f), f);
        let name = s.reserve_non_null_gc_local(f).initialize(
            s.field(OwnDataReadCacheSchema::KEY)
                .read(&head, s, f)
                .reference(),
            f,
        );
        let key = PropertyKeyLocals::from_string(s, &name, f);
        let value = s.reserve_value_local(f);
        let found = s.reserve_i32_local(f);
        let miss = self.open_frame(ControlFrameKind::Block, f);
        // The miss kernel registers a site's first record on its first use.
        s.field(OwnDataReadCacheSchema::SLOT).read(&head, s, f);
        f.instruction(&Instruction::I32Const(super::UNVISITED_SLOT));
        f.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(miss, f);
        // No record of the site answers this receiver.
        let exhausted = self.open_frame(ControlFrameKind::Block, f);
        let primitives = PRIMITIVE_RECEIVERS.map(|receiver| {
            let case = self.open_frame(ControlFrameKind::Block, f);
            (receiver, case)
        });
        for ((tag, _, _), case) in primitives.iter().rev() {
            target.tag().load(f);
            f.instruction(&Instruction::I32Const(*tag as i32));
            f.instruction(&Instruction::I32Eq);
            self.emit_branch_if_to_target(*case, f);
        }
        let receiver = ReadReceiver {
            head: &head,
            name: &name,
            key: &key,
            value: &value,
            found,
            exhausted,
        };
        self.emit_object_receiver_read(target, &receiver, f)?;

        // A String or Number has no own property for a recorded key: its own
        // keys are indices and "length", which never get a record. ToObject
        // would box it with the current Realm's prototype, which holds the
        // place.
        for ((_, kind, slot), _) in primitives.into_iter().rev() {
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            let holder =
                self.emit_primitive_receiver_prototype_header_or_branch(slot, exhausted, f);
            self.emit_record_scan(&head, f, |builder, candidate, next, f| {
                builder.emit_receiver_rule_or_branch(candidate, kind, None, next, f);
                builder.emit_recorded_place_hit(
                    candidate,
                    &holder,
                    &key,
                    &value,
                    found,
                    next,
                    ReadAnswer::Return,
                    f,
                )
            })?;
            holder.clear(f);
            self.emit_branch_to_target(exhausted, f);
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // The site still learns: the miss kernel records this receiver.
        s.field(OwnDataReadCacheSchema::BUDGET).read(&head, s, f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GtS);
        self.emit_branch_if_to_target(miss, f);
        self.emit_static_read_walk(target, &name, &value, miss, f);
        emit_return_normal(&value, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(found, f);
        value.clear(f);
        key.clear(f);
        name.clear(f);
        head.clear(f);
        Ok(())
    }

    /// The read from a receiver with one of the sealed header layouts: its
    /// ordinary header and its storage are classified, and that storage's
    /// proofs ask each record in turn. Returns the value, or branches to
    /// the receiver's `exhausted`; nothing falls through.
    ///
    /// The site's hint selects, by one table branch, the one header cast
    /// and the one storage cast the site last met; a receiver either cast
    /// rejects is classified by every layout in turn. Each storage has one
    /// section of proofs for objects and functions and one for the other
    /// layouts, which may use only their absence rule; the cast that
    /// enters a section hands it the storage as a non-null reference.
    fn emit_object_receiver_read(
        &mut self,
        target: &ValueLocals,
        receiver: &ReadReceiver<'_>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let class = s.reserve_i32_local(f);
        let sections = StorageSections {
            intrinsic: self.open_reference_block(s, f),
            dictionary: ByProofs {
                composite: self.open_reference_block(s, f),
                object: self.open_reference_block(s, f),
            },
            inline: ByProofs {
                composite: self.open_reference_block(s, f),
                object: self.open_reference_block(s, f),
            },
            array: ByProofs {
                composite: self.open_reference_block(s, f),
                object: self.open_reference_block(s, f),
            },
        };
        // Header found by a layout the hint did not name: the storage is
        // then classified by every layout in turn.
        let classified = ByProofs {
            composite: self.open_frame(ControlFrameKind::Block, f),
            object: self.open_frame(ControlFrameKind::Block, f),
        };
        let unhinted = self.open_frame(ControlFrameKind::Block, f);
        let pairs = hinted_pairs();
        let cases = pairs
            .iter()
            .map(|_| self.open_frame(ControlFrameKind::Block, f))
            .collect::<Vec<_>>();
        let labels = (0..HINT_WORDS)
            .map(|word| {
                let case = pairs
                    .iter()
                    .position(|(layout, storage)| layout.hint_word(*storage) == word);
                self.local_branch_label(case.map_or(unhinted, |index| cases[index]))
            })
            .collect::<Vec<_>>();
        s.field(OwnDataReadCacheSchema::HINT)
            .read(receiver.head, s, f);
        f.branch_table_to_labels(labels, self.local_branch_label(unhinted));
        for (layout, storage) in pairs.into_iter().rev() {
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            let proofs = layout.class().proofs();
            let object = s.reserve_non_null_gc_local(f).initialize(
                self.emit_layout_header_or_branch(layout, target, unhinted, f),
                f,
            );
            header.replace(object.load(s, f).nullable(), f);
            class.set_constant(layout.class().word(), f);
            let base = s.reserve_non_null_gc_local(f).initialize(
                s.field(OrdinaryObjectSchema::PROPERTIES)
                    .read(&object, s, f)
                    .reference(),
                f,
            );
            object.clear(f);
            // A storage of another layout is classified by every layout.
            self.emit_storage_section_or_branch(
                storage,
                proofs,
                &base,
                &sections,
                *classified.at(proofs),
                f,
            );
            base.clear(f);
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        for layout in RECEIVER_LAYOUTS {
            let next = self.open_frame(ControlFrameKind::Block, f);
            header.replace(
                self.emit_layout_header_or_branch(layout, target, next, f)
                    .nullable(),
                f,
            );
            class.set_constant(layout.class().word(), f);
            self.emit_branch_to_target(*classified.at(layout.class().proofs()), f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        // Every other value: a Proxy, an exotic object, another primitive.
        self.emit_branch_to_target(receiver.exhausted, f);
        for proofs in [Proofs::Object, Proofs::Composite] {
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            let base = s.reserve_non_null_gc_local(f).initialize(
                s.field(OrdinaryObjectSchema::PROPERTIES)
                    .read(&header, s, f)
                    .reference(),
                f,
            );
            for storage in RECEIVER_STORAGES {
                if !storage.has_proofs(proofs) {
                    continue;
                }
                let next = self.open_frame(ControlFrameKind::Block, f);
                self.emit_storage_section_or_branch(storage, proofs, &base, &sections, next, f);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
            }
            base.clear(f);
            // No record names a receiver rule for a composite layout on
            // intrinsic storage.
            self.emit_branch_to_target(receiver.exhausted, f);
        }

        // Each section, entered with its storage and with `header` and
        // `class` describing the receiver, returns or branches to
        // `exhausted`.
        let StorageSections {
            intrinsic,
            dictionary,
            inline,
            array,
        } = sections;
        for (block, proofs) in [
            (array.object, Proofs::Object),
            (array.composite, Proofs::Composite),
        ] {
            let storage = block.close(f);
            self.pop_control(ControlFrameKind::Block);
            let storage =
                CompactStorage::Array(s.reserve_non_null_gc_local(f).initialize(storage, f));
            self.emit_compact_section(receiver, &header, class, storage, proofs, f)?;
        }
        for (block, proofs) in [
            (inline.object, Proofs::Object),
            (inline.composite, Proofs::Composite),
        ] {
            let storage = block.close(f);
            self.pop_control(ControlFrameKind::Block);
            let storage =
                CompactStorage::Inline(s.reserve_non_null_gc_local(f).initialize(storage, f));
            self.emit_compact_section(receiver, &header, class, storage, proofs, f)?;
        }
        for (block, proofs) in [
            (dictionary.object, Proofs::Object),
            (dictionary.composite, Proofs::Composite),
        ] {
            let storage = block.close(f);
            self.pop_control(ControlFrameKind::Block);
            let storage = s.reserve_non_null_gc_local(f).initialize(storage, f);
            self.emit_dictionary_section(receiver, &header, class, storage, proofs, f)?;
        }
        let storage = intrinsic.close(f);
        self.pop_control(ControlFrameKind::Block);
        let storage = s.reserve_non_null_gc_local(f).initialize(storage, f);
        self.emit_intrinsic_section(receiver, storage, f)?;
        s.release_i32_local(class, f);
        header.clear(f);
        Ok(())
    }

    /// Branches to the section of `storage` for `proofs` with `base` as
    /// that storage, or to `other` for a storage of any other layout.
    fn emit_storage_section_or_branch(
        &self,
        storage: ReceiverStorage,
        proofs: Proofs,
        base: &GcLocal<ObjectPropertyStorage>,
        sections: &StorageSections,
        other: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let label = self.local_branch_label(other);
        match storage {
            ReceiverStorage::Array => sections.array.at(proofs).branch_with(
                base.sealed_nested_subtype_or_branch::<CompactDataStorage, ArrayCompactDataStorage>(
                    s, label, f,
                ),
                f,
            ),
            ReceiverStorage::Inline => sections.inline.at(proofs).branch_with(
                base.sealed_nested_subtype_or_branch::<CompactDataStorage, InlineCompactDataStorage>(
                    s, label, f,
                ),
                f,
            ),
            ReceiverStorage::Dictionary => sections.dictionary.at(proofs).branch_with(
                base.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(s, label, f),
                f,
            ),
            ReceiverStorage::Intrinsic => match proofs {
                Proofs::Object => sections.intrinsic.branch_with(
                    base.sealed_subtype_or_branch::<IntrinsicPropertyStorage>(s, label, f),
                    f,
                ),
                Proofs::Composite => {
                    unreachable!("a composite layout has no proofs on intrinsic storage")
                }
            },
        }
    }

    /// Compact storage: the live shape is compared with each record's.
    fn emit_compact_section(
        &mut self,
        receiver: &ReadReceiver<'_>,
        header: &GcLocal<OrdinaryObject, Nullable>,
        class: I32Local,
        storage: CompactStorage,
        proofs: Proofs,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let ReadReceiver {
            head,
            name,
            key,
            value,
            found,
            exhausted,
        } = *receiver;
        let object = s
            .reserve_non_null_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        let shape = s
            .reserve_non_null_gc_local(f)
            .initialize(storage.shape(s, f), f);
        self.emit_record_scan(head, f, |builder, candidate, next, f| {
            let s = builder.runtime_schema();
            // An own record: the recorded shape has the key at a
            // nonnegative slot; a negative slot is the closed tail beyond a
            // shape that lacks it. Only an object or a function owns it.
            if proofs == Proofs::Object {
                let rule = builder.open_frame(ControlFrameKind::Block, f);
                let slot = builder.emit_recorded_shape_slot_or_branch(candidate, &shape, rule, f);
                builder.emit_recorded_own_slot_hit(&storage, slot, value, ReadAnswer::Return, f);
                // Only an actual ordinary object registers a closed tail.
                class.load(f);
                f.instruction(&Instruction::I32Const(ReceiverClass::Ordinary.word()));
                f.instruction(&Instruction::I32Ne);
                builder.emit_branch_if_to_target(rule, f);
                builder.emit_recorded_closed_tail_hit(
                    &object,
                    slot,
                    value,
                    rule,
                    ReadAnswer::Return,
                    f,
                )?;
                s.release_i32_local(slot, f);
                builder.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
            }
            // The live shape lacks the key, and the receiver's live
            // [[Prototype]] holds the place.
            builder.emit_receiver_rule_or_branch(
                candidate,
                HolderReceiverKind::Object,
                Some(&shape),
                next,
                f,
            );
            builder.emit_prototype_place_hit(
                candidate,
                &object,
                &prototype,
                key,
                value,
                found,
                next,
                ReadAnswer::Return,
                f,
            )
        })?;
        // A composite layout's own properties are never probed.
        if proofs == Proofs::Object {
            self.emit_unrecorded_shape_read(
                head, &object, &storage, &shape, class, &prototype, name, key, value, found,
                exhausted, f,
            )?;
        }
        self.emit_branch_to_target(exhausted, f);
        shape.clear(f);
        storage.clear(f);
        prototype.clear(f);
        object.clear(f);
        Ok(())
    }

    /// A dictionary: an own entry at a record's position, or the absence
    /// rule of a dictionary that never held an entry.
    fn emit_dictionary_section(
        &mut self,
        receiver: &ReadReceiver<'_>,
        header: &GcLocal<OrdinaryObject, Nullable>,
        class: I32Local,
        live: GcLocal<OrdinaryPropertyStorage>,
        proofs: Proofs,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let ReadReceiver {
            head,
            name,
            key,
            value,
            found,
            exhausted,
        } = *receiver;
        let object = s
            .reserve_non_null_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        let prototype = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize_null(s, f);
        self.emit_record_scan(head, f, |builder, candidate, next, f| {
            let s = builder.runtime_schema();
            // Only an object or a function owns an entry a record placed.
            if proofs == Proofs::Object {
                let position = s.reserve_i32_local(f);
                s.field(OwnDataReadCacheSchema::DICTIONARY_POSITION)
                    .read(candidate, s, f)
                    .store(position, f);
                builder.emit_dictionary_position_read_with(
                    &live,
                    position,
                    key,
                    value,
                    found,
                    RecipeValues::Decline,
                    f,
                );
                s.release_i32_local(position, f);
                found.load(f);
                f.instruction(&Instruction::If(BlockType::Empty));
                emit_return_normal(value, f);
                f.instruction(&Instruction::End);
            }
            builder.emit_empty_dictionary_or_branch(&live, next, f);
            builder.emit_receiver_rule_or_branch(
                candidate,
                HolderReceiverKind::Object,
                None,
                next,
                f,
            );
            builder.emit_prototype_place_hit(
                candidate,
                &object,
                &prototype,
                key,
                value,
                found,
                next,
                ReadAnswer::Return,
                f,
            )
        })?;
        self.emit_probed_dictionary_read(
            head, &object, &live, class, &prototype, name, key, value, found, exhausted, f,
        )?;
        self.emit_branch_to_target(exhausted, f);
        live.clear(f);
        prototype.clear(f);
        object.clear(f);
        Ok(())
    }

    /// An intrinsic layout is immutable: storage at a recorded address has
    /// the key's data row in the recorded value group. No record names a
    /// receiver rule for it, and only an object or a function owns it.
    fn emit_intrinsic_section(
        &mut self,
        receiver: &ReadReceiver<'_>,
        layout: GcLocal<IntrinsicPropertyStorage>,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let ReadReceiver {
            head,
            value,
            found,
            exhausted,
            ..
        } = *receiver;
        let address = s.reserve_i32_local(f);
        s.field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(&layout, s, f)
            .store(address, f);
        self.emit_record_scan(head, f, |builder, candidate, next, f| {
            let s = builder.runtime_schema();
            address.load(f);
            s.field(OwnDataReadCacheSchema::LAYOUT)
                .read(candidate, s, f);
            f.instruction(&Instruction::I32Ne);
            builder.emit_branch_if_to_target(next, f);
            let group = s.reserve_i32_local(f);
            s.field(OwnDataReadCacheSchema::GROUP)
                .read(candidate, s, f)
                .store(group, f);
            found.set_constant(0, f);
            builder.emit_materialized_intrinsic_group(&layout, group, value, found, f);
            s.release_i32_local(group, f);
            found.load(f);
            f.instruction(&Instruction::If(BlockType::Empty));
            emit_return_normal(value, f);
            f.instruction(&Instruction::End);
            Ok(())
        })?;
        self.emit_branch_to_target(exhausted, f);
        s.release_i32_local(address, f);
        layout.clear(f);
        Ok(())
    }

    /// A compact receiver whose shape no record of the site names, once the
    /// site has no learning budget: a site that meets more shapes than it
    /// chains. The live shape is probed for the key as the call-free walk
    /// probes it, which finds an own value or proves that the receiver, an
    /// actual ordinary object or a function, has no own property for the
    /// key. What lies beyond the receiver is then any record's closed tail
    /// or place, re-proved on the live chain. Returns the value, or falls
    /// through or branches to `exhausted` having observed nothing.
    #[allow(clippy::too_many_arguments)]
    fn emit_unrecorded_shape_read(
        &mut self,
        head: &GcLocal<OwnDataReadCache>,
        object: &GcLocal<OrdinaryObject>,
        storage: &CompactStorage,
        shape: &GcLocal<CompactDataShape>,
        class: I32Local,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        name: &GcLocal<StringValue>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        exhausted: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // A site that still learns sends the receiver to the miss kernel.
        s.field(OwnDataReadCacheSchema::BUDGET).read(head, s, f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GtS);
        self.emit_branch_if_to_target(exhausted, f);
        class.load(f);
        f.instruction(&Instruction::I32Const(ReceiverClass::Composite.word()));
        f.instruction(&Instruction::I32Eq);
        self.emit_branch_if_to_target(exhausted, f);
        // The miss kernel prepares the key's hash.
        let hash = s.reserve_i64_local(f);
        s.field(StringValueSchema::RAW_KEY_HASH)
            .read(name, s, f)
            .store_i64(hash, f);
        hash.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(exhausted, f);
        let slot = s.reserve_i32_local(f);
        let absent = self.open_frame(ControlFrameKind::Block, f);
        self.emit_walk_shape_slot_or_branch(shape, name, hash, slot, absent, exhausted, f);
        match storage {
            CompactStorage::Array(array) => self.emit_array_compact_value_at(array, slot, value, f),
            CompactStorage::Inline(inline) => {
                self.emit_inline_compact_recorded_value_at(inline, slot, value, f)
            }
        }
        emit_return_normal(value, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(slot, f);
        s.release_i64_local(hash, f);
        self.emit_beyond_receiver_scan(head, object, class, prototype, key, value, found, f)
    }

    /// A receiver on a dictionary that holds entries, which no shape
    /// describes: the dictionary's key index is asked for the key as the
    /// call-free walk asks it. When it proves that the receiver has no own
    /// property for the key, what lies beyond the receiver is any record's
    /// closed tail or place, re-proved on the live chain. Returns the value,
    /// or falls through or branches to `exhausted` having observed nothing;
    /// an own entry that no record placed is left to the walk.
    #[allow(clippy::too_many_arguments)]
    fn emit_probed_dictionary_read(
        &mut self,
        head: &GcLocal<OwnDataReadCache>,
        object: &GcLocal<OrdinaryObject>,
        dictionary: &GcLocal<OrdinaryPropertyStorage>,
        class: I32Local,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        name: &GcLocal<StringValue>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        exhausted: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        // The miss kernel prepares the key's hash.
        let hash = s.reserve_i64_local(f);
        s.field(StringValueSchema::RAW_KEY_HASH)
            .read(name, s, f)
            .store_i64(hash, f);
        hash.load(f);
        f.instruction(&Instruction::I64Eqz);
        self.emit_branch_if_to_target(exhausted, f);
        let entry = s
            .reserve_gc_local::<PropertyEntry, Nullable>(f)
            .initialize_null(s, f);
        let absent = self.open_frame(ControlFrameKind::Block, f);
        self.emit_walk_dictionary_entry_or_branch(
            dictionary,
            ChainKey::String { name, hash },
            &entry,
            absent,
            exhausted,
            f,
        );
        self.emit_branch_to_target(exhausted, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        entry.clear(f);
        s.release_i64_local(hash, f);
        self.emit_beyond_receiver_scan(head, object, class, prototype, key, value, found, f)
    }

    /// The read of a key that the receiver whose header is `object` has
    /// just been proved to lack, from any record's proof that does not rest
    /// on the receiver's own storage. A record with a shape and a negative
    /// slot registered a closed tail for this key: its registration proved
    /// that the tail's immutable rows lack the key, whatever shape it was
    /// registered with, and the tail is re-proved on this receiver's live
    /// chain, for an actual ordinary object as at registration. A record
    /// with a receiver rule names a place, which is re-proved on this
    /// receiver's live prototype and says nothing of any receiver. Returns
    /// the value, or falls through having observed nothing.
    #[allow(clippy::too_many_arguments)]
    fn emit_beyond_receiver_scan(
        &mut self,
        head: &GcLocal<OwnDataReadCache>,
        object: &GcLocal<OrdinaryObject>,
        class: I32Local,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_record_scan(head, f, |builder, candidate, next, f| {
            let s = builder.runtime_schema();
            let place = builder.open_frame(ControlFrameKind::Block, f);
            class.load(f);
            f.instruction(&Instruction::I32Const(ReceiverClass::Ordinary.word()));
            f.instruction(&Instruction::I32Ne);
            builder.emit_branch_if_to_target(place, f);
            s.field(OwnDataReadCacheSchema::SHAPE)
                .read(candidate, s, f)
                .reference()
                .is_null(f);
            builder.emit_branch_if_to_target(place, f);
            let tail = s.reserve_i32_local(f);
            s.field(OwnDataReadCacheSchema::SLOT)
                .read(candidate, s, f)
                .store(tail, f);
            tail.load(f);
            f.instruction(&Instruction::I32Const(0));
            f.instruction(&Instruction::I32GeS);
            builder.emit_branch_if_to_target(place, f);
            builder.emit_recorded_closed_tail_or_branch(object, tail, place, f)?;
            s.release_i32_local(tail, f);
            value.set_undefined(f);
            emit_return_normal(value, f);
            builder.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
            s.field(OwnDataReadCacheSchema::RECEIVER_KIND)
                .read(candidate, s, f);
            f.instruction(&Instruction::I32Const(HolderReceiverKind::None.word()));
            f.instruction(&Instruction::I32Eq);
            builder.emit_branch_if_to_target(next, f);
            builder.emit_prototype_place_hit(
                candidate,
                object,
                prototype,
                key,
                value,
                found,
                next,
                ReadAnswer::Return,
                f,
            )
        })
    }

    /// Runs `proofs` for each record of the chain from `head`, with the
    /// record and the label that leaves it for the next. `proofs` returns
    /// from the function when a record answers; this falls through when the
    /// chain has no further record.
    pub(super) fn emit_record_scan(
        &mut self,
        head: &GcLocal<OwnDataReadCache>,
        f: &mut Function,
        proofs: impl FnOnce(
            &mut Self,
            &GcLocal<OwnDataReadCache>,
            ControlTarget,
            &mut Function,
        ) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        proof_step!(ProofStep::RecordScan);
        let s = self.runtime_schema();
        let cursor = s
            .reserve_gc_local::<OwnDataReadCache, Nullable>(f)
            .initialize(head.load(s, f).nullable(), f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        let scan = self.open_frame(ControlFrameKind::Loop, f);
        // The one null test of each record: the end of the chain.
        let candidate = s.reserve_non_null_gc_local(f).initialize(
            cursor.non_null_or_branch(self.local_branch_label(done), f),
            f,
        );
        let next = self.open_frame(ControlFrameKind::Block, f);
        proofs(self, &candidate, next, f)?;
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        cursor.replace(
            s.field(OwnDataReadCacheSchema::NEXT)
                .read(&candidate, s, f)
                .reference(),
            f,
        );
        candidate.clear(f);
        self.emit_branch_to_target(scan, f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        cursor.clear(f);
        Ok(())
    }

    /// The ordinary header of a receiver with the sealed layout `layout`;
    /// every other value branches to `other`. One cast test admits the
    /// layout and its header is projected right after it.
    pub(super) fn emit_layout_header_or_branch(
        &mut self,
        layout: ReceiverLayout,
        target: &ValueLocals,
        other: ControlTarget,
        f: &mut Function,
    ) -> GcStackReference<OrdinaryObject> {
        proof_step!(ProofStep::LayoutHeader(layout));
        let s = self.runtime_schema();
        let label = self.local_branch_label(other);
        macro_rules! composite {
            ($layout:ty, $field:expr) => {{
                let composite = target.sealed_reference_or_branch::<$layout>(s, label, f);
                s.field($field).read_top(composite, f).reference()
            }};
        }
        match layout {
            ReceiverLayout::Ordinary => {
                target.sealed_reference_or_branch::<OrdinaryObject>(s, label, f)
            }
            ReceiverLayout::Function => composite!(FunctionObject, FunctionObjectSchema::OBJECT),
            ReceiverLayout::Array => composite!(ArrayObject, ArrayObjectSchema::OBJECT),
            ReceiverLayout::PrimitiveBox => composite!(PrimitiveBox, PrimitiveBoxSchema::OBJECT),
            ReceiverLayout::Map => composite!(MapObject, MapObjectSchema::OBJECT),
            ReceiverLayout::WeakMap => composite!(WeakMapObject, WeakMapObjectSchema::OBJECT),
            ReceiverLayout::Set => composite!(SetObject, SetObjectSchema::OBJECT),
            ReceiverLayout::WeakSet => composite!(WeakSetObject, WeakSetObjectSchema::OBJECT),
            ReceiverLayout::Promise => composite!(PromiseObject, PromiseObjectSchema::OBJECT),
        }
    }

    /// Records on a site's first record the header layout and the storage
    /// layout of the receiver the miss kernel met, which the hit entry then
    /// tests first. The word proves nothing: the entry admits a receiver and
    /// its storage by its own cast tests. A value with none of the header
    /// layouts leaves the hint as it is.
    pub(super) fn emit_record_receiver_layout_hint(
        &mut self,
        target: &ValueLocals,
        head: &GcLocal<OwnDataReadCache>,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let hint = s.reserve_i32_local(f);
        let unknown = self.open_frame(ControlFrameKind::Block, f);
        let known = self.open_frame(ControlFrameKind::Block, f);
        for layout in RECEIVER_LAYOUTS {
            let next = self.open_frame(ControlFrameKind::Block, f);
            header.replace(
                self.emit_layout_header_or_branch(layout, target, next, f)
                    .nullable(),
                f,
            );
            hint.set_constant(layout.word(), f);
            self.emit_branch_to_target(known, f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        self.emit_branch_to_target(unknown, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let object = s
            .reserve_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        object.clear(f);
        let stored = self.open_frame(ControlFrameKind::Block, f);
        for kind in RECEIVER_STORAGES {
            let next = self.open_frame(ControlFrameKind::Block, f);
            let label = self.local_branch_label(next);
            match kind {
                ReceiverStorage::Array => {
                    let _ = base.sealed_nested_subtype_or_branch::<
                        CompactDataStorage,
                        ArrayCompactDataStorage,
                    >(s, label, f);
                }
                ReceiverStorage::Inline => {
                    let _ = base.sealed_nested_subtype_or_branch::<
                        CompactDataStorage,
                        InlineCompactDataStorage,
                    >(s, label, f);
                }
                ReceiverStorage::Dictionary => {
                    let _ = base.sealed_subtype_or_branch::<OrdinaryPropertyStorage>(s, label, f);
                }
                ReceiverStorage::Intrinsic => {
                    let _ = base.sealed_subtype_or_branch::<IntrinsicPropertyStorage>(s, label, f);
                }
            }
            f.instruction(&Instruction::Drop);
            hint.load(f);
            f.instruction(&Instruction::I32Const(
                kind.word() << ReceiverStorage::HINT_SHIFT,
            ));
            f.instruction(&Instruction::I32Or);
            hint.store(f);
            self.emit_branch_to_target(stored, f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        base.clear(f);
        s.field(OwnDataReadCacheSchema::HINT)
            .write(head, GcOperand::i32_local(hint), s, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(hint, f);
        header.clear(f);
    }

    /// Answers the value at the record's place on the live [[Prototype]] of
    /// the receiver whose header is `object`, or branches to `miss`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_prototype_place_hit(
        &mut self,
        record: &GcLocal<OwnDataReadCache>,
        object: &GcLocal<OrdinaryObject>,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        miss: ControlTarget,
        answer: ReadAnswer,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_prototype_place_hit_on(
            &HOLDER_STORAGES,
            record,
            object,
            prototype,
            key,
            value,
            found,
            miss,
            answer,
            f,
        )
    }

    /// [`Self::emit_prototype_place_hit`] for a holder on one of `storages`:
    /// a holder on any other storage falls through.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_prototype_place_hit_on(
        &mut self,
        storages: &[HolderStorage],
        record: &GcLocal<OwnDataReadCache>,
        object: &GcLocal<OrdinaryObject>,
        prototype: &GcLocal<JavaScriptObject, Nullable>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        miss: ControlTarget,
        answer: ReadAnswer,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        proof_step!(ProofStep::PrototypePlace);
        let s = self.runtime_schema();
        prototype.replace(
            s.field(OrdinaryObjectSchema::PROTOTYPE)
                .read(object, s, f)
                .reference(),
            f,
        );
        if storages.contains(&HolderStorage::Intrinsic) {
            // A protected prototype of the current Realm that still holds
            // its recorded rows is the header and intrinsic storage the
            // classification below finds for it; the place is then decided
            // exactly as its intrinsic arm decides it.
            let general = self.open_frame(ControlFrameKind::Block, f);
            let protected = self.emit_protected_prototype_holder_or_branch(
                &crate::objects::ProtectedPrototype::HOLDER_ORDER,
                prototype,
                general,
                f,
            );
            let slot = s.reserve_i32_local(f);
            s.field(OwnDataReadCacheSchema::HOLDER_SLOT)
                .read(record, s, f)
                .store(slot, f);
            found.set_constant(0, f);
            self.emit_recorded_intrinsic_place(
                record,
                &protected.header,
                &protected.rows,
                slot,
                value,
                found,
                miss,
                answer,
                f,
            )?;
            s.release_i32_local(slot, f);
            protected.clear(f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        let holder = self.emit_prototype_header_or_branch(prototype, miss, f);
        self.emit_recorded_place_hit_on(
            storages, record, &holder, key, value, found, miss, answer, f,
        )?;
        holder.clear(f);
        Ok(())
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
        let record = &p.record;
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
            s.field(OwnDataReadCacheSchema::SHAPE).read(record, s, f);
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
                .read(record, s, f)
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
                    self.emit_inline_compact_recorded_store_at(inline, slot, &p.value, f)
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
            .read(record, s, f);
        f.instruction(&Instruction::RefEq);
        s.field(OrdinaryObjectSchema::EXTENSIBLE)
            .read(&object, s, f);
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
        // A dictionary holder is proved by its key index alone: the entry
        // makes no call. The record names the site's key.
        let name = s.reserve_gc_local(f).initialize(
            s.field(OwnDataReadCacheSchema::KEY)
                .read(record, s, f)
                .reference(),
            f,
        );
        let key = PropertyKeyLocals::from_string(s, &name, f);
        let tail = self.emit_recorded_add_chain_tail(
            &p.target,
            &object,
            &key,
            record,
            DictionaryAbsence::IndexOnly,
            f,
        )?;
        key.clear(f);
        name.clear(f);
        tail.load(f);
        s.field(OwnDataReadCacheSchema::ADD_TAIL).read(record, s, f);
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
                    .read(record, s, f)
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
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.return_call_helper(
            crate::runtime_helpers::CachedPropertyWriteMissArguments::new(
                &p.target,
                &p.receiver,
                &p.value,
                &p.record,
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
            self.emit_try_load_ready_non_array_realm_intrinsic_or_branch(
                &realm,
                slot,
                &intrinsic,
                miss,
                f,
            );
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
            .reserve_non_null_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        header.clear(f);
        object
    }

    /// The ordinary header of the current Realm's prototype in `slot` for a
    /// primitive receiver, which ToObject would box with it: for String,
    /// the one the Realm's armed protector record holds, which is the
    /// table's; otherwise, or before the record is armed, the table's, by
    /// `emit_realm_prototype_header_or_branch`.
    pub(super) fn emit_primitive_receiver_prototype_header_or_branch(
        &mut self,
        slot: NonArrayRealmIntrinsicSlot,
        miss: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject> {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(s, f);
        let done = self.open_frame(ControlFrameKind::Block, f);
        if matches!(slot, NonArrayRealmIntrinsicSlot::StringPrototype) {
            let table = self.open_frame(ControlFrameKind::Block, f);
            let protected = self.emit_string_prototype_header_or_branch(table, f);
            header.replace(protected.load(s, f).nullable(), f);
            protected.clear(f);
            self.emit_branch_to_target(done, f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        let realm: GcLocal<RealmRecord> = self.load_current_realm(f);
        let intrinsic = s.reserve_value_local(f);
        self.emit_try_load_ready_non_array_realm_intrinsic_or_branch(
            &realm,
            slot,
            &intrinsic,
            miss,
            f,
        );
        realm.clear(f);
        let table = self.emit_realm_prototype_header_or_branch(&intrinsic, miss, f);
        intrinsic.clear(f);
        header.replace(table.load(s, f).nullable(), f);
        table.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let object = s
            .reserve_non_null_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        header.clear(f);
        object
    }

    /// The ordinary header of a Realm's String or Number prototype, a
    /// primitive wrapper or an ordinary object; anything else branches to
    /// `miss`.
    pub(super) fn emit_realm_prototype_header_or_branch(
        &mut self,
        prototype: &ValueLocals,
        miss: ControlTarget,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject> {
        proof_step!(ProofStep::RealmPrototypeHeader);
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
            .reserve_non_null_gc_local(f)
            .initialize(header.load(s, f).require_non_null(f), f);
        header.clear(f);
        object
    }

    /// The storage as `layout`, after branching to `other` for any other
    /// storage.
    pub(super) fn emit_compact_storage_or_branch(
        &mut self,
        layout: CompactLayout,
        base: &GcLocal<ObjectPropertyStorage>,
        other: ControlTarget,
        f: &mut Function,
    ) -> CompactStorage {
        proof_step!(ProofStep::CompactStorage(layout));
        let s = self.runtime_schema();
        let label = self.local_branch_label(other);
        match layout {
            CompactLayout::Array => CompactStorage::Array(
                s.reserve_non_null_gc_local(f).initialize(
                    base.sealed_nested_subtype_or_branch::<
                        CompactDataStorage,
                        ArrayCompactDataStorage,
                    >(s, label, f),
                    f,
                ),
            ),
            CompactLayout::Inline => CompactStorage::Inline(
                s.reserve_non_null_gc_local(f).initialize(
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
    pub(super) fn emit_empty_dictionary_or_branch(
        &self,
        dictionary: &GcLocal<OrdinaryPropertyStorage>,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        proof_step!(ProofStep::EmptyDictionary);
        let s = self.runtime_schema();
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(dictionary, s, f);
        self.emit_branch_if_to_target(miss, f);
    }

    /// Branches to `miss` unless the record's prototype rule is the live
    /// receiver's: its kind, and the compact `shape` that proves an
    /// ordinary receiver lacks the key (None for an empty dictionary and
    /// for primitives).
    pub(super) fn emit_receiver_rule_or_branch(
        &self,
        record: &GcLocal<OwnDataReadCache>,
        kind: HolderReceiverKind,
        shape: Option<&GcLocal<CompactDataShape>>,
        miss: ControlTarget,
        f: &mut Function,
    ) {
        proof_step!(ProofStep::ReceiverRule {
            kind: kind.word(),
            shape: shape.is_some(),
        });
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

    /// Answers the value at the record's place on `holder`, the receiver's
    /// prototype, or branches to `miss`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_recorded_place_hit(
        &mut self,
        record: &GcLocal<OwnDataReadCache>,
        holder: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        miss: ControlTarget,
        answer: ReadAnswer,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_recorded_place_hit_on(
            &HOLDER_STORAGES,
            record,
            holder,
            key,
            value,
            found,
            miss,
            answer,
            f,
        )
    }

    /// Answers the value at the record's place on `holder` when the
    /// holder's storage is one of `storages`, taken in the entry's order, or
    /// branches to `miss`; a holder on any other storage falls through.
    /// Every storage of the entry's order branches to `miss` instead.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_recorded_place_hit_on(
        &mut self,
        storages: &[HolderStorage],
        record: &GcLocal<OwnDataReadCache>,
        holder: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        miss: ControlTarget,
        answer: ReadAnswer,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        proof_step!(ProofStep::RecordedPlace);
        let s = self.runtime_schema();
        let holder_base = s.reserve_non_null_gc_local(f).initialize(
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
        for storage in storages {
            proof_step!(ProofStep::HolderStorage(*storage));
            match *storage {
                // An intrinsic layout is immutable: storage at the recorded
                // address has the same rows, and keeps each materialized
                // value in its group.
                HolderStorage::Intrinsic => {
                    let other = self.open_frame(ControlFrameKind::Block, f);
                    let intrinsic = s.reserve_non_null_gc_local(f).initialize(
                        holder_base.sealed_subtype_or_branch::<IntrinsicPropertyStorage>(
                            s,
                            self.local_branch_label(other),
                            f,
                        ),
                        f,
                    );
                    self.emit_recorded_intrinsic_place(
                        record, holder, &intrinsic, slot, value, found, miss, answer, f,
                    )?;
                    intrinsic.clear(f);
                    self.pop_control(ControlFrameKind::Block);
                    f.instruction(&Instruction::End);
                }
                // A compact shape is immutable: the same shape has the key
                // at the same slot, or lacks it.
                HolderStorage::Compact(layout) => {
                    let other = self.open_frame(ControlFrameKind::Block, f);
                    let storage =
                        self.emit_compact_storage_or_branch(layout, &holder_base, other, f);
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
                            self.emit_inline_compact_recorded_value_at(inline, slot, value, f)
                        }
                    }
                    found.set_constant(1, f);
                    f.instruction(&Instruction::Else);
                    self.emit_recorded_holder_absence(holder, slot, value, found, miss, f)?;
                    f.instruction(&Instruction::End);
                    storage.clear(f);
                    self.emit_found_answer_or_miss(found, value, miss, answer, f);
                    self.pop_control(ControlFrameKind::Block);
                    f.instruction(&Instruction::End);
                }
                // A dictionary entry is confirmed by its own key at the
                // position.
                HolderStorage::Dictionary => {
                    let dictionary = s.reserve_non_null_gc_local(f).initialize(
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
                    self.emit_found_answer_or_miss(found, value, miss, answer, f);
                }
            }
        }
        s.release_i32_local(slot, f);
        holder_base.clear(f);
        Ok(())
    }

    /// Answers the record's place on `holder`, whose storage is the
    /// intrinsic `intrinsic`, or branches to `miss`; nothing falls through.
    /// An intrinsic layout is immutable: storage at the recorded address has
    /// the same rows, and keeps each materialized value in its group.
    #[allow(clippy::too_many_arguments)]
    fn emit_recorded_intrinsic_place(
        &mut self,
        record: &GcLocal<OwnDataReadCache>,
        holder: &GcLocal<OrdinaryObject>,
        intrinsic: &GcLocal<IntrinsicPropertyStorage>,
        slot: I32Local,
        value: &ValueLocals,
        found: I32Local,
        miss: ControlTarget,
        answer: ReadAnswer,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        s.field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(intrinsic, s, f);
        s.field(OwnDataReadCacheSchema::HOLDER_LAYOUT)
            .read(record, s, f);
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(miss, f);
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.emit_materialized_intrinsic_group(intrinsic, slot, value, found, f);
        f.instruction(&Instruction::Else);
        self.emit_recorded_holder_absence(holder, slot, value, found, miss, f)?;
        f.instruction(&Instruction::End);
        self.emit_found_answer_or_miss(found, value, miss, answer, f);
        Ok(())
    }

    /// An absence record holds the closed tail it was registered with as a
    /// negative slot word. The holder's layout or shape was just re-proved
    /// to lack the key; the read is undefined when the holder's own chain
    /// is still that tail, and anything else branches to `miss`.
    fn emit_recorded_holder_absence(
        &mut self,
        holder: &GcLocal<OrdinaryObject>,
        slot: I32Local,
        value: &ValueLocals,
        found: I32Local,
        miss: ControlTarget,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_recorded_closed_tail_or_branch(holder, slot, miss, f)?;
        value.set_undefined(f);
        found.set_constant(1, f);
        Ok(())
    }

    fn emit_found_answer_or_miss(
        &self,
        found: I32Local,
        value: &ValueLocals,
        miss: ControlTarget,
        answer: ReadAnswer,
        f: &mut Function,
    ) {
        found.load(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        answer.emit(self, value, f);
        f.instruction(&Instruction::End);
        self.emit_branch_to_target(miss, f);
    }

    /// The slot of a record whose shape is the receiver's live compact
    /// `shape`, in a new local the caller releases; any other record
    /// branches to `miss`.
    pub(super) fn emit_recorded_shape_slot_or_branch(
        &self,
        record: &GcLocal<OwnDataReadCache>,
        shape: &GcLocal<CompactDataShape>,
        miss: ControlTarget,
        f: &mut Function,
    ) -> I32Local {
        proof_step!(ProofStep::RecordedShape);
        let s = self.runtime_schema();
        let _ = shape.load(s, f);
        s.field(OwnDataReadCacheSchema::SHAPE).read(record, s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, f);
        let slot = s.reserve_i32_local(f);
        s.field(OwnDataReadCacheSchema::SLOT)
            .read(record, s, f)
            .store(slot, f);
        slot
    }

    /// Answers the own value at `slot` of a record whose shape is the
    /// storage's: a shape is immutable, so a nonnegative slot holds the
    /// key's value whatever happened since the record was made. A negative
    /// slot falls through.
    pub(super) fn emit_recorded_own_slot_hit(
        &mut self,
        storage: &CompactStorage,
        slot: I32Local,
        value: &ValueLocals,
        answer: ReadAnswer,
        f: &mut Function,
    ) {
        proof_step!(ProofStep::RecordedOwnSlot);
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        match storage {
            CompactStorage::Array(array) => self.emit_array_compact_value_at(array, slot, value, f),
            CompactStorage::Inline(inline) => {
                self.emit_inline_compact_recorded_value_at(inline, slot, value, f)
            }
        }
        answer.emit(self, value, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    /// Answers undefined when the chain above the actual ordinary object
    /// `object`, whose live shape a record proved lacks the key, is the
    /// closed tail the record registered as `slot`; branches to `miss`
    /// otherwise.
    pub(super) fn emit_recorded_closed_tail_hit(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        slot: I32Local,
        value: &ValueLocals,
        miss: ControlTarget,
        answer: ReadAnswer,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        proof_step!(ProofStep::RecordedClosedTail);
        self.emit_recorded_closed_tail_or_branch(object, slot, miss, f)?;
        value.set_undefined(f);
        answer.emit(self, value, f);
        Ok(())
    }
}

/// Where a proof sends the value it proved: the hit entry returns it as its
/// normal completion; a profiled P site branches with it to its join.
#[derive(Clone, Copy)]
pub(super) enum ReadAnswer {
    Return,
    Branch(ControlTarget),
}

impl ReadAnswer {
    fn emit(self, builder: &FunctionBuilder<'_>, value: &ValueLocals, f: &mut Function) {
        match self {
            Self::Return => emit_return_normal(value, f),
            Self::Branch(join) => builder.emit_branch_to_target(join, f),
        }
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
