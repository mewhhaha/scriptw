//! Lower exact compiler-owned module operations through the ordinary AOT pipeline.

use super::*;

impl ScriptLowerer<'_> {
    pub(super) fn admit_sync_disposable_scope_owner(
        &mut self,
    ) -> Option<SyncDisposableScopeOwnerPlan> {
        if self.root_this_binding == RootThisBinding::Undefined {
            // The retained source-phase drivers do not own independent
            // Module environments. Only a canonical module activation proves
            // this module's resources belong to an admitted execution lifetime.
            let mut owner_id = Some(self.current_owner_id.as_str());
            let mut has_module_activation = false;
            while let Some(id) = owner_id {
                if self.analysis.function_plans.get(id).is_some_and(|owner| {
                    matches!(
                        owner.protocol,
                        FunctionProtocolIr::ModuleActivation
                            | FunctionProtocolIr::AsyncModuleActivation
                    )
                }) {
                    has_module_activation = true;
                    break;
                }
                owner_id = self
                    .analysis
                    .owner_plans
                    .get(id)
                    .and_then(|owner| owner.parent_owner_id.as_deref());
            }
            if !has_module_activation {
                self.unsupported(
                    "using declaration in a module without a canonical execution owner",
                );
                return None;
            }
        }
        Some(
            self.current_function_id
                .as_ref()
                .and_then(|id| self.analysis.function_plans.get(id))
                .map_or(SyncDisposableScopeOwnerPlan::Immediate, |owner| {
                    owner.sync_disposable_scope_owner()
                }),
        )
    }

    pub(super) fn lower_module_instantiation_boundary(
        &mut self,
        statement: &Statement,
    ) -> Option<(StatementIr, ValueKind)> {
        if !self
            .analysis
            .module_execution
            .boundaries
            .contains(&(std::ptr::from_ref(statement) as usize))
        {
            return None;
        }
        let owner = self
            .current_function_id
            .as_ref()
            .and_then(|id| self.analysis.function_plans.get(id))
            .expect("an instantiation boundary belongs to a module owner");
        let protocol = owner.protocol;
        assert!(self.current_generator_resume_state.is_none());
        // Evaluation may follow arbitrary shared-state mutation in another module.
        self.invalidate_unknown_user_code_effects();
        let boundary = match protocol {
            FunctionProtocolIr::ModuleActivation => {
                assert!(self.current_async_resume_state.is_none());
                let boundary = GeneratorPlanIr::MODULE_INSTANTIATION;
                StatementIr::GeneratorYield {
                    value: TypedExpr::undefined(),
                    form: YieldForm::Plain,
                    suspend_state: boundary.suspend_state,
                    resume_state: boundary.resume_state,
                    resume_mode: GeneratorResumeModeIr::Ignore,
                }
            }
            FunctionProtocolIr::AsyncModuleActivation => {
                assert_eq!(self.current_async_resume_state, Some(0));
                self.current_async_resume_state = Some(1);
                StatementIr::AsyncModuleInstantiation
            }
            _ => panic!("trusted instantiation boundary requires a module owner"),
        };
        Some((boundary, ValueKind::Undefined))
    }

    pub(super) fn lower_synchronous_module_expression(
        &mut self,
        expression: &Expression,
    ) -> Option<TypedExpr> {
        let pointer = std::ptr::from_ref(expression) as usize;
        if let Some(function) = self
            .analysis
            .module_execution
            .foreign_calls
            .get(&pointer)
            .copied()
        {
            let Expression::ArrayLiteral(arguments) = expression else {
                panic!("foreign operation is owned by its trusted argument scaffold");
            };
            let declaration = self
                .analysis
                .module_execution
                .foreign_modules
                .function(function)
                .expect("linked foreign declaration");
            let kind = match declaration.mode() {
                crate::foreign::ForeignFunctionMode::Asynchronous => ValueKind::Object,
                crate::foreign::ForeignFunctionMode::Synchronous => match declaration.result() {
                    crate::foreign::ForeignResultType::Number => ValueKind::Number,
                    crate::foreign::ForeignResultType::StringUtf16 => ValueKind::String,
                    crate::foreign::ForeignResultType::Bytes => ValueKind::Object,
                    crate::foreign::ForeignResultType::Void => ValueKind::Undefined,
                },
            };
            let operands = arguments
                .as_ref()
                .iter()
                .map(|argument| {
                    self.lower_expression(argument.as_ref().expect("foreign parameter has no hole"))
                })
                .collect();
            let call = crate::foreign::ForeignCallIr::new(
                &self.analysis.module_execution.foreign_modules,
                function,
                operands,
            );
            self.invalidate_unknown_user_code_effects();
            return Some(TypedExpr::from_info(
                ValueInfo::new(kind),
                ExprIr::ForeignCall(Box::new(call)),
            ));
        }
        let operations = &self.analysis.module_execution;
        if let Some(builtin) = operations.intrinsics.get(&pointer) {
            return Some(TypedExpr::from_info(
                Self::standard_builtin_value_info(*builtin),
                ExprIr::FunctionValue(builtin.function_id()),
            ));
        }
        let operation = if let Some(value) = operations.synthetic_values.get(&pointer) {
            match value {
                crate::modules::SyntheticModuleValueIr::Json(json) => {
                    ExprIr::JsonModuleValue(Box::new(json.clone()))
                }
                crate::modules::SyntheticModuleValueIr::Text(text) => ExprIr::String(text.clone()),
                crate::modules::SyntheticModuleValueIr::Bytes(bytes) => {
                    ExprIr::BytesModuleValue(Box::new(bytes.clone()))
                }
            }
        } else if let Some(cell) = operations.reads.get(&pointer) {
            ExprIr::ModuleBindingRead(cell.clone())
        } else if let Some(evaluation) = operations.evaluations.get(&pointer) {
            ExprIr::ModuleEvaluate(evaluation.clone())
        } else if let Some(evaluation) = operations.async_dependencies.get(&pointer) {
            ExprIr::ModuleHasAsyncDependencies(evaluation.clone())
        } else if let Some(evaluation) = operations.deferred_imports.get(&pointer) {
            ExprIr::ModuleDeferredImportEvaluate(evaluation.clone())
        } else if let Some(evaluation) = operations.deferred_evaluations.get(&pointer) {
            ExprIr::DeferredModuleEvaluate(evaluation.clone())
        } else if let Some(graph) = operations.initializers.get(&pointer) {
            ExprIr::ModuleExecutionGraph(Box::new(graph.clone()))
        } else if let Expression::ArrayLiteral(array) = expression {
            let graph = operations
                .graphs
                .get(&(std::ptr::from_ref(array) as usize))?;
            ExprIr::ModuleExecutionGraph(Box::new(graph.clone()))
        } else {
            return None;
        };
        self.invalidate_unknown_user_code_effects();
        let info = match &operation {
            ExprIr::ModuleBindingRead(_)
            | ExprIr::JsonModuleValue(_)
            | ExprIr::BytesModuleValue(_) => unknown_runtime_value_info(),
            ExprIr::String(text) => Self::string_value_info(text),
            ExprIr::ModuleEvaluate(_) | ExprIr::ModuleDeferredImportEvaluate(_) => {
                ValueInfo::new(ValueKind::Object)
            }
            ExprIr::ModuleHasAsyncDependencies(_) => ValueInfo::new(ValueKind::Boolean),
            _ => ValueInfo::undefined(),
        };
        Some(TypedExpr::from_info(info, operation))
    }
}
