//! Frame-local slot hints borrow current ordinary entries, never a cached table.
use super::*;
use crate::heap::DescriptorMask;

impl RowScratch {
    pub(super) fn load_function_or_ordinary_storage(
        &self,
        builder: &mut FunctionBuilder<'_>,
        declined: ControlTarget,
        f: &mut Function,
    ) {
        let s = builder.runtime_schema();
        self.value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Object.tag()));
        f.instruction(&Instruction::I32Eq);
        builder.open_frame(ControlFrameKind::If, f);
        self.value.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<OrdinaryObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(declined, f);
        self.header.replace(
            self.value.cast_reference::<OrdinaryObject>(s, f).nullable(),
            f,
        );
        f.instruction(&Instruction::Else);
        self.value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
        f.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(declined, f);
        self.value.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            s.reference_type::<FunctionObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        f.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(declined, f);
        let callable = s
            .reserve_gc_local(f)
            .initialize(self.value.cast_reference::<FunctionObject>(s, f), f);
        self.header.replace(
            s.field(FunctionObjectSchema::OBJECT)
                .read(&callable, s, f)
                .reference()
                .nullable(),
            f,
        );
        callable.clear(f);
        builder.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.base.replace(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(&self.header, s, f)
                .reference()
                .nullable(),
            f,
        );
    }

    fn read_stored_value(&self, builder: &FunctionBuilder<'_>, f: &mut Function) {
        let s = builder.runtime_schema();
        // Entry keys are nonnull by construction. Data values reach this owner
        // only after the concrete StoredValue guard above their read.
        let stored = s
            .reserve_gc_local(f)
            .initialize(self.stored.load(s, f).require_non_null(f), f);
        s.struct_type::<StoredValue>()
            .read_into(&stored, &self.value, s, f);
        stored.clear(f);
    }
}

/// Only dictionary preparation reserves these roots. The inline backedge keeps
/// the original compact scratch declaration, initialization and cleanup.
pub(super) struct DictionaryScratch {
    dictionary: GcLocal<OrdinaryPropertyStorage, Nullable>,
    entries: GcLocal<PropertyTable, Nullable>,
    entry: GcLocal<PropertyEntry, Nullable>,
    descriptor: GcLocal<PropertyDescriptor, Nullable>,
    property: GcLocal<PropertyValue, Nullable>,
}

impl DictionaryScratch {
    pub(super) fn new(builder: &FunctionBuilder<'_>, f: &mut Function) -> Self {
        let s = builder.runtime_schema();
        Self {
            dictionary: s.reserve_gc_local(f).initialize_null(s, f),
            entries: s.reserve_gc_local(f).initialize_null(s, f),
            entry: s.reserve_gc_local(f).initialize_null(s, f),
            descriptor: s.reserve_gc_local(f).initialize_null(s, f),
            property: s.reserve_gc_local(f).initialize_null(s, f),
        }
    }

    pub(super) fn release(self, f: &mut Function) {
        self.property.clear(f);
        self.descriptor.clear(f);
        self.entry.clear(f);
        self.entries.clear(f);
        self.dictionary.clear(f);
    }

    pub(super) fn capture(
        &self,
        scratch: &RowScratch,
        builder: &mut FunctionBuilder<'_>,
        declined: ControlTarget,
        f: &mut Function,
    ) {
        let s = builder.runtime_schema();
        scratch.base.is_subtype::<OrdinaryPropertyStorage>(s, f);
        f.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(declined, f);
        self.dictionary.replace(
            scratch
                .base
                .load(s, f)
                .cast_subtype::<OrdinaryPropertyStorage>(s, f)
                .nullable(),
            f,
        );
        self.entries.replace(
            s.field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(&self.dictionary, s, f)
                .reference()
                .nullable(),
            f,
        );
    }

    fn load_entry_key(
        &self,
        scratch: &RowScratch,
        builder: &FunctionBuilder<'_>,
        f: &mut Function,
    ) {
        let s = builder.runtime_schema();
        scratch.stored.replace(
            s.field(PropertyEntrySchema::KEY)
                .read(&self.entry, s, f)
                .reference()
                .nullable(),
            f,
        );
        scratch.read_stored_value(builder, f);
    }

    pub(super) fn read_slot(
        &self,
        scratch: &RowScratch,
        slot: I32Local,
        name: &GcLocal<StringValue, Nullable>,
        builder: &mut FunctionBuilder<'_>,
        declined: ControlTarget,
        f: &mut Function,
    ) {
        let s = builder.runtime_schema();
        // LENGTH owns the live insertion domain, independently of spare array
        // capacity. Deletion leaves a null entry; compaction replaces the table
        // and moves positions. Revalidate this row's live entry in every case.
        slot.load(f);
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&self.dictionary, s, f);
        f.instruction(&Instruction::I32GeU);
        builder.emit_branch_if_to_target(declined, f);
        slot.load(f);
        s.array_type::<PropertyTable>().length(&self.entries, s, f);
        f.instruction(&Instruction::I32GeU);
        builder.emit_branch_if_to_target(declined, f);
        self.entry.replace(
            s.array_type::<PropertyTable>()
                .read(&self.entries, slot, s, f)
                .reference(),
            f,
        );
        self.entry.load(s, f).is_null(f);
        builder.emit_branch_if_to_target(declined, f);
        self.load_entry_key(scratch, builder, f);
        scratch.value.tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Ne);
        builder.emit_branch_if_to_target(declined, f);
        scratch.value.reference().load(f);
        name.load(s, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(declined, f);
        self.descriptor.replace(
            s.field(PropertyEntrySchema::DESCRIPTOR)
                .read(&self.entry, s, f)
                .reference()
                .nullable(),
            f,
        );
        s.field(PropertyDescriptorSchema::FLAGS)
            .read(&self.descriptor, s, f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        builder.emit_branch_if_to_target(declined, f);
        self.property.replace(
            s.field(PropertyDescriptorSchema::VALUE)
                .read(&self.descriptor, s, f)
                .reference()
                .nullable(),
            f,
        );
        // A deferred native recipe may allocate or initialize a Realm. This
        // read-only region consumes only an already concrete immutable Value.
        self.property.is_subtype::<StoredValue>(s, f);
        f.instruction(&Instruction::I32Eqz);
        builder.emit_branch_if_to_target(declined, f);
        scratch.stored.replace(
            self.property
                .load(s, f)
                .cast_subtype::<StoredValue>(s, f)
                .nullable(),
            f,
        );
        scratch.read_stored_value(builder, f);
    }
}

impl FirstRowKeys<'_, '_> {
    pub(super) fn resolve_dictionary(
        &self,
        scratch: &RowScratch,
        dictionary: &DictionaryScratch,
        builder: &mut FunctionBuilder<'_>,
        declined: ControlTarget,
        f: &mut Function,
    ) {
        let s = builder.runtime_schema();
        for ((path, slot), name) in self
            .rows
            .paths
            .iter()
            .zip(&self.rows.slots)
            .zip(&self.rows.names)
        {
            let cached = path.name().cached(builder, f);
            name.replace(cached.load(s, f), f);
            cached.clear(f);
            name.load(s, f).is_null(f);
            builder.emit_branch_if_to_target(declined, f);
            let expected = s
                .reserve_gc_local(f)
                .initialize(name.load(s, f).require_non_null(f), f);
            let key = crate::operations::PropertyKeyLocals::from_string(s, &expected, f);
            expected.clear(f);
            let header = s
                .reserve_gc_local(f)
                .initialize(scratch.header.load(s, f).require_non_null(f), f);
            // The preceding concrete dictionary guard guarantees that this
            // existing finder cannot promote storage or materialize a recipe.
            let entry = s.helper_reference_on_stack(
                s.call_helper(
                    crate::runtime_helpers::OrdinaryPropertyFindArguments::new(&header, &key),
                    builder
                        .runtime_helper_base()
                        .expect("closed row guards link their runtime"),
                    f,
                ),
            );
            dictionary.entry.replace(entry, f);
            header.clear(f);
            key.clear(f);
            dictionary.entry.load(s, f).is_null(f);
            builder.emit_branch_if_to_target(declined, f);
            s.field(PropertyEntrySchema::POSITION)
                .read(&dictionary.entry, s, f)
                .store(*slot, f);
            dictionary.load_entry_key(scratch, builder, f);
            scratch.value.tag().load(f);
            f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
            f.instruction(&Instruction::I32Ne);
            builder.emit_branch_if_to_target(declined, f);
            name.replace(
                scratch.value.cast_reference::<StringValue>(s, f).nullable(),
                f,
            );
        }
    }
}
