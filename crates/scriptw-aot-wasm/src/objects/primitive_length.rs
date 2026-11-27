//! Exact static length consumers inspect actual Strings and Arrays before Get.
use super::*;
use crate::gc_types::{ArrayObject, ArrayObjectSchema, MAX_STRING_LENGTH, StringValue};

mod local_binding;

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
    fallback_binding: Option<crate::emit::LocalBindingId>,
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
            fallback_binding: None,
        })
    }
}

/// A contiguous straight-line loop prefix can establish the exact lengths
/// of private String locals on every trip, with no value feedback at entry.
fn literal_loop_concat_length(
    builder: &FunctionBuilder<'_>,
    plan: &ConcatLengthConsumer<'_>,
) -> Option<i64> {
    if builder.bounded_inductions.is_empty() {
        return None;
    }
    let binding = |value: &TypedExpr| match &value.expr {
        ExprIr::Identifier(name) => builder.private_local_binding(name),
        _ => None,
    };
    let left = binding(plan.left)?;
    let right = binding(plan.right)?;
    for statement in builder.body.statements.iter().take(64) {
        let StatementIr::For {
            body,
            lexical_environment: None,
            ..
        } = statement
        else {
            continue;
        };
        let StatementIr::Block(body) = body.as_ref() else {
            continue;
        };
        if body.lexical_environment.is_some() {
            continue;
        }
        let Some(position) = body.statements.iter().take(64).position(|statement| {
            let StatementIr::Expression(value) = statement else {
                return false;
            };
            let value: &TypedExpr = match &value.expr {
                ExprIr::AssignIdentifier { value, .. } => value,
                ExprIr::CompoundAssignIdentifier { name, value, .. }
                    if builder.private_local_binding(name).is_some() =>
                {
                    value
                }
                _ => value,
            };
            let target: &TypedExpr = match &value.expr {
                ExprIr::PropertyRead { target, key }
                    if StaticLengthKey::for_property_key(key).is_some() =>
                {
                    target
                }
                ExprIr::SpecOperation {
                    operation: SpecOperationIr::GetV,
                    operands,
                } => match operands.as_slice() {
                    [target, key] if StaticLengthKey::for_get_v_operand(key).is_some() => target,
                    _ => return false,
                },
                _ => return false,
            };
            std::ptr::eq(target, plan.target)
        }) else {
            continue;
        };
        let mut left_length = None;
        let mut right_length = None;
        for statement in body.statements[..position].iter().rev().take(8) {
            let StatementIr::Expression(value) = statement else {
                break;
            };
            let ExprIr::AssignIdentifier { name, value } = &value.expr else {
                break;
            };
            let Some(id) = builder.private_local_binding(name) else {
                break;
            };
            let Some(length) = assigned_literal_length(value, builder, 8) else {
                break;
            };
            if id == left && left_length.is_none() {
                left_length = Some(length);
            }
            if id == right && right_length.is_none() {
                right_length = Some(length);
            }
            if let (Some(left), Some(right)) = (left_length, right_length) {
                let total = i64::from(left) + i64::from(right);
                return (total <= MAX_STRING_LENGTH).then_some(total);
            }
        }
    }
    None
}

fn assigned_literal_length(
    value: &TypedExpr,
    builder: &FunctionBuilder<'_>,
    depth: u8,
) -> Option<u32> {
    let depth = depth.checked_sub(1)?;
    match &value.expr {
        ExprIr::String(text) => u32::try_from(scriptw_ir::decode_js_string_utf16(text).len()).ok(),
        ExprIr::Conditional {
            condition,
            then_expr,
            else_expr,
        } if primitive_loop_condition(condition, builder, depth) => {
            let yes = assigned_literal_length(then_expr, builder, depth)?;
            let no = assigned_literal_length(else_expr, builder, depth)?;
            (yes == no).then_some(yes)
        }
        _ => None,
    }
}

/// Predicates stay at the original assignments. Prove they cannot overwrite
/// any other private local while selecting a String alternative.
fn primitive_loop_condition(value: &TypedExpr, builder: &FunctionBuilder<'_>, depth: u8) -> bool {
    let Some(depth) = depth.checked_sub(1) else {
        return false;
    };
    match &value.expr {
        ExprIr::Number(_) | ExprIr::Boolean(_) => true,
        ExprIr::Identifier(_) => builder.bounded_induction_read(value).is_some(),
        ExprIr::BinaryNumber { lhs, rhs, .. }
        | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
        | ExprIr::CompareNumber { lhs, rhs, .. }
        | ExprIr::CompareValue { lhs, rhs, .. }
        | ExprIr::BitwiseNumeric { lhs, rhs, .. } => {
            primitive_loop_condition(lhs, builder, depth)
                && primitive_loop_condition(rhs, builder, depth)
        }
        ExprIr::LogicalNot { expr } => primitive_loop_condition(expr, builder, depth),
        _ => false,
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

    /// Concrete Arrays own a nonconfigurable uint32 length in this I64 field.
    /// The exact-key proof and actual Array root exclude getters and exotics.
    fn emit_array_length_value(
        &self,
        _key: StaticLengthKey,
        array: &GcLocal<ArrayObject>,
        output: &ValueLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let length = schema.reserve_i64_local(function);
        schema
            .field(ArrayObjectSchema::LENGTH)
            .read(array, schema, function)
            .store_i64(length, function);
        length.load(function);
        function.instruction(&Instruction::F64ConvertI64U);
        function.instruction(&Instruction::I64ReinterpretF64);
        length.store(function);
        output.set_number(length, function);
        schema.release_i64_local(length, function);
    }

    pub(super) fn try_compile_static_length(
        &mut self,
        target: &TypedExpr,
        key: &PropertyKeyIr,
        receiver: &ValueLocals,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        let Some(key_proof) = StaticLengthKey::for_property_key(key) else {
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
        // One inline test proves and casts an actual Array.
        let done = self.open_frame(ControlFrameKind::Block, function);
        let other = self.open_frame(ControlFrameKind::Block, function);
        let array = schema.reserve_gc_local(function).initialize(
            receiver.sealed_reference_or_branch::<ArrayObject>(
                schema,
                self.local_branch_label(other),
                function,
            ),
            function,
        );
        self.emit_array_length_value(key_proof, &array, output, function);
        array.clear(function);
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.compile_ordinary_property_read_from_locals(target, key, receiver, output, function)?;
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(true)
    }

    /// Canonical GetV still owns operand evaluation. For a literal length key,
    /// inspect its original receiver before boxing or ordinary property lookup.
    pub(crate) fn emit_get_v_static_length(
        &mut self,
        key: &TypedExpr,
        receiver: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Option<I32Local> {
        let key_proof = StaticLengthKey::for_get_v_operand(key)?;
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
        function.instruction(&Instruction::Else);
        // One inline test proves and casts an actual Array.
        let other = self.open_frame(ControlFrameKind::Block, function);
        let array = schema.reserve_gc_local(function).initialize(
            receiver.sealed_reference_or_branch::<ArrayObject>(
                schema,
                self.local_branch_label(other),
                function,
            ),
            function,
        );
        self.emit_array_length_value(key_proof, &array, result.value(), function);
        result.set_normal(result.value(), function);
        handled.set_constant(1, function);
        array.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
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
        let Some(key) = StaticLengthKey::for_property_key(key) else {
            return Ok(false);
        };
        self.try_compile_string_length_consumer(target, key, output, function)
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
        let Some(key) = StaticLengthKey::for_get_v_operand(key) else {
            return Ok(false);
        };
        self.try_compile_string_length_consumer(target, key, output, function)
    }

    fn try_compile_string_length_consumer(
        &mut self,
        target: &TypedExpr,
        key: StaticLengthKey,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<bool, EmitError> {
        if let Some(plan) = ConcatLengthConsumer::new(target, key) {
            self.compile_concat_length_consumer(plan, output, function)?;
            return Ok(true);
        }
        self.try_compile_joined_string_length(target, output, function)
    }

    fn compile_concat_length_consumer(
        &mut self,
        plan: ConcatLengthConsumer<'_>,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if let Some(length) = literal_loop_concat_length(self, &plan) {
            output.set_scalar(
                ScalarValue::NumberBits((length as f64).to_bits() as i64),
                function,
            );
            return Ok(());
        }
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
        function.instruction(&Instruction::I64Const(MAX_STRING_LENGTH));
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
        if let Some(binding) = plan.fallback_binding {
            // The sole-use local consumer may elide its unobserved String,
            // but the fallback still initializes the real full JavaScript
            // value before ordinary property access or a getter can run.
            self.write_binding_from_locals(BindingStorage::Local(binding), added.value(), function);
        }
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
