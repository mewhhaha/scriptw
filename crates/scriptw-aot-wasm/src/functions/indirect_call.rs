use super::*;
use crate::gc_types::{FunctionContext, ValueLocals};

/// Indexed member-call callees per body that get the inline probe.
const INLINE_INDEXED_CALLEE_SITES: u32 = 32;

impl FunctionBuilder<'_> {
    pub(crate) fn emit_indirect_call(
        &mut self,
        callee_expression: &TypedExpr,
        this_expression: Option<&TypedExpr>,
        arguments: &[TypedExpr],
        static_regexp_compilation: Option<&StaticRegExpCompilation>,
        direct_eval: Option<&scriptw_ir::DirectEvalContextIr>,
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        if let Some(context) = direct_eval {
            return self.emit_direct_eval_call(
                context,
                callee_expression,
                this_expression,
                arguments,
                continuation,
                output,
                function,
            );
        }
        // Static regexp annotations have already populated the native program
        // cache during StringPool planning. Only the actual native constructor
        // may use that cache after it performs its own argument coercions.
        if matches!(continuation, CallContinuation::Continue)
            && arguments.is_empty()
            && static_regexp_compilation.is_none()
            && self
                .current_function_meta()
                .is_some_and(|meta| meta.protocol().class_kind() == ClassFunctionKind::Constructor)
        {
            if let (ExprIr::FunctionValue(id), Some(this_expression)) =
                (&callee_expression.expr, this_expression)
            {
                if matches!(this_expression.expr, ExprIr::This) {
                    if let Some(meta) = self.functions.get(id).cloned().filter(|meta| {
                        meta.class_element_execution_kind
                            == ClassElementExecutionKind::InstanceFieldInitializer
                    }) {
                        let schema = self.runtime_schema();
                        let context = schema
                            .reserve_gc_local::<FunctionContext, _>(function)
                            .initialize(
                                self.body_entry_locals()
                                    .expect("class initializer owns its body entry")
                                    .function_context()
                                    .expect("class initializer owns its captures")
                                    .load(schema, function),
                                function,
                            );
                        let receiver = schema.reserve_value_local(function);
                        self.compile_expr_to_value(this_expression, &receiver, function)?;
                        let result = schema.reserve_completion(function);
                        self.emit_direct_class_element_js_call(
                            &meta,
                            &context,
                            Some(&receiver),
                            &[],
                            &result,
                            function,
                        )?;
                        self.emit_propagate_pending_throw(&result, &[], function);
                        output.copy_from(result.value(), function);
                        result.clear(function);
                        receiver.clear(function);
                        context.clear(function);
                        return Ok(());
                    }
                }
            }
        }
        let schema = self.runtime_schema();
        let callee = schema.reserve_value_local(function);
        let receiver = schema.reserve_value_local(function);
        self.compile_callee_to_value(callee_expression, &callee, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        match this_expression {
            Some(expression) => {
                self.compile_expr_to_value(expression, &receiver, function)?;
                self.emit_propagate_current_throw_if_needed(function);
            }
            None => receiver.set_undefined(function),
        }
        let targets = callee_expression.function_targets.known_targets();
        let known_target = if targets.len() == 1 {
            targets
                .iter()
                .next()
                .and_then(|id| self.functions.get(id))
                .cloned()
        } else {
            None
        };
        // A constant RegExp site binds its compile-time program only when the
        // actual callee is this Realm's %RegExp%; any other callee takes the
        // original dispatch with the same (effect-free, literal) operands.
        // A tail continuation admits it too: the bound allocation runs no
        // user code, its value returns normally, and a miss keeps the proper
        // tail call.
        if let Some(site) = crate::expressions::ConstantRegExpConstructionSite::of(
            callee_expression,
            arguments,
            static_regexp_compilation,
        ) {
            let handled = self
                .emit_constant_regexp_construction_if_intrinsic(&callee, &site, output, function)?;
            handled.load(function);
            function.instruction(&Instruction::I32Eqz);
            self.open_frame(ControlFrameKind::If, function);
            self.emit_indirect_call_from_locals_with_known_target(
                &callee,
                Some(&receiver),
                arguments,
                known_target.as_ref(),
                continuation,
                output,
                function,
            )?;
            self.pop_control(ControlFrameKind::If);
            function.instruction(&Instruction::End);
            self.runtime_schema().release_i32_local(handled, function);
            receiver.clear(function);
            callee.clear(function);
            return Ok(());
        }
        if let Some(meta) = &known_target {
            if self.try_emit_known_leaf_call_from_locals(
                meta,
                &callee,
                &receiver,
                arguments,
                continuation,
                output,
                function,
            )? {
                receiver.clear(function);
                callee.clear(function);
                return Ok(());
            }
        }
        let packed_push_candidate = targets.iter().any(|id| {
            StandardBuiltinId::from_function_id(id) == Some(StandardBuiltinId::ArrayPrototypePush)
        });
        if packed_push_candidate
            && self.try_emit_packed_array_push_call(
                &callee,
                &receiver,
                arguments,
                continuation,
                output,
                function,
            )?
        {
            receiver.clear(function);
            callee.clear(function);
            return Ok(());
        }
        let packed_join_candidate = targets.iter().any(|id| {
            StandardBuiltinId::from_function_id(id) == Some(StandardBuiltinId::ArrayPrototypeJoin)
        });
        if packed_join_candidate
            && self.try_emit_packed_array_join_call(
                &callee,
                &receiver,
                arguments,
                continuation,
                output,
                function,
            )?
        {
            receiver.clear(function);
            callee.clear(function);
            return Ok(());
        }
        // Target knowledge or `x.imul(...)` syntax selects a candidate only.
        // The actual once-read callee and evaluated operands still prove the
        // guarded route at runtime. Syntax matters where the receiver has no
        // static facts, e.g. a module's `Math`, which resolves through the
        // live global environment rather than the script's global property
        // facts.
        let numeric_imul_candidate = callee_expression
            .function_targets
            .known_targets()
            .iter()
            .any(|id| StandardBuiltinId::from_function_id(id) == Some(StandardBuiltinId::MathImul))
            || callee_reads_static_property(callee_expression, "imul");
        let numeric_math_unary_candidate = [
            (StandardBuiltinId::MathFloor, "floor"),
            (StandardBuiltinId::MathCeil, "ceil"),
            (StandardBuiltinId::MathTrunc, "trunc"),
            (StandardBuiltinId::MathAbs, "abs"),
            (StandardBuiltinId::MathSqrt, "sqrt"),
        ]
        .into_iter()
        .find(|(builtin, name)| {
            callee_expression
                .function_targets
                .known_targets()
                .iter()
                .any(|id| StandardBuiltinId::from_function_id(id) == Some(*builtin))
                || callee_reads_static_property(callee_expression, name)
        })
        .map(|(builtin, _)| builtin);
        // `x.call(...)` syntax or target knowledge only nominates the
        // forwarding guard; the actual callee's code identity decides, with
        // these once-evaluated operands.
        let call_forwarding_candidate = this_expression.is_some()
            && (targets.iter().any(|id| {
                StandardBuiltinId::from_function_id(id)
                    == Some(StandardBuiltinId::FunctionPrototypeCall)
            }) || callee_reads_static_property(callee_expression, "call"));
        if (!numeric_imul_candidate
            || !self.try_emit_numeric_imul_call(
                &callee,
                &receiver,
                arguments,
                continuation,
                output,
                function,
            )?)
            && numeric_math_unary_candidate
                .map(|builtin| {
                    self.try_emit_numeric_math_unary_call(
                        &callee,
                        &receiver,
                        arguments,
                        builtin,
                        continuation,
                        output,
                        function,
                    )
                })
                .transpose()?
                != Some(true)
            && (!call_forwarding_candidate
                || !self.try_emit_function_call_forwarding(
                    &callee,
                    &receiver,
                    arguments,
                    continuation,
                    output,
                    function,
                )?)
        {
            self.emit_indirect_call_from_locals_with_known_target(
                &callee,
                Some(&receiver),
                arguments,
                known_target.as_ref(),
                continuation,
                output,
                function,
            )?;
        }
        receiver.clear(function);
        callee.clear(function);
        Ok(())
    }

    /// The callee of `base[key](...)` with a key that may be a Number: the
    /// base, then the key, then the same conversion-free own-data probe that
    /// a computed GetV read (`rows[i & 63]`) uses, which answers a present
    /// packed element of an actual Array without ToObject, ToPropertyKey or
    /// a call. Every other case, a miss of any of its guards included, takes
    /// the complete Get with the same evaluated base and raw key, exactly as
    /// the read would on its own. The base value is the call's `this`
    /// through the call's own materialized receiver binding.
    fn compile_callee_to_value(
        &mut self,
        callee_expression: &TypedExpr,
        callee: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        // The probe is about 60 Wasm instructions per site: past the first
        // INLINE_INDEXED_CALLEE_SITES sites of a body the extra native code
        // costs more in instruction-cache misses than the probe saves.
        let Some(read) = IndexedCalleeRead::of(callee_expression)
            .filter(|_| self.inline_indexed_callee_sites < INLINE_INDEXED_CALLEE_SITES)
        else {
            return self.compile_expr_to_value(callee_expression, callee, function);
        };
        self.inline_indexed_callee_sites += 1;
        // A projected `arguments` read keeps its own route.
        if self.compile_projected_arguments_read(callee_expression, callee, function)? {
            return Ok(());
        }
        let schema = self.runtime_schema();
        let base = schema.reserve_value_local(function);
        self.compile_expr_to_value(read.base, &base, function)?;
        let key = schema.reserve_value_local(function);
        let pending = schema.reserve_completion(function);
        // An int32-domain key probes with its bits; its Number is
        // materialized only for the complete Get on a miss.
        let (found, key_bits) = match crate::operations::Int32Expression::of(read.key) {
            Some(value) => {
                let bits = schema.reserve_i32_local(function);
                self.compile_int32_bits(&value, function)?;
                bits.store(function);
                let signedness = value.signedness();
                let found = self
                    .emit_int32_array_own_data_read(&base, bits, signedness, &pending, function);
                (found, Some((bits, signedness)))
            }
            None => {
                self.compile_expr_to_value(read.key, &key, function)?;
                let found =
                    self.emit_raw_numeric_array_own_data_read(&base, &key, &pending, function);
                (found, None)
            }
        };
        found.load(function);
        schema.release_i32_local(found, function);
        function.instruction(&Instruction::I32Eqz);
        self.open_frame(ControlFrameKind::If, function);
        if let Some((bits, signedness)) = key_bits {
            bits.load(function);
            signedness.emit_to_f64(function);
            function.instruction(&Instruction::I64ReinterpretF64);
            key.scalar().store(function);
            key.set_number(key.scalar(), function);
        }
        schema
            .call_helper(
                crate::runtime_helpers::DynamicPropertyReadArguments::new(
                    &base,
                    &base,
                    &key,
                    self.current_environment(),
                    self.current_program(),
                ),
                self.runtime_helper_base()?,
                function,
            )
            .store(&pending, function);
        self.pop_control(ControlFrameKind::If);
        function.instruction(&Instruction::End);
        if let Some((bits, _)) = key_bits {
            schema.release_i32_local(bits, function);
        }
        key.clear(function);
        base.clear(function);
        self.emit_propagate_pending_throw(&pending, &[pending.value()], function);
        callee.copy_from(pending.value(), function);
        pending.clear(function);
        Ok(())
    }
}

/// `base[key]` as a callee, where `key` is a computed key that may be a
/// Number and `base` may be an Array: the only reads the raw-key probe can
/// answer. Lowering gives a constant index (`steps[0]`) this form.
struct IndexedCalleeRead<'a> {
    base: &'a TypedExpr,
    key: &'a TypedExpr,
}

impl<'a> IndexedCalleeRead<'a> {
    fn of(callee: &'a TypedExpr) -> Option<Self> {
        let ExprIr::PropertyRead {
            target,
            key: PropertyKeyIr::ArrayIndex(key) | PropertyKeyIr::StringExpr(key),
        } = &callee.expr
        else {
            return None;
        };
        (target.possible_kinds.contains(ValueKind::Array)
            && key.possible_kinds.contains(ValueKind::Number))
        .then_some(Self { base: target, key })
    }
}

/// The callee is a `Get` of the static property `name`. Syntax only nominates
/// a guarded native route; it never proves the callee.
fn callee_reads_static_property(callee: &TypedExpr, name: &str) -> bool {
    match &callee.expr {
        ExprIr::PropertyRead {
            key: PropertyKeyIr::StaticString(key),
            ..
        } => key == name,
        ExprIr::SpecOperation {
            operation: scriptw_ir::SpecOperationIr::GetV,
            operands,
        } => matches!(
            operands.get(1).map(|key| &key.expr),
            Some(ExprIr::String(key)) if key == name
        ),
        _ => false,
    }
}
