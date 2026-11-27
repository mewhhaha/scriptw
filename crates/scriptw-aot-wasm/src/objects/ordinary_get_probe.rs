//! Ordinary Get needs absence, a data value, or a getter with the original
//! receiver. Observe that result once without allocating descriptor snapshots.

use super::*;
use crate::gc_types::*;

#[derive(Clone, Copy)]
enum OrdinaryGetState {
    Unsupported,
    Absent,
    Data,
    Getter,
}

impl OrdinaryGetState {
    fn code(self) -> i32 {
        match self {
            Self::Unsupported => 0,
            Self::Absent => 1,
            Self::Data => 2,
            Self::Getter => 3,
        }
    }
}

/// What one own-property probe of an ordinary header found: a data
/// property, no property for the key, or something else (an accessor, or a
/// key the header does not answer alone).
#[derive(Clone, Copy)]
pub(crate) enum OwnDataProbe {
    Other,
    Data,
    Absent,
}

impl OwnDataProbe {
    pub(crate) const fn code(self) -> i32 {
        match self {
            Self::Other => 0,
            Self::Data => 1,
            Self::Absent => 2,
        }
    }
}

#[must_use = "an ordinary Get observation owns a rooted payload and must be cleared"]
pub(super) struct OrdinaryGetProbe {
    state: I32Local,
    value: ValueLocals,
}

impl OrdinaryGetProbe {
    fn new(schema: &RuntimeSchema, f: &mut Function) -> Self {
        let probe = Self {
            state: schema.reserve_i32_local(f),
            value: schema.reserve_value_local(f),
        };
        probe.set_state(OrdinaryGetState::Unsupported, f);
        probe.value.set_undefined(f);
        probe
    }

    fn set_state(&self, state: OrdinaryGetState, f: &mut Function) {
        self.state.set_constant(state.code(), f);
    }

    fn emit_state_i32(&self, state: OrdinaryGetState, f: &mut Function) {
        self.state.load(f);
        f.instruction(&Instruction::I32Const(state.code()));
        f.instruction(&Instruction::I32Eq);
    }

    pub(super) fn emit_supported_i32(&self, f: &mut Function) {
        self.emit_state_i32(OrdinaryGetState::Unsupported, f);
        f.instruction(&Instruction::I32Eqz);
    }

    pub(super) fn emit_found_i32(&self, f: &mut Function) {
        self.emit_state_i32(OrdinaryGetState::Data, f);
        self.emit_state_i32(OrdinaryGetState::Getter, f);
        f.instruction(&Instruction::I32Or);
    }

    pub(super) fn emit_getter_i32(&self, f: &mut Function) {
        self.emit_state_i32(OrdinaryGetState::Getter, f);
    }

    pub(super) fn value(&self) -> &ValueLocals {
        &self.value
    }

    /// Exotics retain their canonical descriptor acquisition. Copy only the
    /// immutable edge needed by Get before its descriptor root is released.
    pub(super) fn observe_completed_descriptor(
        &self,
        descriptor: &CompletedProxyTargetDescriptor,
        builder: &FunctionBuilder<'_>,
        f: &mut Function,
    ) {
        let schema = builder.runtime_schema();
        self.set_state(OrdinaryGetState::Absent, f);
        self.value.set_undefined(f);
        descriptor.emit_found_i32(schema, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        descriptor.emit_accessor_i32(schema, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        descriptor.read_getter(&self.value, schema, f);
        self.set_state(OrdinaryGetState::Getter, f);
        f.instruction(&Instruction::Else);
        descriptor.read_value(&self.value, builder, f);
        self.set_state(OrdinaryGetState::Data, f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
    }

    pub(super) fn clear(self, schema: &RuntimeSchema, f: &mut Function) {
        self.value.clear(f);
        schema.release_i32_local(self.state, f);
    }
}

impl FunctionBuilder<'_> {
    /// Reads an own data property of an ordinary or function object into
    /// `value` and reports whether it did. Absence, an accessor and every
    /// other object kind report false and leave the Get to its owner.
    pub(crate) fn emit_ordinary_own_data_read_i32(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let found = schema.reserve_i32_local(f);
        let probe = self.emit_ordinary_get_probe(target, key, f)?;
        probe.emit_state_i32(OrdinaryGetState::Data, f);
        found.store(f);
        found.load(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        value.copy_from(probe.value(), f);
        f.instruction(&Instruction::End);
        probe.clear(schema, f);
        Ok(found)
    }

    /// The same own data read for a header that the caller already selected.
    pub(crate) fn emit_ordinary_header_own_data_read_i32(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let found = schema.reserve_i32_local(f);
        let probe = OrdinaryGetProbe::new(schema, f);
        self.emit_ordinary_get_from_header(object, key, &probe, f)?;
        probe.emit_state_i32(OrdinaryGetState::Data, f);
        found.store(f);
        found.load(f);
        f.instruction(&Instruction::If(BlockType::Empty));
        value.copy_from(probe.value(), f);
        f.instruction(&Instruction::End);
        probe.clear(schema, f);
        Ok(found)
    }

    /// One own-property probe of the selected ordinary header: a data value
    /// is copied into `value`. The result word is an `OwnDataProbe` code.
    pub(crate) fn emit_ordinary_header_own_data_probe_i32(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        value: &ValueLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let result = schema.reserve_i32_local(f);
        result.set_constant(OwnDataProbe::Other.code(), f);
        let probe = OrdinaryGetProbe::new(schema, f);
        self.emit_ordinary_get_from_header(object, key, &probe, f)?;
        probe.emit_state_i32(OrdinaryGetState::Data, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        value.copy_from(probe.value(), f);
        result.set_constant(OwnDataProbe::Data.code(), f);
        f.instruction(&Instruction::Else);
        probe.emit_state_i32(OrdinaryGetState::Absent, f);
        f.instruction(&Instruction::If(BlockType::Empty));
        result.set_constant(OwnDataProbe::Absent.code(), f);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::End);
        probe.clear(schema, f);
        Ok(result)
    }

    /// True when the selected ordinary header has no own property `key`.
    /// A present property and an unsupported key report false.
    pub(crate) fn emit_ordinary_header_own_property_absent_i32(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let found = schema.reserve_i32_local(f);
        let enumerable = schema.reserve_i32_local(f);
        self.emit_ordinary_own_property_enumerability(object, key, found, enumerable, f)?;
        found.load(f);
        f.instruction(&Instruction::I32Eqz);
        found.store(f);
        schema.release_i32_local(enumerable, f);
        Ok(found)
    }

    /// True when an ordinary or function object has no own property `key`.
    /// A present property and every other object kind report false.
    pub(crate) fn emit_ordinary_own_property_absent_i32(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let absent = schema.reserve_i32_local(f);
        let probe = self.emit_ordinary_get_probe(target, key, f)?;
        probe.emit_state_i32(OrdinaryGetState::Absent, f);
        absent.store(f);
        probe.clear(schema, f);
        Ok(absent)
    }

    /// Only these actual object kinds have entirely ordinary own properties.
    /// Primitive boxes keep their virtual String indices in the exotic path.
    pub(super) fn emit_ordinary_get_probe(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<OrdinaryGetProbe, EmitError> {
        let schema = self.runtime_schema();
        let probe = OrdinaryGetProbe::new(schema, f);
        let header = schema
            .reserve_gc_local::<OrdinaryObject, Nullable>(f)
            .initialize_null(schema, f);
        target.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<OrdinaryObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        header.replace(
            target
                .cast_reference::<OrdinaryObject>(schema, f)
                .nullable(),
            f,
        );
        f.instruction(&Instruction::Else);
        target.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        let callable = schema
            .reserve_gc_local(f)
            .initialize(target.cast_reference::<FunctionObject>(schema, f), f);
        header.replace(
            schema
                .field(FunctionObjectSchema::OBJECT)
                .read(&callable, schema, f)
                .reference()
                .nullable(),
            f,
        );
        callable.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        header.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let object = schema
            .reserve_gc_local(f)
            .initialize(header.load(schema, f).require_non_null(f), f);
        self.emit_ordinary_get_from_header(&object, key, &probe, f)?;
        object.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        header.clear(f);
        Ok(probe)
    }

    /// One step of OrdinaryGet for the kinds whose [[GetOwnProperty]] for
    /// `key` is the ordinary lookup in their header, which `header` then
    /// holds for the caller's [[GetPrototypeOf]]: besides an ordinary object
    /// and a function, an Array for a key that is neither an array index
    /// nor "length", a Map, Set, WeakMap, WeakSet or Promise, a
    /// TextEncoder, TextDecoder, ReadableStream, its default reader or its
    /// byte controller, and a typed array for a key that cannot be a
    /// canonical numeric String. The non-Proxy [[GetOwnProperty]]
    /// (`emit_non_proxy_own_descriptor`) answers every object from its
    /// header first and has a further answer only for a module namespace, an
    /// Array, an arguments object, a String wrapper and a typed array, whose
    /// only further keys are the canonical numeric Strings; none of the
    /// kinds admitted here but the Array, whose only further keys are the
    /// array indices and "length", and the typed array is one of those, and
    /// none overrides [[Get]]. Every other value leaves the probe unsupported and
    /// `header` null, having observed nothing.
    pub(super) fn emit_object_get_probe(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        header: &GcLocal<OrdinaryObject, Nullable>,
        f: &mut Function,
    ) -> Result<OrdinaryGetProbe, EmitError> {
        let schema = self.runtime_schema();
        let probe = OrdinaryGetProbe::new(schema, f);
        header.set_null(schema, f);
        let selected = self.open_frame(ControlFrameKind::Block, f);
        let next = self.open_frame(ControlFrameKind::Block, f);
        header.replace(
            target
                .sealed_reference_or_branch::<OrdinaryObject>(
                    schema,
                    self.local_branch_label(next),
                    f,
                )
                .nullable(),
            f,
        );
        self.emit_branch_to_target(selected, f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        macro_rules! composite {
            ($layout:ty, $field:expr) => {{
                let next = self.open_frame(ControlFrameKind::Block, f);
                let composite = schema.reserve_gc_local(f).initialize(
                    target.sealed_reference_or_branch::<$layout>(
                        schema,
                        self.local_branch_label(next),
                        f,
                    ),
                    f,
                );
                header.replace(
                    schema
                        .field($field)
                        .read(&composite, schema, f)
                        .reference()
                        .nullable(),
                    f,
                );
                composite.clear(f);
                self.emit_branch_to_target(selected, f);
                self.pop_control(ControlFrameKind::Block);
                f.instruction(&Instruction::End);
            }};
        }
        composite!(FunctionObject, FunctionObjectSchema::OBJECT);
        // An Array, the commonest remaining kind, owns its indices and
        // "length" outside its header. The header answers every other key,
        // and for those two the probe stays unsupported.
        {
            let next = self.open_frame(ControlFrameKind::Block, f);
            let array = schema.reserve_gc_local(f).initialize(
                target.sealed_reference_or_branch::<ArrayObject>(
                    schema,
                    self.local_branch_label(next),
                    f,
                ),
                f,
            );
            self.emit_header_named_property_key_i32(key.value(), f);
            self.open_frame(ControlFrameKind::If, f);
            header.replace(
                schema
                    .field(ArrayObjectSchema::OBJECT)
                    .read(&array, schema, f)
                    .reference()
                    .nullable(),
                f,
            );
            self.pop_control(ControlFrameKind::If);
            f.instruction(&Instruction::End);
            array.clear(f);
            self.emit_branch_to_target(selected, f);
            self.pop_control(ControlFrameKind::Block);
            f.instruction(&Instruction::End);
        }
        composite!(TypedArrayObject, TypedArrayObjectSchema::OBJECT);
        composite!(SetObject, SetObjectSchema::OBJECT);
        composite!(MapObject, MapObjectSchema::OBJECT);
        composite!(PromiseObject, PromiseObjectSchema::OBJECT);
        composite!(
            ReadableByteStreamControllerObject,
            ReadableByteStreamControllerObjectSchema::OBJECT
        );
        composite!(TextEncoderObject, TextEncoderObjectSchema::OBJECT);
        composite!(TextDecoderObject, TextDecoderObjectSchema::OBJECT);
        composite!(ReadableStreamObject, ReadableStreamObjectSchema::OBJECT);
        composite!(
            ReadableStreamDefaultReaderObject,
            ReadableStreamDefaultReaderObjectSchema::OBJECT
        );
        composite!(WeakMapObject, WeakMapObjectSchema::OBJECT);
        composite!(WeakSetObject, WeakSetObjectSchema::OBJECT);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        // A typed array answers a canonical numeric String itself and never
        // from its header or its prototype.
        target.reference().load(f);
        f.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<TypedArrayObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, f);
        self.emit_key_never_canonical_numeric_i32(key, f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        header.set_null(schema, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        header.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let object = schema
            .reserve_gc_local(f)
            .initialize(header.load(schema, f).require_non_null(f), f);
        self.emit_ordinary_get_from_header(&object, key, &probe, f)?;
        object.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok(probe)
    }

    /// Leaves on the stack whether CanonicalNumericIndexString(key) is
    /// certainly undefined: a Symbol, or a nonempty String whose first code
    /// unit starts no Number spelling. ToString of a Number begins with a
    /// decimal digit, with '-', or is "Infinity" or "NaN", and "-0" begins
    /// with '-'. Every other key reports false, undecided.
    fn emit_key_never_canonical_numeric_i32(&mut self, key: &PropertyKeyLocals, f: &mut Function) {
        let schema = self.runtime_schema();
        let never = schema.reserve_i32_local(f);
        never.set_constant(0, f);
        key.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Symbol as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        never.set_constant(1, f);
        f.instruction(&Instruction::Else);
        key.value().tag().load(f);
        f.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        f.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, f);
        let name = schema
            .reserve_gc_local(f)
            .initialize(key.value().cast_reference::<StringValue>(schema, f), f);
        let units = schema.reserve_gc_local(f).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(&name, schema, f)
                .reference(),
            f,
        );
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, f);
        self.open_frame(ControlFrameKind::If, f);
        let first = schema.reserve_i32_local(f);
        let at = schema.reserve_i32_local(f);
        at.set_constant(0, f);
        schema
            .array_type::<CodeUnitArray>()
            .read(&units, at, schema, f)
            .store(first, f);
        first.load(f);
        f.instruction(&Instruction::I32Const(i32::from(b'0')));
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32GeU);
        for start in [b'-', b'I', b'N'] {
            first.load(f);
            f.instruction(&Instruction::I32Const(i32::from(start)));
            f.instruction(&Instruction::I32Ne);
            f.instruction(&Instruction::I32And);
        }
        never.store(f);
        schema.release_i32_local(at, f);
        schema.release_i32_local(first, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        units.clear(f);
        name.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        never.load(f);
        schema.release_i32_local(never, f);
    }

    fn emit_ordinary_get_from_header(
        &mut self,
        object: &GcLocal<OrdinaryObject>,
        key: &PropertyKeyLocals,
        probe: &OrdinaryGetProbe,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        probe.set_state(OrdinaryGetState::Absent, f);
        let base = schema.reserve_gc_local(f).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(object, schema, f)
                .reference(),
            f,
        );
        base.is_subtype::<CompactDataStorage>(schema, f);
        self.open_frame(ControlFrameKind::If, f);
        let compact = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<CompactDataStorage>(schema, f),
            f,
        );
        let slot = schema.reserve_i32_local(f);
        let found = schema.reserve_i32_local(f);
        self.emit_compact_slot_probe(&compact, key, slot, found, f)?;
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        self.emit_compact_value_at(&compact, slot, &probe.value, f);
        probe.set_state(OrdinaryGetState::Data, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(found, f);
        schema.release_i32_local(slot, f);
        compact.clear(f);
        f.instruction(&Instruction::Else);
        base.is_subtype::<IntrinsicPropertyStorage>(schema, f);
        self.open_frame(ControlFrameKind::If, f);
        let intrinsic = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<IntrinsicPropertyStorage>(schema, f),
            f,
        );
        // The layout's index usually proves absence without the complete
        // search; the probe state is already Absent.
        let lacks = self.emit_intrinsic_layout_lacks_key_i32(&intrinsic, key, f);
        lacks.load(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let index = schema.reserve_i32_local(f);
        schema
            .call_helper(
                crate::runtime_helpers::IntrinsicPropertyFindArguments::new(&intrinsic, key),
                self.runtime_helper_base()?,
                f,
            )
            .store(index, f);
        index.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        let row = self.emit_intrinsic_property_row(&intrinsic, index, f);
        row.flags().load(f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let getter = self.emit_intrinsic_property_value(&intrinsic, index, false, f)?;
        self.emit_property_value_into(&getter, &probe.value, f);
        getter.clear(f);
        probe.set_state(OrdinaryGetState::Getter, f);
        f.instruction(&Instruction::Else);
        self.emit_intrinsic_data_value_into(&intrinsic, index, &row, &probe.value, f)?;
        probe.set_state(OrdinaryGetState::Data, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        row.clear(schema, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(index, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(lacks, f);
        intrinsic.clear(f);
        f.instruction(&Instruction::Else);
        base.is_subtype::<GlobalBuiltinBacking>(schema, f);
        self.open_frame(ControlFrameKind::If, f);
        let backing = schema.reserve_gc_local(f).initialize(
            base.load(schema, f)
                .cast_subtype::<GlobalBuiltinBacking>(schema, f),
            f,
        );
        self.emit_global_builtin_get(&backing, key, probe, f)?;
        backing.clear(f);
        f.instruction(&Instruction::Else);
        let entry = self.emit_ordinary_property_entry(object, key, f)?;
        let descriptor = self.emit_ordinary_entry_descriptor_reference(&entry, f);
        self.emit_ordinary_get_from_descriptor(&descriptor, probe, f);
        descriptor.clear(f);
        entry.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        base.clear(f);
        Ok(())
    }

    fn emit_ordinary_get_from_descriptor(
        &mut self,
        descriptor: &GcLocal<PropertyDescriptor, Nullable>,
        probe: &OrdinaryGetProbe,
        f: &mut Function,
    ) {
        let schema = self.runtime_schema();
        descriptor.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let flags = schema.reserve_i32_local(f);
        schema
            .field(PropertyDescriptorSchema::FLAGS)
            .read(descriptor, schema, f)
            .store(flags, f);
        flags.load(f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let getter = schema.reserve_gc_local(f).initialize(
            schema
                .field(PropertyDescriptorSchema::GETTER)
                .read(descriptor, schema, f)
                .reference(),
            f,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&getter, &probe.value, schema, f);
        getter.clear(f);
        probe.set_state(OrdinaryGetState::Getter, f);
        f.instruction(&Instruction::Else);
        self.emit_descriptor_data_value_into(descriptor, &probe.value, f);
        probe.set_state(OrdinaryGetState::Data, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(flags, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
    }

    /// One callback-free global property lookup. An appended key has no
    /// original rank. Live original keys may still have only their static row.
    pub(crate) fn emit_global_builtin_own_entry_or_row(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<(GcLocal<PropertyEntry, Nullable>, I32Local), EmitError> {
        let schema = self.runtime_schema();
        let entry = self.emit_global_builtin_append_entry(backing, key, f)?;
        let row = schema.reserve_i32_local(f);
        row.set_constant(-1, f);
        entry.load(schema, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        let original = self.emit_global_builtin_find(backing, key, f)?;
        original.load(f);
        row.store(f);
        schema.release_i32_local(original, f);
        row.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        let realized = self.emit_global_builtin_row_entry(backing, row, f);
        entry.replace(realized.load(schema, f), f);
        realized.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        Ok((entry, row))
    }

    fn emit_global_builtin_get(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        key: &PropertyKeyLocals,
        probe: &OrdinaryGetProbe,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let (entry, row) = self.emit_global_builtin_own_entry_or_row(backing, key, f)?;
        entry.load(schema, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        row.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        let value = self.emit_global_builtin_row_value(backing, row, f)?;
        self.emit_property_value_into(&value, &probe.value, f);
        value.clear(f);
        probe.set_state(OrdinaryGetState::Data, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        let descriptor = self.emit_ordinary_entry_descriptor_reference(&entry, f);
        self.emit_ordinary_get_from_descriptor(&descriptor, probe, f);
        descriptor.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(row, f);
        entry.clear(f);
        Ok(())
    }

    pub(super) fn emit_global_builtin_own_data_value(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        key: &PropertyKeyLocals,
        output: &ValueLocals,
        found: I32Local,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let probe = OrdinaryGetProbe::new(schema, f);
        self.emit_global_builtin_get(backing, key, &probe, f)?;
        probe.emit_state_i32(OrdinaryGetState::Data, f);
        found.store(f);
        found.load(f);
        self.open_frame(ControlFrameKind::If, f);
        output.copy_from(probe.value(), f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        probe.clear(schema, f);
        Ok(())
    }

    /// Presence and attributes of the selected global key. Complete descriptor
    /// flags are nonnegative; -1 represents absence.
    pub(super) fn emit_global_builtin_own_flags(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<I32Local, EmitError> {
        let schema = self.runtime_schema();
        let (entry, row) = self.emit_global_builtin_own_entry_or_row(backing, key, f)?;
        let flags = schema.reserve_i32_local(f);
        flags.set_constant(-1, f);
        entry.load(schema, f).is_null(f);
        self.open_frame(ControlFrameKind::If, f);
        row.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        self.open_frame(ControlFrameKind::If, f);
        let original = self.emit_global_builtin_row_flags(backing, row, f);
        original.load(f);
        flags.store(f);
        schema.release_i32_local(original, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        f.instruction(&Instruction::Else);
        let descriptor = self.emit_ordinary_entry_descriptor_reference(&entry, f);
        schema
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, schema, f)
            .store(flags, f);
        descriptor.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(row, f);
        entry.clear(f);
        Ok(flags)
    }

    pub(super) fn emit_global_builtin_own_descriptor(
        &mut self,
        backing: &GcLocal<GlobalBuiltinBacking>,
        key: &PropertyKeyLocals,
        f: &mut Function,
    ) -> Result<GcLocal<PropertyDescriptor, Nullable>, EmitError> {
        let schema = self.runtime_schema();
        let (entry, row) = self.emit_global_builtin_own_entry_or_row(backing, key, f)?;
        let descriptor = self.emit_ordinary_entry_descriptor_reference(&entry, f);
        descriptor.load(schema, f).is_null(f);
        row.load(f);
        f.instruction(&Instruction::I32Const(0));
        f.instruction(&Instruction::I32GeS);
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let original = self.emit_global_builtin_row_descriptor(backing, row, f)?;
        descriptor.replace(original.load(schema, f).nullable(), f);
        original.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(row, f);
        entry.clear(f);
        Ok(descriptor)
    }
}
