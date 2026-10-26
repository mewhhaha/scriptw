//! Sole native GetOwnProperty owner, including recursive Proxy targets.
use super::*;
use crate::gc_types::{
    CompletionLocals, GcLocal, I32Local, Nullable, PropertyDescriptor, PropertyDescriptorSchema,
    ProxyObject, StoredValue, ValueLocals,
};
use crate::objects::ProxyRevocationRoute;
use crate::objects::{
    DescriptorFlag, DescriptorObjectFields, DescriptorObjectPrototype, PropertyKeyLocals,
};

mod proxy;

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn compile_object_get_own_property_descriptor_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let input = schema.reserve_value_local(function);
        let key_value = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let output = schema.reserve_completion(function);
        self.emit_builtin_arg_to_value(0, &input, function);
        // ToObject precedes ToPropertyKey, including the null/undefined throw.
        self.emit_value_to_object_locals(&input, &pending, function)?;
        output.copy_from(&pending, function);
        pending.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_builtin_arg_to_value(1, &key_value, function);
        let key = self.emit_value_to_property_key_locals(&key_value, function)?;
        pending.value().reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<ProxyObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        self.open_frame(ControlFrameKind::If, function);
        let proxy = schema.reserve_gc_local(function).initialize(
            pending
                .value()
                .cast_reference::<ProxyObject>(schema, function),
            function,
        );
        self.emit_load_live_proxy_slots(
            &proxy,
            ProxyRevocationRoute::CurrentFunctionRealm,
            &output,
            function,
            |builder, slots, function| {
                builder.emit_proxy_get_own_property_descriptor(slots, &key, &output, function)
            },
        )?;
        proxy.clear(function);
        function.instruction(&Instruction::Else);
        let descriptor = self.emit_non_proxy_own_descriptor(pending.value(), &key, function)?;
        descriptor.load(schema, function);
        function.instruction(&Instruction::RefIsNull);
        self.open_frame(ControlFrameKind::If, function);
        output.initialize(function);
        function.instruction(&Instruction::Else);
        let record = schema.reserve_gc_local(function).initialize(
            descriptor.load(schema, function).require_non_null(function),
            function,
        );
        self.emit_native_stored_descriptor_object(&record, &output, function)?;
        record.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        descriptor.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        key.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        output.clear(function);
        pending.clear(function);
        key_value.clear(function);
        input.clear(function);
        Ok(())
    }

    fn emit_native_descriptor_object(
        &mut self,
        fields: &DescriptorObjectFields<'_>,
        output: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let realm = self.emit_execution_realm(function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            crate::functions::NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            function,
        );
        let object = schema.reserve_gc_local(function).initialize(
            self.emit_from_property_descriptor(
                DescriptorObjectPrototype::ObjectPrototypeLocal(&prototype),
                fields,
                function,
            )?,
            function,
        );
        let value = schema.reserve_value_local(function);
        value.set_reference(&object, schema, function);
        output.set_normal(&value, function);
        value.clear(function);
        object.clear(function);
        prototype.clear(function);
        realm.clear(function);
        Ok(())
    }

    fn emit_native_stored_descriptor_object(
        &mut self,
        record: &GcLocal<PropertyDescriptor>,
        output: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let flags = schema.reserve_i32_local(function);
        schema
            .struct_type::<PropertyDescriptor>()
            .field(PropertyDescriptorSchema::FLAGS)
            .read(record, schema, function)
            .store(flags, function);
        let accessor = schema.reserve_i32_local(function);
        let data = schema.reserve_i32_local(function);
        let writable = schema.reserve_i32_local(function);
        let enumerable = schema.reserve_i32_local(function);
        let configurable = schema.reserve_i32_local(function);
        for (mask, destination) in [
            (DescriptorMask::ACCESSOR, accessor),
            (DescriptorMask::WRITABLE, writable),
            (DescriptorMask::ENUMERABLE, enumerable),
            (DescriptorMask::CONFIGURABLE, configurable),
        ] {
            flags.load(function);
            function.instruction(&Instruction::I32Const(mask.as_i32()));
            function.instruction(&Instruction::I32And);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::I32Eqz);
            destination.store(function);
        }
        accessor.load(function);
        function.instruction(&Instruction::I32Eqz);
        data.store(function);
        let value = schema.reserve_value_local(function);
        let getter = schema.reserve_value_local(function);
        let setter = schema.reserve_value_local(function);
        for (field, destination) in [
            (PropertyDescriptorSchema::VALUE, &value),
            (PropertyDescriptorSchema::GETTER, &getter),
            (PropertyDescriptorSchema::SETTER, &setter),
        ] {
            let stored = schema.reserve_gc_local(function).initialize(
                schema
                    .struct_type::<PropertyDescriptor>()
                    .field(field)
                    .read(record, schema, function)
                    .reference(),
                function,
            );
            schema
                .struct_type::<StoredValue>()
                .read_into(&stored, destination, schema, function);
            stored.clear(function);
        }
        self.emit_native_descriptor_object(
            &DescriptorObjectFields {
                value: Presence::Runtime {
                    present: data,
                    value: &value,
                },
                writable: Presence::Runtime {
                    present: data,
                    value: DescriptorFlag::BooleanPayload(writable),
                },
                get: Presence::Runtime {
                    present: accessor,
                    value: &getter,
                },
                set: Presence::Runtime {
                    present: accessor,
                    value: &setter,
                },
                enumerable: Presence::Present(DescriptorFlag::BooleanPayload(enumerable)),
                configurable: Presence::Present(DescriptorFlag::BooleanPayload(configurable)),
            },
            output,
            function,
        )?;
        setter.clear(function);
        getter.clear(function);
        value.clear(function);
        for local in [configurable, enumerable, writable, data, accessor] {
            schema.release_i32_local(local, function);
        }
        schema.release_i32_local(flags, function);
        Ok(())
    }
}
