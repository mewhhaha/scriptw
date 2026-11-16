use super::*;
use boa_ast::operations::{var_scoped_declarations, VarScopedDeclaration};
use scriptw_front::ParseDiagnosticKind;

/// Lower a native global-source request through the same independently compiled
/// Script machinery as prepared sources. The host must invoke the returned
/// Script entry; the containing program has no executable entry body.
pub(crate) fn lower_runtime_global_script_program(
    parsed: &ParsedScript,
    kind: crate::RuntimeGlobalScriptKind,
    host_surface_policy: HostSurfacePolicy,
) -> (ProgramIr, StaticScriptId) {
    let kind = kind.prepared_kind();
    let mut allocations = AnalysisAllocationState::default();
    lower_runtime_prepared_script_program(parsed, kind, host_surface_policy, &mut allocations)
}

pub(crate) fn lower_runtime_direct_eval_program(
    parsed: &ParsedScript,
    context: DirectEvalContextIr,
    private_environment_floor: u32,
    host_surface_policy: HostSurfacePolicy,
) -> (ProgramIr, StaticScriptId) {
    let mut allocations = AnalysisAllocationState::default()
        .with_private_environment_floor(private_environment_floor);
    allocations.script_import_origin = context.script_import_origin();
    lower_runtime_prepared_script_program(
        parsed,
        PreparedScriptKind::DirectEval(context),
        host_surface_policy,
        &mut allocations,
    )
}

fn lower_runtime_prepared_script_program(
    parsed: &ParsedScript,
    kind: PreparedScriptKind,
    host_surface_policy: HostSurfacePolicy,
    allocations: &mut AnalysisAllocationState,
) -> (ProgramIr, StaticScriptId) {
    let id = allocations.allocate_static_script_id();
    let mut program = lower_script_program_with_allocations(
        parsed,
        ParseGoal::Script,
        parsed.source_text.len(),
        vec![LoweringStage::ParsedSource],
        None,
        &modules::LinkedScriptDefinitions::default(),
        host_surface_policy,
        allocations,
        ScriptInstantiation::Prepared(kind.clone()),
    );
    if !program.diagnostics.is_empty() {
        return (program, id);
    }
    let script = program
        .script
        .as_mut()
        .expect("independent source lowers as prepared Script");
    let unit = PreparedScriptUnit {
        template_source: script.template_source.take(),
        eval_environment: script.eval_environment.take(),
        id,
        kind: kind.clone(),
        strict: script.strict,
        body: std::mem::replace(
            &mut script.body,
            BlockIr {
                statements: Vec::new(),
                result_kind: ValueKind::Undefined,
                lexical_environment: None,
            },
        ),
        owned_env_bindings: std::mem::take(&mut script.owned_env_bindings),
        global_bindings: std::mem::take(&mut script.global_bindings),
        declarations: std::mem::take(&mut script.runtime_declarations),
        function_ids: script
            .functions
            .iter()
            .map(|function| function.id.clone())
            .collect(),
    };
    script.strict = false;
    // Preserve every nested prepared Script/Function and all function bodies in
    // this same ProgramIr. IDs were allocated from the same compilation owner.
    script.prepared_scripts.insert(
        0,
        PreparedScript {
            admission: PreparedScriptAdmission::ResolvedIntrinsic,
            kind,
            source: parsed.source_text.clone(),
            outcome: PreparedScriptOutcome::Executable(unit),
        },
    );
    record_prepared_compilation_stage(&mut program);
    (program, id)
}

pub(super) fn compile_dynamic_script_sources(
    program: &mut ProgramIr,
    sources: Vec<DynamicScriptSource>,
    host_surface_policy: HostSurfacePolicy,
    allocations: &mut AnalysisAllocationState,
) {
    for source in sources {
        let parsed_source = allocations
            .prepared_sources
            .parse_script(&source.kind, &source.source);
        let parsed = match parsed_source {
            Ok(parsed) => parsed,
            Err(error) => {
                match error.diagnostic().kind() {
                    ParseDiagnosticKind::MalformedJavaScript => program
                        .script
                        .as_mut()
                        .expect("prepared source has a lowered owner")
                        .prepared_scripts
                        .push(PreparedScript {
                            admission: source.admission,
                            kind: source.kind.clone(),
                            source: source.source,
                            outcome: PreparedScriptOutcome::DeferredSyntaxError {
                                message: error.message().to_string(),
                            },
                        }),
                    ParseDiagnosticKind::UnsupportedParserFeature => {
                        program
                            .diagnostics
                            .push(IrDiagnostic::unsupported_parser_feature(error.message()));
                    }
                }
                continue;
            }
        };
        let id = allocations.allocate_static_script_id();
        let caller_origin = allocations.script_import_origin;
        allocations.script_import_origin = match &source.kind {
            PreparedScriptKind::DirectEval(context) => context.script_import_origin(),
            PreparedScriptKind::IndirectEval
            | PreparedScriptKind::RealmScript
            | PreparedScriptKind::ShadowRealmEvaluate => None,
        };
        let mut compiled = lower_script_program_with_allocations(
            &parsed,
            ParseGoal::Script,
            parsed.source_text.len(),
            vec![LoweringStage::ParsedSource],
            None,
            &modules::LinkedScriptDefinitions::default(),
            host_surface_policy,
            allocations,
            ScriptInstantiation::Prepared(source.kind.clone()),
        );
        allocations.script_import_origin = caller_origin;
        if !compiled.diagnostics.is_empty() {
            if compiled
                .diagnostics
                .iter()
                .all(|diagnostic| diagnostic.error_type() == Some(NativeErrorKind::SyntaxError))
            {
                program
                    .script
                    .as_mut()
                    .expect("prepared source has a lowered owner")
                    .prepared_scripts
                    .push(PreparedScript {
                        admission: source.admission,
                        kind: source.kind.clone(),
                        source: source.source,
                        outcome: PreparedScriptOutcome::DeferredSyntaxError {
                            message: compiled
                                .diagnostics
                                .iter()
                                .map(|diagnostic| diagnostic.message.as_str())
                                .collect::<Vec<_>>()
                                .join("; "),
                        },
                    });
            } else {
                if source.admission == PreparedScriptAdmission::RuntimeCandidate {
                    compiled
                        .diagnostics
                        .retain(|diagnostic| !diagnostic.can_omit_for_runtime_source_candidate());
                }
                program.diagnostics.append(&mut compiled.diagnostics);
            }
            continue;
        }
        let mut compiled = compiled
            .script
            .expect("independent source lowered as Script");
        let unit = PreparedScriptUnit {
            template_source: compiled.template_source,
            eval_environment: compiled.eval_environment.take(),
            id,
            kind: source.kind.clone(),
            strict: compiled.strict,
            body: std::mem::replace(
                &mut compiled.body,
                BlockIr {
                    statements: Vec::new(),
                    result_kind: ValueKind::Undefined,
                    lexical_environment: None,
                },
            ),
            owned_env_bindings: std::mem::take(&mut compiled.owned_env_bindings),
            global_bindings: std::mem::take(&mut compiled.global_bindings),
            declarations: std::mem::take(&mut compiled.runtime_declarations),
            function_ids: compiled
                .functions
                .iter()
                .map(|function| function.id.clone())
                .collect(),
        };
        let script = program
            .script
            .as_mut()
            .expect("prepared source has a lowered owner");
        append_prepared_unit(script, compiled);
        script.prepared_scripts.push(PreparedScript {
            admission: source.admission,
            kind: source.kind.clone(),
            source: source.source,
            outcome: PreparedScriptOutcome::Executable(unit),
        });
    }
    record_prepared_compilation_stage(program);
}

pub(super) fn compile_module_prelude(
    program: &mut ProgramIr,
    parsed: &ParsedScript,
    host_surface_policy: HostSurfacePolicy,
    allocations: &mut AnalysisAllocationState,
) {
    let id = allocations.allocate_static_script_id();
    let kind = PreparedScriptKind::RealmScript;
    let caller_origin = allocations.script_import_origin.take();
    let mut compiled = lower_script_program_with_allocations(
        parsed,
        ParseGoal::Script,
        parsed.source_text.len(),
        vec![LoweringStage::ParsedSource],
        None,
        &modules::LinkedScriptDefinitions::default(),
        host_surface_policy,
        allocations,
        ScriptInstantiation::GlobalScriptAfterPrelude,
    );
    allocations.script_import_origin = caller_origin;
    if !compiled.diagnostics.is_empty() {
        program.diagnostics.extend(
            compiled
                .diagnostics
                .drain(..)
                .map(IrDiagnostic::in_global_prelude),
        );
        record_prepared_compilation_stage(program);
        return;
    }
    let mut compiled = compiled.script.expect("Module prelude lowers as Script");
    let unit = PreparedScriptUnit {
        template_source: compiled.template_source,
        eval_environment: compiled.eval_environment.take(),
        id,
        kind,
        strict: compiled.strict,
        body: std::mem::replace(
            &mut compiled.body,
            BlockIr {
                statements: Vec::new(),
                result_kind: ValueKind::Undefined,
                lexical_environment: None,
            },
        ),
        owned_env_bindings: std::mem::take(&mut compiled.owned_env_bindings),
        global_bindings: std::mem::take(&mut compiled.global_bindings),
        declarations: std::mem::take(&mut compiled.runtime_declarations),
        function_ids: compiled
            .functions
            .iter()
            .map(|function| function.id.clone())
            .collect(),
    };
    let script = program
        .script
        .as_mut()
        .expect("Module graph has a lowered owner");
    append_prepared_unit(script, compiled);
    script.module_prelude = Some(unit);
    record_prepared_compilation_stage(program);
}

impl ScriptLowerer<'_> {
    pub(super) fn runtime_global_declarations(
        &self,
        script: &Script,
    ) -> RuntimeGlobalDeclarationPlan {
        let lexical_names_in_source_order = lexically_declared_names(script)
            .into_iter()
            .map(|name| self.interner.resolve_expect(name).to_string())
            .collect();
        let mut function_names = BTreeSet::new();
        let functions_in_reverse_order = self
            .analysis
            .script_root_functions
            .iter()
            .rev()
            .filter(|function| {
                !self
                    .analysis
                    .module_execution
                    .is_private_dispatcher_function(&function.id)
            })
            .filter(|function| function_names.insert(function.name.clone()))
            .map(|function| GlobalFunctionDeclarationIr {
                name: function.name.clone(),
                function_id: function.id.clone(),
            })
            .collect();
        let mut seen_vars = BTreeSet::new();
        let var_names = var_scoped_declarations(script)
            .into_iter()
            .filter(|declaration| {
                matches!(declaration, VarScopedDeclaration::VariableDeclaration(_))
            })
            .flat_map(|declaration| declaration.bound_names())
            .map(|name| self.interner.resolve_expect(name).to_string())
            .filter(|name| seen_vars.insert(name.clone()))
            .collect();
        let mut seen_annex_b = BTreeSet::new();
        let annex_b_candidates = annex_b_function_declarations(script)
            .into_iter()
            .filter_map(|function| {
                self.analysis
                    .annex_b_function_plans
                    .get(&function_declaration_key(function))
            })
            .filter(|plan| plan.owner_id == SCRIPT_OWNER_ID && plan.copy_to_variable_environment)
            .map(|plan| plan.source_name.clone())
            .filter(|name| seen_annex_b.insert(name.clone()))
            .filter(|_| {
                matches!(
                    self.analysis.script_instantiation,
                    ScriptInstantiation::Prepared(_)
                        | ScriptInstantiation::GlobalScriptAfterPrelude
                )
            })
            .map(|name| {
                let storage_name = annex_b_admission_binding_name(&name);
                let slot =
                    self.analysis.owner_plans[SCRIPT_OWNER_ID].owned_env_slots[&storage_name];
                AnnexBGlobalDeclarationIr {
                    name,
                    admission: OwnedEnvBindingIr {
                        mutability: crate::EnvironmentBindingMutabilityIr::Mutable,
                        name: storage_name,
                        slot,
                    },
                }
            })
            .collect();
        RuntimeGlobalDeclarationPlan {
            lexical_names_in_source_order,
            functions_in_reverse_order,
            var_names,
            annex_b_candidates,
        }
    }

    pub(super) fn script_variables_are_global(&self) -> bool {
        self.analysis
            .script_instantiation
            .has_global_variable_environment(self.analysis.owner_plans[SCRIPT_OWNER_ID].strict)
    }

    pub(super) fn root_functions_need_body_initialization(&self) -> bool {
        if self.borrows_direct_eval_variable_environment() {
            return false;
        }
        self.current_owner_id != SCRIPT_OWNER_ID || !self.script_variables_are_global()
    }

    /// Which Environment Record holds the function declarations of the
    /// statement list being lowered, when the list itself instantiates them.
    /// `None` leaves them to GlobalDeclarationInstantiation or to the
    /// EvalDeclarationInstantiation of a sloppy direct eval.
    pub(super) fn root_function_scope(&self) -> Option<RootFunctionScope> {
        if !self.root_functions_need_body_initialization() {
            return None;
        }
        let module_activation = self
            .analysis
            .function_plans
            .get(&self.current_owner_id)
            .is_some_and(|owner| match owner.protocol {
                FunctionProtocolIr::ModuleActivation
                | FunctionProtocolIr::AsyncModuleActivation => true,
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
            });
        Some(if module_activation {
            RootFunctionScope::LexicalEnvironment
        } else {
            RootFunctionScope::VariableEnvironment
        })
    }
}

/// The Environment Record that receives a function declaration standing at
/// the root of a statement list whose own code instantiates it.
///
/// The two cases are different declarations, not two spellings of one. Only
/// the second is a lexical binding, and lowering the first as one asked the
/// backend to make `var` or parameter storage uninitialized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RootFunctionScope {
    /// A function body, or eval code with a variable environment of its own.
    /// The declaration is var-scoped: its name is one of `varNames`, whose
    /// binding FunctionDeclarationInstantiation (ECMA-262 10.2.11) creates
    /// initialized in step 27 or 28, or finds among the parameters, and step
    /// 36 sets with SetMutableBinding (EvalDeclarationInstantiation, 19.2.1.3,
    /// for eval code). The binding is never uninitialized.
    VariableEnvironment,
    /// A module's top level, where the declaration is a lexically scoped
    /// binding of the module Environment Record that InitializeEnvironment
    /// (16.2.1.7.3.1 step 24) creates and initializes together, and the
    /// private cell of a linker-generated dispatcher. Neither a `var` nor a
    /// parameter can share the name.
    LexicalEnvironment,
}

impl RootFunctionScope {
    /// The statement that gives `name` its function object at the head of
    /// the list. A var-scoped declaration writes the binding that `var`
    /// instantiation or a parameter owns, so it is a declarator of that
    /// binding and cannot be spelled as a lexical declaration.
    pub(super) fn instantiate(self, name: String, function: TypedExpr) -> StatementIr {
        match self {
            Self::VariableEnvironment => StatementIr::Var(vec![VarDeclaratorIr {
                name,
                init: Some(function),
            }]),
            Self::LexicalEnvironment => StatementIr::Lexical {
                mode: BindingMode::Let,
                name,
                init: function,
            },
        }
    }
}

fn annex_b_admission_binding_name(name: &str) -> String {
    format!("$annex.b.admitted:{name}")
}

impl Analysis<'_> {
    pub(super) fn prepare_runtime_script_slots(&mut self, script: &Script, interner: &Interner) {
        if !matches!(
            self.script_instantiation,
            ScriptInstantiation::Prepared(_) | ScriptInstantiation::GlobalScriptAfterPrelude
        ) {
            return;
        }
        let mut names = lexically_declared_names(script)
            .into_iter()
            .map(|name| interner.resolve_expect(name).to_string())
            .collect::<Vec<_>>();
        names.extend(
            self.annex_b_function_plans
                .values()
                .filter(|plan| {
                    plan.owner_id == SCRIPT_OWNER_ID && plan.copy_to_variable_environment
                })
                .map(|plan| annex_b_admission_binding_name(&plan.source_name)),
        );
        let owner = self
            .owner_plans
            .get_mut(SCRIPT_OWNER_ID)
            .expect("Script activation exists");
        let environment = self
            .environment_plans
            .get_mut(&owner.activation_environment_id)
            .expect("Script activation environment exists");
        for name in names {
            if owner.owned_env_slots.contains_key(&name) {
                continue;
            }
            let slot = owner.owned_env_slots.len() as u32;
            owner.owned_env_slots.insert(name.clone(), slot);
            // Annex B admission cells are compiler storage, rather than a
            // source lexical declaration. Publish their policy explicitly.
            environment
                .binding_modes
                .entry(name.clone())
                .or_insert(BindingMode::Let);
            environment.owned_env_slots.insert(name, slot);
        }
    }
}

impl ScriptLowerer<'_> {
    pub(super) fn prepared_annex_b_admission(&self, name: &str) -> Option<OwnedEnvBindingIr> {
        if self.current_owner_id != SCRIPT_OWNER_ID
            || !matches!(
                self.analysis.script_instantiation,
                ScriptInstantiation::Prepared(_) | ScriptInstantiation::GlobalScriptAfterPrelude
            )
        {
            return None;
        }
        let name = annex_b_admission_binding_name(name);
        let slot = self.analysis.owner_plans[SCRIPT_OWNER_ID].owned_env_slots[&name];
        Some(OwnedEnvBindingIr {
            mutability: crate::EnvironmentBindingMutabilityIr::Mutable,
            name,
            slot,
        })
    }
}
