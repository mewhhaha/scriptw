//! An activation may write only its original, still-mutable legacy property.

use super::*;
use crate::gc_types::{
    GcOperand, LegacyReflectionPropertyStateSchema, NonNullable, OrdinaryObject,
    PropertyDescriptor, PropertyDescriptorSchema, PropertyEntry, PropertyEntrySchema,
    PropertyValue, RuntimeSchema, StoredValue, ValueLocals,
};
use crate::heap::DescriptorMask;
use crate::operations::PropertyKeyLocals;
use crate::runtime_helpers::{OrdinaryPropertyFindArguments, RuntimeHelperFunctionBase};

pub(super) struct ReflectionProperty {
    object: GcLocal<OrdinaryObject>,
    key: PropertyKeyLocals,
    state: GcLocal<LegacyReflectionPropertyState, Nullable>,
    previous_value: GcLocal<StoredValue, Nullable>,
    helpers: RuntimeHelperFunctionBase,
}

impl ReflectionProperty {
    pub(super) fn capture(
        builder: &mut FunctionBuilder<'_>,
        object: &GcLocal<OrdinaryObject>,
        name: &str,
        state: GcLocal<LegacyReflectionPropertyState, Nullable>,
        function: &mut Function,
    ) -> Result<Self, EmitError> {
        let schema = builder.runtime_schema();
        let key = builder.emit_function_string_key(name, function)?;
        let helpers = builder.runtime_helper_base()?;
        let object = schema
            .reserve_gc_local(function)
            .initialize(object.load(schema, function), function);
        let previous_value = schema
            .reserve_gc_local::<StoredValue, Nullable>(function)
            .initialize_null(schema, function);
        state.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let record = schema.reserve_gc_local(function).initialize(
            state.load(schema, function).require_non_null(function),
            function,
        );
        previous_value.replace(
            schema
                .field(LegacyReflectionPropertyStateSchema::EXPECTED_VALUE)
                .read(&record, schema, function)
                .reference(),
            function,
        );
        record.clear(function);
        function.instruction(&Instruction::End);
        Ok(Self {
            object,
            key,
            state,
            previous_value,
            helpers,
        })
    }

    // The helper base is admitted once at capture. Neither this fresh own
    // lookup nor the identity/flag checks can enter JavaScript or invoke a Get.
    fn current_descriptor(
        &self,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcLocal<PropertyDescriptor, Nullable> {
        let entry = schema
            .reserve_gc_local::<PropertyEntry, Nullable>(function)
            .initialize(
                schema.helper_reference_on_stack(schema.call_helper(
                    OrdinaryPropertyFindArguments::new(&self.object, &self.key),
                    self.helpers,
                    function,
                )),
                function,
            );
        let descriptor = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize_null(schema, function);
        entry.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let present = schema.reserve_gc_local(function).initialize(
            entry.load(schema, function).require_non_null(function),
            function,
        );
        descriptor.replace(
            schema
                .field(PropertyEntrySchema::DESCRIPTOR)
                .read(&present, schema, function)
                .reference()
                .nullable(),
            function,
        );
        present.clear(function);
        function.instruction(&Instruction::End);
        entry.clear(function);
        descriptor
    }

    fn write(
        &self,
        value: &GcLocal<StoredValue, Nullable>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.state.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let state = schema.reserve_gc_local(function).initialize(
            self.state.load(schema, function).require_non_null(function),
            function,
        );
        let owned = schema
            .reserve_gc_local::<PropertyDescriptor, Nullable>(function)
            .initialize(
                schema
                    .field(LegacyReflectionPropertyStateSchema::DESCRIPTOR)
                    .read(&state, schema, function)
                    .reference(),
                function,
            );
        owned.load(schema, function).is_null(function);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::If(BlockType::Empty));
        let descriptor = schema.reserve_gc_local(function).initialize(
            owned.load(schema, function).require_non_null(function),
            function,
        );
        let current = self.current_descriptor(schema, function);
        let expected = schema
            .reserve_gc_local::<StoredValue, Nullable>(function)
            .initialize(
                schema
                    .field(LegacyReflectionPropertyStateSchema::EXPECTED_VALUE)
                    .read(&state, schema, function)
                    .reference(),
                function,
            );
        // Compare the raw owner token: resolving a deferred value must never
        // revive legacy reflection after a user replacement.
        let actual = schema
            .reserve_gc_local::<PropertyValue, NonNullable>(function)
            .initialize(
                schema
                    .field(PropertyDescriptorSchema::VALUE)
                    .read(&descriptor, schema, function)
                    .reference(),
                function,
            );
        current.load(schema, function);
        owned.load(schema, function);
        function.instruction(&Instruction::RefEq);
        let flags = schema.reserve_i32_local(function);
        schema
            .field(PropertyDescriptorSchema::FLAGS)
            .read(&descriptor, schema, function)
            .store(flags, function);
        flags.load(function);
        function.instruction(&Instruction::I32Const(DescriptorMask::ACCESSOR.as_i32()));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        flags.load(function);
        function.instruction(&Instruction::I32Const(
            DescriptorMask::CONFIGURABLE
                .union(DescriptorMask::WRITABLE)
                .as_i32(),
        ));
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32Eqz);
        function.instruction(&Instruction::I32And);
        actual.load(schema, function);
        expected.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32And);
        schema.release_i32_local(flags, function);
        actual.clear(function);
        function.instruction(&Instruction::If(BlockType::Empty));
        // A live original owner captured a non-null prior token; publication
        // also supplies a non-null token. Retired owners cannot reach this arm.
        let stored = schema.reserve_gc_local(function).initialize(
            value.load(schema, function).require_non_null(function),
            function,
        );
        schema.field(PropertyDescriptorSchema::VALUE).write(
            &descriptor,
            GcOperand::subtype_reference(&stored, schema),
            schema,
            function,
        );
        schema
            .field(LegacyReflectionPropertyStateSchema::EXPECTED_VALUE)
            .write(
                &state,
                GcOperand::nullable_reference(&stored, schema),
                schema,
                function,
            );
        stored.clear(function);
        function.instruction(&Instruction::Else);
        // A user definition, deletion or freeze owns the observed value now.
        // Do not revive it at a later entry, and release the old active roots.
        schema
            .field(LegacyReflectionPropertyStateSchema::DESCRIPTOR)
            .write(&state, GcOperand::null(schema), schema, function);
        schema
            .field(LegacyReflectionPropertyStateSchema::EXPECTED_VALUE)
            .write(&state, GcOperand::null(schema), schema, function);
        function.instruction(&Instruction::End);
        expected.clear(function);
        current.clear(function);
        descriptor.clear(function);
        function.instruction(&Instruction::End);
        owned.clear(function);
        state.clear(function);
        function.instruction(&Instruction::End);
    }

    pub(super) fn publish(
        &self,
        value: &ValueLocals,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        let stored = schema
            .reserve_gc_local::<StoredValue, Nullable>(function)
            .initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(value, function)
                    .nullable(),
                function,
            );
        self.write(&stored, schema, function);
        stored.clear(function);
    }

    pub(super) fn restore(&self, schema: &RuntimeSchema, function: &mut Function) {
        self.write(&self.previous_value, schema, function);
    }

    pub(super) fn clear(self, function: &mut Function) {
        self.previous_value.clear(function);
        self.state.clear(function);
        self.key.clear(function);
        self.object.clear(function);
    }
}
