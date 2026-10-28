//! Actual primitive String consumers bypass boxing without trusting IR kinds.
use super::*;
use crate::gc_types::StringValue;

/// An effect-free, exact `length` key. Computed expressions cannot mint this
/// proof merely because their inferred kind or eventual value is String.
struct StaticLengthKey;
impl StaticLengthKey {
    fn for_property_key(key: &PropertyKeyIr) -> Option<Self> {
        match key {
            PropertyKeyIr::ArrayLength => Some(Self),
            PropertyKeyIr::StaticString(name) if name == "length" => Some(Self),
            _ => None,
        }
    }

    fn for_get_v_operand(key: &TypedExpr) -> Option<Self> {
        matches!(&key.expr, ExprIr::String(name) if name == "length").then_some(Self)
    }
}

/// Both addition operands and the entire static outer consumer are validated
/// before any source expression is emitted. A materialized intermediate or
/// computed key retains its original compilation path.
struct ConcatLengthConsumer<'a> {
    target: &'a TypedExpr,
    left: &'a TypedExpr,
    right: &'a TypedExpr,
}
impl<'a> ConcatLengthConsumer<'a> {
    fn new(target: &'a TypedExpr, _key: StaticLengthKey) -> Option<Self> {
        let (ExprIr::StringConcat { lhs, rhs } | ExprIr::CoerciveAdd { lhs, rhs }) = &target.expr
        else {
            return None;
        };
        Some(Self {
            target,
            left: lhs,
            right: rhs,
        })
    }
}

impl FunctionBuilder<'_> {
    /// `string` is already a rooted, actual StringValue, never a boxed String.
    fn emit_primitive_string_length_value(
        &self,
        string: &GcLocal<StringValue>,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let length = schema.reserve_i64_local(function);
        self.emit_native_gc_string_length(string, length, function);
        length.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        length.store(function);
        output.set_number(length, function);
        schema.release_i64_local(length, function);
    }

    pub(super) fn try_compile_primitive_string_static_length(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        receiver: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(_key) = StaticLengthKey::for_property_key(key) else {
            return Ok(false);
        };
        let schema = self.runtime_schema();
        receiver.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<StringValue>(schema, function),
            function,
        );
        self.emit_primitive_string_length_value(&string, output, function);
        string.clear(function);
        function.instruction(&Instruction::Else);
        self.compile_ordinary_property_read_from_locals(target, key, receiver, output, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(true)
    }

    /// Canonical GetV still owns operand evaluation. For a literal length key,
    /// inspect its original receiver before ToObject can box an actual String.
    pub(crate) fn emit_get_v_primitive_string_length(
        &mut self,
        key: &TypedExpr,
        receiver: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Option<I32Local> {
        let _key = StaticLengthKey::for_get_v_operand(key)?;
        let schema = self.runtime_schema();
        let handled = schema.reserve_i32_local(function);
        handled.set_constant(0, function);
        receiver.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        let string = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<StringValue>(schema, function),
            function,
        );
        self.emit_primitive_string_length_value(&string, result.value(), function);
        result.set_normal(result.value(), function);
        handled.set_constant(1, function);
        string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Some(handled)
    }

    pub(super) fn try_compile_concat_length_property_read(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(plan) = StaticLengthKey::for_property_key(key)
            .and_then(|key| ConcatLengthConsumer::new(target, key))
        else {
            return Ok(false);
        };
        self.compile_concat_length_consumer(plan, output, function)?;
        Ok(true)
    }

    pub(crate) fn try_compile_concat_length_get_v(
        &mut self,
        operation: SpecOperationIr,
        operands: &[TypedExpr],
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        if operation != SpecOperationIr::GetV {
            return Ok(false);
        }
        let [target, key] = operands else {
            return Ok(false);
        };
        let Some(plan) = StaticLengthKey::for_get_v_operand(key)
            .and_then(|key| ConcatLengthConsumer::new(target, key))
        else {
            return Ok(false);
        };
        self.compile_concat_length_consumer(plan, output, function)?;
        Ok(true)
    }

    fn compile_concat_length_consumer(
        &mut self,
        plan: ConcatLengthConsumer<'_>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let left = schema.reserve_value_local(function);
        let right = schema.reserve_value_local(function);
        self.compile_expr_to_value(plan.left, &left, function)?;
        self.compile_expr_to_value(plan.right, &right, function)?;
        let handled = schema.reserve_i32_local(function);
        handled.set_constant(0, function);
        left.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        right.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        let left_string = schema.reserve_gc_local(function).initialize(
            left.cast_reference::<StringValue>(schema, function),
            function,
        );
        let right_string = schema.reserve_gc_local(function).initialize(
            right.cast_reference::<StringValue>(schema, function),
            function,
        );
        let left_length = schema.reserve_i64_local(function);
        let right_length = schema.reserve_i64_local(function);
        let total = schema.reserve_i64_local(function);
        self.emit_native_gc_string_length(&left_string, left_length, function);
        self.emit_native_gc_string_length(&right_string, right_length, function);
        left_length.load(function);
        right_length.load(function);
        function.instruction(&Instruction::I64Add);
        total.store(function);
        total.load(function);
        function.instruction(&Instruction::I64Const(u32::MAX as i64));
        function.instruction(&Instruction::I64LeU);
        self.open_frame(ControlFrameKind::If, function);
        total.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        total.store(function);
        output.set_number(total, function);
        handled.set_constant(1, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        schema.release_i64_local(total, function);
        schema.release_i64_local(right_length, function);
        schema.release_i64_local(left_length, function);
        right_string.clear(function);
        left_string.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);

        let added = schema.reserve_completion(function);
        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_coercive_add_completion_from_values(&left, &right, &added, function)?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        // Both roots are no longer needed on either route. In particular an
        // abrupt fallback must release them before entering a source handler.
        right.clear(function);
        left.clear(function);
        handled.load(function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        self.finish_coercive_add_completion(&added, function)?;
        self.compile_property_read_from_locals(
            plan.target,
            &PropertyKeyIr::ArrayLength,
            added.value(),
            output,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        added.clear(function);
        schema.release_i32_local(handled, function);
        Ok(())
    }
}
