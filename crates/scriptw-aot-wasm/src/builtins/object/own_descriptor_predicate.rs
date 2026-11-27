use super::*;
use crate::gc_types::{
    CompactDataStorage, CompactShapeKeys, I32Local, OrdinaryObjectSchema, StringValueSchema,
};

enum OwnDescriptorPredicateBuiltin {
    ObjectHasOwn,
    PrototypeHasOwnProperty,
    PrototypePropertyIsEnumerable,
}

impl FunctionBuilder<'_> {
    fn emit_hinted_compact_own_property_or_branch(
        &mut self,
        ordinary: &GcLocal<OrdinaryObject>,
        key: &GcLocal<StringValue>,
        found: I32Local,
        miss: ControlTarget,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let slot = schema.reserve_i32_local(function);
        schema
            .field(StringValueSchema::ENTRY_HINT)
            .read(key, schema, function)
            .store(slot, function);
        slot.load(function);
        function.instruction(&Instruction::I32Const(0));
        function.instruction(&Instruction::I32LtS);
        self.emit_branch_if_to_target(miss, function);
        let properties = schema.reserve_gc_local(function).initialize(
            schema
                .field(OrdinaryObjectSchema::PROPERTIES)
                .read(ordinary, schema, function)
                .reference(),
            function,
        );
        properties.is_subtype::<CompactDataStorage>(schema, function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, function);
        let storage = schema.reserve_gc_local(function).initialize(
            properties
                .load(schema, function)
                .cast_subtype::<CompactDataStorage>(schema, function),
            function,
        );
        let keys = self.emit_compact_shape_keys(&storage, function);
        slot.load(function);
        schema
            .array_type::<CompactShapeKeys>()
            .length(&keys, schema, function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(miss, function);
        // The hint is only an ordinal. A hit must identify the actual own
        // key in this receiver's current shape, even after reassignment or
        // a storage change. Structural equality takes the normal lookup.
        schema
            .array_type::<CompactShapeKeys>()
            .read(&keys, slot, schema, function)
            .reference();
        key.load(schema, function);
        function.instruction(&Instruction::RefEq);
        found.store(function);
        found.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, function);
        keys.clear(function);
        storage.clear(function);
        properties.clear(function);
        schema.release_i32_local(slot, function);
    }

    fn compile_object_own_descriptor_predicate_builtin(
        &mut self,
        builtin: OwnDescriptorPredicateBuiltin,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let receiver = schema.reserve_value_local(function);
        let key_value = schema.reserve_value_local(function);
        let object = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        let boolean = schema.reserve_i32_local(function);
        let enumerable = schema.reserve_i32_local(function);
        match &builtin {
            OwnDescriptorPredicateBuiltin::ObjectHasOwn => {
                self.emit_builtin_arg_to_value(0, &receiver, function);
                self.emit_builtin_arg_to_value(1, &key_value, function);
            }
            OwnDescriptorPredicateBuiltin::PrototypeHasOwnProperty
            | OwnDescriptorPredicateBuiltin::PrototypePropertyIsEnumerable => {
                self.compile_this_to_locals(&receiver, function)?;
                self.emit_builtin_arg_to_value(0, &key_value, function);
            }
        }
        // When both operands are already an actual OrdinaryObject and a String,
        // neither ToObject nor ToPropertyKey can observe or throw, regardless of
        // the spec's coercion order between the static and prototype forms.
        receiver.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Object.tag()));
        function.instruction(&Instruction::I32Eq);
        key_value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String.tag()));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        receiver.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<OrdinaryObject>(crate::gc_types::GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let ordinary = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<OrdinaryObject>(schema, function),
            function,
        );
        let key_string = schema.reserve_gc_local(function).initialize(
            key_value.cast_reference::<StringValue>(schema, function),
            function,
        );
        let own_known = (!matches!(
            builtin,
            OwnDescriptorPredicateBuiltin::PrototypePropertyIsEnumerable
        ))
        .then(|| self.open_frame(ControlFrameKind::Block, function));
        if let Some(own_known) = own_known {
            let miss = self.open_frame(ControlFrameKind::Block, function);
            self.emit_hinted_compact_own_property_or_branch(
                &ordinary,
                &key_string,
                boolean,
                miss,
                function,
            );
            self.emit_branch_to_target(own_known, function);
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
        }
        let key = PropertyKeyLocals::from_string(schema, &key_string, function);
        self.emit_ordinary_own_property_enumerability(
            &ordinary, &key, boolean, enumerable, function,
        )?;
        key.clear(function);
        if own_known.is_some() {
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
        }
        key_string.clear(function);
        ordinary.clear(function);
        function.instruction(&Instruction::Else);
        let key = match &builtin {
            OwnDescriptorPredicateBuiltin::ObjectHasOwn => {
                self.emit_value_to_object_locals(&receiver, &output, function)?;
                self.completion().copy_from(&output, function);
                self.emit_propagate_current_throw_if_needed(function);
                object.copy_from(output.value(), function);
                self.emit_value_to_property_key_locals(&key_value, function)?
            }
            OwnDescriptorPredicateBuiltin::PrototypeHasOwnProperty
            | OwnDescriptorPredicateBuiltin::PrototypePropertyIsEnumerable => {
                // Prototype forms coerce the key before ToObject(this).
                let key = self.emit_value_to_property_key_locals(&key_value, function)?;
                self.emit_value_to_object_locals(&receiver, &output, function)?;
                self.completion().copy_from(&output, function);
                self.emit_propagate_current_throw_if_needed(function);
                object.copy_from(output.value(), function);
                key
            }
        };
        self.emit_own_property_enumerability(&object, &key, boolean, enumerable, function)?;
        key.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        match &builtin {
            OwnDescriptorPredicateBuiltin::ObjectHasOwn
            | OwnDescriptorPredicateBuiltin::PrototypeHasOwnProperty => {}
            OwnDescriptorPredicateBuiltin::PrototypePropertyIsEnumerable => {
                boolean.load(function);
                enumerable.load(function);
                function.instruction(&Instruction::I32And);
                boolean.store(function);
            }
        }
        receiver.set_boolean(boolean, function);
        output.set_normal(&receiver, function);
        self.completion().copy_from(&output, function);
        schema.release_i32_local(enumerable, function);
        schema.release_i32_local(boolean, function);
        output.clear(function);
        object.clear(function);
        key_value.clear(function);
        receiver.clear(function);
        Ok(())
    }
    pub(in crate::builtins) fn compile_object_has_own_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_object_own_descriptor_predicate_builtin(
            OwnDescriptorPredicateBuiltin::ObjectHasOwn,
            function,
        )
    }
    pub(in crate::builtins) fn compile_object_prototype_has_own_property_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_object_own_descriptor_predicate_builtin(
            OwnDescriptorPredicateBuiltin::PrototypeHasOwnProperty,
            function,
        )
    }
    pub(in crate::builtins) fn compile_object_prototype_property_is_enumerable_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.compile_object_own_descriptor_predicate_builtin(
            OwnDescriptorPredicateBuiltin::PrototypePropertyIsEnumerable,
            function,
        )
    }
}
