//! A global identifier read site's record in P.
//!
//! A read that starts at the Global Environment Record keeps a record in a
//! global of the module that compiles it, made by R for the record's global
//! object: the dictionary entry that holds the name's property. Three facts
//! make that entry enough to answer the read without a call.
//!
//! - A Global Environment Record keeps its Realm, and a Realm its global
//!   object, for life, so the same Environment means the same object.
//! - An ordinary header whose storage is a dictionary keeps that dictionary,
//!   and a property keeps its one entry record while it lives: compaction
//!   moves the record, redefinition changes the record's descriptor in
//!   place or replaces it in the record.
//! - Deletion, the only way a live entry leaves its table, replaces the
//!   record's descriptor with an accessor descriptor that no property owns.
//!
//! So an entry record whose descriptor is a data descriptor is the live own
//! data property of that global object, and its current value is the
//! binding's value whenever the Global Environment Record has no declarative
//! names at all: HasBinding and GetBindingValue then observe nothing but
//! that property. At each use the site checks that it runs for the record's
//! Environment, that the Environment has no declarative names, and that the
//! entry's descriptor is a data descriptor holding a value. Every other
//! case, an accessor, a deleted or absent property, a value still held as a
//! recipe, another Realm, reports a miss and leaves the read to the shared
//! runtime, having observed nothing: every step only reads.
//!
//! A `typeof` site also answers a name that is nowhere: its record then
//! holds a witness that the global object's dictionary had no entry for the
//! name at one table and insertion length, that its [[Prototype]] was one
//! ordinary object without the property and with a null [[Prototype]]. An
//! insertion grows a dictionary's length or replaces its table, a property
//! never joins immutable intrinsic rows, and both edges are read again, so
//! while the witness holds HasProperty finds nothing, observes nothing, and
//! the operand is an unresolvable Reference: `typeof` reads undefined.
//!
//! A site whose record misses asks for a new one, a bounded number of times,
//! so a name defined after the site first ran, or deleted and defined again,
//! is answered inline once more.
use super::*;
use super::environment_reference::EnvironmentIdentifierRead;
use crate::gc_types::{
    GcOperand, GlobalAbsenceWitness, GlobalAbsenceWitnessSchema, GlobalReadSiteRecord,
    GlobalReadSiteRecordSchema, IntrinsicPropertyStorage, JavaScriptObject,
    ObjectPropertyStorage, OrdinaryObject, OrdinaryObjectSchema, OrdinaryPropertyStorage,
    OrdinaryPropertyStorageSchema, PropertyDescriptor, PropertyDescriptorSchema, PropertyEntry,
    PropertyEntrySchema, PropertyTable, PropertyValue, RealmRecord, RealmRecordSchema,
    SiteRecordGlobal, StoredValueSchema,
};
use crate::emit::ControlTarget;
use crate::heap::DescriptorMask;
use crate::operations::PropertyKeyLocals;
use crate::runtime_helpers::{
    GlobalReadRecordArguments, GlobalReadRecordParameters, HelperParameters,
};

/// How many records a site asks for after its first.
const GLOBAL_READ_RECORD_REMAKES: i32 = 8;

impl FunctionBuilder<'_> {
    /// Reads the global binding `name` into `value` from this site's record
    /// and reports whether it did; a `typeof` read of a name the record
    /// proves unresolvable reads undefined. A miss leaves `value` untouched
    /// and has observed nothing.
    pub(crate) fn emit_global_read_site_i32(
        &mut self,
        global: &GcLocal<Environment, Nullable>,
        name: &str,
        read: EnvironmentIdentifierRead,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let site = self.next_site_record::<GlobalReadSiteRecord>()?;
        self.emit_global_read_site_with_i32(site, global, name, read, value, function)
    }

    /// `emit_global_read_site_i32` for a record global the caller already
    /// took, so that it can also read the record itself.
    pub(crate) fn emit_global_read_site_with_i32(
        &mut self,
        site: SiteRecordGlobal<GlobalReadSiteRecord>,
        global: &GcLocal<Environment, Nullable>,
        name: &str,
        read: EnvironmentIdentifierRead,
        value: &ValueLocals,
        function: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let found = schema.reserve_i32_local(function);
        found.set_constant(0, function);
        let record = schema
            .reserve_gc_local::<GlobalReadSiteRecord, Nullable>(function)
            .initialize(site.load(schema, function), function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let miss = self.open_frame(ControlFrameKind::Block, function);
        let label = self.local_branch_label(miss);
        record.load(schema, function).is_null(function);
        self.emit_branch_if_to_target(miss, function);
        let data = self.emit_global_read_record_data_or_branch(&record, global, miss, function);
        let stored = schema
            .reserve_gc_local::<StoredValue, Nullable>(function)
            .initialize(
                data.sealed_subtype_or_branch::<StoredValue>(schema, label, function)
                    .nullable(),
                function,
            );
        data.clear(function);
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, value, schema, function);
        stored.clear(function);
        found.set_constant(1, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        if matches!(read, EnvironmentIdentifierRead::Typeof) {
            self.emit_global_absence_site(&record, global, value, found, done, function);
        }
        // The site's first use, and a bounded number of later misses, ask R
        // for a record of what the name is now.
        let remakes = schema.reserve_i32_local(function);
        remakes.set_constant(0, function);
        record.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        schema
            .field(GlobalReadSiteRecordSchema::REMAKES)
            .read(&record, schema, function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        remakes.store(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        remakes.load(function);
        function.instruction(&Instruction::I32Const(GLOBAL_READ_RECORD_REMAKES));
        function.instruction(&Instruction::I32LeU);
        self.open_frame(ControlFrameKind::If, function);
        // Bounded per site: its first use and a few remakes.
        function.rarely_reached();
        let key = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.emit_interned_string_reference(name, function)?,
                function,
            );
        let made = schema.helper_reference_on_stack(schema.call_helper(
            GlobalReadRecordArguments::new(&key, global, remakes),
            self.runtime_helper_base()?,
            function,
        ));
        site.store(made, function);
        key.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i32_local(remakes, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        record.clear(function);
        Ok(found)
    }

    /// The current data value of the property a non-null record's entry
    /// holds, after branching to `miss` unless the record is for the
    /// Environment `global`, that Environment has no declarative names and
    /// the entry's descriptor is a data descriptor: the value, or the
    /// recipe that will make it, of the binding the site reads.
    pub(crate) fn emit_global_read_record_data_or_branch(
        &mut self,
        record: &GcLocal<GlobalReadSiteRecord, Nullable>,
        global: &GcLocal<Environment, Nullable>,
        miss: ControlTarget,
        function: &mut Function,
    ) -> GcLocal<PropertyValue, Nullable> {
        let schema = self.runtime_schema();
        // A declined record names no Environment; a null one never matches.
        let _ = schema
            .field(GlobalReadSiteRecordSchema::ENVIRONMENT)
            .read(record, schema, function)
            .reference();
        let _ = global.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        let _ = schema
            .struct_type::<Environment>()
            .field(EnvironmentSchema::NAMED_BINDINGS)
            .read(global, schema, function)
            .reference()
            .is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(miss, function);
        // A record that names an Environment holds an entry.
        let entry = schema
            .reserve_gc_local::<PropertyEntry, Nullable>(function)
            .initialize(
                schema
                    .field(GlobalReadSiteRecordSchema::ENTRY)
                    .read(record, schema, function)
                    .reference(),
                function,
            );
        let descriptor = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize(
                schema
                    .field(PropertyEntrySchema::DESCRIPTOR)
                    .read(&entry, schema, function)
                    .reference()
                    .nullable(),
                function,
            );
        entry.clear(function);
        schema
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, schema, function);
        function.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        function.instruction(&Instruction::I32And);
        self.emit_branch_if_to_target(miss, function);
        let data = schema
            .reserve_gc_local::<PropertyValue, Nullable>(function)
            .initialize(
                schema
                    .field(PropertyDescriptorSchema::VALUE)
                    .read(&descriptor, schema, function)
                    .reference()
                    .nullable(),
                function,
            );
        descriptor.clear(function);
        data
    }

    /// Answers undefined into `value`, sets `found` and branches to `done`
    /// when the record's witness still proves the name absent from the
    /// Global Environment Record `global`; otherwise falls through. A
    /// witness holds every object it reads, so its checks need no order.
    fn emit_global_absence_site(
        &mut self,
        record: &GcLocal<GlobalReadSiteRecord, Nullable>,
        global: &GcLocal<Environment, Nullable>,
        value: &ValueLocals,
        found: I32Local,
        done: ControlTarget,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let unproved = self.open_frame(ControlFrameKind::Block, function);
        record.load(schema, function).is_null(function);
        self.emit_branch_if_to_target(unproved, function);
        let witness = schema
            .reserve_gc_local::<GlobalAbsenceWitness, Nullable>(function)
            .initialize(
                schema
                    .field(GlobalReadSiteRecordSchema::ABSENCE)
                    .read(record, schema, function)
                    .reference(),
                function,
            );
        witness.load(schema, function).is_null(function);
        self.emit_branch_if_to_target(unproved, function);
        // The site's own Environment, with no declarative names.
        let _ = schema
            .field(GlobalAbsenceWitnessSchema::ENVIRONMENT)
            .read(&witness, schema, function)
            .reference();
        let _ = global.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        let _ = schema
            .struct_type::<Environment>()
            .field(EnvironmentSchema::NAMED_BINDINGS)
            .read(global, schema, function)
            .reference()
            .is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        // The global object's dictionary: the same table, nothing inserted.
        let dictionary = schema
            .reserve_gc_local::<OrdinaryPropertyStorage, Nullable>(function)
            .initialize(
                schema
                    .field(GlobalAbsenceWitnessSchema::DICTIONARY)
                    .read(&witness, schema, function)
                    .reference(),
                function,
            );
        let _ = schema
            .field(OrdinaryPropertyStorageSchema::ENTRIES)
            .read(&dictionary, schema, function)
            .reference();
        let _ = schema
            .field(GlobalAbsenceWitnessSchema::TABLE)
            .read(&witness, schema, function)
            .reference();
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        schema
            .field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&dictionary, schema, function);
        schema
            .field(GlobalAbsenceWitnessSchema::LENGTH)
            .read(&witness, schema, function);
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::I32Or);
        dictionary.clear(function);
        // Its [[Prototype]] is still the witnessed parent, whose storage is
        // the witnessed one and whose own [[Prototype]] is still null.
        let header = schema
            .reserve_gc_local::<OrdinaryObject, Nullable>(function)
            .initialize(
                schema
                    .field(GlobalAbsenceWitnessSchema::HEADER)
                    .read(&witness, schema, function)
                    .reference(),
                function,
            );
        let parent = schema
            .reserve_gc_local::<OrdinaryObject, Nullable>(function)
            .initialize(
                schema
                    .field(GlobalAbsenceWitnessSchema::PARENT)
                    .read(&witness, schema, function)
                    .reference(),
                function,
            );
        let _ = schema
            .field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&header, schema, function)
            .reference();
        let _ = parent.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        header.clear(function);
        let _ = schema
            .field(OrdinaryObjectSchema::PROPERTIES)
            .read(&parent, schema, function)
            .reference();
        let _ = schema
            .field(GlobalAbsenceWitnessSchema::PARENT_STORAGE)
            .read(&witness, schema, function)
            .reference();
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        let _ = schema
            .field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&parent, schema, function)
            .reference()
            .is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Or);
        parent.clear(function);
        self.emit_branch_if_to_target(unproved, function);
        // A parent dictionary: the same table, nothing inserted. Intrinsic
        // rows never change under the same storage.
        let dictionary = schema
            .reserve_gc_local::<OrdinaryPropertyStorage, Nullable>(function)
            .initialize(
                schema
                    .field(GlobalAbsenceWitnessSchema::PARENT_DICTIONARY)
                    .read(&witness, schema, function)
                    .reference(),
                function,
            );
        dictionary.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        let _ = schema
            .field(OrdinaryPropertyStorageSchema::ENTRIES)
            .read(&dictionary, schema, function)
            .reference();
        let _ = schema
            .field(GlobalAbsenceWitnessSchema::PARENT_TABLE)
            .read(&witness, schema, function)
            .reference();
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32Eqz);
        schema
            .field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&dictionary, schema, function);
        schema
            .field(GlobalAbsenceWitnessSchema::PARENT_LENGTH)
            .read(&witness, schema, function);
        function.instruction(&Instruction::I32Ne);
        function.instruction(&Instruction::I32Or);
        self.emit_branch_if_to_target(unproved, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        dictionary.clear(function);
        witness.clear(function);
        value.set_undefined(function);
        found.set_constant(1, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
    }

    /// Makes a global read site's record for `name` in the Global
    /// Environment Record the site runs for: the entry record of the name's
    /// own property when the Realm's global object is an ordinary object
    /// whose storage is a dictionary that holds one; a witness of absence
    /// when neither that dictionary nor its one ordinary prototype has the
    /// name; and a declined record, with neither, otherwise. Other storage
    /// is left as it is, so making a record changes nothing but the name's
    /// remembered position.
    pub(crate) fn compile_global_read_record_helper(&mut self) -> Result<Function, EmitError> {
        let mut body = self.begin_helper_body(RuntimeHelperId::GlobalReadRecord);
        let p = self.helper_parameters::<GlobalReadRecordParameters>(&mut body);
        let f = &mut body;
        let s = self.runtime_schema();
        let declined = self.open_frame(ControlFrameKind::Block, f);
        let label = self.local_branch_label(declined);
        let realm = s.reserve_gc_local::<RealmRecord, Nullable>(f).initialize(
            s.struct_type::<Environment>()
                .field(EnvironmentSchema::DEFINING_REALM)
                .read(&p.global_environment, s, f)
                .reference(),
            f,
        );
        realm.load(s, f).is_null(f);
        self.emit_branch_if_to_target(declined, f);
        let slot = s.reserve_gc_local::<StoredValue, Nullable>(f).initialize(
            s.struct_type::<RealmRecord>()
                .field(RealmRecordSchema::GLOBAL_OBJECT)
                .read(&realm, s, f)
                .reference(),
            f,
        );
        realm.clear(f);
        slot.load(s, f).is_null(f);
        self.emit_branch_if_to_target(declined, f);
        let base = s.reserve_value_local(f);
        s.field(StoredValueSchema::REFERENCE)
            .read(&slot, s, f)
            .store_eq(base.reference(), f);
        slot.clear(f);
        let header = s
            .reserve_gc_local::<OrdinaryObject, NonNullable>(f)
            .initialize(
                base.sealed_reference_or_branch::<OrdinaryObject>(s, label, f),
                f,
            );
        base.clear(f);
        let storage = s
            .reserve_gc_local::<ObjectPropertyStorage, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROPERTIES)
                    .read(&header, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        let dictionary = s
            .reserve_gc_local::<OrdinaryPropertyStorage, Nullable>(f)
            .initialize(
                storage
                    .sealed_subtype_or_branch::<OrdinaryPropertyStorage>(s, label, f)
                    .nullable(),
                f,
            );
        storage.clear(f);
        let key = PropertyKeyLocals::from_string(s, &p.name, f);
        let entry = self.emit_ordinary_property_entry(&header, &key, f)?;
        entry.load(s, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let _ = s.struct_type::<GlobalReadSiteRecord>().construct(
            (
                GcOperand::reference(&p.global_environment, s),
                GcOperand::reference(&entry, s),
                GcOperand::null(s),
                GcOperand::i32_local(p.remakes),
            ),
            f,
        );
        f.instruction(&Instruction::Return);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        entry.clear(f);
        // The global object has no such property. The witness also needs
        // its [[Prototype]] to be one ordinary object, with a null
        // [[Prototype]] of its own and no such property in intrinsic rows
        // or a dictionary; the probe reads without changing the storage.
        let table = s.reserve_gc_local::<PropertyTable, Nullable>(f).initialize(
            s.field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(&dictionary, s, f)
                .reference()
                .nullable(),
            f,
        );
        let length = s.reserve_i32_local(f);
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&dictionary, s, f)
            .store(length, f);
        let edge = s
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROTOTYPE)
                    .read(&header, s, f)
                    .reference(),
                f,
            );
        let parent = s
            .reserve_gc_local::<OrdinaryObject, NonNullable>(f)
            .initialize(
                edge.sealed_subtype_or_branch::<OrdinaryObject>(s, label, f),
                f,
            );
        edge.clear(f);
        let _ = s
            .field(OrdinaryObjectSchema::PROTOTYPE)
            .read(&parent, s, f)
            .reference()
            .is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        let absent = self.emit_ordinary_header_own_property_absent_i32(&parent, &key, f)?;
        key.clear(f);
        absent.load(f);
        f.instruction(&Instruction::I32Eqz);
        s.release_i32_local(absent, f);
        self.emit_branch_if_to_target(declined, f);
        let parent_storage = s
            .reserve_gc_local::<ObjectPropertyStorage, Nullable>(f)
            .initialize(
                s.field(OrdinaryObjectSchema::PROPERTIES)
                    .read(&parent, s, f)
                    .reference()
                    .nullable(),
                f,
            );
        let parent_dictionary = s
            .reserve_gc_local::<OrdinaryPropertyStorage, Nullable>(f)
            .initialize_null(s, f);
        let parent_table = s
            .reserve_gc_local::<PropertyTable, Nullable>(f)
            .initialize_null(s, f);
        let parent_length = s.reserve_i32_local(f);
        parent_length.set_constant(0, f);
        parent_storage.is_subtype::<OrdinaryPropertyStorage>(s, f);
        self.open_frame(ControlFrameKind::If, f);
        parent_dictionary.replace(
            parent_storage
                .load(s, f)
                .cast_subtype::<OrdinaryPropertyStorage>(s, f)
                .nullable(),
            f,
        );
        parent_table.replace(
            s.field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(&parent_dictionary, s, f)
                .reference()
                .nullable(),
            f,
        );
        s.field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&parent_dictionary, s, f)
            .store(parent_length, f);
        f.instruction(&Instruction::Else);
        // Any other storage than intrinsic rows can gain a key in place.
        parent_storage.is_subtype::<IntrinsicPropertyStorage>(s, f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(declined, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let witness = s
            .reserve_gc_local::<GlobalAbsenceWitness, Nullable>(f)
            .initialize(
                s.struct_type::<GlobalAbsenceWitness>()
                    .construct(
                        (
                            GcOperand::reference(&p.global_environment, s),
                            GcOperand::reference(&dictionary, s),
                            GcOperand::reference(&table, s),
                            GcOperand::i32_local(length),
                            GcOperand::nullable_reference(&header, s),
                            GcOperand::nullable_reference(&parent, s),
                            GcOperand::reference(&parent_storage, s),
                            GcOperand::reference(&parent_dictionary, s),
                            GcOperand::reference(&parent_table, s),
                            GcOperand::i32_local(parent_length),
                        ),
                        f,
                    )
                    .nullable(),
                f,
            );
        let _ = s.struct_type::<GlobalReadSiteRecord>().construct(
            (
                GcOperand::null(s),
                GcOperand::null(s),
                GcOperand::reference(&witness, s),
                GcOperand::i32_local(p.remakes),
            ),
            f,
        );
        f.instruction(&Instruction::Return);
        witness.clear(f);
        parent.clear(f);
        header.clear(f);
        s.release_i32_local(parent_length, f);
        parent_table.clear(f);
        parent_dictionary.clear(f);
        parent_storage.clear(f);
        s.release_i32_local(length, f);
        table.clear(f);
        dictionary.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        let _ = s.struct_type::<GlobalReadSiteRecord>().construct(
            (
                GcOperand::null(s),
                GcOperand::null(s),
                GcOperand::null(s),
                GcOperand::i32_local(p.remakes),
            ),
            f,
        );
        p.release(f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(body))
    }
}
