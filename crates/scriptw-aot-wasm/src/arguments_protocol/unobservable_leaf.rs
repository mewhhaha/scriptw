//! Elide an activation's implicit arguments and mapping-only storage together.
//!
//! A sloppy callable's legacy `.arguments` is observable during reentry even
//! when its source never mentions `arguments`. This proof therefore admits
//! only synchronous bodies whose entire evaluation cannot invoke source code.
use super::*;
use scriptw_ir::{
    ClassElementExecutionKind, ExprIr, ObjectPropertyIr, OwnedEnvBindingIr, TypedExpr, ValueKind,
};

/// The builder consumes the source, arguments protocol and storage selection
/// from one plan, so arguments elision cannot leave a mapped cell dependency.
#[must_use = "a validated activation plan must be consumed by its source builder"]
pub(crate) struct UserFunctionActivationPlan<'a> {
    function: &'a FunctionIr,
    arguments: FunctionArgumentsProtocol,
    owned_bindings: &'a [OwnedEnvBindingIr],
}

impl<'a> UserFunctionActivationPlan<'a> {
    pub(crate) fn for_function(function: &'a FunctionIr) -> Result<Self, EmitError> {
        // Validate the ORIGINAL mapped slots before considering their elision.
        // An optimization must never admit malformed mapped-arguments IR.
        let arguments = FunctionArgumentsProtocol::for_user_function(function)?;
        Ok(match UnobservableLeafActivation::admit(function) {
            Some(leaf) => leaf.consume(arguments),
            None => match super::unobserved_strict::UnobservedStrictArguments::admit(function) {
                Some(proof) => proof.consume(arguments),
                None => Self::retaining_storage(function, arguments),
            },
        })
    }

    pub(super) fn retaining_storage(
        function: &'a FunctionIr,
        arguments: FunctionArgumentsProtocol,
    ) -> Self {
        Self {
            function,
            arguments,
            owned_bindings: &function.owned_env_bindings,
        }
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        &'a FunctionIr,
        FunctionArgumentsProtocol,
        &'a [OwnedEnvBindingIr],
    ) {
        (self.function, self.arguments, self.owned_bindings)
    }
}

/// Retains the exact admitted source until its arguments/storage plan is made.
/// Neither a boolean nor another FunctionIr can authorize the consumer.
struct UnobservableLeafActivation<'a> {
    function: &'a FunctionIr,
}

impl<'a> UnobservableLeafActivation<'a> {
    fn admit(function: &'a FunctionIr) -> Option<Self> {
        if function.protocol.flavor() != FunctionFlavor::Ordinary
            || function.protocol.execution_kind() != FunctionExecutionKind::Ordinary
            || function.protocol == FunctionProtocolIr::ClassConstructor
            || function.class_element_execution_kind != ClassElementExecutionKind::None
            || function.class_instance_element_plan.is_some()
            || function.is_derived_constructor
            || function.lexical_derived_activation.is_some()
            || function.generator_plan.is_some()
            || function.resumable_plan.is_some()
            || function.eval_environment.is_some()
            || !function.captured_bindings.is_empty()
            || function.captures_private_environment
            || function.body.lexical_environment.is_some()
            || !has_simple_parameter_list(function)
            || function.owned_env_bindings.iter().any(|binding| {
                !function
                    .params
                    .iter()
                    .any(|param| param.name == binding.name)
            })
        {
            return None;
        }
        let proof = Self { function };
        let mut statements = function.body.statements.as_slice();
        // Directive prologues and empty statements cannot run an observer.
        // All nonempty admitted bodies still use normal body compilation.
        while let Some((statement, rest)) = statements.split_first() {
            match statement {
                StatementIr::Empty => statements = rest,
                StatementIr::Expression(TypedExpr {
                    expr: ExprIr::String(_),
                    ..
                }) => statements = rest,
                StatementIr::Return(value) if rest.is_empty() && proof.expression(value) => {
                    return Some(proof);
                }
                _ => return None,
            }
        }
        Some(proof)
    }

    fn expression(&self, expression: &TypedExpr) -> bool {
        match &expression.expr {
            ExprIr::Undefined
            | ExprIr::ArrayHole
            | ExprIr::Null
            | ExprIr::Boolean(_)
            | ExprIr::Number(_)
            | ExprIr::BigInt(_)
            | ExprIr::String(_)
            | ExprIr::WellKnownSymbol(_) => true,
            ExprIr::Identifier(name) => self
                .function
                .params
                .iter()
                .any(|parameter| parameter.name == *name),
            ExprIr::ArrayLiteral(elements) => elements.iter().all(|value| self.expression(value)),
            ExprIr::ObjectLiteral(properties) => properties.iter().all(|property| match property {
                ObjectPropertyIr::Data { value, .. }
                | ObjectPropertyIr::NonEnumerableData { value, .. } => self.expression(value),
                ObjectPropertyIr::PrototypeSetter { .. }
                | ObjectPropertyIr::Spread { .. }
                | ObjectPropertyIr::ComputedData { .. }
                | ObjectPropertyIr::ComputedMethod { .. }
                | ObjectPropertyIr::ComputedGetter { .. }
                | ObjectPropertyIr::ComputedSetter { .. }
                | ObjectPropertyIr::Method { .. }
                | ObjectPropertyIr::Getter { .. }
                | ObjectPropertyIr::Setter { .. } => false,
            }),
            ExprIr::BinaryNumber { lhs, rhs, .. }
            | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
            | ExprIr::BitwiseNumeric { lhs, rhs, .. }
            | ExprIr::CompareNumber { lhs, rhs, .. }
                if lhs.kind == ValueKind::Number && rhs.kind == ValueKind::Number =>
            {
                self.expression(lhs) && self.expression(rhs)
            }
            ExprIr::UnaryPlus { expr }
            | ExprIr::UnaryMinusNumeric { expr }
            | ExprIr::UnaryBitwiseNumeric { expr, .. }
                if expr.kind == ValueKind::Number =>
            {
                self.expression(expr)
            }
            // These operations do not coerce an object or consult its properties.
            ExprIr::StrictEquality { lhs, rhs, .. } => self.expression(lhs) && self.expression(rhs),
            ExprIr::LogicalNot { expr } | ExprIr::Void { expr } | ExprIr::TypeOf { expr } => {
                self.expression(expr)
            }
            ExprIr::LogicalShortCircuit { lhs, rhs, .. } | ExprIr::Comma { lhs, rhs } => {
                self.expression(lhs) && self.expression(rhs)
            }
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                self.expression(condition)
                    && self.expression(then_expr)
                    && self.expression(else_expr)
            }
            // In particular, calls, property reads, coercions, function values,
            // dynamic bindings and arguments reads cannot acquire this proof.
            // New expression kinds conservatively retain the ordinary activation.
            _ => false,
        }
    }

    fn consume(self, mut arguments: FunctionArgumentsProtocol) -> UserFunctionActivationPlan<'a> {
        // Keep the separately admitted empty-strict-body entry/epilogue path.
        // A leaf proof only replaces materialization, never body execution.
        if matches!(
            &arguments.0,
            FunctionArgumentsState::Pending(FunctionArgumentsKind::Present(_))
        ) {
            arguments.0 = FunctionArgumentsState::Pending(FunctionArgumentsKind::UnobservableLeaf);
        }
        UserFunctionActivationPlan {
            function: self.function,
            arguments,
            owned_bindings: &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};

    fn with_function(source: &str, check: impl FnOnce(&FunctionIr)) {
        let parsed = parse(source, ParseOptions::script()).expect("leaf control parses");
        let program = scriptw_ir::lower(&parsed);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let function = program
            .script
            .as_ref()
            .unwrap()
            .functions
            .iter()
            .find(|function| function.name == "make")
            .expect("source function make");
        check(function);
    }

    #[test]
    fn leaf_plan_consumes_mapped_storage_without_treating_a_return_as_an_empty_body() {
        for source in [
            "function make(value) { return value; }",
            "function make(value, other, value) { return [value, other]; }",
            "function make(value) { return {first: value, nested: [value, null]}; }",
            "function make(value) { 'use strict'; return {value}; }",
        ] {
            with_function(source, |function| {
                let (admitted, mut arguments, bindings) =
                    UserFunctionActivationPlan::for_function(function)
                        .unwrap()
                        .into_parts();
                assert!(core::ptr::eq(admitted, function));
                assert!(bindings.is_empty(), "mapping-only slots must be elided");
                assert!(arguments.take_empty_strict_body().is_none());
                assert!(arguments
                    .take_for_binding()
                    .unwrap()
                    .into_present()
                    .is_none());
                assert!(arguments.take_for_binding().is_err());
            });
        }
    }

    #[test]
    fn reentry_capture_dynamic_binding_and_parameter_initialization_refuse_leaf_authority() {
        for source in [
            "function make(value) { return value(); }",
            "function make(value) { return value.x; }",
            "function make(value) { return +value; }",
            "function make(value) { return value + 1; }",
            "function make(value) { return arguments; }",
            "function make(value) { return () => arguments; }",
            "function make(value) { return () => value; }",
            "function make(value) { return eval('value'); }",
            "function make(value = 1) { return value; }",
            "function make(...value) { return value; }",
            "function make({value}) { return value; }",
            "function* make(value) { return value; }",
            "async function make(value) { return value; }",
            "class make { constructor(value) { return {value}; } }",
            "function make(value) { return {[value]: 1}; }",
            "function make(value) { return {...value}; }",
            "function make(value) { return {method() { return value; }}; }",
            "function make(value) { function child() { return value; } return value; }",
            "var captured; function make(value) { return captured; }",
        ] {
            with_function(source, |function| {
                assert!(
                    UnobservableLeafActivation::admit(function).is_none(),
                    "{source}"
                );
                let (_, mut arguments, bindings) =
                    UserFunctionActivationPlan::for_function(function)
                        .unwrap()
                        .into_parts();
                assert_eq!(bindings, function.owned_env_bindings.as_slice(), "{source}");
                assert!(
                    arguments
                        .take_for_binding()
                        .unwrap()
                        .into_present()
                        .is_some(),
                    "{source}"
                );
            });
        }
    }

    #[test]
    fn leaf_elision_still_validates_the_original_mapped_slot_ownership() {
        with_function("function make(value) { return {value}; }", |function| {
            let mut malformed = function.clone();
            malformed
                .owned_env_bindings
                .retain(|binding| binding.name != "value");
            let error = UserFunctionActivationPlan::for_function(&malformed)
                .err()
                .expect("a leaf cannot hide missing mapped storage");
            assert!(error
                .to_string()
                .contains("parameter `value` has no owned environment slot"));

            malformed.owned_env_bindings = function.owned_env_bindings.clone();
            malformed
                .owned_env_bindings
                .push(malformed.owned_env_bindings[0].clone());
            assert!(UserFunctionActivationPlan::for_function(&malformed).is_err());
        });
    }

    #[test]
    fn proven_number_operations_only_admit_number_operands_without_coercion() {
        with_function("function make(value) { return value * 2; }", |function| {
            let mut number = function.clone();
            let StatementIr::Return(value) = &mut number.body.statements[0] else {
                panic!("arithmetic control retains its return");
            };
            let ExprIr::CoerciveBinaryNumber { lhs, .. } = &mut value.expr else {
                panic!("dynamic parameter arithmetic retains coercion");
            };
            assert_eq!(lhs.kind, ValueKind::Dynamic);
            lhs.kind = ValueKind::Number;
            assert!(UnobservableLeafActivation::admit(&number).is_some());
            // Admission does not merely trust the arithmetic result's Number
            // kind: the dynamic operand can still call user valueOf/Symbol.toPrimitive.
            assert!(UnobservableLeafActivation::admit(function).is_none());
        });
    }
}
