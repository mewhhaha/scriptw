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
        let absent = schema.reserve_i32_local(f);
        let probe = OrdinaryGetProbe::new(schema, f);
        self.emit_ordinary_get_from_header(object, key, &probe, f)?;
        probe.emit_state_i32(OrdinaryGetState::Absent, f);
        absent.store(f);
        probe.clear(schema, f);
        Ok(absent)
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
        let entry = self.emit_ordinary_property_entry(object, key, f)?;
        let descriptor = self.emit_ordinary_entry_descriptor_reference(&entry, f);
        descriptor.load(schema, f).is_null(f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, f);
        let flags = schema.reserve_i32_local(f);
        schema
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, schema, f)
            .store(flags, f);
        flags.load(f);
        f.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        f.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, f);
        let getter = schema.reserve_gc_local(f).initialize(
            schema
                .field(PropertyDescriptorSchema::GETTER)
                .read(&descriptor, schema, f)
                .reference(),
            f,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&getter, &probe.value, schema, f);
        getter.clear(f);
        probe.set_state(OrdinaryGetState::Getter, f);
        f.instruction(&Instruction::Else);
        self.emit_descriptor_data_value_into(&descriptor, &probe.value, f);
        probe.set_state(OrdinaryGetState::Data, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        schema.release_i32_local(flags, f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        descriptor.clear(f);
        entry.clear(f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        base.clear(f);
        Ok(())
    }
}
