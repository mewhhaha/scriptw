//! The own data properties of a plain object, read from its compact storage.
//!
//! A serializer that lists an object's enumerable own String keys and then
//! reads each of them needs neither an OwnPropertyKeys list nor a lookup a
//! key while the object is an actual OrdinaryObject on a compact storage
//! whose shape lists its keys in OwnPropertyKeys order with the uniform
//! writable, enumerable, configurable data attributes. Such a shape holds
//! only String keys, every one an enumerable own data property, and the
//! slot of a key is its position among them. Every structural mutation (an
//! add, a delete, a redefinition) publishes a new storage record, so the
//! identity of the object's current storage is the complete guard of a later
//! read: the same record still has the captured shape, and its slots hold
//! the live values. This is the proof of for-in's compact own-key phase.
use super::*;
use crate::emit::ControlTarget;

impl FunctionBuilder<'_> {
    /// The compact storage of `value` when the shape's keys, in order, are
    /// exactly its enumerable own property keys and every one names a data
    /// property; null for every other value. Observes nothing.
    pub(crate) fn emit_plain_data_object_storage(
        &mut self,
        value: &ValueLocals,
        f: &mut Function,
    ) -> GcLocal<CompactDataStorage, Nullable> {
        let s = self.runtime_schema();
        let storage = s
            .reserve_gc_local::<CompactDataStorage, Nullable>(f)
            .initialize_null(s, f);
        let candidate = s
            .reserve_gc_local::<CompactDataStorage, Nullable>(f)
            .initialize_null(s, f);
        let declined = self.open_frame(ControlFrameKind::Block, f);
        value.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<OrdinaryObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        let object = s
            .reserve_gc_local(f)
            .initialize(value.cast_reference::<OrdinaryObject>(s, f), f);
        let base = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, s, f)
                .reference(),
            f,
        );
        let selected = self.open_frame(ControlFrameKind::Block, f);
        let other = self.open_frame(ControlFrameKind::Block, f);
        let inline = s.reserve_gc_local(f).initialize(
            base.sealed_nested_subtype_or_branch::<CompactDataStorage, InlineCompactDataStorage>(
                s,
                self.local_branch_label(other),
                f,
            ),
            f,
        );
        candidate.replace(
            inline.load(s, f).upcast::<CompactDataStorage>().nullable(),
            f,
        );
        inline.clear(f);
        self.emit_branch_to_target(selected, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let array = s.reserve_gc_local(f).initialize(
            base.sealed_nested_subtype_or_branch::<CompactDataStorage, ArrayCompactDataStorage>(
                s,
                self.local_branch_label(declined),
                f,
            ),
            f,
        );
        candidate.replace(
            array.load(s, f).upcast::<CompactDataStorage>().nullable(),
            f,
        );
        array.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let compact = s
            .reserve_gc_local(f)
            .initialize(candidate.load(s, f).require_non_null(f), f);
        let shape = s.reserve_gc_local(f).initialize(
            s.field(CompactDataStorageSchema::SHAPE)
                .read(&compact, s, f)
                .reference(),
            f,
        );
        s.field(CompactDataShapeSchema::OWN_KEYS_ORDERED)
            .read(&shape, s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        s.field(CompactDataShapeSchema::ATTRIBUTES)
            .read(&shape, s, f);
        let attributes = StoredPropertyAttributes::Data {
            writable: true,
            enumerable: true,
            configurable: true,
        }
        .descriptor_word();
        f.instruction(&Instruction::I32Const(attributes.as_i32()));
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(declined, f);
        storage.replace(candidate.load(s, f), f);
        shape.clear(f);
        compact.clear(f);
        base.clear(f);
        object.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        candidate.clear(f);
        storage
    }

    /// The live value of the property at `slot` of `storage`, which
    /// `emit_plain_data_object_storage` returned for `object`: branches to
    /// `changed`, having observed nothing, unless `storage` is still the
    /// object's storage. `slot` is below the shape's key count.
    pub(crate) fn emit_plain_data_object_value_or_branch(
        &mut self,
        object: &ValueLocals,
        storage: &GcLocal<CompactDataStorage>,
        slot: I32Local,
        value: &ValueLocals,
        changed: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let header = s
            .reserve_gc_local(f)
            .initialize(object.cast_reference::<OrdinaryObject>(s, f), f);
        self.emit_plain_data_header_value_or_branch(&header, storage, slot, value, changed, f);
        header.clear(f);
    }

    /// Same check and compact slot read when the caller already holds the
    /// cast `OrdinaryObject` header.
    pub(crate) fn emit_plain_data_header_value_or_branch(
        &mut self,
        header: &GcLocal<OrdinaryObject>,
        storage: &GcLocal<CompactDataStorage>,
        slot: I32Local,
        value: &ValueLocals,
        changed: ControlTarget,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let _ = s
            .field(OrdinaryObjectSchema::PROPERTIES)
            .read(header, s, f)
            .reference();
        let _ = storage.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(changed, f);
        self.emit_compact_value_at(storage, slot, value, f);
    }
}
