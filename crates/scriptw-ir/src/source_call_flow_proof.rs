use crate::{
    ArrayAccumulationElementIr, ArrayAccumulationIr, ArrayAccumulationTargetIr, BlockIr, ExprIr,
    ForInitIr, FunctionParamIr, KindSet, ObjectPropertyIr, SpecOperationIr, StatementIr,
    ToPrimitiveHint, TypedExpr,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceCallFlowState {
    Unobserved,
    ProvenNoFlowInvalidation,
    MayInvalidateCallerFlow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SourceCallFlowEffects(SourceCallFlowState);

impl SourceCallFlowEffects {
    pub(crate) const fn unobserved() -> Self {
        Self(SourceCallFlowState::Unobserved)
    }

    pub(crate) const fn may_invalidate_caller_flow() -> Self {
        Self(SourceCallFlowState::MayInvalidateCallerFlow)
    }

    pub(crate) fn for_finalized_invocation(params: &[FunctionParamIr], body: &BlockIr) -> Self {
        match prove_no_caller_flow_invalidation(params, body) {
            Some(proof) => Self::from_proof(proof),
            None => Self::may_invalidate_caller_flow(),
        }
    }

    /// The same proof used by a complete invocation, consumed a statement at a
    /// time by inference. No source effect rule is duplicated by that consumer.
    pub(crate) fn for_finalized_statement(statement: &StatementIr) -> Self {
        if statement_preserves_caller_flow(statement) {
            Self::from_proof(ProvenNoCallerFlowInvalidation { _private: () })
        } else {
            Self::may_invalidate_caller_flow()
        }
    }

    pub(crate) fn for_finalized_parameters(params: &[FunctionParamIr]) -> Self {
        if parameter_defaults_preserve_caller_flow(params) {
            Self::from_proof(ProvenNoCallerFlowInvalidation { _private: () })
        } else {
            Self::may_invalidate_caller_flow()
        }
    }

    pub(crate) const fn proves_no_flow_invalidation(self) -> bool {
        matches!(self.0, SourceCallFlowState::ProvenNoFlowInvalidation)
    }

    pub(crate) const fn merge_observation(self, observed: Self) -> Self {
        use SourceCallFlowState::{MayInvalidateCallerFlow, ProvenNoFlowInvalidation, Unobserved};

        Self(match (self.0, observed.0) {
            (Unobserved, observed) => observed,
            (current, Unobserved) => current,
            (ProvenNoFlowInvalidation, ProvenNoFlowInvalidation) => ProvenNoFlowInvalidation,
            (ProvenNoFlowInvalidation, MayInvalidateCallerFlow)
            | (MayInvalidateCallerFlow, ProvenNoFlowInvalidation)
            | (MayInvalidateCallerFlow, MayInvalidateCallerFlow) => MayInvalidateCallerFlow,
        })
    }

    pub(crate) const fn combine_caller_flow(self, other: Self) -> Self {
        if self.proves_no_flow_invalidation() && other.proves_no_flow_invalidation() {
            return Self(SourceCallFlowState::ProvenNoFlowInvalidation);
        }
        Self::may_invalidate_caller_flow()
    }

    const fn from_proof(_proof: ProvenNoCallerFlowInvalidation) -> Self {
        Self(SourceCallFlowState::ProvenNoFlowInvalidation)
    }
}

#[must_use = "caller-flow preservation must be consumed by source-call admission"]
pub(crate) struct ProvenNoCallerFlowInvalidation {
    _private: (),
}

pub(crate) fn prove_no_caller_flow_invalidation(
    params: &[FunctionParamIr],
    block: &BlockIr,
) -> Option<ProvenNoCallerFlowInvalidation> {
    (parameter_defaults_preserve_caller_flow(params) && block_preserves_caller_flow(block))
        .then_some(ProvenNoCallerFlowInvalidation { _private: () })
}

fn parameter_defaults_preserve_caller_flow(params: &[FunctionParamIr]) -> bool {
    params
        .iter()
        .filter_map(|param| param.default_init.as_ref())
        .all(expr_preserves_caller_flow)
}

fn block_preserves_caller_flow(block: &BlockIr) -> bool {
    block.statements.iter().all(statement_preserves_caller_flow)
}

fn statement_preserves_caller_flow(statement: &StatementIr) -> bool {
    match statement {
        StatementIr::ModuleImportBinding(_) => true,
        StatementIr::Empty => true,
        StatementIr::AsyncModuleInstantiation => false,
        StatementIr::ModuleUnitOnce {
            module: _module,
            block: _block,
        } => false,
        StatementIr::Lexical {
            mode: _mode,
            name: _name,
            init,
        } => expr_preserves_caller_flow(init),
        StatementIr::AnnexBFunctionCopy {
            source_name: _source_name,
            block_storage_name: _block_storage_name,
            target: _target,
            admission: _admission,
        } => false,
        StatementIr::EmptyStatementCompletion(item) => {
            statement_preserves_caller_flow(item.statement())
        }
        StatementIr::LexicalBlock(statements) => {
            statements.iter().all(statement_preserves_caller_flow)
        }
        StatementIr::SyncDisposableScope {
            execution: _execution,
            resources: _resources,
            body: _body,
        } => false,
        StatementIr::AsyncDisposableScope {
            execution: _execution,
            resources: _resources,
            body: _body,
        } => false,
        StatementIr::ParameterInitialization {
            parameter_index: _parameter_index,
            statements,
        } => statements.iter().all(statement_preserves_caller_flow),
        StatementIr::Var(declarators) => declarators.iter().all(|declarator| {
            declarator
                .init
                .as_ref()
                .is_none_or(expr_preserves_caller_flow)
        }),
        StatementIr::ResumableClassDefinition(_) => false,
        StatementIr::DeclarationEvaluation(expr) | StatementIr::Expression(expr) => {
            expr_preserves_caller_flow(expr)
        }
        StatementIr::GeneratorYield {
            value: _value,
            form: _form,
            suspend_state: _suspend_state,
            resume_state: _resume_state,
            resume_mode: _resume_mode,
        } => false,
        StatementIr::AsyncAwait {
            value: _value,
            suspend_state: _suspend_state,
            resume_state: _resume_state,
            resume_mode: _resume_mode,
        } => false,
        StatementIr::AsyncGeneratorLoop(_)
        | StatementIr::AsyncGeneratorIf(_)
        | StatementIr::AsyncGeneratorWith(_)
        | StatementIr::AsyncGeneratorSwitch(_)
        | StatementIr::AsyncGeneratorArrayDestructuring(_)
        | StatementIr::AsyncGeneratorResourceScope(_)
        | StatementIr::AsyncGeneratorResourceRegistration(_)
        | StatementIr::AsyncGeneratorForOf(_)
        | StatementIr::AsyncGeneratorForIn(_)
        | StatementIr::OrdinaryGeneratorLoop(_)
        | StatementIr::OrdinaryGeneratorIf(_)
        | StatementIr::OrdinaryGeneratorSwitch(_)
        | StatementIr::ArrayDestructuringOperation(_)
        | StatementIr::OrdinaryGeneratorWith(_)
        | StatementIr::OrdinaryGeneratorArrayDestructuring(_)
        | StatementIr::AsyncFunctionArrayDestructuring(_)
        | StatementIr::AsyncFunctionWith(_) => false,
        StatementIr::GeneratorLoop {
            init: _init,
            test: _test,
            update: _update,
            iteration_environment: _iteration_environment,
            before_suspension: _before_suspension,
            suspension_statement: _suspension_statement,
            after_suspension: _after_suspension,
            entry_state: _entry_state,
            resume_state: _resume_state,
            exit_state: _exit_state,
        } => false,
        StatementIr::GeneratorIf {
            condition: _condition,
            then_before_yield: _then_before_yield,
            then_yield_statement: _then_yield_statement,
            then_after_yield: _then_after_yield,
            else_before_yield: _else_before_yield,
            else_yield_statement: _else_yield_statement,
            else_after_yield: _else_after_yield,
            entry_state: _entry_state,
            then_resume_state: _then_resume_state,
            else_resume_state: _else_resume_state,
            exit_state: _exit_state,
        } => false,
        StatementIr::AsyncFunctionWhile(_) => false,
        StatementIr::AsyncFunctionSwitch(_) => false,
        StatementIr::AsyncFunctionIf {
            condition: _condition,
            then_branch: _then_branch,
            else_branch: _else_branch,
            plan: _plan,
        } => false,
        StatementIr::Block(block) => block_preserves_caller_flow(block),
        StatementIr::If {
            condition,
            then_branch,
            else_branch,
        } => {
            expr_preserves_caller_flow(condition)
                && statement_preserves_caller_flow(then_branch)
                && else_branch
                    .as_deref()
                    .is_none_or(statement_preserves_caller_flow)
        }
        StatementIr::While { condition, body } => {
            expr_preserves_caller_flow(condition) && statement_preserves_caller_flow(body)
        }
        StatementIr::DoWhile { body, condition } => {
            statement_preserves_caller_flow(body) && expr_preserves_caller_flow(condition)
        }
        StatementIr::For {
            init,
            test,
            update,
            body,
            lexical_environment: _lexical_environment,
        } => {
            init.as_ref().is_none_or(for_init_preserves_caller_flow)
                && test.as_ref().is_none_or(expr_preserves_caller_flow)
                && update.as_ref().is_none_or(expr_preserves_caller_flow)
                && statement_preserves_caller_flow(body)
        }
        StatementIr::ForOfIterator {
            head: _head,
            iterable: _iterable,
            body: _body,
            lexical_environment: _lexical_environment,
        } => false,
        StatementIr::ForInArray {
            mode: _mode,
            name: _name,
            target: _iterable,
            body: _body,
            lexical_environment: _lexical_environment,
        }
        | StatementIr::ForInString {
            mode: _mode,
            name: _name,
            target: _iterable,
            body: _body,
            lexical_environment: _lexical_environment,
        }
        | StatementIr::ForInObject {
            mode: _mode,
            name: _name,
            target: _iterable,
            body: _body,
            lexical_environment: _lexical_environment,
        } => false,
        StatementIr::AsyncFunctionForOfIterator {
            iterable: _iterable,
            plan: _plan,
        } => false,
        StatementIr::GeneratorForOfIterator {
            iterable: _iterable,
            plan: _plan,
        } => false,
        StatementIr::Switch {
            discriminant,
            lexical_environment: _lexical_environment,
            lexical_declarations,
            cases,
        } => {
            expr_preserves_caller_flow(discriminant)
                && lexical_declarations
                    .iter()
                    .all(statement_preserves_caller_flow)
                && cases.iter().all(|case| {
                    case.condition
                        .as_ref()
                        .is_none_or(expr_preserves_caller_flow)
                        && block_preserves_caller_flow(&case.body)
                })
        }
        StatementIr::Labelled {
            labels: _labels,
            statement,
            async_plan: _,
        } => statement_preserves_caller_flow(statement),
        StatementIr::Debugger => false,
        StatementIr::Throw(expr) | StatementIr::Return(expr) => expr_preserves_caller_flow(expr),
        StatementIr::TryCatch {
            try_block,
            catch_name: _catch_name,
            catch_source_name: _catch_source_name,
            catch_parameter_environment: _catch_parameter_environment,
            catch_block,
            generator_plan: _generator_plan,
            async_plan: _async_plan,
        } => block_preserves_caller_flow(try_block) && block_preserves_caller_flow(catch_block),
        StatementIr::TryFinally {
            try_block,
            finally_block,
            generator_plan: _generator_plan,
            async_plan: _async_plan,
        } => block_preserves_caller_flow(try_block) && block_preserves_caller_flow(finally_block),
        StatementIr::TryCatchFinally {
            try_block,
            catch_name: _catch_name,
            catch_source_name: _catch_source_name,
            catch_parameter_environment: _catch_parameter_environment,
            catch_block,
            finally_block,
            generator_plan: _generator_plan,
            async_plan: _async_plan,
        } => {
            block_preserves_caller_flow(try_block)
                && block_preserves_caller_flow(catch_block)
                && block_preserves_caller_flow(finally_block)
        }
        StatementIr::Break { label: _label } | StatementIr::Continue { label: _label } => true,
    }
}

fn for_init_preserves_caller_flow(init: &ForInitIr) -> bool {
    match init {
        ForInitIr::Lexical {
            mode: _mode,
            name: _name,
            init,
        } => expr_preserves_caller_flow(init),
        ForInitIr::LexicalBlock(initializers) => initializers
            .iter()
            .all(|initializer| expr_preserves_caller_flow(&initializer.init)),
        ForInitIr::Var(declarators) => declarators.iter().all(|declarator| {
            declarator
                .init
                .as_ref()
                .is_none_or(expr_preserves_caller_flow)
        }),
        ForInitIr::Expression(expr) => expr_preserves_caller_flow(expr),
        ForInitIr::Statements(statements) => statements.iter().all(statement_preserves_caller_flow),
        ForInitIr::SyncDisposable(_resources) => false,
        ForInitIr::AsyncDisposable(_resources) => false,
    }
}

fn expr_preserves_caller_flow(expr: &TypedExpr) -> bool {
    match &expr.expr {
        ExprIr::EnvironmentIdentifier(_) | ExprIr::ForeignCall(_) => false,
        ExprIr::Undefined
        | ExprIr::ArrayHole
        | ExprIr::Null
        | ExprIr::This
        | ExprIr::ExecutionGlobalObject
        | ExprIr::Arguments
        | ExprIr::NewTarget => true,
        ExprIr::Boolean(_value) => true,
        ExprIr::Number(_bits) => true,
        ExprIr::BigInt(_value) => true,
        ExprIr::String(_value) => true,
        ExprIr::WellKnownSymbol(_symbol) => true,
        ExprIr::FunctionValue(_function_id) => true,
        ExprIr::ClassFieldNamedFunctionValue {
            function_id: _function_id,
            name: _name,
        } => true,
        ExprIr::Identifier(_name) => true,
        ExprIr::Symbol { description } => description
            .as_deref()
            .is_none_or(expr_preserves_caller_flow),
        ExprIr::RegExpLiteral {
            source: _source,
            flags: _flags,
            static_compilation: _static_compilation,
        } => true,
        ExprIr::DynamicImport {
            specifier: _specifier,
            options: _options,
            phase: _phase,
            referrer: _referrer,
        } => false,
        ExprIr::ImportMeta { module: _module } => false,
        ExprIr::ModuleEntryEvaluation(_) => false,
        ExprIr::ModuleExecutionGraph(_)
        | ExprIr::ModuleBindingRead(_)
        | ExprIr::JsonModuleValue(_)
        | ExprIr::BytesModuleValue(_)
        | ExprIr::ModuleEvaluate(_)
        | ExprIr::DeferredModuleEvaluate(_)
        | ExprIr::ModuleHasAsyncDependencies(_)
        | ExprIr::ModuleDeferredImportEvaluate(_) => false,
        ExprIr::ModuleNamespacePublish { namespace, .. } => expr_preserves_caller_flow(namespace),
        ExprIr::ModuleNamespace { exports, .. } => expr_preserves_caller_flow(exports),
        ExprIr::ObjectLiteral(properties) => {
            properties.iter().all(object_property_preserves_caller_flow)
        }
        ExprIr::ObjectPropertyDefinition(definition) => {
            expr_preserves_caller_flow(definition.target())
                && object_property_preserves_caller_flow(definition.property())
        }
        ExprIr::ArrayLiteral(elements) => elements.iter().all(expr_preserves_caller_flow),
        ExprIr::ArrayAccumulation(accumulation) => {
            array_accumulation_preserves_caller_flow(accumulation)
        }
        ExprIr::GlobalPropertyRead { name: _name }
        | ExprIr::GlobalIdentifierRead { name: _name } => false,
        ExprIr::AssignIdentifier {
            name: _name,
            value: _value,
        } => false,
        ExprIr::GlobalPropertyWrite {
            name: _name,
            value: _value,
            implicit: _implicit,
            strictness: _strictness,
        } => false,
        ExprIr::PropertyWrite {
            target: _target,
            key: _key,
            value: _value,
            strictness: _strictness,
        } => false,
        ExprIr::PropertyRead {
            target: _target,
            key: _key,
        } => false,
        ExprIr::OptionalPropertyChain {
            target: _target,
            chain: _chain,
        } => false,
        ExprIr::OrdinaryPropertyAssignment(_assignment) => false,
        ExprIr::OrdinaryPropertyLogicalAssignment(_assignment) => false,
        ExprIr::OrdinaryPropertyGetCapture(_) | ExprIr::CapturedOrdinaryPropertyWrite(_) => false,
        ExprIr::OrdinaryPropertyNumericUpdate(_update) => false,
        ExprIr::OrdinaryPropertyEagerCompoundAssignment(_assignment) => false,
        ExprIr::UpdateIdentifier {
            name: _name,
            op: _op,
            return_mode: _return_mode,
            value_kind: _value_kind,
        } => false,
        ExprIr::CompoundAssignIdentifier {
            name: _name,
            op: _op,
            value: _value,
        } => false,
        ExprIr::UnaryPlus { expr: _operand } | ExprIr::UnaryMinusNumeric { expr: _operand } => {
            false
        }
        ExprIr::UnaryBitwiseNumeric {
            op: _op,
            expr: _operand,
        } => false,
        ExprIr::Void { expr } | ExprIr::TypeOf { expr } | ExprIr::LogicalNot { expr } => {
            expr_preserves_caller_flow(expr)
        }
        ExprIr::DeleteOptionalPropertyChain(_) => false,
        ExprIr::DeleteValue { expr: _expr } => false,
        ExprIr::DeleteIdentifier { name: _name } => false,
        ExprIr::DeleteGlobalProperty {
            name: _name,
            strictness: _strictness,
        } => false,
        ExprIr::DeleteProperty {
            target: _expr,
            key: _key,
            strictness: _strictness,
        } => false,
        ExprIr::TypeOfUnresolvedIdentifier { name: _name } => false,
        ExprIr::SpecOperation {
            operation,
            operands,
        } => spec_operation_preserves_caller_flow(*operation, operands),
        ExprIr::BinaryNumber {
            op: _op,
            lhs: _lhs,
            rhs: _rhs,
        } => false,
        ExprIr::CoerciveAdd {
            lhs: _lhs,
            rhs: _rhs,
        } => false,
        ExprIr::CoerciveBinaryNumber {
            op: _op,
            lhs: _lhs,
            rhs: _rhs,
        } => false,
        ExprIr::BitwiseNumeric {
            op: _op,
            lhs: _lhs,
            rhs: _rhs,
        } => false,
        ExprIr::StringFromCharCode { code: _code } => false,
        ExprIr::StringConcat {
            lhs: _lhs,
            rhs: _rhs,
        } => false,
        ExprIr::CompareNumber {
            op: _op,
            lhs: _lhs,
            rhs: _rhs,
        } => false,
        ExprIr::CompareValue {
            op: _op,
            lhs: _lhs,
            rhs: _rhs,
        } => false,
        ExprIr::LooseEquality {
            op: _op,
            lhs: _lhs,
            rhs: _rhs,
        } => false,
        ExprIr::TemplateObject(_template) => false,
        ExprIr::StrictEquality { op: _op, lhs, rhs } => {
            expr_preserves_caller_flow(lhs) && expr_preserves_caller_flow(rhs)
        }
        ExprIr::LogicalShortCircuit { op: _op, lhs, rhs } => {
            expr_preserves_caller_flow(lhs) && expr_preserves_caller_flow(rhs)
        }
        ExprIr::Comma { lhs, rhs } => {
            expr_preserves_caller_flow(lhs) && expr_preserves_caller_flow(rhs)
        }
        ExprIr::Conditional {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_preserves_caller_flow(condition)
                && expr_preserves_caller_flow(then_expr)
                && expr_preserves_caller_flow(else_expr)
        }
        ExprIr::MaterializeBinding {
            name: _name,
            value,
            body,
        } => expr_preserves_caller_flow(value) && expr_preserves_caller_flow(body),
        ExprIr::ArrayDestructure {
            value: _value,
            pattern: _pattern,
            evaluation: _evaluation,
        } => false,
        ExprIr::ObjectDestructure {
            value: _value,
            pattern: _pattern,
        } => false,
        ExprIr::ObjectDestructuringOperation(_) => false,
        ExprIr::CallNamed {
            name: _name,
            args: _args,
        } => false,
        ExprIr::CallIndirect {
            direct_eval: _,
            callee: _callee,
            this_arg: _this_arg,
            args: _args,
            static_regexp_compilation: _static_regexp_compilation,
        } => false,
        ExprIr::Construct {
            callee: _callee,
            args: _args,
            static_regexp_compilation: _static_regexp_compilation,
        } => false,
        ExprIr::CallMethod {
            receiver: _callee,
            key: _key,
            args: _args,
        } => false,
        ExprIr::SpreadArgument(_)
        | ExprIr::CaptureArgumentList(_)
        | ExprIr::CaptureOptionalCallReference(_)
        | ExprIr::CapturedArgumentList(_) => false,
        ExprIr::AssertSameValue {
            actual: _actual,
            expected: _expected,
            message: _message,
        } => false,
        ExprIr::RuntimeThrow {
            name: _name,
            message: _message,
        } => true,
        ExprIr::ClassDefinition(_class) => false,
        ExprIr::SuperConstruct { args: _ }
        | ExprIr::PreparedSuperConstruct(_)
        | ExprIr::SuperConstructor
        | ExprIr::SuperNewTarget => false,
        ExprIr::SuperPropertyRead {
            key: _key,
            receiver: _receiver,
        } => false,
        ExprIr::SuperPropertyWrite {
            key: _key,
            receiver: _receiver,
            value: _value,
            strictness: _strictness,
        } => false,
        ExprIr::SuperPropertyMutation(_mutation) => false,
        ExprIr::PrivateRead {
            target: _target,
            private_name_id: _private_name_id,
        } => false,
        ExprIr::PrivateWrite {
            target: _target,
            private_name_id: _private_name_id,
            value: _value,
        } => false,
        ExprIr::PrivateIn {
            private_name_id: _private_name_id,
            rhs,
        } => expr_preserves_caller_flow(rhs),
        ExprIr::InstanceOf {
            lhs: _lhs,
            rhs: _rhs,
        }
        | ExprIr::In {
            lhs: _lhs,
            rhs: _rhs,
        } => false,
    }
}

fn object_property_preserves_caller_flow(property: &ObjectPropertyIr) -> bool {
    match property {
        ObjectPropertyIr::PrototypeSetter { value } => expr_preserves_caller_flow(value),
        ObjectPropertyIr::Data {
            key: _key,
            value,
            is_shorthand: _is_shorthand,
        } => expr_preserves_caller_flow(value),
        ObjectPropertyIr::NonEnumerableData { key: _key, value } => {
            expr_preserves_caller_flow(value)
        }
        ObjectPropertyIr::Spread { source: _source } => false,
        ObjectPropertyIr::ComputedData {
            key: _key,
            value: _value,
            name_inference: _,
        } => false,
        ObjectPropertyIr::ComputedMethod {
            key: _key,
            function: _function,
        } => false,
        ObjectPropertyIr::ComputedGetter {
            key: _key,
            function: _function,
        } => false,
        ObjectPropertyIr::ComputedSetter {
            key: _key,
            function: _function,
        } => false,
        ObjectPropertyIr::Method {
            key: _key,
            function: _function,
        }
        | ObjectPropertyIr::Getter {
            key: _key,
            function: _function,
        }
        | ObjectPropertyIr::Setter {
            key: _key,
            function: _function,
        } => true,
    }
}

fn array_accumulation_preserves_caller_flow(accumulation: &ArrayAccumulationIr) -> bool {
    let target_preserves_caller_flow = match accumulation.target() {
        ArrayAccumulationTargetIr::Fresh => true,
        ArrayAccumulationTargetIr::SuspensionOwned(_slots) => true,
    };
    target_preserves_caller_flow
        && accumulation.elements().iter().all(|element| match element {
            ArrayAccumulationElementIr::Elision => true,
            ArrayAccumulationElementIr::Value(value) => expr_preserves_caller_flow(value),
            ArrayAccumulationElementIr::Spread(_spread) => false,
        })
}

fn spec_operation_preserves_caller_flow(
    operation: SpecOperationIr,
    operands: &[TypedExpr],
) -> bool {
    if !operands.iter().all(expr_preserves_caller_flow) {
        return false;
    }

    match operation {
        SpecOperationIr::IsCallable
        | SpecOperationIr::IsConstructor
        | SpecOperationIr::IsPropertyKey
        | SpecOperationIr::ToBoolean
        | SpecOperationIr::ToObject
        | SpecOperationIr::SameValue
        | SpecOperationIr::SameValueZero
        | SpecOperationIr::StrictEqualityComparison => true,
        SpecOperationIr::ToPrimitive(
            ToPrimitiveHint::Default | ToPrimitiveHint::Number | ToPrimitiveHint::String,
        )
        | SpecOperationIr::ToNumeric
        | SpecOperationIr::ToNumber
        | SpecOperationIr::ToBigInt
        | SpecOperationIr::ToString
        | SpecOperationIr::ToPropertyKey
        | SpecOperationIr::ToIntegerOrInfinity
        | SpecOperationIr::ToLength
        | SpecOperationIr::ToIndex
        | SpecOperationIr::IsLooselyEqual => operands
            .iter()
            .all(|operand| operand.possible_kinds.is_subset_of(KindSet::PRIMITIVE_ONLY)),
        SpecOperationIr::Get
        | SpecOperationIr::GetV
        | SpecOperationIr::Set
        | SpecOperationIr::HasProperty
        | SpecOperationIr::WithEnvironmentHasBinding
        | SpecOperationIr::WithEnvironmentGetBindingValueSloppy
        | SpecOperationIr::WithEnvironmentGetBindingValueStrict
        | SpecOperationIr::HasOwnProperty
        | SpecOperationIr::DeletePropertyOrThrow
        | SpecOperationIr::CreateDataPropertyOrThrow
        | SpecOperationIr::CopyDataProperties
        | SpecOperationIr::GetMethod
        | SpecOperationIr::Call
        | SpecOperationIr::Construct => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ValueInfo, ValueKind};

    fn body_with(statement: StatementIr) -> BlockIr {
        BlockIr {
            statements: vec![statement],
            result_kind: ValueKind::Undefined,
            lexical_environment: None,
        }
    }

    #[test]
    fn a_literal_return_proves_caller_flow_preservation() {
        let body = body_with(StatementIr::Return(TypedExpr::undefined()));

        assert!(
            SourceCallFlowEffects::for_finalized_invocation(&[], &body)
                .proves_no_flow_invalidation()
        );
    }

    #[test]
    fn a_call_cannot_prove_caller_flow_preservation() {
        let call = TypedExpr::from_info(
            ValueInfo::new(ValueKind::Dynamic),
            ExprIr::CallNamed {
                name: "callee".to_string(),
                args: Vec::new(),
            },
        );
        let body = body_with(StatementIr::Return(call));

        assert!(
            !SourceCallFlowEffects::for_finalized_invocation(&[], &body)
                .proves_no_flow_invalidation()
        );
    }

    #[test]
    fn a_retained_literal_property_cannot_hide_operand_calls() {
        let target = || {
            TypedExpr::from_info(
                ValueInfo::new(ValueKind::Object),
                ExprIr::Identifier("retained_literal".to_string()),
            )
        };
        let call = || {
            TypedExpr::from_info(
                ValueInfo::new(ValueKind::Object),
                ExprIr::CallNamed {
                    name: "effect".to_string(),
                    args: Vec::new(),
                },
            )
        };
        for (target, value, preserves_flow) in [
            (target(), TypedExpr::undefined(), true),
            (call(), TypedExpr::undefined(), false),
            (target(), call(), false),
        ] {
            let definition = crate::ObjectPropertyDefinitionIr::new(
                target,
                ObjectPropertyIr::Data {
                    key: "value".to_string(),
                    value,
                    is_shorthand: false,
                },
            )
            .expect("the retained target has the exact Object kind");
            let expr = TypedExpr::from_info(
                ValueInfo::new(ValueKind::Object),
                ExprIr::ObjectPropertyDefinition(Box::new(definition)),
            );
            let body = body_with(StatementIr::Return(expr));
            assert_eq!(
                SourceCallFlowEffects::for_finalized_invocation(&[], &body)
                    .proves_no_flow_invalidation(),
                preserves_flow,
            );
        }
    }

    #[test]
    fn a_primitive_conversion_proves_caller_flow_preservation() {
        let conversion = TypedExpr::spec_to_string(TypedExpr::undefined());
        let body = body_with(StatementIr::Return(conversion));

        assert!(
            SourceCallFlowEffects::for_finalized_invocation(&[], &body)
                .proves_no_flow_invalidation()
        );
    }

    #[test]
    fn an_object_conversion_cannot_prove_caller_flow_preservation() {
        let object = TypedExpr::from_info(
            ValueInfo::new(ValueKind::Object),
            ExprIr::Identifier("object".to_string()),
        );
        let conversion = TypedExpr::spec_to_string(object);
        let body = body_with(StatementIr::Return(conversion));

        assert!(
            !SourceCallFlowEffects::for_finalized_invocation(&[], &body)
                .proves_no_flow_invalidation()
        );
    }

    #[test]
    fn a_literal_parameter_default_proves_caller_flow_preservation() {
        let params = vec![FunctionParamIr {
            name: "value".to_string(),
            kind: ValueKind::Dynamic,
            default_init: Some(TypedExpr::undefined()),
            is_rest: false,
        }];
        let body = body_with(StatementIr::Return(TypedExpr::undefined()));

        assert!(
            SourceCallFlowEffects::for_finalized_invocation(&params, &body)
                .proves_no_flow_invalidation()
        );
    }

    #[test]
    fn a_call_in_a_parameter_default_cannot_prove_caller_flow_preservation() {
        let call = TypedExpr::from_info(
            ValueInfo::new(ValueKind::Dynamic),
            ExprIr::CallNamed {
                name: "callee".to_string(),
                args: Vec::new(),
            },
        );
        let params = vec![FunctionParamIr {
            name: "value".to_string(),
            kind: ValueKind::Dynamic,
            default_init: Some(call),
            is_rest: false,
        }];
        let body = body_with(StatementIr::Return(TypedExpr::undefined()));

        assert!(
            !SourceCallFlowEffects::for_finalized_invocation(&params, &body)
                .proves_no_flow_invalidation()
        );
    }
}
