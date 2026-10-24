use super::*;
use crate::gc_types::{FunctionContext, ValueLocals};

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
                        self.completion().copy_from(&result, function);
                        self.emit_propagate_current_throw_if_needed(function);
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
        self.compile_expr_to_value(callee_expression, &callee, function)?;
        self.emit_propagate_current_throw_if_needed(function);
        match this_expression {
            Some(expression) => {
                self.compile_expr_to_value(expression, &receiver, function)?;
                self.emit_propagate_current_throw_if_needed(function);
            }
            None => receiver.set_undefined(function),
        }
        let targets = callee_expression.function_targets.known_targets();
        let numeric_leaf = (targets.len() == 1)
            .then(|| targets.iter().next().and_then(|id| self.functions.get(id)))
            .flatten()
            .filter(|meta| meta.numeric_leaf.is_some())
            .cloned();
        if let Some(meta) = numeric_leaf {
            if self.try_emit_numeric_leaf_call(
                &meta,
                meta.numeric_leaf.as_ref().expect("selected pure leaf plan"),
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
        // Target knowledge selects a candidate only. The actual once-read
        // callee and evaluated operands still prove the guarded route at runtime.
        let numeric_imul_candidate = callee_expression
            .function_targets
            .known_targets()
            .iter()
            .any(|id| StandardBuiltinId::from_function_id(id) == Some(StandardBuiltinId::MathImul));
        if !numeric_imul_candidate
            || !self.try_emit_numeric_imul_call(
                &callee,
                &receiver,
                arguments,
                continuation,
                output,
                function,
            )?
        {
            self.emit_indirect_call_from_locals(
                &callee,
                Some(&receiver),
                arguments,
                continuation,
                output,
                function,
            )?;
        }
        receiver.clear(function);
        callee.clear(function);
        Ok(())
    }
}
