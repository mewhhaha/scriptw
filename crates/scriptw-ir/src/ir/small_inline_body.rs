use crate::{
    CapturedBindingInitializationIr, DestructuringPropertyKeyIr, DestructuringTargetIr, ExprIr,
    FunctionIr, FunctionProtocolIr, ObjectPropertyIr, PropertyKeyIr, SpecOperationIr, StatementIr,
    TypedExpr,
};
use std::collections::BTreeSet;

const MAX_NODES: usize = 20;
const MAX_PARAMETERS: usize = 4;

/// A small source body whose only free reads are its planned captures. The
/// call-site proof must separately establish agreement for those cells.
#[derive(Debug, Clone)]
pub struct SmallInlineBodyIr {
    parameters: Vec<String>,
    initializations: Vec<SmallInlineInitializationIr>,
    result: TypedExpr,
    effects: SmallInlineEffects,
}

#[derive(Debug, Clone)]
pub enum SmallInlineInitializationIr {
    Binding {
        name: String,
        value: TypedExpr,
    },
    ObjectPattern {
        names: Vec<String>,
        expression: TypedExpr,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SmallInlineEffects {
    /// No abrupt completion, user code or observable source activation.
    Closed,
    /// Keep the strict source activation live while evaluating the expression.
    StrictActivation,
}

impl SmallInlineBodyIr {
    pub fn for_function(function: &FunctionIr) -> Option<Self> {
        if !matches!(
            function.protocol,
            FunctionProtocolIr::OrdinaryCallAndConstruct | FunctionProtocolIr::Arrow
        ) || function.builtin_entry.is_some()
            || function.eval_environment.is_some()
            || !function.owned_env_bindings.is_empty()
            || function.captures_private_environment
            || function.captures_lexical_arguments
            || function.captures_lexical_this
            || function.body.lexical_environment.is_some()
            || function.params.len() > MAX_PARAMETERS
        {
            return None;
        }
        let mut proof = BodyProof {
            names: BTreeSet::new(),
            remaining: MAX_NODES,
            effects: SmallInlineEffects::Closed,
        };
        for capture in &function.captured_bindings {
            if capture.source_name.contains('.') {
                return None;
            }
            proof.consume()?;
            proof.names.insert(capture.name.clone());
            if capture.initialization == CapturedBindingInitializationIr::Checked {
                proof.effects = SmallInlineEffects::StrictActivation;
            }
        }
        let (last, prefix) = function.body.statements.split_last()?;
        let StatementIr::Return(result) = last else {
            return None;
        };

        let mut parameters = Vec::new();
        let mut initializations = Vec::new();
        let mut prefix_index = 0;
        for (index, param) in function.params.iter().enumerate() {
            if param.is_rest
                || param.default_init.is_some()
                || !proof.names.insert(param.name.clone())
            {
                return None;
            }
            parameters.push(param.name.clone());
            if param.bound_names() == std::slice::from_ref(&param.name) {
                continue;
            }
            let StatementIr::ParameterInitialization {
                parameter_index,
                statements,
            } = prefix.get(prefix_index)?
            else {
                return None;
            };
            if *parameter_index != index {
                return None;
            }
            let [StatementIr::DeclarationEvaluation(expression)] = statements.as_slice() else {
                return None;
            };
            let ExprIr::ObjectDestructure { value, pattern } = &expression.expr else {
                return None;
            };
            if !matches!(&value.expr, ExprIr::Identifier(name) if name == &param.name)
                || pattern.rest.is_some()
            {
                return None;
            }
            proof.consume()?;
            let mut names = Vec::new();
            for property in &pattern.properties {
                let DestructuringTargetIr::Binding { name, .. } = &property.target else {
                    return None;
                };
                if property.default.is_some()
                    || !matches!(property.key, DestructuringPropertyKeyIr::Static(_))
                    || !proof.names.insert(name.clone())
                {
                    return None;
                }
                proof.consume()?;
                names.push(name.clone());
            }
            proof.effects = SmallInlineEffects::StrictActivation;
            initializations.push(SmallInlineInitializationIr::ObjectPattern {
                names,
                expression: expression.clone(),
            });
            prefix_index += 1;
        }

        for statement in &prefix[prefix_index..] {
            match statement {
                StatementIr::Expression(TypedExpr {
                    expr: ExprIr::String(_),
                    ..
                }) => {
                    // Function-body directives have already selected strictness.
                    proof.consume()?;
                }
                StatementIr::Lexical { name, init, .. } => {
                    proof.bind(name, init)?;
                    initializations.push(SmallInlineInitializationIr::Binding {
                        name: name.clone(),
                        value: init.clone(),
                    });
                }
                StatementIr::Var(declarations) => {
                    for declaration in declarations {
                        let init = declaration
                            .init
                            .clone()
                            .unwrap_or_else(TypedExpr::undefined);
                        proof.bind(&declaration.name, &init)?;
                        initializations.push(SmallInlineInitializationIr::Binding {
                            name: declaration.name.clone(),
                            value: init,
                        });
                    }
                }
                _ => return None,
            }
        }
        proof.expression(result, true)?;
        if proof.effects == SmallInlineEffects::StrictActivation && !function.strict {
            return None;
        }
        Some(Self {
            parameters,
            initializations,
            result: result.clone(),
            effects: proof.effects,
        })
    }

    pub fn parameters(&self) -> &[String] {
        &self.parameters
    }

    pub fn initializations(&self) -> &[SmallInlineInitializationIr] {
        &self.initializations
    }

    pub fn result(&self) -> &TypedExpr {
        &self.result
    }

    pub fn effects(&self) -> SmallInlineEffects {
        self.effects
    }
}

struct BodyProof {
    names: BTreeSet<String>,
    remaining: usize,
    effects: SmallInlineEffects,
}

impl BodyProof {
    fn consume(&mut self) -> Option<()> {
        self.remaining = self.remaining.checked_sub(1)?;
        Some(())
    }

    fn bind(&mut self, name: &str, init: &TypedExpr) -> Option<()> {
        self.consume()?;
        // Reads of a later hoisted var, lexical TDZ and duplicate declarations
        // still need the ordinary FunctionDeclarationInstantiation path.
        self.expression(init, false)?;
        self.names.insert(name.to_owned()).then_some(())
    }

    fn expression(&mut self, value: &TypedExpr, tail: bool) -> Option<()> {
        self.consume()?;
        match &value.expr {
            ExprIr::Undefined
            | ExprIr::Null
            | ExprIr::Boolean(_)
            | ExprIr::Number(_)
            | ExprIr::String(_) => Some(()),
            ExprIr::Identifier(name)
                if self.names.contains(name) && !matches!(name.as_str(), "arguments" | "eval") =>
            {
                Some(())
            }
            ExprIr::Void { expr } | ExprIr::TypeOf { expr } | ExprIr::LogicalNot { expr } => {
                self.expression(expr, false)
            }
            ExprIr::StrictEquality { lhs, rhs, .. } => {
                self.expression(lhs, false)?;
                self.expression(rhs, false)
            }
            ExprIr::SpecOperation {
                operation:
                    SpecOperationIr::StrictEqualityComparison | SpecOperationIr::SameValueZero,
                operands,
            } => {
                let [lhs, rhs] = operands.as_slice() else {
                    return None;
                };
                self.expression(lhs, false)?;
                self.expression(rhs, false)
            }
            ExprIr::LogicalShortCircuit { lhs, rhs, .. } | ExprIr::Comma { lhs, rhs } => {
                self.expression(lhs, false)?;
                self.expression(rhs, tail)
            }
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                self.expression(condition, false)?;
                self.expression(then_expr, tail)?;
                self.expression(else_expr, tail)
            }
            ExprIr::ObjectLiteral(properties) => {
                for property in properties {
                    let ObjectPropertyIr::Data { value, .. } = property else {
                        return None;
                    };
                    self.consume()?;
                    self.expression(value, false)?;
                }
                Some(())
            }
            ExprIr::PropertyRead {
                target,
                key: PropertyKeyIr::StaticString(_),
            } => {
                self.effects = SmallInlineEffects::StrictActivation;
                self.expression(target, false)
            }
            ExprIr::CoerciveAdd { lhs, rhs }
            | ExprIr::StringConcat { lhs, rhs }
            | ExprIr::BinaryNumber { lhs, rhs, .. }
            | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. } => {
                self.effects = SmallInlineEffects::StrictActivation;
                self.expression(lhs, false)?;
                self.expression(rhs, false)
            }
            ExprIr::MaterializeBinding { name, value, body } => {
                self.bind(name, value)?;
                let result = self.expression(body, tail);
                self.names.remove(name);
                result
            }
            ExprIr::CallMethod {
                receiver,
                key: PropertyKeyIr::StaticString(_),
                args,
            } if !tail => {
                self.effects = SmallInlineEffects::StrictActivation;
                self.expression(receiver, false)?;
                for argument in args {
                    self.expression(argument, false)?;
                }
                Some(())
            }
            ExprIr::CallIndirect {
                direct_eval: None,
                callee,
                this_arg: Some(receiver),
                args,
                static_regexp_compilation: None,
            } if !tail && matches!(callee.expr, ExprIr::PropertyRead { .. }) => {
                self.effects = SmallInlineEffects::StrictActivation;
                self.expression(callee, false)?;
                self.expression(receiver, false)?;
                for argument in args {
                    self.expression(argument, false)?;
                }
                Some(())
            }
            _ => None,
        }
    }
}
