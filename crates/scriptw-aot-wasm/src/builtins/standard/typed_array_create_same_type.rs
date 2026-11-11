use super::*;

impl FunctionBuilder<'_> {
    pub(super) fn emit_typed_array_create_same_type(
        &mut self,
        kind: I32Local,
        length: I64Local,
        result: &CompletionLocals,
        f: &mut Function,
    ) -> Result<(), EmitError> {
        let s = self.runtime_schema();
        let constructor = self.emit_current_function_realm_typed_array_constructor(kind, f)?;
        let number = s.reserve_value_local(f);
        length.load(f);
        f.instruction(&Instruction::F64ConvertI64U);
        f.instruction(&Instruction::I64ReinterpretF64);
        number.scalar().store(f);
        number.set_number(number.scalar(), f);
        let argv = self.emit_pre_evaluated_arg_vector(&[&number], f);
        self.emit_binary_construct_typed_array(
            constructor.value(),
            &argv,
            TypedArrayCreateAccess::ReadWrite,
            Some(length),
            result,
            f,
        )?;
        argv.clear(f);
        number.clear(f);
        constructor.clear(f);
        Ok(())
    }
}
