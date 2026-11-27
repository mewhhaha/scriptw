use super::*;
use crate::gc_types::{
    ExecutableCodeSchema, FunctionContext, FunctionContextSchema, FunctionObject,
    FunctionObjectSchema, GcNullability, SourceDepthOverflow, ValueLocals,
};
use scriptw_ir::{SmallInlineBodyIr, SmallInlineEffects, SmallInlineInitializationIr};

/// Indexed member-call callees per body that get the inline probe.
const INLINE_INDEXED_CALLEE_SITES: u32 = 32;
/// Source bodies and indexed callee probes share the per-body code-size budget.
const INLINE_SMALL_SOURCE_SITES: u32 = 8;
const INLINE_SMALL_SOURCE_MAX_ARGUMENTS: usize = 6;

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
            if let Some(plan) = meta
                .constant_leaf
                .as_ref()
                .and_then(|plan| plan.small_source())
            {
                if matches!(callee_expression.expr, ExprIr::Identifier(_))
                    && this_expression.is_none()
                    && callee_expression
                        .function_targets
                        .proves_closed_local_call()
                    && (plan.effects() == SmallInlineEffects::Closed
                        || (self.planned_entry().is_some_and(|entry| {
                            matches!(
                                entry.source_body_owner(),
                                Some(crate::function_layout::SourceBodyOwner::Program)
                            )
                        }) && (matches!(continuation, CallContinuation::Continue)
                            || (self.strict && self.source_activation.is_some()))))
                    && self.inline_indexed_callee_sites < INLINE_SMALL_SOURCE_SITES
                    && arguments.len() <= INLINE_SMALL_SOURCE_MAX_ARGUMENTS
                    && arguments.iter().all(|argument| {
                        !matches!(
                            argument.expr,
                            ExprIr::SpreadArgument(_) | ExprIr::CapturedArgumentList(_)
                        )
                    })
                {
                    self.inline_indexed_callee_sites += 1;
                    self.emit_small_source_call(
                        meta,
                        plan,
                        &callee,
                        &receiver,
                        arguments,
                        continuation,
                        output,
                        function,
                    )?;
                    receiver.clear(function);
                    callee.clear(function);
                    if plan.effects() == SmallInlineEffects::StrictActivation {
                        self.emit_propagate_current_throw_if_needed(function);
                    }
                    return Ok(());
                }
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
            targets
                .iter()
                .any(|id| StandardBuiltinId::from_function_id(id) == Some(*builtin))
                || callee_reads_static_property(callee_expression, name)
        })
        .map(|(builtin, _)| builtin);
        let numeric_math_binary_candidate = [
            (StandardBuiltinId::MathMin, "min"),
            (StandardBuiltinId::MathMax, "max"),
        ]
        .into_iter()
        .find(|(builtin, name)| {
            targets
                .iter()
                .any(|id| StandardBuiltinId::from_function_id(id) == Some(*builtin))
                || callee_reads_static_property(callee_expression, name)
        })
        .map(|(builtin, _)| builtin);
        let string_constructor_candidate =
            targets.iter().any(|id| {
                StandardBuiltinId::from_function_id(id)
                    == Some(StandardBuiltinId::StringConstructor)
            }) || callee_nominates_identifier(callee_expression, "String");
        let number_constructor_candidate =
            targets.iter().any(|id| {
                StandardBuiltinId::from_function_id(id)
                    == Some(StandardBuiltinId::NumberConstructor)
            }) || callee_nominates_identifier(callee_expression, "Number");
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
            && numeric_math_binary_candidate
                .map(|builtin| {
                    self.try_emit_numeric_math_binary_call(
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
            && (!string_constructor_candidate
                || !self.try_emit_string_constructor_call(
                    &callee,
                    &receiver,
                    arguments,
                    continuation,
                    output,
                    function,
                )?)
            && (!number_constructor_candidate
                || !self.try_emit_number_constructor_call(
                    &callee,
                    &receiver,
                    arguments,
                    continuation,
                    output,
                    function,
                )?)
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

    /// Hold the actual operands across the identity, Realm and depth checks
    /// so the same arguments reach either arm. Observable strict activations
    /// still own their source depth and caller identity across nested effects.
    #[allow(clippy::too_many_arguments)]
    fn emit_small_source_call(
        &mut self,
        meta: &WasmFunctionMeta,
        plan: &SmallInlineBodyIr,
        callee: &ValueLocals,
        receiver: &ValueLocals,
        arguments: &[TypedExpr],
        continuation: &CallContinuation,
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let mut values = Vec::with_capacity(arguments.len());
        for argument in arguments {
            let value = schema.reserve_value_local(function);
            self.compile_expr_to_value(argument, &value, function)?;
            self.emit_propagate_current_throw_if_needed(function);
            values.push(value);
        }

        let done = self.open_frame(ControlFrameKind::Block, function);
        let miss = self.open_frame(ControlFrameKind::Block, function);
        callee.tag().load(function);
        function.instruction(&Instruction::I32Const(WasmRuntimeValueTag::Function.tag()));
        function.instruction(&Instruction::I32Eq);
        callee.reference().load(function);
        function.instruction(&Instruction::RefTestNonNull(
            schema
                .reference_type::<FunctionObject>(GcNullability::NonNullable)
                .heap_type,
        ));
        function.instruction(&Instruction::I32And);
        schema.emit_source_activation_admission_test(function);
        function.instruction(&Instruction::I32And);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, function);

        let callable = schema.reserve_gc_local(function).initialize(
            callee.cast_reference::<FunctionObject>(schema, function),
            function,
        );
        let code = schema.reserve_gc_local(function).initialize(
            schema
                .field(FunctionObjectSchema::CODE)
                .read(&callable, schema, function)
                .reference(),
            function,
        );
        self.emit_source_code_identity_test(meta, &callable, &code, function)?;
        code.clear(function);
        let caller_realm = self.emit_execution_realm(function);
        let context = schema
            .field(FunctionObjectSchema::CONTEXT)
            .read(&callable, schema, function)
            .reference();
        schema
            .field(FunctionContextSchema::REALM)
            .read_top(context, function);
        caller_realm.load(schema, function);
        function.instruction(&Instruction::RefEq);
        function.instruction(&Instruction::I32And);
        caller_realm.clear(function);
        callable.clear(function);
        function.instruction(&Instruction::I32Eqz);
        self.emit_branch_if_to_target(miss, function);

        match plan.effects() {
            SmallInlineEffects::Closed => {
                self.emit_small_source_body(plan, &values, output, function)?;
            }
            SmallInlineEffects::StrictActivation => {
                let callable = schema.reserve_gc_local(function).initialize(
                    callee.cast_reference::<FunctionObject>(schema, function),
                    function,
                );
                let depth = if matches!(continuation, CallContinuation::Continue) {
                    let realm = self.load_current_realm(function);
                    let base = self.runtime_helper_base()?;
                    // The same depth was just admitted, before any body effect.
                    let depth = schema.enter_source_activation_depth(function, |function| {
                        Ok::<_, EmitError>(SourceDepthOverflow::ProgramHelper(
                            schema.return_call_source_depth_overflow(&realm, base, function),
                        ))
                    })?;
                    realm.clear(function);
                    Some(depth)
                } else {
                    // Both bodies are strict. A proper tail call replaces the
                    // caller's opaque activation at the same source depth.
                    None
                };
                let previous = schema
                    .reserve_gc_local(function)
                    .initialize(schema.load_active_source_function(function), function);
                schema.replace_active_source_function(&callable, function);
                callable.clear(function);
                self.emit_small_source_body(plan, &values, output, function)?;
                schema.replace_active_source_function(&previous, function);
                previous.clear(function);
                if let Some(depth) = depth {
                    depth.restore(function);
                    depth.clear(function);
                }
            }
        }
        self.emit_branch_to_target(done, function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);

        let references: Vec<_> = values.iter().collect();
        let argv = self.emit_pre_evaluated_arg_vector(&references, function);
        self.emit_indirect_call_with_argv_to_value(
            callee,
            receiver,
            &argv,
            continuation,
            output,
            function,
        )?;
        argv.clear(function);
        self.pop_control(ControlFrameKind::Block);
        function.instruction(&Instruction::End);
        for value in values.into_iter().rev() {
            value.clear(function);
        }
        Ok(())
    }

    fn emit_small_source_body(
        &mut self,
        plan: &SmallInlineBodyIr,
        arguments: &[ValueLocals],
        output: &ValueLocals,
        function: &mut Function,
    ) -> Result<(), EmitError> {
        let schema = self.runtime_schema();
        let value = schema.reserve_value_local(function);
        value.set_undefined(function);
        self.push_scope();
        let mut bindings = Vec::new();
        for (index, name) in plan.parameters().iter().enumerate() {
            let argument = arguments.get(index).unwrap_or(&value);
            bindings.push(self.retain_expression_operand(name, argument, function).1);
        }
        for initialization in plan.initializations() {
            let names = match initialization {
                SmallInlineInitializationIr::Binding { name, .. } => std::slice::from_ref(name),
                SmallInlineInitializationIr::ObjectPattern { names, .. } => names.as_slice(),
            };
            for name in names {
                bindings.push(self.retain_expression_operand(name, &value, function).1);
            }
        }
        // Retire these explicit roots even if a getter or coercion throws.
        let cleanup = (plan.effects() == SmallInlineEffects::StrictActivation)
            .then(|| self.open_frame(ControlFrameKind::Block, function));
        if let Some(cleanup) = cleanup {
            self.throw_handler_stack.push(cleanup);
        }
        for initialization in plan.initializations() {
            match initialization {
                SmallInlineInitializationIr::Binding {
                    name,
                    value: expression,
                } => {
                    self.compile_expr_to_value(expression, &value, function)?;
                    let storage = self
                        .lookup_current_scope_binding(name)
                        .expect("inline binding was reserved");
                    self.write_binding_from_locals(storage, &value, function);
                }
                SmallInlineInitializationIr::ObjectPattern { expression, .. } => {
                    self.compile_expr_to_value(expression, &value, function)?;
                }
            }
        }
        self.compile_expr_to_value(plan.result(), output, function)?;
        if cleanup.is_some() {
            self.throw_handler_stack.pop();
            self.pop_control(ControlFrameKind::Block);
            function.instruction(&Instruction::End);
        }
        self.pop_scope();
        for binding in bindings.into_iter().rev() {
            self.release_local_binding(binding, function);
        }
        value.clear(function);
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

/// The callee is an identifier, global read, or static property named
/// `expected`. Syntax only nominates a guarded native route; the runtime
/// builtin guard still proves the evaluated callee and its Realm.
fn callee_nominates_identifier(callee: &TypedExpr, expected: &str) -> bool {
    match &callee.expr {
        ExprIr::GlobalPropertyRead { name }
        | ExprIr::GlobalIdentifierRead { name }
        | ExprIr::Identifier(name) => name == expected,
        ExprIr::EnvironmentIdentifier(identifier) => {
            identifier.name == expected
                && matches!(
                    identifier.operation,
                    scriptw_ir::EnvironmentIdentifierOperationIr::Read
                )
        }
        _ => callee_reads_static_property(callee, expected),
    }
}
