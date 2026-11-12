//! A data property that a static read finds on the receiver's direct
//! prototype, or the absence of the key on that prototype and beyond it.
//!
//! The site record says where the value lives on the prototype: an intrinsic
//! layout row, a compact shape slot or a dictionary position. It retains no
//! object, value or Realm, only shapes and integers, and it names no
//! particular prototype: each use proves again that the receiver has no own
//! property for the key and that its live [[Prototype]] holds the property
//! at that place. An absence record instead re-proves that the prototype's
//! immutable layout or shape lacks the key and that its own chain is one of
//! the two closed tails of the own-data cache: null, or an unmodified
//! %Object.prototype%. Nothing invalidates a record from outside: a use that
//! cannot prove every step falls through to the general Get.
use super::*;
use crate::functions::NonArrayRealmIntrinsicSlot;
use crate::gc_types::{
    ArrayObject, ArrayObjectSchema, CodeUnitArray, FunctionObject, FunctionObjectSchema,
    GcNullability, IntrinsicPropertyValueCache, JavaScriptObject, MapObject, MapObjectSchema,
    PrimitiveBox, PrimitiveBoxSchema, PromiseObject, PromiseObjectSchema, PropertyValue,
    RealmRecord, SetObject, SetObjectSchema, StringValue, StringValueSchema, WeakMapObject,
    WeakMapObjectSchema, WeakSetObject, WeakSetObjectSchema,
};

/// The layout word of a record that names no intrinsic layout. Layout
/// addresses are offsets into static data and never negative.
pub(super) const NO_INTRINSIC_LAYOUT: i32 = -1;

/// How a dictionary read takes a data value that is still a lazy recipe.
#[derive(Clone, Copy)]
pub(super) enum RecipeValues {
    /// Resolve it through the resolver helper.
    Resolve,
    /// Leave it unread, so the read makes no call.
    Decline,
}

/// How a use proves that the receiver has no own property for the key.
#[derive(Clone, Copy)]
pub(super) enum HolderReceiverKind {
    /// No prototype record.
    None,
    /// An object whose named own properties all live in its ordinary header:
    /// a compact shape that lacks the key, or a dictionary that never held an
    /// entry. Index keys and "length" never get a record.
    Object,
    /// A String primitive: its own keys are indices and "length".
    String,
    /// A Number primitive: it has no own keys.
    Number,
}

impl HolderReceiverKind {
    pub(super) const fn word(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Object => 1,
            Self::String => 2,
            Self::Number => 3,
        }
    }
}

/// An object examined by its layout alone: a value's reference, or a
/// [[Prototype]] edge that has not been given a value tag.
#[derive(Clone, Copy)]
enum LayoutSubject<'a> {
    Value(&'a ValueLocals),
    Prototype(&'a GcLocal<JavaScriptObject, Nullable>),
}

impl LayoutSubject<'_> {
    fn has_layout<T: JavaScriptReference>(self, schema: &RuntimeSchema, f: &mut Function) {
        match self {
            Self::Value(value) => {
                value.reference().load(f);
                f.instruction(&Instruction::RefTestNonNull(
                    schema
                        .reference_type::<T>(GcNullability::NonNullable)
                        .heap_type,
                ));
            }
            Self::Prototype(reference) => reference.has_layout::<T>(schema, f),
        }
    }

    fn cast_layout<T: JavaScriptReference>(
        self,
        schema: &RuntimeSchema,
        f: &mut Function,
    ) -> GcStackReference<T> {
        match self {
            Self::Value(value) => value.cast_reference::<T>(schema, f),
            Self::Prototype(reference) => reference.cast_layout::<T>(schema, f),
        }
    }
}

impl FunctionBuilder<'_> {
    /// The ordinary header of an object whose named own properties, other than
    /// array indices and "length", all live in that header. Every other
    /// object kind, and null, yields null: Proxies, typed arrays, arguments
    /// objects and module namespaces answer [[GetOwnProperty]] differently.
    fn emit_named_property_header(
        &mut self,
        target: LayoutSubject<'_>,
        f: &mut Function,
    ) -> GcLocal<OrdinaryObject, Nullable> {
        let schema = self.runtime_schema();
        let result = schema
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(schema, f);
        target.has_layout::<OrdinaryObject>(schema, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        result.replace(
            target.cast_layout::<OrdinaryObject>(schema, f).nullable(),
            f,
        );
        let mut open = 1usize;
        macro_rules! composite {
            ($layout:ty, $field:expr) => {{
                f.instruction(&Instruction::Else);
                target.has_layout::<$layout>(schema, f);
                f.instruction(&Instruction::If(BlockType::Empty));
                let composite = schema
                    .reserve_gc_local(f)
                    .initialize(target.cast_layout::<$layout>(schema, f), f);
                result.replace(
                    schema
                        .field($field)
                        .read(&composite, schema, f)
                        .reference()
                        .nullable(),
                    f,
                );
                composite.clear(f);
                open += 1;
            }};
        }
        composite!(FunctionObject, FunctionObjectSchema::OBJECT);
        composite!(ArrayObject, ArrayObjectSchema::OBJECT);
        composite!(PrimitiveBox, PrimitiveBoxSchema::OBJECT);
        composite!(MapObject, MapObjectSchema::OBJECT);
        composite!(WeakMapObject, WeakMapObjectSchema::OBJECT);
        composite!(SetObject, SetObjectSchema::OBJECT);
        composite!(WeakSetObject, WeakSetObjectSchema::OBJECT);
        composite!(PromiseObject, PromiseObjectSchema::OBJECT);
        for _ in 0..open {
            f.instruction(&Instruction::End);
        }
        result
    }

    /// Reads the data value of the dictionary entry at `position` when that
    /// entry is live and holds exactly this key object. A deleted entry is a
    /// null slot, and an entry keeps its position for as long as it lives.
    pub(crate) fn emit_dictionary_position_read(
        &mut self,
        storage: &GcLocal<OrdinaryPropertyStorage>,
        position: I32Local,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) {
        self.emit_dictionary_position_read_with(
            storage,
            position,
            key,
            value,
            found,
            RecipeValues::Resolve,
            f,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_dictionary_position_read_with(
        &mut self,
        storage: &GcLocal<OrdinaryPropertyStorage>,
        position: I32Local,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        recipes: RecipeValues,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        found.set_constant(0, f);
        let length = schema.reserve_i32_local(f);
        schema
            .field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(storage, schema, f)
            .store(length, f);
        position.load(f);
        length.load(f);
        f.instruction(&Instruction::I32LtU);
        schema.release_i32_local(length, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        let table = schema.reserve_gc_local(f).initialize(
            schema
                .field(OrdinaryPropertyStorageSchema::ENTRIES)
                .read(storage, schema, f)
                .reference(),
            f,
        );
        position.load(f);
        schema.array_type::<PropertyTable>().length(&table, schema, f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        let entry = schema
            .reserve_gc_local::<PropertyEntry, Nullable>(f)
            .initialize(
                schema
                    .array_type::<PropertyTable>()
                    .read(&table, position, schema, f)
                    .reference(),
                f,
            );
        entry.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        let present = schema
            .reserve_gc_local(f)
            .initialize(entry.load(schema, f).require_non_null(f), f);
        let stored_key = schema.reserve_gc_local(f).initialize(
            schema
                .field(PropertyEntrySchema::KEY)
                .read(&present, schema, f)
                .reference(),
            f,
        );
        schema
            .field(StoredValueSchema::REFERENCE)
            .read(&stored_key, schema, f);
        key.value().reference().load(f);
        f.instruction(&Instruction::RefEq);
        stored_key.clear(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        let descriptor = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(f)
            .initialize(
                schema
                    .field(PropertyEntrySchema::DESCRIPTOR)
                    .read(&present, schema, f)
                    .reference()
                    .nullable(),
                f,
            );
        let flags = schema.reserve_i32_local(f);
        schema
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, schema, f)
            .store(flags, f);
        flags.load(f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        schema.release_i32_local(flags, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        match recipes {
            RecipeValues::Resolve => {
                self.emit_descriptor_data_value_into(&descriptor, value, f);
                found.set_constant(1, f);
            }
            RecipeValues::Decline => {
                let data = schema.reserve_gc_local(f).initialize(
                    schema
                        .field(PropertyDescriptorSchema::VALUE)
                        .read(&descriptor, schema, f)
                        .reference(),
                    f,
                );
                data.is_subtype::<StoredValue>(schema, f);
                f.instruction(&Instruction::If(BlockType::Empty));
                let stored = schema.reserve_gc_local(f).initialize(
                    data.load(schema, f).cast_subtype::<StoredValue>(schema, f),
                    f,
                );
                self.emit_stored_value_to_locals(&stored, value, f);
                stored.clear(f);
                found.set_constant(1, f);
                f.instruction(&Instruction::End);
                data.clear(f);
            }
        }
        f.instruction(&Instruction::End);
        descriptor.clear(f);
        f.instruction(&Instruction::End);
        present.clear(f);
        f.instruction(&Instruction::End);
        entry.clear(f);
        f.instruction(&Instruction::End);
        table.clear(f);
        f.instruction(&Instruction::End);
    }

    /// True when the first code unit of a String key is a decimal digit.
    /// Every canonical array index starts with one; such keys never get a
    /// prototype record because Arrays and Strings own them outside the header.
    fn emit_key_starts_with_digit_i32(&self, key: &PropertyKeyLocals, f: &mut Function) {
        let schema = self.runtime_schema();
        let string = schema.reserve_gc_local(f).initialize(
            key.value().cast_reference::<StringValue>(schema, f),
            f,
        );
        let units = schema.reserve_gc_local(f).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(&string, schema, f)
                .reference(),
            f,
        );
        string.clear(f);
        let first = schema.reserve_i32_local(f);
        first.set_constant(0, f);
        schema.array_type::<CodeUnitArray>().length(&units, schema, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        let zero = schema.reserve_i32_local(f);
        zero.set_constant(0, f);
        schema
            .array_type::<CodeUnitArray>()
            .read(&units, zero, schema, f)
            .store(first, f);
        schema.release_i32_local(zero, f);
        f.instruction(&Instruction::End);
        units.clear(f);
        first.load(f);
        f.instruction(&Instruction::I32Const(i32::from(b'0')));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32LtU);
        schema.release_i32_local(first, f);
    }

    /// The prototype-record stage of a static read: use the site's record
    /// when the live receiver, its prototype and the holder still match it;
    /// otherwise look one object up the chain and record a data property
    /// found there. `handled` is set only together with a normal `result`.
    pub(super) fn emit_prototype_record_read(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        cache: &GcLocal<OwnDataReadCache>,
        result: &CompletionLocals,
        handled: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let done = self.open_frame(ControlFrameKind::Block, f);

        // The live receiver: which absence rule applies, its compact shape if
        // it has one, and its [[Prototype]].
        let live_kind = schema.reserve_i32_local(f);
        live_kind.set_constant(HolderReceiverKind::None.word(), f);
        let shape = schema
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(schema, f);
        let own_compact = schema
            .reserve_gc_local::<CompactDataStorage, Nullable>(f)
            .initialize_null(schema, f);
        // The prototype's ordinary header, shared by the use and the record.
        // Only the prototype's layout is examined, so its edge needs no tag.
        let holder_header = schema
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(schema, f);
        let value = schema.reserve_value_local(f);
        value.set_undefined(f);
        let found = schema.reserve_i32_local(f);
        found.set_constant(0, f);

        self.emit_is_heap_object_like_tag_i32(target.tag(), f);
        self.open_frame(ControlFrameKind::If, f);
        let header = self.emit_named_property_header(LayoutSubject::Value(target), f);
        header.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let object = schema
            .reserve_gc_local(f)
            .initialize(header.load(schema, f).require_non_null(f), f);
        let base = schema.reserve_gc_local(f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&object, schema, f)
                .reference(),
            f,
        );
        base.is_subtype::<CompactDataStorage>(schema, f);
        self.open_frame(ControlFrameKind::If, f);
        own_compact.replace(
            base.load(schema, f)
                .cast_subtype::<CompactDataStorage>(schema, f)
                .nullable(),
            f,
        );
        let compact = schema
            .reserve_gc_local(f)
            .initialize(own_compact.load(schema, f).require_non_null(f), f);
        shape.replace(
            schema
                .field(CompactDataStorageSchema::SHAPE)
                .read(&compact, schema, f)
                .reference()
                .nullable(),
            f,
        );
        compact.clear(f);
        live_kind.set_constant(HolderReceiverKind::Object.word(), f);
        f.instruction(&Instruction::Else);
        base.is_subtype::<OrdinaryPropertyStorage>(schema, f);
        self.open_frame(ControlFrameKind::If, f);
        let dictionary = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<OrdinaryPropertyStorage>(schema, f),
            f,
        );
        let length = schema.reserve_i32_local(f);
        schema
            .field(OrdinaryPropertyStorageSchema::LENGTH)
            .read(&dictionary, schema, f)
            .store(length, f);
        dictionary.clear(f);
        // Every live entry sits below the insertion extent: zero means the
        // dictionary has no entries now.
        length.load(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        live_kind.set_constant(HolderReceiverKind::Object.word(), f);
        f.instruction(&Instruction::End);
        schema.release_i32_local(length, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        let prototype = schema
            .reserve_gc_local::<JavaScriptObject, Nullable>(f)
            .initialize(
                schema
                    .field(OrdinaryObjectSchema::PROTOTYPE)
                    .read(&object, schema, f)
                    .reference(),
                f,
            );
        let projected = self.emit_named_property_header(LayoutSubject::Prototype(&prototype), f);
        holder_header.replace(projected.load(schema, f), f);
        projected.clear(f);
        prototype.clear(f);
        base.clear(f);
        object.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        header.clear(f);
        f.instruction(&Instruction::Else);
        for (tag, kind, slot) in [
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
        ] {
            target.tag().load(f);
            f.instruction(&Instruction::I32Const(tag as i32));
            f.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, f);
            // ToObject boxes a primitive with the current Realm's prototype.
            let realm: GcLocal<RealmRecord> = self.load_current_realm(f);
            let prototype_value = schema.reserve_value_local(f);
            self.emit_load_non_array_realm_intrinsic(&realm, slot, &prototype_value, f);
            realm.clear(f);
            let projected =
                self.emit_named_property_header(LayoutSubject::Value(&prototype_value), f);
            holder_header.replace(projected.load(schema, f), f);
            projected.clear(f);
            prototype_value.clear(f);
            live_kind.set_constant(kind.word(), f);
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
        }
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        live_kind.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(done, f);
        holder_header.load(schema, f).is_null(f);
        self.emit_branch_if_to_target(done, f);
        let holder = schema
            .reserve_gc_local(f)
            .initialize(holder_header.load(schema, f).require_non_null(f), f);

        // Use: same absence rule and shape; the place is then proved on the
        // live prototype, whichever object that is. An empty record has no
        // receiver rule and never matches.
        live_kind.load(f);
        let recorded_kind = schema.reserve_i32_local(f);
        schema
            .field(OwnDataReadCacheSchema::RECEIVER_KIND)
            .read(cache, schema, f)
            .store(recorded_kind, f);
        recorded_kind.load(f);
        schema.release_i32_local(recorded_kind, f);
        f.instruction(&Instruction::I32Eq);
        let _ = shape.load(schema, f);
        schema
            .field(OwnDataReadCacheSchema::RECEIVER_SHAPE)
            .read(cache, schema, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_recorded_holder_value(cache, &holder, key, &value, found, f)?;
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        result.set_normal(&value, f);
        handled.set_constant(1, f);
        self.emit_branch_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        // Record: "length" and index keys are owned outside the header by
        // Arrays and Strings, so they never get a record.
        let length_key = schema.reserve_gc_local(f).initialize(
            self.emit_interned_string_reference("length", f)?,
            f,
        );
        let _ = length_key.load(schema, f);
        key.value().reference().load(f);
        f.instruction(&Instruction::RefEq);
        length_key.clear(f);
        self.emit_branch_if_to_target(done, f);
        self.emit_key_starts_with_digit_i32(key, f);
        self.emit_branch_if_to_target(done, f);

        // A compact receiver proves absence by its shape; ask the shape once.
        own_compact.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let compact = schema
            .reserve_gc_local(f)
            .initialize(own_compact.load(schema, f).require_non_null(f), f);
        let own_slot = schema.reserve_i32_local(f);
        let own_found = schema.reserve_i32_local(f);
        self.emit_compact_slot_probe(&compact, key, own_slot, own_found, f)?;
        compact.clear(f);
        own_found.load(f);
        schema.release_i32_local(own_found, f);
        schema.release_i32_local(own_slot, f);
        self.emit_branch_if_to_target(done, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        let holder_base = schema.reserve_gc_local(f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(&holder, schema, f)
                .reference(),
            f,
        );
        let holder_shape = schema
            .reserve_gc_local::<CompactDataShape, Nullable>(f)
            .initialize_null(schema, f);
        let holder_layout = schema.reserve_i32_local(f);
        holder_layout.set_constant(NO_INTRINSIC_LAYOUT, f);
        let holder_slot = schema.reserve_i32_local(f);
        holder_slot.set_constant(0, f);
        found.set_constant(0, f);
        // Set when an immutable layout or shape proves the holder lacks the key.
        let absent = schema.reserve_i32_local(f);
        absent.set_constant(0, f);

        holder_base.is_subtype::<CompactDataStorage>(schema, f);
        self.open_frame(ControlFrameKind::If, f);
        let compact = schema.reserve_gc_local(f).initialize(
            holder_base
                .load(schema, f)
                .cast_subtype::<CompactDataStorage>(schema, f),
            f,
        );
        self.emit_compact_slot_probe(&compact, key, holder_slot, found, f)?;
        holder_shape.replace(
            schema
                .field(CompactDataStorageSchema::SHAPE)
                .read(&compact, schema, f)
                .reference()
                .nullable(),
            f,
        );
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_compact_value_at(&compact, holder_slot, &value, f);
        f.instruction(&Instruction::Else);
        absent.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        compact.clear(f);
        f.instruction(&Instruction::Else);
        holder_base.is_subtype::<IntrinsicPropertyStorage>(schema, f);
        self.open_frame(ControlFrameKind::If, f);
        let intrinsic = schema.reserve_gc_local(f).initialize(
            holder_base
                .load(schema, f)
                .cast_subtype::<IntrinsicPropertyStorage>(schema, f),
            f,
        );
        // A found row records its value group, where the layout keeps the
        // materialized value.
        self.emit_intrinsic_row_data_value(
            &intrinsic,
            key,
            &value,
            holder_slot,
            found,
            absent,
            f,
        )?;
        schema
            .field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(&intrinsic, schema, f)
            .store(holder_layout, f);
        intrinsic.clear(f);
        f.instruction(&Instruction::Else);
        // A dictionary holder: remember the entry position, checked by key.
        let entry = self.emit_ordinary_property_entry(&holder, key, f)?;
        entry.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let present = schema
            .reserve_gc_local(f)
            .initialize(entry.load(schema, f).require_non_null(f), f);
        schema
            .field(PropertyEntrySchema::POSITION)
            .read(&present, schema, f)
            .store(holder_slot, f);
        present.clear(f);
        let dictionary = schema.reserve_gc_local(f).initialize(
            holder_base
                .load(schema, f)
                .cast_subtype::<OrdinaryPropertyStorage>(schema, f),
            f,
        );
        self.emit_dictionary_position_read(&dictionary, holder_slot, key, &value, found, f);
        dictionary.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        entry.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        // The holder lacks the key. The read is undefined when the rest of
        // the chain is a closed tail that lacks it too; registering proves
        // that for an unmodified %Object.prototype%.
        absent.load(f);
        self.open_frame(ControlFrameKind::If, f);
        let holder_value = schema.reserve_value_local(f);
        holder_value.set_reference(&holder, schema, f);
        let mode = self.emit_negative_cache_mode(&holder_value, &holder, Some(key), f)?;
        holder_value.clear(f);
        mode.load(f);
        self.open_frame(ControlFrameKind::If, f);
        mode.load(f);
        holder_slot.store(f);
        value.set_undefined(f);
        found.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(mode, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(absent, f);

        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        schema.field(OwnDataReadCacheSchema::RECEIVER_KIND).write(
            cache,
            GcOperand::i32_local(live_kind),
            schema,
            f,
        );
        schema.field(OwnDataReadCacheSchema::RECEIVER_SHAPE).write(
            cache,
            GcOperand::reference(&shape, schema),
            schema,
            f,
        );
        schema.field(OwnDataReadCacheSchema::HOLDER_LAYOUT).write(
            cache,
            GcOperand::i32_local(holder_layout),
            schema,
            f,
        );
        schema.field(OwnDataReadCacheSchema::HOLDER_SHAPE).write(
            cache,
            GcOperand::reference(&holder_shape, schema),
            schema,
            f,
        );
        schema.field(OwnDataReadCacheSchema::HOLDER_SLOT).write(
            cache,
            GcOperand::i32_local(holder_slot),
            schema,
            f,
        );
        result.set_normal(&value, f);
        handled.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        schema.release_i32_local(holder_slot, f);
        schema.release_i32_local(holder_layout, f);
        holder_shape.clear(f);
        holder_base.clear(f);
        holder.clear(f);
        holder_header.clear(f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        schema.release_i32_local(found, f);
        value.clear(f);
        own_compact.clear(f);
        shape.clear(f);
        schema.release_i32_local(live_kind, f);
        Ok(())
    }

    /// The data value of an intrinsic row for `key`, when the layout has one,
    /// and the value group the layout's storage keeps it in once materialized.
    fn emit_intrinsic_row_data_value(
        &mut self,
        intrinsic: &GcLocal<IntrinsicPropertyStorage>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        group: I32Local,
        found: I32Local,
        absent: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        found.set_constant(0, f);
        absent.set_constant(0, f);
        let index = schema.reserve_i32_local(f);
        schema
            .call_helper(
                crate::runtime_helpers::IntrinsicPropertyFindArguments::new(intrinsic, key),
                self.runtime_helper_base()?,
                f,
            )
            .store(index, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        let row = self.emit_intrinsic_property_row(intrinsic, index, f);
        row.flags().load(f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_intrinsic_data_value_into(intrinsic, index, &row, value, f)?;
        row.group().load(f);
        group.store(f);
        found.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        row.clear(schema, f);
        f.instruction(&Instruction::Else);
        absent.set_constant(1, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(index, f);
        Ok(())
    }

    /// The value an intrinsic storage keeps for a value group once it has
    /// been materialized. A group still holding its recipe, or none, is left
    /// to the caller's search, which materializes it.
    pub(super) fn emit_materialized_intrinsic_group(
        &mut self,
        intrinsic: &GcLocal<IntrinsicPropertyStorage>,
        group_index: I32Local,
        value: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let groups = schema
            .reserve_gc_local::<IntrinsicPropertyValueCache, Nullable>(f)
            .initialize(
                schema
                    .field(IntrinsicPropertyStorageSchema::CACHE)
                    .read(intrinsic, schema, f)
                    .reference(),
                f,
            );
        groups.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        let materialized = schema
            .reserve_gc_local(f)
            .initialize(groups.load(schema, f).require_non_null(f), f);
        group_index.load(f);
        schema
            .array_type::<IntrinsicPropertyValueCache>()
            .length(&materialized, schema, f);
        f.instruction(&Instruction::I32LtU);
        f.instruction(&Instruction::If(BlockType::Empty));
        let group = schema
            .reserve_gc_local::<PropertyValue, Nullable>(f)
            .initialize(
                schema
                    .array_type::<IntrinsicPropertyValueCache>()
                    .read(&materialized, group_index, schema, f)
                    .reference(),
                f,
            );
        group.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        f.instruction(&Instruction::If(BlockType::Empty));
        let present = schema
            .reserve_gc_local(f)
            .initialize(group.load(schema, f).require_non_null(f), f);
        // A group still holding its recipe is left to the record path,
        // which materializes it.
        present.is_subtype::<StoredValue>(schema, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        let stored = schema.reserve_gc_local(f).initialize(
            present
                .load(schema, f)
                .cast_subtype::<StoredValue>(schema, f),
            f,
        );
        self.emit_stored_value_to_locals(&stored, value, f);
        stored.clear(f);
        found.set_constant(1, f);
        f.instruction(&Instruction::End);
        present.clear(f);
        f.instruction(&Instruction::End);
        group.clear(f);
        f.instruction(&Instruction::End);
        materialized.clear(f);
        f.instruction(&Instruction::End);
        groups.clear(f);
    }

    /// An own data property of a receiver on intrinsic storage. A layout is
    /// immutable, so storage at the recorded layout address has the key's
    /// row in the recorded value group; the value is read from this
    /// storage's own groups. A miss searches the layout and records a data
    /// row. Accessors and absent keys are left unhandled.
    pub(crate) fn emit_own_intrinsic_record_read(
        &mut self,
        intrinsic: &GcLocal<IntrinsicPropertyStorage>,
        key: &PropertyKeyLocals,
        cache: &GcLocal<OwnDataReadCache>,
        value: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        found.set_constant(0, f);
        let address = schema.reserve_i32_local(f);
        schema
            .field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(intrinsic, schema, f)
            .store(address, f);
        let group = schema.reserve_i32_local(f);
        group.set_constant(0, f);
        address.load(f);
        schema
            .field(OwnDataReadCacheSchema::LAYOUT)
            .read(cache, schema, f);
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        schema
            .field(OwnDataReadCacheSchema::GROUP)
            .read(cache, schema, f)
            .store(group, f);
        self.emit_materialized_intrinsic_group(intrinsic, group, value, found, f);
        f.instruction(&Instruction::End);
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let absent = schema.reserve_i32_local(f);
        self.emit_intrinsic_row_data_value(intrinsic, key, value, group, found, absent, f)?;
        schema.release_i32_local(absent, f);
        found.load(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        schema.field(OwnDataReadCacheSchema::LAYOUT).write(
            cache,
            GcOperand::i32_local(address),
            schema,
            f,
        );
        schema.field(OwnDataReadCacheSchema::GROUP).write(
            cache,
            GcOperand::i32_local(group),
            schema,
            f,
        );
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(group, f);
        schema.release_i32_local(address, f);
        Ok(())
    }

    /// The live value at the recorded place on the receiver's prototype.
    fn emit_recorded_holder_value(
        &mut self,
        cache: &GcLocal<OwnDataReadCache>,
        holder: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        found.set_constant(0, f);
        let base = schema.reserve_gc_local(f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(holder, schema, f)
                .reference(),
            f,
        );
        let slot = schema.reserve_i32_local(f);
        schema
            .field(OwnDataReadCacheSchema::HOLDER_SLOT)
            .read(cache, schema, f)
            .store(slot, f);
        base.is_subtype::<IntrinsicPropertyStorage>(schema, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        // An intrinsic layout is immutable: storage at the same layout
        // address has the same rows, and keeps each materialized value in
        // its row's group.
        let intrinsic = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<IntrinsicPropertyStorage>(schema, f),
            f,
        );
        schema
            .field(IntrinsicPropertyStorageSchema::ADDRESS)
            .read(&intrinsic, schema, f);
        schema
            .field(OwnDataReadCacheSchema::HOLDER_LAYOUT)
            .read(cache, schema, f);
        f.instruction(&Instruction::I32Eq);
        f.instruction(&Instruction::If(BlockType::Empty));
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.emit_materialized_intrinsic_group(&intrinsic, slot, value, found, f);
        f.instruction(&Instruction::Else);
        self.emit_recorded_absence(holder, slot, value, found, f)?;
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        intrinsic.clear(f);
        f.instruction(&Instruction::Else);
        base.is_subtype::<CompactDataStorage>(schema, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        // A compact shape is immutable: the same shape has the key at the same slot.
        let compact = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<CompactDataStorage>(schema, f),
            f,
        );
        schema
            .field(CompactDataStorageSchema::SHAPE)
            .read(&compact, schema, f);
        schema
            .field(OwnDataReadCacheSchema::HOLDER_SHAPE)
            .read(cache, schema, f);
        f.instruction(&Instruction::RefEq);
        f.instruction(&Instruction::If(BlockType::Empty));
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        f.instruction(&Instruction::If(BlockType::Empty));
        self.emit_compact_value_at(&compact, slot, value, f);
        found.set_constant(1, f);
        f.instruction(&Instruction::Else);
        self.emit_recorded_absence(holder, slot, value, found, f)?;
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        compact.clear(f);
        f.instruction(&Instruction::Else);
        base.is_subtype::<OrdinaryPropertyStorage>(schema, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        // A dictionary entry is confirmed by its own key at the position.
        let dictionary = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<OrdinaryPropertyStorage>(schema, f),
            f,
        );
        self.emit_dictionary_position_read(&dictionary, slot, key, value, found, f);
        dictionary.clear(f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        schema.release_i32_local(slot, f);
        base.clear(f);
        Ok(())
    }

    /// An absence record holds the closed tail it was registered with as a
    /// negative slot word. The holder's layout or shape was just re-proved;
    /// the read is undefined when its chain still ends in that same tail.
    pub(super) fn emit_recorded_absence(
        &mut self,
        holder: &GcLocal<OrdinaryObject>,
        slot: I32Local,
        value: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        slot.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32LtS);
        f.instruction(&Instruction::If(BlockType::Empty));
        let holder_value = schema.reserve_value_local(f);
        holder_value.set_reference(holder, schema, f);
        let mode = self.emit_negative_cache_mode(&holder_value, holder, None, f)?;
        holder_value.clear(f);
        mode.load(f);
        slot.load(f);
        f.instruction(&Instruction::I32Eq);
        schema.release_i32_local(mode, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        value.set_undefined(f);
        found.set_constant(1, f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        Ok(())
    }
}
