//! Strict callers hide legacy activation reflection. Only source arguments
//! access, direct eval or a newly created callable that captures the binding
//! can observe it; all ordinary activation storage and entry logic stay live.
use super::*;
use scriptw_ir::{
    BlockIr, ClassElementExecutionKind, DestructuringPropertyKeyIr, DestructuringTargetIr,
    EnvironmentIdentifierOperationIr, ExprIr, ForInitIr, ObjectDestructuringPatternIr,
    ObjectPropertyIr, PropertyKeyIr, SpecOperationIr, TypedExpr, LEXICAL_ARGUMENTS_NAME,
};

pub(crate) struct UnobservedStrictArguments<'a>(&'a FunctionIr);

impl<'a> UnobservedStrictArguments<'a> {
    pub(crate) fn admit(function: &'a FunctionIr) -> Option<Self> {
        // Strict ordinary functions hide legacy reflection; arrow functions
        // (strict or sloppy) never own `arguments`, `this`, or legacy reflection.
        let protocol_admitted = match function.protocol {
            FunctionProtocolIr::OrdinaryCallAndConstruct => function.strict,
            FunctionProtocolIr::Arrow => true,
            _ => false,
        };
        if !protocol_admitted
            || function.class_element_execution_kind != ClassElementExecutionKind::None
            || function.class_instance_element_plan.is_some()
            || function.lexical_derived_activation.is_some()
            || function.is_derived_constructor
            || function.generator_plan.is_some()
            || function.resumable_plan.is_some()
            // An object binding pattern is walked with the body, as its
            // ParameterInitialization statement. Initializers of the list
            // itself and rest parameters stay refused.
            || function
                .params
                .iter()
                .any(|parameter| parameter.is_rest || parameter.default_init.is_some())
            || function.captures_lexical_arguments
            || function
                .captured_bindings
                .iter()
                .any(|binding| !unobserved_name(&binding.source_name))
            || !BodyWalk::for_function(function).block(&function.body)
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

/// Sloppy ordinary functions whose own body cannot observe `arguments` (no
/// source `arguments` read, no direct eval, no child capturing `arguments`).
/// Only legacy `fn.arguments` reflection can observe their ArgumentsObject, so
/// they publish a lightweight `DeferredLegacyArguments` token and materialize
/// the mapped ArgumentsObject lazily on first reflection read.
pub(crate) struct UnobservedSloppyArguments<'a>(&'a FunctionIr);

impl<'a> UnobservedSloppyArguments<'a> {
    pub(crate) fn admit(function: &'a FunctionIr) -> Option<Self> {
        if function.strict
            || function.protocol != FunctionProtocolIr::OrdinaryCallAndConstruct
            || function.class_element_execution_kind != ClassElementExecutionKind::None
            || function.class_instance_element_plan.is_some()
            || function.lexical_derived_activation.is_some()
            || function.is_derived_constructor
            || function.generator_plan.is_some()
            || function.resumable_plan.is_some()
            || function.params.len() > 8
            || function
                .params
                .iter()
                .any(|parameter| parameter.is_rest || parameter.default_init.is_some())
            || function.captures_lexical_arguments
            || function
                .captured_bindings
                .iter()
                .any(|binding| !unobserved_name(&binding.source_name))
            || !BodyWalk::for_function(function).block(&function.body)
        {
            return None;
        }
        Some(Self(function))
    }

    pub(super) fn consume(
        self,
        mut arguments: FunctionArgumentsProtocol,
    ) -> UserFunctionActivationPlan<'a> {
        let deferred = match &arguments.0 {
            FunctionArgumentsState::Pending(FunctionArgumentsKind::Present(
                PresentArgumentsObjectProtocol::Mapped(plan),
            )) => DeferredLegacyArgumentsPlan::from_mapped(plan, self.0.params.len()),
            _ => None,
        };
        if let Some(plan) = deferred {
            arguments.0 = FunctionArgumentsState::Pending(FunctionArgumentsKind::DeferredLegacy(plan));
        }
        UserFunctionActivationPlan::retaining_storage(self.0, arguments)
    }
}

fn unobserved_name(name: &str) -> bool {
    name != "arguments" && name != LEXICAL_ARGUMENTS_NAME
}

/// The complete walk of one admitted body.
#[derive(Clone, Copy)]
struct BodyWalk {
    /// Whether a callable created in the body can reach the binding. An
    /// arrow function, or a direct eval inside one, resolves `arguments` to
    /// this activation's binding only through an environment slot, which
    /// capture analysis gives the activation whenever a nested function
    /// names `arguments` or holds a direct eval.
    child_callables: ChildCallables,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ChildCallables {
    /// No environment slot holds the binding, so no closure created here,
    /// and no direct eval running in one, can name it.
    CannotNameArguments,
    MayNameArguments,
}

impl BodyWalk {
    fn for_function(function: &FunctionIr) -> Self {
        let named = function
            .owned_env_bindings
            .iter()
            .any(|binding| !unobserved_name(&binding.name))
            || function
                .eval_environment
                .as_ref()
                .is_some_and(|role| match role {
                    scriptw_ir::EvalEnvironmentRoleIr::Declarative { bindings, .. } => bindings
                        .iter()
                        .any(|binding| !unobserved_name(&binding.source_name)),
                    scriptw_ir::EvalEnvironmentRoleIr::WithObject { .. } => true,
                });
        Self {
            child_callables: if named {
                ChildCallables::MayNameArguments
            } else {
                ChildCallables::CannotNameArguments
            },
        }
    }

    fn block(self, body: &BlockIr) -> bool {
        body.statements.iter().all(|value| self.statement(value))
    }

    fn statement(self, source: &StatementIr) -> bool {
        match source {
            StatementIr::Empty | StatementIr::Break { .. } | StatementIr::Continue { .. } => true,
            StatementIr::Expression(value)
            | StatementIr::DeclarationEvaluation(value)
            | StatementIr::Return(value)
            | StatementIr::Throw(value) => self.expression(value),
            StatementIr::Lexical { name, init, .. } => {
                unobserved_name(name) && self.expression(init)
            }
            StatementIr::Var(declarations) => declarations.iter().all(|declaration| {
                unobserved_name(&declaration.name)
                    && declaration
                        .init
                        .as_ref()
                        .is_none_or(|value| self.expression(value))
            }),
            StatementIr::Block(body) => self.block(body),
            StatementIr::LexicalBlock(statements)
            | StatementIr::ParameterInitialization { statements, .. } => {
                statements.iter().all(|value| self.statement(value))
            }
            StatementIr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expression(condition)
                    && self.statement(then_branch)
                    && else_branch
                        .as_deref()
                        .is_none_or(|value| self.statement(value))
            }
            StatementIr::While { condition, body } | StatementIr::DoWhile { condition, body } => {
                self.expression(condition) && self.statement(body)
            }
            StatementIr::For {
                init,
                test,
                update,
                body,
                ..
            } => {
                init.as_ref().is_none_or(|value| self.for_init(value))
                    && test.as_ref().is_none_or(|value| self.expression(value))
                    && update.as_ref().is_none_or(|value| self.expression(value))
                    && self.statement(body)
            }
            StatementIr::ForInObject {
                name, target, body, ..
            } => unobserved_name(name) && self.expression(target) && self.statement(body),
            StatementIr::TryCatch {
                try_block,
                catch_name,
                catch_source_name,
                catch_block,
                generator_plan: None,
                async_plan: None,
                ..
            } => {
                unobserved_name(catch_name)
                    && unobserved_name(catch_source_name)
                    && self.block(try_block)
                    && self.block(catch_block)
            }
            StatementIr::Switch {
                discriminant,
                lexical_declarations,
                cases,
                ..
            } => {
                self.expression(discriminant)
                    && lexical_declarations
                        .iter()
                        .all(|value| self.statement(value))
                    && cases.iter().all(|case| {
                        case.condition
                            .as_ref()
                            .is_none_or(|value| self.expression(value))
                            && self.block(&case.body)
                    })
            }
            // Refusal loses only this optimization. In particular, generated
            // callable/class bodies, resumable protocols and opaque plans stay out.
            _ => false,
        }
    }

    fn for_init(self, init: &ForInitIr) -> bool {
        match init {
            ForInitIr::Expression(value) => self.expression(value),
            ForInitIr::Lexical { name, init, .. } => unobserved_name(name) && self.expression(init),
            ForInitIr::LexicalBlock(bindings) => bindings
                .iter()
                .all(|binding| unobserved_name(&binding.name) && self.expression(&binding.init)),
            ForInitIr::Var(declarations) => declarations.iter().all(|declaration| {
                unobserved_name(&declaration.name)
                    && declaration
                        .init
                        .as_ref()
                        .is_none_or(|value| self.expression(value))
            }),
            ForInitIr::Statements(statements) => {
                statements.iter().all(|value| self.statement(value))
            }
            ForInitIr::SyncDisposable(_) | ForInitIr::AsyncDisposable(_) => false,
        }
    }

    /// BindingInitialization of an object pattern reads properties of the
    /// value, evaluates computed keys and initializers, and initializes the
    /// bound names. The reads may run getters and Proxy traps, which are
    /// other functions' code like any call. Only the walked expressions and
    /// the bound names belong to this activation.
    fn object_pattern(self, pattern: &ObjectDestructuringPatternIr) -> bool {
        pattern.properties.iter().all(|property| {
            (match &property.key {
                DestructuringPropertyKeyIr::Static(_) => true,
                DestructuringPropertyKeyIr::Computed(value) => self.expression(value),
            }) && self.binding_target(&property.target)
                && property
                    .default
                    .as_ref()
                    .is_none_or(|value| self.expression(value))
        }) && pattern
            .rest
            .as_ref()
            .is_none_or(|target| self.binding_target(target))
    }

    fn binding_target(self, target: &DestructuringTargetIr) -> bool {
        match target {
            DestructuringTargetIr::Binding { name, .. } => unobserved_name(name),
            DestructuringTargetIr::NestedObject(pattern) => self.object_pattern(pattern),
            // Assignment targets, resolved var references and iterator
            // patterns have no admitted walk.
            _ => false,
        }
    }

    fn key(self, key: &PropertyKeyIr) -> bool {
        match key {
            PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => true,
            PropertyKeyIr::StringExpr(value) | PropertyKeyIr::ArrayIndex(value) => {
                self.expression(value)
            }
        }
    }

    fn expression(self, value: &TypedExpr) -> bool {
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
                    && reference
                        .operation
                        .operands()
                        .all(|value| self.expression(value))
            }
            ExprIr::AssignIdentifier { name, value }
            | ExprIr::CompoundAssignIdentifier { name, value, .. }
            | ExprIr::GlobalPropertyWrite { name, value, .. } => {
                unobserved_name(name) && self.expression(value)
            }
            ExprIr::MaterializeBinding { name, value, body } => {
                unobserved_name(name) && self.expression(value) && self.expression(body)
            }
            ExprIr::ObjectDestructure { value, pattern } => {
                self.expression(value) && self.object_pattern(pattern)
            }
            ExprIr::PropertyRead {
                target,
                key: property,
            } => self.expression(target) && self.key(property),
            ExprIr::PropertyWrite {
                target,
                key: property,
                value,
                ..
            } => self.expression(target) && self.key(property) && self.expression(value),
            ExprIr::OrdinaryPropertyAssignment(assignment) => {
                self.expression(assignment.base_and_receiver())
                    && self.key(assignment.referenced_name())
                    && self.expression(assignment.rhs())
            }
            ExprIr::OrdinaryPropertyNumericUpdate(update) => {
                self.expression(update.base_and_receiver()) && self.key(update.referenced_name())
            }
            ExprIr::ArrayLiteral(values) => values.iter().all(|value| self.expression(value)),
            ExprIr::ObjectLiteral(properties) => properties.iter().all(|property| match property {
                ObjectPropertyIr::Data { value, .. }
                | ObjectPropertyIr::NonEnumerableData { value, .. } => self.expression(value),
                _ => false,
            }),
            ExprIr::CallIndirect {
                direct_eval: None,
                callee,
                this_arg,
                args,
                ..
            } => {
                self.expression(callee)
                    && this_arg
                        .as_deref()
                        .is_none_or(|value| self.expression(value))
                    && args.iter().all(|value| self.expression(value))
            }
            ExprIr::CallNamed { name, args } => {
                name != "eval"
                    && unobserved_name(name)
                    && args.iter().all(|value| self.expression(value))
            }
            ExprIr::CallMethod {
                receiver,
                key: property,
                args,
            } => {
                self.expression(receiver)
                    && self.key(property)
                    && args.iter().all(|value| self.expression(value))
            }
            ExprIr::Construct { callee, args, .. } => {
                self.expression(callee) && args.iter().all(|value| self.expression(value))
            }
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
            | ExprIr::InstanceOf { lhs, rhs } => self.expression(lhs) && self.expression(rhs),
            ExprIr::UnaryPlus { expr }
            | ExprIr::UnaryMinusNumeric { expr }
            | ExprIr::UnaryBitwiseNumeric { expr, .. }
            | ExprIr::LogicalNot { expr }
            | ExprIr::Void { expr }
            | ExprIr::TypeOf { expr } => self.expression(expr),
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                self.expression(condition)
                    && self.expression(then_expr)
                    && self.expression(else_expr)
            }
            ExprIr::SpecOperation {
                operation:
                    SpecOperationIr::GetV
                    | SpecOperationIr::StrictEqualityComparison
                    | SpecOperationIr::IsLooselyEqual,
                operands,
            } => operands.iter().all(|value| self.expression(value)),
            // A function value closes over this activation's environment
            // and nothing else of the invocation, and that environment has
            // no slot for the binding.
            ExprIr::FunctionValue(_) => self.child_callables == ChildCallables::CannotNameArguments,
            // No source arguments reads, direct eval, class execution or
            // uninspected child plan may authorize elision.
            _ => false,
        }
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
    fn ordinary_operand_walks_elide_five_formal_arguments_without_changing_storage() {
        for body in [
            "return row === value || row == value;",
            "row[key] = value; return row;",
            "return row[key]++;",
            "try { return row[key](value); } catch (error) { return error === value; }",
            "for (var name in row) value += row[name]; return value;",
            "try { row[key] = value; } catch (error) { value = error; } for (var name in row) row[name]++; return row[key] == value;",
        ] {
            let source = format!(
                "export function make(row, key, value, unused, extra) {{ {body} }}"
            );
            with_module(&source, |function| {
                assert_eq!(function.params.len(), 5);
                assert!(UnobservedStrictArguments::admit(function).is_some(), "{body}");
                let (actual, mut arguments, storage) =
                    UserFunctionActivationPlan::for_function(function)
                        .unwrap()
                        .into_parts();
                assert!(core::ptr::eq(actual, function));
                assert_eq!(storage, function.owned_env_bindings.as_slice());
                assert!(arguments.take_for_binding().unwrap().into_present().is_none());
                assert!(arguments.take_for_binding().is_err());
            });
        }
    }

    #[test]
    fn child_callables_that_cannot_name_the_binding_keep_the_omission() {
        // A child that names no `arguments` and holds no direct eval
        // captures only the bindings it reads.
        for source in [
            "export function make(input) { return function(){return input;}; }",
            "export function make(input) { return function(first){return first + input;}; }",
            "export function make(input) { return () => input; }",
            "export function make(input) { return () => () => input; }",
            "export function make(input) { return async function(){return input;}; }",
            "export function make(input) { return function*(){yield input;}; }",
            "export function make(input, other) { other.ping = function(){return make(input, other);}; return other; }",
            "export function make(input) { var record = {id: input, ping: function(){return record;}, next: null}; return record; }",
            "export function make(input) { for (var name in input) return () => name; }",
        ] {
            with_module(source, |function| {
                assert!(
                    UnobservedStrictArguments::admit(function).is_some(),
                    "{source}"
                );
                assert!(
                    !function
                        .owned_env_bindings
                        .iter()
                        .any(|binding| binding.name == LEXICAL_ARGUMENTS_NAME),
                    "{source}"
                );
                let (_, mut arguments, storage) =
                    UserFunctionActivationPlan::for_function(function)
                        .unwrap()
                        .into_parts();
                assert_eq!(storage, function.owned_env_bindings.as_slice());
                assert!(
                    arguments.take_for_binding().unwrap().into_present().is_none(),
                    "{source}"
                );
            });
        }
    }

    #[test]
    fn object_binding_patterns_are_walked_with_the_body() {
        // The pattern's own expressions and bound names are the only part of
        // BindingInitialization that belongs to the activation.
        for source in [
            "export function make({input}) { return input; }",
            "export function make({input}, other) { return input + other; }",
            "export function make({input: renamed, other}) { return renamed + other; }",
            "export function make({inner: {input}}) { return input; }",
            "export function make({input = 1}) { return input; }",
            "var key = 'k'; export function make({[key]: input}) { return input; }",
            "export function make({input, ...others}) { return others; }",
        ] {
            with_module(source, |function| {
                assert!(
                    function.body.statements.iter().any(|statement| matches!(
                        statement,
                        StatementIr::ParameterInitialization { .. }
                    )),
                    "{source}"
                );
                assert!(
                    UnobservedStrictArguments::admit(function).is_some(),
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
                        .is_none(),
                    "{source}"
                );
            });
        }
    }

    #[test]
    fn a_pattern_owner_that_creates_a_callable_keeps_its_named_arguments_slot() {
        // An owner with a binding pattern is not a module-local owner, so its
        // activation record keeps a slot for the implicit binding, and a
        // callable created in the body could name it through that record.
        for source in [
            "export function make({input}) { return function(){return input;}; }",
            "export function make({input}) { return () => input; }",
        ] {
            with_module(source, |function| {
                assert!(
                    function
                        .owned_env_bindings
                        .iter()
                        .any(|binding| binding.name == LEXICAL_ARGUMENTS_NAME),
                    "{source}"
                );
                assert!(
                    UnobservedStrictArguments::admit(function).is_none(),
                    "{source}"
                );
            });
        }
    }

    #[test]
    fn an_ordinary_child_that_names_its_own_binding_leaves_the_parent_unobserved() {
        // Without eval, a Module child's `arguments` is its own private
        // binding, so the parent keeps neither a slot nor an observer.
        for source in [
            "export function make(input) { return function(){return arguments[0];}; }",
            "export function make(input) { return function(){return () => arguments[0];}; }",
            "export function make(input) { return function*(){yield arguments[0];}; }",
        ] {
            with_module(source, |function| {
                assert!(
                    function
                        .owned_env_bindings
                        .iter()
                        .all(|binding| binding.name != LEXICAL_ARGUMENTS_NAME),
                    "{source}"
                );
                assert!(
                    UnobservedStrictArguments::admit(function).is_some(),
                    "{source}"
                );
            });
        }
    }

    #[test]
    fn a_child_callable_that_names_the_binding_owns_its_environment_slot() {
        // The slot is what refuses the proof: the walk never reads a child.
        // Capture analysis also gives the slot to an activation whose
        // ordinary child names its own `arguments` or holds a direct eval,
        // so those children keep materialization too.
        for source in [
            "export function make(input) { return () => arguments[0]; }",
            "export function make(input) { return () => () => arguments[0]; }",
            "export function make(input) { return () => eval(input); }",
            "export function make(input) { return () => () => eval(input); }",
            "export function make(input) { return function(){return eval('arguments[0]');}; }",
        ] {
            with_module(source, |function| {
                assert!(
                    function
                        .owned_env_bindings
                        .iter()
                        .any(|binding| binding.name == LEXICAL_ARGUMENTS_NAME),
                    "{source}"
                );
                assert!(
                    UnobservedStrictArguments::admit(function).is_none(),
                    "{source}"
                );
                let (_, mut arguments, _) = UserFunctionActivationPlan::for_function(function)
                    .unwrap()
                    .into_parts();
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
    fn ordinary_operand_walks_retain_every_nested_arguments_observer() {
        for body in [
            "return arguments[0] === row;",
            "return row === arguments[0];",
            "return arguments[0] == row;",
            "return row == arguments[0];",
            "arguments[0][key] = value;",
            "row[arguments[0]] = value;",
            "row[key] = arguments[0];",
            "arguments[0][key]++;",
            "row[arguments[0]]++;",
            "try { return arguments[0]; } catch (error) { return error; }",
            "try { return row[key]; } catch (error) { return arguments[0]; }",
            "for (var name in arguments[0]) value += name; return value;",
            "for (var name in row) value += arguments[0]; return value;",
            "try { return eval('arguments[0]'); } catch (error) { return error; }",
            "try { return row[key]; } catch (error) { return () => arguments[0]; }",
            "for (var name in row) return () => arguments.length + value;",
            // These opaque regions still have no admitted complete child walk.
            "try { return row; } finally { row[key] = value; }",
        ] {
            let source =
                format!("export function make(row, key, value, unused, extra) {{ {body} }}");
            with_module(&source, |function| {
                assert!(
                    UnobservedStrictArguments::admit(function).is_none(),
                    "{body}"
                );
                let (_, mut arguments, storage) =
                    UserFunctionActivationPlan::for_function(function)
                        .unwrap()
                        .into_parts();
                assert_eq!(storage, function.owned_env_bindings.as_slice());
                assert!(arguments
                    .take_for_binding()
                    .unwrap()
                    .into_present()
                    .is_some());
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
            "export function make(input) { return () => () => arguments[0]; }",
            "export function make(input) { return () => typeof arguments; }",
            "export function make(input) { return () => eval('arguments[0]'); }",
            "export function make(input) { return () => eval(input); }",
            "export function make(input) { return () => () => eval(input); }",
            "export function make(input) { return async () => arguments[0]; }",
            "export function make(input) { return [input, {deep: () => arguments}]; }",
            "export function make(input) { return () => function(){ return 1; } && arguments; }",
            "export function make(input) { return class { method() { return input; } }; }",
            "export function make(input) { return class { [arguments[0]]() {} }; }",
            "export function make(input) { return eval('arguments[0]'); }",
            "export function make(input) { return input[eval('arguments[0]')]; }",
            "export function make(input = arguments[0]) { return input; }",
            "export function make(...input) { return input; }",
            "export function make({input}, ...rest) { return input; }",
            "export function make({input} = arguments[0]) { return input; }",
            "export function make({input} = {}) { return input; }",
            "export function make({input = arguments[0]}) { return input; }",
            "export function make({[arguments[0]]: input}) { return input; }",
            "export function make({input = () => arguments[0]}) { return input; }",
            "export function make({input = eval('arguments[0]')}) { return input; }",
            "export function make({input}) { return arguments[0]; }",
            "export function make({input}) { return () => arguments[0]; }",
            "export function make({inner: {input = arguments}}) { return input; }",
            "export function make([input]) { return input; }",
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
