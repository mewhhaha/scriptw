//! Proof of read-only access to a strict invocation's own arguments object.
use super::*;
use scriptw_ir::{
    BindingMode, BlockIr, ClassElementExecutionKind, EnvironmentBindingMutabilityIr,
    EnvironmentIdentifierOperationIr, EnvironmentIdentifierResolutionStart,
    EvalBindingDeclarationIr, EvalDeclarativeEnvironmentKindIr, EvalEnvironmentRoleIr, ExprIr,
    ForInitIr, ObjectPropertyIr, PropertyKeyIr, SpecOperationIr, Strictness, TypedExpr, ValueKind,
    LEXICAL_ARGUMENTS_NAME,
};

#[derive(Debug, Clone)]
pub(crate) struct StrictArgumentsProjection {
    source: scriptw_ir::FunctionId,
    named_own_binding: bool,
}

#[derive(Clone, Copy)]
pub(crate) enum ArgumentsProjectionRead {
    First,
    Length,
    /// `arguments[key]` for any other key expression: the passed value at an
    /// integer index below the count, otherwise the real object's [[Get]].
    Index,
    /// Other static keys. A canonical decimal name may have a literal index.
    Other,
}

#[derive(Clone, Copy)]
enum ReadDomain {
    AllProperties,
    LengthAndIndices,
}

impl StrictArgumentsProjection {
    pub(crate) fn admit(function: &FunctionIr) -> Option<Self> {
        Self::admit_read_domain(function, ReadDomain::AllProperties)
    }

    /// A counted local entry can project length and indexed reads. A computed
    /// key still needs the full property lookup when its value is not an
    /// in-range index.
    pub(crate) fn admit_local(function: &FunctionIr) -> Option<Self> {
        Self::admit_read_domain(function, ReadDomain::LengthAndIndices)
    }

    fn admit_read_domain(function: &FunctionIr, domain: ReadDomain) -> Option<Self> {
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
            domain,
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
            ExprIr::PropertyRead { target, key } if self.own_binding(target) => Some(match key {
                PropertyKeyIr::ArrayLength => ArgumentsProjectionRead::Length,
                PropertyKeyIr::StaticString(key) => {
                    Self::string_key(key).unwrap_or(ArgumentsProjectionRead::Other)
                }
                PropertyKeyIr::ArrayIndex(key) | PropertyKeyIr::StringExpr(key) => {
                    Self::key(key).unwrap_or(ArgumentsProjectionRead::Index)
                }
            }),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::GetV,
                operands,
            } => {
                let [target, key] = operands.as_slice() else {
                    return None;
                };
                if self.own_binding(target) {
                    Some(Self::key(key).unwrap_or(ArgumentsProjectionRead::Index))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// The key expression a computed own read evaluates, if any.
    pub(crate) fn read_key<'a>(&self, value: &'a TypedExpr) -> Option<&'a TypedExpr> {
        match &value.expr {
            ExprIr::PropertyRead {
                target,
                key: PropertyKeyIr::ArrayIndex(key) | PropertyKeyIr::StringExpr(key),
            } if self.own_binding(target) => Some(key),
            ExprIr::SpecOperation {
                operation: SpecOperationIr::GetV,
                operands,
            } => match operands.as_slice() {
                [target, key] if self.own_binding(target) => Some(key),
                _ => None,
            },
            _ => None,
        }
    }

    /// The actual argument position an admitted own read names when its key
    /// is a literal array index: `length` and computed keys answer `None`.
    pub(crate) fn literal_index(&self, value: &TypedExpr) -> Option<u32> {
        match self.read(value)? {
            ArgumentsProjectionRead::First => Some(0),
            ArgumentsProjectionRead::Length => None,
            ArgumentsProjectionRead::Other => {
                let ExprIr::PropertyRead {
                    key: PropertyKeyIr::StaticString(key),
                    ..
                } = &value.expr
                else {
                    return None;
                };
                Self::literal_index_string(key)
            }
            ArgumentsProjectionRead::Index => match &self.read_key(value)?.expr {
                ExprIr::Number(bits) => {
                    let index = f64::from_bits(*bits);
                    // ToPropertyKey(-0) is "0", like the First read.
                    (index.fract() == 0.0 && (0.0..f64::from(u32::MAX)).contains(&index))
                        .then_some(index as u32)
                }
                ExprIr::String(key) => Self::literal_index_string(key),
                _ => None,
            },
        }
    }

    fn literal_index_string(key: &str) -> Option<u32> {
        if (key.len() > 1 && key.starts_with('0')) || !key.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        key.parse::<u32>().ok().filter(|&index| index < u32::MAX)
    }

    fn is_local_read(&self, value: &TypedExpr, read: ArgumentsProjectionRead) -> bool {
        match read {
            ArgumentsProjectionRead::First | ArgumentsProjectionRead::Length => true,
            ArgumentsProjectionRead::Other => self.literal_index(value).is_some(),
            ArgumentsProjectionRead::Index => {
                self.read_key(value).is_some_and(|key| match &key.expr {
                    ExprIr::String(key) => Self::literal_index_string(key).is_some(),
                    ExprIr::Undefined | ExprIr::Null | ExprIr::Boolean(_) => false,
                    _ => key.kind != ValueKind::Symbol && !reads_symbol_iterator(key),
                })
            }
        }
    }

    /// The unmapped construction protocol a late read materializes with: the
    /// projection admits only strict functions, whose objects are unmapped.
    pub(crate) fn unmapped_protocol(&self) -> PresentArgumentsObjectProtocol {
        PresentArgumentsObjectProtocol::Unmapped(UnmappedArgumentsPlan { _private: () })
    }

    /// A read of the binding itself, as the base of an admitted read that an
    /// emitter fast path evaluates on its own.
    pub(crate) fn own_binding(&self, value: &TypedExpr) -> bool {
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

fn reads_symbol_iterator(value: &TypedExpr) -> bool {
    // A late-bound Symbol reference can leave both spellings of this member
    // as ordinary Get IR. Refuse the counted entry even when its result kind
    // is unknown. A computed key supplied by the caller still uses full Get.
    let base = match &value.expr {
        ExprIr::PropertyRead {
            target,
            key: PropertyKeyIr::StaticString(key),
        } if key == "iterator" => target.as_ref(),
        ExprIr::SpecOperation {
            operation: SpecOperationIr::GetV,
            operands,
        } => {
            let [base, key] = operands.as_slice() else {
                return false;
            };
            if !matches!(&key.expr, ExprIr::String(key) if key == "iterator") {
                return false;
            }
            base
        }
        _ => return false,
    };
    match &base.expr {
        ExprIr::Identifier(name)
        | ExprIr::GlobalPropertyRead { name }
        | ExprIr::GlobalIdentifierRead { name } => name == "Symbol",
        ExprIr::EnvironmentIdentifier(reference) => {
            reference.name == "Symbol"
                && matches!(reference.operation, EnvironmentIdentifierOperationIr::Read)
        }
        _ => false,
    }
}

fn other_name(name: &str) -> bool {
    name != "arguments" && name != LEXICAL_ARGUMENTS_NAME
}

/// A deliberately closed initial walk. Unknown statements, binding plans and
/// Reference producers refuse the whole invocation, never just one read.
struct ReadOnlyUses<'a> {
    proof: &'a StrictArgumentsProjection,
    domain: ReadDomain,
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
            | StatementIr::DeclarationEvaluation(value)
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
            StatementIr::Break { .. } | StatementIr::Continue { .. } => true,
            // Loops without their own lexical environment keep every read in
            // the function's single environment.
            StatementIr::While { condition, body } | StatementIr::DoWhile { condition, body } => {
                self.expression(condition, true) && self.statement(body)
            }
            StatementIr::For {
                init,
                test,
                update,
                body,
                lexical_environment: None,
            } => {
                init.as_ref().is_none_or(|init| match init {
                    ForInitIr::Expression(value) => self.expression(value, true),
                    ForInitIr::Var(declarations) => declarations.iter().all(|declaration| {
                        other_name(&declaration.name)
                            && declaration
                                .init
                                .as_ref()
                                .is_none_or(|value| self.expression(value, true))
                    }),
                    _ => false,
                }) && test
                    .as_ref()
                    .is_none_or(|value| self.expression(value, true))
                    && update
                        .as_ref()
                        .is_none_or(|value| self.expression(value, true))
                    && self.statement(body)
            }
            StatementIr::ForInObject {
                name,
                target,
                body,
                lexical_environment: None,
                ..
            } => other_name(name) && self.expression(target, true) && self.statement(body),
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
        if let Some(read) = self.proof.read(value) {
            self.reads += 1;
            // A computed key is evaluated first and may itself read the
            // binding.
            return reads
                && (matches!(self.domain, ReadDomain::AllProperties)
                    || self.proof.is_local_read(value, read))
                && self
                    .proof
                    .read_key(value)
                    .is_none_or(|key| self.expression(key, reads));
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
            // A Script global's PutValue, like a runtime Reference's above,
            // may run a setter but can reach this object only through a read.
            ExprIr::AssignIdentifier { name, value }
            | ExprIr::CompoundAssignIdentifier { name, value, .. }
            | ExprIr::GlobalPropertyWrite { name, value, .. } => {
                other_name(name) && self.expression(value, reads)
            }
            ExprIr::PropertyRead { target, key } => {
                self.expression(target, reads) && self.key(key, reads)
            }
            // A method call's receiver bound to a temporary: walked as a
            // receiver, where reads refuse.
            ExprIr::MaterializeBinding { name, value, body } => {
                other_name(name) && self.expression(value, false) && self.expression(body, reads)
            }
            // A write's target is walked like any operand: the binding itself
            // as a target refuses, a value read from it does not.
            ExprIr::PropertyWrite {
                target, key, value, ..
            } => {
                self.expression(target, reads)
                    && self.key(key, reads)
                    && self.expression(value, reads)
            }
            ExprIr::OrdinaryPropertyAssignment(assignment) => {
                self.expression(assignment.base_and_receiver(), reads)
                    && self.key(assignment.referenced_name(), reads)
                    && self.expression(assignment.rhs(), reads)
            }
            ExprIr::OrdinaryPropertyNumericUpdate(update) => {
                self.expression(update.base_and_receiver(), reads)
                    && self.key(update.referenced_name(), reads)
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
    fn owned_reads_and_existing_captures_keep_the_binding_and_admit_local_transport() {
        for options in [ParseOptions::module(), ParseOptions::script()] {
            let function = function(
                "var tick=1; function make(value,index) { 'use strict'; var original=value; value=index+tick; tick=(tick+1)&255; var row=arguments[0]; return ((row.x^value)+row.y+arguments.length+tick)&1023; } make;",
                options,
            );
            assert!(StrictArgumentsProjection::admit(&function).is_some());
            assert!(super::super::UnobservedStrictArguments::admit(&function).is_none());
            assert!(crate::function_entry::LocalArgumentsPlan::for_function(&function).is_some());
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
    fn scored_observer_has_one_admitted_projected_body() {
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
        assert!(crate::function_entry::LocalArgumentsPlan::for_function(admitted[0]).is_some());
    }

    #[test]
    fn incomplete_or_exposing_uses_refuse_the_entire_function() {
        for source in [
            "function make(a) { 'use strict'; return arguments; }",
            "function make(a) { 'use strict'; arguments[0]=a; return arguments.length; }",
            "function make(a) { 'use strict'; arguments.length=0; return arguments[0]; }",
            "function make(a) { 'use strict'; delete arguments[0]; return arguments.length; }",
            "function make(a) { 'use strict'; return 0 in arguments ? arguments.length : 0; }",
            "function make(a) { 'use strict'; return arguments[0](); }",
            "function make(a) { 'use strict'; return arguments[0]?.(); }",
            "function make(a) { 'use strict'; return arguments[0]`value`; }",
            "function make(a) { 'use strict'; return eval('arguments[0]'); }",
            "function make(a) { 'use strict'; return () => arguments[0]; }",
            "function make(a) { 'use strict'; function child(){return a;} return arguments.length; }",
            "function make(a=1) { return arguments[0]; }",
            "function make(...a) { return arguments.length; }",
            "function make({a}) { return arguments[0]; }",
            "async function make(a) { return arguments[0]; }",
            "function* make(a) { return arguments[0]; }",
            "function make(a) { 'use strict'; try { return arguments[0]; } finally { a(); } }",
            "function make(a) { 'use strict'; return arguments.hasOwnProperty(0); }",
            "function make(a) { 'use strict'; return arguments[0].call(a) + arguments.length; }",
        ] {
            let function = function(source, ParseOptions::module());
            assert!(
                StrictArgumentsProjection::admit(&function).is_none(),
                "{source}"
            );
            assert!(StrictArgumentsProjection::admit_local(&function).is_none(), "{source}");
        }
        for source in [
            "function make(a) { 'use strict'; return arguments[1]; }",
            "function make(a) { 'use strict'; return arguments['2']; }",
            "function make(a) { 'use strict'; return arguments[a]; }",
            "function make(a) { 'use strict'; return arguments[(a(),0)]; }",
            "function make(a) { 'use strict'; return arguments[-1]; }",
            "function make(a) { 'use strict'; return arguments[1.25]; }",
            "function make(a) { 'use strict'; return arguments[arguments.length - 1]; }",
            // A one-unit String comparison, which has its own fast path.
            "function make(a) { 'use strict'; return arguments[0] === 'a' && arguments[1] !== 'b'; }",
            "function make(a) { 'use strict'; for (var i = 0; i < arguments.length; i++) a(arguments[i]); }",
            "function make(t, c, d) { 'use strict'; var p = {}, n = arguments.length - 2, k; for (k in c) p[k] = c[k]; if (n === 1) p.children = d; else if (n > 1) { var a = Array(n); for (var i = 0; i < n; i++) a[i] = arguments[i + 2]; p.children = a; } return p; }",
            // A method call on another receiver, as createElement's own-key test.
            "function make(t, c) { 'use strict'; var p = {}, k; for (k in c) t.call(c, k) && (p[k] = c[k]); return arguments.length; }",
            "function make(t, c) { 'use strict'; return t.call(c, arguments[1], arguments.length); }",
            // React's createElement, as the retained bundle compiles it.
            "var Mt = Object.prototype.hasOwnProperty, jn = function () {}; function make(n,t,r){'use strict';var i,o={},e=null;if(t!=null)for(i in t.key!==void 0&&(e=\"\"+t.key),t)Mt.call(t,i)&&i!==\"key\"&&i!==\"__self\"&&i!==\"__source\"&&(o[i]=t[i]);var f=arguments.length-2;if(f===1)o.children=r;else if(1<f){for(var l=Array(f),c=0;c<f;c++)l[c]=arguments[c+2];o.children=l}return jn(n,e,o)}",
        ] {
            let function = function(source, ParseOptions::module());
            assert!(
                StrictArgumentsProjection::admit_local(&function).is_some(),
                "{source}"
            );
        }
        let sloppy = function(
            "function make(a) { a=7; return arguments[0]; }",
            ParseOptions::script(),
        );
        assert!(!sloppy.strict);
        assert!(StrictArgumentsProjection::admit_local(&sloppy).is_none());
    }

    #[test]
    fn explicitly_named_non_index_reads_keep_vector_projection() {
        for read in [
            "arguments.callee",
            "arguments['callee']",
            "arguments[Symbol.iterator]",
            "arguments[Symbol['iterator']]",
            "arguments['02']",
            "arguments[false]",
        ] {
            for options in [ParseOptions::module(), ParseOptions::script()] {
                let source = format!("function make(a) {{ 'use strict'; return {read}; }}");
                let function = function(&source, options);
                assert!(
                    StrictArgumentsProjection::admit(&function).is_some(),
                    "{source}"
                );
                assert!(
                    StrictArgumentsProjection::admit_local(&function).is_none(),
                    "{source}"
                );
            }
        }
    }

    #[test]
    fn named_implicit_binding_requires_the_actual_unique_owned_slot() {
        // A Script with a direct eval keeps named records; a Module owner
        // keeps its implicit binding private.
        let original = function(
            "function make(a) { 'use strict'; return arguments[0]+arguments.length; } function named() { eval(''); }",
            ParseOptions::script(),
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
