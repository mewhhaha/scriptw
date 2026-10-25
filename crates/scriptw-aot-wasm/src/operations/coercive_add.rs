use super::*;
use crate::runtime_helpers::{CoerciveAddArguments, CoerciveAddParameters, HelperParameters};

impl FunctionBuilder<'_> {
    /// Evaluation stays in P; the complete observable addition runs once in R.
    pub(crate) fn compile_coercive_add_to_locals(
        &mut self,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if lhs.kind == ValueKind::Number && rhs.kind == ValueKind::Number {
            return self.compile_binary_number_to_value(
                ArithmeticBinaryOp::Add,
                lhs,
                rhs,
                output,
                function,
            );
        }
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        let result = schema.reserve_completion(function);
        // Both GetValue operations complete before the first ToPrimitive call.
        self.compile_expr_to_value(lhs, &left, function)?;
        self.compile_expr_to_value(rhs, &right, function)?;
        let realm = self.emit_execution_realm(function);
        schema
            .call_helper(
                CoerciveAddArguments::new(
                    &left,
                    &right,
                    &realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&result, function);
        realm.clear(function);
        right.clear(function);
        left.clear(function);
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ActiveHandler,
            &result,
            function,
        )?;
        output.copy_from(result.value(), function);
        result.clear(function);
        Ok(())
    }

    pub(crate) fn compile_coercive_add_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::CoerciveAdd);
        let parameters = self.helper_parameters::<CoerciveAddParameters>(&mut function);
        self.push_scope();
        self.completion().initialize(&mut function);
        let output = self.runtime_schema().reserve_value_local(&mut function);
        self.emit_coercive_add_from_values_inner(
            &parameters.left,
            &parameters.right,
            &output,
            &mut function,
        )?;
        self.completion().set_normal(&output, &mut function);
        output.clear(&mut function);
        self.pop_scope();
        self.clear_helper_execution_realm(&mut function);
        parameters.release(&mut function);
        self.completion().emit(&mut function);
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Only the registered helper compiler can emit this physical body. There
    /// is no call back to the source-expression facade or its own helper.
    /// With no helper-local handler, the existing abrupt routes return the
    /// original complete Throw; P alone selects the caller's active handler.
    fn emit_coercive_add_from_values_inner(
        &mut self,
        left_input: &ValueLocals,
        right_input: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        for (input, primitive) in [(left_input, &left), (right_input, &right)] {
            self.emit_tagged_to_primitive_locals(
                ToPrimitiveHint::Default,
                input,
                &pending,
                ToPrimitiveAbruptRoute::ActiveHandler,
                function,
            )?;
            primitive.copy_from(pending.value(), function);
        }
        left.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        right.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32Or);
        self.open_frame(ControlFrameKind::If, function);
        // ToPrimitive has completed for both operands. ToString is an identity
        // only for an actual String; retain conversion and abrupt completion
        // for every other primitive, in left-to-right order. Reuse these rooted
        // values so the concatenation body and its temporary roots occur once.
        for value in [&left, &right] {
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
            function.instruction(&Instruction::I32Ne);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_value_to_string_payload(value, &pending, function)?;
            self.finish_to_primitive_operation(
                ToPrimitiveAbruptRoute::ActiveHandler,
                &pending,
                function,
            )?;
            value.copy_from(pending.value(), function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        let left_string = schema.reserve_gc_local(function).initialize(
            left.cast_reference::<StringValue>(schema, function),
            function,
        );
        let right_string = schema.reserve_gc_local(function).initialize(
            right.cast_reference::<StringValue>(schema, function),
            function,
        );
        let string = schema.reserve_gc_local(function).initialize(
            self.emit_concat_gc_strings(&left_string, &right_string, function),
            function,
        );
        output.set_reference(&string, schema, function);
        string.clear(function);
        right_string.clear(function);
        left_string.clear(function);
        function.instruction(&Instruction::Else);
        for value in [&left, &right] {
            value.tag().load(function);
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::BigInt as i32));
            function.instruction(&Instruction::I32Ne);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_value_to_number_payload(value, &pending, function)?;
            self.finish_to_primitive_operation(
                ToPrimitiveAbruptRoute::ActiveHandler,
                &pending,
                function,
            )?;
            value.copy_from(pending.value(), function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
        }
        self.emit_numeric_pair_agreement(&left, &right, function)?;
        self.emit_is_bigint_tag_i32(left.tag(), function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_numeric_bigint_operation(BigIntHelperOp::Add, &left, &right, output, function)?;
        function.instruction(&Instruction::Else);
        left.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        right.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Add);
        function.instruction(&Instruction::I64ReinterpretF64);
        output.scalar().store(function);
        output.set_number(output.scalar(), function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        pending.clear(function);
        right.clear(function);
        left.clear(function);
        Ok(())
    }
}
