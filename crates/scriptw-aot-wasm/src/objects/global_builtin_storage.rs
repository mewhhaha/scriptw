//! Global builtins retain their immutable insertion ranks until deletion.
//! Only a definition or a positive identifier cache needs a stable entry.
use super::*;
use crate::data::RPropertyLayoutSpan;
use crate::gc_types::*;
use crate::runtime_helpers::{GlobalBuiltinAppendFindArguments, IntrinsicPropertyFindArguments};

pub(crate) const MAX_GLOBAL_BUILTIN_ROWS: usize = 2 * u64::BITS as usize;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_alloc_global_builtin_object(
        &self,
        prototype: &ValueLocals,
        realm: &GcLocal<RealmRecord>,
        span: RPropertyLayoutSpan,
        deleted: [u64; 2],
        f: &mut Function,
    ) -> GcStackReference<OrdinaryObject> {
        let s = self.runtime_schema();
        let rows = intrinsic_properties::emit_intrinsic_layout_storage_record(span, realm, s, f);
        let storage = s.reserve_gc_local(f).initialize(
            s.struct_type::<GlobalBuiltinBacking>()
                .construct(
                    (
                        GcOperand::reference(&rows, s),
                        GcOperand::null(s),
                        GcOperand::i64(deleted[0] as i64),
                        GcOperand::i64(deleted[1] as i64),
                        GcOperand::null(s),
                    ),
                    f,
                )
                .upcast::<ObjectPropertyStorage>(),
            f,
        );
        let immutable_prototype = s.reserve_i32_local(f);
        immutable_prototype.set_constant(0, f);
        let object =
            self.emit_alloc_object_header_with_storage(prototype, immutable_prototype, &storage, f);
        s.release_i32_local(immutable_prototype, f);
        storage.clear(f);
        rows.clear(f);
        object
    }

    pub(super) fn emit_global_builtin_backing(
        &self,
        object: &GcLocal<OrdinaryObject>,
        f: &mut Function,
    ) -> GcLocal<GlobalBuiltinBacking, Nullable> {
        let s = self.runtime_schema();
        let storage = s.reserve_gc_local(f).initialize(
            s.field(OrdinaryObjectSchema::PROPERTIES)
                .read(object, s, f)
                .reference(),
            f,
        );
        let backing = s
            .reserve_gc_local::<GlobalBuiltinBacking, Nullable>(f)
            .initialize_null(s, f);
        storage.is_subtype::<GlobalBuiltinBacking>(s, f);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        backing.replace(
            storage
                .load(s, f)
                .cast_subtype::<GlobalBuiltinBacking>(s, f)
                .nullable(),
            f,
        );
        f.instruction(&Instruction::End);
        storage.clear(f);
        backing
    }

    pub(crate) fn emit_global_builtin_find(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let s = self.runtime_schema();
        let index = s.reserve_i32_local(f);
        index.set_constant(-1, f);
        key.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let rows = self.emit_global_builtin_rows(backing, f);
        s.call_helper(
            IntrinsicPropertyFindArguments::new(&rows, key),
            self.runtime_helper_base()?,
            f,
        )
        .store(index, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_global_builtin_is_deleted(backing, index, f);
        self.open_frame(ControlFrameKind::If, f);
        index.set_constant(-1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        rows.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(index)
    }

    pub(crate) fn emit_global_builtin_append_entry(
        &self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<GcLocal<PropertyEntry, Nullable>, EmitError> {
        let s = self.runtime_schema();
        let entry = s.helper_reference_on_stack(s.call_helper(
            GlobalBuiltinAppendFindArguments::new(backing, key),
            self.runtime_helper_base()?,
            f,
        ));
        Ok(s.reserve_gc_local(f).initialize(entry, f))
    }

    pub(crate) fn emit_global_builtin_rows(
        &self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        f: &mut Function,
    ) -> GcLocal<IntrinsicPropertyStorage> {
        let s = self.runtime_schema();
        s.reserve_gc_local(f).initialize(
            s.field(GlobalBuiltinBackingSchema::ROWS)
                .read(backing, s, f)
                .reference(),
            f,
        )
    }

    pub(crate) fn emit_global_builtin_row_key(
        &self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        index: I32Local,
        f: &mut Function,
    ) -> Result<PropertyKeyLocals, EmitError> {
        let s = self.runtime_schema();
        let rows = self.emit_global_builtin_rows(backing, f);
        let row = self.emit_intrinsic_property_row(&rows, index, f);
        let key = PropertyKeyLocals::from_intrinsic_installation_parameter(
            row.key(),
            s,
            self.runtime_helper_base()?,
            f,
        );
        row.clear(s, f);
        rows.clear(f);
        Ok(key)
    }

    pub(crate) fn emit_global_builtin_row_entry(
        &self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        index: I32Local,
        f: &mut Function,
    ) -> GcLocal<PropertyEntry, Nullable> {
        let s = self.runtime_schema();
        let entries = s.reserve_gc_local(f).initialize(
            s.field(GlobalBuiltinBackingSchema::ENTRIES)
                .read(backing, s, f)
                .reference(),
            f,
        );
        let entry = s
            .reserve_gc_local::<PropertyEntry, Nullable>(f)
            .initialize_null(s, f);
        entries.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        let table = s
            .reserve_gc_local(f)
            .initialize(entries.load(s, f).require_non_null(f), f);
        entry.replace(
            s.array_type::<PropertyTable>()
                .read(&table, index, s, f)
                .reference(),
            f,
        );
        table.clear(f);
        f.instruction(&Instruction::End);
        entries.clear(f);
        entry
    }

    pub(crate) fn emit_global_builtin_row_value(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        index: I32Local,
        f: &mut Function,
    ) -> Result<GcLocal<PropertyValue>, EmitError> {
        let rows = self.emit_global_builtin_rows(backing, f);
        let value = self.emit_intrinsic_property_value(&rows, index, false, f)?;
        rows.clear(f);
        Ok(value)
    }

    pub(crate) fn emit_global_builtin_row_flags(
        &self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        index: I32Local,
        f: &mut Function,
    ) -> I32Local {
        let s = self.runtime_schema();
        let flags = s.reserve_i32_local(f);
        let entry = self.emit_global_builtin_row_entry(backing, index, f);
        entry.load(s, f).is_null(f);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        let rows = self.emit_global_builtin_rows(backing, f);
        let row = self.emit_intrinsic_property_row(&rows, index, f);
        row.flags().load(f);
        flags.store(f);
        row.clear(s, f);
        rows.clear(f);
        f.instruction(&Instruction::Else);
        let live = s
            .reserve_gc_local(f)
            .initialize(entry.load(s, f).require_non_null(f), f);
        let descriptor = s.reserve_gc_local(f).initialize(
            s.field(PropertyEntrySchema::DESCRIPTOR)
                .read(&live, s, f)
                .reference(),
            f,
        );
        s.field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, s, f)
            .store(flags, f);
        descriptor.clear(f);
        live.clear(f);
        f.instruction(&Instruction::End);
        entry.clear(f);
        flags
    }

    pub(crate) fn emit_global_builtin_row_descriptor(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        index: I32Local,
        f: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor>, EmitError> {
        let s = self.runtime_schema();
        let entry = self.emit_global_builtin_row_entry(backing, index, f);
        let result = self.emit_ordinary_entry_descriptor_reference(&entry, f);
        result.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let rows = self.emit_global_builtin_rows(backing, f);
        let row = self.emit_intrinsic_property_row(&rows, index, f);
        let value = self.emit_intrinsic_property_value(&rows, index, false, f)?;
        let absent = s.reserve_gc_local(f).initialize(s.stored_undefined(f), f);
        let descriptor = s.reserve_gc_local(f).initialize(
            s.struct_type::<PropertyDescriptor>().construct(
                (
                    GcOperand::intrinsic_layout_flags(&row),
                    GcOperand::reference(&value, s),
                    GcOperand::reference(&absent, s),
                    GcOperand::reference(&absent, s),
                ),
                f,
            ),
            f,
        );
        result.replace(descriptor.load(s, f).nullable(), f);
        descriptor.clear(f);
        absent.clear(f);
        value.clear(f);
        row.clear(s, f);
        rows.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let descriptor = s
            .reserve_gc_local(f)
            .initialize(result.load(s, f).require_non_null(f), f);
        result.clear(f);
        entry.clear(f);
        Ok(descriptor)
    }

    pub(crate) fn emit_global_builtin_ensure_entry(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        index: I32Local,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<GcLocal<PropertyEntry>, EmitError> {
        let s = self.runtime_schema();
        let entry = self.emit_global_builtin_row_entry(backing, index, f);
        entry.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let descriptor = self.emit_global_builtin_row_descriptor(backing, index, f)?;
        let installed = self.emit_global_builtin_install_entry(backing, index, key, &descriptor, f);
        entry.replace(installed.load(s, f).nullable(), f);
        installed.clear(f);
        descriptor.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let live = s
            .reserve_gc_local(f)
            .initialize(entry.load(s, f).require_non_null(f), f);
        entry.clear(f);
        Ok(live)
    }

    /// The caller has proved that this original row is live and has no entry.
    pub(super) fn emit_global_builtin_install_entry(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        index: I32Local,
        key: &PropertyKeyLocals,
        descriptor: &GcLocal<PropertyDescriptor>,
        f: &mut Function,
    ) -> GcLocal<PropertyEntry> {
        let s = self.runtime_schema();
        let rows = self.emit_global_builtin_rows(backing, f);
        let entries = s.reserve_gc_local(f).initialize(
            s.field(GlobalBuiltinBackingSchema::ENTRIES)
                .read(backing, s, f)
                .reference(),
            f,
        );
        entries.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let count = s.reserve_i32_local(f);
        s.field(IntrinsicPropertyStorageSchema::COUNT)
            .read(&rows, s, f)
            .store(count, f);
        let table = s.reserve_gc_local(f).initialize(
            s.array_type::<PropertyTable>()
                .filled(GcOperand::null(s), count, f),
            f,
        );
        s.field(GlobalBuiltinBackingSchema::ENTRIES).write(
            backing,
            GcOperand::nullable_reference(&table, s),
            s,
            f,
        );
        entries.replace(table.load(s, f).nullable(), f);
        table.clear(f);
        s.release_i32_local(count, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let row = self.emit_intrinsic_property_row(&rows, index, f);
        let stored_key = s
            .reserve_gc_local(f)
            .initialize(s.struct_type::<StoredValue>().from_value(key.value(), f), f);
        let entry = s.reserve_gc_local(f).initialize(
            s.struct_type::<PropertyEntry>().construct(
                (
                    GcOperand::reference(&stored_key, s),
                    GcOperand::reference(descriptor, s),
                    GcOperand::i64_local(row.raw_key_hash()),
                    GcOperand::i32_local(index),
                    GcOperand::boolean(false),
                ),
                f,
            ),
            f,
        );
        let table = s
            .reserve_gc_local(f)
            .initialize(entries.load(s, f).require_non_null(f), f);
        s.array_type::<PropertyTable>().write(
            &table,
            index,
            GcOperand::nullable_reference(&entry, s),
            s,
            f,
        );
        self.emit_clear_global_builtin_row_value(&rows, index, f);
        table.clear(f);
        stored_key.clear(f);
        row.clear(s, f);
        entries.clear(f);
        rows.clear(f);
        entry
    }

    pub(crate) fn emit_global_builtin_is_deleted(
        &self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        index: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let mask = s.reserve_i64_local(f);
        s.field(GlobalBuiltinBackingSchema::DELETED_LOW)
            .read(backing, s, f)
            .store_i64(mask, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(u64::BITS as i32));
        f.instruction(&Instruction::I32GeU);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        s.field(GlobalBuiltinBackingSchema::DELETED_HIGH)
            .read(backing, s, f)
            .store_i64(mask, f);
        f.instruction(&Instruction::End);
        mask.load(f);
        Self::emit_global_builtin_bit(index, f);
        f.instruction(&Instruction::I64And);
        f.instruction(&Instruction::I64Eqz);
        f.instruction(&Instruction::I32Eqz);
        s.release_i64_local(mask, f);
    }

    fn emit_global_builtin_bit(index: I32Local, f: &mut Function) {
        // Wasm masks the shift count modulo 64, for either half.
        f.instruction(&Instruction::I64Const(1));
        index.load(f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Shl);
    }

    pub(super) fn emit_global_builtin_delete_row(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        index: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let mask = s.reserve_i64_local(f);
        index.load(f);
        f.instruction(&Instruction::I32Const(u64::BITS as i32));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, f);
        for (high, field) in [
            (false, GlobalBuiltinBackingSchema::DELETED_LOW),
            (true, GlobalBuiltinBackingSchema::DELETED_HIGH),
        ] {
            if high {
                f.instruction(&Instruction::Else);
            }
            s.field(field).read(backing, s, f).store_i64(mask, f);
            mask.load(f);
            Self::emit_global_builtin_bit(index, f);
            f.instruction(&Instruction::I64Or);
            mask.store(f);
            s.field(field)
                .write(backing, GcOperand::i64_local(mask), s, f);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(mask, f);
        let previous = self.emit_global_builtin_row_entry(backing, index, f);
        previous.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let entry = s
            .reserve_gc_local(f)
            .initialize(previous.load(s, f).require_non_null(f), f);
        let table = s.reserve_gc_local(f).initialize(
            s.field(GlobalBuiltinBackingSchema::ENTRIES)
                .read(backing, s, f)
                .reference()
                .require_non_null(f),
            f,
        );
        s.array_type::<PropertyTable>()
            .write(&table, index, GcOperand::null(s), s, f);
        self.emit_retire_ordinary_property_entry(&entry, f);
        table.clear(f);
        entry.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        previous.clear(f);
        let rows = self.emit_global_builtin_rows(backing, f);
        self.emit_clear_global_builtin_row_value(&rows, index, f);
        rows.clear(f);
    }

    // Once a row has an entry, its descriptor owns any surviving recipe.
    // Deleting the row must also release a value cached by an earlier read.
    fn emit_clear_global_builtin_row_value(
        &self,
        rows: &GcLocal<IntrinsicPropertyStorage>,
        index: I32Local,
        f: &mut Function,
    ) {
        let s = self.runtime_schema();
        let cache = s.reserve_gc_local(f).initialize(
            s.field(IntrinsicPropertyStorageSchema::CACHE)
                .read(rows, s, f)
                .reference(),
            f,
        );
        cache.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        s.array_type::<IntrinsicPropertyValueCache>().write(
            &cache,
            index,
            GcOperand::null(s),
            s,
            f,
        );
        f.instruction(&Instruction::End);
        cache.clear(f);
    }

    pub(super) fn emit_ensure_global_builtin_append(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        f: &mut Function,
    ) -> GcLocal<OrdinaryPropertyStorage> {
        let s = self.runtime_schema();
        let append = s.reserve_gc_local(f).initialize(
            s.field(GlobalBuiltinBackingSchema::APPEND)
                .read(backing, s, f)
                .reference(),
            f,
        );
        append.load(s, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let entries = s
            .reserve_gc_local(f)
            .initialize(s.empty_property_table(f), f);
        let index = s
            .reserve_gc_local(f)
            .initialize(s.empty_property_index(f), f);
        let dictionary = s.reserve_gc_local(f).initialize(
            s.struct_type::<OrdinaryPropertyStorage>().construct(
                (
                    GcOperand::reference(&entries, s),
                    GcOperand::i32(0),
                    GcOperand::reference(&index, s),
                ),
                f,
            ),
            f,
        );
        s.field(GlobalBuiltinBackingSchema::APPEND).write(
            backing,
            GcOperand::nullable_reference(&dictionary, s),
            s,
            f,
        );
        append.replace(dictionary.load(s, f).nullable(), f);
        dictionary.clear(f);
        index.clear(f);
        entries.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let dictionary = s
            .reserve_gc_local(f)
            .initialize(append.load(s, f).require_non_null(f), f);
        append.clear(f);
        dictionary
    }
}
