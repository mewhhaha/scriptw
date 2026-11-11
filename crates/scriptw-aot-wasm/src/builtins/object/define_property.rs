//! Object.defineProperty consumes the sole validated descriptor owner.
use super::*;

impl FunctionBuilder<'_> {
    pub(in crate::builtins) fn compile_object_define_property_builtin(
        &mut self,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let target = schema.reserve_value_local(function);
        let key_value = schema.reserve_value_local(function);
        let attributes = schema.reserve_value_local(function);
        let output = schema.reserve_completion(function);
        self.emit_builtin_arg_to_value(0, &target, function);
        self.emit_is_heap_object_like_tag_i32(target.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_builtin_arg_to_value(1, &key_value, function);
        let key = self.emit_value_to_property_key_locals(&key_value, function)?;
        self.emit_builtin_arg_to_value(2, &attributes, function);
        let descriptor = self.emit_to_property_descriptor(
            &attributes,
            RuntimeErrorMessage::OBJECT_DEFINEPROPERTY_ATTRIBUTES_MUST_BE_OBJECT,
            function,
        )?;
        self.emit_object_define_entry_validated(
            &target,
            &key,
            &descriptor.definition_descriptor(),
            &output,
            function,
        )?;
        descriptor.clear(schema, function);
        key.clear(function);
        output.kind().load(function);
        function.instruction(&Instruction::I32Const(CompletionKind::Normal.code() as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        self.compile_truthy_tagged_i32(output.value(), function)?;
        self.open_frame(ControlFrameKind::If, function);
        output.set_normal(&target, function);
        function.instruction(&Instruction::Else);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::TYPEERROR,
            &output,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        function.instruction(&Instruction::Else);
        self.emit_throw_current_function_realm_type_error(
            RuntimeErrorMessage::TYPEERROR,
            &output,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.completion().copy_from(&output, function);
        output.clear(function);
        attributes.clear(function);
        key_value.clear(function);
        target.clear(function);
        Ok(())
    }
}
