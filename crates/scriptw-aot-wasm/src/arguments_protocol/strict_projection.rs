//! A strict invocation whose own arguments object has only two read observers.
//! This does not authorize a different entry ABI or omit any activation cell.
use super::*;
use scriptw_ir::{
    BindingMode, BlockIr, ClassElementExecutionKind, EnvironmentBindingMutabilityIr,
    EnvironmentIdentifierOperationIr, EnvironmentIdentifierResolutionStart,
    EvalBindingDeclarationIr, EvalDeclarativeEnvironmentKindIr, EvalEnvironmentRoleIr, ExprIr,
    ObjectPropertyIr, PropertyKeyIr, SpecOperationIr, Strictness, TypedExpr,
    LEXICAL_ARGUMENTS_NAME,
};

pub(crate) struct StrictArgumentsProjection {
    source: scriptw_ir::FunctionId,
    named_own_binding: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum ArgumentsProjectionRead {
    First,
    Length,
}

impl StrictArgumentsProjection {
    pub(crate) fn admit(function: &FunctionIr) -> Option<Self> {
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
            || function.params.iter().any(|param| !other_name(&param.name))
            || function
                .captured_bindings
                .iter()
                .any(|binding| !other_name(&binding.name) || !other_name(&binding.source_name))
        {
            return None;
        }
        // Source ResolveBinding may name the implicit binding through its real
        // activation record. Authenticate the source/storage alias and unique
        // physical slot; metadata alone is not a direct-eval observer.
        let named_own_binding = match &function.eval_environment {
            None => false,
            Some(EvalEnvironmentRoleIr::Declarative {
                kind: EvalDeclarativeEnvironmentKindIr::Variable,
                bindings,
            }) => {
                let mut named = bindings
                    .iter()
                    .filter(|binding| binding.source_name == "arguments");
                let own = named.next()?;
                if named.next().is_some()
                    || own.mode != BindingMode::Let
                    || own.declaration != EvalBindingDeclarationIr::Variable
                    || bindings
                        .iter()
                        .filter(|binding| binding.slot == own.slot)
                        .count()
                        != 1
                {
                    return None;
                }
                let mut storage = function.owned_env_bindings.iter().filter(|binding| {
                    binding.name == LEXICAL_ARGUMENTS_NAME || binding.slot == own.slot
                });
                let binding = storage.next()?;
                if storage.next().is_some()
                    || binding.name != LEXICAL_ARGUMENTS_NAME
                    || binding.slot != own.slot
                    || binding.mutability != EnvironmentBindingMutabilityIr::Mutable
                {
                    return None;
                }
                true
            }
            Some(_) => return None,
        };
        let proof = Self {
            source: function.id.clone(),
            named_own_binding,
        };
        let mut walk = ReadOnlyUses {
            proof: &proof,
            reads: 0,
        };
        if !walk.block(&function.body) || walk.reads == 0 {
            return None;
        }
        Some(proof)
    }

    pub(crate) fn require_source(&self, source: &scriptw_ir::FunctionId) {
        assert_eq!(
            &self.source, source,
            "arguments projection belongs to this exact source body"
        );
    }

    pub(crate) fn read(&self, value: &TypedExpr) -> Option<ArgumentsProjectionRead> {
        match &value.expr {
            ExprIr::PropertyRead { target, key } if self.own_binding(target) => match key {
                PropertyKeyIr::ArrayLength => Some(ArgumentsProjectionRead::Length),
                PropertyKeyIr::StaticString(key) => Self::string_key(key),
                PropertyKeyIr::ArrayIndex(key) | PropertyKeyIr::StringExpr(key) => Self::key(key),
            },
            ExprIr::SpecOperation {
                operation: SpecOperationIr::GetV,
                operands,
            } => {
                let [target, key] = operands.as_slice() else {
                    return None;
                };
                if self.own_binding(target) {
                    Self::key(key)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn own_binding(&self, value: &TypedExpr) -> bool {
        match &value.expr {
            ExprIr::Arguments => true,
            ExprIr::Identifier(name) => name == LEXICAL_ARGUMENTS_NAME,
            ExprIr::EnvironmentIdentifier(reference) => {
                self.named_own_binding
                    && reference.name == "arguments"
                    && reference.strictness == Strictness::Strict
                    && matches!(&reference.operation, EnvironmentIdentifierOperationIr::Read)
                    && reference.resolution_start()
                        == EnvironmentIdentifierResolutionStart::CurrentEnvironment
            }
            _ => false,
        }
    }

    fn string_key(key: &str) -> Option<ArgumentsProjectionRead> {
        match key {
            "0" => Some(ArgumentsProjectionRead::First),
            "length" => Some(ArgumentsProjectionRead::Length),
            _ => None,
        }
    }

    fn key(key: &TypedExpr) -> Option<ArgumentsProjectionRead> {
        match &key.expr {
            ExprIr::String(key) => Self::string_key(key),
            ExprIr::Number(bits) if f64::from_bits(*bits) == 0.0 => {
                Some(ArgumentsProjectionRead::First)
            }
            _ => None,
        }
    }
}

fn other_name(name: &str) -> bool {
    name != "arguments" && name != LEXICAL_ARGUMENTS_NAME
}

/// A deliberately closed initial walk. Unknown statements, binding plans and
/// Reference producers refuse the whole invocation, never just one read.
struct ReadOnlyUses<'a> {
    proof: &'a StrictArgumentsProjection,
    reads: usize,
}

impl ReadOnlyUses<'_> {
    fn block(&mut self, block: &BlockIr) -> bool {
        block.lexical_environment.is_none()
            && block
                .statements
                .iter()
                .all(|statement| self.statement(statement))
    }

    fn statement(&mut self, statement: &StatementIr) -> bool {
        match statement {
            StatementIr::Empty => true,
            StatementIr::Expression(value)
            | StatementIr::Return(value)
            | StatementIr::Throw(value) => self.expression(value, true),
            StatementIr::Var(declarations) => declarations.iter().all(|declaration| {
                other_name(&declaration.name)
                    && declaration
                        .init
                        .as_ref()
                        .is_none_or(|value| self.expression(value, true))
            }),
            StatementIr::Block(block) => self.block(block),
            StatementIr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.expression(condition, true)
                    && self.statement(then_branch)
                    && else_branch
                        .as_deref()
                        .is_none_or(|statement| self.statement(statement))
            }
            _ => false,
        }
    }

    fn key(&mut self, key: &PropertyKeyIr, reads: bool) -> bool {
        match key {
            PropertyKeyIr::StaticString(_) | PropertyKeyIr::ArrayLength => true,
            PropertyKeyIr::StringExpr(key) | PropertyKeyIr::ArrayIndex(key) => {
                self.expression(key, reads)
            }
        }
    }

    fn expression(&mut self, value: &TypedExpr, reads: bool) -> bool {
        if self.proof.read(value).is_some() {
            self.reads += 1;
            return reads;
        }
        match &value.expr {
            ExprIr::Undefined
            | ExprIr::Null
            | ExprIr::Boolean(_)
            | ExprIr::Number(_)
            | ExprIr::BigInt(_)
            | ExprIr::String(_)
            | ExprIr::WellKnownSymbol(_)
            | ExprIr::This
            | ExprIr::NewTarget => true,
            ExprIr::Identifier(name)
            | ExprIr::GlobalPropertyRead { name }
            | ExprIr::GlobalIdentifierRead { name }
            | ExprIr::TypeOfUnresolvedIdentifier { name }
            | ExprIr::UpdateIdentifier { name, .. } => other_name(name),
            ExprIr::EnvironmentIdentifier(reference) => {
                other_name(&reference.name)
                    && match &reference.operation {
                        EnvironmentIdentifierOperationIr::Read
                        | EnvironmentIdentifierOperationIr::Typeof
                        | EnvironmentIdentifierOperationIr::Assign { .. }
                        | EnvironmentIdentifierOperationIr::Update { .. }
                        | EnvironmentIdentifierOperationIr::EagerCompound { .. }
                        | EnvironmentIdentifierOperationIr::LogicalCompound { .. }
                        | EnvironmentIdentifierOperationIr::Call {
                            direct_eval: None, ..
                        } => reference
                            .operation
                            .operands()
                            .all(|value| self.expression(value, reads)),
                        _ => false,
                    }
            }
            ExprIr::AssignIdentifier { name, value }
            | ExprIr::CompoundAssignIdentifier { name, value, .. } => {
                other_name(name) && self.expression(value, reads)
            }
            ExprIr::PropertyRead { target, key } => {
                self.expression(target, reads) && self.key(key, reads)
            }
            ExprIr::ArrayLiteral(values) => {
                values.iter().all(|value| self.expression(value, reads))
            }
            ExprIr::ObjectLiteral(properties) => properties.iter().all(|property| match property {
                ObjectPropertyIr::Data { value, .. }
                | ObjectPropertyIr::NonEnumerableData { value, .. } => {
                    self.expression(value, reads)
                }
                _ => false,
            }),
            // A property used as a callee or retained receiver can expose the
            // arguments object through `this`, even though GetValue is safe.
            ExprIr::CallIndirect {
                direct_eval: None,
                callee,
                this_arg,
                args,
                ..
            } => {
                self.expression(callee, false)
                    && this_arg
                        .as_deref()
                        .is_none_or(|value| self.expression(value, false))
                    && args.iter().all(|value| self.expression(value, reads))
            }
            ExprIr::CallNamed { name, args } => {
                name != "eval"
                    && other_name(name)
                    && args.iter().all(|value| self.expression(value, reads))
            }
            ExprIr::CallMethod {
                receiver,
                key,
                args,
            } => {
                self.expression(receiver, false)
                    && self.key(key, false)
                    && args.iter().all(|value| self.expression(value, reads))
            }
            ExprIr::Construct { callee, args, .. } => {
                self.expression(callee, false)
                    && args.iter().all(|value| self.expression(value, reads))
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
            | ExprIr::InstanceOf { lhs, rhs } => {
                self.expression(lhs, reads) && self.expression(rhs, reads)
            }
            ExprIr::UnaryPlus { expr }
            | ExprIr::UnaryMinusNumeric { expr }
            | ExprIr::UnaryBitwiseNumeric { expr, .. }
            | ExprIr::LogicalNot { expr }
            | ExprIr::Void { expr }
            | ExprIr::TypeOf { expr } => self.expression(expr, reads),
            ExprIr::Conditional {
                condition,
                then_expr,
                else_expr,
            } => {
                self.expression(condition, reads)
                    && self.expression(then_expr, reads)
                    && self.expression(else_expr, reads)
            }
            ExprIr::SpecOperation {
                operation:
                    SpecOperationIr::GetV
                    | SpecOperationIr::StrictEqualityComparison
                    | SpecOperationIr::IsLooselyEqual,
                operands,
            } => operands.iter().all(|value| self.expression(value, reads)),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scriptw_front::{parse, ParseOptions};

    fn function(source: &str, options: ParseOptions) -> FunctionIr {
        let program = scriptw_ir::lower(&parse(source, options).unwrap());
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        program
            .script
            .unwrap()
            .functions
            .into_iter()
            .find(|function| function.name == "make")
            .unwrap()
    }

    #[test]
    fn snapshots_admit_owned_reads_and_existing_captures_but_never_local_entry() {
        for options in [ParseOptions::module(), ParseOptions::script()] {
            let function = function(
                "var tick=1; function make(value,index) { 'use strict'; var original=value; value=index+tick; tick=(tick+1)&255; var row=arguments[0]; return ((row.x^value)+row.y+arguments.length+tick)&1023; } make;",
                options,
            );
            assert!(StrictArgumentsProjection::admit(&function).is_some());
            assert!(super::super::UnobservedStrictArguments::admit(&function).is_none());
            assert!(crate::function_entry::LocalArgumentsPlan::for_function(&function).is_none());
            let (_, mut arguments, storage) = UserFunctionActivationPlan::for_function(&function)
                .unwrap()
                .into_parts();
            assert_eq!(storage, function.owned_env_bindings.as_slice());
            assert!(matches!(
                arguments.take_for_binding().unwrap().into_present(),
                Some(PresentArgumentsObjectProtocol::Unmapped(_))
            ));
        }
    }

    #[test]
    fn unchanged_scored_observer_has_one_admitted_vector_body() {
        let source = include_str!("../../tests/fixtures/strict_arguments_observed.js");
        let program = scriptw_ir::lower(&parse(source, ParseOptions::module()).unwrap());
        assert!(program.is_wasm_supported(), "{:?}", program.diagnostics);
        let admitted = program
            .script
            .as_ref()
            .unwrap()
            .functions
            .iter()
            .filter(|function| StrictArgumentsProjection::admit(function).is_some())
            .collect::<Vec<_>>();
        assert_eq!(admitted.len(), 1);
        assert_eq!(admitted[0].params.len(), 2);
        assert!(admitted[0]
            .captured_bindings
            .iter()
            .any(|binding| binding.source_name == "tick"));
        assert!(crate::function_entry::LocalArgumentsPlan::for_function(admitted[0]).is_none());
    }

    #[test]
    fn incomplete_or_exposing_uses_refuse_the_entire_function() {
        for source in [
            "function make(a) { 'use strict'; return arguments; }",
            "function make(a) { 'use strict'; return arguments[1]; }",
            "function make(a) { 'use strict'; return arguments[a]; }",
            "function make(a) { 'use strict'; return arguments[(a(),0)]; }",
            "function make(a) { 'use strict'; arguments[0]=a; return arguments.length; }",
            "function make(a) { 'use strict'; arguments.length=0; return arguments[0]; }",
            "function make(a) { 'use strict'; delete arguments[0]; return arguments.length; }",
            "function make(a) { 'use strict'; return arguments[0](); }",
            "function make(a) { 'use strict'; return arguments[0]?.(); }",
            "function make(a) { 'use strict'; return arguments[0]`value`; }",
            "function make(a) { 'use strict'; return arguments.callee; }",
            "function make(a) { 'use strict'; return eval('arguments[0]'); }",
            "function make(a) { 'use strict'; return () => arguments[0]; }",
            "function make(a) { 'use strict'; function child(){return a;} return arguments.length; }",
            "function make(a=1) { return arguments[0]; }",
            "function make(...a) { return arguments.length; }",
            "function make({a}) { return arguments[0]; }",
            "async function make(a) { return arguments[0]; }",
            "function* make(a) { return arguments[0]; }",
            "function make(a) { 'use strict'; try { return arguments[0]; } finally { a(); } }",
        ] {
            let function = function(source, ParseOptions::module());
            assert!(
                StrictArgumentsProjection::admit(&function).is_none(),
                "{source}"
            );
        }
        let sloppy = function(
            "function make(a) { a=7; return arguments[0]; }",
            ParseOptions::script(),
        );
        assert!(!sloppy.strict);
        assert!(StrictArgumentsProjection::admit(&sloppy).is_none());
    }

    #[test]
    fn named_implicit_binding_requires_the_actual_unique_owned_slot() {
        let original = function(
            "function make(a) { return arguments[0]+arguments.length; }",
            ParseOptions::module(),
        );
        assert!(StrictArgumentsProjection::admit(&original).is_some());
        assert!(
            original.eval_environment.is_some(),
            "this control exercises named storage"
        );
        let mut missing = original.clone();
        missing
            .owned_env_bindings
            .retain(|binding| binding.name != LEXICAL_ARGUMENTS_NAME);
        assert!(StrictArgumentsProjection::admit(&missing).is_none());
        let mut renamed = original.clone();
        renamed
            .owned_env_bindings
            .iter_mut()
            .find(|binding| binding.name == LEXICAL_ARGUMENTS_NAME)
            .unwrap()
            .name = "other".into();
        assert!(StrictArgumentsProjection::admit(&renamed).is_none());
        let mut alias = original.clone();
        let mut duplicate = alias
            .owned_env_bindings
            .iter()
            .find(|binding| binding.name == LEXICAL_ARGUMENTS_NAME)
            .unwrap()
            .clone();
        duplicate.name = "alias".into();
        alias.owned_env_bindings.push(duplicate);
        assert!(StrictArgumentsProjection::admit(&alias).is_none());
    }
}
