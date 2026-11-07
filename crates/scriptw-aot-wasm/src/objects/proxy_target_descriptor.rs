//! Shared [[GetOwnProperty]] observation. Snapshot consumers copy ordinary/Array
//! descriptors; ForIn reads its two scalar predicates before further work.

use super::*;
use crate::gc_types::{I32Local, RuntimeSchema};

#[must_use = "a completed descriptor root must be consumed and cleared"]
pub(crate) struct CompletedProxyTargetDescriptor {
    descriptor: GcLocal<PropertyDescriptor, Nullable>,
}

impl CompletedProxyTargetDescriptor {
    pub(crate) fn emit_found_i32(&self, schema: &RuntimeSchema, function: &mut Function) {
        self.descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        function.instruction(&Instruction::I32Eqz);
    }

    fn emit_flag_i32(&self, mask: DescriptorMask, schema: &RuntimeSchema, function: &mut Function) {
        self.emit_found_i32(schema, function);
        function.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        let flags = schema.reserve_i32_local(function);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&self.descriptor, schema, function)
            .store(flags, function);
        flags.load(function);
        function.instruction(&Instruction::I32Const(mask.as_i32()));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        schema.release_i32_local(flags, function);
        function.instruction(&Instruction::Else);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::End);
    }

    pub(crate) fn emit_accessor_i32(&self, schema: &RuntimeSchema, function: &mut Function) {
        self.emit_flag_i32(DescriptorMask::ACCESSOR, schema, function);
    }
    pub(crate) fn emit_writable_i32(&self, schema: &RuntimeSchema, function: &mut Function) {
        self.emit_flag_i32(DescriptorMask::WRITABLE, schema, function);
    }
    pub(crate) fn emit_enumerable_i32(&self, schema: &RuntimeSchema, function: &mut Function) {
        self.emit_flag_i32(DescriptorMask::ENUMERABLE, schema, function);
    }
    pub(crate) fn emit_configurable_i32(&self, schema: &RuntimeSchema, function: &mut Function) {
        self.emit_flag_i32(DescriptorMask::CONFIGURABLE, schema, function);
    }

    fn read_field(
        &self,
        field: crate::gc_types::GcField<
            PropertyDescriptor,
            crate::gc_types::GcRef<StoredValue>,
            crate::gc_types::Mutable,
            crate::gc_types::NonNullable,
        >,
        result: &ValueLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        result.set_undefined(function);
        self.emit_found_i32(schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let stored_slot = schema.reserve_gc_local(function);
        let stored = stored_slot.initialize(
            schema
                .struct_type::<PropertyDescriptor>()
                .field(field)
                .read(&self.descriptor, schema, function)
                .reference(),
            function,
        );
        schema
            .struct_type::<StoredValue>()
            .read_into(&stored, result, schema, function);
        stored.clear(function);
        function.instruction(&Instruction::End);
    }
    pub(crate) fn read_value(
        &self,
        out: &ValueLocals,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let schema = builder.runtime_schema();
        out.set_undefined(function);
        self.emit_found_i32(schema, function);
        function.instruction(&Instruction::If(BlockType::Empty));
        let value = schema.reserve_gc_local(function).initialize(
            schema
                .field(PropertyDescriptorSchema::VALUE)
                .read(&self.descriptor, schema, function)
                .reference(),
            function,
        );
        builder.emit_property_value_into(&value, out, function);
        value.clear(function);
        function.instruction(&Instruction::End);
    }
    pub(crate) fn read_getter(
        &self,
        out: &ValueLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.read_field(PropertyDescriptorSchema::GETTER, out, schema, function);
    }
    pub(crate) fn read_setter(
        &self,
        out: &ValueLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.read_field(PropertyDescriptorSchema::SETTER, out, schema, function);
    }
    /// IsCompatiblePropertyDescriptor uses the same stored-descriptor kernel
    /// as DefineOwnProperty, after this proof has completed target acquisition.
    pub(crate) fn emit_compatibility(
        &self,
        incoming: &WasmDescriptor<'_>,
        extensible: I32Local,
        valid: I32Local,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = builder.runtime_schema();
        self.emit_found_i32(schema, function);
        builder.open_frame(ControlFrameKind::If, function);
        let descriptor = schema.reserve_gc_local(function).initialize(
            self.descriptor
                .load(schema, function)
                .require_non_null(function),
            function,
        );
        builder.emit_validate_stored_descriptor(&descriptor, incoming, valid, function)?;
        descriptor.clear(function);
        function.instruction(&Instruction::Else);
        extensible.load(function);
        valid.store(function);
        builder.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }
    pub(super) fn into_descriptor(self) -> GcLocal<PropertyDescriptor, Nullable> {
        self.descriptor
    }
    pub(crate) fn clear(self, function: &mut Function) {
        self.descriptor.clear(function);
    }
}

// Storage references must not escape as completed snapshots. The scalar form
// consumes the two predicates immediately, before any later callback can run.
#[derive(Clone, Copy)]
enum OwnDescriptorObservation<'a> {
    Snapshot(&'a GcLocal<PropertyDescriptor, Nullable>),
    Enumerability {
        found: I32Local,
        enumerable: I32Local,
    },
}

impl OwnDescriptorObservation<'_> {
    fn storage(
        self,
        source: GcLocal<PropertyDescriptor, Nullable>,
        builder: &mut FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        match self {
            Self::Snapshot(output) => {
                builder.emit_completed_descriptor_snapshot(source, output, function);
            }
            Self::Enumerability { .. } => self.completed(source, builder, function),
        }
    }

    fn completed(
        self,
        source: GcLocal<PropertyDescriptor, Nullable>,
        builder: &FunctionBuilder<'_>,
        function: &mut Function,
    ) {
        let schema = builder.runtime_schema();
        match self {
            Self::Snapshot(output) => {
                output.replace(source.load(schema, function), function);
                source.clear(function);
            }
            Self::Enumerability { found, enumerable } => {
                let source = CompletedProxyTargetDescriptor { descriptor: source };
                source.emit_found_i32(schema, function);
                found.store(function);
                source.emit_enumerable_i32(schema, function);
                enumerable.store(function);
                source.clear(function);
            }
        }
    }
}

impl FunctionBuilder<'_> {
    /// Consume a storage lookup, preserving a fresh mutable descriptor record.
    /// A later consumer may invoke user code that redefines the source property;
    /// only immutable value edges may be shared with that property's live record.
    pub(super) fn emit_completed_descriptor_snapshot(
        &mut self,
        source: GcLocal<PropertyDescriptor, Nullable>,
        descriptor: &GcLocal<PropertyDescriptor, Nullable>,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let source = CompletedProxyTargetDescriptor { descriptor: source };
        source.emit_found_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        let writable = schema.reserve_i32_local(function);
        let enumerable = schema.reserve_i32_local(function);
        let configurable = schema.reserve_i32_local(function);
        source.emit_writable_i32(schema, function);
        writable.store(function);
        source.emit_enumerable_i32(schema, function);
        enumerable.store(function);
        source.emit_configurable_i32(schema, function);
        configurable.store(function);
        let stored_value = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PropertyDescriptor>()
                .field(PropertyDescriptorSchema::VALUE)
                .read(&source.descriptor, schema, function)
                .reference(),
            function,
        );
        let stored_getter = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PropertyDescriptor>()
                .field(PropertyDescriptorSchema::GETTER)
                .read(&source.descriptor, schema, function)
                .reference(),
            function,
        );
        let stored_setter = schema.reserve_gc_local(function).initialize(
            schema
                .struct_type::<PropertyDescriptor>()
                .field(PropertyDescriptorSchema::SETTER)
                .read(&source.descriptor, schema, function)
                .reference(),
            function,
        );
        source.emit_accessor_i32(schema, function);
        self.open_frame(ControlFrameKind::If, function);
        descriptor.replace(
            schema
                .struct_type::<PropertyDescriptor>()
                .construct(
                    (
                        GcOperand::accessor_descriptor_flags(enumerable, configurable),
                        GcOperand::reference(&stored_value, schema),
                        GcOperand::reference(&stored_getter, schema),
                        GcOperand::reference(&stored_setter, schema),
                    ),
                    function,
                )
                .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        descriptor.replace(
            schema
                .struct_type::<PropertyDescriptor>()
                .construct(
                    (
                        GcOperand::data_descriptor_flags(writable, enumerable, configurable),
                        GcOperand::reference(&stored_value, schema),
                        GcOperand::reference(&stored_getter, schema),
                        GcOperand::reference(&stored_setter, schema),
                    ),
                    function,
                )
                .nullable(),
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        stored_setter.clear(function);
        stored_getter.clear(function);
        stored_value.clear(function);
        schema.release_i32_local(configurable, function);
        schema.release_i32_local(enumerable, function);
        schema.release_i32_local(writable, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        source.clear(function);
    }

    /// Actual OrdinaryObject and Array references snapshot their storage directly.
    /// Other exotics use the same completed private descriptor runtime boundary.
    pub(crate) fn emit_proxy_target_own_descriptor(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<CompletedProxyTargetDescriptor, EmitError> {
        let schema = self.runtime_schema();
        let descriptor = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize_null(schema, function);
        self.emit_own_descriptor_observation(
            target,
            key,
            OwnDescriptorObservation::Snapshot(&descriptor),
            function,
        )?;
        Ok(CompletedProxyTargetDescriptor { descriptor })
    }

    /// Read live presence/enumerability through the same classifier and lookup.
    /// Ordinary storage is consumed immediately; exotics retain their whole Throw.
    pub(crate) fn emit_own_property_enumerability(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        found: I32Local,
        enumerable: I32Local,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.emit_own_descriptor_observation(
            target,
            key,
            OwnDescriptorObservation::Enumerability { found, enumerable },
            function,
        )
    }

    fn emit_own_descriptor_observation(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        observation: OwnDescriptorObservation<'_>,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<OrdinaryObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let object = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<OrdinaryObject>(schema, function),
            function,
        );
        match observation {
            OwnDescriptorObservation::Snapshot(_) => {
                let source = self.emit_ordinary_own_descriptor_reference(&object, key, function)?;
                observation.storage(source, self, function);
            }
            OwnDescriptorObservation::Enumerability { found, enumerable } => {
                self.emit_ordinary_own_property_enumerability(
                    &object, key, found, enumerable, function,
                )?;
            }
        }
        object.clear(function);
        function.instruction(&Instruction::Else);
        // Concrete Arrays need their exotic indexed/length lookup, but no
        // public descriptor object or temporary builtin callable. A Proxy that
        // wraps an Array fails this reference test and retains its trap path.
        target.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<crate::gc_types::ArrayObject>(
                    crate::gc_types::GcNullability::NonNullable,
                )
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let array = schema.reserve_gc_local(function).initialize(
            target.cast_reference::<crate::gc_types::ArrayObject>(schema, function),
            function,
        );
        let source = self.emit_array_own_descriptor_reference(&array, key, function)?;
        observation.storage(source, self, function);
        array.clear(function);
        function.instruction(&Instruction::Else);
        let result = schema.reserve_completion(function);
        let returned = self.emit_object_get_own_property(target, key, &result, function)?;
        result.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Throw.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.completion().copy_from(&result, function);
        self.emit_propagate_current_throw(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        observation.completed(returned, self, function);
        result.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    pub(super) fn emit_direct_own_descriptor_for_proxy_get(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<CompletedProxyTargetDescriptor, EmitError> {
        self.emit_proxy_target_own_descriptor(target, key, function)
    }
    pub(super) fn emit_direct_own_descriptor_for_proxy_set(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<CompletedProxyTargetDescriptor, EmitError> {
        self.emit_proxy_target_own_descriptor(target, key, function)
    }
    pub(super) fn emit_proxy_target_descriptor_fact(
        &mut self,
        target: &ValueLocals,
        key: &PropertyKeyLocals,
        function: &mut Function,
    ) -> Result<CompletedProxyTargetDescriptor, EmitError> {
        self.emit_proxy_target_own_descriptor(target, key, function)
    }
}
