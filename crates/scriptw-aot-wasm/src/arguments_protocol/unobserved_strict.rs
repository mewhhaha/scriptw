//! Strict callers hide legacy activation reflection. Only source arguments
//! access, direct eval or a newly created capturing callable can observe this
//! implicit binding; all ordinary activation storage and entry logic stay live.
use super::*;
use scriptw_ir::{
    BlockIr, ClassElementExecutionKind, EnvironmentIdentifierOperationIr, ExprIr, ForInitIr,
    ObjectPropertyIr, PropertyKeyIr, SpecOperationIr, TypedExpr, LEXICAL_ARGUMENTS_NAME,
};

pub(super) struct UnobservedStrictArguments<'a>(&'a FunctionIr);

impl<'a> UnobservedStrictArguments<'a> {
    pub(super) fn admit(function: &'a FunctionIr) -> Option<Self> {
        if !function.strict
            || function.protocol != FunctionProtocolIr::OrdinaryCallAndConstruct
            || function.class_element_execution_kind != ClassElementExecutionKind::None
            || function.class_instance_element_plan.is_some()
            || function.lexical_derived_activation.is_some()
            || function.is_derived_constructor
            || function.generator_plan.is_some()
            || function.resumable_plan.is_some()
            || !has_simple_parameter_list(function)
            || function.captures_lexical_arguments
            || function
                .captured_bindings
                .iter()
                .any(|binding| !unobserved_name(&binding.source_name))
            || !block(&function.body)
        {
            return None;
        }
        Some(Self(function))
    }

    pub(super) fn consume(
        self,
        mut arguments: FunctionArgumentsProtocol,
    ) -> UserFunctionActivationPlan<'a> {
        if matches!(
            &arguments.0,
            FunctionArgumentsState::Pending(FunctionArgumentsKind::Present(
                PresentArgumentsObjectProtocol::Unmapped(_)
            ))
        ) {
            arguments.0 = FunctionArgumentsState::Pending(FunctionArgumentsKind::UnobservedStrict);
        }
        UserFunctionActivationPlan::retaining_storage(self.0, arguments)
    }
}

fn unobserved_name(name: &str) -> bool {
    name != "arguments" && name != LEXICAL_ARGUMENTS_NAME
}

fn block(body: &BlockIr) -> bool {
    body.statements.iter().all(statement)
}

fn statement(source: &StatementIr) -> bool {
    match source {
        StatementIr::Empty | StatementIr::Break { .. } | StatementIr::Continue { .. } => true,
        StatementIr::Expression(value)
        | StatementIr::DeclarationEvaluation(value)
        | StatementIr::Return(value)
        | StatementIr::Throw(value) => expression(value),
        StatementIr::Lexical { name, init, .. } => unobserved_name(name) && expression(init),
        StatementIr::Var(declarations) => declarations.iter().all(|declaration| {
            unobserved_name(&declaration.name) && declaration.init.as_ref().is_none_or(expression)
        }),
        StatementIr::Block(body) => block(body),
        StatementIr::LexicalBlock(statements) => statements.iter().all(statement),
        StatementIr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expression(condition)
                && statement(then_branch)
                && else_branch.as_deref().is_none_or(statement)
        }
        StatementIr::While { condition, body } | StatementIr::DoWhile { condition, body } => {
            expression(condition) && statement(body)
        }
        StatementIr::For {
            init,
            test,
            update,
            body,
            ..
        } => {
            init.as_ref().is_none_or(for_init)
                && test.as_ref().is_none_or(expression)
                && update.as_ref().is_none_or(expression)
                && statement(body)
        }
        StatementIr::Switch {
            discriminant,
            lexical_declarations,
            cases,
            ..
        } => {
            expression(discriminant)
                && lexical_declarations.iter().all(statement)
                && cases
                    .iter()
                    .all(|case| case.condition.as_ref().is_none_or(expression) && block(&case.body))
        }
        // Refusal loses only this optimization. In particular, generated
        // callable/class bodies, resumable protocols and opaque plans stay out.
        _ => false,
    }
}

fn for_init(init: &ForInitIr) -> bool {
    match init {
        ForInitIr::Expression(value) => expression(value),
        ForInitIr::Lexical { name, init, .. } => unobserved_name(name) && expression(init),
        ForInitIr::LexicalBlock(bindings) => bindings
            .iter()
            .all(|binding| unobserved_name(&binding.name) && expression(&binding.init)),
        ForInitIr::Var(declarations) => declarations.iter().all(|declaration| {
            unobserved_name(&declaration.name) && declaration.init.as_ref().is_none_or(expression)
        }),
        ForInitIr::Statements(statements) => statements.iter().all(statement),
        ForInitIr::SyncDisposable(_) | ForInitIr::AsyncDisposable(_) => false,
    }
}

fn key(key: &PropertyKeyIr) -> bool {
    match key {
        PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => true,
        PropertyKeyIr::StringExpr(value) | PropertyKeyIr::ArrayIndex(value) => expression(value),
    }
}

fn expression(value: &TypedExpr) -> bool {
    match &value.expr {
        ExprIr::Undefined
        | ExprIr::ArrayHole
        | ExprIr::Null
        | ExprIr::Boolean(_)
        | ExprIr::Number(_)
        | ExprIr::BigInt(_)
        | ExprIr::String(_)
        | ExprIr::WellKnownSymbol(_)
        | ExprIr::This
        | ExprIr::NewTarget
        | ExprIr::ExecutionGlobalObject
        | ExprIr::RegExpLiteral { .. } => true,
        ExprIr::Identifier(name)
        | ExprIr::GlobalPropertyRead { name }
        | ExprIr::GlobalIdentifierRead { name }
        | ExprIr::TypeOfUnresolvedIdentifier { name }
        | ExprIr::UpdateIdentifier { name, .. } => unobserved_name(name),
        ExprIr::EnvironmentIdentifier(reference) => {
            unobserved_name(&reference.name)
                && !matches!(
                    &reference.operation,
                    EnvironmentIdentifierOperationIr::Call {
                        direct_eval: Some(_),
                        ..
                    }
                )
                && reference.operation.operands().all(expression)
        }
        ExprIr::AssignIdentifier { name, value }
        | ExprIr::CompoundAssignIdentifier { name, value, .. }
        | ExprIr::GlobalPropertyWrite { name, value, .. } => {
            unobserved_name(name) && expression(value)
        }
        ExprIr::MaterializeBinding { name, value, body } => {
            unobserved_name(name) && expression(value) && expression(body)
        }
        ExprIr::PropertyRead {
            target,
            key: property,
        } => expression(target) && key(property),
        ExprIr::PropertyWrite {
            target,
            key: property,
            value,
            ..
        } => expression(target) && key(property) && expression(value),
        ExprIr::ArrayLiteral(values) => values.iter().all(expression),
        ExprIr::ObjectLiteral(properties) => properties.iter().all(|property| match property {
            ObjectPropertyIr::Data { value, .. }
            | ObjectPropertyIr::NonEnumerableData { value, .. } => expression(value),
            _ => false,
        }),
        ExprIr::CallIndirect {
            direct_eval: None,
            callee,
            this_arg,
            args,
            ..
        } => {
            expression(callee)
                && this_arg.as_deref().is_none_or(expression)
                && args.iter().all(expression)
        }
        ExprIr::CallNamed { name, args } => {
            name != "eval" && unobserved_name(name) && args.iter().all(expression)
        }
        ExprIr::CallMethod {
            receiver,
            key: property,
            args,
        } => expression(receiver) && key(property) && args.iter().all(expression),
        ExprIr::Construct { callee, args, .. } => expression(callee) && args.iter().all(expression),
        ExprIr::BinaryNumber { lhs, rhs, .. }
        | ExprIr::CoerciveBinaryNumber { lhs, rhs, .. }
        | ExprIr::CoerciveAdd { lhs, rhs }
        | ExprIr::BitwiseNumeric { lhs, rhs, .. }
        | ExprIr::CompareNumber { lhs, rhs, .. }
        | ExprIr::CompareValue { lhs, rhs, .. }
        | ExprIr::StrictEquality { lhs, rhs, .. }
        | ExprIr::LooseEquality { lhs, rhs, .. }
        | ExprIr::LogicalShortCircuit { lhs, rhs, .. }
        | ExprIr::Comma { lhs, rhs }
        | ExprIr::StringConcat { lhs, rhs }
        | ExprIr::In { lhs, rhs }
        | ExprIr::InstanceOf { lhs, rhs } => expression(lhs) && expression(rhs),
        ExprIr::UnaryPlus { expr }
        | ExprIr::UnaryMinusNumeric { expr }
        | ExprIr::UnaryBitwiseNumeric { expr, .. }
        | ExprIr::LogicalNot { expr }
        | ExprIr::Void { expr }
        | ExprIr::TypeOf { expr } => expression(expr),
        ExprIr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => expression(condition) && expression(then_expr) && expression(else_expr),
        ExprIr::SpecOperation {
            operation: SpecOperationIr::GetV,
            operands,
        } => operands.iter().all(expression),
        // No source arguments reads, direct eval, capturing callable creation,
        // class execution or uninspected child plan may authorize elision.
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};

    fn with_module(source: &str, check: impl FnOnce(&FunctionIr)) {
        let parsed = parse(source, ParseOptions::module()).unwrap();
        let program = scriptw_ir::lower(&parsed);
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let function = program
            .script
            .as_ref()
            .unwrap()
            .functions
            .iter()
            .find(|function| function.name == "make")
            .unwrap();
        check(function);
    }

    #[test]
    fn strict_observer_absence_allows_capture_reads_coercion_calls_and_control_flow() {
        for source in [
            "var tick=1; export function make(row, index) { var original=row; row=index+tick; tick=(tick+1)&255; return ((original.x^row)+original.y+tick)&1023; }",
            "var steps; export function make(index,row) { return steps[index&1](row); }",
            "var steps; export function make(index,row) { switch(index&3) { case 0:return steps[0](row); default:return steps[1](row); } }",
            "export function make(input) { var sum=0; for(var i=0;i<4;i++) sum+=input[i]; return sum; }",
            "export function make(input) { return input[(input.key)]; }",
        ] {
            with_module(source, |function| {
                assert!(UnobservedStrictArguments::admit(function).is_some(), "{source}");
                let (actual, mut arguments, storage) = UserFunctionActivationPlan::for_function(function).unwrap().into_parts();
                assert!(core::ptr::eq(actual, function));
                assert_eq!(storage, function.owned_env_bindings.as_slice());
                assert!(arguments.take_for_binding().unwrap().into_present().is_none());
                assert!(arguments.take_for_binding().is_err());
            });
        }
    }

    #[test]
    fn source_arguments_eval_and_child_callable_observers_keep_materialization() {
        for source in [
            "export function make(input) { return arguments[0]; }",
            "export function make(input) { return typeof arguments; }",
            "export function make(input) { return input[arguments[0]]; }",
            "export function make(input) { return input.call(arguments, input); }",
            "export function make(input) { return () => arguments[0]; }",
            "export function make(input) { return function(){return input;}; }",
            "export function make(input) { return eval('arguments[0]'); }",
            "export function make(input) { return input[eval('arguments[0]')]; }",
            "export function make(input = arguments[0]) { return input; }",
            "export function make(...input) { return input; }",
            "export function make({input}) { return input; }",
            "export function* make(input) { return input.x; }",
            "export async function make(input) { return input.x; }",
        ] {
            with_module(source, |function| {
                assert!(
                    UnobservedStrictArguments::admit(function).is_none(),
                    "{source}"
                );
                let (_, mut arguments, storage) =
                    UserFunctionActivationPlan::for_function(function)
                        .unwrap()
                        .into_parts();
                assert_eq!(storage, function.owned_env_bindings.as_slice());
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
}
