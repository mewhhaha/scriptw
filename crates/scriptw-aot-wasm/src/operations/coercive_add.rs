use super::*;
use crate::runtime_helpers::{CoerciveAddArguments, CoerciveAddParameters, HelperParameters};

#[derive(Clone, Copy)]
enum StringAddSide {
    Left,
    Right,
}

/// An addition whose syntax expects a String operand. The expectation
/// selects the emitted test and nothing else: the test itself reads the live
/// tags, so a wrong expectation only reaches the general kernel.
enum StringAddSite<'a> {
    /// One operand is the empty String literal, which contributes no code
    /// units: the other operand's String is the whole result.
    Empty { literal: StringAddSide },
    /// One operand is expected to be a String.
    Expected { side: StringAddSide },
    /// `(a + b) + c` whose inner addition is an `Expected` site: three
    /// Strings concatenate into one result without the intermediate one.
    Chain {
        inner_lhs: &'a TypedExpr,
        inner_rhs: &'a TypedExpr,
        inner_side: StringAddSide,
    },
}

impl<'a> StringAddSite<'a> {
    fn for_operands(lhs: &'a TypedExpr, rhs: &'a TypedExpr) -> Option<Self> {
        let string = |operand: &TypedExpr| operand.kind == ValueKind::String;
        let empty =
            |operand: &TypedExpr| matches!(&operand.expr, ExprIr::String(text) if text.is_empty());
        if empty(lhs) {
            return Some(Self::Empty {
                literal: StringAddSide::Left,
            });
        }
        if empty(rhs) {
            return Some(Self::Empty {
                literal: StringAddSide::Right,
            });
        }
        if string(lhs) {
            if let ExprIr::StringConcat {
                lhs: inner_lhs,
                rhs: inner_rhs,
            }
            | ExprIr::CoerciveAdd {
                lhs: inner_lhs,
                rhs: inner_rhs,
            } = &lhs.expr
            {
                if let Some(StringAddSite::Expected { side }) =
                    StringAddSite::for_operands(inner_lhs, inner_rhs)
                {
                    return Some(Self::Chain {
                        inner_lhs,
                        inner_rhs,
                        inner_side: side,
                    });
                }
            }
            return Some(Self::Expected {
                side: StringAddSide::Left,
            });
        }
        string(rhs).then_some(Self::Expected {
            side: StringAddSide::Right,
        })
    }
}

impl FunctionBuilder<'_> {
    /// Evaluation stays in P; non-Number pairs retain R's complete coercion owner.
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
        match StringAddSite::for_operands(lhs, rhs) {
            Some(site) => {
                self.compile_string_add_site(site, lhs, rhs, &left, &right, &result, function)?
            }
            None => {
                // Both GetValue operations complete before the first
                // ToPrimitive call.
                self.compile_expr_to_value(lhs, &left, function)?;
                self.compile_expr_to_value(rhs, &right, function)?;
                self.emit_coercive_add_completion_from_values(&left, &right, &result, function)?;
            }
        }
        right.clear(function);
        left.clear(function);
        self.finish_coercive_add_completion(&result, function)?;
        output.copy_from(result.value(), function);
        result.clear(function);
        Ok(())
    }

    /// Borrow once-evaluated rooted operands. The caller releases its roots
    /// before routing abrupt completion. Actual Number pairs need no coercion;
    /// all other pairs retain R's existing observable addition owner.
    pub(crate) fn emit_coercive_add_completion_from_values(
        &mut self,
        left: &ValueLocals,
        right: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        left.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        function.instruction(&Instruction::I32Eq);
        right.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        function.instruction(&Instruction::I32Eq);
        function.instruction(&Instruction::I32And);
        self.open_frame(ControlFrameKind::If, function);
        left.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        right.scalar().load(function);
        function.instruction(&Instruction::F64ReinterpretI64);
        function.instruction(&Instruction::F64Add);
        function.instruction(&Instruction::I64ReinterpretF64);
        result.value().scalar().store(function);
        result.value().set_number(result.value().scalar(), function);
        result.set_normal(result.value(), function);
        function.instruction(&Instruction::Else);
        // Operands that are not both Numbers go to R's complete addition.
        function.rarely_reached();
        let realm = self.emit_execution_realm(function);
        self.runtime_schema()
            .call_helper(
                CoerciveAddArguments::new(
                    left,
                    right,
                    &realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        realm.clear(function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    /// A site whose syntax expects a String operand. The emitted test reads
    /// the live tags: with a String on the expected side and a String or a
    /// Number on the other, ToPrimitive returns both operands unchanged and
    /// calls nothing, ToString of the Number is Number::toString, and the
    /// result is the concatenation. Every other pair of tags, the Number
    /// pair included, enters the general kernel with the operands as they
    /// were evaluated.
    #[allow(clippy::too_many_arguments)]
    fn compile_string_add_site(
        &mut self,
        site: StringAddSite<'_>,
        lhs: &TypedExpr,
        rhs: &TypedExpr,
        left: &ValueLocals,
        right: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        match site {
            StringAddSite::Empty { literal } => {
                // The literal has no evaluation to order against the other
                // operand; only the general kernel needs its value.
                let (literal_operand, literal_value, other_operand, other_value) = match literal {
                    StringAddSide::Left => (lhs, left, rhs, right),
                    StringAddSide::Right => (rhs, right, lhs, left),
                };
                self.compile_expr_to_value(other_operand, other_value, function)?;
                let done = self.open_frame(ControlFrameKind::Block, function);
                let general = self.open_frame(ControlFrameKind::Block, function);
                let text = schema
                    .reserve_gc_local::<StringValue, crate::gc_types::Nullable>(function)
                    .initialize_null(schema, function);
                self.emit_string_add_operand_or_branch(other_value, &text, general, function)?;
                self.emit_string_add_result(&text, result, function);
                text.clear(function);
                self.emit_branch_to_target(done, function);
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                self.compile_expr_to_value(literal_operand, literal_value, function)?;
                self.emit_coercive_add_kernel_call(left, right, result, function)?;
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
            }
            StringAddSite::Expected { side } => {
                // Both GetValue operations complete before either conversion.
                self.compile_expr_to_value(lhs, left, function)?;
                self.compile_expr_to_value(rhs, right, function)?;
                let done = self.open_frame(ControlFrameKind::Block, function);
                let general = self.open_frame(ControlFrameKind::Block, function);
                if matches!(side, StringAddSide::Left) {
                    // Direct fused String + Number helper avoids allocating an
                    // intermediate decimal StringValue and checking bounds twice.
                    left.tag().load(function);
                    function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
                    function.instruction(&Instruction::I32Eq);
                    right.tag().load(function);
                    function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
                    function.instruction(&Instruction::I32Eq);
                    function.instruction(&Instruction::I32And);
                    self.open_frame(ControlFrameKind::If, function);
                    let prefix = schema.reserve_gc_local(function).initialize(
                        left.cast_reference::<StringValue>(schema, function),
                        function,
                    );
                    let fused = schema
                        .reserve_gc_local::<StringValue, crate::gc_types::Nullable>(function)
                        .initialize(
                            schema.helper_reference_on_stack(schema.call_helper(
                                crate::runtime_helpers::StringConcatStringNumberArguments::new(
                                    &prefix,
                                    right.scalar(),
                                ),
                                self.runtime_helper_base()?,
                                function,
                            )),
                            function,
                        );
                    prefix.clear(function);
                    fused.load(schema, function).is_null(function);
                    self.emit_branch_if_to_target(general, function);
                    self.emit_string_add_result(&fused, result, function);
                    fused.clear(function);
                    self.emit_branch_to_target(done, function);
                    self.pop_control(ControlFrameKind::If);
                    function.instruction(&Instruction::End);
                }
                let first = schema
                    .reserve_gc_local::<StringValue, crate::gc_types::Nullable>(function)
                    .initialize_null(schema, function);
                let second = schema
                    .reserve_gc_local::<StringValue, crate::gc_types::Nullable>(function)
                    .initialize_null(schema, function);
                self.emit_string_add_pair_or_branch(
                    side, left, right, &first, &second, general, function,
                )?;
                // An over-long result is the general kernel's RangeError.
                self.emit_string_parts_exceed_limit_i32(&[&first, &second], function);
                self.emit_branch_if_to_target(general, function);
                let text = schema
                    .reserve_gc_local::<StringValue, crate::gc_types::Nullable>(function)
                    .initialize(
                        self.emit_string_concat_call(&[&first, &second], function)?
                            .nullable(),
                        function,
                    );
                self.emit_string_add_result(&text, result, function);
                text.clear(function);
                second.clear(function);
                first.clear(function);
                self.emit_branch_to_target(done, function);
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                self.emit_coercive_add_kernel_call(left, right, result, function)?;
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
            }
            StringAddSite::Chain {
                inner_lhs,
                inner_rhs,
                inner_side,
            } => {
                let inner_left = schema.reserve_value_local(function);
                let inner_right = schema.reserve_value_local(function);
                let first = schema
                    .reserve_gc_local::<StringValue, crate::gc_types::Nullable>(function)
                    .initialize_null(schema, function);
                let second = schema
                    .reserve_gc_local::<StringValue, crate::gc_types::Nullable>(function)
                    .initialize_null(schema, function);
                // The inner addition completes before the third operand is
                // evaluated. Its admitted conversions call nothing, so their
                // Strings wait for the third operand; otherwise its general
                // kernel leaves the inner result in `left` and both Strings
                // null.
                self.compile_expr_to_value(inner_lhs, &inner_left, function)?;
                self.compile_expr_to_value(inner_rhs, &inner_right, function)?;
                let inner_done = self.open_frame(ControlFrameKind::Block, function);
                let inner_general = self.open_frame(ControlFrameKind::Block, function);
                self.emit_string_add_pair_or_branch(
                    inner_side,
                    &inner_left,
                    &inner_right,
                    &first,
                    &second,
                    inner_general,
                    function,
                )?;
                // The inner addition throws its RangeError before the third
                // operand is evaluated: through its own general kernel.
                self.emit_string_parts_exceed_limit_i32(&[&first, &second], function);
                self.emit_branch_if_to_target(inner_general, function);
                self.emit_branch_to_target(inner_done, function);
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                self.emit_coercive_add_kernel_call(&inner_left, &inner_right, result, function)?;
                inner_right.reference().clear(function);
                inner_left.reference().clear(function);
                self.finish_coercive_add_completion(result, function)?;
                left.copy_from(result.value(), function);
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                inner_right.clear(function);
                inner_left.clear(function);
                self.compile_expr_to_value(rhs, right, function)?;
                let done = self.open_frame(ControlFrameKind::Block, function);
                let general = self.open_frame(ControlFrameKind::Block, function);
                first.load(schema, function).is_null(function);
                self.open_frame(ControlFrameKind::If, function);
                left.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
                function.instruction(&Instruction::I32Ne);
                self.emit_branch_if_to_target(general, function);
                first.replace(
                    left.cast_reference::<StringValue>(schema, function)
                        .nullable(),
                    function,
                );
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                let third = schema
                    .reserve_gc_local::<StringValue, crate::gc_types::Nullable>(function)
                    .initialize_null(schema, function);
                let text = schema
                    .reserve_gc_local::<StringValue, crate::gc_types::Nullable>(function)
                    .initialize_null(schema, function);
                right.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                third.replace(
                    right
                        .cast_reference::<StringValue>(schema, function)
                        .nullable(),
                    function,
                );
                function.instruction(&Instruction::Else);
                right.tag().load(function);
                function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
                function.instruction(&Instruction::I32Eq);
                self.open_frame(ControlFrameKind::If, function);
                // ToString(Number) is Number::toString, which cannot throw.
                third.replace(
                    self.emit_number_to_string_payload(right.scalar(), function)?
                        .nullable(),
                    function,
                );
                function.instruction(&Instruction::Else);
                // The kernel takes the inner result as its left operand.
                second.load(schema, function).is_null(function);
                function.instruction(&Instruction::I32Eqz);
                self.open_frame(ControlFrameKind::If, function);
                text.replace(
                    self.emit_string_concat_call(&[&first, &second], function)?
                        .nullable(),
                    function,
                );
                let inner = schema.reserve_gc_local(function).initialize(
                    text.load(schema, function).require_non_null(function),
                    function,
                );
                left.set_reference(&inner, schema, function);
                inner.clear(function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.emit_branch_to_target(general, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                // An over-long whole result: every conversion is complete and
                // effect-free, so throw the general kernel's RangeError here
                // rather than materialize the inner sum for it.
                self.emit_string_parts_exceed_limit_i32(&[&first, &second, &third], function);
                self.open_frame(ControlFrameKind::If, function);
                self.emit_throw_runtime_error(
                    scriptw_ir::NativeErrorKind::RangeError,
                    RuntimeErrorMessage::STRING_LENGTH_EXCEEDS_IMPLEMENTATION_LIMIT,
                    result,
                    function,
                )?;
                self.emit_branch_to_target(done, function);
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                second.load(schema, function).is_null(function);
                self.open_frame(ControlFrameKind::If, function);
                text.replace(
                    self.emit_string_concat_call(&[&first, &third], function)?
                        .nullable(),
                    function,
                );
                function.instruction(&Instruction::Else);
                text.replace(
                    self.emit_string_concat_call(&[&first, &second, &third], function)?
                        .nullable(),
                    function,
                );
                self.pop_control(ControlFrameKind::If);
                function.instruction(&Instruction::End);
                self.emit_string_add_result(&text, result, function);
                text.clear(function);
                third.clear(function);
                self.emit_branch_to_target(done, function);
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                self.emit_coercive_add_kernel_call(left, right, result, function)?;
                self.pop_control(ControlFrameKind::Block);
                function.instruction(&Instruction::End);
                second.clear(function);
                first.clear(function);
            }
        }
        Ok(())
    }

    /// The Strings of an `Expected` pair in source order, or a branch to
    /// `general` before either is assigned: the expected side must be a
    /// String, the other a String or a Number.
    #[allow(clippy::too_many_arguments)]
    fn emit_string_add_pair_or_branch(
        &mut self,
        side: StringAddSide,
        left: &ValueLocals,
        right: &ValueLocals,
        first: &GcLocal<StringValue, crate::gc_types::Nullable>,
        second: &GcLocal<StringValue, crate::gc_types::Nullable>,
        general: crate::emit::ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let (expected, expected_text, other, other_text) = match side {
            StringAddSide::Left => (left, first, right, second),
            StringAddSide::Right => (right, second, left, first),
        };
        expected.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(general, function);
        self.emit_string_add_operand_or_branch(other, other_text, general, function)?;
        expected_text.replace(
            expected
                .cast_reference::<StringValue>(schema, function)
                .nullable(),
            function,
        );
        Ok(())
    }

    /// The String an operand contributes when it is a String or a Number;
    /// every other operand branches to `general` with `text` unassigned.
    fn emit_string_add_operand_or_branch(
        &mut self,
        value: &ValueLocals,
        text: &GcLocal<StringValue, crate::gc_types::Nullable>,
        general: crate::emit::ControlTarget,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::String as i32));
        function.instruction(&Instruction::I32Eq);
        self.open_frame(ControlFrameKind::If, function);
        text.replace(
            value
                .cast_reference::<StringValue>(schema, function)
                .nullable(),
            function,
        );
        function.instruction(&Instruction::Else);
        value.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
        function.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(general, function);
        // ToString(Number) is Number::toString, which cannot throw.
        text.replace(
            self.emit_number_to_string_payload(value.scalar(), function)?
                .nullable(),
            function,
        );
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        Ok(())
    }

    fn emit_string_add_result(
        &mut self,
        text: &GcLocal<StringValue, crate::gc_types::Nullable>,
        result: &CompletionLocals,
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let string = schema.reserve_gc_local(function).initialize(
            text.load(schema, function).require_non_null(function),
            function,
        );
        result.value().set_reference(&string, schema, function);
        string.clear(function);
        result.set_normal(result.value(), function);
    }

    /// Leaves 1 when `parts` (a null part is absent) hold more than
    /// `MAX_STRING_LENGTH` code units together.
    fn emit_string_parts_exceed_limit_i32(
        &self,
        parts: &[&GcLocal<StringValue, crate::gc_types::Nullable>],
        function: &mut Function,
    ) {
        let schema = self.runtime_schema();
        let total = schema.reserve_i64_local(function);
        let length = schema.reserve_i64_local(function);
        function.instruction(&Instruction::I64Const(0));
        total.store(function);
        for part in parts {
            part.load(schema, function).is_null(function);
            function.instruction(&Instruction::I32Eqz);
            function.instruction(&Instruction::If(BlockType::Empty));
            let string = schema.reserve_gc_local(function).initialize(
                part.load(schema, function).require_non_null(function),
                function,
            );
            self.emit_native_gc_string_length(&string, length, function);
            string.clear(function);
            total.load(function);
            length.load(function);
            function.instruction(&Instruction::I64Add);
            total.store(function);
            function.instruction(&Instruction::End);
        }
        total.load(function);
        function.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
        function.instruction(&Instruction::I64GtU);
        schema.release_i64_local(length, function);
        schema.release_i64_local(total, function);
    }

    /// One String holding the code units of two or three Strings in order.
    fn emit_string_concat_call<const N: usize>(
        &mut self,
        parts: &[&GcLocal<StringValue, crate::gc_types::Nullable>; N],
        function: &mut Function,
    ) -> Result<crate::gc_types::GcStackReference<StringValue>, EmitError> {
        let schema = self.runtime_schema();
        let base = self.runtime_helper_base()?;
        let strings = parts.map(|part| {
            schema.reserve_gc_local(function).initialize(
                part.load(schema, function).require_non_null(function),
                function,
            )
        });
        let result = match &strings[..] {
            [left, right] => schema.helper_reference_on_stack(schema.call_helper(
                crate::runtime_helpers::StringConcatArguments::new(left, right),
                base,
                function,
            )),
            [first, second, third] => schema.helper_reference_on_stack(schema.call_helper(
                crate::runtime_helpers::StringConcatThreeArguments::new(first, second, third),
                base,
                function,
            )),
            _ => unreachable!("two or three Strings concatenate"),
        };
        for string in strings {
            string.clear(function);
        }
        Ok(result)
    }

    fn emit_coercive_add_kernel_call(
        &mut self,
        left: &ValueLocals,
        right: &ValueLocals,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let realm = self.emit_execution_realm(function);
        self.runtime_schema()
            .call_helper(
                CoerciveAddArguments::new(
                    left,
                    right,
                    &realm,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(result, function);
        realm.clear(function);
        Ok(())
    }

    /// Concatenation of two Strings: one allocation, no conversion.
    pub(crate) fn compile_string_concat_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::StringConcat);
        let parameters =
            self.helper_parameters::<crate::runtime_helpers::StringConcatParameters>(&mut function);
        let result =
            self.emit_concat_gc_strings(&parameters.left, &parameters.right, &mut function);
        parameters.release(&mut function);
        let _result = result;
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// Concatenation of three Strings: one allocation, no intermediate.
    pub(crate) fn compile_string_concat_three_helper(&mut self) -> Result<Function, EmitError> {
        let mut function = self.begin_helper_body(RuntimeHelperId::StringConcatThree);
        let parameters = self
            .helper_parameters::<crate::runtime_helpers::StringConcatThreeParameters>(
                &mut function,
            );
        let result = self.emit_concat_three_gc_strings(
            [&parameters.first, &parameters.second, &parameters.third],
            &mut function,
        );
        parameters.release(&mut function);
        let _result = result;
        function.instruction(&Instruction::End);
        Ok(self.finish_function(function))
    }

    /// An empty prefix returns the Number's String directly. A nonempty
    /// prefix and a u32 are written into one String, with null signaling
    /// that the combined length exceeds `MAX_STRING_LENGTH`.
    pub(crate) fn compile_string_concat_string_number_helper(
        &mut self,
    ) -> Result<Function, EmitError> {
        let mut f = self.begin_helper_body(RuntimeHelperId::StringConcatStringNumber);
        let p = self
            .helper_parameters::<crate::runtime_helpers::StringConcatStringNumberParameters>(
                &mut f,
            );
        let s = self.runtime_schema();
        let output = s
            .reserve_gc_local::<StringValue, crate::gc_types::Nullable>(&mut f)
            .initialize_null(s, &mut f);
        let left_units = s.reserve_gc_local(&mut f).initialize(
            s.field(StringValueSchema::CODE_UNITS)
                .read(&p.left, s, &mut f)
                .reference(),
            &mut f,
        );
        let left_len = s.reserve_i32_local(&mut f);
        s.array_type::<CodeUnitArray>()
            .length(&left_units, s, &mut f);
        left_len.store(&mut f);

        let done = self.open_frame(ControlFrameKind::Block, &mut f);
        left_len.load(&mut f);
        f.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, &mut f);
        output.replace(
            self.emit_number_to_string_payload(p.right_bits, &mut f)?
                .nullable(),
            &mut f,
        );
        self.emit_branch_to_target(done, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);

        let small = s.reserve_i32_local(&mut f);
        p.right_bits.load(&mut f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::I32TruncSatF64U);
        small.store(&mut f);
        small.load(&mut f);
        f.instruction(&Instruction::F64ConvertI32U);
        p.right_bits.load(&mut f);
        f.instruction(&Instruction::F64ReinterpretI64);
        f.instruction(&Instruction::F64Eq);
        self.open_frame(ControlFrameKind::If, &mut f);
        let digits = s.reserve_i32_local(&mut f);
        f.instruction(&Instruction::I32Const(1));
        for threshold in [
            10u32,
            100,
            1_000,
            10_000,
            100_000,
            1_000_000,
            10_000_000,
            100_000_000,
            1_000_000_000,
        ] {
            small.load(&mut f);
            f.instruction(&Instruction::I32Const(threshold as i32));
            f.instruction(&Instruction::I32GeU);
            f.instruction(&Instruction::I32Add);
        }
        digits.store(&mut f);
        let total64 = s.reserve_i64_local(&mut f);
        left_len.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        digits.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        f.instruction(&Instruction::I64Add);
        total64.store(&mut f);
        total64.load(&mut f);
        f.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
        f.instruction(&Instruction::I64LeU);
        self.open_frame(ControlFrameKind::If, &mut f);
        let total = s.reserve_i32_local(&mut f);
        let cursor = s.reserve_i32_local(&mut f);
        let rem = s.reserve_i32_local(&mut f);
        let quot = s.reserve_i32_local(&mut f);
        let unit = s.reserve_i32_local(&mut f);
        total64.load(&mut f);
        f.instruction(&Instruction::I32WrapI64);
        total.store(&mut f);
        let text = crate::gc_types::StringConstruction::allocate(
            s,
            s.reserve_gc_local(&mut f),
            total,
            &mut f,
        );
        cursor.set_constant(0, &mut f);
        text.copy_concatenated_units_from(cursor, &left_units, left_len, s, &mut f);
        small.load(&mut f);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32LtU);
        self.open_frame(ControlFrameKind::If, &mut f);
        small.load(&mut f);
        f.instruction(&Instruction::I32Const(48));
        f.instruction(&Instruction::I32Add);
        unit.store(&mut f);
        text.write(left_len, unit, s, &mut f);
        f.instruction(&Instruction::Else);
        total.load(&mut f);
        cursor.store(&mut f);
        small.load(&mut f);
        rem.store(&mut f);
        let digit_loop = self.open_frame(ControlFrameKind::Loop, &mut f);
        cursor.load(&mut f);
        f.instruction(&Instruction::I32Const(1));
        f.instruction(&Instruction::I32Sub);
        cursor.store(&mut f);
        rem.load(&mut f);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32DivU);
        quot.store(&mut f);
        rem.load(&mut f);
        quot.load(&mut f);
        f.instruction(&Instruction::I32Const(10));
        f.instruction(&Instruction::I32Mul);
        f.instruction(&Instruction::I32Sub);
        f.instruction(&Instruction::I32Const(48));
        f.instruction(&Instruction::I32Add);
        unit.store(&mut f);
        text.write(cursor, unit, s, &mut f);
        quot.load(&mut f);
        rem.store(&mut f);
        cursor.load(&mut f);
        left_len.load(&mut f);
        f.instruction(&Instruction::I32Ne);
        self.emit_branch_if_to_target(digit_loop, &mut f);
        self.pop_control(ControlFrameKind::Loop);
        f.instruction(&Instruction::End);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        output.replace(text.publish(s, &mut f).nullable(), &mut f);
        s.release_i32_local(unit, &mut f);
        s.release_i32_local(quot, &mut f);
        s.release_i32_local(rem, &mut f);
        s.release_i32_local(cursor, &mut f);
        s.release_i32_local(total, &mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(total64, &mut f);
        s.release_i32_local(digits, &mut f);
        f.instruction(&Instruction::Else);
        let right_text = s.reserve_gc_local(&mut f).initialize(
            self.emit_number_to_string_payload(p.right_bits, &mut f)?,
            &mut f,
        );
        let right_len = s.reserve_i64_local(&mut f);
        self.emit_native_gc_string_length(&right_text, right_len, &mut f);
        left_len.load(&mut f);
        f.instruction(&Instruction::I64ExtendI32U);
        right_len.load(&mut f);
        f.instruction(&Instruction::I64Add);
        f.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
        f.instruction(&Instruction::I64LeU);
        self.open_frame(ControlFrameKind::If, &mut f);
        output.replace(
            self.emit_concat_gc_strings(&p.left, &right_text, &mut f)
                .nullable(),
            &mut f,
        );
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i64_local(right_len, &mut f);
        right_text.clear(&mut f);
        self.pop_control(ControlFrameKind::If);
        f.instruction(&Instruction::End);
        s.release_i32_local(small, &mut f);
        self.pop_control(ControlFrameKind::Block);
        f.instruction(&Instruction::End);
        s.release_i32_local(left_len, &mut f);
        left_units.clear(&mut f);
        output.load(s, &mut f);
        output.clear(&mut f);
        p.release(&mut f);
        f.instruction(&Instruction::End);
        Ok(self.finish_function(f))
    }

    pub(crate) fn finish_coercive_add_completion(
        &mut self,
        result: &CompletionLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        self.finish_to_primitive_operation(ToPrimitiveAbruptRoute::ActiveHandler, result, function)
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
            function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Number as i32));
            function.instruction(&Instruction::I32Eq);
            self.open_frame(ControlFrameKind::If, function);
            // ToString(Number) is Number::toString, which cannot throw.
            let text = schema.reserve_gc_local(function).initialize(
                self.emit_number_to_string_payload(value.scalar(), function)?,
                function,
            );
            value.set_reference(&text, schema, function);
            text.clear(function);
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
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
        // Both conversions are complete; only the result's length is left.
        let lengths = schema.reserve_i64_local(function);
        let length = schema.reserve_i64_local(function);
        self.emit_native_gc_string_length(&left_string, lengths, function);
        self.emit_native_gc_string_length(&right_string, length, function);
        lengths.load(function);
        length.load(function);
        function.instruction(&Instruction::I64Add);
        function.instruction(&Instruction::I64Const(crate::gc_types::MAX_STRING_LENGTH));
        function.instruction(&Instruction::I64GtU);
        schema.release_i64_local(length, function);
        schema.release_i64_local(lengths, function);
        self.open_frame(ControlFrameKind::If, function);
        self.emit_throw_runtime_error(
            scriptw_ir::NativeErrorKind::RangeError,
            RuntimeErrorMessage::STRING_LENGTH_EXCEEDS_IMPLEMENTATION_LIMIT,
            &pending,
            function,
        )?;
        self.finish_to_primitive_operation(
            ToPrimitiveAbruptRoute::ActiveHandler,
            &pending,
            function,
        )?;
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
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
