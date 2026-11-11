use super::*;

/// One identifier logical-assignment Reference located before RHS lowering.
/// Possible global callable targets are saved before GetValue effects and RHS
/// lowering; they remain candidates, never proof of the value a live Get returns.
#[must_use = "a pre-RHS logical-assignment Reference must be consumed after RHS lowering"]
pub(super) struct LocatedIdentifierLogicalAssignment {
    reference: LocatedIdentifierReference,
    global_value_candidates: Option<ValueInfo>,
}

impl LocatedIdentifierLogicalAssignment {
    pub(super) fn declarative_position(&self) -> Option<DeclarativeEnvironmentPosition> {
        self.reference.declarative_position()
    }

    pub(super) fn is_declarative(&self) -> bool {
        matches!(
            self.reference,
            LocatedIdentifierReference::Declarative { .. }
        )
    }

    pub(super) fn reject_definite_tdz(self) -> Result<Self, TypedExpr> {
        let Self {
            reference,
            global_value_candidates,
        } = self;
        match reference {
            LocatedIdentifierReference::Declarative {
                resolution: BindingResolution::Uninitialized(violation),
                ..
            } => Err(violation.into_throw()),
            reference => Ok(Self {
                reference,
                global_value_candidates,
            }),
        }
    }
}

pub(super) enum LogicalAssignmentReachability {
    Definite,
    WithEnvironmentFallback,
}

impl<'a> ScriptLowerer<'a> {
    pub(super) fn locate_identifier_logical_assignment(
        &self,
        name: &str,
    ) -> LocatedIdentifierLogicalAssignment {
        let reference = self.locate_identifier_reference(name);
        let global_value_candidates =
            (matches!(&reference, LocatedIdentifierReference::Unresolvable)
                || self.is_unshadowed_script_global_binding(name))
            .then(|| self.lookup_global_property_info(name))
            .flatten()
            .filter(|info| info.source != GlobalPropertySource::DefinitelyDeleted)
            .map(|info| info.value_info.clone());
        LocatedIdentifierLogicalAssignment {
            reference,
            global_value_candidates,
        }
    }

    fn lower_global_environment_logical_assignment(
        &mut self,
        name: String,
        op: LogicalBinaryOp,
        rhs: TypedExpr,
        lhs_candidates: Option<ValueInfo>,
    ) -> TypedExpr {
        let mut result_info = self.merge_value_infos(
            lhs_candidates.unwrap_or_else(unknown_runtime_value_info),
            rhs.value_info(),
        );
        result_info.widen_for_possible_replacement();
        self.mark_host_builtins_from_info(&result_info);
        // The conditional Put can invoke setters or inherited Proxy traps. Its
        // effects follow the RHS and cannot leave that branch's facts intact.
        self.observe_all_planned_source_as_unknown_property_hooks();
        self.invalidate_unknown_user_code_effects();
        TypedExpr::from_info(
            result_info,
            ExprIr::EnvironmentIdentifier(Box::new(EnvironmentIdentifierIr::global(
                name,
                self.reference_strictness(),
                EnvironmentIdentifierOperationIr::LogicalCompound {
                    operation: op,
                    rhs: Box::new(rhs),
                },
            ))),
        )
    }

    pub(super) fn lower_located_identifier_logical_assignment(
        &mut self,
        name: String,
        op: LogicalBinaryOp,
        rhs: TypedExpr,
        located: LocatedIdentifierLogicalAssignment,
        reachability: LogicalAssignmentReachability,
    ) -> TypedExpr {
        let LocatedIdentifierLogicalAssignment {
            reference,
            global_value_candidates,
        } = located;
        let binding = match reference {
            LocatedIdentifierReference::Declarative {
                resolution: BindingResolution::Uninitialized(violation),
                ..
            } => return violation.into_throw(),
            LocatedIdentifierReference::Declarative {
                resolution: BindingResolution::Initialized(binding),
                ..
            } => Some(binding),
            LocatedIdentifierReference::Unresolvable => None,
            LocatedIdentifierReference::Declarative {
                resolution: BindingResolution::Unresolvable,
                ..
            } => unreachable!("a declarative location cannot be unresolvable"),
        };

        if binding.is_none() || self.is_unshadowed_script_global_binding(&name) {
            if binding.is_some() {
                self.widen_binding_for_possible_replacement(&name);
            }
            return self.lower_global_environment_logical_assignment(
                name,
                op,
                rhs,
                global_value_candidates,
            );
        }

        let binding = binding.expect("a non-global located Reference must own a binding");
        let lhs_info = match &reachability {
            LogicalAssignmentReachability::Definite => ValueInfo {
                kind: binding.kind,
                possible_kinds: binding.possible_kinds,
                heap_shape: binding.heap_shape.clone(),
                function_targets: binding.function_targets.clone(),
            },
            LogicalAssignmentReachability::WithEnvironmentFallback => {
                let mut value = ValueInfo {
                    kind: binding.kind,
                    possible_kinds: binding.possible_kinds,
                    heap_shape: binding.heap_shape.clone(),
                    function_targets: binding.function_targets.clone(),
                };
                value.widen_for_possible_replacement();
                value
            }
        };
        let lhs = TypedExpr::from_info(lhs_info, ExprIr::Identifier(binding.storage_name.clone()));
        let write = if binding.mode == BindingMode::Const {
            self.immutable_binding_write(&binding.storage_name, rhs)
        } else {
            let result_info = match &reachability {
                LogicalAssignmentReachability::Definite => {
                    self.merge_value_infos(lhs.value_info(), rhs.value_info())
                }
                LogicalAssignmentReachability::WithEnvironmentFallback => {
                    let mut value = self.merge_value_infos(lhs.value_info(), rhs.value_info());
                    value.widen_for_possible_replacement();
                    value
                }
            };
            self.set_binding_value_info(&name, result_info);
            TypedExpr::from_info(
                rhs.value_info(),
                ExprIr::AssignIdentifier {
                    name: binding.storage_name,
                    value: Box::new(rhs),
                },
            )
        };

        let result_info = match &reachability {
            LogicalAssignmentReachability::Definite => {
                self.merge_value_infos(lhs.value_info(), write.value_info())
            }
            LogicalAssignmentReachability::WithEnvironmentFallback => {
                let mut value = self.merge_value_infos(lhs.value_info(), write.value_info());
                value.widen_for_possible_replacement();
                value
            }
        };
        TypedExpr::from_info(
            result_info,
            ExprIr::LogicalShortCircuit {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(write),
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};

    #[test]
    fn global_logical_assignment_owns_one_reference_and_a_conditional_rhs() {
        for (source, op) in [
            ("x = 1; x &&= (x = 'rhs');", LogicalBinaryOp::And),
            ("x = 1; x ||= (x = 'rhs');", LogicalBinaryOp::Or),
            ("x = 1; x ??= (x = 'rhs');", LogicalBinaryOp::Coalesce),
        ] {
            let source = parse(source, ParseOptions::script()).expect("script should parse");
            let program = lower(&source);
            assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
            let script = program.script.expect("script IR should exist");
            let StatementIr::Expression(logical) = script.body.statements.last().unwrap() else {
                panic!("logical assignment must remain an expression");
            };
            let ExprIr::EnvironmentIdentifier(reference) = &logical.expr else {
                panic!("GetValue and PutValue must share one Reference: {logical:?}");
            };
            assert_eq!(reference.name, "x");
            assert_eq!(
                reference.resolution_start(),
                EnvironmentIdentifierResolutionStart::GlobalEnvironment
            );
            let EnvironmentIdentifierOperationIr::LogicalCompound { operation, rhs } =
                &reference.operation
            else {
                panic!("RHS and PutValue must be conditional");
            };
            assert_eq!(*operation, op);
            assert!(
                matches!(&rhs.expr, ExprIr::GlobalPropertyWrite { name, value, .. }
                if name == "x" && matches!(&value.expr, ExprIr::String(text) if text == "rhs"))
            );
            assert_eq!(logical.possible_kinds, KindSet::all_runtime_tags());
            assert!(logical.heap_shape.is_none());
        }
    }

    #[test]
    fn global_logical_get_effects_precede_rhs_value_facts() {
        let source = parse(
            "let effect = 1; function hook() { effect = 's'; } var parseInt; parseInt &&= effect + 1;",
            ParseOptions::script(),
        )
        .expect("script should parse");
        let program = lower(&source);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.expect("script IR should exist");
        let StatementIr::Expression(logical) = script.body.statements.last().unwrap() else {
            panic!("logical assignment must remain an expression");
        };
        let ExprIr::EnvironmentIdentifier(reference) = &logical.expr else {
            panic!("global logical assignment must retain its Reference");
        };
        let EnvironmentIdentifierOperationIr::LogicalCompound { rhs, .. } = &reference.operation
        else {
            panic!("logical assignment must retain its conditional RHS");
        };
        assert!(
            matches!(&rhs.expr, ExprIr::CoerciveAdd { .. }),
            "a possible getter can replace captured effect before the RHS: {rhs:?}"
        );
    }

    #[test]
    fn global_logical_result_keeps_possible_callable_targets_without_a_value_proof() {
        let source = parse(
            "candidate = Function; (candidate ||= (candidate = 1))('return 2');",
            ParseOptions::script(),
        )
        .expect("script should parse");
        let program = lower(&source);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let script = program.script.expect("script IR should exist");
        assert!(
            script.prepared_dynamic_functions.iter().any(|prepared| {
                prepared.arguments == ["return 2"]
                    && matches!(
                        prepared.outcome,
                        PreparedDynamicFunctionOutcome::Compiled { .. }
                    )
            }),
            "the skipped logical branch can still return the original Function constructor"
        );
    }
}
