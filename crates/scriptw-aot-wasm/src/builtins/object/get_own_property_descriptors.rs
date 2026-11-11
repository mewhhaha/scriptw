//! A fixed own-key snapshot precedes per-key descriptor acquisition/publication.
use super::*;

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn compile_object_get_own_property_descriptors_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let input = schema.reserve_value_local(function);
        let object = schema.reserve_value_local(function);
        let output_value = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        let output = schema.reserve_completion(function);
        let index = schema.reserve_i32_local(function);
        let count = schema.reserve_i32_local(function);
        let cleanup = self.open_frame(ControlFrameKind::Block, function);
        self.emit_builtin_arg_to_value(0, &input, function);
        self.emit_value_to_object_locals(&input, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, cleanup, function);
        object.copy_from(pending.value(), function);
        let keys = self.emit_object_own_property_keys(&object, function)?;
        let realm = self.emit_execution_realm(function);
        let prototype = schema.reserve_value_local(function);
        self.emit_load_non_array_realm_intrinsic(
            &realm,
            crate::functions::NonArrayRealmIntrinsicSlot::ObjectPrototype,
            &prototype,
            function,
        );
        let result = schema.reserve_gc_local(function).initialize(
            self.emit_alloc_plain_object_with_prototype(Some(&prototype), function)?,
            function,
        );
        output_value.set_reference(&result, schema, function);
        keys.length(count, schema, function);
        function.instruction(&Instruction::I32Const(0));
        index.store(function);
        let exit = self.open_frame(ControlFrameKind::Block, function);
        let iteration = self.open_frame(ControlFrameKind::Loop, function);
        index.load(function);
        count.load(function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(exit, function);
        let key = keys.read_key(index, self, function)?;
        self.emit_get_own_property_descriptor_object(&object, &key, &pending, function)?;
        self.emit_native_object_abrupt_exit(&pending, &output, cleanup, function);
        pending.value().tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Undefined.tag()));
        function.instruction(&Instruction::I32Ne);
        self.open_frame(ControlFrameKind::If, function);
        // Save the complete fresh descriptor while the definition overwrites
        // pending. The key may be a Symbol; no string conversion is performed.
        let descriptor = schema.reserve_value_local(function);
        descriptor.copy_from(pending.value(), function);
        self.emit_create_data_property_or_throw(
            &output_value,
            &key,
            &descriptor,
            &pending,
            function,
        )?;
        descriptor.clear(function);
        self.emit_native_object_abrupt_exit(&pending, &output, cleanup, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        key.clear(function);
        index.load(function);
        function.instruction(&Instruction::I32Const(1));
        function.instruction(&Instruction::I32Add);
        index.store(function);
        self.emit_branch_to_target(iteration, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        output.set_normal(&output_value, function);
        result.clear(function);
        prototype.clear(function);
        realm.clear(function);
        keys.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        schema.release_i32_local(count, function);
        schema.release_i32_local(index, function);
        output.clear(function);
        pending.clear(function);
        output_value.clear(function);
        object.clear(function);
        input.clear(function);
        Ok(())
    }
}
