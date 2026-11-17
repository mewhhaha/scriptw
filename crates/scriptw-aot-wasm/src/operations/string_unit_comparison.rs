//! `s[i] === "c"` for a literal index and a literal one-unit String asks
//! whether one code unit of a String is that unit. When the base is a String
//! primitive and the index is inside it, the indexed property is the own,
//! non-writable String exotic element holding exactly that unit as a
//! one-unit String, so the comparison is the unit comparison and nothing is
//! allocated. Every other base, and an index at or past the length (which
//! reads the prototype chain), takes the ordinary read and comparison.
use super::*;

struct StringUnitComparison<'a> {
    read: &'a TypedExpr,
    target: &'a TypedExpr,
    key: &'a PropertyKeyIr,
    literal: &'a TypedExpr,
    index: i32,
    unit: u16,
}

impl<'a> StringUnitComparison<'a> {
    fn for_operands(left: &'a TypedExpr, right: &'a TypedExpr) -> Option<Self> {
        Self::for_read_and_literal(left, right).or_else(|| Self::for_read_and_literal(right, left))
    }

    fn for_read_and_literal(read: &'a TypedExpr, literal: &'a TypedExpr) -> Option<Self> {
        let ExprIr::PropertyRead { target, key } = &read.expr else {
            return None;
        };
        let (PropertyKeyIr::StringExpr(index) | PropertyKeyIr::ArrayIndex(index)) = key else {
            return None;
        };
        // A literal index has no evaluation to order against the base.
        let ExprIr::Number(bits) = &index.expr else {
            return None;
        };
        let number = f64::from_bits(*bits);
        // -0 names index 0, as its property key is "0".
        if !(number >= 0.0 && number <= f64::from(i32::MAX) && number.fract() == 0.0) {
            return None;
        }
        let ExprIr::String(text) = &literal.expr else {
            return None;
        };
        let units = scriptw_ir::decode_js_string_utf16(text);
        let [unit] = units[..] else {
            return None;
        };
        Some(Self {
            read,
            target,
            key,
            literal,
            index: number as i32,
            unit,
        })
    }
}

impl FunctionBuilder<'_> {
    pub(super) fn try_compile_string_unit_comparison_i32(
        &mut self,
        left: &TypedExpr,
        right: &TypedExpr,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(plan) = StringUnitComparison::for_operands(left, right) else {
            return Ok(false);
        };
        // A strict invocation's projected `arguments[i]` is never a String
        // base, and the projection answers it from the argument vector only
        // when it is compiled whole: evaluating its base here would build the
        // object the projection exists to avoid.
        if self.is_projected_arguments_read(plan.read) {
            return Ok(false);
        }
        let schema = self.runtime_schema();
        let result = schema.reserve_i32_local(function);
        let receiver = schema.reserve_value_local(function);
        // The base is the only operand with an evaluation.
        self.compile_expr_to_value(plan.target, &receiver, function)?;
        let done = self.open_frame(ControlFrameKind::Block, function);
        let general = self.open_frame(ControlFrameKind::Block, function);
        receiver.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(general, function);
        let string = schema.reserve_gc_local(function).initialize(
            receiver.cast_reference::<StringValue>(schema, function),
            function,
        );
        let units = schema.reserve_gc_local(function).initialize(
            schema
                .field(StringValueSchema::CODE_UNITS)
                .read(&string, schema, function)
                .reference(),
            function,
        );
        string.clear(function);
        let index = schema.reserve_i32_local(function);
        index.set_constant(plan.index, function);
        index.load(function);
        schema
            .array_type::<CodeUnitArray>()
            .length(&units, schema, function);
        function.instruction(&Instruction::I32GeU);
        self.emit_branch_if_to_target(general, function);
        schema
            .array_type::<CodeUnitArray>()
            .read(&units, index, schema, function)
            .store(result, function);
        result.load(function);
        function.instruction(&Instruction::I32Const(i32::from(plan.unit)));
        function.instruction(&Instruction::I32Eq);
        result.store(function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        schema.release_i32_local(index, function);
        units.clear(function);
        let value = schema.reserve_value_local(function);
        let literal = schema.reserve_value_local(function);
        self.compile_property_read_from_locals(plan.target, plan.key, &receiver, &value, function)?;
        self.compile_expr_to_value(plan.literal, &literal, function)?;
        self.emit_tagged_payload_equality_i32(&value, &literal, function)?;
        result.store(function);
        literal.clear(function);
        value.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        receiver.clear(function);
        result.load(function);
        schema.release_i32_local(result, function);
        Ok(true)
    }
}
