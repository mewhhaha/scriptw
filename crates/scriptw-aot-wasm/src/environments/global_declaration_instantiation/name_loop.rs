//! One emitted loop for each ordered phase, using the retained actual P owner.
use super::*;
use crate::data::GlobalVarDeclarationNames;

impl FunctionBuilder<'_> {
    pub(super) fn emit_global_var_name_loop(
        &mut self,
        names: GlobalVarDeclarationNames,
        function: &mut Function,
        mut body: impl FnMut(&mut Self, &GcLocal<StringValue>, &mut Function) -> Result<(), EmitError>,
    ) -> Result<(), EmitError> {
        let span = names.span();
        if span.length() == 0 {
            return Ok(());
        }
        let schema = self.runtime_schema();
        let metadata = self.current_program().metadata_span(schema, span, function);
        let cursor = schema.reserve_i32_local(function);
        let operand = schema.reserve_i64_local(function);
        cursor.set_constant(0, function);
        let done = self.open_frame(ControlFrameKind::Block, function);
        let next = self.open_frame(ControlFrameKind::Loop, function);
        cursor.load(function);
        function.instruction(&Instruction::I32Const(span.length() as i32));
        function.instruction(&Instruction::I32GeU);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        metadata.read_u64(cursor, 0, schema, function);
        operand.store(function);
        let key = schema
            .reserve_gc_local::<StringValue, NonNullable>(function)
            .initialize(
                self.current_program().literal_operand(
                    schema,
                    operand,
                    self.runtime_helper_base()?,
                    function,
                ),
                function,
            );
        body(self, &key, function)?;
        key.clear(function);
        cursor.load(function);
        function.instruction(&Instruction::I32Const(8));
        function.instruction(&Instruction::I32Add);
        cursor.store(function);
        self.emit_branch_to_target(next, function);
        self.pop_control(ControlFrameKind::Loop);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        metadata.clear(schema, function);
        schema.release_i64_local(operand, function);
        schema.release_i32_local(cursor, function);
        Ok(())
    }
}
