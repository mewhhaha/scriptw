use super::*;

/// Evidence for the initial value of one analyzed implicit arguments cell.
/// The named runtime Reference still performs ResolveBinding and GetValue.
struct StableImplicitArgumentsCell {
    environment_id: EnvironmentId,
}

/// Conservatively reject every source form that can invoke user code, suspend,
/// or replace a binding. Traversing nested functions is intentional: a closure
/// may be called after its owner returns. Refusal only loses a ValueInfo fact.
struct ArgumentsCellStability;

impl<'ast> Visitor<'ast> for ArgumentsCellStability {
    type BreakTy = ();

    fn visit_expression(&mut self, expression: &'ast Expression) -> ControlFlow<()> {
        match expression {
            Expression::Identifier(_)
            | Expression::Literal(_)
            | Expression::This(_)
            | Expression::NewTarget(_)
            | Expression::Parenthesized(_)
            | Expression::FunctionExpression(_)
            | Expression::ArrowFunction(_)
            | Expression::AsyncArrowFunction(_)
            | Expression::GeneratorExpression(_)
            | Expression::AsyncFunctionExpression(_)
            | Expression::AsyncGeneratorExpression(_) => expression.visit_with(self),
            _ => ControlFlow::Break(()),
        }
    }

    fn visit_statement(&mut self, statement: &'ast Statement) -> ControlFlow<()> {
        match statement {
            Statement::With(_)
            | Statement::ForInLoop(_)
            | Statement::ForOfLoop(_)
            | Statement::Debugger => ControlFlow::Break(()),
            _ => statement.visit_with(self),
        }
    }

    fn visit_pattern(&mut self, _: &'ast Pattern) -> ControlFlow<()> {
        // Binding patterns can run arbitrary iterator/getter code.
        ControlFlow::Break(())
    }

    fn visit_class_declaration(&mut self, _: &'ast ClassDeclaration) -> ControlFlow<()> {
        ControlFlow::Break(())
    }
}

impl StableImplicitArgumentsCell {
    fn analyze(lowerer: &ScriptLowerer<'_>, name: &str) -> Option<Self> {
        if name != "arguments"
            || lowerer.lookup_binding(name).is_some()
            || !lowerer.with_environment_chain.is_empty()
        {
            return None;
        }
        let current = lowerer
            .analysis
            .function_plans
            .get(&lowerer.current_owner_id)?;
        let (owner_id, environment_id) = if current.protocol.flavor() == FunctionFlavor::Arrow {
            let capture = current.captures.get(LEXICAL_ARGUMENTS_NAME)?;
            if capture.source_name != LEXICAL_ARGUMENTS_NAME {
                return None;
            }
            (&capture.owner_id, capture.environment_id)
        } else {
            let owner = &lowerer.analysis.owner_plans[&lowerer.current_owner_id];
            (&lowerer.current_owner_id, owner.activation_environment_id)
        };
        let owner = lowerer.analysis.owner_plans.get(owner_id)?;
        let function = lowerer.analysis.function_plans.get(owner_id)?;
        let environment = lowerer.analysis.environment_plans.get(&environment_id)?;
        if function.protocol.flavor() != FunctionFlavor::Ordinary
            || owner.root_bindings.contains("arguments")
            || environment.owner_id != *owner_id
            || !environment
                .binding_storage_names
                .contains(LEXICAL_ARGUMENTS_NAME)
            || !environment
                .owned_env_slots
                .contains_key(LEXICAL_ARGUMENTS_NAME)
        {
            return None;
        }
        let mut stability = ArgumentsCellStability;
        if function.parameters.visit_with(&mut stability).is_break()
            || function.body.visit_with(&mut stability).is_break()
        {
            return None;
        }
        Some(Self { environment_id })
    }

    fn value_info(self, lowerer: &ScriptLowerer<'_>) -> ValueInfo {
        // Retain the analyzed identity until the only consumer requests its
        // initial object-kind fact. No bare private-storage spelling proves it.
        debug_assert!(lowerer.analysis.environment_plans[&self.environment_id]
            .binding_storage_names
            .contains(LEXICAL_ARGUMENTS_NAME));
        ValueInfo::new(ValueKind::Arguments)
    }
}

impl ScriptLowerer<'_> {
    /// Var bookkeeping must keep its activation/captured declarative position.
    /// Creating a block binding here would incorrectly cut off the enclosing
    /// with chain for the next occurrence of the same target name.
    pub(super) fn record_destructuring_binding(
        &mut self,
        source_name: String,
        info: BindingInfo,
        target: &DestructuringTargetIr,
    ) {
        match info.mode {
            BindingMode::Var => {
                let value_info = match target {
                    // The selected Object Environment may receive the value
                    // while the fallback variable keeps its previous value.
                    // Getter/default/eval effects can also change that fallback;
                    // no precise rest Array/Object fact survives this Reference.
                    DestructuringTargetIr::ResolvedVarBinding { .. }
                    | DestructuringTargetIr::AssignmentIdentifier(_) => {
                        unknown_runtime_value_info()
                    }
                    DestructuringTargetIr::Binding { .. }
                    | DestructuringTargetIr::AssignmentProperty { .. }
                    | DestructuringTargetIr::AssignmentPrivate { .. }
                    | DestructuringTargetIr::AssignmentSuper { .. }
                    | DestructuringTargetIr::NestedArray(_)
                    | DestructuringTargetIr::NestedObject(_) => ValueInfo {
                        kind: info.kind,
                        possible_kinds: info.possible_kinds,
                        heap_shape: info.heap_shape,
                        function_targets: info.function_targets,
                    },
                };
                self.set_binding_value_info(&source_name, value_info);
            }
            BindingMode::Let | BindingMode::Const => self.declare_binding(source_name, info),
        }
    }

    pub(super) fn destructuring_binding_target(
        &self,
        mode: BindingMode,
        source_name: String,
        storage_name: String,
    ) -> DestructuringTargetIr {
        if mode == BindingMode::Var {
            if self.borrows_direct_eval_variable_environment() {
                return DestructuringTargetIr::AssignmentIdentifier(
                    IdentifierWriteReferenceIr::environment(
                        source_name,
                        self.reference_strictness(),
                    ),
                );
            }
            if self.uses_runtime_identifier_environment_for(&source_name) {
                return DestructuringTargetIr::ResolvedVarBinding {
                    name: storage_name,
                    reference: IdentifierWriteReferenceIr::environment(
                        source_name,
                        self.reference_strictness(),
                    ),
                };
            }
            let fallback = self.locate_identifier_reference(&source_name);
            if let Some(objects) = self
                .with_environment_chain
                .select_preceding(fallback.declarative_position())
            {
                let reference = objects
                    .into_reference_plan(source_name, self.reference_strictness())
                    .deferred_var_write(storage_name.clone());
                return DestructuringTargetIr::ResolvedVarBinding {
                    name: storage_name,
                    reference,
                };
            }
        }
        DestructuringTargetIr::Binding {
            mode,
            name: storage_name,
        }
    }

    pub(super) fn uses_runtime_identifier_environment(&self) -> bool {
        let owner = &self.analysis.owner_plans[&self.current_owner_id];
        self.analysis
            .module_local_candidates
            .contains_key(&self.current_owner_id)
            || self.analysis.environment_plans[&owner.activation_environment_id].eval_visible
    }

    /// Only a resolved current-owner activation local with no physical cell
    /// may bypass named lookup. Captures, module cells and Realm globals retain
    /// their live Reference even when the current activation has no storage.
    pub(super) fn uses_runtime_identifier_environment_for(&self, name: &str) -> bool {
        if !self.uses_runtime_identifier_environment() {
            return false;
        }
        let Some(locals) = self
            .analysis
            .module_local_candidates
            .get(&self.current_owner_id)
        else {
            return true;
        };
        let Some(binding) = self.lookup_binding(name) else {
            return true;
        };
        if !locals.contains(&binding.storage_name)
            || self
                .captured_binding_positions
                .contains_key(&binding.storage_name)
            || !self.with_environment_chain.is_empty()
        {
            return true;
        }
        let Some(environments) = self
            .analysis
            .physical_binding_environments
            .get(&binding.storage_name)
        else {
            return true;
        };
        let owner = &self.analysis.owner_plans[&self.current_owner_id];
        !environments.contains(&owner.activation_environment_id)
            || environments.iter().any(|id| {
                let environment = &self.analysis.environment_plans[id];
                environment.owner_id == self.current_owner_id
                    && environment
                        .owned_env_slots
                        .contains_key(&binding.storage_name)
            })
    }

    pub(super) fn environment_identifier(
        &mut self,
        name: String,
        operation: EnvironmentIdentifierOperationIr,
    ) -> TypedExpr {
        if let Some(host) = self.host_surface_policy.resolve_global(&name) {
            self.used_host_builtins.insert(host);
        }
        let read_info = matches!(operation, EnvironmentIdentifierOperationIr::Read)
            .then(|| StableImplicitArgumentsCell::analyze(self, &name))
            .flatten()
            .map(|cell| cell.value_info(self));
        self.invalidate_unknown_user_code_effects();
        TypedExpr::from_info(
            read_info.unwrap_or_else(unknown_runtime_value_info),
            ExprIr::EnvironmentIdentifier(Box::new(EnvironmentIdentifierIr::current(
                name,
                self.reference_strictness(),
                operation,
            ))),
        )
    }

    pub(super) fn lower_environment_identifier_call(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
    ) -> Option<TypedExpr> {
        let Expression::Identifier(identifier) = Self::unwrap_parenthesized_expr(callee) else {
            return None;
        };
        let name = self.interner.resolve_expect(identifier.sym()).to_string();
        if !self.uses_runtime_identifier_environment_for(&name) {
            return None;
        }
        if self
            .analysis
            .module_execution
            .is_private_dispatcher_name(&name)
        {
            return None;
        }
        let direct_eval = (name == "eval").then(|| self.direct_eval_context());
        if let Some(context) = &direct_eval {
            self.register_direct_eval_source(context, arguments);
        }
        let args = self
            .lower_call_args_expanding_spread(arguments)
            .into_arguments_without_predecessor();
        Some(self.environment_identifier(
            name,
            EnvironmentIdentifierOperationIr::Call { args, direct_eval },
        ))
    }

    pub(super) fn lower_environment_identifier_assignment(
        &mut self,
        name: String,
        operation: AssignOp,
        rhs: &Expression,
    ) -> TypedExpr {
        let value = Box::new(self.lower_expression(rhs));
        use EnvironmentCompoundOperationIr as Compound;
        use EnvironmentIdentifierOperationIr as Identifier;
        let operation = match operation {
            AssignOp::Assign => Identifier::Assign { value },
            AssignOp::BoolAnd | AssignOp::BoolOr | AssignOp::Coalesce => {
                Identifier::LogicalCompound {
                    operation: match operation {
                        AssignOp::BoolAnd => LogicalBinaryOp::And,
                        AssignOp::BoolOr => LogicalBinaryOp::Or,
                        AssignOp::Coalesce => LogicalBinaryOp::Coalesce,
                        _ => unreachable!(),
                    },
                    rhs: value,
                }
            }
            operation => Identifier::EagerCompound {
                operation: match operation {
                    AssignOp::Add => Compound::Add,
                    AssignOp::Sub => Compound::Arithmetic(ArithmeticBinaryOp::Sub),
                    AssignOp::Mul => Compound::Arithmetic(ArithmeticBinaryOp::Mul),
                    AssignOp::Div => Compound::Arithmetic(ArithmeticBinaryOp::Div),
                    AssignOp::Mod => Compound::Arithmetic(ArithmeticBinaryOp::Mod),
                    AssignOp::Exp => Compound::Arithmetic(ArithmeticBinaryOp::Exp),
                    AssignOp::And => Compound::Bitwise(BitwiseBinaryOp::And),
                    AssignOp::Or => Compound::Bitwise(BitwiseBinaryOp::Or),
                    AssignOp::Xor => Compound::Bitwise(BitwiseBinaryOp::Xor),
                    AssignOp::Shl => Compound::Bitwise(BitwiseBinaryOp::Shl),
                    AssignOp::Shr => Compound::Bitwise(BitwiseBinaryOp::Shr),
                    AssignOp::Ushr => Compound::Bitwise(BitwiseBinaryOp::UShr),
                    AssignOp::Assign
                    | AssignOp::BoolAnd
                    | AssignOp::BoolOr
                    | AssignOp::Coalesce => unreachable!(),
                },
                rhs: value,
            },
        };
        self.environment_identifier(name, operation)
    }
}
