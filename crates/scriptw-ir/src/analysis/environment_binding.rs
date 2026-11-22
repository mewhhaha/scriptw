//! BindingCell policies belong to the analyzed source environment, including
//! cells retained in an activation whose names are not visible to direct eval.

use super::*;

impl Analysis<'_> {
    pub(crate) fn owned_environment_binding(
        &self,
        environment_id: EnvironmentId,
        name: &str,
        slot: u32,
        interner: &Interner,
    ) -> OwnedEnvBindingIr {
        let environment = &self.environment_plans[&environment_id];
        // An uncaptured block/head binding may be retained in the invocation
        // environment. Its write policy still comes from its source record.
        let source = if environment.binding_modes.contains_key(name) {
            environment
        } else {
            self.physical_binding_environments
                .get(name)
                .into_iter()
                .flatten()
                .map(|id| &self.environment_plans[id])
                .find(|source| {
                    source.owner_id == environment.owner_id
                        && source.binding_modes.contains_key(name)
                })
                .unwrap_or_else(|| {
                    panic!("owned source binding `{name}` must have an analyzed write policy")
                })
        };
        let mode = source.binding_modes[name];
        let named_self = source.kind == EnvironmentKind::NamedFunctionExpression
            || (matches!(
                environment.kind,
                EnvironmentKind::Activation | EnvironmentKind::FunctionParameters
            ) && mode == BindingMode::Const
                && self
                    .function_plans
                    .get(&environment.owner_id)
                    .is_some_and(|function| {
                        function.activation_self_binding_name() == Some(name)
                            && (self.owner_plans[&environment.owner_id]
                                .body_environment_id
                                .is_some()
                                || !lexically_declared_names(function.body).into_iter().any(
                                    |bound| interner.resolve_expect(bound).to_string() == name,
                                ))
                    }));
        let mutability = if mode == BindingMode::Const {
            EnvironmentBindingMutabilityIr::Immutable {
                strict: !named_self,
            }
        } else {
            EnvironmentBindingMutabilityIr::Mutable
        };
        OwnedEnvBindingIr {
            name: name.to_string(),
            slot,
            mutability,
        }
    }

    /// The TDZ policy of a cell another body captures, decided from the
    /// binding's physical environment (`environment_id`, the capture's own).
    ///
    /// Only bindings that declaration instantiation of their owner initializes
    /// before any closure able to name them exists are `BeforeAnyReference`:
    ///
    /// - a `var` of a function or module owner (FunctionDeclarationInstantiation
    ///   steps 27-28, module InitializeEnvironment). Its cell also holds
    ///   `undefined` from allocation, the value instantiation gives it;
    /// - a top-level function declaration of a function owner (steps 35-36:
    ///   lowering emits the root function initializations before any body
    ///   statement), and a binding the separate body record initializes when it
    ///   is created (`begin_function_body_environment` initializes exactly the
    ///   `var` and function names);
    /// - a parameter of a list without expressions. Such a list keeps the plain
    ///   `Activation` kind (`prepare_function_body_environment`), and no closure
    ///   exists before its last parameter is bound.
    ///
    /// Everything else keeps the check: lexical and class bindings, parameters
    /// of a list with expressions (`FunctionParameters`), imports, Script and
    /// eval owners, Annex B function copies, `arguments` and module function
    /// declarations (a cyclic import can run before the module body).
    pub(crate) fn captured_binding_initialization(
        &self,
        environment_id: EnvironmentId,
        name: &str,
    ) -> CapturedBindingInitializationIr {
        let checked = CapturedBindingInitializationIr::Checked;
        let Some(environment) = self.environment_plans.get(&environment_id) else {
            return checked;
        };
        if environment.owner_id == SCRIPT_OWNER_ID
            || matches!(name, "arguments" | LEXICAL_ARGUMENTS_NAME)
        {
            return checked;
        }
        let (Some(owner), Some(function), Some(mode)) = (
            self.owner_plans.get(&environment.owner_id),
            self.function_plans.get(&environment.owner_id),
            environment.binding_modes.get(name).copied(),
        ) else {
            return checked;
        };
        let annex_b_copy = self
            .annex_b_function_plans
            .values()
            .any(|plan| plan.owner_id == environment.owner_id && plan.source_name == name);
        if annex_b_copy || mode == BindingMode::Const {
            return checked;
        }
        let module = match function.protocol {
            FunctionProtocolIr::ModuleActivation | FunctionProtocolIr::AsyncModuleActivation => {
                true
            }
            FunctionProtocolIr::OrdinaryCallOnly
            | FunctionProtocolIr::OrdinaryCallAndConstruct
            | FunctionProtocolIr::Arrow
            | FunctionProtocolIr::Generator
            | FunctionProtocolIr::Async
            | FunctionProtocolIr::AsyncArrow
            | FunctionProtocolIr::AsyncGenerator
            | FunctionProtocolIr::ObjectMethod(_)
            | FunctionProtocolIr::ObjectGetter
            | FunctionProtocolIr::ObjectSetter
            | FunctionProtocolIr::ClassConstructor
            | FunctionProtocolIr::ClassMethod(_)
            | FunctionProtocolIr::ClassGetter
            | FunctionProtocolIr::ClassSetter => false,
        };
        let function_declaration = owner.function_bindings.contains_key(name);
        let instantiated = match environment.kind {
            EnvironmentKind::Activation => {
                environment_id == owner.activation_environment_id
                    && (mode == BindingMode::Var
                        || (!module
                            && (function_declaration || owner.parameter_names.contains(name))))
            }
            EnvironmentKind::FunctionBody => {
                !module
                    && Some(environment_id) == owner.body_environment_id
                    && (mode == BindingMode::Var || function_declaration)
            }
            EnvironmentKind::FunctionParameters
            | EnvironmentKind::ParameterEvalVariable
            | EnvironmentKind::Block
            | EnvironmentKind::WithObject
            | EnvironmentKind::ClassName
            | EnvironmentKind::NamedFunctionExpression
            | EnvironmentKind::SwitchCaseBlock
            | EnvironmentKind::CatchParameter
            | EnvironmentKind::SimpleCatchParameter
            | EnvironmentKind::ForLexicalHead
            | EnvironmentKind::ForInOfTdzHead
            | EnvironmentKind::ForInOfIteration => false,
        };
        if instantiated {
            CapturedBindingInitializationIr::BeforeAnyReference
        } else {
            checked
        }
    }

    pub(crate) fn owner_environment_binding(
        &self,
        owner_id: &str,
        name: &str,
        slot: u32,
        interner: &Interner,
    ) -> OwnedEnvBindingIr {
        self.owned_environment_binding(
            self.owner_plans[owner_id].activation_environment_id,
            name,
            slot,
            interner,
        )
    }
}
