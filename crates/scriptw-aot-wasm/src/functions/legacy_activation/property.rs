//! An activation may write only its original, still-mutable legacy property.

use super::*;
use crate::arguments_protocol::DeferredLegacyArgumentsPlan;
use crate::gc_types::{
    DeferredLegacyArguments, Environment, FunctionObject, GcOperand,
    LegacyReflectionPropertyStateSchema, NonNullable, PropertyDescriptor, PropertyDescriptorSchema,
    PropertyEntrySchema, PropertyValue, RuntimeSchema, StoredValue, ValueArray, ValueLocals,
};
use crate::heap::DescriptorMask;

/// What an activation writes: the value captured at entry, restored at exit,
/// a value boxed only on the arm that stores it, or a deferred arguments
/// recipe materialized only on reflection observation, so an unobserved
/// property costs no allocation.
#[derive(Clone, Copy)]
enum Written<'a> {
    Stored(&'a GcLocal<PropertyValue, Nullable>),
    Fresh(&'a ValueLocals),
    DeferredLegacy {
        callable: &'a GcLocal<FunctionObject>,
        arguments: &'a GcLocal<ValueArray>,
        environment: &'a GcLocal<Environment, Nullable>,
        plan: DeferredLegacyArgumentsPlan,
    },
}

/// The state names the property's entry record, so no write looks the key
/// up: see `LegacyReflectionPropertyState` and the active legacy function
/// reflection contract.
pub(super) struct ReflectionProperty {
    state: GcLocal<LegacyReflectionPropertyState, Nullable>,
    previous_value: GcLocal<PropertyValue, Nullable>,
}

impl ReflectionProperty {
    pub(super) fn capture(
        builder: &FunctionBuilder<'_>,
        state: GcLocal<LegacyReflectionPropertyState, Nullable>,
        function: &mut Function,
    ) -> Self {
        let schema = builder.runtime_schema();
        let previous_value = schema
            .reserve_gc_local::<PropertyValue, Nullable>(function)
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
        Self {
            state,
            previous_value,
        }
    }

    // The descriptor the property's entry record holds now: what a fresh own
    // lookup of the key would select while the property lives, and a
    // descriptor no property owns once it was deleted. Only reads; it can
    // neither enter JavaScript nor invoke a Get.
    fn current_descriptor(
        state: &GcLocal<LegacyReflectionPropertyState>,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) -> GcLocal<PropertyDescriptor> {
        // Publication fills ENTRY, DESCRIPTOR and EXPECTED_VALUE together and
        // retirement clears all three, so a live owner has its entry.
        let entry = schema.reserve_gc_local(function).initialize(
            schema
                .field(LegacyReflectionPropertyStateSchema::ENTRY)
                .read(state, schema, function)
                .reference()
                .require_non_null(function),
            function,
        );
        let descriptor = schema.reserve_gc_local(function).initialize(
            schema
                .field(PropertyEntrySchema::DESCRIPTOR)
                .read(&entry, schema, function)
                .reference(),
            function,
        );
        entry.clear(function);
        descriptor
    }

    fn write(&self, value: Written<'_>, schema: &RuntimeSchema, function: &mut Function) {
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
        let current = Self::current_descriptor(&state, schema, function);
        let expected = schema
            .reserve_gc_local::<PropertyValue, Nullable>(function)
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
        let stored: GcLocal<PropertyValue> = match value {
            Written::Stored(value) => schema.reserve_gc_local(function).initialize(
                value.load(schema, function).require_non_null(function),
                function,
            ),
            Written::Fresh(value) => schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<StoredValue>()
                    .from_value(value, function)
                    .upcast::<PropertyValue>(),
                function,
            ),
            Written::DeferredLegacy {
                callable,
                arguments,
                environment,
                plan,
            } => schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<DeferredLegacyArguments>()
                    .construct(
                        (
                            GcOperand::nullable_reference(callable, schema),
                            GcOperand::nullable_reference(arguments, schema),
                            GcOperand::reference(environment, schema),
                            GcOperand::i64(plan.packed_slots()),
                            GcOperand::i32(plan.param_count()),
                            GcOperand::null(schema),
                        ),
                        function,
                    )
                    .upcast::<PropertyValue>(),
                function,
            ),
        };
        schema.field(PropertyDescriptorSchema::VALUE).write(
            &descriptor,
            GcOperand::reference(&stored, schema),
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
            .field(LegacyReflectionPropertyStateSchema::ENTRY)
            .write(&state, GcOperand::null(schema), schema, function);
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
        self.write(Written::Fresh(value), schema, function);
    }

    pub(super) fn publish_deferred_arguments(
        &self,
        callable: &GcLocal<FunctionObject>,
        arguments: &GcLocal<ValueArray>,
        environment: &GcLocal<Environment, Nullable>,
        plan: DeferredLegacyArgumentsPlan,
        schema: &RuntimeSchema,
        function: &mut Function,
    ) {
        self.write(
            Written::DeferredLegacy {
                callable,
                arguments,
                environment,
                plan,
            },
            schema,
            function,
        );
    }

    pub(super) fn restore(&self, schema: &RuntimeSchema, function: &mut Function) {
        self.write(Written::Stored(&self.previous_value), schema, function);
    }

    pub(super) fn clear(self, function: &mut Function) {
        self.previous_value.clear(function);
        self.state.clear(function);
    }
}
