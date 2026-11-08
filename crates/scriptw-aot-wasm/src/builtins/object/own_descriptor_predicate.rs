use super::*;

enum OwnDescriptorPredicateBuiltin {
    ObjectHasOwn,
    PrototypeHasOwnProperty,
    PrototypePropertyIsEnumerable,
}

impl FunctionBuilder<'_> {
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
        let key = match &builtin {
            OwnDescriptorPredicateBuiltin::ObjectHasOwn => {
                self.emit_builtin_arg_to_value(0, &receiver, function);
                self.emit_value_to_object_locals(&receiver, &output, function)?;
                self.completion().copy_from(&output, function);
                self.emit_propagate_current_throw_if_needed(function);
                object.copy_from(output.value(), function);
                self.emit_builtin_arg_to_value(1, &key_value, function);
                self.emit_value_to_property_key_locals(&key_value, function)?
            }
            OwnDescriptorPredicateBuiltin::PrototypeHasOwnProperty
            | OwnDescriptorPredicateBuiltin::PrototypePropertyIsEnumerable => {
                self.compile_this_to_locals(&receiver, function)?;
                self.emit_builtin_arg_to_value(0, &key_value, function);
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
        key.clear(function);
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
